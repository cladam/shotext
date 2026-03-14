use crate::config::Config;
use crate::error::AppError;
use std::env;
use std::path::PathBuf;

pub use cli::{Cli, Commands};

pub mod cli;
pub mod colours;
pub mod config;
pub mod db;
pub mod error;
mod indexer;
pub mod ingest;
pub mod ocr;
pub mod search;

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
    let db = db::open(config.clone())?; // Clone config for search index init
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
            // TODO: implement watch logic
            colours::info("Watch mode started (not yet implemented)");
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
                        Some(idx) => search::print_detail(&records[idx]),
                        None => colours::info("Search cancelled."),
                    }
                }
            }
            Ok(())
        }
        Commands::View { target } => {
            // TODO: implement view logic
            colours::info(&format!("View called for target: {}", target));
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
    }
}
