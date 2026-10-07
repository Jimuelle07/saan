//! On-disk vector index: `meta.json` (roots + files + chunks) and
//! `vectors.f32` (row-major little-endian f32, one L2-normalised row per chunk).
//!
//! An index covers every root of a [`Scope`]; files remember which root they
//! came from, so relative paths stay unique per root and the display path
//! (`<label>/<rel>` for several roots) is derived on demand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::chunk::chunk_ranges;
use crate::embed::{doc_prompt, Embedder};
use crate::extract::{extract_text, is_content_file};
use crate::scope::{Scope, ScopedFile};

/// Bump when the on-disk shape changes; older indexes fail to load and are rebuilt.
const FORMAT_VERSION: u32 = 2;
const EMBED_BATCH: usize = 8;
/// Characters of each chunk kept for result snippets.
const PREVIEW_CHARS: usize = 240;
/// Longest files contribute only their first this many chunks (~10 KB of text).
/// Embedding is the indexing bottleneck (~0.5 s per chunk on a laptop CPU); on a
/// real Documents + Desktop sample this cap keeps 69% of the chunks a 64-chunk
/// cap embedded, and every file still gets its opening sections indexed.
pub const MAX_CHUNKS_PER_FILE: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Index into [`Index::roots`].
    pub root: u16,
    /// Path relative to that root, forward slashes.
    pub rel: String,
    pub size: u64,
    pub mtime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkEntry {
    pub file: u32,
    pub preview: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    version: u32,
    roots: Vec<PathBuf>,
    max_file_bytes: u64,
    dim: usize,
    files: Vec<FileEntry>,
    chunks: Vec<ChunkEntry>,
}

pub struct Index {
    pub roots: Vec<PathBuf>,
    pub max_file_bytes: u64,
    pub dim: usize,
    pub files: Vec<FileEntry>,
    pub chunks: Vec<ChunkEntry>,
    vectors: Vec<f32>,
}

/// One file-level semantic match.
#[derive(Debug, Clone)]
pub struct FileScore {
    pub file: usize,
    pub score: f32,
    pub chunk: usize,
}

/// Progress reported to [`Index::build`]'s `on_event` callback.
pub enum BuildEvent<'a> {
    /// About to embed `done + 1` of `total` files that need (re)embedding.
    File { done: usize, total: usize, rel: &'a str },
    /// A valid, searchable snapshot: reused files + everything embedded so far.
    Checkpoint(&'a Index),
}

#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    /// Embedded files between [`BuildEvent::Checkpoint`] events; `0` = never.
    pub checkpoint_every: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BuildStats {
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_reused: usize,
    pub chunks: usize,
    pub cancelled: bool,
}

fn canonical_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots.iter().map(|r| r.canonicalize().unwrap_or_else(|_| r.clone())).collect()
}

impl Index {
    /// Build an index for every visible file of `scope`.
    ///
    /// Files that `previous` already indexed with the same absolute path, size
    /// and mtime are reused (chunks and vectors copied) instead of re-embedded.
    /// Only content files ([`is_content_file`]) whose text extracts are stored;
    /// everything else stays reachable through grep/glob, which walk the disk.
    ///
    /// `on_event` sees a [`BuildEvent::File`] before each embedded file and a
    /// [`BuildEvent::Checkpoint`] every `opts.checkpoint_every` embedded files;
    /// returning `false` cancels the build, yielding the partial index
    /// (reused + embedded so far) with `stats.cancelled = true`.
    pub fn build(
        scope: &Scope,
        embedder: &Embedder,
        previous: Option<&Index>,
        opts: &BuildOptions,
        mut on_event: impl FnMut(BuildEvent) -> bool,
    ) -> Result<(Index, BuildStats)> {
        let dim = embedder.dim();
        let files = scope.files();
        let mut stats = BuildStats { files_seen: files.len(), ..Default::default() };

        // Reuse lookup: absolute path -> entry in `previous`, plus where each of
        // its files' chunks live (they are stored grouped, in file order).
        let mut reuse_paths: HashMap<PathBuf, usize> = HashMap::new();
        let mut reuse_chunks: HashMap<usize, (usize, usize)> = HashMap::new();
        let reusable = previous.filter(|p| p.dim == dim);
        if let Some(prev) = reusable {
            reuse_paths = (0..prev.files.len()).map(|i| (prev.abs_path(i), i)).collect();
            for (ci, chunk) in prev.chunks.iter().enumerate() {
                reuse_chunks.entry(chunk.file as usize).and_modify(|s| s.1 = ci + 1).or_insert((ci, ci + 1));
            }
        }

        let mut index = Index {
            roots: canonical_roots(&scope.roots),
            max_file_bytes: scope.max_file_bytes,
            dim,
            files: Vec::new(),
            chunks: Vec::new(),
            vectors: Vec::new(),
        };

        // Pass 1: reuse what `previous` covers (pushed first, so every
        // checkpoint is a valid index) and extract the rest.
        let mut to_embed: Vec<(ScopedFile, String)> = Vec::new();
        for file in files {
            let abs = index.roots[file.root].join(&file.rel);
            if let (Some(prev), Some(&old)) = (reusable, reuse_paths.get(&abs)) {
                let entry = &prev.files[old];
                if entry.size == file.size && entry.mtime == file.mtime {
                    let id = index.files.len() as u32;
                    if let Some(&(start, end)) = reuse_chunks.get(&old) {
                        for ci in start..end {
                            index.chunks.push(ChunkEntry { file: id, preview: prev.chunks[ci].preview.clone() });
                            index.vectors.extend_from_slice(prev.vector(ci));
                        }
                    }
                    index.files.push(FileEntry {
                        root: file.root as u16,
                        rel: file.rel,
                        size: file.size,
                        mtime: file.mtime,
                    });
                    stats.files_reused += 1;
                    continue;
                }
            }
            if !is_content_file(&file.path) {
                continue;
            }
            let Some(text) = extract_text(&file.path) else { continue };
            to_embed.push((file, text));
        }

        // Pass 2: embed, reporting progress and checkpoints while we go.
        let total = to_embed.len();
        for (done, (file, text)) in to_embed.into_iter().enumerate() {
            let rel = scope.display_rel(file.root, &file.rel);
            if !on_event(BuildEvent::File { done, total, rel: &rel }) {
                stats.cancelled = true;
                break;
            }
            let id = index.files.len() as u32;
            let title = file.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let ranges = chunk_ranges(&text);
            let ranges = &ranges[..ranges.len().min(MAX_CHUNKS_PER_FILE)];
            for batch in ranges.chunks(EMBED_BATCH) {
                let prompts: Vec<String> =
                    batch.iter().map(|&(s, e)| doc_prompt(&format!("{rel} ({title})"), &text[s..e])).collect();
                for (vec, &(s, e)) in embedder.embed_docs(&prompts)?.into_iter().zip(batch) {
                    index.vectors.extend_from_slice(&vec);
                    let preview: String = text[s..e].split_whitespace().collect::<Vec<_>>().join(" ");
                    let preview: String = preview.chars().take(PREVIEW_CHARS).collect();
                    index.chunks.push(ChunkEntry { file: id, preview });
                }
            }
            index.files.push(FileEntry {
                root: file.root as u16,
                rel: file.rel,
                size: file.size,
                mtime: file.mtime,
            });
            stats.files_indexed += 1;
            if opts.checkpoint_every > 0
                && (done + 1) % opts.checkpoint_every == 0
                && !on_event(BuildEvent::Checkpoint(&index))
            {
                stats.cancelled = true;
                break;
            }
        }
        stats.chunks = index.chunks.len();
        Ok((index, stats))
    }

    fn vector(&self, chunk: usize) -> &[f32] {
        &self.vectors[chunk * self.dim..(chunk + 1) * self.dim]
    }

    /// The scope this index was built from, for grep/glob walks over the disk.
    pub fn scope(&self) -> Scope {
        Scope::new(self.roots.clone(), self.max_file_bytes)
    }

    pub fn abs_path(&self, file: usize) -> PathBuf {
        let entry = &self.files[file];
        self.roots[entry.root as usize].join(&entry.rel)
    }

    /// Path shown for `file`: `<rel>` for one root, `<label>/<rel>` for several.
    pub fn display_rel(&self, file: usize) -> String {
        let entry = &self.files[file];
        self.scope().display_rel(entry.root as usize, &entry.rel)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let meta = Meta {
            version: FORMAT_VERSION,
            roots: self.roots.clone(),
            max_file_bytes: self.max_file_bytes,
            dim: self.dim,
            files: self.files.clone(),
            chunks: self.chunks.clone(),
        };
        std::fs::write(dir.join("meta.json"), serde_json::to_vec(&meta)?)?;
        let bytes: Vec<u8> = self.vectors.iter().flat_map(|f| f.to_le_bytes()).collect();
        std::fs::write(dir.join("vectors.f32"), bytes)?;
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let meta: Meta = serde_json::from_slice(
            &std::fs::read(dir.join("meta.json")).with_context(|| format!("no index in {}", dir.display()))?,
        )?;
        if meta.version != FORMAT_VERSION {
            bail!("index format {} is not supported; re-run `saan index`", meta.version);
        }
        if meta.roots.is_empty() {
            bail!("index in {} is corrupt (no roots)", dir.display());
        }
        let bytes = std::fs::read(dir.join("vectors.f32"))?;
        let vectors: Vec<f32> =
            bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        if vectors.len() != meta.chunks.len() * meta.dim {
            bail!("index in {} is corrupt (vector count mismatch)", dir.display());
        }
        Ok(Index {
            roots: meta.roots,
            max_file_bytes: meta.max_file_bytes,
            dim: meta.dim,
            files: meta.files,
            chunks: meta.chunks,
            vectors,
        })
    }

    /// Top `k` files by best-chunk cosine similarity to `query` (normalised).
    pub fn search(&self, query: &[f32], k: usize) -> Vec<FileScore> {
        let mut best: Vec<Option<FileScore>> = vec![None; self.files.len()];
        for (ci, row) in self.vectors.chunks_exact(self.dim).enumerate() {
            let score: f32 = row.iter().zip(query).map(|(a, b)| a * b).sum();
            let file = self.chunks[ci].file as usize;
            if best[file].as_ref().is_none_or(|b| score > b.score) {
                best[file] = Some(FileScore { file, score, chunk: ci });
            }
        }
        let mut hits: Vec<FileScore> = best.into_iter().flatten().collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(k);
        hits
    }
}
