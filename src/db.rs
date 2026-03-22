use crate::config::Config;
use crate::error::AppError;
use crate::ingest::ShotRecord;
use sled::Db;
use std::path::PathBuf;
use std::{env, fs, str};

// Helper function to open the database
pub fn open(config: &Config) -> Result<Db, AppError> {
    let db_path = match env::var("SHOTEXT_DB_PATH") {
        Ok(path_str) => PathBuf::from(path_str),
        Err(_) => config.paths.database.clone(),
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

// Get a single record
pub fn get_record(db: &Db, key: &str) -> Result<Option<ShotRecord>, AppError> {
    match db.get(key)? {
        Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        None => Ok(None),
    }
}

/// Delete a record from the database by key.
pub fn delete_record(db: &Db, key: &str) -> Result<(), AppError> {
    db.remove(key)?;
    Ok(())
}

/// Save a record back to the database (overwrites existing).
pub fn save_record(db: &Db, key: &str, record: &ShotRecord) -> Result<(), AppError> {
    let json = record.to_json()?;
    db.insert(key, json)?;
    Ok(())
}

/// Add a tag to a record. Returns the updated record, or None if the hash was not found.
/// Duplicate tags are silently ignored.
pub fn add_tag(db: &Db, hash: &str, tag: &str) -> Result<Option<ShotRecord>, AppError> {
    let mut record = match get_record(db, hash)? {
        Some(r) => r,
        None => return Ok(None),
    };

    let tag = tag.to_lowercase();
    if !record.tags.contains(&tag) {
        record.tags.push(tag);
        save_record(db, hash, &record)?;
    }

    Ok(Some(record))
}

/// Remove a tag from a record. Returns the updated record, or None if the hash was not found.
pub fn remove_tag(db: &Db, hash: &str, tag: &str) -> Result<Option<ShotRecord>, AppError> {
    let mut record = match get_record(db, hash)? {
        Some(r) => r,
        None => return Ok(None),
    };

    let tag = tag.to_lowercase();
    record.tags.retain(|t| t != &tag);
    save_record(db, hash, &record)?;

    Ok(Some(record))
}
