// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use klecks_gpu::GpuInfo;
use klecks_reconstruct::{CancelToken, ReconstructError, ScanRequest, ScanResult};
use klecks_settings::AppPaths;
use serde::Serialize;
use tauri::{Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::state::{AppState, StartupHooks};

const FALLBACK_EXTENSIONS: [&str; 5] = ["mp4", "mov", "mkv", "webm", "avi"];
const RECENT_SCANS: usize = 6;

#[derive(Serialize)]
pub struct AppStatus {
    version: &'static str,
    gpu: Option<GpuInfo>,
    gpu_error: Option<String>,
    config_error: Option<String>,
    reconstruct_error: Option<String>,
    preset: Option<String>,
    paths: Option<AppPaths>,
    startup: StartupHooks,
}

#[derive(Serialize)]
pub struct VideoInfo {
    path: String,
    name: String,
    size_mb: f64,
}

#[derive(Serialize)]
pub struct ScanListItem {
    name: String,
    modified_ms: u64,
    result: ScanResult,
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
        reconstruct_error: state.reconstructor.as_ref().err().cloned(),
        preset: state.settings.as_ref().map(|s| s.pipeline.preset.clone()),
        paths: state.paths.clone(),
        startup: state.startup.clone(),
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

/// Starts a reconstruction in the background. Progress arrives as events:
/// `reconstruct://progress`, then one of `reconstruct://done`, `://failed`, `://cancelled`.
#[tauri::command]
pub fn start_reconstruction(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let (reconstructor, scans_dir, token) = {
        let state = app.state::<AppState>();
        // The GPU rule is checked here as well as inside the backend: never ML without a GPU.
        state.gpu.as_ref().map_err(|reason| reason.clone())?;
        let reconstructor = state.reconstructor.as_ref().map_err(|reason| reason.clone())?.clone();
        let scans_dir = state
            .paths
            .as_ref()
            .ok_or("The configuration could not be loaded.")?
            .scans
            .clone();
        let token = CancelToken::new();
        let mut job = state.job.lock().map_err(|_| "Internal error: job lock poisoned")?;
        if job.is_some() {
            return Err("A scan is already running.".into());
        }
        *job = Some(token.clone());
        (reconstructor, scans_dir, token)
    };

    let request = ScanRequest::new(Path::new(&path), &scans_dir);
    std::thread::spawn(move || {
        let outcome = reconstructor.run(&request, &token, &mut |event| {
            let _ = app.emit("reconstruct://progress", event);
        });
        if let Ok(mut job) = app.state::<AppState>().job.lock() {
            job.take();
        }
        let _ = match outcome {
            Ok(result) => app.emit("reconstruct://done", &result),
            Err(ReconstructError::Cancelled) => app.emit("reconstruct://cancelled", ()),
            Err(error) => app.emit("reconstruct://failed", error.to_string()),
        };
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_reconstruction(state: State<AppState>) {
    if let Ok(job) = state.job.lock() {
        if let Some(token) = job.as_ref() {
            token.cancel();
        }
    }
}

/// Earlier scans, newest first, so a finished scan survives restarting the app.
#[tauri::command]
pub fn list_scans(state: State<AppState>) -> Vec<ScanListItem> {
    let Some(paths) = state.paths.as_ref() else {
        return Vec::new();
    };
    let mut scans: Vec<ScanListItem> = std::fs::read_dir(&paths.scans)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let result = ScanResult::from_dir(&entry.path())?;
            let modified_ms = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0);
            Some(ScanListItem {
                name: entry.file_name().to_string_lossy().into_owned(),
                modified_ms,
                result,
            })
        })
        .collect();
    scans.sort_by_key(|scan| std::cmp::Reverse(scan.modified_ms));
    scans.truncate(RECENT_SCANS);
    scans
}

/// Opens a scan folder, but only from inside the scans directory.
#[tauri::command]
pub fn open_scan(dir: String, state: State<AppState>) -> Result<ScanResult, String> {
    let scans = state.paths.as_ref().ok_or("The configuration could not be loaded.")?.scans.clone();
    let requested: PathBuf = std::fs::canonicalize(&dir).map_err(|error| format!("Cannot open the scan: {error}"))?;
    let root = std::fs::canonicalize(&scans).map_err(|error| error.to_string())?;
    if !requested.starts_with(&root) {
        return Err("That folder is not one of your scans.".into());
    }
    ScanResult::from_dir(&requested).ok_or_else(|| "That scan has no 3D scene.".into())
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
