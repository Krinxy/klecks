// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::recent_scans::RecentScans;
use crate::components::stage::Stage;
use crate::flow::ScanFlow;
use crate::model::{DropState, VideoInfo};

/// Reads `event.payload.paths` from a Tauri drag-drop event.
fn dropped_paths(event: &JsValue) -> Vec<String> {
    let payload = js_sys::Reflect::get(event, &"payload".into()).unwrap_or(JsValue::UNDEFINED);
    let paths = js_sys::Reflect::get(&payload, &"paths".into()).unwrap_or(JsValue::UNDEFINED);
    js_sys::Array::from(&paths)
        .iter()
        .filter_map(|path| path.as_string())
        .collect()
}

async fn validate(path: String, state: RwSignal<DropState>) {
    match TauriBridge::call::<VideoInfo>("accept_video", TauriBridge::arg("path", &path)).await {
        Ok(video) => state.set(DropState::Accepted(video)),
        Err(message) => state.set(DropState::Rejected(message)),
    }
}

/// The one input of the app: a video of the room, dropped anywhere in the window or chosen
/// with the file dialog. Behind the card a field of soft "splats" reacts to the drag.
/// `blocker` is the reason a scan can't start right now (no GPU, backend not set up).
#[component]
pub fn DropZone(#[prop(into)] blocker: Signal<Option<String>>, flow: ScanFlow) -> impl IntoView {
    let state = RwSignal::new(DropState::Idle);

    leptos::task::spawn_local(async move {
        TauriBridge::listen("tauri://drag-enter", move |_| state.set(DropState::Hover)).await;
        TauriBridge::listen("tauri://drag-leave", move |_| state.set(DropState::Idle)).await;
        TauriBridge::listen("tauri://drag-drop", move |event| {
            match dropped_paths(&event).into_iter().next() {
                Some(path) => leptos::task::spawn_local(validate(path, state)),
                None => state.set(DropState::Idle),
            }
        })
        .await;
    });

    let pick = move |_| {
        leptos::task::spawn_local(async move {
            match TauriBridge::call::<Option<String>>("pick_video", JsValue::UNDEFINED).await {
                Ok(Some(path)) => validate(path, state).await,
                Ok(None) => {}
                Err(message) => state.set(DropState::Rejected(message)),
            }
        });
    };

    let mood = Signal::derive(move || match state.get() {
        DropState::Hover => "is-hover",
        DropState::Accepted(_) => "is-accepted",
        DropState::Rejected(_) => "is-rejected",
        DropState::Idle => "",
    });

    view! {
        <Stage mood=mood>
            <div class="card">
                {move || match state.get() {
                    DropState::Accepted(video) => {
                        let to_scan = video.clone();
                        view! {
                            <h1 class="card-title">{video.name.clone()}</h1>
                            <p class="card-text">{format!("{:.1} MB, ready to scan.", video.size_mb)}</p>
                            <div class="card-actions">
                                <button
                                    class="btn btn-primary"
                                    disabled=move || blocker.get().is_some()
                                    on:click=move |_| flow.start(to_scan.path.clone(), to_scan.name.clone())
                                >
                                    "Reconstruct room"
                                </button>
                                <button class="btn btn-ghost" on:click=pick>"Choose another"</button>
                            </div>
                            {move || blocker.get().map(|reason| view! { <p class="card-note card-note-bad">{reason}</p> })}
                        }.into_any()
                    }
                    DropState::Rejected(message) => view! {
                        <h1 class="card-title">{message}</h1>
                        <p class="card-text">"Drop another video or choose one from your computer."</p>
                        <div class="card-actions">
                            <button class="btn btn-primary" on:click=pick>"Choose video"</button>
                        </div>
                    }.into_any(),
                    DropState::Hover => view! {
                        <h1 class="card-title">"Release to load it"</h1>
                        <p class="card-text">"Drop the video anywhere in this window."</p>
                    }.into_any(),
                    DropState::Idle => view! {
                        <h1 class="card-title">"Drop a video of your room"</h1>
                        <p class="card-text">
                            "Walk slowly through the room, then drop the file here. mp4, mov, mkv, webm and avi work."
                        </p>
                        <div class="card-actions">
                            <button class="btn btn-primary" on:click=pick>"Choose video"</button>
                        </div>
                        <RecentScans flow=flow />
                    }.into_any(),
                }}
            </div>
        </Stage>
    }
}
