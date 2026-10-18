// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::path::PathBuf;
use std::process::Command;

use klecks_library::{Asset, Library, Room};
use klecks_reconstruct::{SceneKind, ScanResult};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::{all_scans, begin_scan, room_of_scan, ScanListItem};
use crate::state::AppState;

const MAX_LISTED_SCANS: usize = 40;
/// Brush trains about 60 steps per second on an RTX 4080; the rest is preparation.
const STEPS_PER_MINUTE: f32 = 3600.0;
const PREPARATION_MINUTES: u32 = 1;

#[derive(Serialize)]
pub struct RoomView {
    room: Room,
    total_mb: f64,
    scan_count: usize,
    latest_scan: Option<ScanListItem>,
}

/// One film of a room and how much of the latest scan came from it.
#[derive(Serialize)]
pub struct VideoShare {
    name: String,
    size_mb: f64,
    /// Frames of this film that went into the scene; 0 when the film is newer than the scan.
    frames: u32,
    /// Share of all frames of the scene, 0 to 100.
    share_pct: u8,
    /// False for films that were added after the last scan.
    scanned: bool,
    /// A frame cut from this film, for the little picture in the list.
    thumb: Option<String>,
    /// Position of the film in the scan (its colour on the plan).
    index: Option<usize>,
}

/// One scan folder of a room, finished or not, so a stopped scan can be seen and tried again.
#[derive(Serialize)]
pub struct ScanEntry {
    name: String,
    modified_ms: u64,
    /// "splats", "points" or "unfinished"
    state: &'static str,
    frames: u32,
    seconds: u32,
    scan: Option<ScanResult>,
    /// What the log said last, for a scan that did not finish.
    note: String,
}

/// A door as the person decided about it, kept in `doors.json` inside the room's folder so it travels
/// with the library. Doors are matched to detected ones by position, so a new scan keeps the answers.
#[derive(Clone, Serialize, Deserialize)]
pub struct DoorMark {
    x: f64,
    y: f64,
    /// "confirmed", "rejected" or "manual"
    state: String,
    /// The room this door leads to.
    to: Option<String>,
}

/// A door on the plan: found by the detector or placed by hand, with what the person decided.
#[derive(Serialize)]
pub struct DoorView {
    x: f64,
    y: f64,
    width: f64,
    score: f64,
    frames: u64,
    /// "detected" (not reviewed), "confirmed", "rejected" or "manual"
    state: String,
    to: Option<String>,
}

#[derive(Serialize)]
pub struct RoomRef {
    id: String,
    name: String,
}

/// What a room screen shows: which films built the scene, and the top-down plan if the scan has one.
#[derive(Serialize)]
pub struct RoomOverview {
    room: Room,
    scan: Option<ScanListItem>,
    videos: Vec<VideoShare>,
    frames: u32,
    plan: Option<serde_json::Value>,
    history: Vec<ScanEntry>,
    doors: Vec<DoorView>,
    other_rooms: Vec<RoomRef>,
    /// Furniture placed in the room and areas cut out of the scan (`layout.json` in the room's folder).
    layout: serde_json::Value,
}

#[derive(Serialize)]
pub struct PresetView {
    id: String,
    train_iterations: u32,
    max_splats: u32,
    minutes: u32,
}

#[derive(Serialize)]
pub struct FolderView {
    kind: &'static str,
    label: &'static str,
    path: String,
    /// The settings key that moves this folder, if the user may move it.
    setting: Option<&'static str>,
}

#[derive(Serialize)]
pub struct SettingsView {
    preset: String,
    presets: Vec<PresetView>,
    upscale_model: String,
    brush: String,
    reconstruct_error: Option<String>,
    folders: Vec<FolderView>,
}

fn library(state: &AppState) -> Result<Library, String> {
    let config = state.config();
    let paths = config.paths.as_ref().ok_or("The configuration could not be loaded.")?;
    let extensions = config
        .settings
        .as_ref()
        .map(|settings| settings.capture.extensions.clone())
        .unwrap_or_default();
    Ok(Library::new(paths.media.clone(), extensions))
}

/// Rooms found in `media/rooms/`, each with how many scans of it exist and the newest one.
#[tauri::command]
pub fn list_rooms(state: State<AppState>) -> Result<Vec<RoomView>, String> {
    let rooms = library(&state)?.rooms();
    let scans = state.config().paths.as_ref().map(all_scans).unwrap_or_default();
    Ok(rooms
        .into_iter()
        .map(|room| {
            let mut own = scans.iter().filter(|scan| scan.room == room.id);
            let latest_scan = own.next().cloned();
            RoomView {
                total_mb: room.total_bytes as f64 / (1024.0 * 1024.0),
                scan_count: latest_scan.iter().count() + own.count(),
                latest_scan,
                room,
            }
        })
        .collect())
}

/// The room with its films and, when it was scanned, the contribution of each film and the plan.
#[tauri::command]
pub fn room_overview(state: State<AppState>, room: String) -> Result<RoomOverview, String> {
    let found = library(&state)?.room(&room).ok_or("That room does not exist.")?;
    let scan = state
        .config()
        .paths
        .as_ref()
        .map(all_scans)
        .unwrap_or_default()
        .into_iter()
        .find(|scan| scan.room == found.id);
    let read = |file: &str| -> Option<serde_json::Value> {
        let dir = &scan.as_ref()?.result.scan_dir;
        serde_json::from_str(&std::fs::read_to_string(PathBuf::from(dir).join(file)).ok()?).ok()
    };
    let report = read("report.json");
    let names: Vec<String> = report
        .as_ref()
        .and_then(|report| report.get("videos")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .collect();
    let counts: Vec<u32> = report
        .as_ref()
        .and_then(|report| report.get("frames_per_video")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|count| count.as_u64().unwrap_or(0) as u32)
        .collect();
    let frames: u32 = counts.iter().sum();
    let mut videos: Vec<VideoShare> = found
        .videos
        .iter()
        .map(|video| {
            let at = names.iter().position(|name| name == &video.name);
            let used = at.and_then(|index| counts.get(index)).copied().unwrap_or(0);
            VideoShare {
                name: video.name.clone(),
                size_mb: video.size_bytes as f64 / (1024.0 * 1024.0),
                frames: used,
                share_pct: (used * 100).checked_div(frames).unwrap_or(0) as u8,
                scanned: at.is_some(),
                index: at,
                thumb: at.and_then(|index| {
                    let frame = PathBuf::from(&scan.as_ref()?.result.scan_dir).join("frames").join(format!("{index:02}_0000.jpg"));
                    frame.is_file().then(|| frame.to_string_lossy().into_owned())
                }),
            }
        })
        .collect();
    videos.sort_by_key(|video| std::cmp::Reverse(video.frames));
    let plan = read("plan.json");
    let history = state.config().paths.as_ref().map(|paths| scan_history(&paths.scans, &found.id)).unwrap_or_default();
    let doors = door_views(&found, plan.as_ref());
    let other_rooms = library(&state)?
        .rooms()
        .into_iter()
        .filter(|other| other.id != found.id)
        .map(|other| RoomRef { id: other.id, name: other.name })
        .collect();
    let layout = read_layout(&found);
    Ok(RoomOverview { room: found, scan, videos, frames, plan, history, doors, other_rooms, layout })
}

fn read_layout(room: &Room) -> serde_json::Value {
    std::fs::read_to_string(room.dir.join("layout.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({ "items": [], "cuts": [] }))
}

/// Saves the furniture and cut-outs of a room. The shape is the UI's business (items, cuts, a scale override);
/// the backend only makes sure it is an object and keeps it small.
#[tauri::command]
pub fn save_layout(state: State<AppState>, room: String, layout: serde_json::Value) -> Result<(), String> {
    const MAX_BYTES: usize = 1_000_000;
    let found = library(&state)?.room(&room).ok_or("That room does not exist.")?;
    if !layout.is_object() {
        return Err("The layout is not valid.".into());
    }
    let text = serde_json::to_string_pretty(&layout).map_err(|error| error.to_string())?;
    if text.len() > MAX_BYTES {
        return Err("The layout is too large.".into());
    }
    std::fs::write(found.dir.join("layout.json"), text).map_err(|error| format!("Could not save the layout: {error}"))
}

/// Doors closer than this many plan cells are the same door.
const SAME_DOOR: f64 = 5.0;

fn read_marks(room: &Room) -> Vec<DoorMark> {
    std::fs::read_to_string(room.dir.join("doors.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).hypot(a.1 - b.1) <= SAME_DOOR
}

/// Detected doors from the plan, with the person's decisions laid over them, plus the doors placed by hand.
fn door_views(room: &Room, plan: Option<&serde_json::Value>) -> Vec<DoorView> {
    let marks = read_marks(room);
    let mut taken = vec![false; marks.len()];
    let number = |value: &serde_json::Value, key: &str| value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let mut views: Vec<DoorView> = plan
        .and_then(|plan| plan.get("doors")?.as_array())
        .into_iter()
        .flatten()
        .map(|door| {
            let spot = (number(door, "x"), number(door, "y"));
            let mark = marks.iter().position(|mark| near((mark.x, mark.y), spot));
            if let Some(index) = mark {
                taken[index] = true;
            }
            DoorView {
                x: spot.0,
                y: spot.1,
                width: number(door, "width"),
                score: number(door, "score"),
                frames: door.get("frames").and_then(|v| v.as_u64()).unwrap_or(0),
                state: mark.map(|index| marks[index].state.clone()).unwrap_or_else(|| "detected".into()),
                to: mark.and_then(|index| marks[index].to.clone()),
            }
        })
        .collect();
    for (mark, used) in marks.iter().zip(taken) {
        if !used {
            views.push(DoorView { x: mark.x, y: mark.y, width: 0.0, score: 1.0, frames: 0, state: mark.state.clone(), to: mark.to.clone() });
        }
    }
    views
}

/// Saves the person's answer about one door: confirm it, reject it, add one by hand, link it to another room,
/// or take the answer back (`state` "detected").
#[tauri::command]
pub fn set_door(state: State<AppState>, room: String, x: f64, y: f64, status: String, to: Option<String>) -> Result<RoomOverview, String> {
    let found = library(&state)?.room(&room).ok_or("That room does not exist.")?;
    if !["confirmed", "rejected", "manual", "detected"].contains(&status.as_str()) {
        return Err("Unknown door state.".into());
    }
    let mut marks = read_marks(&found);
    marks.retain(|mark| !near((mark.x, mark.y), (x, y)));
    if status != "detected" {
        marks.push(DoorMark { x, y, state: status, to: to.filter(|id| !id.is_empty() && id != &found.id) });
    }
    let text = serde_json::to_string_pretty(&marks).map_err(|error| error.to_string())?;
    std::fs::write(found.dir.join("doors.json"), text).map_err(|error| format!("Could not save the door: {error}"))?;
    room_overview(state, room)
}

/// Which rooms the doors connect, for the apartment map.
#[derive(Serialize)]
pub struct Apartment {
    rooms: Vec<RoomRef>,
    links: Vec<(String, String)>,
}

#[tauri::command]
pub fn apartment(state: State<AppState>) -> Result<Apartment, String> {
    let rooms = library(&state)?.rooms();
    let mut links: Vec<(String, String)> = Vec::new();
    for room in &rooms {
        for mark in read_marks(room) {
            let Some(to) = mark.to else { continue };
            if mark.state == "rejected" || !rooms.iter().any(|other| other.id == to) {
                continue;
            }
            let pair = if room.id < to { (room.id.clone(), to) } else { (to, room.id.clone()) };
            if !links.contains(&pair) {
                links.push(pair);
            }
        }
    }
    Ok(Apartment { rooms: rooms.into_iter().map(|room| RoomRef { id: room.id, name: room.name }).collect(), links })
}

/// Every folder in the scans directory that belongs to a room, newest first.
fn scan_history(scans: &std::path::Path, room: &str) -> Vec<ScanEntry> {
    let mut entries: Vec<ScanEntry> = std::fs::read_dir(scans)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| room_of_scan(&entry.file_name().to_string_lossy()) == room)
        .map(|entry| {
            let dir = entry.path();
            let modified_ms = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0);
            let report: Option<serde_json::Value> =
                std::fs::read_to_string(dir.join("report.json")).ok().and_then(|text| serde_json::from_str(&text).ok());
            let scan = ScanResult::from_dir(&dir);
            let state = match &scan {
                Some(result) if result.kind == SceneKind::Splats => "splats",
                Some(_) => "points",
                None => "unfinished",
            };
            let note = if scan.is_none() {
                std::fs::read_to_string(dir.join("log.txt"))
                    .ok()
                    .and_then(|text| text.lines().rev().find(|line| !line.trim().is_empty()).map(str::to_owned))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            ScanEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                modified_ms,
                state,
                frames: report.as_ref().and_then(|r| r.get("frames")?.as_u64()).unwrap_or(0) as u32,
                seconds: report.as_ref().and_then(|r| r.get("total_seconds")?.as_f64()).unwrap_or(0.0) as u32,
                scan,
                note,
            }
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.modified_ms));
    entries
}

/// Furniture found in `media/assets/`.
#[tauri::command]
pub fn list_assets(state: State<AppState>) -> Result<Vec<Asset>, String> {
    Ok(library(&state)?.assets())
}

/// Scans that don't belong to a room of the library (dropped loose videos, deleted rooms).
#[tauri::command]
pub fn list_scans(state: State<AppState>) -> Result<Vec<ScanListItem>, String> {
    let room_ids: Vec<String> = library(&state)?.rooms().into_iter().map(|room| room.id).collect();
    let mut scans = state.config().paths.as_ref().map(all_scans).unwrap_or_default();
    scans.retain(|scan| !room_ids.contains(&scan.room));
    scans.truncate(MAX_LISTED_SCANS);
    Ok(scans)
}

/// Reconstructs a room from all videos in its folder.
#[tauri::command]
pub fn start_room_scan(app: AppHandle, room: String) -> Result<(), String> {
    let found = {
        let state = app.state::<AppState>();
        library(&state)?.room(&room)
    };
    let found = found.ok_or("That room does not exist.")?;
    if found.videos.is_empty() {
        return Err(format!("{} has no videos yet. Put them in {}.", found.name, found.dir.display()));
    }
    let videos = found.videos.iter().map(|video| video.path.clone()).collect();
    begin_scan(app, videos, Some(found.id))
}

fn settings_view(state: &AppState) -> Result<SettingsView, String> {
    let config = state.config();
    let settings = config.settings.as_ref().ok_or("The configuration could not be loaded.")?;
    let paths = config.paths.as_ref().ok_or("The configuration could not be loaded.")?;
    let shown = |path: &PathBuf| path.to_string_lossy().into_owned();
    Ok(SettingsView {
        preset: settings.pipeline.preset.clone(),
        presets: settings
            .pipeline
            .presets
            .iter()
            .map(|(id, preset)| PresetView {
                id: id.clone(),
                train_iterations: preset.train_iterations,
                max_splats: preset.max_splats,
                minutes: (preset.train_iterations as f32 / STEPS_PER_MINUTE).ceil() as u32 + PREPARATION_MINUTES,
            })
            .collect(),
        upscale_model: settings.pipeline.reconstruct.upscale_model.clone(),
        brush: settings.pipeline.reconstruct.brush.clone(),
        reconstruct_error: config.reconstructor.as_ref().err().cloned(),
        folders: vec![
            FolderView { kind: "media", label: "Media library", path: shown(&paths.media), setting: Some("paths.media_dir") },
            FolderView { kind: "scans", label: "Finished scans", path: shown(&paths.scans), setting: Some("paths.scans_dir") },
            FolderView { kind: "models", label: "Models", path: shown(&paths.models), setting: Some("paths.models_dir") },
            FolderView { kind: "config", label: "Your settings files", path: shown(&paths.user_config), setting: None },
        ],
    })
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<SettingsView, String> {
    settings_view(&state)
}

/// Settings the Settings screen may change; anything else is refused.
const EDITABLE: [&str; 4] = ["pipeline.preset", "paths.media_dir", "paths.scans_dir", "paths.models_dir"];

/// Saves one setting in the user's override file and applies it right away. An empty value for a
/// folder brings the default back.
#[tauri::command]
pub fn set_setting(app: AppHandle, key: String, value: String) -> Result<SettingsView, String> {
    if !EDITABLE.contains(&key.as_str()) {
        return Err(format!("'{key}' can't be changed here."));
    }
    let state = app.state::<AppState>();
    let loader = state.loader.as_ref().ok_or("The configuration could not be loaded.")?;
    let value = value.trim().to_string();

    if key == "pipeline.preset" {
        let known = state
            .config()
            .settings
            .as_ref()
            .is_some_and(|settings| settings.pipeline.presets.contains_key(&value));
        if !known {
            return Err(format!("There is no quality preset called '{value}'."));
        }
        loader.set_text_override(&key, &value).map_err(|error| error.to_string())?;
    } else if value.is_empty() {
        loader.clear_override(&key).map_err(|error| error.to_string())?;
    } else {
        // A folder has to exist (or be creatable) before it is saved, so a typo can't break the app.
        std::fs::create_dir_all(&value).map_err(|error| format!("Can't use that folder: {error}"))?;
        loader.set_text_override(&key, &value).map_err(|error| error.to_string())?;
    }
    state.reload();
    crate::refresh_asset_scope(&app);
    settings_view(&state)
}

/// Brings a setting back to its shipped default.
#[tauri::command]
pub fn reset_setting(app: AppHandle, key: String) -> Result<SettingsView, String> {
    if !EDITABLE.contains(&key.as_str()) {
        return Err(format!("'{key}' can't be changed here."));
    }
    let state = app.state::<AppState>();
    state
        .loader
        .as_ref()
        .ok_or("The configuration could not be loaded.")?
        .clear_override(&key)
        .map_err(|error| error.to_string())?;
    state.reload();
    crate::refresh_asset_scope(&app);
    settings_view(&state)
}

/// Opens one of klecks' own folders in the file manager. Only named folders, never a path from the UI.
#[tauri::command]
pub fn reveal_folder(state: State<AppState>, kind: String) -> Result<(), String> {
    let folder = {
        let config = state.config();
        let paths = config.paths.as_ref().ok_or("The configuration could not be loaded.")?;
        match kind.as_str() {
            "media" => paths.media.clone(),
            "rooms" => paths.media.join("rooms"),
            "assets" => paths.media.join("assets"),
            "scans" => paths.scans.clone(),
            "models" => paths.models.clone(),
            "config" => paths.user_config.clone(),
            other => return Err(format!("Unknown folder '{other}'.")),
        }
    };
    std::fs::create_dir_all(&folder).map_err(|error| format!("Can't create {}: {error}", folder.display()))?;
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // Fire and forget: explorer.exe reports exit code 1 even when it worked.
    Command::new(opener)
        .arg(&folder)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Can't open {}: {error}", folder.display()))
}
