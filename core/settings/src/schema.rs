// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub app: AppSection,
    pub paths: PathsSection,
    pub gpu: GpuSettings,
    pub capture: CaptureSettings,
    pub pipeline: PipelineSettings,
    pub gemini: GeminiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSection {
    pub name: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsSection {
    /// Library of rooms (`rooms/<name>/` with videos) and furniture (`assets/<name>/`).
    pub media_dir: String,
    pub models_dir: String,
    pub scans_dir: String,
    pub cache_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSettings {
    pub require_gpu: bool,
    pub allowed_vendors: Vec<String>,
    pub backends: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSettings {
    pub extensions: Vec<String>,
    pub sample_fps: f32,
    pub max_frames: u32,
    pub min_sharpness: f32,
    pub min_coverage_pct: u8,
    pub min_duration_s: u32,
    pub max_duration_s: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructSettings {
    pub backend: String,
    pub python: String,
    pub script: String,
    pub hf_home: String,
    /// Frames VGGT looks at together; lowered automatically to fit the graphics card.
    pub window: u32,
    /// Frames taken per second of film (long films are cut into overlapping windows).
    pub frame_rate: f32,
    /// Frames for the whole room.
    pub max_frames: u32,
    pub conf_percentile: f32,
    pub view_points: u32,
    /// ESRGAN-style model that sharpens the training images; empty = off.
    pub upscale_model: String,
    /// `brush_app` executable that trains the Gaussian splat; empty = point cloud only.
    pub brush: String,
    /// Colour detail per splat (0-3). Each step roughly doubles the file and the loading time;
    /// 1 looked the same as 3 in the held-out test and is 2.6 times smaller.
    pub sh_degree: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSettings {
    pub reconstruct: ReconstructSettings,
    pub preset: String,
    pub presets: BTreeMap<String, PresetSettings>,
    pub floorplan: FloorplanSettings,
    pub scale: ScaleSettings,
}

impl PipelineSettings {
    /// The preset selected by `preset`, if it exists in `presets`.
    pub fn active_preset(&self) -> Option<&PresetSettings> {
        self.presets.get(&self.preset)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetSettings {
    pub train_iterations: u32,
    pub max_splats: u32,
    /// Colour detail of this quality (0-3); empty = the reconstruct setting.
    #[serde(default)]
    pub sh_degree: Option<u32>,
    /// Weight of the perceptual loss (0 = off): sharper detail at the cost of training time.
    #[serde(default)]
    pub lpips_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorplanSettings {
    pub slice_min_m: f32,
    pub slice_max_m: f32,
    pub cell_size_m: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleSettings {
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSettings {
    pub enabled: bool,
    pub api_key_env: String,
    pub model: String,
    pub cache_by_image_hash: bool,
    pub batch_per_scan: bool,
    pub budget: BudgetSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSettings {
    pub max_requests_per_minute: u32,
    pub max_requests_per_day: u32,
}
