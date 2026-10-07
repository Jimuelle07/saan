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
with their full, distinct relative path so you can tell them apart, together
with their size, modification date and type, so same-name files are easy to
distinguish at a glance.

## Features

- **Local by default.** Files are embedded on-device with
  [EmbeddingGemma](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX)
  (300M params, 4-bit ONNX, ~197 MB). No file data leaves the machine unless you opt into
  the Jev router.
- **Fast.** Rust backend, brute-force cosine search over an in-memory index,
  target p95 < 100 ms per warm query.
- **Lightweight.** One Tauri binary + one model folder. No Python, no server, no
  GPU required. Plain TypeScript UI (no framework). The ~197 MB model is **not**
  loaded at startup: the engine loads it lazily on the first semantic query and
  drops it again after `SAAN_IDLE_UNLOAD_SECS` (default 300) idle seconds, so an
  idle launcher stays small. Grep and glob queries never load the model.
- **CPU-only inference.** saan embeds on the CPU execution provider: measured on
  an RTX 4050 Laptop + i5-13420H, the DirectML GPU path was ~13× slower (p95
  836–1072 ms vs 76–80 ms CPU, uncached `saan bench fixtures/eval.json`), so the
  GPU option was removed entirely.
- **Index all your folders.** Pick any number of root folders; indexing runs in
  the background with live progress (done/total, ETA, current file) and
  Start/Cancel controls. Long runs save a partial index as they go, so search
  works while indexing continues, and starting again **resumes** — unchanged
  files are reused, not re-embedded. Files larger than the max file size
  (default 10 MB) are hidden from semantic, grep and glob results everywhere,
  and heavy/system folders (`node_modules`, `target`, `.git`, `Windows`,
  `Program Files`, `$Recycle.Bin`, …) are never walked.
- **Settings panel.** Gear button in the search bar (or `Ctrl+,`): four themes
  (blue, violet, green, orange — accent **and** text colours follow the theme),
  folder list, max file size, index speed, Jev key (stored in the Windows
  Credential Manager, never on disk) and privacy levels.
- **Three modes, one box.** A local router picks semantic vs grep vs glob from
  the query, with explicit prefixes to override it.
- **Optional typed decisions.** [Jev](https://docs.typesafe.ai/api) (TypeSafe AI
  System One) can choose the search mode for ambiguous queries and pick between
  near-duplicate candidates. Off by default; three privacy levels control what
  is sent.
- **Global hotkey.** `Ctrl+Shift+Space` toggles the launcher; type, hit Enter.

## Install (Windows)

PowerShell 5+. Builds the desktop app with `npm run tauri build`, which embeds the
frontend; a plain `cargo build -p saan-app` would point the window at the dev
server instead. It also builds the CLI with cargo. It stops any running launcher,
copies `saan.exe` and `saan-app.exe` into `%LOCALAPPDATA%\saan\bin`, and copies or
fetches the model into `%LOCALAPPDATA%\saan\models\embeddinggemma-300m`. Finally it
adds that `bin` folder to your **user** `PATH`, keeping `%VAR%` entries and the
registry value type unchanged:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/install.ps1
```

Then open a **new** terminal and type:

```sh
saan
```

With **no subcommand**, `saan` opens the desktop launcher. It spawns
`saan-app.exe` (next to `saan.exe`, or `$SAAN_APP`) detached with `SAAN_SHOW=1`,
then exits. Before that it hands its foreground right to the launcher, so the
window comes up focused rather than behind the terminal. The window appears once
the page has loaded. The app is single-instance: running `saan` again focuses
the existing window instead of starting a second process.

```powershell
# Reuse the existing target\release binaries (no rebuild):
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -SkipBuild

# Uninstall: drop bin\ from the user PATH and delete bin\ (keeps models + app data):
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Uninstall

# ...and delete the whole install root, including models and app data:
powershell -ExecutionPolicy Bypass -File scripts/install.ps1 -Uninstall -Purge
```

## Quick start (from source)

Requires the Rust toolchain and Node.js.

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

Other CLI commands: `grep <pattern>` and `glob <pattern>` take `--root <dir>`
(repeatable, default `.`) plus `--max-file-mb <n>` (default 10); `eval <file>`
(top-k hit rate) and `bench <file>` (warm p95 latency).
Use `--index <dir>` to point at a different index (default `$SAAN_INDEX_DIR` or
`./.saan/index`). Run with no subcommand (`cargo run --release -p saan-cli`) to
open the desktop launcher from `target/release/saan-app.exe` (use `SAAN_APP` to
point elsewhere).

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
run, enter a folder to index (or set `SAAN_ROOT` to index one at startup);
afterwards manage folders from the settings panel.

### Settings

Press the gear in the search bar (or `Ctrl+,`) to open settings; `Esc` closes
settings first, before hiding the window.

- **Appearance** — four themes: blue, violet, green, orange. Accent **and** text
  colours follow the selection.
- **Folders** — the indexed roots: remove one, "Add folder…" (native picker), or
  one-click add of Documents/Desktop/Downloads.
- **Max file size** — files larger than this (MB, default 10) are hidden from
  semantic, grep and glob results everywhere.
- **Index speed** — `background` (2 threads) or `fast` (all cores).
- **Index** — Start/Cancel with a progress bar, done/total, ETA and the current
  file. Long runs save a partial index every 200 embedded files so search works
  during the run; starting again resumes by reusing unchanged files.
- **Jev** — save or remove the API key; it is stored in the **Windows
  Credential Manager** (service `saan`, user `jev-api-key`), never in
  `config.json` or logs, and the `JEV_API_KEY` env var overrides it when set.
  Plus the enable toggle and privacy level A/B/C with one-line descriptions
  (Jev "adds ~0.5 s to ambiguous searches"; level A sends only the query).

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

Jev is off by default. Turn it on either from the settings panel — save the API
key there (stored in the **Windows Credential Manager**, service `saan`, user
`jev-api-key`; never written to `config.json` or logs) and flip the enable
toggle — or through the environment: when **both** `SAAN_JEV` is
`on`/`1`/`true` and `JEV_API_KEY` are set, the environment (and its key)
overrides the saved settings. Requests go to `POST {base}/v1/systemone` with a
2500 ms timeout.

| Privacy level | `SAAN_JEV_PRIVACY` | Routing request | Candidate pick request |
|---|---|---|---|
| `a` (default) | `a` / `query` | query text | never sent |
| `b` | `b` / `paths` | query text | query + candidate relative paths |
| `c` | `c` / `snippets` | query text | query + candidate relative paths + short snippets |

The level is also choosable in the settings panel (default `a`); the env
variable above applies when the environment path is active. Absolute paths, file
contents beyond the snippet, and index data are never sent.

| Variable | Purpose |
|---|---|
| `SAAN_JEV` | `on`/`1`/`true` enables the Jev router. |
| `JEV_API_KEY` | Bearer token for the env path; overrides the key saved in the Windows Credential Manager. The CLI reads only this environment variable. |
| `SAAN_JEV_PRIVACY` | `a`/`query` (default), `b`/`paths`, `c`/`snippets`. |
| `SAAN_JEV_URL` | API base URL (default `https://api.typesafe.ai`). |
| `SAAN_JEV_MODEL` | Model name (default `jev-latest`). |

Other environment variables:

| Variable | Purpose |
|---|---|
| `SAAN_MODEL_DIR` | Model folder (overrides discovery). |
| `SAAN_INDEX_DIR` | CLI index folder (same as `--index`). |
| `SAAN_ROOT` | Root folder to (re)index when the app starts (`roots: [SAAN_ROOT]`). |
| `SAAN_SHOW` | Show the window at launch; the CLI sets it to `1` when it spawns the app. |
| `SAAN_APP` | Path to `saan-app.exe` for `saan` with no subcommand (default: next to `saan.exe`). |
| `SAAN_IDLE_UNLOAD_SECS` | Seconds of no semantic use before the app unloads the model (default 300; `0` keeps it loaded). |

Search results carry metadata for every hit: `size` (bytes), `modified` (Unix
seconds, `0` if unknown) and `ext` (lowercase extension without the dot, empty
if none), alongside `rel`, `path`, `name`, `score`, `line`, `snippet` and
`same_name`. Indexing skips `node_modules`, `target`, `__pycache__`, `.venv`,
`venv`, `.git`, `$Recycle.Bin`, `System Volume Information`, `Windows`,
`Program Files`, `Program Files (x86)`, `ProgramData` and `AppData`
directories, on top of the usual gitignore/hidden-file rules, and files larger
than the max file size (default 10 MB) are hidden from semantic, grep and glob
results everywhere.

See `docs/system-design` for the full architecture and the verification log.

## License

Apache-2.0. See [`LICENSE`](LICENSE).

**Model license:** the app code is Apache-2.0, but the EmbeddingGemma weights
downloaded by `saan fetch-model` (from
[`onnx-community/embeddinggemma-300m-ONNX`](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX),
derived from `google/embeddinggemma-300m`) are governed by Google's
[Gemma Terms of Use](https://ai.google.dev/gemma/terms) and are **not** covered
by this repository's license; they are not redistributed in the repo.
