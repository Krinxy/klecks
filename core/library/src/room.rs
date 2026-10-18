// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::display_name;

#[derive(Debug, Clone, Serialize)]
pub struct VideoFile {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// One room: a folder under `rooms/` and the videos inside it.
#[derive(Debug, Clone, Serialize)]
pub struct Room {
    /// Folder name; also what its scans are named after.
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
    pub videos: Vec<VideoFile>,
    pub total_bytes: u64,
    /// What kind of room it is (`livingroom`, `bedroom`, `office`, ...), from `room.json`; empty when unknown.
    pub kind: String,
}

impl Room {
    pub(crate) fn read(dir: &Path, extensions: &[String]) -> Option<Self> {
        let id = dir.file_name()?.to_string_lossy().into_owned();
        let mut videos: Vec<VideoFile> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| {
                let path = entry.path();
                path.is_file()
                    && path
                        .extension()
                        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                        .is_some_and(|ext| extensions.contains(&ext))
            })
            .map(|entry| VideoFile {
                name: entry.file_name().to_string_lossy().into_owned(),
                size_bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
                path: entry.path(),
            })
            .collect();
        videos.sort_by(|a, b| a.name.cmp(&b.name));
        let kind = std::fs::read_to_string(dir.join("room.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| value.get("kind")?.as_str().map(str::to_owned))
            .unwrap_or_default();
        Some(Self {
            kind,
            name: display_name(&id),
            total_bytes: videos.iter().map(|video| video.size_bytes).sum(),
            id,
            dir: dir.to_path_buf(),
            videos,
        })
    }
}
