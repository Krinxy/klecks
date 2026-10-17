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
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScanResult {
    pub scan_dir: String,
    pub scene_ply: String,
    pub report: ScanReport,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScanListItem {
    pub name: String,
    pub modified_ms: u64,
    pub result: ScanResult,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct StartupHooks {
    pub scene: Option<ScanResult>,
    pub autoscan_video: Option<String>,
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
    Accepted(VideoInfo),
    Rejected(String),
}

/// Which screen the app is on.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Welcome,
    Scanning(String),
    Viewing(ScanResult),
    Failed(String),
}
