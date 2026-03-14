use sled::Db;
use std::path::Path;
use tantivy::schema::*;
use tantivy::{Index, IndexWriter};

pub struct ShotIndexer {
    kv_store: Db,
    index: Index,
    writer: IndexWriter,
    // Schema fields
    path_field: Field,
    content_field: Field,
    hash_field: Field,
    created_at_field: Field,
}

impl ShotIndexer {
    pub fn new(db_path: &Path, index_path: &Path) -> anyhow::Result<Self> {
        let kv_store = sled::open(db_path)?;

        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let hash_field = schema_builder.add_text_field("hash", STORED);
        let created_at_field = schema_builder.add_date_field(
            "created_at",
            DateOptions::default().set_fast().set_stored().set_indexed(),
        );
        let schema = schema_builder.build();

        std::fs::create_dir_all(index_path)?;
        let index =
            Index::open_or_create(tantivy::directory::MmapDirectory::open(index_path)?, schema)?;
        let writer = index.writer(50_000_000)?; // 50MB heap

        Ok(Self {
            kv_store,
            index,
            writer,
            path_field,
            content_field,
            hash_field,
            created_at_field,
        })
    }

    /// Add a screenshot to the index.
    ///
    /// `created_at` is the file's mtime (when the screenshot was actually taken)
    /// expressed as a UTC `tantivy::DateTime`.  The caller can derive it from
    /// `std::fs::metadata(path)?.modified()?`.
    pub fn add_shot(
        &mut self,
        hash: &str,
        path: &str,
        content: &str,
        created_at: tantivy::DateTime,
    ) -> anyhow::Result<()> {
        self.kv_store.insert(hash, content)?;

        let mut doc = tantivy::TantivyDocument::default();
        doc.add_text(self.path_field, path);
        doc.add_text(self.content_field, content);
        doc.add_text(self.hash_field, hash);
        doc.add_date(self.created_at_field, created_at);

        self.writer.add_document(doc)?;
        self.writer.commit()?;
        Ok(())
    }

    /// Helper: read a file's mtime and convert it to a `tantivy::DateTime`.
    pub fn file_created_at(file_path: &Path) -> anyhow::Result<tantivy::DateTime> {
        let meta = std::fs::metadata(file_path)?;
        let mtime = meta.modified()?;
        let duration = mtime
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Ok(tantivy::DateTime::from_timestamp_secs(
            duration.as_secs() as i64,
        ))
    }
}
