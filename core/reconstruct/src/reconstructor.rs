// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use crate::{CancelToken, ProgressEvent, ReconstructError, ScanRequest, ScanResult};

/// A way to turn a video into a 3D point cloud. Today a Python lab process, later an ONNX model
/// run from Rust; the app only talks to this trait.
pub trait Reconstructor: Send + Sync {
    fn run(
        &self,
        request: &ScanRequest,
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<ScanResult, ReconstructError>;
}
