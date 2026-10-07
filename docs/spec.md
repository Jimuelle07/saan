# saan — specification

*saan* (Filipino, "where?") is a privacy-first local file launcher: it finds a
file by what it is about (semantic search), by text inside it (grep) or by its
name/path shape (glob). This document is the contract for the product. Each
requirement is numbered, written in MUST language, and states the acceptance
check and the exact artifact that proves it, so every requirement is verifiable
without judgement calls.

Context: `docs/idea.md` (intent), `docs/system-design` (architecture as
implemented), `README.md` (usage). Requirements reference real files, real test
function names and real CLI commands; no other flags exist.

Prerequisites for the semantic requirements (R3–R5, R9, R14, R15): the
EmbeddingGemma model at `models/embeddinggemma-300m` (`saan fetch-model`, or
`SAAN_MODEL_DIR`) and a built release CLI. Requirements whose verification needs
the model are
"model-gated": when the model is absent the test MUST be skipped, not failed, so
R1 stays green on machines without the weights.

## Requirements

### R1 — The workspace builds and its tests pass

**Statement.** The Rust workspace (`crates/core`, `crates/cli`, `src-tauri`)
MUST compile in release mode, and the full test suite MUST pass:
`cargo build --release` and `cargo test --workspace` MUST both exit 0.

**Acceptance check.** Run both commands from the repo root; each exits 0.

**Verifying artifact.** `cargo build --release`; `cargo test --workspace`. The
suite consists of the unit tests in `crates/core/src/chunk.rs` and
`crates/core/src/router.rs` plus the integration tests in
`crates/core/tests/grep_glob.rs`, `crates/core/tests/jev.rs` and
`crates/core/tests/engine.rs`.

### R2 — Frontend and desktop app build

**Statement.** The TypeScript frontend MUST typecheck and bundle via
`npm run build` (script `tsc --noEmit && vite build` in `package.json`), and the
Tauri app MUST build in debug mode via `npm run tauri build -- --debug` (the
`tauri` script, using the `@tauri-apps/cli` devDependency) or via
`cargo tauri build --debug` with the project-local Tauri CLI (`.tools/bin`) on
`PATH`. Both MUST exit 0.

**Acceptance check.** `npm run build` exits 0. `npm run tauri build -- --debug`
exits 0. Alternatively, with `.tools/bin` on `PATH`
(`export PATH="$PWD/.tools/bin:$PATH"`, Windows
`set PATH=%CD%\.tools\bin;%PATH%`), `cargo tauri build --debug` exits 0. The
`cargo tauri` form uses the Tauri CLI installed project-locally with
`cargo install tauri-cli --version ^2 --root .tools --locked`.

**Verifying artifact.** `npm run build` (the `build` script in `package.json`);
`npm run tauri build -- --debug` (or `cargo tauri build --debug`).

### R3 — The corpus indexes

**Statement.** `saan index fixtures/corpus` MUST exit 0 and index at least 50
files covering Markdown, plain text, code and PDF, including at least three
same-name pairs (files that share a file name but live at different paths).

**Acceptance check.** `saan index fixtures/corpus` exits 0 and prints a summary
line `indexed N files (M re-embedded, K reused, C chunks) in …s -> …` with
`N >= 50`. The committed tree `fixtures/corpus` holds 62 files (23 `.md`,
15 `.txt`, 7 `.pdf`, 5 `.py`, 3 `.rs`, 2 `.ts`, 2 `.sql`, 2 `.go`, 1 `.toml`,
1 `.json`, 1 `.js`) and the five same-name pairs listed in `fixtures/README.md`
(`README.md`, `meeting-notes.md`, `todo.txt`, `utils.py`, `packing-list.txt`).
Running the same command twice MUST report a non-zero reuse count on the second
run.

**Verifying artifact.** `saan index fixtures/corpus` (index directory defaults
to `$SAAN_INDEX_DIR` or `./.saan/index`; use the global option `--index <dir>`
to relocate it). Inventory: `fixtures/corpus`, documented in `fixtures/README.md`.

### R4 — Semantic top-5 hit rate is at least 80%

**Statement.** Over `fixtures/eval.json` the routed engine MUST place the
expected file in the top 5 for at least 80% of cases:
`saan eval fixtures/eval.json` MUST exit 0.

**Acceptance check.** With the model and an index of `fixtures/corpus` present,
`saan eval fixtures/eval.json` exits 0 and prints
`top-k hit rate: P/46 = X% (min 80.0%)` with `X >= 80.0`. Defaults are
`-k 5` and `--min 0.8`; `eval` runs a local-only engine (no Jev), so the result
is reproducible.

**Verifying artifact.** `saan eval fixtures/eval.json`; cases in
`fixtures/eval.json` (46 `{query, expected}` objects).

### R5 — Warm p95 query latency is under 100 ms

**Statement.** A warm query MUST complete in under 100 ms at the 95th
percentile: `saan bench fixtures/eval.json` MUST exit 0 when run with the
release binary.

**Acceptance check.** `saan bench fixtures/eval.json` exits 0 and prints
`… warm queries over N files: p50 … ms, p95 … ms, max … ms (limit p95 100 ms)`
with `p95 < 100`. Defaults are `--runs 5` and `--max-ms 100`; `bench` warms the
engine with one pass over the queries before measuring and uses a local-only
engine (no Jev).

**Verifying artifact.** `saan bench fixtures/eval.json`.

### R6 — Grep and glob return exact results, with same-name files distinct

**Statement.** `grep(scope, pattern, limit)` MUST be line-oriented and
smart-case (case-insensitive unless the pattern contains an uppercase letter)
and MUST error on an invalid regex; `glob(scope, pattern, limit)` MUST be
case-insensitive, match a bare pattern against the file name at any depth and a
pattern containing `/` against either the per-root relative path or the display
relative path. Both MUST walk the given `Scope` — its roots and file-size cap,
with hidden-file/`.gitignore` rules and `SKIP_DIRS` applied (see R13) — and
both MUST return the exact expected result sets for the committed corpus, with
same-name files reported as distinct relative paths, never merged or
duplicated.

**Acceptance check.** The tests below pass under `cargo test --workspace`;
independently, `saan glob --root fixtures/corpus todo.txt` prints exactly
`notes/todo.txt` and `work/todo.txt`, and
`saan grep --root fixtures/corpus 'Packing list'` prints one line each for
`travel/iceland/packing-list.txt` and `travel/kyoto/packing-list.txt`.

**Verifying artifact.** Tests in `crates/core/tests/grep_glob.rs`:
`grep_regex_fn_definitions`, `grep_case_insensitive_word`,
`grep_smart_case_uppercase`, `grep_matches_two_same_name_files`,
`grep_invalid_regex_is_error`, `glob_todo_txt_returns_both`,
`glob_readme_md_returns_both_projects`, `glob_all_pdfs`,
`glob_path_pattern_travel_packing_lists`, `glob_is_case_insensitive`,
`glob_no_match_is_empty`, `glob_same_name_pairs_are_distinct`.

### R7 — Jev is off by default and bounded by privacy levels

**Statement.** The Jev decision layer MUST make zero network requests unless
`SAAN_JEV` is `on`/`1`/`true` **and** an API key is available. The API key MUST
come from the `JEV_API_KEY` environment variable **or** the OS credential store
(Windows Credential Manager via the `keyring` crate, service `saan`, user
`jev-api-key`) and MUST NEVER be read from a file; when `SAAN_JEV` and
`JEV_API_KEY` are both set the environment overrides the stored key, and the
stored key MUST NOT be written to `config.json` or logs. The routing request
MUST carry only the query at every level. The candidate-pick request MUST obey
the privacy level: level `a` (default) MUST NOT build or send a pick request,
level `b` MUST send only the query plus candidate relative paths, level `c` MUST
send the query, candidate relative paths and snippets truncated to
`SNIPPET_CHARS` (160) characters. Absolute paths, file contents beyond the
snippet and index data MUST never be sent, and a dead endpoint MUST surface as
an error rather than a panic or a hang.

**Acceptance check.** `env_gating_and_privacy_levels` passes: it drives a local
mock HTTP server (bound to `127.0.0.1:0`) and asserts the gating cases, one
recorded `POST /v1/systemone` with `Authorization: Bearer <key>` for routing,
the level `a`/`b`/`c` payload shapes and snippet truncation, and an `Err` for an
unreachable endpoint. A repository check that `JEV_API_KEY` occurs only as the
environment-variable name in `crates/core/src/jev.rs`, `src-tauri/src/main.rs`
and the docs, and that no source reads a key from a file path — the only key
inputs are `JevClient::from_env()` and `JevClient::new(key, privacy)` fed from
the `keyring` store — confirms the key comes from no other source.

**Verifying artifact.** `env_gating_and_privacy_levels` in
`crates/core/tests/jev.rs`, plus the repository checks named above.

### R8 — The local router follows fixed rules

**Statement.** `local_route` MUST resolve prefixes before heuristics:
`grep:` → grep, `glob:` → glob, `find:` → semantic, and `/…/` → grep, all as
`RouteSource::Explicit` with the prefix stripped. Without a prefix it MUST apply,
in order: a single token containing `*`, `?` or `[` → glob; a single token that
looks like a file name (`stem.ext`) or contains a non-leading `/` → glob; text
containing `( ) { } ; = < >` or `::` → grep with the literal escaped; a lone word
→ semantic; otherwise a multi-word phrase → semantic. Confidence MUST be below
`CONFIDENT` (0.75) only for the routes the router calls ambiguous (a lone word,
code-looking text), so those alone may consult Jev. `Mode` MUST serialise as
`semantic`/`grep`/`glob`.

**Acceptance check.** The tests below pass under `cargo test --workspace`.

**Verifying artifact.** Unit tests in `crates/core/src/router.rs` (module
`tests`): `prefixes_win_over_heuristics`, `heuristics`,
`only_ambiguous_routes_are_below_confidence_threshold`.

### R9 — Engine behaviour

**Statement.** `Engine::search` MUST satisfy all of the following, in addition
to R4:

1. A semantic query returns the expected file as a hit (result ordering puts it
   in the top `k`).
2. A guessed exact search (auto-routed grep/glob, `source != Explicit`) that
   returns no hits MUST fall back to semantic search over the raw trimmed input,
   and the returned route MUST be semantic.
3. An explicit `grep:` query that returns no hits MUST NOT fall back to
   semantic: the response MUST keep `mode == grep` with an empty hit list.
4. When two hits share a file name, every such hit MUST be flagged
   `same_name = true`; distinct files MUST carry distinct relative paths.
5. Re-indexing an unchanged tree MUST reuse every file without re-embedding:
   `BuildStats.files_reused` equals the file count and `files_indexed` is 0.

**Acceptance check.** The model-gated tests described below pass under
`cargo test --workspace`; they MUST be skipped (not failed) when the model folder
is absent.

**Verifying artifact.** Tests in `crates/core/tests/engine.rs` (one test per
behaviour above; being written now, so the individual function names are not yet
fixed).

### R10 — Launcher smoke test

**Statement.** The desktop launcher MUST perform its core loop: pressing
`Ctrl+Shift+Space` toggles the window's visibility, typing a query displays
matching results in the launcher, and pressing `Enter` opens the selected file
with the default application.

**Acceptance check.** A manual (or automated) smoke run — launch the app, press
`Ctrl+Shift+Space`, type a query against an indexed folder, confirm results
appear and `Enter` opens the selected file — MUST be performed and recorded as
a row in the Verification log of `docs/system-design` (date, check, result).

**Verifying artifact.** The row recording the smoke run in the Verification log
of `docs/system-design`; the wired behaviour is
`HOTKEY_LABEL = "Ctrl+Shift+Space"`, `toggle_window` and the `open_path` command
in `src-tauri/src/main.rs`, driving `src/main.ts`.

### R11 — Privacy and secrets

**Statement.** No API key and no model weights MUST be committed:
`models/`, `.saan/`, `.tools/`, `node_modules/`, `dist/` and `.env` MUST stay
gitignored, and no commit in the repository history MUST contain a model weight
file or a live credential (a non-empty `JEV_API_KEY` value, private key or
bearer token).

**Acceptance check.** `git check-ignore -v models .saan .tools node_modules dist .env`
names a matching `.gitignore` rule for each path;
`git log --all --full-history --name-only -- models .saan` prints nothing; and
a scan of history, `git log --all -p | grep -E 'JEV_API_KEY *=|BEGIN [A-Z ]*PRIVATE KEY'`,
returns no matches (the only hits for the identifier are the documented
environment-variable name).

**Verifying artifact.** `git check-ignore -v`, `git log --all --full-history`,
and the `.gitignore` rules for `/models/`, `/.saan/`, `/.tools/`,
`node_modules/`, `/dist/` and `.env`.

### R12 — Settings panel and themes

**Statement.** The desktop app MUST expose a settings view, opened by the gear
button in the search bar and by `Ctrl+,` and closed by `Esc` before the window
hides, containing: an Appearance section with exactly four themes (`blue`,
`violet`, `green`, `orange`, default `blue`) where accent **and** text colours
both follow the selection; a Folders section (root list with remove,
"Add folder…" via the native picker, one-click add of the suggested
Documents/Desktop/Downloads, max file size MB, index speed
`background`|`fast`); an Index section (Start/Cancel with progress); a Jev
section (password field with Save/Remove, a "key saved" state, enable toggle,
privacy levels A/B/C with one-line descriptions, env-override notice). Settings MUST
persist to `config.json` with every field `#[serde(default)]`, and the legacy
`{"root": "..."}` shape MUST migrate to `roots: [root]`. The Jev key MUST be
stored in the OS credential store (Windows Credential Manager, `keyring`
service `saan`, user `jev-api-key`) and MUST NOT appear in `config.json` or
logs; `JEV_API_KEY` + `SAAN_JEV` MUST override the saved key when set.

**Acceptance check.** A manual UI smoke run: open settings with the gear and
with `Ctrl+,`; switch each of the four themes and see accent and text recolour;
add and remove a folder; set max file size and index speed; save a Jev key →
"key saved" appears, `config.json` contains no key, Remove clears it; the
env-override notice appears when `JEV_API_KEY` is set; `Esc` closes settings before the window hides.

**Verifying artifact.** Manual UI smoke of the settings view in the running
app, exercising `get_settings`, `save_settings`, `set_jev_key` and
`pick_folder` in `src-tauri/src/main.rs`; `config.json` inspected for the
absence of the key.

### R13 — Multi-root scopes, with large and system files hidden

**Statement.** Indexing, grep and glob MUST operate on a `Scope` of one or more
roots with a `max_file_bytes` cap: CLI `saan index <ROOT>...` /
`--max-file-mb` (default 10) and `--root` (repeatable), app `roots` +
`max_file_mb` (default 10). With several roots every displayed path MUST carry
its root label (`<label>/<rel>`, the drive for a drive root); files larger than
the cap MUST NOT appear in semantic, grep or glob results; directories in
`SKIP_DIRS` (`node_modules`, `target`, `__pycache__`, `.venv`, `venv`, `.git`,
`$Recycle.Bin`, `System Volume Information`, `Windows`, `Program Files`,
`Program Files (x86)`, `ProgramData`, `AppData`) MUST be skipped
case-insensitively; `.gitignore`/`.ignore` and hidden-file rules MUST keep
applying; and only content files (PDF plus the text/doc/code allow-list) MUST
be embedded, while every other file remains glob- and grep-able.

**Acceptance check.** The tests below pass under `cargo test --workspace`;
independently, `saan grep --root <A> --root <B> <pattern>` prints hits whose
rels carry both root labels, and an oversized file under a root yields no
semantic, grep or glob hit.

**Verifying artifact.** Tests in `crates/core/tests/grep_glob.rs`:
`multi_root_display_rels_are_prefixed_and_paths_distinct`,
`drive_root_label_is_the_drive`, `oversized_file_is_hidden_from_files_glob_and_grep`,
`skip_dirs_are_not_walked`, `content_file_classification`.

### R14 — Index progress, cancellation and resume

**Statement.** `saan index <ROOT>...` MUST print progress on
`BuildEvent::File` to stderr at most every 250 ms as
`[done/total] eta 1m23s  rel`, then the usual summary line on stdout. The
app's `start_index` MUST emit `index-progress` at most 4×/s with payload
`{state, done, total, current, filesIndexed, filesReused, elapsedMs, message}`,
MUST save the partial index and swap it into the engine every 200 embedded
files (so search works during a long run), and `cancel_index` MUST stop the
build with `BuildStats.cancelled = true` while keeping the partial index. A
subsequent run MUST resume: unchanged files are counted in `files_reused` and
never re-embedded.

**Acceptance check.** The model-gated tests below pass under
`cargo test --workspace`. `saan index fixtures/corpus --index <tmp>` prints
`[N/M] eta …` lines to stderr and the summary to stdout; running it twice
reports a non-zero `files_reused` on the second run. Manual app smoke: Start
indexing a large folder, the progress bar shows done/total, ETA and the current
file; Cancel flips the state to `cancelled`; Start again resumes with growing
reuse counts.

**Verifying artifact.** Tests in `crates/core/tests/engine.rs`:
`rebuild_reuses_unchanged_files_and_reindexes_the_changed_one` and
`cancelled_build_yields_a_partial_index_the_next_build_reuses` (both emit
`BuildEvent::File` and assert reuse/cancel/resume), plus the `saan index`
stderr progress check.

### R15 — CPU-only by measurement

**Statement.** Inference MUST run on the CPU execution provider: no GPU /
DirectML execution provider is registered, and no settings, CLI or engine
option selects one.

**Rationale (recorded measurement).** Uncached `saan bench fixtures/eval.json`
on an RTX 4050 Laptop + i5-13420H measured CPU p95 76–80 ms vs DirectML p95
836–1072 ms — the 4-bit model's `MatMulNBits` / `GatherBlockQuantized` ops are
not DirectML-native and bounce between devices — so the GPU option was removed.

**Acceptance check.** `saan bench fixtures/eval.json` exits 0 with p95 < 100 ms
(R5 gate), and the settings panel exposes no GPU/Performance control.

**Verifying artifact.** `saan bench fixtures/eval.json`.

## Requirement → artifact

| # | Requirement | Artifact |
|---|---|---|
| R1 | Workspace builds; tests pass | `cargo build --release`, `cargo test --workspace` |
| R2 | Frontend + Tauri build | `npm run build`, `npm run tauri build -- --debug` (or `cargo tauri build --debug`) |
| R3 | Corpus indexes (≥50 files, ≥3 same-name pairs) | `saan index fixtures/corpus` |
| R4 | Top-5 hit rate ≥ 80% | `saan eval fixtures/eval.json` |
| R5 | Warm p95 < 100 ms | `saan bench fixtures/eval.json` |
| R6 | Grep/glob exact, same-name distinct | `crates/core/tests/grep_glob.rs` |
| R7 | Jev off by default; privacy levels A/B/C | `crates/core/tests/jev.rs` |
| R8 | Routing rules | `crates/core/src/router.rs` unit tests |
| R9 | Engine behaviour | `crates/core/tests/engine.rs` |
| R10 | Launcher smoke | `docs/system-design` Verification log |
| R11 | No keys or weights committed | `.gitignore`; `git log` scans |
| R12 | Settings panel, themes, key storage | Manual UI smoke; `config.json` without the key |
| R13 | Multi-root Scope; large/skipped files hidden | `crates/core/tests/grep_glob.rs` |
| R14 | Index progress, cancel, resume | `crates/core/tests/engine.rs`; `saan index` stderr progress |
| R15 | CPU-only inference by measurement | `saan bench fixtures/eval.json` |

## Workflow

Every change follows the same loop:

1. **Spec first.** Add or amend the numbered requirement here, including its
   acceptance check and verifying artifact, before writing code.
2. **Failing test.** Add the test named in the requirement (or the CLI check,
   for R3–R5, R10 and R15), and confirm it fails for the intended reason.
3. **Implement.** Make the smallest change that satisfies the requirement.
4. **Green.** The requirement's artifact passes, and R1 (plus R2 when the
   frontend or Tauri code changed) still passes.
5. **Commit.** One commit per requirement, referencing its number; commits MUST
   NOT include co-author trailers.
6. **Merge.** Merge to `main` only when every requirement R1–R15 passes; a
   requirement may be waived only by an explicit note in `docs/system-design`
   recording the reason and the replacement check.
