# saan — idea

*saan* is Filipino for "where": the question you ask when looking for a file.

A privacy-first, local launcher that replaces slow exact-string file exploration
(e.g. Windows File Explorer search) with three fast ways to find a file:

| Mode | Use when | Example query |
|---|---|---|
| **Semantic** | You remember what the file is *about* | `how long to proof bread dough overnight` |
| **Grep** | You remember text *inside* the file | `grep: fn dijkstra` · `/TODO\(.*\)/` |
| **Glob** | You remember the *name or path shape* | `glob: **/*.pdf` · `meeting-notes.md` |

Files that share a name (two `README.md`, three `todo.txt`) are always shown with
their full, distinct path — plus size, modification date and type — so they can be
told apart.

## Principles

- **Local by default.** Files are embedded on-device with
  [EmbeddingGemma](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX)
  (300M params, 4-bit ONNX `model_q4.onnx`, ~197 MB). No file data leaves the machine unless the user
  opts into the Jev router.
- **Fast.** Rust backend, brute-force cosine search over an in-memory index,
  target p95 < 100 ms per warm query.
- **Lightweight.** One Tauri binary + one model folder. No Python, no server, no GPU
  required. Plain TypeScript UI (no framework). Footprint counts: the ~197 MB
  model is loaded lazily on the first semantic query and dropped again after a
  few idle minutes, so an unused launcher stays small.
- **Optional typed decisions.** [Jev](https://docs.typesafe.ai/api) (TypeSafe AI
  System One) can choose the search mode for ambiguous queries and pick between
  near-duplicate candidates. Off by default; three privacy levels control what is sent.

## Decision-layer history

- Cloudflare **Clef** / **Clef-Flash** were evaluated first. Clef is a 27B model
  (55 GB of weights) that ships as custom Transformers/PyTorch code tested on an H200;
  Clef-Flash is 9B. Neither fits the "lightweight for anyone" goal on a 6 GB laptop GPU,
  so the hosted **Jev** API (same SystemOne request/response contract) was chosen,
  behind an opt-in privacy switch.

## Status

See `docs/system-design` for the architecture and the verification log.
