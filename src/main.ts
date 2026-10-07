import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

const THEMES = ["blue", "violet", "green", "orange"] as const;
const PRIVACIES = ["a", "b", "c"] as const;
type Theme = (typeof THEMES)[number];
type Privacy = (typeof PRIVACIES)[number];
type IndexSpeed = "background" | "fast";

interface Settings {
  roots: string[];
  theme: Theme;
  jev_enabled: boolean;
  jev_privacy: Privacy;
  max_file_mb: number;
  index_speed: IndexSpeed;
}

interface SettingsView extends Settings {
  jev_key_saved: boolean;
  jev_env_override: boolean;
  suggested_roots: string[];
}

interface Status {
  ready: boolean;
  roots: string[];
  files: number;
  jev: boolean;
  hotkey: string;
  model_loaded: boolean;
  error: string | null;
  theme: Theme;
  indexing: boolean;
}

interface IndexProgress {
  state: "running" | "done" | "cancelled" | "error";
  done: number;
  total: number;
  current: string;
  filesIndexed: number;
  filesReused: number;
  elapsedMs: number;
  message: string;
}

function isTheme(v: string): v is Theme {
  return (THEMES as readonly string[]).includes(v);
}

// Apply the cached theme before anything renders.
const storedTheme = localStorage.getItem("saanTheme");
const hasStoredTheme = storedTheme !== null && isTheme(storedTheme);
document.documentElement.dataset.theme = hasStoredTheme ? storedTheme : "blue";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const queryEl = $<HTMLInputElement>("query");
const resultsEl = $<HTMLUListElement>("results");
const modeEl = $("mode");
const timingEl = $("timing");
const statusEl = $("status");
const gearEl = $<HTMLButtonElement>("gear");
const settingsEl = $<HTMLElement>("settings");
const loadingEl = $<HTMLElement>("settings-loading");
const rootsEl = $<HTMLUListElement>("roots");
const addFolderEl = $<HTMLButtonElement>("add-folder");
const addSuggestedEl = $<HTMLButtonElement>("add-suggested");
const maxMbEl = $<HTMLInputElement>("max-mb");
const speedEl = $<HTMLSelectElement>("speed");
const startEl = $<HTMLButtonElement>("start-index");
const cancelEl = $<HTMLButtonElement>("cancel-index");
const fillEl = $<HTMLElement>("idx-fill");
const countEl = $<HTMLElement>("idx-count");
const etaEl = $<HTMLElement>("idx-eta");
const currentEl = $<HTMLElement>("idx-current");
const jevEnableEl = $<HTMLInputElement>("jev-enable");
const jevKeyEl = $<HTMLInputElement>("jev-key");
const jevSaveEl = $<HTMLButtonElement>("jev-save");
const jevRemoveEl = $<HTMLButtonElement>("jev-remove");
const jevStateEl = $<HTMLElement>("jev-state");
const jevEnvEl = $<HTMLElement>("jev-env");
const swatchEls = THEMES.map((theme) => $<HTMLButtonElement>(`swatch-${theme}`));
const privacyEls = PRIVACIES.map((level) => $<HTMLButtonElement>(`priv-${level}`));

let hits: Hit[] = [];
let selected = 0;
let seq = 0;
let debounce: number | undefined;
let statusTimer: number | undefined;
let lastStatus: Status | null = null;
let indexing = false;
let progress: IndexProgress | null = null;
let settingsOpen = false;
let autoOpenDismissed = false;
let view: SettingsView | null = null;
let viewTicket = 0;
let settingsLoaded = false;
const draft: Settings = {
  roots: [],
  theme: "blue",
  jev_enabled: false,
  jev_privacy: "a",
  max_file_mb: 10,
  index_speed: "background",
};

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

// ---------- settings ----------

function asTheme(v: string): Theme {
  return isTheme(v) ? v : "blue";
}

function asPrivacy(v: string): Privacy {
  const t = v.trim().toLowerCase();
  if (t === "b" || t === "paths") return "b";
  if (t === "c" || t === "snippets") return "c";
  return "a";
}

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem("saanTheme", theme);
}

function adoptSettings(res: SettingsView): void {
  draft.roots = [...res.roots];
  draft.theme = asTheme(res.theme);
  draft.jev_enabled = res.jev_enabled;
  draft.jev_privacy = asPrivacy(res.jev_privacy);
  draft.max_file_mb = res.max_file_mb > 0 ? res.max_file_mb : 10;
  draft.index_speed = res.index_speed === "fast" ? "fast" : "background";
}

function setView(res: SettingsView): void {
  viewTicket++;
  view = res;
}

// First response wins the draft; the cached theme (localStorage) wins over config.
function adoptIfFirst(res: SettingsView): void {
  if (settingsLoaded) return;
  settingsLoaded = true;
  adoptSettings(res);
  if (!hasStoredTheme) applyTheme(asTheme(res.theme));
  draft.theme = asTheme(document.documentElement.dataset.theme ?? "blue");
}

async function loadSettings(): Promise<void> {
  const ticket = ++viewTicket;
  try {
    const res = await invoke<SettingsView>("get_settings");
    if (ticket !== viewTicket) return; // a fresher response already landed
    setView(res);
    adoptIfFirst(res);
    syncSettings();
  } catch (err) {
    statusEl.textContent = String(err);
  }
}

async function save(): Promise<void> {
  if (!view) return;
  const ticket = ++viewTicket;
  try {
    const res = await invoke<SettingsView>("save_settings", { settings: { ...draft } });
    if (ticket !== viewTicket) return;
    setView(res);
    adoptSettings(res);
    syncSettings();
    renderFooter();
  } catch (err) {
    statusEl.textContent = String(err);
  }
}

function pickTheme(theme: Theme): void {
  applyTheme(theme);
  draft.theme = theme;
  syncSettings();
  void save();
}

async function addRoot(root: string): Promise<void> {
  if (draft.roots.includes(root)) return;
  draft.roots = [...draft.roots, root];
  syncSettings();
  await save();
}

async function removeRoot(root: string): Promise<void> {
  draft.roots = draft.roots.filter((r) => r !== root);
  syncSettings();
  await save();
}

function syncIndexButtons(): void {
  startEl.disabled = indexing;
  cancelEl.disabled = !indexing;
}

function syncSettings(): void {
  const theme = document.documentElement.dataset.theme;
  THEMES.forEach((t, i) => {
    const on = t === theme;
    swatchEls[i].classList.toggle("selected", on);
    swatchEls[i].setAttribute("aria-pressed", String(on));
  });

  rootsEl.replaceChildren();
  if (draft.roots.length === 0) {
    rootsEl.append(el("li", "hint", "No folders yet — add one below."));
  } else {
    draft.roots.forEach((root) => {
      const li = document.createElement("li");
      const path = el("span", "path", root);
      path.title = root;
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "Remove";
      remove.setAttribute("aria-label", `Remove ${root}`);
      remove.addEventListener("click", () => {
        void removeRoot(root);
      });
      li.append(path, remove);
      rootsEl.append(li);
    });
  }

  const ready = view !== null;
  addSuggestedEl.hidden = !view || !view.suggested_roots.some((s) => !draft.roots.includes(s));
  loadingEl.hidden = ready;
  if (document.activeElement !== maxMbEl) maxMbEl.value = String(draft.max_file_mb);
  speedEl.value = draft.index_speed;
  jevEnableEl.checked = draft.jev_enabled;
  jevStateEl.textContent = view?.jev_key_saved ? "Key saved" : "No key";
  jevStateEl.classList.toggle("saved", !!view?.jev_key_saved);
  jevEnvEl.hidden = !view?.jev_env_override;
  PRIVACIES.forEach((level, i) => {
    const on = draft.jev_privacy === level;
    privacyEls[i].classList.toggle("selected", on);
    privacyEls[i].setAttribute("aria-pressed", String(on));
  });

  const controls: Array<HTMLButtonElement | HTMLInputElement | HTMLSelectElement> = [
    ...swatchEls,
    ...privacyEls,
    addFolderEl,
    addSuggestedEl,
    maxMbEl,
    speedEl,
    jevEnableEl,
  ];
  controls.forEach((control) => {
    control.disabled = !ready;
  });
  syncIndexButtons();
}

function openSettings(): void {
  if (settingsOpen) return;
  settingsOpen = true;
  settingsEl.hidden = false;
  resultsEl.hidden = true;
  if (!view) void loadSettings();
}

function closeSettings(): void {
  if (!settingsOpen) return;
  settingsOpen = false;
  settingsEl.hidden = true;
  resultsEl.hidden = false;
  if (lastStatus && !lastStatus.ready) autoOpenDismissed = true;
}

function toggleSettings(): void {
  if (settingsOpen) closeSettings();
  else openSettings();
}

// ---------- index progress ----------

function fmtDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
  if (m > 0) return `${m}:${String(sec).padStart(2, "0")}`;
  return `${sec}s`;
}

function fmtEta(done: number, total: number, elapsedMs: number): string {
  if (done <= 0 || total <= done || elapsedMs <= 0) return "";
  return fmtDuration(((total - done) * elapsedMs) / done);
}

function progressLine(): string {
  if (!progress) return "Indexing…";
  const eta = fmtEta(progress.done, progress.total, progress.elapsedMs);
  return `Indexing… ${progress.done} / ${progress.total}${eta ? ` · ETA ${eta}` : ""}`;
}

function applyProgress(p: IndexProgress): void {
  progress = p;
  indexing = p.state === "running";
  const pct = p.state === "done" ? 100 : p.total > 0 ? Math.min(100, (p.done / p.total) * 100) : 0;
  fillEl.style.width = `${pct}%`;
  countEl.textContent = `${p.done} / ${p.total}`;
  if (p.state === "running") {
    const eta = fmtEta(p.done, p.total, p.elapsedMs);
    etaEl.textContent = eta ? `ETA ${eta}` : "ETA —";
    currentEl.textContent = p.current;
    currentEl.title = p.current;
  } else if (p.state === "done") {
    etaEl.textContent = `done in ${fmtDuration(p.elapsedMs)}`;
    currentEl.textContent = "";
    currentEl.title = "";
  } else {
    etaEl.textContent = p.state === "cancelled" ? "cancelled" : "error";
    currentEl.textContent = p.message;
    currentEl.title = p.message;
  }
  syncIndexButtons();
  renderFooter();
  if (p.state !== "running") {
    const summary =
      p.state === "done"
        ? `Indexed ${p.filesIndexed} files · ${p.filesReused} reused · ${fmtDuration(p.elapsedMs)}`
        : p.state === "cancelled"
          ? "Indexing cancelled."
          : p.message || "Indexing failed.";
    void refreshStatus().then(() => {
      statusEl.textContent = summary;
    });
  }
}

// ---------- status / footer ----------

function renderFooter(): void {
  if (indexing) {
    statusEl.textContent = progressLine();
  } else if (lastStatus?.error) {
    statusEl.textContent = lastStatus.error;
  } else if (lastStatus?.ready) {
    const s = lastStatus;
    const folders = s.roots.join(", ");
    const jev = s.jev ? "Jev on" : "local only";
    const model = s.model_loaded ? "model loaded" : "model sleeping";
    statusEl.textContent = `${s.files} files${folders ? ` · ${folders}` : ""} · ${jev} · ${model} · ${s.hotkey} to toggle · Enter open · Ctrl+Enter reveal`;
  } else if (lastStatus) {
    // Not ready and not indexing: there is no usable index for the saved folders.
    statusEl.textContent =
      lastStatus.roots.length === 0
        ? `No folders yet: add one in Settings (Ctrl+,), then Start indexing · ${lastStatus.hotkey} toggles this window`
        : `No index for these folders yet: open Settings (Ctrl+,) and Start indexing · ${lastStatus.hotkey} toggles this window`;
  }
}

async function refreshStatus(): Promise<void> {
  let ready = false;
  try {
    const s = await invoke<Status>("status");
    lastStatus = s;
    ready = s.ready;
    if (s.ready) autoOpenDismissed = false;
    if (!s.ready && !indexing && !s.indexing && !settingsOpen && !autoOpenDismissed) openSettings();
    renderFooter();
  } catch (err) {
    lastStatus = null;
    statusEl.textContent = String(err);
  }
  if (ready) {
    if (statusTimer !== undefined) {
      window.clearTimeout(statusTimer);
      statusTimer = undefined;
    }
  } else if (statusTimer === undefined) {
    statusTimer = window.setTimeout(() => {
      statusTimer = undefined;
      void refreshStatus();
    }, 1000);
  }
}

// ---------- wiring ----------

queryEl.addEventListener("input", () => {
  window.clearTimeout(debounce);
  debounce = window.setTimeout(() => run(queryEl.value), 40);
});

document.addEventListener("keydown", (e) => {
  if (e.ctrlKey && !e.altKey && !e.metaKey && (e.key === "," || e.code === "Comma")) {
    e.preventDefault();
    toggleSettings();
    return;
  }
  if (e.key === "Escape") {
    if (settingsOpen) {
      e.preventDefault();
      closeSettings();
    } else {
      invoke("hide_window");
    }
    return;
  }
  const target = e.target;
  const inOtherField =
    (target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement) &&
    target !== queryEl;
  if (inOtherField) return;
  if (e.key === "ArrowDown") {
    e.preventDefault();
    select(selected + 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    select(selected - 1);
  } else if (e.key === "Enter" && document.activeElement === queryEl) {
    e.preventDefault();
    open(selected, e.ctrlKey);
  }
});

gearEl.addEventListener("click", () => toggleSettings());

THEMES.forEach((theme, i) => {
  swatchEls[i].addEventListener("click", () => pickTheme(theme));
});

PRIVACIES.forEach((level, i) => {
  privacyEls[i].addEventListener("click", () => {
    draft.jev_privacy = level;
    syncSettings();
    void save();
  });
});

addFolderEl.addEventListener("click", async () => {
  try {
    const folder = await invoke<string | null>("pick_folder");
    if (folder) await addRoot(folder);
  } catch (err) {
    statusEl.textContent = String(err);
  }
});

addSuggestedEl.addEventListener("click", () => {
  if (!view) return;
  const merged = [...draft.roots];
  view.suggested_roots.forEach((root) => {
    if (!merged.includes(root)) merged.push(root);
  });
  draft.roots = merged;
  syncSettings();
  void save();
});

maxMbEl.addEventListener("change", () => {
  const n = Math.round(Number(maxMbEl.value));
  draft.max_file_mb = Number.isFinite(n) && n > 0 ? n : 10;
  void save();
});

speedEl.addEventListener("change", () => {
  draft.index_speed = speedEl.value === "fast" ? "fast" : "background";
  void save();
});

jevEnableEl.addEventListener("change", () => {
  draft.jev_enabled = jevEnableEl.checked;
  void save();
});

jevSaveEl.addEventListener("click", async () => {
  const key = jevKeyEl.value.trim();
  if (!key) {
    statusEl.textContent = "Enter a Jev API key first.";
    return;
  }
  const ticket = ++viewTicket;
  try {
    const res = await invoke<SettingsView>("set_jev_key", { key });
    if (ticket !== viewTicket) return;
    setView(res);
    adoptIfFirst(res);
    jevKeyEl.value = ""; // the key is never echoed back
    syncSettings();
    renderFooter();
  } catch (err) {
    statusEl.textContent = String(err);
  }
});

jevRemoveEl.addEventListener("click", async () => {
  const ticket = ++viewTicket;
  try {
    const res = await invoke<SettingsView>("set_jev_key", { key: null });
    if (ticket !== viewTicket) return;
    setView(res);
    adoptIfFirst(res);
    jevKeyEl.value = "";
    syncSettings();
    renderFooter();
  } catch (err) {
    statusEl.textContent = String(err);
  }
});

startEl.addEventListener("click", async () => {
  try {
    await invoke("start_index");
    indexing = true;
    progress = null;
    fillEl.style.width = "0%";
    countEl.textContent = "0 / 0";
    etaEl.textContent = "ETA —";
    currentEl.textContent = "";
    currentEl.title = "";
    syncIndexButtons();
    renderFooter();
  } catch (err) {
    statusEl.textContent = String(err);
  }
});

cancelEl.addEventListener("click", async () => {
  try {
    await invoke("cancel_index");
  } catch (err) {
    statusEl.textContent = String(err);
  }
});

window.addEventListener("focus", () => {
  queryEl.focus();
  queryEl.select();
  void refreshStatus();
});

listen<IndexProgress>("index-progress", (event) => applyProgress(event.payload)).catch((err) => {
  statusEl.textContent = String(err);
});

syncSettings();
void loadSettings();
void refreshStatus();
