use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use saan_core::embed::{find_model_dir, Embedder, MODEL_DIR_NAME, MODEL_FILES, MODEL_REPO};
use saan_core::jev::JevClient;
use saan_core::{Engine, Index};
use serde::Deserialize;

/// saan ("where?"): local semantic file search, grep and glob.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Index directory (default: $SAAN_INDEX_DIR or ./.saan/index).
    #[arg(long, global = true)]
    index: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download EmbeddingGemma (ONNX, int8) into ./models.
    FetchModel,
    /// Embed every supported file under ROOT (unchanged files are reused).
    Index { root: PathBuf },
    /// Routed search: semantic by default; `grep:`, `glob:`, `/regex/` prefixes force a mode.
    Search {
        query: Vec<String>,
        #[arg(short, default_value_t = 10)]
        k: usize,
        #[arg(long)]
        json: bool,
    },
    /// Regex search inside files under ROOT.
    Grep {
        pattern: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// File name / path pattern search under ROOT.
    Glob {
        pattern: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Top-k hit rate on an eval file of {query, expected} pairs; fails below --min.
    Eval {
        file: PathBuf,
        #[arg(short, default_value_t = 5)]
        k: usize,
        #[arg(long, default_value_t = 0.8)]
        min: f64,
    },
    /// Warm query latency over the eval queries; fails if p95 exceeds --max-ms.
    Bench {
        file: PathBuf,
        #[arg(long, default_value_t = 5)]
        runs: usize,
        #[arg(long, default_value_t = 100.0)]
        max_ms: f64,
    },
}

#[derive(Deserialize)]
struct EvalCase {
    query: String,
    expected: String,
}

fn index_dir(cli: &Cli) -> PathBuf {
    cli.index
        .clone()
        .or_else(|| std::env::var_os("SAAN_INDEX_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(".saan").join("index"))
}

fn load_embedder() -> Result<Embedder> {
    let dir = find_model_dir().context("EmbeddingGemma not found; run `saan fetch-model` or set SAAN_MODEL_DIR")?;
    Embedder::load(&dir)
}

/// Engine for eval/bench: always local-only so results are reproducible.
fn local_engine(cli: &Cli) -> Result<Engine> {
    Ok(Engine::new(Index::load(&index_dir(cli))?, load_embedder()?, None))
}

fn load_cases(file: &Path) -> Result<Vec<EvalCase>> {
    let cases: Vec<EvalCase> = serde_json::from_slice(&std::fs::read(file)?)?;
    if cases.is_empty() {
        bail!("{} has no cases", file.display());
    }
    Ok(cases)
}

fn fetch_model() -> Result<()> {
    let dest = std::env::var_os("SAAN_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("models").join(MODEL_DIR_NAME));
    for file in MODEL_FILES {
        let path = dest.join(file);
        if path.is_file() {
            println!("ok       {file}");
            continue;
        }
        std::fs::create_dir_all(path.parent().expect("file has parent"))?;
        let url = format!("https://huggingface.co/{MODEL_REPO}/resolve/main/{file}");
        println!("fetching {file}");
        let mut body = ureq::get(&url).call()?.into_body();
        let tmp = path.with_extension("part");
        std::io::copy(&mut body.as_reader(), &mut std::fs::File::create(&tmp)?)?;
        std::fs::rename(tmp, &path)?;
    }
    println!("model ready in {}", dest.display());
    Ok(())
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn run(cli: &Cli) -> Result<bool> {
    match &cli.cmd {
        Cmd::FetchModel => fetch_model()?,
        Cmd::Index { root } => {
            let embedder = load_embedder()?;
            let dir = index_dir(cli);
            let previous = Index::load(&dir).ok();
            let start = Instant::now();
            let (index, stats) = Index::build(root, &embedder, previous.as_ref(), |rel| eprintln!("embed  {rel}"))?;
            index.save(&dir)?;
            println!(
                "indexed {} files ({} re-embedded, {} reused, {} chunks) in {:.1}s -> {}",
                index.files.len(),
                stats.files_indexed,
                stats.files_reused,
                stats.chunks,
                start.elapsed().as_secs_f64(),
                dir.display()
            );
        }
        Cmd::Search { query, k, json } => {
            let engine = Engine::new(Index::load(&index_dir(cli))?, load_embedder()?, JevClient::from_env());
            let res = engine.search(&query.join(" "), *k)?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!("[{} via {:?}] {:.1} ms", res.route.mode.as_str(), res.route.source, res.elapsed_ms);
                for h in &res.hits {
                    let line = h.line.map(|l| format!(":{l}")).unwrap_or_default();
                    println!("{:>6.3}  {}{line}", h.score, h.rel);
                }
            }
        }
        Cmd::Grep { pattern, root } => {
            for h in saan_core::grep::grep(root, pattern, usize::MAX)? {
                println!("{}:{}: {}", h.rel, h.line, h.text);
            }
        }
        Cmd::Glob { pattern, root } => {
            for rel in saan_core::glob::glob(root, pattern, usize::MAX)? {
                println!("{rel}");
            }
        }
        Cmd::Eval { file, k, min } => {
            let engine = local_engine(cli)?;
            let cases = load_cases(file)?;
            let mut passed = 0;
            for case in &cases {
                let res = engine.search(&case.query, *k)?;
                let rank = res.hits.iter().position(|h| h.rel == case.expected);
                if rank.is_some() {
                    passed += 1;
                }
                let mark = rank.map_or("MISS".to_string(), |r| format!("#{}", r + 1));
                let top = res.hits.first().map_or("-", |h| h.rel.as_str());
                println!("{mark:>5}  {:<40}  expected {:<36} top {top}", case.query, case.expected);
            }
            let rate = passed as f64 / cases.len() as f64;
            println!("top-{k} hit rate: {passed}/{} = {:.1}% (min {:.1}%)", cases.len(), rate * 100.0, min * 100.0);
            return Ok(rate >= *min);
        }
        Cmd::Bench { file, runs, max_ms } => {
            let engine = local_engine(cli)?;
            let cases = load_cases(file)?;
            for case in &cases {
                engine.search(&case.query, 10)?; // warm-up
            }
            let mut times = Vec::with_capacity(cases.len() * runs);
            for _ in 0..*runs {
                for case in &cases {
                    let t = Instant::now();
                    engine.search(&case.query, 10)?;
                    times.push(t.elapsed().as_secs_f64() * 1000.0);
                }
            }
            times.sort_by(f64::total_cmp);
            let p95 = percentile(&times, 95.0);
            println!(
                "{} warm queries over {} files: p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms (limit p95 {max_ms} ms)",
                times.len(),
                engine.index.files.len(),
                percentile(&times, 50.0),
                p95,
                times.last().copied().unwrap_or_default()
            );
            return Ok(p95 < *max_ms);
        }
    }
    Ok(true)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
