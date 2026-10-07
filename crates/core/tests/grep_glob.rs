//! Integration tests for `saan_core::grep` and `saan_core::glob` over the
//! committed `fixtures/corpus` tree, plus `Scope` behaviour (size cap,
//! SKIP_DIRS pruning, multi-root display paths, content classification).
//! Expected values are derived from the real files; outputs are sorted so the
//! tests are robust to walk order.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use saan_core::scope::{Scope, DEFAULT_MAX_FILE_BYTES};

/// `crates/core/../../fixtures/corpus` = `<repo>/fixtures/corpus`.
fn corpus() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/corpus"))
}

/// The corpus as a single-root scope: relative paths stay unprefixed.
fn corpus_scope() -> Scope {
    Scope::single(corpus())
}

/// Grep and reduce to `(rel, line)` pairs, sorted.
fn grep_hits(pattern: &str) -> Vec<(String, usize)> {
    let mut hits: Vec<(String, usize)> =
        saan_core::grep::grep(&corpus_scope(), pattern, usize::MAX)
            .unwrap_or_else(|e| panic!("grep `{pattern}` failed: {e}"))
            .into_iter()
            .map(|h| (h.rel, h.line))
            .collect();
    hits.sort();
    hits
}

/// Glob and sort the relative paths.
fn glob_hits(pattern: &str) -> Vec<String> {
    let mut out: Vec<String> = saan_core::glob::glob(&corpus_scope(), pattern, usize::MAX)
        .unwrap_or_else(|e| panic!("glob `{pattern}` failed: {e}"))
        .into_iter()
        .map(|h| h.rel)
        .collect();
    out.sort();
    out
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Temp directory removed on drop (even when a test panics).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "saan-scope-{tag}-{}-{nanos}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write a file, creating parent directories as needed.
fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn rels_of(scope: &Scope) -> Vec<String> {
    scope.files().into_iter().map(|f| f.rel).collect()
}

// ---------------------------------------------------------------- grep ------

#[test]
fn grep_regex_fn_definitions() {
    // No uppercase in the pattern -> smart case makes it case-insensitive.
    let expected = vec![
        ("code/rust/cli_args.rs".to_string(), 39),
        ("code/rust/dijkstra.rs".to_string(), 10),
        ("code/rust/dijkstra.rs".to_string(), 14),
        ("code/rust/dijkstra.rs".to_string(), 20),
        ("code/rust/dijkstra.rs".to_string(), 47),
        ("code/rust/lru_cache.rs".to_string(), 13),
        ("code/rust/lru_cache.rs".to_string(), 18),
        ("code/rust/lru_cache.rs".to_string(), 25),
        ("code/rust/lru_cache.rs".to_string(), 32),
        ("code/rust/lru_cache.rs".to_string(), 48),
    ];
    assert_eq!(grep_hits(r"fn \w+\("), expected);
}

#[test]
fn grep_case_insensitive_word() {
    // `personal` has no uppercase -> case-insensitive, matching "personally".
    let expected = vec![
        ("finance/tax-deductions-checklist.md".to_string(), 1),
        ("notes/todo.txt".to_string(), 1),
        ("work/performance-review-2025.md".to_string(), 17),
    ];
    assert_eq!(grep_hits("personal"), expected);
}

#[test]
fn grep_smart_case_uppercase() {
    // Upper-case pattern -> case-sensitive; only the exact `TODO` occurrences.
    let expected = vec![
        ("code/web/todo-app.ts".to_string(), 1),
        ("notes/todo.txt".to_string(), 1),
        ("work/todo.txt".to_string(), 1),
    ];
    assert_eq!(grep_hits("TODO"), expected);
}

#[test]
fn grep_matches_two_same_name_files() {
    // Same content line in both `packing-list.txt` files (kyoto and iceland).
    let expected = vec![
        ("travel/iceland/packing-list.txt".to_string(), 1),
        ("travel/kyoto/packing-list.txt".to_string(), 1),
    ];
    let hits = grep_hits("Packing list");
    assert_eq!(hits, expected);
    // Both hits are same-name files distinguished by their relative path.
    let rels: Vec<&String> = hits.iter().map(|(rel, _)| rel).collect();
    assert!(rels.iter().all(|r| r.ends_with("packing-list.txt")));
    assert_ne!(rels[0], rels[1]);
}

#[test]
fn grep_invalid_regex_is_error() {
    assert!(saan_core::grep::grep(&corpus_scope(), "(", usize::MAX).is_err());
    assert!(saan_core::grep::grep(&corpus_scope(), "[unclosed", usize::MAX).is_err());
}

#[test]
fn grep_reports_absolute_path_beside_rel() {
    let hits = saan_core::grep::grep(&corpus_scope(), "Packing list", usize::MAX).unwrap();
    assert!(!hits.is_empty());
    for hit in hits {
        assert!(hit.path.is_absolute(), "`{}` is not absolute", hit.path.display());
        assert!(hit.path.ends_with(hit.rel.replace('/', std::path::MAIN_SEPARATOR_STR)));
    }
}

// ---------------------------------------------------------------- glob ------

#[test]
fn glob_todo_txt_returns_both() {
    assert_eq!(glob_hits("todo.txt"), s(&["notes/todo.txt", "work/todo.txt"]));
}

#[test]
fn glob_readme_md_returns_both_projects() {
    assert_eq!(
        glob_hits("README.md"),
        s(&["work/projectA/README.md", "work/projectB/README.md"])
    );
}

#[test]
fn glob_all_pdfs() {
    assert_eq!(
        glob_hits("*.pdf"),
        s(&[
            "finance/retirement-plan.pdf",
            "papers/coral-bleaching-thesis.pdf",
            "papers/sleep-and-memory-review.pdf",
            "papers/transformer-attention-summary.pdf",
            "papers/urban-heat-islands.pdf",
            "school/lecture-photosynthesis.pdf",
            "work/projectA/vendor-contract-summary.pdf",
        ])
    );
}

#[test]
fn glob_path_pattern_travel_packing_lists() {
    assert_eq!(
        glob_hits("travel/**/packing-list.txt"),
        s(&[
            "travel/iceland/packing-list.txt",
            "travel/kyoto/packing-list.txt",
        ])
    );
}

#[test]
fn glob_is_case_insensitive() {
    assert_eq!(glob_hits("*.PDF"), glob_hits("*.pdf"));
    assert_eq!(glob_hits("TODO.TXT"), glob_hits("todo.txt"));
}

#[test]
fn glob_no_match_is_empty() {
    assert!(glob_hits("no-such-file.xyz").is_empty());
    assert!(glob_hits("*.catch").is_empty());
}

#[test]
fn glob_same_name_pairs_are_distinct() {
    // Each same-name pair from fixtures/README.md.
    let names = [
        "README.md",
        "meeting-notes.md",
        "todo.txt",
        "utils.py",
        "packing-list.txt",
    ];
    for name in names {
        let hits = glob_hits(name);
        assert!(hits.len() >= 2, "expected >=2 hits for `{name}`, got {hits:?}");
        let unique: BTreeSet<&String> = hits.iter().collect();
        assert_eq!(unique.len(), hits.len(), "duplicate rel paths for `{name}`: {hits:?}");
        for rel in &hits {
            assert!(
                rel.ends_with(name),
                "`{rel}` does not end with `{name}`"
            );
        }
    }
}

#[test]
fn glob_reports_size_and_mtime() {
    let hits = saan_core::glob::glob(&corpus_scope(), "notes/todo.txt", usize::MAX).unwrap();
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert!(hit.size > 0, "expected a non-empty todo.txt");
    assert!(hit.mtime > 0, "expected an mtime for todo.txt");
    let meta = std::fs::metadata(&hit.path).unwrap();
    assert_eq!(hit.size, meta.len());
    assert_eq!(hit.path, corpus().join("notes").join("todo.txt"));
}

#[test]
fn glob_invalid_pattern_is_error() {
    assert!(saan_core::glob::glob(&corpus_scope(), "a[", usize::MAX).is_err());
}

// --------------------------------------------------------------- scope ------

#[test]
fn oversized_file_is_hidden_from_files_glob_and_grep() {
    let dir = TempDir::new("large");
    let big = dir.path().join("big.txt");
    let small = dir.path().join("small.txt");
    write(&big, &format!("needle {}", "x".repeat(4096)));
    write(&small, "needle here\n");
    assert!(
        std::fs::metadata(&big).unwrap().len() > 1024,
        "the test file must exceed the limit"
    );

    let scope = Scope::new(vec![dir.path().to_path_buf()], 1024);
    assert_eq!(rels_of(&scope), s(&["small.txt"]));
    assert_eq!(glob_hits_of(&scope, "*.txt"), s(&["small.txt"]));
    let grepped: Vec<String> = saan_core::grep::grep(&scope, "needle", usize::MAX)
        .unwrap()
        .into_iter()
        .map(|h| h.rel)
        .collect();
    assert_eq!(grepped, s(&["small.txt"]));

    // With a limit above both files, both are visible again.
    let all = Scope::new(vec![dir.path().to_path_buf()], DEFAULT_MAX_FILE_BYTES);
    assert_eq!(rels_of(&all), s(&["big.txt", "small.txt"]));
}

fn glob_hits_of(scope: &Scope, pattern: &str) -> Vec<String> {
    let mut out: Vec<String> = saan_core::glob::glob(scope, pattern, usize::MAX)
        .unwrap()
        .into_iter()
        .map(|h| h.rel)
        .collect();
    out.sort();
    out
}

#[test]
fn skip_dirs_are_not_walked() {
    let dir = TempDir::new("skip");
    write(dir.path().join("keep.txt"), "keep\n");
    write(dir.path().join("src/main.rs"), "fn main() {}\n");
    write(dir.path().join("node_modules/dep/package.txt"), "dep\n");
    write(dir.path().join("AppData/Local/state.txt"), "state\n");
    // Pruning matches by directory name, case-insensitively.
    write(dir.path().join("NODE_MODULES/upper.txt"), "upper\n");
    write(dir.path().join(".venv/lib/module.py"), "print('x')\n");

    let scope = Scope::single(dir.path().to_path_buf());
    assert_eq!(rels_of(&scope), s(&["keep.txt", "src/main.rs"]));
    assert_eq!(
        glob_hits_of(&scope, "*.txt"),
        s(&["keep.txt"]),
        "skipped directories must not leak into glob"
    );
    let grepped = saan_core::grep::grep(&scope, "state", usize::MAX).unwrap();
    assert!(grepped.is_empty(), "skipped directories must not leak into grep");
}

#[test]
fn multi_root_display_rels_are_prefixed_and_paths_distinct() {
    let base = TempDir::new("multi");
    let alpha = base.path().join("alpha");
    let beta = base.path().join("beta");
    write(alpha.join("notes.txt"), "alpha note\n");
    write(beta.join("notes.txt"), "beta note\n");

    let scope = Scope::new(vec![alpha.clone(), beta.clone()], DEFAULT_MAX_FILE_BYTES);
    assert_eq!(scope.display_rel(0, "notes.txt"), "alpha/notes.txt");
    assert_eq!(scope.display_rel(1, "notes.txt"), "beta/notes.txt");
    // The per-root relative paths stay unprefixed.
    assert_eq!(rels_of(&scope), s(&["notes.txt", "notes.txt"]));

    let hits = saan_core::glob::glob(&scope, "notes.txt", usize::MAX).unwrap();
    let rels: Vec<String> = hits.iter().map(|h| h.rel.clone()).collect();
    assert_eq!(rels, s(&["alpha/notes.txt", "beta/notes.txt"]));
    assert_eq!(hits[0].path, alpha.join("notes.txt"));
    assert_eq!(hits[1].path, beta.join("notes.txt"));
    assert_ne!(hits[0].path, hits[1].path, "same-name roots must stay distinct files");

    // A path pattern may address the displayed, root-labelled path.
    let beta_only = saan_core::glob::glob(&scope, "beta/notes.txt", usize::MAX).unwrap();
    assert_eq!(
        beta_only.into_iter().map(|h| h.rel).collect::<Vec<_>>(),
        s(&["beta/notes.txt"])
    );

    // Grep reports the same display paths and absolute paths.
    let grepped = saan_core::grep::grep(&scope, "note", usize::MAX).unwrap();
    assert_eq!(
        grepped.iter().map(|h| h.rel.clone()).collect::<Vec<_>>(),
        s(&["alpha/notes.txt", "beta/notes.txt"])
    );
    assert_eq!(grepped[1].path, beta.join("notes.txt"));
}

#[cfg(windows)]
#[test]
fn drive_root_label_is_the_drive() {
    let scope = Scope::new(
        vec![PathBuf::from(r"C:\"), PathBuf::from(r"D:\")],
        DEFAULT_MAX_FILE_BYTES,
    );
    assert_eq!(scope.display_rel(0, "Users/me/a.txt"), "C:/Users/me/a.txt");
    assert_eq!(scope.display_rel(1, "work/b.txt"), "D:/work/b.txt");
}

#[test]
fn content_file_classification() {
    for name in ["a.md", "README.MARKDOWN", "notes.txt", "main.rs", "data.json", "page.pdf"] {
        assert!(
            saan_core::extract::is_content_file(Path::new(name)),
            "`{name}` should be a content file"
        );
    }
    for name in ["app.exe", "photo.png", "clip.mp4", "archive.zip", "no-extension"] {
        assert!(
            !saan_core::extract::is_content_file(Path::new(name)),
            "`{name}` should not be a content file"
        );
    }
}
