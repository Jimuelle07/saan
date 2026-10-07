import { invoke } from "@tauri-apps/api/core";

type Mode = "semantic" | "grep" | "glob";

interface Hit {
  rel: string;
  path: string;
  name: string;
  score: number;
  line: number | null;
  snippet: string;
  same_name: boolean;
  size: number;
  modified: number;
  ext: string;
}

interface SearchResponse {
  route: { mode: Mode; query: string; confidence: number; source: "explicit" | "rules" | "jev" };
  hits: Hit[];
  jev_calls: number;
  elapsed_ms: number;
}

interface Status {
  ready: boolean;
  root: string | null;
  files: number;
  jev: boolean;
  hotkey: string;
  model_loaded: boolean;
  error: string | null;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const queryEl = $<HTMLInputElement>("query");
const resultsEl = $<HTMLUListElement>("results");
const modeEl = $("mode");
const timingEl = $("timing");
const statusEl = $("status");
const setupEl = $<HTMLFormElement>("setup");
const rootEl = $<HTMLInputElement>("root");

let hits: Hit[] = [];
let selected = 0;
let seq = 0;
let debounce: number | undefined;
let statusTimer: number | undefined;

function el(tag: string, cls: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.className = cls;
  node.textContent = text;
  return node;
}

const EXT_LABELS: Record<string, string> = {
  md: "Markdown",
  txt: "Text",
  pdf: "PDF",
  rs: "Rust",
  py: "Python",
  ts: "TypeScript",
  js: "JavaScript",
  go: "Go",
  sql: "SQL",
  json: "JSON",
  toml: "TOML",
};

function fmtSize(bytes: number): string {
  if (bytes <= 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

function fmtModified(unix: number): string {
  if (unix <= 0) return "";
  const then = unix * 1000;
  const diff = Date.now() - then;
  if (diff < 60_000) return "just now";
  const min = Math.floor(diff / 60_000);
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} h ago`;
  const days = Math.floor(diff / 86_400_000);
  if (days <= 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  return new Date(then).toLocaleDateString();
}

function metaEl(hit: Hit): HTMLElement {
  const meta = el("div", "meta", "");
  const label = hit.ext ? (EXT_LABELS[hit.ext] ?? hit.ext.toUpperCase()) : "File";
  const pieces: HTMLElement[] = [el("span", "", label)];
  const size = fmtSize(hit.size);
  if (size) pieces.push(el("span", "", size));
  const when = fmtModified(hit.modified);
  if (when) {
    const span = el("span", "", when);
    span.title = new Date(hit.modified * 1000).toLocaleString();
    pieces.push(span);
  }
  pieces.forEach((piece, i) => {
    if (i > 0) meta.append(document.createTextNode(" · "));
    meta.append(piece);
  });
  return meta;
}

function sharedDirCount(name: string): number {
  const group = hits.filter((h) => h.name === name);
  if (group.length === 0) return 0;
  const dirs = group.map((h) => h.rel.split(/[\\/]/).slice(0, -1));
  const first = dirs[0];
  let n = 0;
  while (n < first.length && dirs.every((d) => n < d.length && d[n] === first[n])) n++;
  return n;
}

function relEl(hit: Hit): HTMLElement {
  const node = el("div", "rel", "");
  const pieces = hit.rel.split(/([\\/])/); // alternating: segment, separator, segment, …
  const segCount = Math.ceil(pieces.length / 2);
  const dirCount = segCount - 1;
  const shared = hit.same_name ? sharedDirCount(hit.name) : dirCount;
  pieces.forEach((piece, i) => {
    if (i % 2 === 1) {
      node.append(document.createTextNode(piece));
      return;
    }
    const seg = i / 2;
    if (seg < dirCount && seg >= shared) node.append(el("strong", "diff", piece));
    else node.append(document.createTextNode(piece));
  });
  if (hit.line) node.append(document.createTextNode(`:${hit.line}`));
  return node;
}

function render(): void {
  resultsEl.replaceChildren();
  if (hits.length === 0) {
    if (queryEl.value.trim()) resultsEl.append(el("li", "empty", "No matches"));
    return;
  }
  hits.forEach((hit, i) => {
    const li = document.createElement("li");
    li.setAttribute("role", "option");
    li.className = i === selected ? "selected" : "";
    const title = el("div", "", "");
    title.append(el("span", "name", hit.name));
    if (hit.same_name) title.append(el("span", "badge", "same name"));
    li.append(title, relEl(hit), metaEl(hit));
    if (hit.snippet) li.append(el("div", "snippet", hit.snippet));
    li.addEventListener("mousemove", () => select(i));
    li.addEventListener("click", () => open(i, false));
    resultsEl.append(li);
  });
}

function select(i: number): void {
  if (i === selected || i < 0 || i >= hits.length) return;
  selected = i;
  render();
  resultsEl.children[i]?.scrollIntoView({ block: "nearest" });
}

async function run(query: string): Promise<void> {
  const mine = ++seq;
  if (!query.trim()) {
    hits = [];
    timingEl.textContent = "";
    render();
    return;
  }
  try {
    const res = await invoke<SearchResponse>("search", { query, k: 12 });
    if (mine !== seq) return; // a newer keystroke won
    hits = res.hits;
    selected = 0;
    modeEl.textContent = res.route.source === "jev" ? `${res.route.mode} · jev` : res.route.mode;
    timingEl.textContent = `${res.elapsed_ms.toFixed(0)} ms`;
    render();
  } catch (err) {
    if (mine !== seq) return;
    hits = [];
    render();
    statusEl.textContent = String(err);
  }
}

async function open(i: number, reveal: boolean): Promise<void> {
  const hit = hits[i];
  if (!hit) return;
  try {
    await invoke("open_path", { path: hit.path, reveal });
    queryEl.value = "";
    hits = [];
    render();
  } catch (err) {
    statusEl.textContent = String(err);
  }
}

async function refreshStatus(): Promise<void> {
  const s = await invoke<Status>("status");
  setupEl.hidden = s.ready;
  if (s.root && !rootEl.value) rootEl.value = s.root;
  const jev = s.jev ? "Jev on" : "local only";
  const model = s.model_loaded ? "model loaded" : "model sleeping";
  statusEl.textContent = s.error
    ? s.error
    : s.ready
      ? `${s.files} files · ${s.root ?? ""} · ${jev} · ${model} · ${s.hotkey} to toggle · Enter open · Ctrl+Enter reveal`
      : `Loading index… (${s.hotkey} toggles this window)`;
  if (s.ready) {
    if (statusTimer !== undefined) {
      window.clearTimeout(statusTimer);
      statusTimer = undefined;
    }
  } else if (statusTimer === undefined) {
    statusTimer = window.setTimeout(() => {
      statusTimer = undefined;
      refreshStatus();
    }, 1000);
  }
}

queryEl.addEventListener("input", () => {
  window.clearTimeout(debounce);
  debounce = window.setTimeout(() => run(queryEl.value), 40);
});

document.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") { e.preventDefault(); select(selected + 1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); select(selected - 1); }
  else if (e.key === "Enter" && document.activeElement === queryEl) { e.preventDefault(); open(selected, e.ctrlKey); }
  else if (e.key === "Escape") { invoke("hide_window"); }
});

setupEl.addEventListener("submit", async (e) => {
  e.preventDefault();
  statusEl.textContent = "Indexing… this embeds every file locally and can take a while.";
  try {
    const files = await invoke<number>("index_folder", { root: rootEl.value });
    statusEl.textContent = `Indexed ${files} files.`;
  } catch (err) {
    statusEl.textContent = String(err);
  }
  refreshStatus();
});

window.addEventListener("focus", () => {
  queryEl.focus();
  queryEl.select();
  refreshStatus();
});

refreshStatus();
