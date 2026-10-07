//! Regex content search over text files (ripgrep-style, line oriented).

use std::path::PathBuf;

use anyhow::{Context, Result};
use regex::RegexBuilder;
use serde::Serialize;

use crate::extract::looks_binary;
use crate::scope::Scope;

const MAX_LINE_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GrepHit {
    /// Display path: the root-relative path, prefixed with the root's label when
    /// the scope has several roots.
    pub rel: String,
    /// Path of the file on disk (root-joined, so absolute for absolute roots).
    pub path: PathBuf,
    /// 1-based line number.
    pub line: usize,
    pub text: String,
}

/// Search every visible non-binary file in `scope` for `pattern`.
/// Smart case: case-insensitive unless the pattern contains an uppercase letter.
pub fn grep(scope: &Scope, pattern: &str, limit: usize) -> Result<Vec<GrepHit>> {
    let smart_ci = !pattern.chars().any(char::is_uppercase);
    let re = RegexBuilder::new(pattern)
        .case_insensitive(smart_ci)
        .build()
        .with_context(|| format!("invalid regex `{pattern}`"))?;
    let mut hits = Vec::new();
    for file in scope.files() {
        let Ok(bytes) = std::fs::read(&file.path) else { continue };
        if looks_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = scope.display_rel(file.root, &file.rel);
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                let text: String = line.trim().chars().take(MAX_LINE_CHARS).collect();
                hits.push(GrepHit { rel: rel.clone(), path: file.path.clone(), line: i + 1, text });
                if hits.len() >= limit {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}
