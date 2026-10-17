// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::{
    dropzone::DropZone, scan_progress::ScanProgress, scene_viewer::SceneViewer, stage::Stage,
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
                } else if let Some(video) = hooks.autoscan_video {
                    let name = video.rsplit(['\\', '/']).next().unwrap_or("video").to_string();
                    flow.start(video, name);
                }
            }
            Err(error) => leptos::logging::error!("app_status failed: {error}"),
        }
    });

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
                    View::Welcome => view! { <DropZone blocker=blocker flow=flow /> }.into_any(),
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
