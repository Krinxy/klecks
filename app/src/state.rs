// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::path::Path;
use std::sync::{Arc, Mutex};

use klecks_gpu::{GpuGuard, GpuInfo};
use klecks_reconstruct::{CancelToken, PythonLabReconstructor, Reconstructor, ScanResult};
use klecks_settings::{AppPaths, Settings, SettingsLoader};
use serde::Serialize;

const OPEN_SCENE_ENV: &str = "KLECKS_OPEN_SCENE";
const AUTOSCAN_ENV: &str = "KLECKS_AUTOSCAN";

/// Developer shortcuts so the GUI can be checked without clicking:
/// `KLECKS_OPEN_SCENE=<scan dir>` opens the viewer, `KLECKS_AUTOSCAN=<video>` starts a scan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StartupHooks {
    pub scene: Option<ScanResult>,
    pub autoscan_video: Option<String>,
}

impl StartupHooks {
    fn from_env() -> Self {
        Self {
            scene: std::env::var_os(OPEN_SCENE_ENV)
                .and_then(|dir| ScanResult::from_dir(Path::new(&dir))),
            autoscan_video: std::env::var(AUTOSCAN_ENV).ok().filter(|v| !v.trim().is_empty()),
        }
    }
}

/// Everything resolved once at startup. A missing GPU or broken config does not crash
/// the window; it is kept as a message so the UI can show exactly what is wrong.
pub struct AppState {
    pub settings: Option<Settings>,
    pub paths: Option<AppPaths>,
    pub gpu: Result<GpuInfo, String>,
    pub config_error: Option<String>,
    pub reconstructor: Result<Arc<dyn Reconstructor>, String>,
    /// Cancel handle of the scan that is running right now, if any.
    pub job: Mutex<Option<CancelToken>>,
    pub startup: StartupHooks,
}

impl AppState {
    pub fn boot() -> Self {
        let loader = match SettingsLoader::discover() {
            Ok(loader) => loader,
            Err(error) => return Self::broken(error.to_string()),
        };
        let (settings, paths) = match loader.load_with_paths() {
            Ok(loaded) => loaded,
            Err(error) => return Self::broken(error.to_string()),
        };
        let gpu = GpuGuard::new(&settings.gpu)
            .detect()
            .map_err(|error| error.to_string());
        if let Err(error) = paths.ensure() {
            eprintln!("klecks: cannot create data directories: {error}");
        }
        let reconstructor = PythonLabReconstructor::new(&settings.pipeline.reconstruct, loader.default_dir())
            .map(|backend| Arc::new(backend) as Arc<dyn Reconstructor>)
            .map_err(|error| error.to_string());
        Self {
            settings: Some(settings),
            paths: Some(paths),
            gpu,
            config_error: None,
            reconstructor,
            job: Mutex::new(None),
            startup: StartupHooks::from_env(),
        }
    }

    fn broken(config_error: String) -> Self {
        Self {
            settings: None,
            paths: None,
            gpu: Err("GPU check skipped: configuration could not be loaded".into()),
            config_error: Some(config_error),
            reconstructor: Err("configuration could not be loaded".into()),
            job: Mutex::new(None),
            startup: StartupHooks::default(),
        }
    }
}
