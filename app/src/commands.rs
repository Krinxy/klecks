// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
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

#[derive(Serialize)]
pub struct AppStatus {
    version: String,
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

#[derive(Clone, Serialize)]
pub struct ScanListItem {
    pub name: String,
    /// The room this scan belongs to (the part of the folder name before the time stamp).
    pub room: String,
    pub modified_ms: u64,
    pub result: ScanResult,
}

// The #[tauri::command] macro needs free functions, so these are thin shims over
// AppState / the helpers below (same exception gutla makes).
#[tauri::command]
pub fn app_status(state: State<AppState>) -> AppStatus {
    let config = state.config();
    AppStatus {
        version: format!("{} {}", env!("CARGO_PKG_VERSION"), option_env!("KLECKS_BUILD").unwrap_or("")).trim().to_string(),
        gpu: state.gpu.as_ref().ok().cloned(),
        gpu_error: state.gpu.as_ref().err().cloned(),
        config_error: config.error.clone(),
        reconstruct_error: config.reconstructor.as_ref().err().cloned(),
        preset: config.settings.as_ref().map(|s| s.pipeline.preset.clone()),
        paths: config.paths.clone(),
        startup: state.startup.clone(),
    }
}

/// Native file picker; several videos of the same room can be chosen at once.
/// Async so the dialog never blocks the main thread.
#[tauri::command]
pub async fn pick_videos(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let extensions = allowed_extensions(&state);
    let extension_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
    let picked = app
        .dialog()
        .file()
        .add_filter("Video", &extension_refs)
        .blocking_pick_files();
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

/// Validates what was chosen or dropped: video files, or folders (their videos are used).
#[tauri::command]
pub fn accept_videos(paths: Vec<String>, state: State<AppState>) -> Result<Vec<VideoInfo>, String> {
    let extensions = allowed_extensions(&state);
    let mut videos = Vec::new();
    for path in &paths {
        let path = Path::new(path);
        if path.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(path)
                .map_err(|error| format!("Cannot read the folder: {error}"))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|file| file.is_file() && has_extension(file, &extensions))
                .collect();
            inside.sort();
            videos.extend(inside);
        } else if has_extension(path, &extensions) && path.is_file() {
            videos.push(path.to_path_buf());
        }
    }
    videos.dedup();
    if videos.is_empty() {
        return Err(format!("No supported video found (use {}).", extensions.join(", ")));
    }
    videos
        .into_iter()
        .map(|file| {
            let metadata = std::fs::metadata(&file).map_err(|error| format!("Cannot read the file: {error}"))?;
            Ok(VideoInfo {
                name: file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
                size_mb: metadata.len() as f64 / (1024.0 * 1024.0),
                path: file.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

/// Starts a reconstruction of loose videos. Progress arrives as events:
/// `reconstruct://progress`, then one of `reconstruct://done`, `://failed`, `://cancelled`.
#[tauri::command]
pub fn start_reconstruction(app: tauri::AppHandle, paths: Vec<String>) -> Result<(), String> {
    begin_scan(app, paths.into_iter().map(PathBuf::from).collect(), None)
}

/// Runs the reconstruction in the background; `name` names the scan folder (the room, if any).
pub fn begin_scan(app: tauri::AppHandle, videos: Vec<PathBuf>, name: Option<String>) -> Result<(), String> {
    let (reconstructor, scans_dir, token) = {
        let state = app.state::<AppState>();
        // The GPU rule is checked here as well as inside the backend: never ML without a GPU.
        state.gpu.as_ref().map_err(|reason| reason.clone())?;
        let config = state.config();
        let reconstructor = config.reconstructor.as_ref().map_err(|reason| reason.clone())?.clone();
        let scans_dir = config
            .paths
            .as_ref()
            .ok_or("The configuration could not be loaded.")?
            .scans
            .clone();
        drop(config);
        let token = CancelToken::new();
        let mut job = state.job.lock().map_err(|_| "Internal error: job lock poisoned")?;
        if job.is_some() {
            return Err("A scan is already running.".into());
        }
        *job = Some(token.clone());
        (reconstructor, scans_dir, token)
    };

    let request = match name {
        Some(room) => ScanRequest::named(&videos, &scans_dir, &room),
        None => ScanRequest::new(&videos, &scans_dir),
    };
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

/// "livingroom-1792241488" -> "livingroom"; a folder without the time stamp is its own room.
pub fn room_of_scan(folder: &str) -> String {
    match folder.rsplit_once('-') {
        Some((room, stamp)) if !room.is_empty() && stamp.chars().all(|c| c.is_ascii_digit()) => room.to_string(),
        _ => folder.to_string(),
    }
}

/// Every finished scan, newest first.
pub fn all_scans(paths: &AppPaths) -> Vec<ScanListItem> {
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
            let name = entry.file_name().to_string_lossy().into_owned();
            Some(ScanListItem { room: room_of_scan(&name), name, modified_ms, result })
        })
        .collect();
    scans.sort_by_key(|scan| std::cmp::Reverse(scan.modified_ms));
    scans
}

/// Opens a scan folder, but only from inside the scans directory.
#[tauri::command]
pub fn open_scan(dir: String, state: State<AppState>) -> Result<ScanResult, String> {
    let scans = state
        .config()
        .paths
        .as_ref()
        .ok_or("The configuration could not be loaded.")?
        .scans
        .clone();
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

fn has_extension(file: &Path, extensions: &[String]) -> bool {
    file.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|ext| extensions.contains(&ext))
}

pub fn allowed_extensions(state: &AppState) -> Vec<String> {
    state
        .config()
        .settings
        .as_ref()
        .map(|settings| settings.capture.extensions.clone())
        .unwrap_or_else(|| FALLBACK_EXTENSIONS.iter().map(|ext| ext.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::room_of_scan;

    #[test]
    fn scan_folders_map_back_to_their_room() {
        assert_eq!(room_of_scan("livingroom-1792241488"), "livingroom");
        assert_eq!(room_of_scan("living-room-1792241488"), "living-room");
        assert_eq!(room_of_scan("5-videos-1792241488"), "5-videos");
        assert_eq!(room_of_scan("kitchen"), "kitchen");
        assert_eq!(room_of_scan("-1792241488"), "-1792241488");
    }
}
