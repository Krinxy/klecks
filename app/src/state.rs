// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use klecks_gpu::{GpuGuard, GpuInfo};
use klecks_settings::{AppPaths, Settings, SettingsLoader};

/// Everything resolved once at startup. A missing GPU or broken config does not crash
/// the window; it is kept as a message so the UI can show exactly what is wrong.
pub struct AppState {
    pub settings: Option<Settings>,
    pub paths: Option<AppPaths>,
    pub gpu: Result<GpuInfo, String>,
    pub config_error: Option<String>,
}

impl AppState {
    pub fn boot() -> Self {
        let loaded = SettingsLoader::discover().and_then(|loader| loader.load_with_paths());
        match loaded {
            Ok((settings, paths)) => {
                let gpu = GpuGuard::new(&settings.gpu)
                    .detect()
                    .map_err(|error| error.to_string());
                if let Err(error) = paths.ensure() {
                    eprintln!("klecks: cannot create data directories: {error}");
                }
                Self {
                    settings: Some(settings),
                    paths: Some(paths),
                    gpu,
                    config_error: None,
                }
            }
            Err(error) => Self {
                settings: None,
                paths: None,
                gpu: Err("GPU check skipped: configuration could not be loaded".into()),
                config_error: Some(error.to_string()),
            },
        }
    }
}
