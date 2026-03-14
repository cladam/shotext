use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{PollWatcher, RecursiveMode, Watcher};
use sled::Db;

use crate::colours;
use crate::config::Config;
use crate::error::AppError;
use crate::ingest;
use crate::search;

/// Start watching the screenshots directory for new PNG files.
///
/// Blocks forever (until Ctrl-C). When a new or modified PNG is detected
/// it is automatically OCR'd and indexed, just like `shotext ingest` would do.
///
/// Uses a `PollWatcher` instead of the platform-native FSEvents backend because
/// macOS FSEvents can silently miss events for files created by system processes
/// (e.g. the screenshot utility) or in protected directories like `~/Desktop`.
pub fn run(config: &Config, db: &Db, index: &tantivy::Index) -> Result<(), AppError> {
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

    // Resolve to an absolute/canonical path so there's no ambiguity
    let canonical =
        std::fs::canonicalize(screenshots_dir).unwrap_or_else(|_| screenshots_dir.clone());
    colours::info(&format!("  Resolved watch path: {}", canonical.display()));

    // Channel to receive filesystem events
    let (tx, rx) = mpsc::channel();

    // Use PollWatcher with a short interval. This is more reliable than the
    // FSEvents backend on macOS, which can miss events from system processes
    // (like the screenshot utility) or for directories protected by TCC.
    let poll_config = notify::Config::default().with_poll_interval(Duration::from_secs(2));

    let mut watcher = PollWatcher::new(
        move |res: Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                if let Err(e) = tx.send(event) {
                    eprintln!("  [poll-watcher] channel send failed: {}", e);
                }
            }
            Err(e) => {
                eprintln!("  [poll-watcher] ⚠ error: {}", e);
            }
        },
        poll_config,
    )
    .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    watcher
        .watch(&canonical, RecursiveMode::Recursive)
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    colours::success(&format!(
        "👁  Watching {} for new screenshots (Ctrl-C to stop)",
        canonical.display()
    ));
    let _ = std::io::stdout().flush();

    // Create one Tantivy writer for the watcher session
    let mut tantivy_writer = search::writer(index).map_err(|e| AppError::Search(e.to_string()))?;

    // Event loop with periodic heartbeat so you can tell it's alive
    loop {
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(event) => {
                // Log every raw event on the main thread too
                let kind_label = format!("{:?}", event.kind);
                for p in &event.paths {
                    colours::info(&format!("  📡 {} → {}", kind_label, p.display()));
                }
                let _ = std::io::stdout().flush();

                // Only care about Create and data-Modify events
                let dominated_event = matches!(
                    event.kind,
                    notify::EventKind::Create(_) | notify::EventKind::Modify(_)
                );

                // Collect PNG paths from this event
                let mut paths: Vec<std::path::PathBuf> = if dominated_event {
                    event
                        .paths
                        .into_iter()
                        .filter(|p| ingest::is_png(p))
                        .collect()
                } else {
                    Vec::new()
                };

                // Drain any events that arrive within the debounce window
                let debounce = Duration::from_millis(1500);
                while let Ok(extra) = rx.recv_timeout(debounce) {
                    for p in &extra.paths {
                        colours::info(&format!("  📡 {:?} → {}", extra.kind, p.display()));
                    }
                    if matches!(
                        extra.kind,
                        notify::EventKind::Create(_) | notify::EventKind::Modify(_)
                    ) {
                        paths.extend(extra.paths.into_iter().filter(|p| ingest::is_png(p)));
                    }
                }

                // Deduplicate
                paths.sort();
                paths.dedup();

                if paths.is_empty() {
                    colours::info("  ↳ no new PNG files in this batch, skipping");
                    let _ = std::io::stdout().flush();
                    continue;
                }

                colours::info(&format!("  ↳ processing {} PNG file(s)…", paths.len()));
                let _ = std::io::stdout().flush();

                for path in &paths {
                    process_and_commit(path, config, db, &mut tantivy_writer);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Heartbeat — so you know the loop is alive
                colours::info("  💓 still watching… (no events in the last 30s)");
                let _ = std::io::stdout().flush();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                colours::warn("File watcher stopped unexpectedly (channel closed).");
                break;
            }
        }
    }

    Ok(())
}

/// Process a single file and commit the Tantivy writer.
/// Errors are logged but do not crash the watcher.
fn process_and_commit(
    path: &Path,
    config: &Config,
    db: &Db,
    tantivy_writer: &mut tantivy::IndexWriter,
) {
    match ingest::process_single_file(path, config, db, tantivy_writer) {
        Ok(()) => {
            if let Err(e) = tantivy_writer.commit() {
                colours::warn(&format!("  ✗ Tantivy commit failed: {}", e));
            }
        }
        Err(e) => {
            colours::warn(&format!("  ✗ {}: {}", path.display(), e));
        }
    }
}
