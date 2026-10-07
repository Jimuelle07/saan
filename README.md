<div align="center">

# saan

*saan* is Filipino for **"where?"** — the question you ask when you're looking for a file.

saan is a privacy-first, local file launcher. Instead of slowly guessing exact strings in a file explorer, you describe what you want and it finds the file, using one of three search modes:

| Mode | Use when | Example query |
|---|---|---|
| **Semantic** | You remember what the file is *about* | `how long to proof bread dough overnight` |
| **Grep** | You remember text *inside* the file | `grep: fn dijkstra` · `/TODO\(.*\)/` |
| **Glob** | You remember the *name or path shape* | `glob: **/*.pdf` · `meeting-notes.md` |

<br>

[![Rust: 2021 edition](https://img.shields.io/badge/Rust-2021%20edition-CE422B?style=flat&labelColor=000000&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri: 2](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat&labelColor=1B1B1D&logo=tauri&logoColor=white)](https://tauri.app/)
[![TypeScript: 5](https://img.shields.io/badge/TypeScript-5-3178C6?style=flat&labelColor=1E5A9E&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Embeddings: EmbeddingGemma 300M](https://img.shields.io/badge/Embeddings-EmbeddingGemma%20300M-4285F4?style=flat&labelColor=1A4FA0&logo=google&logoColor=white)](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX)
[![Inference: ONNX Runtime (CPU)](https://img.shields.io/badge/Inference-ONNX%20Runtime%20%28CPU%29-005CED?style=flat&labelColor=00337F&logo=onnx&logoColor=white)](https://onnxruntime.ai/)
[![Platform: Windows 10 | 11](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-5B8FD9?style=flat&labelColor=1A3A6B)](#installation-windows)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-D22128?style=flat&labelColor=8A1A1A&logo=apache&logoColor=white)](LICENSE)

Built by [Jimuelle Patron](https://github.com/Jimuelle07)

<br>

[Installation](#installation-windows) ·
[Features](#features) ·
[Usage](#usage) ·
[Configuration](#configuration) ·
[Architecture](#architecture) ·
[FAQ](#faq) ·
[Author](#author)

</div>

---

## Installation (Windows)

### Prerequisites

| Requirement | Notes |
|---|---|
| Windows 10/11 | The installer, hotkey and Credential Manager key storage target Windows. |
| PowerShell 5.1+ | Required by `scripts/install.ps1`. |
| [Rust toolchain](https://rustup.rs/) (MSVC) | Install via rustup with the default `x86_64-pc-windows-msvc` toolchain. |
| [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) | "Desktop development with C++" workload; needed by the Rust MSVC toolchain. |
| [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) | Preinstalled on Windows 11 and current Windows 10; install it if missing. |
| [Node.js](https://nodejs.org/) | Builds the TypeScript frontend via npm. |

See Tauri's [Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)
for details.

### Install

```powershell
git clone https://github.com/Jimuelle07/saan.git
cd saan
powershell -ExecutionPolicy Bypass -File scripts/install.ps1
```

The installer:

1. Builds the desktop app with `npm run tauri build` (which embeds the frontend —
   a plain `cargo build -p saan-app` would point the window at the dev server)
   and the CLI with cargo.
2. Stops any running launcher and copies `saan.exe` and `saan-app.exe` into
   `%LOCALAPPDATA%\saan\bin`.
3. Copies or fetches the model into `%LOCALAPPDATA%\saan\models\embeddinggemma-300m`.
4. Adds that `bin` folder to your **user** `PATH`, keeping `%VAR%` entries and
   the registry value type unchanged.

Then open a **new** terminal and run:

```sh
saan
```

With **no subcommand**, `saan` opens the desktop launcher: it spawns
`saan-app.exe` (next to `saan.exe`, or `$SAAN_APP`) detached with `SAAN_SHOW=1`,
hands over foreground rights so the window comes up focused, then exits. The
app is single-instance — running `saan` again focuses the existing window.

<details>
<summary><b>Installer options (custom location, skip rebuild, uninstall, purge)</b></summary>

```powershell
# Install somewhere other than %LOCALAPPDATA%\saan:
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Prefix "D:\Apps\saan"

# Reuse the existing target\release binaries (no rebuild):
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -SkipBuild

# Uninstall: drop bin\ from the user PATH and delete bin\ (keeps models + app data):
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Uninstall

# ...and delete the whole install root, including models and app data:
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Uninstall -Purge
```

</details>

---

## Build from Source

### CLI quick start

```sh
# 1. Download the EmbeddingGemma model into ./models (gitignored).
cargo run --release -p saan-cli -- fetch-model

# 2. Index one folder — or several, with a size cap (default 10 MB).
cargo run --release -p saan-cli -- index "C:\Users\you\Documents"
cargo run --release -p saan-cli -- index "C:\Users\you\Documents" "C:\Users\you\Downloads" --max-file-mb 10

# 3. Search it.
cargo run --release -p saan-cli -- search "notes about the japan trip"
cargo run --release -p saan-cli -- search --json -k 5 "grep: fn main"
```

### Desktop app

```sh
npm install

# Tauri CLI from devDependencies, run via npm scripts:
npm run tauri dev
npm run tauri build -- --debug
```

<details>
<summary><b>Alternative: project-local <code>cargo tauri</code></b></summary>

`cargo tauri` needs `.tools/bin` on `PATH`:

```sh
# tauri CLI installed project-locally into .tools (no global install needed)
cargo install tauri-cli --version ^2 --root .tools --locked

# with .tools/bin on PATH:
export PATH="$PWD/.tools/bin:$PATH"   # Windows: set PATH=%CD%\.tools\bin;%PATH%
cargo tauri dev
cargo tauri build --debug
```

</details>

The window is hidden at launch; press **Ctrl+Shift+Space** to toggle it. On first
run, enter a folder to index (or set `SAAN_ROOT` to index one at startup);
afterwards manage folders from the settings panel.

---

## Features

| Feature | Details |
|---|---|
| **Same-name files, disambiguated** | Files that share a name (two `README.md`, three `todo.txt`) are shown with their full relative path plus size, modification date and type. |
| **Local by default** | Files are embedded on-device with [EmbeddingGemma](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX) (300M params, 4-bit ONNX, ~197 MB). No file data leaves the machine unless you opt into the Jev router. |
| **Fast** | Rust backend with brute-force cosine search over an in-memory index; target p95 < 100 ms per warm query. |
| **Lightweight** | One Tauri binary + one model folder. No Python, no server, no GPU. Plain TypeScript UI (no framework). |
| **Lazy model loading** | The model loads on the first semantic query and unloads after `SAAN_IDLE_UNLOAD_SECS` (default 300) idle seconds. Grep and glob never load it. |
| **CPU-only inference** | Measured on an RTX 4050 Laptop + i5-13420H, the DirectML GPU path was ~13× slower (p95 836–1072 ms vs 76–80 ms on CPU, uncached `saan bench fixtures/eval.json`), so the GPU option was removed. |
| **Multi-folder indexing** | Index any number of root folders in the background with live progress (done/total, ETA, current file) and Start/Cancel. Partial indexes are saved as you go; restarting **resumes** and reuses unchanged files. |
| **Three modes, one box** | A local router picks semantic, grep or glob from the query; explicit prefixes override it. |
| **Optional typed decisions** | [Jev](https://docs.typesafe.ai/api) (TypeSafe AI System One) can route ambiguous queries and pick between near-duplicates. Off by default, with three privacy levels. |
| **Settings panel** | Four themes, folder list, max file size, index speed, Jev key (stored in Windows Credential Manager) and privacy level. |
| **Global hotkey** | `Ctrl+Shift+Space` toggles the launcher from anywhere. |

---

## Tech Stack

| Layer | Technology |
|---|---|
| Core engine | [Rust](https://www.rust-lang.org/) (`saan-core`) — indexing, routing, semantic search, grep, glob |
| Embeddings | [EmbeddingGemma 300M](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX) via [ONNX Runtime](https://onnxruntime.ai/) ([`ort`](https://github.com/pykeio/ort)) and [Hugging Face `tokenizers`](https://github.com/huggingface/tokenizers) |
| File walking & matching | [`ignore`](https://crates.io/crates/ignore), [`globset`](https://crates.io/crates/globset), [`regex`](https://crates.io/crates/regex) |
| PDF text extraction | [`pdf-extract`](https://crates.io/crates/pdf-extract) |
| Desktop app | [Tauri 2](https://tauri.app/) (`saan-app`) with dialog, global-shortcut, opener and single-instance plugins |
| Frontend | [TypeScript](https://www.typescriptlang.org/) + [Vite](https://vite.dev/), no UI framework |
| CLI | [`clap`](https://crates.io/crates/clap) (`saan`) |
| Secret storage | [`keyring`](https://crates.io/crates/keyring) → Windows Credential Manager |
| HTTP (optional Jev) | [`ureq`](https://crates.io/crates/ureq) with rustls |

---

## Usage

### Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+Space` | Show / hide the launcher (global) |
| `↑` / `↓` | Move the selection |
| `Enter` | Open the selected file |
| `Ctrl+Enter` | Reveal the selected file in File Explorer |
| `Ctrl+,` | Open / close settings |
| `Esc` | Close settings, otherwise hide the window |

### Query Syntax

| Syntax | Mode | What it does |
|---|---|---|
| `grep: <text>` | Grep | Regex search inside files (smart case). |
| `glob: <pattern>` | Glob | File-name / path pattern search. |
| `find: <text>` | Semantic | Force meaning-based search. |
| `/regex/` | Grep | Regex search inside files (leading and trailing `/`). |
| *(no prefix)* | Auto | Local heuristics decide; see below. |

**Auto-routing heuristics** (in order):

1. A single token containing `*`, `?` or `[` → **glob**.
2. A single token that looks like a file name (`stem.ext`) or contains `/` (not leading) → **glob**.
3. Text containing `( ) { } ; = < >` or `::` → **grep** (escaped literal).
4. A lone word → **semantic** (low confidence; Jev may reroute).
5. Otherwise, a multi-word phrase → **semantic**.

When an auto-routed grep/glob finds nothing, saan falls back to semantic search
over the raw query.

### Command-Line Interface

| Command | Description |
|---|---|
| `saan` | Open the desktop launcher. |
| `saan fetch-model` | Download EmbeddingGemma into `./models`. |
| `saan index <root>...` | Embed supported files under one or more roots (unchanged files are reused). `--max-file-mb <n>` caps file size. |
| `saan search <query>` | Routed search. `-k <n>` result count, `--json` for machine output. |
| `saan grep <pattern>` | Regex search inside files. `--root <dir>` (repeatable, default `.`), `--max-file-mb <n>` (default 10). |
| `saan glob <pattern>` | File name / path pattern search. Same `--root` and `--max-file-mb` options. |
| `saan eval <file>` | Top-k hit rate over `{query, expected}` pairs. |
| `saan bench <file>` | Warm query latency (p95) over the eval queries. |

Use `--index <dir>` to point at a different index (default `$SAAN_INDEX_DIR` or
`./.saan/index`). From source, `cargo run --release -p saan-cli` with no
subcommand opens `target/release/saan-app.exe` (override with `SAAN_APP`).

### Settings Panel

Open with the gear in the search bar or `Ctrl+,`.

| Section | Options |
|---|---|
| **Appearance** | Four themes — blue, violet, green, orange. Accent **and** text colours follow the selection. |
| **Folders** | Indexed roots: remove one, "Add folder…" (native picker), or one-click add of Documents/Desktop/Downloads. |
| **Max file size** | Files larger than this (MB, default 10) are hidden from semantic, grep and glob results everywhere. |
| **Index speed** | `background` (2 threads) or `fast` (all cores). |
| **Index** | Start/Cancel with progress bar, done/total, ETA and current file. A partial index is saved every 200 embedded files so search works during the run; starting again resumes. |
| **Jev** | Save or remove the API key, enable toggle, and privacy level A/B/C. Jev adds ~0.5 s to ambiguous searches. |

---

## Privacy

- File contents are embedded **on your machine**; the index stays local.
- The Jev router is **off by default**. When enabled, privacy level A (default)
  sends only the query text.
- The Jev API key is stored in the **Windows Credential Manager** (service
  `saan`, user `jev-api-key`) — never in `config.json` or logs.
- Absolute paths, file contents beyond the snippet, and index data are never sent.

---

## Jev Smart Routing (Optional)

Turn Jev on either from the settings panel (save the key, flip the toggle) or
through the environment: when **both** `SAAN_JEV` is `on`/`1`/`true` and
`JEV_API_KEY` are set, the environment (and its key) overrides the saved
settings. Requests go to `POST {base}/v1/systemone` with a 2500 ms timeout.

| Privacy level | `SAAN_JEV_PRIVACY` | Routing request | Candidate pick request |
|---|---|---|---|
| `a` (default) | `a` / `query` | query text | never sent |
| `b` | `b` / `paths` | query text | query + candidate relative paths |
| `c` | `c` / `snippets` | query text | query + candidate relative paths + short snippets |

The level is also selectable in the settings panel (default `a`); the env
variable applies when the environment path is active.

---

## Configuration

### Jev variables

| Variable | Purpose |
|---|---|
| `SAAN_JEV` | `on`/`1`/`true` enables the Jev router. |
| `JEV_API_KEY` | Bearer token for the env path; overrides the key saved in the Windows Credential Manager. The CLI reads only this environment variable. |
| `SAAN_JEV_PRIVACY` | `a`/`query` (default), `b`/`paths`, `c`/`snippets`. |
| `SAAN_JEV_URL` | API base URL (default `https://api.typesafe.ai`). |
| `SAAN_JEV_MODEL` | Model name (default `jev-latest`). |

### General variables

| Variable | Purpose |
|---|---|
| `SAAN_MODEL_DIR` | Model folder (overrides discovery). |
| `SAAN_INDEX_DIR` | CLI index folder (same as `--index`). |
| `SAAN_ROOT` | Root folder to (re)index when the app starts (`roots: [SAAN_ROOT]`). |
| `SAAN_SHOW` | Show the window at launch; the CLI sets it to `1` when it spawns the app. |
| `SAAN_APP` | Path to `saan-app.exe` for `saan` with no subcommand (default: next to `saan.exe`). |
| `SAAN_IDLE_UNLOAD_SECS` | Seconds of no semantic use before the app unloads the model (default 300; `0` keeps it loaded). |

---

## Indexing Details

**Semantically indexed files:** PDFs plus text, document and source-code
extensions — `md`, `txt`, `rst`, `org`, `tex`, `csv`, `json`, `yaml`, `toml`,
`xml`, `html`, `css`, `js`, `ts`, `py`, `rs`, `go`, `java`, `kt`, `c`, `cpp`,
`cs`, `rb`, `php`, `swift`, `sh`, `ps1`, `sql`, `lua`, `dart`, `vue`, `svelte`,
`log` and more. Other files are not embedded but remain grep/glob-searchable.

**Skipped directories:** `node_modules`, `target`, `__pycache__`, `.venv`,
`venv`, `.git`, `$Recycle.Bin`, `System Volume Information`, `Windows`,
`Program Files`, `Program Files (x86)`, `ProgramData` and `AppData`, on top of
the usual gitignore/hidden-file rules. Files larger than the max file size
(default 10 MB) are hidden from semantic, grep and glob results everywhere.

**Result metadata:** every hit carries `rel`, `path`, `name`, `score`, `line`,
`snippet`, `same_name`, `size` (bytes), `modified` (Unix seconds, `0` if
unknown) and `ext` (lowercase extension without the dot, empty if none).

---

## Architecture

```text
saan/
├── crates/
│   ├── core/        # saan-core: indexing, embeddings, router, semantic search, grep, glob, Jev client
│   └── cli/         # saan-cli: the `saan` command (index, search, grep, glob, eval, bench)
├── src-tauri/       # saan-app: Tauri 2 desktop launcher
├── src/             # TypeScript frontend (no framework)
├── scripts/         # install.ps1 Windows installer
└── docs/            # idea, spec and system design
```

See [`docs/system-design`](docs/system-design) for the full architecture and
verification log, plus [`docs/spec.md`](docs/spec.md) and
[`docs/idea.md`](docs/idea.md).

---

## FAQ

<details>
<summary><b>Does saan upload my files to the cloud?</b></summary>

No. Embedding and search run locally on the CPU. The only network features are
the one-time model download (`saan fetch-model`) and the optional, off-by-default
Jev router, whose data sharing is controlled by the [privacy level](#jev-smart-routing-optional).
</details>

<details>
<summary><b>Do I need a GPU?</b></summary>

No. saan is CPU-only by design — on tested hardware the CPU path was ~13× faster
than DirectML for this workload.
</details>

<details>
<summary><b>How is this different from Windows Search or File Explorer?</b></summary>

File Explorer matches names; saan also matches **meaning** (semantic search
with EmbeddingGemma), **content** (regex grep) and **path patterns** (glob) from
a single hotkey-driven search box.
</details>

<details>
<summary><b>Which platforms are supported?</b></summary>

Windows. The installer, Credential Manager key storage and skipped system
folders target Windows.
</details>

---

## Author

**Jimuelle Patron** — creator and maintainer of saan.

- GitHub: [@Jimuelle07](https://github.com/Jimuelle07)
- Project: [github.com/Jimuelle07/saan](https://github.com/Jimuelle07/saan)

If saan helps you find your files, consider starring the repository.

---

## License

Copyright © 2026 Jimuelle Patron. Licensed under the **Apache License 2.0** —
see [`LICENSE`](LICENSE).

**Model license:** the app code is Apache-2.0, but the EmbeddingGemma weights
downloaded by `saan fetch-model` (from
[`onnx-community/embeddinggemma-300m-ONNX`](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX),
derived from `google/embeddinggemma-300m`) are governed by Google's
[Gemma Terms of Use](https://ai.google.dev/gemma/terms) and are **not** covered
by this repository's license; they are not redistributed in the repo.

<sub>Keywords: local file search, semantic file search, AI file finder, desktop
file launcher, offline search, privacy-first, Windows, Rust, Tauri,
EmbeddingGemma, ONNX Runtime, grep, glob — by Jimuelle Patron.</sub>
