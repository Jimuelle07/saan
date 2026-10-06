//! Path pattern search. Patterns without `/` match the file name at any depth
//! (`todo.txt`, `*.pdf`); patterns with `/` match the root-relative path
//! (`work/**/README.md`). Matching is case-insensitive.

use std::path::Path;

use anyhow::{Context, Result};
use globset::GlobBuilder;

use crate::extract::walk_files;
use crate::rel_path;

pub fn glob(root: &Path, pattern: &str, limit: usize) -> Result<Vec<String>> {
    let pattern = pattern.trim().replace('\\', "/");
    let by_path = pattern.contains('/');
    let matcher = GlobBuilder::new(pattern.trim_start_matches("./"))
        .case_insensitive(true)
        .literal_separator(true)
        .build()
        .with_context(|| format!("invalid glob `{pattern}`"))?
        .compile_matcher();
    let mut out = Vec::new();
    for path in walk_files(root) {
        let rel = rel_path(root, &path);
        let subject = if by_path { rel.as_str() } else { rel.rsplit('/').next().unwrap_or(&rel) };
        if matcher.is_match(subject) {
            out.push(rel);
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}
