// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("no config directory found (looked for config/klecks.yaml next to the program, in parent folders and via KLECKS_CONFIG_DIR)")]
    ConfigDirNotFound,
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid YAML in {path}: {source}")]
    Yaml {
        path: PathBuf,
        source: serde_yaml_ng::Error,
    },
    #[error("settings do not match the schema: {0}")]
    Schema(serde_yaml_ng::Error),
}
