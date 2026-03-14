use std::path::PathBuf;
use crate::config::Config;
use std::{env, fs, io};
use crate::error::AppError;

pub use cli::{Cli, Commands};

pub mod cli;
mod indexer;
pub mod config;
pub mod colours;
pub mod error;
pub mod search;
pub mod db;

/// Initialise or open the Tantivy search index located at the specified path.
pub fn initialise_search_index(config: &Config) -> Result<tantivy::Index, AppError> {
    let search_index_path = match env::var("SHOTEXT_DB_PATH") {
        Ok(path_str) => PathBuf::from(path_str).join("search_index"),
        Err(_) => config
            .db_path
            .as_ref()
            .map(|db_path| db_path.join("search_index"))
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
            // TODO: implement ingest logic
            colours::info(&format!("Ingest called (force={})", force));
            Ok(())
        }
        Commands::Watch => {
            // TODO: implement watch logic
            colours::info("Watch mode started (not yet implemented)");
            Ok(())
        }
        Commands::Search { query } => {
            // TODO: implement search logic
            let q = query.unwrap_or_default();
            colours::info(&format!("Search called with query: {}", q));
            Ok(())
        }
        Commands::View { target } => {
            // TODO: implement view logic
            colours::info(&format!("View called for target: {}", target));
            Ok(())
        }
        Commands::Config { edit } => {
            // TODO: implement config logic
            colours::info(&format!("Config called (edit={})", edit));
            Ok(())
        }
    }
}