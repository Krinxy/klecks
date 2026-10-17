// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
// Prints which GPU klecks would use, or why it refuses to start:
//   cargo run -p klecks-gpu --example detect
use klecks_gpu::GpuGuard;
use klecks_settings::SettingsLoader;

fn main() {
    let settings = SettingsLoader::discover()
        .and_then(|loader| loader.load())
        .expect("config/ not found or invalid");
    match GpuGuard::new(&settings.gpu).detect() {
        Ok(gpu) => println!("OK: {gpu:?}"),
        Err(error) => println!("REFUSED: {error}"),
    }
}
