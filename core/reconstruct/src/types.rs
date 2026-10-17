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

/// Videos of one room and the folder their result goes to.
#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub videos: Vec<PathBuf>,
    pub scan_dir: PathBuf,
}

impl ScanRequest {
    /// Puts the scan in a fresh `<scans_dir>/<name>-<unix time>` folder, named after the video
    /// (or "<n>-videos" when several films of the same room are combined).
    pub fn new(videos: &[PathBuf], scans_dir: &Path) -> Self {
        let name = match videos {
            [single] => single
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "scan".into()),
            many => format!("{}-videos", many.len()),
        };
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        Self {
            videos: videos.to_vec(),
            scan_dir: scans_dir.join(format!("{name}-{seconds}")),
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
    /// Typical distance of surfaces from the filming position (model units); sizes the first view.
    pub median_depth: f64,
    pub videos: Vec<String>,
}

/// What the viewer gets to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SceneKind {
    /// A trained Gaussian splat (`splat.ply`): sharp, view-dependent colour.
    Splats,
    /// The fused point cloud (`scene.ply`): quick, rougher.
    Points,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub scan_dir: PathBuf,
    /// The file the viewer opens: the splat if one was trained, else the downsampled cloud.
    pub scene_ply: PathBuf,
    pub kind: SceneKind,
    /// Full-resolution cloud for later processing.
    pub points_ply: PathBuf,
    pub report: ScanReport,
}

impl ScanResult {
    /// Reads a finished scan folder; used right after a run and to re-open old scans.
    pub fn from_dir(scan_dir: &Path) -> Option<Self> {
        let splat = scan_dir.join("splat.ply");
        let cloud = scan_dir.join("scene.ply");
        let (scene_ply, kind) = if splat.is_file() {
            (splat, SceneKind::Splats)
        } else if cloud.is_file() {
            (cloud, SceneKind::Points)
        } else {
            return None;
        };
        let report = std::fs::read_to_string(scan_dir.join("report.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Some(Self {
            scan_dir: scan_dir.to_path_buf(),
            scene_ply,
            kind,
            points_ply: scan_dir.join("points.ply"),
            report,
        })
    }
}
