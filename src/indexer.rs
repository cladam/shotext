use sled::Db;
use std::path::Path;
use tantivy::{schema::*, Index, IndexWriter};

pub struct ShotIndexer {
    kv_store: Db,
    index: Index,
    writer: IndexWriter,
    // Schema fields
    path_field: Field,
    text_field: Field,
    hash_field: Field,
}

impl ShotIndexer {
    pub fn new(db_path: &Path, index_path: &Path) -> anyhow::Result<Self> {
        let kv_store = sled::open(db_path)?;

        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STORED);
        let text_field = schema_builder.add_text_field("text", TEXT | STORED);
        let hash_field = schema_builder.add_text_field("hash", STORED);
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
            text_field,
            hash_field,
        })
    }

    pub fn add_shot(&mut self, hash: &str, path: &str, text: &str) -> anyhow::Result<()> {
        self.kv_store.insert(hash, text)?;

        let mut doc = tantivy::TantivyDocument::default();
        doc.add_text(self.path_field, path);
        doc.add_text(self.text_field, text);
        doc.add_text(self.hash_field, hash);

        self.writer.add_document(doc)?;
        self.writer.commit()?;
        Ok(())
    }
}
