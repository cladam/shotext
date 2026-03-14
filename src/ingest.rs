use std::fs;
use std::path::Path;

use blake3;
use chrono::{DateTime, Local, NaiveDateTime};
use sled::Db;
use walkdir::WalkDir;

use crate::colours;
use crate::config::Config;
use crate::db;
use crate::error::AppError;

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
pub fn run(config: &Config, db: &Db, force: bool) -> Result<IngestReport, AppError> {
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

        // --- placeholder: OCR + indexing will happen here later ---

        // For now, just mark the file as "seen" in sled so the next run skips it.
        // We store the original path as the value — handy for debugging.
        if let Err(e) = db.insert(hash.as_bytes(), path.to_string_lossy().as_bytes()) {
            colours::warn(&format!("  ✗ DB insert failed for {}: {}", path.display(), e));
            report.errors += 1;
            continue;
        }

        let date_str = screenshot_date(path)
            .unwrap_or_else(|| "unknown date".into());

        colours::success(&format!(
            "  ✔ {} ({})",
            path.display(),
            date_str,
        ));
        report.new += 1;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the path has a `.png` extension (case-insensitive).
fn is_png(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("png"))
        .unwrap_or(false)
}

/// Hash an entire file with blake3 and return the hex digest.
fn hash_file(path: &Path) -> Result<String, AppError> {
    let bytes = fs::read(path)?;
    let hash = blake3::hash(&bytes);
    Ok(hash.to_hex().to_string())
}

/// Determine when a screenshot was taken.
///
/// 1. Try parsing the macOS filename convention: `Screenshot YYYY-MM-DD at HH.MM.SS`
/// 2. Fall back to the file's mtime (displayed in local time via chrono).
fn screenshot_date(path: &Path) -> Option<String> {
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





