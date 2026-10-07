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
pub mod scope;

pub use engine::{Engine, Hit, SearchResponse};
pub use index::Index;
pub use router::{Mode, Route};
pub use scope::Scope;
