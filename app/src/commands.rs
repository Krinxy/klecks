// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::Path;

use klecks_gpu::GpuInfo;
use klecks_settings::AppPaths;
use serde::Serialize;
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::state::AppState;

const FALLBACK_EXTENSIONS: [&str; 5] = ["mp4", "mov", "mkv", "webm", "avi"];

#[derive(Serialize)]
pub struct AppStatus {
    version: &'static str,
    gpu: Option<GpuInfo>,
    gpu_error: Option<String>,
    config_error: Option<String>,
    preset: Option<String>,
    paths: Option<AppPaths>,
}

#[derive(Serialize)]
pub struct VideoInfo {
    path: String,
    name: String,
    size_mb: f64,
}

// The #[tauri::command] macro needs free functions, so these are thin shims over
// AppState / the helpers below (same exception gutla makes).
#[tauri::command]
pub fn app_status(state: State<AppState>) -> AppStatus {
    AppStatus {
        version: env!("CARGO_PKG_VERSION"),
        gpu: state.gpu.as_ref().ok().cloned(),
        gpu_error: state.gpu.as_ref().err().cloned(),
        config_error: state.config_error.clone(),
        preset: state.settings.as_ref().map(|s| s.pipeline.preset.clone()),
        paths: state.paths.clone(),
    }
}

/// Native file picker. Async so the dialog never blocks the main thread.
#[tauri::command]
pub async fn pick_video(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let extensions = allowed_extensions(&state);
    let extension_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
    let picked = app
        .dialog()
        .file()
        .add_filter("Video", &extension_refs)
        .blocking_pick_file();
    Ok(picked
        .and_then(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned()))
}

/// Validates a video chosen via dialog or drag and drop.
#[tauri::command]
pub fn accept_video(path: String, state: State<AppState>) -> Result<VideoInfo, String> {
    let file = Path::new(&path);
    let extensions = allowed_extensions(&state);
    let has_allowed_extension = file
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|ext| extensions.contains(&ext));
    if !has_allowed_extension {
        return Err(format!("Not a supported video (use {}).", extensions.join(", ")));
    }
    let metadata = std::fs::metadata(file).map_err(|error| format!("Cannot read the file: {error}"))?;
    if !metadata.is_file() {
        return Err("That is a folder, not a video file.".into());
    }
    Ok(VideoInfo {
        name: file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size_mb: metadata.len() as f64 / (1024.0 * 1024.0),
        path,
    })
}

#[tauri::command]
pub fn window_minimize(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn window_toggle_maximize(window: WebviewWindow) -> Result<(), String> {
    let maximized = window.is_maximized().map_err(|error| error.to_string())?;
    if maximized {
        window.unmaximize()
    } else {
        window.maximize()
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn window_close(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|error| error.to_string())
}

fn allowed_extensions(state: &AppState) -> Vec<String> {
    state
        .settings
        .as_ref()
        .map(|settings| settings.capture.extensions.clone())
        .unwrap_or_else(|| FALLBACK_EXTENSIONS.iter().map(|ext| ext.to_string()).collect())
}
