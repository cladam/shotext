use crate::colours::warn;
use crate::config::Config;
use crate::error::AppError;
use serde_json;
use sled::Db;
use std::path::PathBuf;
use std::{env, fs, str};
use tantivy::{Index, IndexWriter, TantivyDocument};

// Helper function to open the database
pub fn open(config: Config) -> Result<Db, AppError> {
    let db_path = match env::var("SHOTEXT_DB_PATH") {
        Ok(path_str) => PathBuf::from(path_str),
        Err(_) => config.db_path.clone().unwrap_or_else(|| {
            // Default path logic
            let mut path = dirs::home_dir().expect("Could not find home directory.");
            path.push(".shotext/shotext_db");
            path
        }),
    };

    // Ensure the parent directory exists.
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    sled::open(db_path).map_err(AppError::from)
}

/// Checks if a key exists in the database.
pub fn key_exists(db: &Db, key: &str) -> Result<bool, AppError> {
    db.contains_key(key).map_err(AppError::from)
}

