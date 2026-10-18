// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};

use klecks_gpu::{GpuGuard, GpuInfo};
use klecks_reconstruct::{CancelToken, PythonLabReconstructor, Reconstructor, ScanResult};
use klecks_settings::{AppPaths, Settings, SettingsLoader};
use serde::Serialize;

const OPEN_SCENE_ENV: &str = "KLECKS_OPEN_SCENE";
const AUTOSCAN_ENV: &str = "KLECKS_AUTOSCAN";

/// Developer shortcuts so the GUI can be checked without clicking:
/// `KLECKS_OPEN_SCENE=<scan dir>` opens the viewer, `KLECKS_AUTOSCAN=<video>;<video>...` starts a scan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StartupHooks {
    pub scene: Option<ScanResult>,
    pub autoscan_videos: Vec<String>,
}

impl StartupHooks {
    fn from_env() -> Self {
        Self {
            scene: std::env::var_os(OPEN_SCENE_ENV)
                .and_then(|dir| ScanResult::from_dir(Path::new(&dir))),
            autoscan_videos: std::env::var(AUTOSCAN_ENV)
                .map(|list| {
                    list.split(';')
                        .map(str::trim)
                        .filter(|path| !path.is_empty())
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Everything that depends on the settings. It is rebuilt when the user changes one, so a new
/// preset or media folder applies right away without restarting the app.
pub struct Config {
    pub settings: Option<Settings>,
    pub paths: Option<AppPaths>,
    pub error: Option<String>,
    pub reconstructor: Result<Arc<dyn Reconstructor>, String>,
}

impl Config {
    fn load(loader: &SettingsLoader) -> Self {
        match loader.load_with_paths() {
            Ok((settings, paths)) => {
                if let Err(error) = paths.ensure() {
                    eprintln!("klecks: cannot create data directories: {error}");
                }
                let reconstructor = PythonLabReconstructor::new(&settings.pipeline, loader.default_dir())
                    .map(|backend| Arc::new(backend) as Arc<dyn Reconstructor>)
                    .map_err(|error| error.to_string());
                Self { settings: Some(settings), paths: Some(paths), error: None, reconstructor }
            }
            Err(error) => Self::broken(error.to_string()),
        }
    }

    fn broken(error: String) -> Self {
        Self {
            settings: None,
            paths: None,
            error: Some(error),
            reconstructor: Err("configuration could not be loaded".into()),
        }
    }
}

/// Resolved at startup. A missing GPU or a broken config does not crash the window; it is kept as a
/// message so the UI can show exactly what is wrong.
pub struct AppState {
    pub loader: Option<SettingsLoader>,
    config: RwLock<Config>,
    pub gpu: Result<GpuInfo, String>,
    /// Cancel handle of the scan that is running right now, if any.
    pub job: Mutex<Option<CancelToken>>,
    pub startup: StartupHooks,
}

impl AppState {
    pub fn boot() -> Self {
        let loader = SettingsLoader::discover();
        let (loader, config) = match loader {
            Ok(loader) => {
                let config = Config::load(&loader);
                (Some(loader), config)
            }
            Err(error) => (None, Config::broken(error.to_string())),
        };
        let gpu = match &config.settings {
            Some(settings) => GpuGuard::new(&settings.gpu).detect().map_err(|error| error.to_string()),
            None => Err("GPU check skipped: configuration could not be loaded".into()),
        };
        Self {
            loader,
            config: RwLock::new(config),
            gpu,
            job: Mutex::new(None),
            startup: StartupHooks::from_env(),
        }
    }

    /// The current settings-derived state. Keep the guard short-lived.
    pub fn config(&self) -> RwLockReadGuard<'_, Config> {
        self.config.read().expect("config lock is never poisoned by a panic-free writer")
    }

    /// Re-reads all settings files (after the user changed one).
    pub fn reload(&self) {
        if let Some(loader) = &self.loader {
            let fresh = Config::load(loader);
            *self.config.write().expect("config lock is never poisoned by a panic-free writer") = fresh;
        }
    }
}
