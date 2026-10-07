# saan

*saan* is Filipino for **"where?"** — the question you ask when you're looking
for a file.

saan is a privacy-first, local file launcher. Instead of slowly guessing exact
strings in a file explorer, you describe what you want and it finds the file,
using one of three search modes:

| Mode | Use when | Example query |
|---|---|---|
| **Semantic** | You remember what the file is *about* | `how long to proof bread dough overnight` |
| **Grep** | You remember text *inside* the file | `grep: fn dijkstra` · `/TODO\(.*\)/` |
| **Glob** | You remember the *name or path shape* | `glob: **/*.pdf` · `meeting-notes.md` |

Files that share a name (two `README.md`, three `todo.txt`) are always shown
with their full, distinct relative path so you can tell them apart.

## Features

- **Local by default.** Files are embedded on-device with
  [EmbeddingGemma](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX)
  (300M params, 4-bit ONNX, ~197 MB). No file data leaves the machine unless you opt into
  the Jev router.
- **Fast.** Rust backend, brute-force cosine search over an in-memory index,
  target p95 < 100 ms per warm query.
- **Lightweight.** One Tauri binary + one model folder. No Python, no server, no
  GPU required. Plain TypeScript UI (no framework).
- **Three modes, one box.** A local router picks semantic vs grep vs glob from
  the query, with explicit prefixes to override it.
- **Optional typed decisions.** [Jev](https://docs.typesafe.ai/api) (TypeSafe AI
  System One) can choose the search mode for ambiguous queries and pick between
  near-duplicate candidates. Off by default; three privacy levels control what
  is sent.
- **Global hotkey.** `Ctrl+Shift+Space` toggles the launcher; type, hit Enter.

## Quick start

Requires the Rust toolchain and Node.js.

```sh
# 1. Download the EmbeddingGemma model into ./models (gitignored).
cargo run --release -p saan-cli -- fetch-model

# 2. Index a folder.
cargo run --release -p saan-cli -- index "C:\Users\you\Documents"

# 3. Search it.
cargo run --release -p saan-cli -- search "notes about the japan trip"
cargo run --release -p saan-cli -- search --json -k 5 "grep: fn main"
```

Other CLI commands: `grep <pattern>`, `glob <pattern>`, `eval <file>` (top-k hit
rate) and `bench <file>` (warm p95 latency); use `--index <dir>` to point at a
different index (default `$SAAN_INDEX_DIR` or `./.saan/index`).

### Desktop app

```sh
npm install

# Primary: Tauri CLI from devDependencies, run via npm scripts.
npm run tauri dev
npm run tauri build -- --debug
```

Alternative — the `.tools`-installed `cargo tauri` (note: `cargo tauri` needs
`.tools/bin` on `PATH`):

```sh
# tauri CLI installed project-locally into .tools (no global install needed)
cargo install tauri-cli --version ^2 --root .tools --locked

# with .tools/bin on PATH:
export PATH="$PWD/.tools/bin:$PATH"   # Windows: set PATH=%CD%\.tools\bin;%PATH%
cargo tauri dev
cargo tauri build --debug
```

The window is hidden at launch; press **Ctrl+Shift+Space** to toggle it. On first
run, enter a folder to index (or set `SAAN_ROOT` to index one at startup).

## Query syntax

| Syntax | Mode | What it does |
|---|---|---|
| `grep: <text>` | Grep | Regex search inside files (smart case). |
| `glob: <pattern>` | Glob | File-name / path pattern search. |
| `find: <text>` | Semantic | Force meaning-based search. |
| `/regex/` | Grep | Regex search inside files (leading and trailing `/`). |
| *(no prefix)* | Auto | Local heuristics decide; see below. |

Local heuristics (in order): a single token containing `*`, `?` or `[` → glob; a
single token that looks like a file name (`stem.ext`) or contains `/` (not
leading) → glob; text containing `( ) { } ; = < >` or `::` → grep (escaped
literal); a lone word → semantic (low confidence, Jev may reroute); otherwise a
multi-word phrase → semantic. When an auto-routed grep/glob finds nothing, the
search falls back to semantic over the raw query.

## Jev (optional)

Jev is off unless **both** `SAAN_JEV` is `on`/`1`/`true` and `JEV_API_KEY` is
set. Requests go to `POST {base}/v1/systemone` with a 2500 ms timeout.

| Privacy level | `SAAN_JEV_PRIVACY` | Routing request | Candidate pick request |
|---|---|---|---|
| `a` (default) | `a` / `query` | query text | never sent |
| `b` | `b` / `paths` | query text | query + candidate relative paths |
| `c` | `c` / `snippets` | query text | query + candidate relative paths + short snippets |

Absolute paths, file contents beyond the snippet, and index data are never sent.

| Variable | Purpose |
|---|---|
| `SAAN_JEV` | `on`/`1`/`true` enables the Jev router. |
| `JEV_API_KEY` | Bearer token (required). |
| `SAAN_JEV_PRIVACY` | `a`/`query` (default), `b`/`paths`, `c`/`snippets`. |
| `SAAN_JEV_URL` | API base URL (default `https://api.typesafe.ai`). |
| `SAAN_JEV_MODEL` | Model name (default `jev-latest`). |

Other environment variables: `SAAN_MODEL_DIR` (model folder),
`SAAN_INDEX_DIR` (CLI index folder), `SAAN_ROOT` (folder to index at app
startup), `SAAN_SHOW` (show the window at launch).

See `docs/system-design` for the full architecture and the verification log.

## License

Apache-2.0. See [`LICENSE`](LICENSE).

**Model license:** the app code is Apache-2.0, but the EmbeddingGemma weights
downloaded by `saan fetch-model` (from
[`onnx-community/embeddinggemma-300m-ONNX`](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX),
derived from `google/embeddinggemma-300m`) are governed by Google's
[Gemma Terms of Use](https://ai.google.dev/gemma/terms) and are **not** covered
by this repository's license; they are not redistributed in the repo.
