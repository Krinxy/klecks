// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Runs a full reconstruction from the command line, without the GUI:
//   cargo run -p klecks-reconstruct --example scan -- media/IMG_7695.MP4 [cancel-after-seconds]
use std::path::PathBuf;

use klecks_gpu::GpuGuard;
use klecks_reconstruct::{CancelToken, PythonLabReconstructor, Reconstructor, ScanRequest};
use klecks_settings::SettingsLoader;

fn main() {
    let video = PathBuf::from(std::env::args().nth(1).expect("usage: scan <video>"));
    let loader = SettingsLoader::discover().expect("config/ not found");
    let (settings, paths) = loader.load_with_paths().expect("config invalid");

    let gpu = GpuGuard::new(&settings.gpu).detect().expect("no allowed GPU");
    println!("GPU: {}", gpu.name);

    let backend = PythonLabReconstructor::new(&settings.pipeline.reconstruct, loader.default_dir())
        .expect("backend not configured");
    let request = ScanRequest::new(&video, &paths.scans);
    println!("scan folder: {}", request.scan_dir.display());

    // Optional second argument: cancel after N seconds (to test stopping a running scan).
    let cancel = CancelToken::new();
    if let Some(seconds) = std::env::args().nth(2).and_then(|value| value.parse::<u64>().ok()) {
        let token = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(seconds));
            token.cancel();
        });
    }

    let result = match backend.run(&request, &cancel, &mut |event| {
        println!("[{:>3}%] {:<10} {}", event.pct, event.stage, event.message)
    }) {
        Ok(result) => result,
        Err(error) => {
            println!("stopped: {error}");
            return;
        }
    };
    println!("done: {} view points, {:.1}s inference", result.report.view_points, result.report.inference_seconds);
    println!("scene: {}", result.scene_ply.display());
}
