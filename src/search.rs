use crate::error::AppError;
use crate::ingest::ShotRecord;
use crate::{colours, ocr};

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

// Schema
lazy_static! {
    pub static ref SCHEMA: Schema = {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("path", STORED);
        schema_builder.add_text_field("content", TEXT | STORED);
        schema_builder.add_text_field("hash", STRING | STORED);
        schema_builder.add_date_field(
            "created_at",
            DateOptions::default().set_fast().set_stored().set_indexed(),
        );
        schema_builder.add_text_field("tags", TEXT | STORED);
        schema_builder.build()
    };
}

/// A single search result.
pub struct SearchResult {
    pub hash: String,
    pub path: String,
    pub content: String,
    pub created_at: String,
    pub tags: Vec<String>,
}

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

/// Add a screenshot document to the Tantivy full-text index.
pub fn index_document(
    writer: &IndexWriter,
    hash: &str,
    path: &str,
    content: &str,
    created_at_str: &str,
    tags: &[String],
) -> Result<(), AppError> {
    let path_field = SCHEMA.get_field("path")?;
    let content_field = SCHEMA.get_field("content")?;
    let hash_field = SCHEMA.get_field("hash")?;
    let created_at_field = SCHEMA.get_field("created_at")?;
    let tags_field = SCHEMA.get_field("tags")?;

    let created_at = parse_date_to_tantivy(created_at_str);

    let mut doc = TantivyDocument::default();
    doc.add_text(path_field, path);
    doc.add_text(content_field, content);
    doc.add_text(hash_field, hash);
    doc.add_date(created_at_field, created_at);
    doc.add_text(tags_field, tags.join(" "));

    writer
        .add_document(doc)
        .map_err(|e| AppError::Search(e.to_string()))?;
    Ok(())
}

/// Delete a document from the Tantivy index by its hash.
pub fn delete_document(writer: &mut IndexWriter, hash: &str) -> Result<(), AppError> {
    let hash_field = SCHEMA.get_field("hash")?;
    let term = tantivy::Term::from_field_text(hash_field, hash);
    writer.delete_term(term);
    writer
        .commit()
        .map_err(|e| AppError::Search(e.to_string()))?;
    Ok(())
}

/// Re-index a document: delete the old one and add a fresh copy with current data.
/// Use this after updating tags or other mutable fields.
pub fn reindex_document(
    writer: &mut IndexWriter,
    hash: &str,
    record: &ShotRecord,
) -> Result<(), AppError> {
    let hash_field = SCHEMA.get_field("hash")?;
    let term = tantivy::Term::from_field_text(hash_field, hash);
    writer.delete_term(term);
    index_document(
        writer,
        hash,
        &record.path,
        &record.content,
        &record.created_at,
        &record.tags,
    )?;
    writer
        .commit()
        .map_err(|e| AppError::Search(e.to_string()))?;
    Ok(())
}

/// Full-text search over the Tantivy index. Returns up to `limit` results ranked by relevance.
pub fn query(index: &Index, query_str: &str, limit: usize) -> Result<Vec<SearchResult>, AppError> {
    let reader = index
        .reader()
        .map_err(|e| AppError::Search(e.to_string()))?;
    let searcher = reader.searcher();

    let content_field = SCHEMA.get_field("content")?;
    let tags_field = SCHEMA.get_field("tags")?;
    let query_parser = QueryParser::for_index(index, vec![content_field, tags_field]);
    let parsed = query_parser
        .parse_query(query_str)
        .map_err(|e| AppError::Search(e.to_string()))?;

    let top_docs = searcher
        .search(&parsed, &TopDocs::with_limit(limit))
        .map_err(|e| AppError::Search(e.to_string()))?;

    let path_field = SCHEMA.get_field("path")?;
    let hash_field = SCHEMA.get_field("hash")?;
    let created_at_field = SCHEMA.get_field("created_at")?;

    let mut results = Vec::new();
    for (_score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher
            .doc(doc_address)
            .map_err(|e| AppError::Search(e.to_string()))?;

        let tags_str = field_as_str(&doc, tags_field);
        let tags: Vec<String> = if tags_str.is_empty() {
            Vec::new()
        } else {
            tags_str.split_whitespace().map(String::from).collect()
        };

        results.push(SearchResult {
            path: field_as_str(&doc, path_field),
            content: field_as_str(&doc, content_field),
            hash: field_as_str(&doc, hash_field),
            created_at: field_as_date_string(&doc, created_at_field),
            tags,
        });
    }

    Ok(results)
}

/// Load every ingested record from the sled database.
pub fn all_records(db: &Db) -> Vec<SearchResult> {
    db.iter()
        .filter_map(|result| {
            let (key, value) = match result {
                Ok(kv) => kv,
                Err(e) => {
                    colours::warn(&format!("Failed to read DB entry: {e}"));
                    return None;
                }
            };

            let hash = match String::from_utf8(key.to_vec()) {
                Ok(h) => h,
                Err(e) => {
                    colours::warn(&format!("Invalid UTF-8 in DB key: {e}"));
                    return None;
                }
            };

            let record: ShotRecord = match serde_json::from_slice(&value) {
                Ok(r) => r,
                Err(e) => {
                    colours::warn(&format!("Failed to deserialize record {hash}: {e}"));
                    return None;
                }
            };

            Some(SearchResult {
                hash,
                path: record.path,
                content: record.content,
                created_at: record.created_at,
                tags: record.tags,
            })
        })
        .collect()
}

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

    let output = Skim::run_with(&options, Some(items))?;
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
        if !r.tags.is_empty() {
            colours::info(&format!("     Tags: {}", r.tags.join(", ")));
        }
        println!("     Text: {}", ocr::truncate(&r.content, 200));
    }
    println!();
}

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
