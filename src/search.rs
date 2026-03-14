use crate::error::AppError;
use crate::ingest::ShotRecord;
use crate::ocr;

use chrono::NaiveDateTime;
use lazy_static::lazy_static;
use skim::prelude::*;
use sled::Db;
use std::io::Cursor;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::schema::document::Value;
use tantivy::schema::*;
use tantivy::{Index, IndexWriter, TantivyDocument};

// ---------------------------------------------------------------------------
// Schema (single source of truth)
// ---------------------------------------------------------------------------

lazy_static! {
    pub static ref SCHEMA: Schema = {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("path", STORED);
        schema_builder.add_text_field("content", TEXT | STORED);
        schema_builder.add_text_field("hash", STORED);
        schema_builder.add_date_field(
            "created_at",
            DateOptions::default().set_fast().set_stored().set_indexed(),
        );
        schema_builder.build()
    };
}

/// A single search result.
pub struct SearchResult {
    pub hash: String,
    pub path: String,
    pub content: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Index management
// ---------------------------------------------------------------------------

/// Opens an existing Tantivy index or creates a new one at `path`.
pub fn open_index(path: &Path) -> Result<Index, tantivy::error::TantivyError> {
    std::fs::create_dir_all(path)?;
    let directory = MmapDirectory::open(path)?;
    Index::open_or_create(directory, SCHEMA.clone())
}

/// Create an `IndexWriter` for the given index (50 MB heap).
pub fn writer(index: &Index) -> Result<IndexWriter, tantivy::error::TantivyError> {
    index.writer(50_000_000)
}

// ---------------------------------------------------------------------------
// Indexing (writing documents)
// ---------------------------------------------------------------------------

/// Add a screenshot document to the Tantivy full-text index.
pub fn index_document(
    writer: &IndexWriter,
    hash: &str,
    path: &str,
    content: &str,
    created_at_str: &str,
) -> Result<(), AppError> {
    let path_field = SCHEMA.get_field("path").unwrap();
    let content_field = SCHEMA.get_field("content").unwrap();
    let hash_field = SCHEMA.get_field("hash").unwrap();
    let created_at_field = SCHEMA.get_field("created_at").unwrap();

    let created_at = parse_date_to_tantivy(created_at_str);

    let mut doc = TantivyDocument::default();
    doc.add_text(path_field, path);
    doc.add_text(content_field, content);
    doc.add_text(hash_field, hash);
    doc.add_date(created_at_field, created_at);

    writer
        .add_document(doc)
        .map_err(|e| AppError::Search(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Querying (Tantivy full-text search)
// ---------------------------------------------------------------------------

/// Full-text search over the Tantivy index. Returns up to `limit` results
/// ranked by relevance.
pub fn query(index: &Index, query_str: &str, limit: usize) -> Result<Vec<SearchResult>, AppError> {
    let reader = index
        .reader()
        .map_err(|e| AppError::Search(e.to_string()))?;
    let searcher = reader.searcher();

    let content_field = SCHEMA.get_field("content").unwrap();
    let query_parser = QueryParser::for_index(index, vec![content_field]);
    let parsed = query_parser
        .parse_query(query_str)
        .map_err(|e| AppError::Search(e.to_string()))?;

    let top_docs = searcher
        .search(&parsed, &TopDocs::with_limit(limit))
        .map_err(|e| AppError::Search(e.to_string()))?;

    let path_field = SCHEMA.get_field("path").unwrap();
    let hash_field = SCHEMA.get_field("hash").unwrap();
    let created_at_field = SCHEMA.get_field("created_at").unwrap();

    let mut results = Vec::new();
    for (_score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher
            .doc(doc_address)
            .map_err(|e| AppError::Search(e.to_string()))?;

        results.push(SearchResult {
            path: field_as_str(&doc, path_field),
            content: field_as_str(&doc, content_field),
            hash: field_as_str(&doc, hash_field),
            created_at: field_as_date_string(&doc, created_at_field),
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Sled iteration (for skim interactive mode)
// ---------------------------------------------------------------------------

/// Load every ingested record from the sled database.
pub fn all_records(db: &Db) -> Vec<SearchResult> {
    db.iter()
        .filter_map(|result| {
            let (key, value) = result.ok()?;
            let hash = String::from_utf8(key.to_vec()).ok()?;
            let record: ShotRecord = serde_json::from_slice(&value).ok()?;
            Some(SearchResult {
                hash,
                path: record.path,
                content: record.content,
                created_at: record.created_at,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Interactive fuzzy search with skim
// ---------------------------------------------------------------------------

/// Launch an interactive skim fuzzy-finder over the given results.
/// Returns the index of the selected result, or `None` if the user aborted.
pub fn interactive_search(results: &[SearchResult]) -> Option<usize> {
    if results.is_empty() {
        return None;
    }

    // Build display lines: "  idx │ [date] path — snippet"
    let lines: Vec<String> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let snippet = ocr::truncate(&r.content, 80).replace('\n', " ");
            format!("{:>4} │ [{}] {} — {}", i, r.created_at, r.path, snippet)
        })
        .collect();

    let input = lines.join("\n");

    let options = SkimOptionsBuilder::default()
        .height("80%".to_string())
        .multi(false)
        .reverse(true)
        .prompt("🔍 ".to_string())
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(input));

    let output = Skim::run_with(options, Some(items)).ok()?;
    if output.is_abort {
        return None;
    }

    // Parse the index back from the selected line ("  42 │ …")
    output.selected_items.first().and_then(|item| {
        let text = item.output();
        let idx_str = text.split('│').next()?.trim();
        idx_str.parse::<usize>().ok()
    })
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

/// Print a list of search results to stdout.
pub fn print_results(results: &[SearchResult]) {
    use crate::colours;

    if results.is_empty() {
        colours::info("No results found.");
        return;
    }

    for r in results {
        println!();
        colours::success(&format!("  📄 {}", r.path));
        colours::info(&format!("     Date: {}", r.created_at));
        println!("     Text: {}", ocr::truncate(&r.content, 200));
    }
    println!();
}

/// Print full detail of a single search result.
pub fn print_detail(r: &SearchResult) {
    use crate::colours;

    println!();
    colours::success(&format!("  📄 {}", r.path));
    colours::info(&format!("     Date:  {}", r.created_at));
    colours::info(&format!("     Hash:  {}", r.hash));
    println!("     ─── Extracted Text ───");
    println!("{}", r.content);
    println!();
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse a date string (e.g. "2025-04-23 20:36") into a `tantivy::DateTime`.
fn parse_date_to_tantivy(date_str: &str) -> tantivy::DateTime {
    NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S"))
        .map(|dt| tantivy::DateTime::from_timestamp_secs(dt.and_utc().timestamp()))
        .unwrap_or_else(|_| tantivy::DateTime::from_timestamp_secs(0))
}

/// Extract a text field value from a `TantivyDocument`.
fn field_as_str(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Extract a date field from a `TantivyDocument` and format as "YYYY-MM-DD HH:MM".
fn field_as_date_string(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_datetime())
        .and_then(|dt| {
            let ts = dt.into_timestamp_secs();
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|ct| ct.format("%Y-%m-%d %H:%M").to_string())
        })
        .unwrap_or_default()
}
