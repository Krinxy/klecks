// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::bridge::Viewer3d;
use crate::flow::ScanFlow;
use crate::model::ScanResult;

const HOST_ID: &str = "klecks-viewer";

/// 12345678 -> "12,345,678"
fn with_commas(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The finished scene: fly through the point cloud of your room.
#[component]
pub fn SceneViewer(scan: ScanResult, flow: ScanFlow) -> impl IntoView {
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let host = NodeRef::<leptos::html::Div>::new();
    let path = scan.scene_ply.clone();

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
            if let Err(message) = Viewer3d::load(&path).await {
                error.set(Some(message));
            }
            loading.set(false);
        });
        true
    });
    on_cleanup(Viewer3d::dispose);

    let points = with_commas(scan.report.view_points);
    view! {
        <div class="viewer">
            <div id=HOST_ID node_ref=host class="viewer-host"></div>
            <div class="hud hud-top">
                <span class="chip">{format!("{points} points")}</span>
                <span class="spacer"></span>
                <button class="btn btn-ghost" on:click=move |_| Viewer3d::reset_view()>"Reset view"</button>
                <button class="btn btn-primary" on:click=move |_| flow.home()>"New scan"</button>
            </div>
            <p class="hud-hint">"Drag to look around, scroll to zoom, W A S D to walk, right-drag to pan."</p>
            <Show when=move || loading.get()>
                <div class="viewer-overlay">"Loading the scene…"</div>
            </Show>
            {move || error.get().map(|message| view! { <div class="viewer-overlay viewer-overlay-bad">{message}</div> })}
        </div>
    }
}
