//! Path pattern search. Patterns without `/` match the file name at any depth
//! (`todo.txt`, `*.pdf`); patterns with `/` match the root-relative path or the
//! displayed path (`work/**/README.md`). Matching is case-insensitive.

use std::path::PathBuf;

use anyhow::{Context, Result};
use globset::GlobBuilder;
use serde::Serialize;

use crate::scope::Scope;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GlobHit {
    /// Display path: the root-relative path, prefixed with the root's label when
    /// the scope has several roots.
    pub rel: String,
    /// Path of the file on disk (root-joined, so absolute for absolute roots).
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64,
}

/// Match every visible file in `scope` against `pattern`.
pub fn glob(scope: &Scope, pattern: &str, limit: usize) -> Result<Vec<GlobHit>> {
    let pattern = pattern.trim().replace('\\', "/");
    let by_path = pattern.contains('/');
    let matcher = GlobBuilder::new(pattern.trim_start_matches("./"))
        .case_insensitive(true)
        .literal_separator(true)
        .build()
        .with_context(|| format!("invalid glob `{pattern}`"))?
        .compile_matcher();
    let mut out = Vec::new();
    for file in scope.files() {
        let rel = scope.display_rel(file.root, &file.rel);
        let matched = if by_path {
            // A path pattern may be written either against the per-root rel or
            // against the displayed (root-labelled) path.
            matcher.is_match(file.rel.as_str()) || matcher.is_match(&rel)
        } else {
            let name = file.rel.rsplit('/').next().unwrap_or(&file.rel);
            matcher.is_match(name)
        };
        if matched {
            out.push(GlobHit { rel, path: file.path, size: file.size, mtime: file.mtime });
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}
