//! Multi-root file scope: which files are visible for indexing, grep and glob.
//!
//! A [`Scope`] owns the roots to search and the maximum file size that may be
//! read. Walking applies the usual ignore rules (`.gitignore`/`.ignore`, hidden
//! files) plus [`SKIP_DIRS`] pruning, and hides files larger than the limit so
//! they never reach the index, grep or glob results.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use ignore::WalkBuilder;

use crate::extract::SKIP_DIRS;

/// Default cap on file size: files larger than this are never indexed, grepped
/// or globbed.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// The roots to search and the largest file that may be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    pub roots: Vec<PathBuf>,
    pub max_file_bytes: u64,
}

/// One visible file: its `path` (root-joined), the `rel` path inside the root
/// it came from, and the metadata callers need to reuse index entries.
#[derive(Debug, Clone)]
pub struct ScopedFile {
    /// Index of the owning root in [`Scope::roots`].
    pub root: usize,
    pub path: PathBuf,
    /// Relative to its root, forward slashes.
    pub rel: String,
    pub size: u64,
    /// Unix seconds.
    pub mtime: u64,
}

impl Scope {
    /// One root with the default file-size cap.
    pub fn single(root: impl Into<PathBuf>) -> Self {
        Self::new(vec![root.into()], DEFAULT_MAX_FILE_BYTES)
    }

    pub fn new(roots: Vec<PathBuf>, max_file_bytes: u64) -> Self {
        Self { roots, max_file_bytes }
    }

    /// Label shown for `root` when several roots are in play: its last path
    /// component, or the drive (e.g. `C:`) when the root is a drive root.
    fn root_label(&self, root: usize) -> String {
        let Some(path) = self.roots.get(root) else { return String::new() };
        if let Some(name) = path.file_name() {
            return name.to_string_lossy().into_owned();
        }
        // `C:\` has no file name; fall back to the prefix (`C:`).
        let text = path.to_string_lossy().replace('\\', "/");
        let trimmed = text.trim_end_matches('/');
        if trimmed.is_empty() { text } else { trimmed.to_string() }
    }

    /// Single root: `rel` unchanged. Several roots: `"<label>/<rel>"`.
    pub fn display_rel(&self, root: usize, rel: &str) -> String {
        if self.roots.len() <= 1 {
            return rel.to_string();
        }
        let label = self.root_label(root);
        if rel.is_empty() { label } else { format!("{label}/{rel}") }
    }

    /// Every visible file under every root, sorted by (root, rel).
    ///
    /// Applies `.gitignore`/`.ignore`, hidden-file rules, [`SKIP_DIRS`] pruning
    /// and the [`Scope::max_file_bytes`] cap.
    pub fn files(&self) -> Vec<ScopedFile> {
        let mut files = Vec::new();
        for (root, path) in self.roots.iter().enumerate() {
            walk_root(root, path, self.max_file_bytes, &mut files);
        }
        files.sort_by(|a, b| a.root.cmp(&b.root).then_with(|| a.rel.cmp(&b.rel)));
        files
    }
}

/// Walk one root, appending every visible file to `out`.
fn walk_root(root: usize, path: &Path, max_file_bytes: u64, out: &mut Vec<ScopedFile>) {
    let root_path = path.to_path_buf();
    let filter_root = root_path.clone();
    let mut builder = WalkBuilder::new(path);
    builder.standard_filters(true).filter_entry(move |entry| {
        // Never prune a root itself, even one named `node_modules`.
        if entry.depth() == 0 || entry.path() == filter_root.as_path() {
            return true;
        }
        if !entry.file_type().is_some_and(|t| t.is_dir()) {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        !SKIP_DIRS.iter().any(|skip| name.eq_ignore_ascii_case(skip))
    });
    for entry in builder.build().filter_map(Result::ok) {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let size = meta.len();
        if size > max_file_bytes {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let file_path = entry.into_path();
        let rel = relative(&root_path, &file_path);
        out.push(ScopedFile { root, path: file_path, rel, size, mtime });
    }
}

/// Path of `path` inside `root`, forward slashes; `path` itself if it is not
/// actually below `root`.
fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}
