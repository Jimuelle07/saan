// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use saan_core::embed::{find_model_dir, Embedder, MODEL_DIR_NAME};
use saan_core::jev::JevClient;
use saan_core::{Engine, Index, SearchResponse};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Global hotkey that toggles the launcher.
const HOTKEY_LABEL: &str = "Ctrl+Shift+Space";

#[derive(Default)]
struct AppState {
    engine: Arc<RwLock<Option<Engine>>>,
    /// Last load/index error, shown in the launcher footer.
    error: Arc<RwLock<Option<String>>>,
}

#[derive(Default, Serialize, Deserialize)]
struct Config {
    root: Option<PathBuf>,
}

#[derive(Serialize)]
struct Status {
    ready: bool,
    root: Option<String>,
    files: usize,
    jev: bool,
    hotkey: &'static str,
    error: Option<String>,
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

fn load_config(app: &AppHandle) -> Config {
    config_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, config: &Config) -> Result<(), String> {
    let path = config_path(app)?;
    std::fs::create_dir_all(path.parent().expect("config has parent")).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    find_model_dir()
        .or_else(|| data_dir(app).ok().map(|d| d.join("models").join(MODEL_DIR_NAME)))
        .filter(|d| d.join("tokenizer.json").is_file())
        .ok_or_else(|| "EmbeddingGemma model not found: run `saan fetch-model` or set SAAN_MODEL_DIR".to_string())
}

/// (Re)build the index for `root` and swap in a fresh engine.
fn build_engine(app: &AppHandle, root: &Path) -> Result<usize, String> {
    let embedder = Embedder::load(&model_dir(app)?).map_err(|e| format!("{e:#}"))?;
    let dir = index_dir(app)?;
    let previous = Index::load(&dir).ok();
    let (index, _) = Index::build(root, &embedder, previous.as_ref(), |_| {}).map_err(|e| format!("{e:#}"))?;
    index.save(&dir).map_err(|e| format!("{e:#}"))?;
    let files = index.files.len();
    let state = app.state::<AppState>();
    *state.engine.write().expect("engine lock") = Some(Engine::new(index, embedder, JevClient::from_env()));
    *state.error.write().expect("error lock") = None;
    Ok(files)
}

/// Startup: load the saved index; if `SAAN_ROOT` names a folder, (re)index it first.
fn startup(app: &AppHandle) {
    let result = (|| -> Result<(), String> {
        if let Some(root) = std::env::var_os("SAAN_ROOT").map(PathBuf::from) {
            save_config(app, &Config { root: Some(root.clone()) })?;
            build_engine(app, &root)?;
            return Ok(());
        }
        let Ok(index) = Index::load(&index_dir(app)?) else { return Ok(()) };
        let embedder = Embedder::load(&model_dir(app)?).map_err(|e| format!("{e:#}"))?;
        *app.state::<AppState>().engine.write().expect("engine lock") =
            Some(Engine::new(index, embedder, JevClient::from_env()));
        Ok(())
    })();
    if let Err(e) = result {
        *app.state::<AppState>().error.write().expect("error lock") = Some(e);
    }
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
async fn index_folder(app: AppHandle, root: String) -> Result<usize, String> {
    let root = PathBuf::from(root.trim());
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    save_config(&app, &Config { root: Some(root.clone()) })?;
    // `build_engine` walks the tree, embeds and saves the index: keep all of that
    // off the async runtime. It swaps the engine only after the build finishes,
    // so in-flight searches keep using the previous index.
    tauri::async_runtime::spawn_blocking(move || build_engine(&app, &root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn status(app: AppHandle, state: State<'_, AppState>) -> Result<Status, String> {
    let engine = state.engine.read().expect("engine lock");
    Ok(Status {
        ready: engine.is_some(),
        root: load_config(&app).root.map(|r| r.display().to_string()),
        files: engine.as_ref().map_or(0, |e| e.index.files.len()),
        jev: engine.as_ref().is_some_and(Engine::jev_enabled),
        hotkey: HOTKEY_LABEL,
        error: state.error.read().expect("error lock").clone(),
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
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn main() {
    let hotkey = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            if std::env::var_os("SAAN_SHOW").is_some() {
                toggle_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![search, index_folder, status, open_path, hide_window])
        .run(tauri::generate_context!())
        .expect("error while running saan");
}
