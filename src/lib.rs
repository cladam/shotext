use crate::config::Config;
use crate::error::AppError;
pub use cli::{Cli, Commands};
use colored::Colorize;
use std::env;
use std::path::PathBuf;

pub mod cli;
pub mod colours;
pub mod config;
pub mod db;
pub mod error;
pub mod experimental_ui;
pub mod ingest;
pub mod ocr;
pub mod search;
pub mod viewer;
pub mod watch;

/// Initialise or open the Tantivy search index located at the specified path.
pub fn initialise_search_index(config: &Config) -> Result<tantivy::Index, AppError> {
    let search_index_path = match env::var("SHOTEXT_DB_PATH") {
        Ok(path_str) => PathBuf::from(path_str).join("search_index"),
        Err(_) => config
            .paths
            .database
            .parent()
            .map(|p| p.join("search_index"))
            .unwrap_or_else(|| {
                dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("shotext")
                    .join("search_index")
            }),
    };

    let index = search::open_index(&search_index_path)?;
    Ok(index)
}

// The main logic function, which takes the parsed CLI commands
pub fn run(cli: Cli, config: Config) -> Result<(), AppError> {
    // Open the database
    let db = db::open(&config)?; // Clone config for search index init
                                 // Initialise the search index
    let search_index =
        initialise_search_index(&config).map_err(|e| AppError::Search(e.to_string()))?;

    match cli.command {
        Commands::Ingest { force } => {
            let report = ingest::run(&config, &db, &search_index, force)?;
            colours::info(&format!(
                "\nDone — {} found, {} new, {} skipped, {} errors",
                report.found, report.new, report.skipped, report.errors
            ));
            Ok(())
        }
        Commands::Watch => {
            watch::run(&config, &db, &search_index)?;
            Ok(())
        }
        Commands::List { verbose } => {
            let mut records = search::all_records(&db);
            if records.is_empty() {
                colours::info("No screenshots indexed yet. Run `shotext ingest` first.");
                return Ok(());
            }

            records.sort_by(|a, b| a.created_at.cmp(&b.created_at));

            colours::info(&format!("{} indexed screenshots\n", records.len()));

            // Header
            if verbose {
                println!("{:<12}  {:<16}  {:<60}  TEXT", "HASH", "DATE", "PATH");
                println!("{}", "─".repeat(120));
            } else {
                println!("{:<64}  {:<16}  PATH", "HASH", "DATE");
                println!("{}", "─".repeat(120));
            }

            for r in &records {
                let file_name = std::path::Path::new(&r.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&r.path);

                if verbose {
                    println!("{:<64}  {:<16}  {}", r.hash, r.created_at, r.path);
                    let snippet = ocr::truncate(&r.content, 75).replace('\n', " ");
                    if !snippet.is_empty() {
                        println!("  └─ {}", snippet);
                    }
                } else {
                    println!("{:<64}  {:<16}  {:<60}", r.hash, r.created_at, file_name);
                }
            }
            Ok(())
        }
        Commands::Search { query } => {
            match query {
                Some(q) if !q.is_empty() => {
                    // Tantivy full-text search
                    colours::info(&format!("Searching for: \"{}\"", q));
                    let results = search::query(&search_index, &q, 20)?;
                    search::print_results(&results);
                }
                _ => {
                    // Interactive skim fuzzy search over all ingested records
                    let records = search::all_records(&db);
                    if records.is_empty() {
                        colours::info("No screenshots indexed yet. Run `shotext ingest` first.");
                        return Ok(());
                    }
                    colours::info(&format!(
                        "Loaded {} records — launching fuzzy finder…",
                        records.len()
                    ));
                    match search::interactive_search(&records) {
                        Some(idx) => {
                            let r = &records[idx];
                            let vt = ViewTarget {
                                path: r.path.clone(),
                                content: r.content.clone(),
                                hash: r.hash.clone(),
                                tags: r.tags.clone(),
                            };
                            launch_viewer(&vt, db, &search_index)?;
                        }
                        None => colours::info("Search cancelled."),
                    }
                }
            }
            Ok(())
        }
        Commands::View { target } => {
            let vt = resolve_view_target(&target, &db)?;
            launch_viewer(&vt, db, &search_index)?;
            Ok(())
        }
        Commands::X => {
            let records = search::all_records(&db);
            if records.is_empty() {
                colours::info("No screenshots indexed yet. Run `shotext ingest` first.");
                return Ok(());
            }
            colours::info(&format!(
                "Launching dashboard with {} records…",
                records.len()
            ));
            experimental_ui::launch_dashboard(records, search_index, db)?;
            Ok(())
        }
        Commands::Tag {
            target,
            add,
            remove,
        } => {
            let hash = resolve_hash(&target, &db)?;

            // Need a writer for re-indexing
            let mut tantivy_writer =
                search::writer(&search_index).map_err(|e| AppError::Search(e.to_string()))?;

            // Add tags
            for tag in &add {
                if let Some(record) = db::add_tag(&db, &hash, tag)? {
                    search::reindex_document(&mut tantivy_writer, &hash, &record)?;
                    colours::success(&format!("  + added tag \"{}\"", tag));
                }
            }

            // Remove tags
            for tag in &remove {
                if let Some(record) = db::remove_tag(&db, &hash, tag)? {
                    search::reindex_document(&mut tantivy_writer, &hash, &record)?;
                    colours::success(&format!("  − removed tag \"{}\"", tag));
                }
            }

            // If no add/remove flags, just list current tags
            if add.is_empty() && remove.is_empty() {
                if let Some(record) = db::get_record(&db, &hash)? {
                    if record.tags.is_empty() {
                        colours::info("No tags on this screenshot.");
                    } else {
                        colours::info(&format!("Tags: {}", record.tags.join(", ")));
                    }
                }
            }

            Ok(())
        }
        Commands::Config { edit } => {
            if edit {
                let path = config::config_path();
                let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                std::process::Command::new(&editor)
                    .arg(&path)
                    .status()
                    .map_err(|e| AppError::ConfigError(format!("Failed to open editor: {}", e)))?;
            } else {
                let path = config::config_path();
                colours::info(&format!("Config file: {}\n", path.display()));
                println!("{}", config);
            }
            Ok(())
        }
        Commands::Update => {
            println!("{}", "--- Checking for updates ---".blue());
            let status = self_update::backends::github::Update::configure()
                .repo_owner("cladam")
                .repo_name("shotext")
                .bin_name("shotext")
                .show_download_progress(true)
                .current_version(self_update::cargo_crate_version!())
                .build()?
                .update()?;

            println!("Update status: `{}`!", status.version());
            if status.updated() {
                println!("{}", "Successfully updated shotext!".green());
            } else {
                println!("{}", "shotext is already up to date.".green());
            }
            Ok(())
        }
    }
}

/// Resolved view target with all data needed by the viewer.
struct ViewTarget {
    path: String,
    content: String,
    hash: String,
    tags: Vec<String>,
}

/// Resolve a view target to all metadata needed for the viewer.
///
/// The target can be:
/// - A file path to a PNG (hashes the file and looks up text in sled)
/// - A blake3 hash (looks up the record directly in sled)
fn resolve_view_target(target: &str, db: &sled::Db) -> Result<ViewTarget, AppError> {
    let path = std::path::Path::new(target);

    if path.exists() && path.is_file() {
        let bytes = std::fs::read(path)?;
        let hash = blake3::hash(&bytes).to_hex().to_string();

        if let Some(record) = db::get_record(db, &hash)? {
            return Ok(ViewTarget {
                path: target.to_string(),
                content: record.content,
                hash,
                tags: record.tags,
            });
        }

        // File exists but hasn't been indexed yet
        return Ok(ViewTarget {
            path: target.to_string(),
            content: "(not yet indexed — run `shotext ingest` first)".to_string(),
            hash,
            tags: Vec::new(),
        });
    }

    if let Some(record) = db::get_record(db, target)? {
        return Ok(ViewTarget {
            path: record.path,
            content: record.content,
            hash: target.to_string(),
            tags: record.tags,
        });
    }

    Err(AppError::GuiError(format!(
        "Target not found: '{}' — provide a file path or a known hash",
        target
    )))
}

/// Resolve a target (file path or hash) into the blake3 hash key used in the database.
fn resolve_hash(target: &str, db: &sled::Db) -> Result<String, AppError> {
    let path = std::path::Path::new(target);

    if path.exists() && path.is_file() {
        let bytes = std::fs::read(path)?;
        let hash = blake3::hash(&bytes).to_hex().to_string();
        if db::key_exists(db, &hash)? {
            return Ok(hash);
        }
        return Err(AppError::Database(format!(
            "File exists but is not indexed: '{}'. Run `shotext ingest` first.",
            target
        )));
    }

    // Assume it's a hash
    if db::key_exists(db, target)? {
        return Ok(target.to_string());
    }

    Err(AppError::Database(format!(
        "Not found: '{}' — provide a file path or a known hash",
        target
    )))
}

/// Read the image from disk and open the egui viewer window.
fn launch_viewer(
    vt: &ViewTarget,
    db: sled::Db,
    search_index: &tantivy::Index,
) -> Result<(), AppError> {
    let image_bytes = std::fs::read(&vt.path)
        .map_err(|e| AppError::GuiError(format!("Failed to read image {}: {}", vt.path, e)))?;

    colours::info(&format!("Opening viewer for: {}", vt.path));
    let v = viewer::ShotViewer::new(
        &vt.path,
        vt.content.clone(),
        image_bytes,
        vt.hash.clone(),
        vt.tags.clone(),
        db,
        search_index,
    )?;
    v.launch().map_err(|e| AppError::GuiError(e.to_string()))?;
    Ok(())
}
