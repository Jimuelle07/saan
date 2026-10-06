//! Integration tests for `saan_core::grep` and `saan_core::glob` over the
//! committed `fixtures/corpus` tree. Expected values are derived from the real
//! files; outputs are sorted so the tests are robust to walk order.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// `crates/core/../../fixtures/corpus` = `<repo>/fixtures/corpus`.
fn corpus() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/corpus"))
}

/// Grep and reduce to `(rel, line)` pairs, sorted.
fn grep_hits(pattern: &str) -> Vec<(String, usize)> {
    let mut hits: Vec<(String, usize)> =
        saan_core::grep::grep(&corpus(), pattern, usize::MAX)
            .unwrap_or_else(|e| panic!("grep `{pattern}` failed: {e}"))
            .into_iter()
            .map(|h| (h.rel, h.line))
            .collect();
    hits.sort();
    hits
}

/// Glob and sort the relative paths.
fn glob_hits(pattern: &str) -> Vec<String> {
    let mut out = saan_core::glob::glob(&corpus(), pattern, usize::MAX)
        .unwrap_or_else(|e| panic!("glob `{pattern}` failed: {e}"));
    out.sort();
    out
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

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
    assert!(saan_core::grep::grep(&corpus(), "(", usize::MAX).is_err());
    assert!(saan_core::grep::grep(&corpus(), "[unclosed", usize::MAX).is_err());
}

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
