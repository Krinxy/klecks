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

/// Top-down plan of a scanned room: a density grid and where the cameras walked.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Vec<u32>>,
    /// `[x, y, video, heading_x, heading_y]` in grid cells.
    pub cameras: Vec<[f64; 5]>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RoomOverview {
    pub room: Room,
    pub scan: Option<ScanListItem>,
    pub videos: Vec<VideoShare>,
    pub frames: u32,
    pub plan: Option<Plan>,
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
