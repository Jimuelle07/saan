//! Integration tests for [`saan_core::Engine`]: routing, semantic search, the
//! guessed-search fallback, incremental reindexing and build cancellation, over
//! a small copy of `fixtures/corpus`.
//!
//! Every test needs the EmbeddingGemma model (`models/embeddinggemma-300m`).
//! When [`saan_core::embed::find_model_dir`] finds nothing the test prints
//! `skipping: model not found` and passes, so the suite is green on machines
//! without the (gitignored, ~200 MB) model. Each test builds its index with its
//! own throwaway `Embedder`; the `Engine` under test loads the model lazily and
//! is handed only the model directory. That is why there are only a handful of
//! tests and why each one indexes a fixed subset rather than the whole corpus.
//!
//! Tests run with cwd `crates/core`, so `find_model_dir` walks up to the repo's
//! `models/` folder; the corpus is read from `<repo>/fixtures/corpus`.

use std::path::{Path, PathBuf};

use saan_core::embed::Embedder;
use saan_core::index::{BuildEvent, BuildOptions};
use saan_core::router::{CONFIDENT, RouteSource};
use saan_core::{Engine, Index, Mode, Scope};

/// The corpus files each test indexes: recipes, code, notes, a PDF and both
/// same-name pairs the tests assert on. Small so the model stays fast.
const SUBSET: &[&str] = &[
    "code/rust/dijkstra.rs",
    "notes/meeting-notes.md",
    "notes/todo.txt",
    "papers/coral-bleaching-thesis.pdf",
    "recipes/sourdough.md",
    "work/projectA/README.md",
    "work/projectB/README.md",
    "work/todo.txt",
];

/// `crates/core/../../fixtures/corpus` = `<repo>/fixtures/corpus`.
fn corpus() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/corpus"))
}

/// Load the embedder, or print the skip notice and return `None` without it.
fn embedder_or_skip() -> Option<Embedder> {
    let Some(model_dir) = saan_core::embed::find_model_dir() else {
        println!("skipping: model not found");
        return None;
    };
    Some(Embedder::load(&model_dir).expect("loading embedder"))
}

/// A fresh temp tree holding [`SUBSET`] with its relative paths intact, or
/// `None` (after printing the skip notice) when the model is missing. The name
/// is unique per test process and test so parallel tests never share a tree.
fn fresh_subset_root(test: &str) -> Option<PathBuf> {
    if saan_core::embed::find_model_dir().is_none() {
        println!("skipping: model not found");
        return None;
    }
    let root = std::env::temp_dir().join(format!("saan-engine-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for rel in SUBSET {
        let dest = root.join(rel);
        std::fs::create_dir_all(dest.parent().expect("subset paths have a parent"))
            .expect("creating temp dirs");
        std::fs::copy(corpus().join(rel), &dest).unwrap_or_else(|e| panic!("copying {rel}: {e}"));
    }
    Some(root)
}

/// Index `root` and wrap it in an `Engine` with Jev disabled. The index is built
/// with a local `Embedder`; the engine only gets the model directory and loads
/// its own copy on the first semantic query. Returns the engine and the
/// canonical temp root (so callers can clean it up), or `None` when the model is
/// missing (the caller has already printed the skip notice).
fn engine_for(root: &Path) -> Option<(Engine, PathBuf)> {
    let model_dir = saan_core::embed::find_model_dir()?;
    let embedder = Embedder::load(&model_dir).expect("loading embedder");
    let (index, _stats) = Index::build(
        &Scope::single(root),
        &embedder,
        None,
        &BuildOptions { checkpoint_every: 0 },
        |_| true,
    )
    .expect("building index");
    let temp_root = index.roots[0].clone();
    Some((Engine::new(index, model_dir, None), temp_root))
}

#[test]
fn semantic_paraphrase_ranks_sourdough_first() {
    let Some(dir) = fresh_subset_root("semantic") else { return };
    let Some((engine, temp_root)) = engine_for(&dir) else { return };

    // Paraphrase from fixtures/eval.json (expected: recipes/sourdough.md).
    let response = engine
        .search(
            "how long should bread dough rest in the fridge before baking it in a dutch oven",
            5,
        )
        .expect("searching");

    assert_eq!(response.route.mode, Mode::Semantic);
    assert_eq!(response.route.source, RouteSource::Rules);
    assert_eq!(response.hits[0].rel, "recipes/sourdough.md");
    assert_eq!(response.hits[0].name, "sourdough.md");
    assert!(response.hits[0].score > 0.0);
    assert!(!response.hits[0].snippet.is_empty());
    assert!(!response.hits[0].same_name);

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn guessed_glob_falls_back_to_semantics_but_explicit_glob_does_not() {
    let Some(dir) = fresh_subset_root("fallback") else { return };
    let Some((engine, temp_root)) = engine_for(&dir) else { return };

    // Looks like a file name, so the local rules guess glob; nothing matches, so
    // an inexact (non-explicit) route falls back to meaning.
    let guessed = engine.search("nonexistent-file.txt", 5).expect("searching");
    assert_eq!(guessed.route.mode, Mode::Semantic);
    assert_eq!(guessed.route.source, RouteSource::Rules);
    assert!(!guessed.hits.is_empty(), "guessed glob should fall back to semantic hits");

    // An explicit prefix is honoured even when it finds nothing.
    let explicit = engine.search("glob:nonexistent-file.txt", 5).expect("searching");
    assert_eq!(explicit.route.mode, Mode::Glob);
    assert_eq!(explicit.route.source, RouteSource::Explicit);
    assert!(explicit.hits.is_empty());

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn same_name_files_are_reported_separately() {
    let Some(dir) = fresh_subset_root("same-name") else { return };
    let Some((engine, temp_root)) = engine_for(&dir) else { return };

    let response = engine.search("todo.txt", 10).expect("searching");
    assert_eq!(response.route.mode, Mode::Glob);

    let mut rels: Vec<&str> = response.hits.iter().map(|h| h.rel.as_str()).collect();
    rels.sort_unstable();
    assert_eq!(rels, ["notes/todo.txt", "work/todo.txt"]);
    assert!(response.hits.iter().all(|h| h.name == "todo.txt" && h.same_name));
    // Glob hits carry filesystem metadata, so same-name rows can be told apart.
    assert!(
        response.hits.iter().all(|h| h.ext == "txt" && h.size > 0 && h.modified > 0),
        "hits: {:?}",
        response.hits
    );

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn model_loads_lazily_and_unloads_when_idle() {
    let Some(dir) = fresh_subset_root("lazy") else { return };
    let Some((engine, temp_root)) = engine_for(&dir) else { return };

    assert!(!engine.model_loaded(), "Engine::new must not load the model");

    // An exact-name search is answered by glob and never needs the model.
    let globbed = engine.search("todo.txt", 10).expect("searching");
    assert_eq!(globbed.route.mode, Mode::Glob);
    assert!(!globbed.hits.is_empty());
    assert!(!engine.model_loaded(), "glob must not load the model");

    // Meaning needs the model, so the first semantic query loads it.
    let semantic = engine
        .search(
            "how long should bread dough rest in the fridge before baking it in a dutch oven",
            5,
        )
        .expect("searching");
    assert_eq!(semantic.route.mode, Mode::Semantic);
    assert_eq!(semantic.hits[0].rel, "recipes/sourdough.md");
    assert!(engine.model_loaded());

    // Nothing has been searched "recently" with a zero idle window.
    assert!(engine.release_if_idle(std::time::Duration::ZERO));
    assert!(!engine.model_loaded());
    assert!(!engine.release_if_idle(std::time::Duration::ZERO), "already unloaded");

    // Unloading only frees memory: the next semantic query reloads on demand.
    let again = engine.search("dijkstra shortest path through a weighted graph", 5).expect("searching");
    assert_eq!(again.route.mode, Mode::Semantic);
    assert!(!again.hits.is_empty());
    assert!(engine.model_loaded());

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn rebuild_reuses_unchanged_files_and_reindexes_the_changed_one() {
    let Some(dir) = fresh_subset_root("incremental") else { return };
    let Some(embedder) = embedder_or_skip() else { return };
    let scope = Scope::single(&dir);
    let opts = BuildOptions { checkpoint_every: 0 };

    let (first, stats) = Index::build(&scope, &embedder, None, &opts, |_| true).expect("building index");
    assert_eq!(stats.files_seen, SUBSET.len(), "every copied file is walked");
    let files = first.files.len();
    assert_eq!(stats.files_indexed, files);
    assert!(
        first.files.iter().any(|f| f.rel == "papers/coral-bleaching-thesis.pdf"),
        "PDF text is indexed"
    );

    // Nothing changed: every file comes from the previous index.
    let (reused, stats) =
        Index::build(&scope, &embedder, Some(&first), &opts, |_| true).expect("rebuilding index");
    assert_eq!(stats.files_indexed, 0);
    assert_eq!(stats.files_reused, files);
    assert_eq!(reused.files.len(), files);

    // A different length changes the (size, mtime) stamp, so exactly this file
    // is re-extracted and re-embedded.
    let rewritten = "sourdough schedule: feed the starter twice a day for a week, then bake.";
    std::fs::write(dir.join("recipes/sourdough.md"), rewritten).expect("rewriting a corpus file");
    let (_, stats) =
        Index::build(&scope, &embedder, Some(&reused), &opts, |_| true).expect("rebuilding index");
    assert_eq!(stats.files_indexed, 1);
    assert_eq!(stats.files_reused, files - 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelled_build_yields_a_partial_index_the_next_build_reuses() {
    let Some(dir) = fresh_subset_root("cancel") else { return };
    let Some(embedder) = embedder_or_skip() else { return };
    let scope = Scope::single(&dir);
    let opts = BuildOptions { checkpoint_every: 0 };

    // Refuse to continue once the second file is announced: only the first file
    // is embedded, so the partial index holds a single file.
    let mut announced = 0usize;
    let (partial, stats) = Index::build(&scope, &embedder, None, &opts, |event| {
        if matches!(event, BuildEvent::File { .. }) {
            announced += 1;
            if announced >= 2 {
                return false;
            }
        }
        true
    })
    .expect("building index");

    assert!(stats.cancelled, "stats: {stats:?}");
    assert_eq!(announced, 2, "the build stops on the second announcement");
    assert_eq!(stats.files_seen, SUBSET.len());
    assert_eq!(stats.files_indexed, 1, "the first file was embedded before the cancel");
    assert_eq!(partial.files.len(), 1);
    assert!(partial.files.len() < SUBSET.len(), "a cancelled index is incomplete");
    assert_eq!(stats.chunks, partial.chunks.len());

    // The partial index is searchable on its own.
    let query = embedder.embed_query("dijkstra shortest path through a weighted graph").expect("embedding");
    assert!(!partial.search(&query, 5).is_empty());

    // Rebuilding on top of it reuses what it holds and finishes the rest.
    let (full, stats) =
        Index::build(&scope, &embedder, Some(&partial), &opts, |_| true).expect("rebuilding index");
    assert!(!stats.cancelled);
    assert!(
        stats.files_reused >= partial.files.len(),
        "reused {} of the partial index's {} files",
        stats.files_reused,
        partial.files.len()
    );
    assert_eq!(stats.files_reused, partial.files.len(), "exactly the partial files are reused");
    assert_eq!(full.files.len(), stats.files_reused + stats.files_indexed);
    assert!(full.files.len() > partial.files.len(), "the rebuilt index is complete");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn jev_is_never_called_when_disabled() {
    let Some(dir) = fresh_subset_root("jev-disabled") else { return };
    let Some((engine, temp_root)) = engine_for(&dir) else { return };

    assert!(!engine.jev_enabled());

    // A lone identifier routes below CONFIDENT, which is exactly when an enabled
    // Jev would have been asked to route/pick; with Jev disabled it is silent.
    let response = engine.search("dijkstra", 5).expect("searching");
    assert_eq!(response.route.mode, Mode::Semantic);
    assert!(response.route.confidence < CONFIDENT);
    assert_eq!(response.jev_calls, 0);
    assert!(!response.hits.is_empty());

    let _ = std::fs::remove_dir_all(&temp_root);
}
