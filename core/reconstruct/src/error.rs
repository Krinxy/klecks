// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
#[derive(Debug, thiserror::Error)]
pub enum ReconstructError {
    #[error("Reconstruction is not set up: {0}")]
    NotConfigured(String),
    #[error("Could not start the reconstruction process: {0}")]
    Spawn(std::io::Error),
    #[error("The scan folder could not be prepared: {0}")]
    ScanDir(std::io::Error),
    #[error("Reconstruction failed (exit code {code}).{detail}")]
    Failed { code: i32, detail: String },
    #[error("Reconstruction was cancelled.")]
    Cancelled,
    #[error("Reconstruction finished but {0} is missing.")]
    MissingOutput(String),
}
