//! File discovery and text extraction.

use std::path::Path;

/// Files larger than this are skipped for embedding (grep still scans them).
pub const MAX_INDEX_BYTES: u64 = 2 * 1024 * 1024;

/// Directory names never walked, matched case-insensitively by directory name:
/// dependency/build caches and OS directories that hold no user code and would
/// otherwise flood the index and grep results. See `Scope::files`.
pub const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    ".git",
    "$Recycle.Bin",
    "System Volume Information",
    "Windows",
    "Program Files",
    "Program Files (x86)",
    "ProgramData",
    "AppData",
];

/// Extensions whose contents are worth embedding: text, docs and source code.
const CONTENT_EXTS: &[&str] = &[
    "md", "markdown", "txt", "rst", "org", "tex", "csv", "tsv", "json", "yaml", "yml", "toml",
    "ini", "cfg", "xml", "html", "htm", "css", "js", "jsx", "ts", "tsx", "mjs", "cjs", "py", "rs",
    "go", "java", "kt", "c", "h", "cpp", "hpp", "cc", "cs", "rb", "php", "swift", "sh", "ps1",
    "bat", "sql", "lua", "r", "m", "scala", "dart", "vue", "svelte", "log",
];

/// True when the first 8 KiB contain a NUL byte.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// True when a file's contents are worth embedding: PDFs and text/doc/code
/// extensions. Other files are never embedded but stay glob/grep-able.
pub fn is_content_file(path: &Path) -> bool {
    if is_pdf(path) {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| CONTENT_EXTS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
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
