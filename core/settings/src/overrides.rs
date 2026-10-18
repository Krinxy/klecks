// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::path::PathBuf;

use serde_yaml_ng::{Mapping, Value};

use crate::error::SettingsError;

const HEADER: &str = "# Your personal klecks settings. Written by the Settings screen; edit freely.\n\
# Only the values you changed are listed, everything else keeps its shipped default.\n";

/// Reads and writes the user's override files (`%APPDATA%\klecks\<name>.yaml`).
///
/// Each top-level section of the settings lives in the file the shipped config keeps it in, so an
/// override replaces exactly the key it changes.
pub struct UserOverrides {
    dir: Option<PathBuf>,
}

impl UserOverrides {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    /// Sets `a.b.c` to `value`, creating the file and the nested mappings as needed.
    pub fn set(&self, dotted_key: &str, value: Value) -> Result<(), SettingsError> {
        let (path, mut document) = self.open(dotted_key)?;
        let keys: Vec<&str> = dotted_key.split('.').collect();
        let (last, parents) = keys.split_last().expect("split always yields one item");
        let mut cursor = &mut document;
        for key in parents {
            cursor = Self::child(cursor, key);
        }
        if let Value::Mapping(map) = Self::as_mapping(cursor) {
            map.insert(Value::String((*last).to_string()), value);
        }
        self.write(&path, &document)
    }

    /// Removes `a.b.c` again so the shipped default applies. Missing keys are fine.
    pub fn clear(&self, dotted_key: &str) -> Result<(), SettingsError> {
        let (path, mut document) = self.open(dotted_key)?;
        let keys: Vec<&str> = dotted_key.split('.').collect();
        let (last, parents) = keys.split_last().expect("split always yields one item");
        let mut cursor = &mut document;
        for key in parents {
            match cursor.get_mut(*key) {
                Some(next) => cursor = next,
                None => return Ok(()),
            }
        }
        if let Value::Mapping(map) = cursor {
            map.remove(*last);
        }
        self.write(&path, &document)
    }

    fn file_for(dotted_key: &str) -> &'static str {
        match dotted_key.split('.').next().unwrap_or_default() {
            "capture" => "capture.yaml",
            "pipeline" => "pipeline.yaml",
            "gemini" => "gemini.yaml",
            _ => "klecks.yaml", // app, paths, gpu
        }
    }

    fn open(&self, dotted_key: &str) -> Result<(PathBuf, Value), SettingsError> {
        let dir = self.dir.clone().ok_or(SettingsError::ConfigDirNotFound)?;
        std::fs::create_dir_all(&dir).map_err(|source| SettingsError::Io { path: dir.clone(), source })?;
        let path = dir.join(Self::file_for(dotted_key));
        let document = match std::fs::read_to_string(&path) {
            Ok(text) if !text.trim().is_empty() => serde_yaml_ng::from_str(&text)
                .map_err(|source| SettingsError::Yaml { path: path.clone(), source })?,
            _ => Value::Mapping(Mapping::new()),
        };
        Ok((path, document))
    }

    fn write(&self, path: &PathBuf, document: &Value) -> Result<(), SettingsError> {
        let body = serde_yaml_ng::to_string(document)
            .map_err(|source| SettingsError::Yaml { path: path.clone(), source })?;
        std::fs::write(path, format!("{HEADER}{body}"))
            .map_err(|source| SettingsError::Io { path: path.clone(), source })
    }

    fn as_mapping(value: &mut Value) -> &mut Value {
        if !value.is_mapping() {
            *value = Value::Mapping(Mapping::new());
        }
        value
    }

    fn child<'a>(value: &'a mut Value, key: &str) -> &'a mut Value {
        let map = match Self::as_mapping(value) {
            Value::Mapping(map) => map,
            _ => unreachable!("as_mapping always returns a mapping"),
        };
        map.entry(Value::String(key.to_string()))
            .or_insert_with(|| Value::Mapping(Mapping::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("klecks-overrides-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn sets_nested_keys_in_the_matching_file_and_keeps_the_rest() {
        let dir = temp("set");
        let overrides = UserOverrides::new(Some(dir.clone()));
        overrides.set("pipeline.preset", Value::String("draft".into())).unwrap();
        overrides.set("pipeline.reconstruct.brush", Value::String("D:/b.exe".into())).unwrap();
        overrides.set("paths.media_dir", Value::String("D:/media".into())).unwrap();

        let pipeline = std::fs::read_to_string(dir.join("pipeline.yaml")).unwrap();
        assert!(pipeline.contains("preset: draft") && pipeline.contains("brush: D:/b.exe"));
        assert!(std::fs::read_to_string(dir.join("klecks.yaml")).unwrap().contains("media_dir: D:/media"));
    }

    #[test]
    fn clearing_a_key_restores_the_default_and_tolerates_missing_keys() {
        let dir = temp("clear");
        let overrides = UserOverrides::new(Some(dir.clone()));
        overrides.set("pipeline.preset", Value::String("max".into())).unwrap();
        overrides.clear("pipeline.preset").unwrap();
        overrides.clear("pipeline.nothing.here").unwrap();
        assert!(!std::fs::read_to_string(dir.join("pipeline.yaml")).unwrap().contains("preset"));
    }

    #[test]
    fn written_overrides_load_back_over_the_defaults() {
        let dir = temp("roundtrip");
        let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        let loader = crate::SettingsLoader::new(config, Some(dir));
        loader.set_override("pipeline.preset", Value::String("draft".into())).unwrap();
        assert_eq!(loader.load().unwrap().pipeline.preset, "draft");
        loader.clear_override("pipeline.preset").unwrap();
        assert_eq!(loader.load().unwrap().pipeline.preset, "high");
    }
}
