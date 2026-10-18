// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::bridge::Viewer3d;
use crate::flow::ScanFlow;
use crate::format::with_commas;
use crate::model::ScanResult;

const HOST_ID: &str = "klecks-viewer";

/// The finished scene: fly through the point cloud of your room.
#[component]
pub fn SceneViewer(scan: ScanResult, flow: ScanFlow) -> impl IntoView {
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let summary = RwSignal::new(String::new());
    let host = NodeRef::<leptos::html::Div>::new();
    let path = scan.scene_ply.clone();
    let scale = scan.report.median_depth;

    // The element must exist before three.js can attach to it, hence the effect.
    Effect::new(move |started: Option<bool>| {
        if started == Some(true) || host.get().is_none() {
            return started.unwrap_or(false);
        }
        let path = path.clone();
        leptos::task::spawn_local(async move {
            if !Viewer3d::mount(HOST_ID) {
                error.set(Some("The 3D view could not start.".into()));
                loading.set(false);
                return;
            }
            match Viewer3d::load(&path, scale).await {
                Ok(scene) => {
                    let noun = if scene.kind == "splats" { "splats" } else { "points" };
                    summary.set(format!("{} {noun}", with_commas(scene.count)));
                }
                Err(message) => error.set(Some(message)),
            }
            loading.set(false);
        });
        true
    });
    on_cleanup(Viewer3d::dispose);

    view! {
        <div class="viewer">
            <div id=HOST_ID node_ref=host class="viewer-host"></div>
            <div class="hud hud-top">
                <span class="chip">{move || summary.get()}</span>
                <span class="spacer"></span>
                <button class="btn btn-ghost" on:click=move |_| Viewer3d::reset_view()>"Reset view"</button>
                <button class="btn btn-primary" on:click=move |_| flow.home()>"Menu"</button>
            </div>
            <p class="hud-hint">"Drag to look around, scroll to zoom, W A S D to walk, right-drag to pan."</p>
            <Show when=move || loading.get()>
                <div class="viewer-overlay">"Loading the scene…"</div>
            </Show>
            {move || error.get().map(|message| view! { <div class="viewer-overlay viewer-overlay-bad">{message}</div> })}
        </div>
    }
}
