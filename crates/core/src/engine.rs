//! Ties routing, semantic search, grep and glob together.

use std::time::Instant;

use anyhow::Result;
use serde::Serialize;

use crate::embed::Embedder;
use crate::index::Index;
use crate::jev::{Candidate, JevClient};
use crate::router::{local_route, Mode, Route, RouteSource, CONFIDENT};

/// Ask Jev to pick between the top two semantic hits when they score this close.
const PICK_MARGIN: f32 = 0.02;
const PICK_CANDIDATES: usize = 5;
const GREP_HIT_LIMIT: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    /// Root-relative path with forward slashes; unique per file.
    pub rel: String,
    /// Absolute path to open.
    pub path: String,
    pub name: String,
    pub score: f32,
    /// 1-based line for grep hits.
    pub line: Option<usize>,
    pub snippet: String,
    /// Another hit in this response has the same file name.
    pub same_name: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub route: Route,
    pub hits: Vec<Hit>,
    /// Number of requests made to Jev for this search.
    pub jev_calls: u32,
    pub elapsed_ms: f64,
}

pub struct Engine {
    pub index: Index,
    embedder: Embedder,
    jev: Option<JevClient>,
}

fn file_name(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

fn mark_same_names(hits: &mut [Hit]) {
    for i in 0..hits.len() {
        let name = hits[i].name.to_lowercase();
        hits[i].same_name = hits.iter().enumerate().any(|(j, h)| j != i && h.name.to_lowercase() == name);
    }
}

impl Engine {
    pub fn new(index: Index, embedder: Embedder, jev: Option<JevClient>) -> Self {
        Self { index, embedder, jev }
    }

    pub fn jev_enabled(&self) -> bool {
        self.jev.is_some()
    }

    pub fn search(&self, input: &str, k: usize) -> Result<SearchResponse> {
        let start = Instant::now();
        let mut jev_calls = 0;
        let mut route = local_route(input);
        if route.confidence < CONFIDENT {
            if let Some(jev) = &self.jev {
                jev_calls += 1;
                if let Ok(mode) = jev.route(input.trim()) {
                    if mode != route.mode {
                        let raw = input.trim();
                        let query = if mode == Mode::Grep { regex::escape(raw) } else { raw.to_string() };
                        route = Route { mode, query, confidence: route.confidence, source: RouteSource::Jev };
                    } else {
                        route.source = RouteSource::Jev;
                    }
                }
            }
        }

        let mut hits = match route.mode {
            Mode::Semantic => self.semantic(&route.query, k)?,
            Mode::Grep => self.grep(&route.query, k)?,
            Mode::Glob => self.glob(&route.query, k)?,
        };
        // A guessed exact search that finds nothing falls back to meaning.
        if hits.is_empty() && route.mode != Mode::Semantic && route.source != RouteSource::Explicit {
            route = Route { mode: Mode::Semantic, query: input.trim().to_string(), ..route };
            hits = self.semantic(&route.query, k)?;
        }

        mark_same_names(&mut hits);
        if route.mode == Mode::Semantic && hits.len() >= 2 {
            let ambiguous = hits[0].score - hits[1].score < PICK_MARGIN || hits.iter().take(PICK_CANDIDATES).any(|h| h.same_name);
            if let (true, Some(jev)) = (ambiguous, &self.jev) {
                let candidates: Vec<Candidate> = hits
                    .iter()
                    .take(PICK_CANDIDATES)
                    .map(|h| Candidate { rel: h.rel.clone(), snippet: h.snippet.clone() })
                    .collect();
                if jev.pick_request(&route.query, &candidates).is_some() {
                    jev_calls += 1;
                    if let Ok(Some(i)) = jev.pick(&route.query, &candidates) {
                        let picked = hits.remove(i);
                        hits.insert(0, picked);
                    }
                }
            }
        }
        Ok(SearchResponse { route, hits, jev_calls, elapsed_ms: start.elapsed().as_secs_f64() * 1000.0 })
    }

    pub fn semantic(&self, query: &str, k: usize) -> Result<Vec<Hit>> {
        let qv = self.embedder.embed_query(query)?;
        Ok(self
            .index
            .search(&qv, k)
            .into_iter()
            .map(|s| {
                let rel = self.index.files[s.file].rel.clone();
                Hit {
                    name: file_name(&rel),
                    path: self.index.abs_path(s.file).to_string_lossy().into_owned(),
                    rel,
                    score: s.score,
                    line: None,
                    snippet: self.index.chunks[s.chunk].preview.clone(),
                    same_name: false,
                }
            })
            .collect())
    }

    fn grep(&self, pattern: &str, k: usize) -> Result<Vec<Hit>> {
        let root = &self.index.root;
        let mut hits: Vec<Hit> = Vec::new();
        for g in crate::grep::grep(root, pattern, GREP_HIT_LIMIT)? {
            // One row per file (first matching line); results stay file-oriented.
            if hits.last().is_some_and(|h| h.rel == g.rel) {
                continue;
            }
            hits.push(Hit {
                name: file_name(&g.rel),
                path: root.join(&g.rel).to_string_lossy().into_owned(),
                rel: g.rel,
                score: 1.0,
                line: Some(g.line),
                snippet: g.text,
                same_name: false,
            });
            if hits.len() >= k {
                break;
            }
        }
        Ok(hits)
    }

    fn glob(&self, pattern: &str, k: usize) -> Result<Vec<Hit>> {
        let root = &self.index.root;
        Ok(crate::glob::glob(root, pattern, k)?
            .into_iter()
            .map(|rel| Hit {
                name: file_name(&rel),
                path: root.join(&rel).to_string_lossy().into_owned(),
                rel,
                score: 1.0,
                line: None,
                snippet: String::new(),
                same_name: false,
            })
            .collect())
    }
}
