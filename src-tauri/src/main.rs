// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use saan_core::embed::{find_model_dir, EmbedOptions, Embedder, MODEL_DIR_NAME};
use saan_core::index::{BuildEvent, BuildOptions, BuildStats};
use saan_core::jev::{JevClient, Privacy};
use saan_core::{Engine, Index, Scope, SearchResponse};
use serde::{Deserialize, Serialize};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Global hotkey that toggles the launcher.
const HOTKEY_LABEL: &str = "Ctrl+Shift+Space";

/// OS credential-store coordinates for the Jev API key. The key never touches
/// `config.json`, the log, or any other file.
const KEYRING_SERVICE: &str = "saan";
const KEYRING_USER: &str = "jev-api-key";

/// Set at startup when launched with `SAAN_SHOW`; consumed by the first page load.
static SHOW_ON_LOAD: AtomicBool = AtomicBool::new(false);

/// Set while the native folder picker is open: the window loses focus when the
/// modal dialog appears, and hide-on-blur must not close the launcher under it.
static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct AppState {
    /// The engine itself is reference-counted so background threads (model
    /// preload, indexing checkpoints, idle unload) can work on it without
    /// holding this lock.
    engine: Arc<RwLock<Option<Arc<Engine>>>>,
    /// Last load/index error, shown in the launcher footer.
    error: Arc<RwLock<Option<String>>>,
    /// True from `start_index` until the build thread finishes.
    indexing: Arc<AtomicBool>,
    /// Set by `cancel_index`; polled by the build callback.
    cancel_index: Arc<AtomicBool>,
}

/// Persisted application settings (`config.json` in the app data dir). Every
/// field has a default so unknown/older files still load; a legacy
/// `{"root": "..."}` file migrates to `roots: [root]`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    roots: Vec<PathBuf>,
    /// One of `blue`, `violet`, `green`, `orange`.
    theme: String,
    jev_enabled: bool,
    /// Privacy level: `a` | `b` | `c`.
    jev_privacy: String,
    max_file_mb: u64,
    /// `background` (2 threads) or `fast` (all cores).
    index_speed: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            theme: "blue".to_string(),
            jev_enabled: false,
            jev_privacy: "a".to_string(),
            max_file_mb: 10,
            index_speed: "background".to_string(),
        }
    }
}

/// `Settings` plus the read-only extras the settings panel shows.
#[derive(Serialize)]
struct SettingsView {
    #[serde(flatten)]
    settings: Settings,
    jev_key_saved: bool,
    /// `JEV_API_KEY` + `SAAN_JEV` are set, so they override the stored key.
    jev_env_override: bool,
    /// Existing Documents / Desktop / Downloads folders.
    suggested_roots: Vec<String>,
}

#[derive(Serialize)]
struct Status {
    ready: bool,
    roots: Vec<String>,
    files: usize,
    jev: bool,
    /// The ONNX model is in memory right now (it unloads while hidden).
    model_loaded: bool,
    hotkey: &'static str,
    error: Option<String>,
    theme: String,
    indexing: bool,
}

/// Payload of the `index-progress` event, at most four per second plus a final
/// event. `message` carries the failure text on `state == "error"`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexProgress {
    state: &'static str,
    done: usize,
    total: usize,
    current: String,
    files_indexed: usize,
    files_reused: usize,
    elapsed_ms: u64,
    message: String,
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("config.json"))
}

fn index_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("index"))
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    find_model_dir()
        .or_else(|| data_dir(app).ok().map(|d| d.join("models").join(MODEL_DIR_NAME)))
        .filter(|d| d.join("tokenizer.json").is_file())
        .ok_or_else(|| "EmbeddingGemma model not found: run `saan fetch-model` or set SAAN_MODEL_DIR".to_string())
}

/// Coerce out-of-range values (a hand-edited config, a future version) back to
/// something the app understands.
fn sanitize(mut settings: Settings) -> Settings {
    if !matches!(settings.theme.as_str(), "blue" | "violet" | "green" | "orange") {
        settings.theme = "blue".to_string();
    }
    if Privacy::parse(&settings.jev_privacy).is_none() {
        settings.jev_privacy = "a".to_string();
    }
    if !matches!(settings.index_speed.as_str(), "background" | "fast") {
        settings.index_speed = "background".to_string();
    }
    if settings.max_file_mb == 0 {
        settings.max_file_mb = 10;
    }
    settings
}

fn load_settings(app: &AppHandle) -> Settings {
    let Some(bytes) = config_path(app).ok().and_then(|p| std::fs::read(p).ok()) else {
        return Settings::default();
    };
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Settings::default();
    };
    // Legacy single-root config: `{"root": "C:\\..."}` -> `roots: [ ... ]`.
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("roots") {
            let roots = match obj.remove("root") {
                Some(serde_json::Value::String(root)) => vec![serde_json::Value::String(root)],
                _ => Vec::new(),
            };
            obj.insert("roots".to_string(), serde_json::Value::Array(roots));
        }
    }
    serde_json::from_value::<Settings>(value).map(sanitize).unwrap_or_default()
}

fn save_settings_file(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = config_path(app)?;
    std::fs::create_dir_all(path.parent().expect("config has parent")).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

/// The stored Jev key, if the credential store has a non-empty one.
fn stored_jev_key() -> Option<String> {
    keyring_entry().ok()?.get_password().ok().filter(|k| !k.trim().is_empty())
}

/// The Jev client to hand to the engine. `JEV_API_KEY`/`SAAN_JEV` win when set;
/// otherwise the key comes from the OS credential store and only when the user
/// enabled Jev in settings.
fn jev_client(settings: &Settings) -> Option<JevClient> {
    if let Some(client) = JevClient::from_env() {
        return Some(client);
    }
    if !settings.jev_enabled {
        return None;
    }
    let key = stored_jev_key()?;
    let privacy = Privacy::parse(&settings.jev_privacy).unwrap_or(Privacy::Query);
    Some(JevClient::new(key, privacy))
}

/// Swap in a fresh lazy engine, preserving the Jev client.
fn swap_engine(app: &AppHandle, index: Index, model_path: &Path, settings: &Settings) {
    let engine = Engine::new(index, model_path.to_path_buf(), jev_client(settings));
    *app.state::<AppState>().engine.write().expect("engine lock") = Some(Arc::new(engine));
}

fn apply_jev(app: &AppHandle, settings: &Settings) {
    let jev = jev_client(settings);
    let engine = app.state::<AppState>().engine.read().expect("engine lock").clone();
    if let Some(engine) = engine {
        engine.set_jev(jev);
    }
}

fn emit_progress(
    app: &AppHandle,
    start: Instant,
    state: &'static str,
    done: usize,
    total: usize,
    current: &str,
    files_indexed: usize,
    files_reused: usize,
    message: &str,
) {
    let _ = app.emit(
        "index-progress",
        IndexProgress {
            state,
            done,
            total,
            current: current.to_string(),
            files_indexed,
            files_reused,
            elapsed_ms: start.elapsed().as_millis() as u64,
            message: message.to_string(),
        },
    );
}

/// Index `settings.roots` on a dedicated thread: load the previous index,
/// embed with the configured thread options, and swap each checkpoint into
/// the live engine so search keeps working during a long run.
fn run_index(app: &AppHandle, settings: Settings) {
    let state = app.state::<AppState>();
    let cancel = state.cancel_index.clone();
    let indexing = state.indexing.clone();
    let start = Instant::now();

    emit_progress(app, start, "running", 0, 0, "", 0, 0, "");

    let outcome: Result<(usize, BuildStats), String> = (|| {
        let model_path = model_dir(app)?;
        let dir = index_dir(app)?;
        let previous = Index::load(&dir).ok();
        // Background indexing stays at two threads so the UI and searches keep
        // cores free; "fast" leaves the count to the runtime default.
        let threads = if settings.index_speed == "fast" { None } else { Some(2) };
        let embedder = Embedder::load_with(&model_path, EmbedOptions { threads })
            .map_err(|e| format!("{e:#}"))?;
        let scope = Scope::new(settings.roots.clone(), settings.max_file_mb.saturating_mul(1024 * 1024));
        let build_opts = BuildOptions { checkpoint_every: 200 };
        let mut last_emit = Instant::now();
        let mut done = 0usize;
        let mut reused_hint = 0usize;
        let (index, stats) = Index::build(&scope, &embedder, previous.as_ref(), &build_opts, |ev| {
            match ev {
                BuildEvent::File { done: d, total: t, rel } => {
                    done = d;
                    if last_emit.elapsed() >= Duration::from_millis(250) {
                        last_emit = Instant::now();
                        emit_progress(app, start, "running", d, t, rel, d, reused_hint, "");
                    }
                }
                BuildEvent::Checkpoint(index) => {
                    // Persist the partial index and swap it in: re-load it so the
                    // engine owns an index rather than borrowing the build's.
                    if let Ok(saved) = index.save(&dir).and_then(|_| Index::load(&dir)) {
                        reused_hint = saved.files.len().saturating_sub(done);
                        swap_engine(app, saved, &model_path, &settings);
                    }
                }
            }
            !cancel.load(Ordering::SeqCst)
        })
        .map_err(|e| format!("{e:#}"))?;
        let files = index.files.len();
        index.save(&dir).map_err(|e| format!("{e:#}"))?;
        swap_engine(app, index, &model_path, &settings);
        Ok((files, stats))
    })();

    match outcome {
        Ok((files, stats)) => {
            let state_name = if stats.cancelled { "cancelled" } else { "done" };
            emit_progress(app, start, state_name, files, files, "", stats.files_indexed, stats.files_reused, "");
            *state.error.write().expect("error lock") = None;
        }
        Err(message) => {
            emit_progress(app, start, "error", 0, 0, "", 0, 0, &message);
            *state.error.write().expect("error lock") = Some(message);
        }
    }
    indexing.store(false, Ordering::SeqCst);
}

/// Claim the indexing slot and start the build thread.
fn begin_index(app: &AppHandle, settings: Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.indexing.swap(true, Ordering::SeqCst) {
        return Err("Indexing is already running.".to_string());
    }
    state.cancel_index.store(false, Ordering::SeqCst);
    let handle = app.clone();
    std::thread::spawn(move || run_index(&handle, settings));
    Ok(())
}

/// Startup: load the saved index for the configured folders; if `SAAN_ROOT`
/// names a folder, adopt it as the only root and index it first.
fn startup(app: &AppHandle) {
    let result = (|| -> Result<(), String> {
        if let Some(root) = std::env::var_os("SAAN_ROOT").map(PathBuf::from) {
            let mut settings = load_settings(app);
            settings.roots = vec![root];
            save_settings_file(app, &settings)?;
            begin_index(app, settings)?;
            return Ok(());
        }
        let settings = load_settings(app);
        // No folders configured: leave the engine empty and let the frontend
        // open the settings panel. No model load happens at startup either way.
        if settings.roots.is_empty() {
            return Ok(());
        }
        if let Ok(index) = Index::load(&index_dir(app)?) {
            let model_path = model_dir(app)?;
            swap_engine(app, index, &model_path, &settings);
        }
        Ok(())
    })();
    if let Err(e) = result {
        *app.state::<AppState>().error.write().expect("error lock") = Some(e);
    }
}

fn suggested_roots(app: &AppHandle) -> Vec<String> {
    let paths = app.path();
    [paths.document_dir(), paths.desktop_dir(), paths.download_dir()]
        .into_iter()
        .flatten()
        .filter(|dir| dir.is_dir())
        .map(|dir| dir.to_string_lossy().into_owned())
        .collect()
}

fn settings_view(app: &AppHandle, settings: &Settings) -> SettingsView {
    SettingsView {
        settings: settings.clone(),
        jev_key_saved: stored_jev_key().is_some(),
        jev_env_override: JevClient::from_env().is_some(),
        suggested_roots: suggested_roots(app),
    }
}

#[tauri::command]
async fn get_settings(app: AppHandle) -> Result<SettingsView, String> {
    let settings = load_settings(&app);
    Ok(settings_view(&app, &settings))
}

#[tauri::command]
async fn save_settings(app: AppHandle, settings: Settings) -> Result<SettingsView, String> {
    let settings = sanitize(settings);
    save_settings_file(&app, &settings)?;
    // The key takes effect immediately; folders and max size apply on the next
    // indexing run.
    apply_jev(&app, &settings);
    Ok(settings_view(&app, &settings))
}

#[tauri::command]
async fn set_jev_key(app: AppHandle, key: Option<String>) -> Result<SettingsView, String> {
    let entry = keyring_entry()?;
    match key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => entry.set_password(key).map_err(|e| e.to_string())?,
        None => {
            // Removing a key that was never stored is not an error.
            let _ = entry.delete_credential();
        }
    }
    let settings = load_settings(&app);
    apply_jev(&app, &settings);
    Ok(settings_view(&app, &settings))
}

/// Native folder picker. Runs off the async runtime because it blocks until the
/// user closes the dialog; `DIALOG_OPEN` keeps hide-on-blur at bay meanwhile.
#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        DIALOG_OPEN.store(true, Ordering::SeqCst);
        let picked = app.dialog().file().blocking_pick_folder();
        DIALOG_OPEN.store(false, Ordering::SeqCst);
        picked.and_then(|fp| fp.into_path().ok()).map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
async fn start_index(app: AppHandle) -> Result<(), String> {
    let settings = load_settings(&app);
    if settings.roots.is_empty() {
        return Err("No folders configured: add one in Settings first.".to_string());
    }
    begin_index(&app, settings)
}

#[tauri::command]
fn cancel_index(state: State<'_, AppState>) {
    state.cancel_index.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: String, k: Option<usize>) -> Result<SearchResponse, String> {
    // Clone the `'static` Arc out of the borrowed command state so the blocking
    // task owns everything it touches; no lock guard crosses an await point.
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let engine = engine.read().expect("engine lock");
        let engine = engine.as_ref().ok_or("No index yet: choose a folder to index.")?;
        engine.search(&query, k.unwrap_or(12)).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn status(app: AppHandle, state: State<'_, AppState>) -> Result<Status, String> {
    let settings = load_settings(&app);
    let engine = state.engine.read().expect("engine lock");
    Ok(Status {
        ready: engine.is_some(),
        roots: settings.roots.iter().map(|r| r.display().to_string()).collect(),
        files: engine.as_ref().map_or(0, |e| e.index.files.len()),
        jev: engine.as_ref().is_some_and(|e| e.jev_enabled()),
        model_loaded: engine.as_ref().is_some_and(|e| e.model_loaded()),
        hotkey: HOTKEY_LABEL,
        error: state.error.read().expect("error lock").clone(),
        theme: settings.theme,
        indexing: state.indexing.load(Ordering::SeqCst),
    })
}

/// Open with the default application, or show it in Explorer when `reveal`.
#[tauri::command]
fn open_path(app: AppHandle, path: String, reveal: bool) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let opener = app.opener();
    let result = if reveal { opener.reveal_item_in_dir(&path) } else { opener.open_path(&path, None::<&str>) };
    result.map_err(|e| e.to_string())?;
    hide_window(app);
    Ok(())
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

fn toggle_window(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
    } else {
        show_window(app);
    }
}

/// Show the launcher centered and focused, then warm the model on a background
/// thread so inference is ready while the user types.
fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let state = app.state::<AppState>();
    let engine = state.engine.clone();
    let error = state.error.clone();
    std::thread::spawn(move || {
        // `SAAN_SHOW` can fire before the startup thread has loaded the index;
        // wait briefly for the engine to appear instead of skipping the warmup.
        let deadline = Instant::now() + Duration::from_secs(10);
        let ready = loop {
            let current = engine.read().expect("engine lock").clone();
            if current.is_some() {
                break current;
            }
            if Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        // Clone the engine out so the state lock is not held while the model loads.
        let Some(engine) = ready else { return };
        if let Err(err) = engine.preload() {
            *error.write().expect("error lock") = Some(format!("{err:#}"));
        }
    });
}

fn main() {
    let hotkey = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    tauri::Builder::default()
        // Registered first: a second `saan` launch signals this process to
        // show/focus the existing window instead of starting another instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &hotkey && event.state() == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .manage(AppState::default())
        .setup(move |app| {
            app.global_shortcut().register(hotkey)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || startup(&handle));
            // `SAAN_SHOW` (set by `saan` with no arguments) shows the window once the
            // page has loaded: showing it earlier lets WebView2 initialisation hand
            // focus back to the terminal, and hide-on-blur then closes it again.
            if std::env::var_os("SAAN_SHOW").is_some() {
                SHOW_ON_LOAD.store(true, Ordering::SeqCst);
            }
            // While the launcher sits hidden, drop the model once it has been
            // idle long enough so an empty window does not pin ~150 MB.
            let idle_secs: u64 = std::env::var("SAAN_IDLE_UNLOAD_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300);
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let hidden = handle
                    .get_webview_window("main")
                    .map(|w| !w.is_visible().unwrap_or(true))
                    .unwrap_or(true);
                if !hidden {
                    continue;
                }
                let state = handle.state::<AppState>();
                let ready = state.engine.read().expect("engine lock").clone();
                if let Some(engine) = ready {
                    let _ = engine.release_if_idle(Duration::from_secs(idle_secs));
                }
            });
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished && SHOW_ON_LOAD.swap(false, Ordering::SeqCst) {
                show_window(webview.app_handle());
            }
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                // The folder picker is modal: it takes focus away from the
                // launcher, and we must not hide underneath it.
                if DIALOG_OPEN.load(Ordering::SeqCst) {
                    return;
                }
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            search,
            status,
            open_path,
            hide_window,
            get_settings,
            save_settings,
            set_jev_key,
            pick_folder,
            start_index,
            cancel_index
        ])
        .run(tauri::generate_context!())
        .expect("error while running saan");
}
