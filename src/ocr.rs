// Basic OCR function using the tesseract crate
use tesseract::Tesseract;

pub fn extract_text(path: &str) -> anyhow::Result<String> {
    let mut tes = Tesseract::new(None, Some("eng"))?;
    tes.set_image(path)?;
    Ok(tes.get_text()?)
}