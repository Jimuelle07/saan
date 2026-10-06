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

function el(tag: string, cls: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.className = cls;
  node.textContent = text;
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
    li.append(title, el("div", "rel", hit.line ? `${hit.rel}:${hit.line}` : hit.rel));
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
  statusEl.textContent = s.error
    ? s.error
    : s.ready
      ? `${s.files} files · ${s.root ?? ""} · ${jev} · ${s.hotkey} to toggle · Enter open · Ctrl+Enter reveal`
      : `Loading index… (${s.hotkey} toggles this window)`;
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
window.setInterval(refreshStatus, 3000);
