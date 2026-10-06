//! File discovery and text extraction.

use std::path::{Path, PathBuf};

/// Files larger than this are skipped for embedding (grep still scans them).
pub const MAX_INDEX_BYTES: u64 = 2 * 1024 * 1024;

/// Walk `root` respecting .gitignore/.ignore and hidden-file rules.
pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(root)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .collect();
    files.sort();
    files
}

/// True when the first 8 KiB contain a NUL byte.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Extract indexable text, or `None` for binary/unsupported/oversized files.
pub fn extract_text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_INDEX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if is_pdf(path) {
        let text = pdf_extract::extract_text_from_mem(&bytes).ok()?;
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        return (!text.is_empty()).then_some(text);
    }
    if looks_binary(&bytes) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (!text.trim().is_empty()).then_some(text)
}
