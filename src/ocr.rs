use crate::error::AppError;
// OCR module — extracts text from screenshot images using Tesseract.
use tesseract::{PageSegMode, Tesseract};

/// Run Tesseract OCR on the image at `path` using the given language code
/// (e.g. "eng", "swe", "deu").
///
/// Uses **sparse-text** page segmentation (PSM 11) so Tesseract finds as much
/// text as possible — including footers, sidebars and isolated UI labels that
/// the default "auto" mode would skip.
pub fn extract_text(path: &str, language: &str) -> Result<String, AppError> {
    let mut tes = Tesseract::new(None, Some(language))?
        .set_image(path)?
        .set_source_resolution(72); // screenshots are 72–144 ppi, not 300
    tes.set_page_seg_mode(PageSegMode::PsmSparseText);
    let text = tes.get_text()?;
    // Trim trailing whitespace/newlines that Tesseract often appends
    Ok(text.trim().to_string())
}

/// Return at most `max_chars` characters, appending "…" if truncated.
pub fn truncate(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        text.to_string()
    } else {
        let end = text
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        format!("{}…", &text[..end])
    }
}
