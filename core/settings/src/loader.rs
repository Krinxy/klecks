// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use crate::error::SettingsError;
use crate::paths::AppPaths;
use crate::schema::Settings;

const CONFIG_ENV: &str = "KLECKS_CONFIG_DIR";
const MARKER_FILE: &str = "klecks.yaml";
const MAX_PARENT_LEVELS: usize = 6;

/// Loads every `*.yaml` in the shipped config dir, then overlays same-named files
/// from the user config dir key by key.
pub struct SettingsLoader {
    default_dir: PathBuf,
    user_dir: Option<PathBuf>,
}

impl SettingsLoader {
    pub fn new(default_dir: PathBuf, user_dir: Option<PathBuf>) -> Self {
        Self {
            default_dir,
            user_dir,
        }
    }

    /// Finds the shipped config dir: `KLECKS_CONFIG_DIR`, else `config/` next to the
    /// executable, else `config/` in a parent folder (development checkouts).
    pub fn discover() -> Result<Self, SettingsError> {
        let default_dir = Self::locate_default_dir()?;
        let user_dir = dirs::config_dir().map(|dir| dir.join("klecks"));
        Ok(Self::new(default_dir, user_dir))
    }

    pub fn default_dir(&self) -> &Path {
        &self.default_dir
    }

    pub fn load(&self) -> Result<Settings, SettingsError> {
        let mut merged = Value::Mapping(Mapping::new());
        Self::overlay_dir(&mut merged, &self.default_dir)?;
        if let Some(user_dir) = self.user_dir.as_deref().filter(|dir| dir.is_dir()) {
            Self::overlay_dir(&mut merged, user_dir)?;
        }
        serde_yaml_ng::from_value(merged).map_err(SettingsError::Schema)
    }

    /// Settings plus the resolved storage locations.
    pub fn load_with_paths(&self) -> Result<(Settings, AppPaths), SettingsError> {
        let settings = self.load()?;
        let paths = AppPaths::resolve(&settings.paths);
        Ok((settings, paths))
    }

    fn locate_default_dir() -> Result<PathBuf, SettingsError> {
        if let Some(from_env) = std::env::var_os(CONFIG_ENV).map(PathBuf::from) {
            if from_env.join(MARKER_FILE).is_file() {
                return Ok(from_env);
            }
        }
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let mut cursor = exe_dir.as_deref();
        for _ in 0..=MAX_PARENT_LEVELS {
            let Some(dir) = cursor else { break };
            let candidate = dir.join("config");
            if candidate.join(MARKER_FILE).is_file() {
                return Ok(candidate);
            }
            cursor = dir.parent();
        }
        Err(SettingsError::ConfigDirNotFound)
    }

    fn overlay_dir(target: &mut Value, dir: &Path) -> Result<(), SettingsError> {
        let io_err = |source| SettingsError::Io {
            path: dir.to_path_buf(),
            source,
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(io_err)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
            .collect();
        files.sort();
        for file in files {
            let text = std::fs::read_to_string(&file).map_err(|source| SettingsError::Io {
                path: file.clone(),
                source,
            })?;
            let value: Value = serde_yaml_ng::from_str(&text).map_err(|source| SettingsError::Yaml {
                path: file.clone(),
                source,
            })?;
            Self::deep_merge(target, value);
        }
        Ok(())
    }

    fn deep_merge(target: &mut Value, incoming: Value) {
        match (target, incoming) {
            (Value::Mapping(base), Value::Mapping(extra)) => {
                for (key, value) in extra {
                    match base.get_mut(&key) {
                        Some(existing) => Self::deep_merge(existing, value),
                        None => {
                            base.insert(key, value);
                        }
                    }
                }
            }
            (slot, value) => *slot = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_config() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config")
    }

    #[test]
    fn shipped_defaults_match_the_schema() {
        let settings = SettingsLoader::new(repo_config(), None).load().unwrap();
        assert!(settings.gpu.require_gpu);
        assert_eq!(settings.gpu.allowed_vendors, vec!["nvidia"]);
        assert!(settings.pipeline.active_preset().is_some());
        assert!(!settings.gemini.enabled);
    }

    #[test]
    fn user_overrides_replace_single_keys_only() {
        let dir = std::env::temp_dir().join("klecks-settings-test-override");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("klecks.yaml"), "gpu:\n  allowed_vendors: [nvidia, amd]\n").unwrap();
        let settings = SettingsLoader::new(repo_config(), Some(dir)).load().unwrap();
        assert_eq!(settings.gpu.allowed_vendors, vec!["nvidia", "amd"]);
        assert!(settings.gpu.require_gpu, "untouched keys keep their default");
    }
}
