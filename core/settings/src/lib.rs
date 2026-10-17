// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
mod error;
mod loader;
mod paths;
mod schema;

pub use error::SettingsError;
pub use loader::SettingsLoader;
pub use paths::AppPaths;
pub use schema::{
    AppSection, BudgetSettings, CaptureSettings, FloorplanSettings, GeminiSettings, GpuSettings,
    PathsSection, PipelineSettings, PresetSettings, ReconstructSettings, ScaleSettings, Settings,
};
