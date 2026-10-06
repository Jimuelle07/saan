//! saan core: local semantic search (EmbeddingGemma), grep and glob over a
//! directory tree, plus a typed router that can optionally defer to Jev.

pub mod chunk;
pub mod embed;
pub mod engine;
pub mod extract;
pub mod glob;
pub mod grep;
pub mod index;
pub mod jev;
pub mod router;

pub use engine::{Engine, Hit, SearchResponse};
pub use index::Index;
pub use router::{Mode, Route};

use std::path::Path;

/// Display path for a file under `root`: forward slashes, relative when possible.
/// Same-name files differ by their relative path, so this is what the UI shows.
pub fn rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}
