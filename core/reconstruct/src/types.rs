// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// One progress step reported by a backend (`stage` is machine-readable, `message` is for people).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressEvent {
    pub stage: String,
    pub pct: u8,
    pub message: String,
}

/// A video to reconstruct and the folder its results go to.
#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub video: PathBuf,
    pub scan_dir: PathBuf,
}

impl ScanRequest {
    /// Puts the scan in a fresh `<scans_dir>/<video name>-<unix time>` folder.
    pub fn new(video: &Path, scans_dir: &Path) -> Self {
        let stem = video
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "scan".into());
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        Self {
            video: video.to_path_buf(),
            scan_dir: scans_dir.join(format!("{stem}-{seconds}")),
        }
    }
}

/// Numbers written by the backend next to the cloud (`report.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanReport {
    pub frames: u32,
    pub points: u64,
    pub view_points: u64,
    pub inference_seconds: f64,
    pub peak_vram_gb: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub scan_dir: PathBuf,
    /// Downsampled cloud for the viewer.
    pub scene_ply: PathBuf,
    /// Full-resolution cloud for later processing.
    pub points_ply: PathBuf,
    pub report: ScanReport,
}

impl ScanResult {
    /// Reads a finished scan folder; used right after a run and to re-open old scans.
    pub fn from_dir(scan_dir: &Path) -> Option<Self> {
        let scene_ply = scan_dir.join("scene.ply");
        if !scene_ply.is_file() {
            return None;
        }
        let report = std::fs::read_to_string(scan_dir.join("report.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Some(Self {
            scan_dir: scan_dir.to_path_buf(),
            scene_ply,
            points_ply: scan_dir.join("points.ply"),
            report,
        })
    }
}
