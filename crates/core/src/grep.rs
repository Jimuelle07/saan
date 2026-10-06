//! Regex content search over text files (ripgrep-style, line oriented).

use std::path::Path;

use anyhow::{Context, Result};
use regex::RegexBuilder;
use serde::Serialize;

use crate::extract::{looks_binary, walk_files};
use crate::rel_path;

const MAX_LINE_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GrepHit {
    pub rel: String,
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

/// Search every non-binary file under `root` for `pattern`.
/// Smart case: case-insensitive unless the pattern contains an uppercase letter.
pub fn grep(root: &Path, pattern: &str, limit: usize) -> Result<Vec<GrepHit>> {
    let smart_ci = !pattern.chars().any(char::is_uppercase);
    let re = RegexBuilder::new(pattern)
        .case_insensitive(smart_ci)
        .build()
        .with_context(|| format!("invalid regex `{pattern}`"))?;
    let mut hits = Vec::new();
    for path in walk_files(root) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if looks_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = rel_path(root, &path);
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                let text: String = line.trim().chars().take(MAX_LINE_CHARS).collect();
                hits.push(GrepHit { rel: rel.clone(), line: i + 1, text });
                if hits.len() >= limit {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}
