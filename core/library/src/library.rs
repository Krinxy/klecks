// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::path::{Path, PathBuf};

use crate::{Asset, Room};

/// Reads the media folder. Nothing is cached: the library is whatever is on disk right now, so a
/// folder created in the file manager shows up the next time the screen is opened.
pub struct Library {
    root: PathBuf,
    video_extensions: Vec<String>,
}

impl Library {
    pub fn new(root: PathBuf, video_extensions: Vec<String>) -> Self {
        Self { root, video_extensions }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rooms_dir(&self) -> PathBuf {
        self.root.join("rooms")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn rooms(&self) -> Vec<Room> {
        Self::folders(&self.rooms_dir())
            .into_iter()
            .filter_map(|dir| Room::read(&dir, &self.video_extensions))
            .collect()
    }

    pub fn room(&self, id: &str) -> Option<Room> {
        // `id` comes from the UI: only a plain folder name may select a room.
        let plain = !id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != "..";
        plain
            .then(|| Room::read(&self.rooms_dir().join(id), &self.video_extensions))
            .flatten()
    }

    pub fn assets(&self) -> Vec<Asset> {
        Self::folders(&self.assets_dir())
            .into_iter()
            .filter_map(|dir| Asset::read(&dir))
            .collect()
    }

    /// Sub-folders sorted by name; a missing folder is just an empty library.
    fn folders(parent: &Path) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(parent)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        dirs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str) -> (Library, PathBuf) {
        let root = std::env::temp_dir().join(format!("klecks-library-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let extensions = ["mp4", "mov"].map(String::from).to_vec();
        (Library::new(root.clone(), extensions), root)
    }

    #[test]
    fn a_missing_media_folder_is_an_empty_library() {
        let (library, root) = library("empty");
        let _ = std::fs::remove_dir_all(&root);
        assert!(library.rooms().is_empty() && library.assets().is_empty());
    }

    #[test]
    fn rooms_list_their_videos_in_any_letter_case_and_ignore_other_files() {
        let (library, root) = library("rooms");
        let room = root.join("rooms").join("livingroom");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::write(room.join("IMG_1.MP4"), [0u8; 10]).unwrap();
        std::fs::write(room.join("walk.mov"), [0u8; 5]).unwrap();
        std::fs::write(room.join("notes.txt"), "x").unwrap();
        std::fs::create_dir_all(root.join("rooms").join("kitchen")).unwrap();

        let rooms = library.rooms();
        assert_eq!(rooms.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["kitchen", "livingroom"]);
        let living = &rooms[1];
        assert_eq!(living.videos.len(), 2);
        assert_eq!(living.total_bytes, 15);
        assert_eq!(living.name, "Livingroom");
        assert!(rooms[0].videos.is_empty());
    }

    #[test]
    fn only_a_plain_folder_name_selects_a_room() {
        let (library, root) = library("select");
        std::fs::create_dir_all(root.join("rooms").join("a")).unwrap();
        assert!(library.room("a").is_some());
        for bad in ["", ".", "..", "../a", "a/b", "a\\b"] {
            assert!(library.room(bad).is_none(), "{bad:?} must not select a room");
        }
    }

    #[test]
    fn assets_read_their_yaml_and_photos_and_work_without_a_yaml() {
        let (library, root) = library("assets");
        let sofa = root.join("assets").join("sofa_grey");
        std::fs::create_dir_all(&sofa).unwrap();
        std::fs::write(
            sofa.join("asset.yaml"),
            "name: Corner sofa\ncategory: sofa\nwidth_cm: 280\ndepth_cm: 160\nheight_cm: 85\n",
        )
        .unwrap();
        std::fs::write(sofa.join("front.JPG"), "x").unwrap();
        std::fs::write(sofa.join("side.png"), "x").unwrap();
        let table = root.join("assets").join("round_table");
        std::fs::create_dir_all(&table).unwrap();
        std::fs::write(table.join("a.webp"), "x").unwrap();

        let assets = library.assets();
        let table = &assets[0];
        assert_eq!((table.name.as_str(), table.images.len(), table.size), ("Round table", 1, None));
        let sofa = &assets[1];
        assert_eq!(sofa.name, "Corner sofa");
        assert_eq!(sofa.images.len(), 2);
        assert_eq!(sofa.size.unwrap().width_cm, 280.0);
    }

    #[test]
    fn a_partial_size_counts_as_unknown() {
        let (library, root) = library("partial");
        let item = root.join("assets").join("lamp");
        std::fs::create_dir_all(&item).unwrap();
        std::fs::write(item.join("asset.yaml"), "width_cm: 30\n").unwrap();
        assert!(library.assets()[0].size.is_none());
    }
}
