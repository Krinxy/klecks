// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Mirrors the payloads of app/src/commands.rs.
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub backend: String,
    pub driver: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AppStatus {
    pub version: String,
    pub gpu: Option<GpuInfo>,
    pub gpu_error: Option<String>,
    pub config_error: Option<String>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VideoInfo {
    pub path: String,
    pub name: String,
    pub size_mb: f64,
}

/// What the drop zone currently shows.
#[derive(Debug, Clone, PartialEq)]
pub enum DropState {
    Idle,
    Hover,
    Accepted(VideoInfo),
    Rejected(String),
}
