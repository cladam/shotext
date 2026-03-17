use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Sled(#[from] sled::Error),

    #[error("Regexp error: {0}")]
    Regexp(#[from] regex::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("JSON serialization/deserialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Self-update error: {0}")]
    SelfUpdate(#[from] self_update::errors::Error),

    #[error("Search operation failed: {0}")]
    Search(String),

    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::error::TantivyError),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("GUI error: {0}")]
    GuiError(String),

    #[error("Failed to initialize Tesseract: {0}")]
    TesseractInit(#[from] tesseract::InitializeError),

    #[error("Failed to set image for OCR: {0}")]
    TesseractImage(#[from] tesseract::SetImageError),

    #[error("Failed to extract text: {0}")]
    TesseractExtraction(#[from] tesseract::plumbing::TessBaseApiGetUtf8TextError),

    // You can still keep a generic one for string-based errors if needed
    #[error("General OCR error: {0}")]
    OcrGeneral(String),
}
