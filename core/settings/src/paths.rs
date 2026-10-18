// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::schema::PathsSection;

const APP_DIR: &str = "klecks";
const MAX_PARENT_LEVELS: usize = 6;

/// Resolved, absolute locations for everything klecks writes.
///
/// Windows: config in `%APPDATA%\klecks`, big data in `%LOCALAPPDATA%\klecks`.
/// Linux:   config in `~/.config/klecks`, big data in `~/.local/share/klecks`.
#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub user_config: PathBuf,
    pub data: PathBuf,
    pub media: PathBuf,
    pub models: PathBuf,
    pub scans: PathBuf,
    pub cache: PathBuf,
}

impl AppPaths {
    /// `config_dir` is the shipped `config/` folder; a `media/` folder next to it or above it (a repo
    /// checkout) is used when `media_dir` is empty.
    pub fn resolve(section: &PathsSection, config_dir: &Path) -> Self {
        let user_config = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DIR);
        let data = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DIR);
        let nearby_media = config_dir
            .ancestors()
            .skip(1)
            .take(MAX_PARENT_LEVELS)
            .map(|dir| dir.join("media"))
            .find(|dir| dir.is_dir());
        Self {
            media: Self::pick(&section.media_dir, || nearby_media.unwrap_or_else(|| data.join("media"))),
            models: Self::pick(&section.models_dir, || data.join("models")),
            scans: Self::pick(&section.scans_dir, || data.join("scans")),
            cache: Self::pick(&section.cache_dir, || data.join("cache")),
            user_config,
            data,
        }
    }

    /// Creates all directories klecks writes to.
    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            &self.user_config,
            &self.data,
            &self.media.join("rooms"),
            &self.media.join("assets"),
            &self.models,
            &self.scans,
            &self.cache,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    fn pick(configured: &str, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            fallback()
        } else {
            // Re-assembling the components gives one separator style ("D:/a/b" -> "D:\a\b" on Windows).
            Path::new(trimmed).components().collect()
        }
    }
}
