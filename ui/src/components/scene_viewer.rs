// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use std::time::Duration;

use leptos::prelude::*;

use crate::bridge::Viewer3d;
use crate::flow::ScanFlow;
use crate::format::with_commas;
use crate::model::ScanResult;

const HOST_ID: &str = "klecks-viewer";
/// After this long the loading text offers to start over.
const SLOW_AFTER: Duration = Duration::from_secs(15);

/// The finished scene: fly through the point cloud of your room.
#[component]
pub fn SceneViewer(scan: ScanResult, flow: ScanFlow, #[prop(optional)] embedded: bool) -> impl IntoView {
    let enlarged = scan.clone();
    let loading = RwSignal::new(true);
    let slow = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let summary = RwSignal::new(String::new());
    let attempt = RwSignal::new(0u32);
    let host = NodeRef::<leptos::html::Div>::new();
    let path = scan.scene_ply.clone();
    let scale = scan.report.median_depth;

    // The element must exist before three.js can attach to it, hence the effect. It runs again for every try.
    Effect::new(move |_| {
        let this_try = attempt.get();
        if host.get().is_none() {
            return;
        }
        loading.set(true);
        slow.set(false);
        error.set(None);
        set_timeout(
            move || {
                if loading.get_untracked() && attempt.get_untracked() == this_try {
                    slow.set(true);
                }
            },
            SLOW_AFTER,
        );
        let path = path.clone();
        leptos::task::spawn_local(async move {
            if !Viewer3d::mount(HOST_ID) {
                error.set(Some("The 3D view could not start.".into()));
                loading.set(false);
                return;
            }
            let outcome = Viewer3d::load(&path, scale).await;
            if attempt.get_untracked() != this_try {
                return; // the person started over meanwhile; this answer is old
            }
            match outcome {
                Ok(scene) => {
                    let noun = if scene.kind == "splats" { "splats" } else { "points" };
                    summary.set(format!("{} {noun}", with_commas(scene.count)));
                }
                Err(message) => error.set(Some(message)),
            }
            loading.set(false);
        });
    });
    on_cleanup(Viewer3d::dispose);

    view! {
        <div class="viewer" class:viewer-embedded=embedded>
            <div id=HOST_ID node_ref=host class="viewer-host"></div>
            <div class="hud hud-top">
                <span class="chip">{move || summary.get()}</span>
                <span class="spacer"></span>
                <button class="btn btn-ghost" on:click=move |_| Viewer3d::reset_view()>"Reset view"</button>
                {if embedded {
                    view! { <button class="btn btn-primary" on:click=move |_| flow.show(enlarged.clone())>"Full screen"</button> }.into_any()
                } else {
                    view! { <button class="btn btn-primary" on:click=move |_| flow.home()>"Menu"</button> }.into_any()
                }}
            </div>
            <p class="hud-hint">"Drag to look around, scroll to zoom, W A S D to walk, right-drag to pan."</p>
            <Show when=move || loading.get()>
                <div class="viewer-overlay">
                    <div class="overlay-box">
                        <span id="klecks-viewer-status">"Loading the scene…"</span>
                        <Show when=move || slow.get()>
                            <p class="overlay-hint">"This takes longer than usual."</p>
                            <button class="btn btn-primary" on:click=move |_| attempt.update(|n| *n += 1)>"Try again"</button>
                        </Show>
                    </div>
                </div>
            </Show>
            {move || error.get().map(|message| view! {
                <div class="viewer-overlay viewer-overlay-bad">
                    <div class="overlay-box">
                        <span>{message}</span>
                        <button class="btn btn-primary" on:click=move |_| attempt.update(|n| *n += 1)>"Try again"</button>
                    </div>
                </div>
            })}
        </div>
    }
}
