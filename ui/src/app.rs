// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::{
    catalog::CatalogScreen, dropzone::DropZone, menu::MainMenu, rooms::RoomsScreen,
    room::RoomScreen, scan_progress::ScanProgress, scene_viewer::SceneViewer, settings::SettingsScreen, stage::Stage,
    statusbar::StatusBar, titlebar::TitleBar,
};
use crate::flow::ScanFlow;
use crate::model::{AppStatus, View};

const NO_GPU: &str = "No allowed GPU was found, so a scan can't start. Details are at the bottom left.";

#[component]
pub fn App() -> impl IntoView {
    let status = RwSignal::new(None::<AppStatus>);
    let flow = ScanFlow::new();

    leptos::task::spawn_local(async move {
        flow.listen().await;
        match TauriBridge::call::<AppStatus>("app_status", JsValue::UNDEFINED).await {
            Ok(loaded) => {
                // Developer shortcuts (KLECKS_OPEN_SCENE / KLECKS_AUTOSCAN) skip the clicking.
                let hooks = loaded.startup.clone();
                status.set(Some(loaded));
                if let Some(scene) = hooks.scene {
                    flow.show(scene);
                } else if !hooks.autoscan_videos.is_empty() {
                    let label = format!("{} videos", hooks.autoscan_videos.len());
                    flow.start(hooks.autoscan_videos, label);
                }
            }
            Err(error) => leptos::logging::error!("app_status failed: {error}"),
        }
    });

    // Esc goes back to the menu from the menu screens (never while a scan runs or a scene is open).
    let escape = window_event_listener(leptos::ev::keydown, move |event| {
        if event.key() != "Escape" {
            return;
        }
        match flow.view.get_untracked() {
            View::NewScan | View::Rooms | View::Catalog | View::Settings => flow.home(),
            View::Room(_) => flow.go(View::Rooms),
            _ => {}
        }
    });
    on_cleanup(move || escape.remove());

    let version = Signal::derive(move || {
        status
            .get()
            .map(|s| format!("v{}", s.version))
            .unwrap_or_default()
    });
    // Why a scan can't start right now; None means it can.
    let blocker = Signal::derive(move || match status.get() {
        None => Some("Checking your GPU…".to_string()),
        Some(s) if s.gpu.is_none() => Some(NO_GPU.to_string()),
        Some(s) => s.reconstruct_error,
    });

    view! {
        <div class="shell">
            <TitleBar version=version />
            <main class="canvas">
                {move || match flow.view.get() {
                    View::Menu => view! { <MainMenu flow=flow /> }.into_any(),
                    View::NewScan => view! { <DropZone blocker=blocker flow=flow /> }.into_any(),
                    View::Rooms => view! { <RoomsScreen flow=flow blocker=blocker /> }.into_any(),
                    View::Room(id) => view! { <RoomScreen id=id flow=flow blocker=blocker /> }.into_any(),
                    View::Catalog => view! { <CatalogScreen flow=flow /> }.into_any(),
                    View::Settings => view! { <SettingsScreen flow=flow status=status /> }.into_any(),
                    View::Scanning(name) => view! { <ScanProgress name=name flow=flow /> }.into_any(),
                    View::Viewing(scan) => view! { <SceneViewer scan=scan flow=flow /> }.into_any(),
                    View::Failed(message) => view! {
                        <Stage mood="is-rejected">
                            <div class="card">
                                <h1 class="card-title">"The scan didn't finish"</h1>
                                <p class="card-text card-error">{message}</p>
                                <div class="card-actions">
                                    <button class="btn btn-primary" on:click=move |_| flow.home()>"Back"</button>
                                </div>
                            </div>
                        </Stage>
                    }.into_any(),
                }}
            </main>
            <StatusBar status=status />
        </div>
    }
}
