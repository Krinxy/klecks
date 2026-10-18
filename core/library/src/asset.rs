// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::display_name;

const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// Real-world size in centimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AssetSize {
    pub width_cm: f32,
    pub depth_cm: f32,
    pub height_cm: f32,
}

/// What `assets/<item>/asset.yaml` may contain. Everything is optional: a folder with just photos
/// is already a valid asset.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AssetFile {
    name: String,
    category: String,
    width_cm: Option<f32>,
    depth_cm: Option<f32>,
    height_cm: Option<f32>,
    notes: String,
    source_url: String,
}

/// One piece of furniture from the catalog.
#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    /// Folder name.
    pub id: String,
    pub name: String,
    pub category: String,
    /// Known only when width, depth and height are all given.
    pub size: Option<AssetSize>,
    pub notes: String,
    pub source_url: String,
    pub images: Vec<PathBuf>,
    pub dir: PathBuf,
}

impl Asset {
    pub(crate) fn read(dir: &Path) -> Option<Self> {
        let id = dir.file_name()?.to_string_lossy().into_owned();
        let file: AssetFile = std::fs::read_to_string(dir.join("asset.yaml"))
            .ok()
            .and_then(|text| serde_yaml_ng::from_str(&text).ok())
            .unwrap_or_default();

        let mut images: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                    .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
            })
            .collect();
        images.sort();

        let size = match (file.width_cm, file.depth_cm, file.height_cm) {
            (Some(width_cm), Some(depth_cm), Some(height_cm)) => Some(AssetSize { width_cm, depth_cm, height_cm }),
            _ => None,
        };
        Some(Self {
            name: if file.name.trim().is_empty() { display_name(&id) } else { file.name },
            category: file.category,
            size,
            notes: file.notes,
            source_url: file.source_url,
            images,
            id,
            dir: dir.to_path_buf(),
        })
    }
}
