use std::fs;
use std::path::Path;

use blake3;
use chrono::{DateTime, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sled::Db;
use walkdir::WalkDir;

use crate::colours;
use crate::config::Config;
use crate::db;
use crate::error::AppError;
use crate::ocr;
use crate::search;

/// Metadata stored in sled for each ingested screenshot.
#[derive(Serialize, Deserialize, Debug)]
pub struct ShotRecord {
    pub path: String,
    pub content: String,
    pub created_at: String,
}

impl ShotRecord {
    pub fn new(path: String, content: String, created_at: String) -> Self {
        Self {
            path,
            content,
            created_at,
        }
    }

    pub fn to_json(&self) -> Result<Vec<u8>, AppError> {
        serde_json::to_vec(self)
            .map_err(|e| AppError::Database(format!("Failed to serialize record: {e}")))
    }
}

/// Result summary returned after an ingest run.
pub struct IngestReport {
    pub found: usize,
    pub new: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Walk the screenshots directory, find all `.png` files, hash them with
/// blake3 for deduplication, and report what was found.
///
/// When `force` is true every file is treated as new (the hash check is skipped).
///
/// OCR is **not** performed yet — this only discovers and deduplicates files.
pub fn run(
    config: &Config,
    db: &Db,
    index: &tantivy::Index,
    force: bool,
) -> Result<IngestReport, AppError> {
    let screenshots_dir = &config.paths.screenshots;

    if !screenshots_dir.exists() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Screenshots directory does not exist: {}",
                screenshots_dir.display()
            ),
        )));
    }

    colours::info(&format!(
        "Scanning {} for PNG files…",
        screenshots_dir.display()
    ));

    // Create one Tantivy writer for the entire ingest run
    let mut tantivy_writer = search::writer(index).map_err(|e| AppError::Search(e.to_string()))?;

    let mut report = IngestReport {
        found: 0,
        new: 0,
        skipped: 0,
        errors: 0,
    };

    for entry in WalkDir::new(screenshots_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Only consider .png files
        if !is_png(path) {
            continue;
        }

        report.found += 1;

        // Hash the file contents with blake3
        let hash = match hash_file(path) {
            Ok(h) => h,
            Err(e) => {
                colours::warn(&format!("  ✗ Failed to hash {}: {}", path.display(), e));
                report.errors += 1;
                continue;
            }
        };

        // Dedup check: skip files we have already seen (unless --force)
        if !force {
            match db::key_exists(db, &hash) {
                Ok(true) => {
                    report.skipped += 1;
                    continue;
                }
                Ok(false) => { /* new file — fall through */ }
                Err(e) => {
                    colours::warn(&format!(
                        "  ✗ DB lookup failed for {}: {}",
                        path.display(),
                        e
                    ));
                    report.errors += 1;
                    continue;
                }
            }
        }

        // --- OCR: extract text from the image ---
        let path_str = path.to_string_lossy().to_string();
        let content = match ocr::extract_text(&path_str, &config.ocr.language) {
            Ok(text) => text,
            Err(e) => {
                colours::warn(&format!("  ✗ OCR failed for {}: {}", path.display(), e));
                report.errors += 1;
                continue;
            }
        };

        let date_str = screenshot_date(path).unwrap_or_else(|| "unknown date".into());

        // Build a record and persist as JSON in sled
        let record = ShotRecord::new(path_str, content, date_str);
        let json = record.to_json()?;

        if let Err(e) = db.insert(hash.as_bytes(), json) {
            colours::warn(&format!(
                "  ✗ DB insert failed for {}: {}",
                path.display(),
                e
            ));
            report.errors += 1;
            continue;
        }

        // Also index in Tantivy for full-text search
        if let Err(e) = search::index_document(
            &tantivy_writer,
            &hash,
            &record.path,
            &record.content,
            &record.created_at,
        ) {
            colours::warn(&format!(
                "  ✗ Search index failed for {}: {}",
                path.display(),
                e
            ));
            // Non-fatal — sled record was already written
        }

        let snippet = ocr::truncate(&record.content, 60);
        colours::success(&format!(
            "  ✔ {} ({}) — \"{}\"",
            record.path, record.created_at, snippet,
        ));
        report.new += 1;
    }

    // Commit the Tantivy writer once at the end
    tantivy_writer
        .commit()
        .map_err(|e| AppError::Search(e.to_string()))?;

    Ok(report)
}

/// Process a single PNG file: hash → dedup → OCR → sled + Tantivy.
///
/// This is the shared logic used by both `ingest` (batch) and `watch` (live).
/// The caller must supply a pre-created Tantivy `IndexWriter` and commit
/// it themselves after one or more calls.
pub fn process_single_file(
    path: &Path,
    config: &Config,
    db: &Db,
    tantivy_writer: &tantivy::IndexWriter,
) -> Result<(), AppError> {
    if !is_png(path) {
        return Ok(());
    }

    let hash = hash_file(path)?;

    // Dedup
    if db::key_exists(db, &hash)? {
        colours::info(&format!("  ⏭ Already indexed: {}", path.display()));
        return Ok(());
    }

    // OCR
    let path_str = path.to_string_lossy().to_string();
    let content = ocr::extract_text(&path_str, &config.ocr.language)?;

    let date_str = screenshot_date(path).unwrap_or_else(|| "unknown date".into());

    // Persist to sled
    let record = ShotRecord::new(path_str, content, date_str);
    let json = record.to_json()?;
    db.insert(hash.as_bytes(), json)?;

    // Index in Tantivy
    search::index_document(
        tantivy_writer,
        &hash,
        &record.path,
        &record.content,
        &record.created_at,
    )?;

    let snippet = ocr::truncate(&record.content, 60);
    colours::success(&format!(
        "  ✔ {} ({}) — \"{}\"",
        record.path, record.created_at, snippet,
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the path has a `.png` extension (case-insensitive).
pub fn is_png(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("png"))
        .unwrap_or(false)
}

/// Hash an entire file with blake3 and return the hex digest.
pub fn hash_file(path: &Path) -> Result<String, AppError> {
    let bytes = fs::read(path)?;
    let hash = blake3::hash(&bytes);
    Ok(hash.to_hex().to_string())
}

/// Determine when a screenshot was taken.
///
/// 1. Try parsing the macOS filename convention: `Screenshot YYYY-MM-DD at HH.MM.SS`
/// 2. Fall back to the file's mtime (displayed in local time via chrono).
pub fn screenshot_date(path: &Path) -> Option<String> {
    // Try the filename first
    if let Some(dt) = parse_macos_screenshot_name(path) {
        return Some(dt.format("%Y-%m-%d %H:%M").to_string());
    }

    // Fallback: file mtime → local time
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dt: DateTime<Local> = mtime.into();
    Some(dt.format("%Y-%m-%d %H:%M").to_string())
}

/// Parse the date/time embedded in a macOS screenshot filename.
/// Handles: `Screenshot 2025-04-23 at 20.36.00.png`
fn parse_macos_screenshot_name(path: &Path) -> Option<NaiveDateTime> {
    let stem = path.file_stem()?.to_str()?;
    let rest = stem.strip_prefix("Screenshot ")?;
    let parts: Vec<&str> = rest.splitn(2, " at ").collect();
    if parts.len() != 2 {
        return None;
    }
    // "2025-04-23" + "20.36.00" → "2025-04-23 20:36:00"
    let combined = format!("{} {}", parts[0], parts[1].replace('.', ":"));
    NaiveDateTime::parse_from_str(&combined, "%Y-%m-%d %H:%M:%S").ok()
}
