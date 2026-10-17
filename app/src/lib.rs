// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
mod commands;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::boot();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            // The viewer fetches scene.ply through the asset protocol; only scan folders are readable.
            let state = app.state::<AppState>();
            if let Some(paths) = &state.paths {
                app.asset_protocol_scope().allow_directory(&paths.scans, true)?;
            }
            if let Some(scene) = &state.startup.scene {
                app.asset_protocol_scope().allow_directory(&scene.scan_dir, true)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::pick_video,
            commands::accept_video,
            commands::start_reconstruction,
            commands::cancel_reconstruction,
            commands::list_scans,
            commands::open_scan,
            commands::window_minimize,
            commands::window_toggle_maximize,
            commands::window_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running klecks");
}
