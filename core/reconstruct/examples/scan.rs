// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Runs a full reconstruction from the command line, without the GUI:
//   cargo run -p klecks-reconstruct --example scan -- media/a.mp4 media/b.mp4 [--cancel-after 20]
use std::path::PathBuf;

use klecks_gpu::GpuGuard;
use klecks_reconstruct::{CancelToken, PythonLabReconstructor, Reconstructor, ScanRequest};
use klecks_settings::SettingsLoader;

fn main() {
    let mut videos = Vec::new();
    let mut cancel_after = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--cancel-after" {
            cancel_after = args.next().and_then(|value| value.parse::<u64>().ok());
        } else {
            videos.push(PathBuf::from(arg));
        }
    }
    assert!(!videos.is_empty(), "usage: scan <video>... [--cancel-after <seconds>]");

    let loader = SettingsLoader::discover().expect("config/ not found");
    let (settings, paths) = loader.load_with_paths().expect("config invalid");

    let gpu = GpuGuard::new(&settings.gpu).detect().expect("no allowed GPU");
    println!("GPU: {}", gpu.name);

    let backend = PythonLabReconstructor::new(&settings.pipeline.reconstruct, loader.default_dir())
        .expect("backend not configured");
    let request = ScanRequest::new(&videos, &paths.scans);
    println!("scan folder: {}", request.scan_dir.display());

    let cancel = CancelToken::new();
    if let Some(seconds) = cancel_after {
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
    println!("done: {:?}, {} frames, {:.1}s inference", result.kind, result.report.frames, result.report.inference_seconds);
    println!("scene: {}", result.scene_ply.display());
}
