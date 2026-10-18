// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
mod commands;
mod library_commands;
mod state;

use state::AppState;
use tauri::Manager;

/// The viewer fetches scenes and the catalog shows photos through the asset protocol. Only the scans
/// folder and the catalog's photos are readable; call again whenever those folders may have moved.
pub fn refresh_asset_scope(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let config = state.config();
    let Some(paths) = config.paths.as_ref() else { return };
    let scope = app.asset_protocol_scope();
    for folder in [paths.scans.clone(), paths.media.join("assets")] {
        if let Err(error) = scope.allow_directory(&folder, true) {
            eprintln!("klecks: cannot allow {}: {error}", folder.display());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::boot();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|app| {
            refresh_asset_scope(app.handle());
            // Developer shortcut: a scan outside the scans folder may be opened on start.
            if let Some(scene) = &app.state::<AppState>().startup.scene {
                app.asset_protocol_scope().allow_directory(&scene.scan_dir, true)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::pick_videos,
            commands::accept_videos,
            commands::start_reconstruction,
            commands::cancel_reconstruction,
            commands::open_scan,
            library_commands::list_rooms,
            library_commands::room_overview,
            library_commands::set_door,
            library_commands::apartment,
            library_commands::list_assets,
            library_commands::list_scans,
            library_commands::start_room_scan,
            library_commands::get_settings,
            library_commands::set_setting,
            library_commands::reset_setting,
            library_commands::reveal_folder,
            commands::window_minimize,
            commands::window_toggle_maximize,
            commands::window_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running klecks");
}
