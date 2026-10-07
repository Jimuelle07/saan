//! EmbeddingGemma-300m via ONNX Runtime, fully on-device.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use ndarray::Array2;
use ort::session::{builder::GraphOptimizationLevel, OutputSelector, RunOptions, Session};
use ort::value::TensorRef;
use tokenizers::Tokenizer;

/// Folder name under `models/` that holds `tokenizer.json` and `onnx/<MODEL_FILE>`.
pub const MODEL_DIR_NAME: &str = "embeddinggemma-300m";
/// Hugging Face repo the model files come from (public, ungated).
pub const MODEL_REPO: &str = "onnx-community/embeddinggemma-300m-ONNX";
/// 4-bit weights (~197 MB). Measured on an i5-13420H CPU: ~3x faster per query
/// than the int8 `model_quantized.onnx` and ~6x faster than `model_no_gather_q4.onnx`.
pub const MODEL_FILE: &str = "model_q4.onnx";
/// Files fetched by `saan fetch-model`.
pub const MODEL_FILES: &[&str] = &["tokenizer.json", "config.json", "onnx/model_q4.onnx", "onnx/model_q4.onnx_data"];

/// Token cap per document chunk; chunks are ~1200 chars so this rarely truncates.
const MAX_DOC_TOKENS: usize = 512;
const MAX_QUERY_TOKENS: usize = 128;

/// EmbeddingGemma task prompts (see the model card).
const QUERY_PREFIX: &str = "task: search result | query: ";

pub fn doc_prompt(title: &str, text: &str) -> String {
    format!("title: {title} | text: {text}")
}

/// Locate the model folder: `$SAAN_MODEL_DIR`, then `models/<name>` in the
/// current dir or any ancestor of the current dir / executable.
pub fn find_model_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SAAN_MODEL_DIR") {
        return Some(PathBuf::from(dir));
    }
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
        }
    }
    starts.iter().find_map(|start| {
        start
            .ancestors()
            .map(|a| a.join("models").join(MODEL_DIR_NAME))
            .find(|p| p.join("tokenizer.json").is_file())
    })
}

pub struct Embedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    dim: usize,
}

impl Embedder {
    pub fn load(model_dir: &Path) -> Result<Self> {
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| anyhow!("loading tokenizer: {e}"))?;
        let threads = std::env::var("SAAN_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get().min(8)));
        let ort_err = |e: ort::Error<_>| anyhow!("{e}");
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_err)?
            .with_intra_threads(threads)
            .map_err(ort_err)?
            .commit_from_file(model_dir.join("onnx").join(MODEL_FILE))
            .with_context(|| format!("loading ONNX model from {}", model_dir.display()))?;
        let mut me = Self { session: Mutex::new(session), tokenizer, dim: 0 };
        me.dim = me.embed_query("probe")?.len();
        Ok(me)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let text = format!("{QUERY_PREFIX}{query}");
        Ok(self.embed(&[text], MAX_QUERY_TOKENS)?.pop().expect("one input"))
    }

    pub fn embed_docs(&self, docs: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed(docs, MAX_DOC_TOKENS)
    }

    fn embed(&self, texts: &[String], max_tokens: usize) -> Result<Vec<Vec<f32>>> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| anyhow!("tokenizing: {e}"))?;
        let seq = encodings.iter().map(|e| e.len().min(max_tokens)).max().unwrap_or(1).max(1);
        let mut ids = Array2::<i64>::zeros((texts.len(), seq));
        let mut mask = Array2::<i64>::zeros((texts.len(), seq));
        for (row, enc) in encodings.iter().enumerate() {
            for (col, &id) in enc.get_ids().iter().take(seq).enumerate() {
                ids[[row, col]] = i64::from(id);
                mask[[row, col]] = 1;
            }
        }
        let mut session = self.session.lock().expect("embedder session poisoned");
        // Ask ONNX Runtime for only `sentence_embedding` so the large
        // `last_hidden_state` tensor is never materialised. Built per call:
        // rc.13 only marks `RunOptions<NoSelectedOutputs>` as `Sync`, so a
        // `RunOptions<HasSelectedOutputs>` field would make `Embedder` !Sync
        // (it is shared via `Arc<RwLock<Option<Engine>>>` in the Tauri app).
        let run_options =
            RunOptions::new()?.with_outputs(OutputSelector::no_default().with("sentence_embedding"));
        let outputs = session.run_with_options(
            ort::inputs![
                "input_ids" => TensorRef::from_array_view(&ids)?,
                "attention_mask" => TensorRef::from_array_view(&mask)?,
            ],
            &run_options,
        )?;
        let emb = outputs["sentence_embedding"].try_extract_array::<f32>()?;
        let emb = emb.into_dimensionality::<ndarray::Ix2>()?;
        Ok(emb.outer_iter().map(|row| normalize(row.to_vec())).collect())
    }
}

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}
