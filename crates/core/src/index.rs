//! On-disk vector index: `meta.json` (files + chunks) and `vectors.f32`
//! (row-major little-endian f32, one L2-normalised row per chunk).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::chunk::chunk_ranges;
use crate::embed::{doc_prompt, Embedder};
use crate::extract::{extract_text, walk_files};
use crate::rel_path;

const FORMAT_VERSION: u32 = 1;
const EMBED_BATCH: usize = 8;
/// Characters of each chunk kept for result snippets.
const PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path relative to the index root, forward slashes.
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
    root: PathBuf,
    dim: usize,
    files: Vec<FileEntry>,
    chunks: Vec<ChunkEntry>,
}

pub struct Index {
    pub root: PathBuf,
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

#[derive(Debug, Default, Clone, Copy)]
pub struct BuildStats {
    pub files_seen: usize,
    pub files_indexed: usize,
    pub files_reused: usize,
    pub chunks: usize,
}

fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((meta.len(), mtime))
}

impl Index {
    /// Index every supported file under `root`. Unchanged files (same size and
    /// mtime) are copied from `previous` instead of being re-embedded.
    pub fn build(
        root: &Path,
        embedder: &Embedder,
        previous: Option<&Index>,
        mut progress: impl FnMut(&str),
    ) -> Result<(Self, BuildStats)> {
        let root = root.canonicalize().with_context(|| format!("resolving {}", root.display()))?;
        let reuse: HashMap<&str, usize> = previous
            .filter(|p| p.root == root && p.dim == embedder.dim())
            .map(|p| p.files.iter().enumerate().map(|(i, f)| (f.rel.as_str(), i)).collect())
            .unwrap_or_default();

        let dim = embedder.dim();
        let mut index = Index { root: root.clone(), dim, files: Vec::new(), chunks: Vec::new(), vectors: Vec::new() };
        let mut stats = BuildStats::default();

        for path in walk_files(&root) {
            stats.files_seen += 1;
            let Some((size, mtime)) = file_stamp(&path) else { continue };
            let rel = rel_path(&root, &path);
            let file_id = index.files.len() as u32;

            if let (Some(&old), Some(prev)) = (reuse.get(rel.as_str()), previous) {
                let f = &prev.files[old];
                if f.size == size && f.mtime == mtime {
                    for (ci, c) in prev.chunks.iter().enumerate().filter(|(_, c)| c.file as usize == old) {
                        index.chunks.push(ChunkEntry { file: file_id, preview: c.preview.clone() });
                        index.vectors.extend_from_slice(prev.vector(ci));
                    }
                    index.files.push(FileEntry { rel, size, mtime });
                    stats.files_reused += 1;
                    continue;
                }
            }

            let Some(text) = extract_text(&path) else { continue };
            progress(&rel);
            let title = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let ranges = chunk_ranges(&text);
            for batch in ranges.chunks(EMBED_BATCH) {
                let prompts: Vec<String> =
                    batch.iter().map(|&(s, e)| doc_prompt(&format!("{rel} ({title})"), &text[s..e])).collect();
                for (vec, &(s, e)) in embedder.embed_docs(&prompts)?.into_iter().zip(batch) {
                    index.vectors.extend_from_slice(&vec);
                    let preview: String = text[s..e].split_whitespace().collect::<Vec<_>>().join(" ");
                    let preview: String = preview.chars().take(PREVIEW_CHARS).collect();
                    index.chunks.push(ChunkEntry { file: file_id, preview });
                }
            }
            index.files.push(FileEntry { rel, size, mtime });
            stats.files_indexed += 1;
        }
        stats.chunks = index.chunks.len();
        Ok((index, stats))
    }

    fn vector(&self, chunk: usize) -> &[f32] {
        &self.vectors[chunk * self.dim..(chunk + 1) * self.dim]
    }

    pub fn abs_path(&self, file: usize) -> PathBuf {
        self.root.join(&self.files[file].rel)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let meta = Meta {
            version: FORMAT_VERSION,
            root: self.root.clone(),
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
        let bytes = std::fs::read(dir.join("vectors.f32"))?;
        let vectors: Vec<f32> =
            bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        if vectors.len() != meta.chunks.len() * meta.dim {
            bail!("index in {} is corrupt (vector count mismatch)", dir.display());
        }
        Ok(Index { root: meta.root, dim: meta.dim, files: meta.files, chunks: meta.chunks, vectors })
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
