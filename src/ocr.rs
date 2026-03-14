// OCR module — extracts text from screenshot images using Tesseract.
use tesseract::Tesseract;

/// Run Tesseract OCR on the image at `path` using the given language code
/// (e.g. "eng", "swe", "deu").
pub fn extract_text(path: &str, language: &str) -> anyhow::Result<String> {
    let mut tes = Tesseract::new(None, Some(language))?.set_image(path)?;
    let text = tes.get_text()?;
    // Trim trailing whitespace/newlines that Tesseract often appends
    Ok(text.trim().to_string())
}

/// Return at most `max_chars` characters, appending "…" if truncated.
pub fn truncate(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}…", &text[..max_chars])
    }
}
