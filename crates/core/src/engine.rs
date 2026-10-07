//! Ties routing, semantic search, grep and glob together.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

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
    /// File size in bytes; `0` when unknown.
    pub size: u64,
    /// Last modification time in unix seconds; `0` when unknown.
    pub modified: u64,
    /// Lowercase extension without the dot; `""` when there is none.
    pub ext: String,
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
    /// Where the model lives; the ONNX session is opened lazily.
    model_dir: PathBuf,
    /// The embedder, loaded on demand and dropped again when it goes idle.
    embedder: Mutex<Option<Arc<Embedder>>>,
    /// When the embedder was last needed (load or query), for idle unloading.
    last_use: Mutex<Instant>,
    jev: Option<JevClient>,
}

fn file_name(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

/// Lowercase extension of a root-relative path, without the dot.
fn file_ext(rel: &str) -> String {
    let name = file_name(rel);
    match name.rsplit_once('.') {
        // A leading dot means a dotfile (`.gitignore`), not an extension.
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => ext.to_lowercase(),
        _ => String::new(),
    }
}

/// Size and mtime (unix seconds) of `path`; `(0, 0)` when it cannot be read.
fn file_meta(path: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::metadata(path) else { return (0, 0) };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    (meta.len(), modified)
}

fn mark_same_names(hits: &mut [Hit]) {
    for i in 0..hits.len() {
        let name = hits[i].name.to_lowercase();
        hits[i].same_name = hits.iter().enumerate().any(|(j, h)| j != i && h.name.to_lowercase() == name);
    }
}

impl Engine {
    /// Wrap `index`. The model at `model_dir` is NOT loaded here: the first
    /// semantic query loads it, [`Engine::preload`] loads it eagerly and
    /// [`Engine::release_if_idle`] drops it again once search goes quiet.
    pub fn new(index: Index, model_dir: PathBuf, jev: Option<JevClient>) -> Self {
        Self { index, model_dir, embedder: Mutex::new(None), last_use: Mutex::new(Instant::now()), jev }
    }

    pub fn jev_enabled(&self) -> bool {
        self.jev.is_some()
    }

    /// True while the ONNX session is resident in memory.
    pub fn model_loaded(&self) -> bool {
        self.embedder.lock().expect("embedder lock").is_some()
    }

    /// Load the model now instead of waiting for the first semantic query.
    pub fn preload(&self) -> Result<()> {
        self.embedder().map(drop)
    }

    /// Drop the model once it has been unused for `idle`; true when unloaded.
    pub fn release_if_idle(&self, idle: Duration) -> bool {
        if self.last_use.lock().expect("last use lock").elapsed() < idle {
            return false;
        }
        self.embedder.lock().expect("embedder lock").take().is_some()
    }

    /// The embedder, loading it on demand. The `Arc` outlives the lock, so
    /// inference never runs with the mutex held (and an unload mid-query only
    /// drops the cached handle, not the session being used).
    fn embedder(&self) -> Result<Arc<Embedder>> {
        let mut slot = self.embedder.lock().expect("embedder lock");
        if slot.is_none() {
            *slot = Some(Arc::new(Embedder::load(&self.model_dir)?));
        }
        let embedder = Arc::clone(slot.as_ref().expect("embedder was just loaded"));
        drop(slot);
        *self.last_use.lock().expect("last use lock") = Instant::now();
        Ok(embedder)
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

    /// Meaning search; loads the model on the first call.
    pub fn semantic(&self, query: &str, k: usize) -> Result<Vec<Hit>> {
        let embedder = self.embedder()?;
        let qv = embedder.embed_query(query)?;
        Ok(self
            .index
            .search(&qv, k)
            .into_iter()
            .map(|s| {
                let f = &self.index.files[s.file];
                Hit {
                    name: file_name(&f.rel),
                    path: self.index.abs_path(s.file).to_string_lossy().into_owned(),
                    rel: f.rel.clone(),
                    score: s.score,
                    line: None,
                    snippet: self.index.chunks[s.chunk].preview.clone(),
                    same_name: false,
                    size: f.size,
                    modified: f.mtime,
                    ext: file_ext(&f.rel),
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
            let path = root.join(&g.rel);
            let (size, modified) = file_meta(&path);
            hits.push(Hit {
                name: file_name(&g.rel),
                path: path.to_string_lossy().into_owned(),
                score: 1.0,
                line: Some(g.line),
                snippet: g.text,
                same_name: false,
                size,
                modified,
                ext: file_ext(&g.rel),
                rel: g.rel,
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
            .map(|rel| {
                let path = root.join(&rel);
                let (size, modified) = file_meta(&path);
                Hit {
                    name: file_name(&rel),
                    path: path.to_string_lossy().into_owned(),
                    size,
                    modified,
                    ext: file_ext(&rel),
                    rel,
                    score: 1.0,
                    line: None,
                    snippet: String::new(),
                    same_name: false,
                }
            })
            .collect())
    }
}
