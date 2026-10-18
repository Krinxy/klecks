// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Mirrors the payloads of app/src/commands.rs and core/reconstruct.
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub backend: String,
    pub driver: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScanReport {
    pub frames: u32,
    pub points: u64,
    pub view_points: u64,
    pub inference_seconds: f64,
    pub peak_vram_gb: f64,
    pub median_depth: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScanResult {
    pub scan_dir: String,
    pub scene_ply: String,
    /// "splats" or "points"
    pub kind: String,
    pub report: ScanReport,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScanListItem {
    pub name: String,
    /// The room the scan belongs to (the scan folder's name without its time stamp).
    pub room: String,
    pub modified_ms: u64,
    pub result: ScanResult,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RoomVideo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub videos: Vec<RoomVideo>,
}

/// A room of the library with what has been scanned of it so far.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RoomView {
    pub room: Room,
    pub total_mb: f64,
    pub scan_count: usize,
    pub latest_scan: Option<ScanListItem>,
}

/// One film of a room and how much of the scene it gave.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VideoShare {
    pub name: String,
    pub size_mb: f64,
    pub frames: u32,
    pub share_pct: u8,
    pub scanned: bool,
    pub thumb: Option<String>,
    /// Position of the film in the scan, which is also its colour on the plan.
    pub index: Option<usize>,
}

/// How much of the room one film saw, as estimated from the camera positions.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct VideoScore {
    pub frames: u32,
    /// Cells of the footprint this film saw.
    pub seen: u32,
    /// Cells only this film saw.
    pub unique: u32,
}

/// The largest part of the footprint that few frames saw.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Thin {
    pub share_pct: u32,
    /// Centre of the part, 0 to 1 from the left and from the top of the plan.
    pub x: f64,
    pub y: f64,
}

/// Top-down plan of a scanned room: a density grid, where the cameras walked and how well each part is seen.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Vec<u32>>,
    /// `[x, y, video, heading_x, heading_y]` in grid cells.
    pub cameras: Vec<[f64; 5]>,
    /// Frames that saw each cell (0 outside the footprint, capped at 9).
    pub seen: Vec<Vec<u32>>,
    pub videos: Vec<VideoScore>,
    pub footprint: u32,
    pub covered_pct: u32,
    pub thin: Option<Thin>,
}

/// One scan folder of a room, finished or stopped.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScanEntry {
    pub name: String,
    pub modified_ms: u64,
    /// "splats", "points" or "unfinished"
    pub state: String,
    pub frames: u32,
    pub seconds: u32,
    pub scan: Option<ScanResult>,
    pub note: String,
}

/// A door on the plan, found by the detector or placed by hand.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DoorView {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub score: f64,
    pub frames: u64,
    /// "detected" (not reviewed), "confirmed", "rejected" or "manual"
    pub state: String,
    /// The room this door leads to.
    pub to: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RoomRef {
    pub id: String,
    pub name: String,
}

/// Which rooms the doors connect.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Apartment {
    pub rooms: Vec<RoomRef>,
    pub links: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RoomOverview {
    pub room: Room,
    pub scan: Option<ScanListItem>,
    pub videos: Vec<VideoShare>,
    pub frames: u32,
    pub plan: Option<Plan>,
    pub history: Vec<ScanEntry>,
    pub doors: Vec<DoorView>,
    pub other_rooms: Vec<RoomRef>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct AssetSize {
    pub width_cm: f32,
    pub depth_cm: f32,
    pub height_cm: f32,
}

/// One piece of furniture of the catalog.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub category: String,
    pub size: Option<AssetSize>,
    pub notes: String,
    pub source_url: String,
    pub tags: Vec<String>,
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PresetView {
    pub id: String,
    pub train_iterations: u32,
    pub max_splats: u32,
    pub minutes: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FolderView {
    pub kind: String,
    pub label: String,
    pub path: String,
    /// The settings key that moves this folder; None = read-only.
    pub setting: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SettingsView {
    pub preset: String,
    pub presets: Vec<PresetView>,
    pub upscale_model: String,
    pub brush: String,
    pub reconstruct_error: Option<String>,
    pub folders: Vec<FolderView>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct StartupHooks {
    pub scene: Option<ScanResult>,
    pub autoscan_videos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AppStatus {
    pub version: String,
    pub gpu: Option<GpuInfo>,
    pub gpu_error: Option<String>,
    pub config_error: Option<String>,
    pub reconstruct_error: Option<String>,
    pub preset: Option<String>,
    #[serde(default)]
    pub startup: StartupHooks,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VideoInfo {
    pub path: String,
    pub name: String,
    pub size_mb: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ProgressEvent {
    pub stage: String,
    pub pct: u8,
    pub message: String,
}

/// What the drop zone currently shows.
#[derive(Debug, Clone, PartialEq)]
pub enum DropState {
    Idle,
    Hover,
    Accepted(Vec<VideoInfo>),
    Rejected(String),
}

/// Which screen the app is on.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// The start menu.
    Menu,
    /// Drop loose videos to scan them.
    NewScan,
    Rooms,
    /// One room of the library: its films, plan and free camera.
    Room(String),
    Catalog,
    Settings,
    Scanning(String),
    Viewing(ScanResult),
    Failed(String),
}
