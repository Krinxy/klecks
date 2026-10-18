// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
//! The media library on disk:
//!
//! ```text
//! media/
//!   rooms/<room>/            every video of one room (livingroom/, kitchen/, ...)
//!   assets/<item>/           one piece of furniture: photos + an optional asset.yaml
//! ```
//!
//! Folders are the database: adding a room or a piece of furniture means creating a folder.
mod asset;
mod library;
mod room;

pub use asset::{Asset, AssetSize};
pub use library::Library;
pub use room::{Room, VideoFile};

/// "living_room" / "living-room" / "livingroom" -> "Living room" / "Living room" / "Livingroom".
pub(crate) fn display_name(folder: &str) -> String {
    let spaced = folder.replace(['_', '-'], " ");
    let mut chars = spaced.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::display_name;

    #[test]
    fn folder_names_become_readable() {
        assert_eq!(display_name("living_room"), "Living room");
        assert_eq!(display_name("sofa-grey"), "Sofa grey");
        assert_eq!(display_name("livingroom"), "Livingroom");
    }
}
