// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
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

/// The one input of the app: drop a video of the room here, or click to pick one.
#[component]
pub fn DropZone(#[prop(into)] gpu_ready: Signal<bool>) -> impl IntoView {
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

    view! {
        <section class="stage">
            <div
                class="dropzone"
                class:is-hover=move || state.get() == DropState::Hover
                class:is-accepted=move || matches!(state.get(), DropState::Accepted(_))
                class:is-rejected=move || matches!(state.get(), DropState::Rejected(_))
                on:click=pick
                role="button"
                tabindex="0"
            >
                <span class="reg reg-tl"></span>
                <span class="reg reg-tr"></span>
                <span class="reg reg-bl"></span>
                <span class="reg reg-br"></span>
                {move || match state.get() {
                    DropState::Accepted(video) => view! {
                        <div class="drop-body">
                            <p class="eyebrow">"video ready"</p>
                            <h2 class="drop-title">{video.name.clone()}</h2>
                            <p class="drop-sub mono">{format!("{:.1} MB", video.size_mb)}</p>
                            <button
                                class="btn-primary"
                                disabled=move || !gpu_ready.get()
                                title=move || if gpu_ready.get() {
                                    "The reconstruction pipeline arrives in the next phase"
                                } else {
                                    "No allowed GPU — see the status bar"
                                }
                                on:click=move |ev| ev.stop_propagation()
                            >
                                "Reconstruct room"
                            </button>
                        </div>
                    }.into_any(),
                    DropState::Rejected(message) => view! {
                        <div class="drop-body">
                            <p class="eyebrow eyebrow-bad">"can't use that"</p>
                            <h2 class="drop-title">{message}</h2>
                            <p class="drop-sub">"Drop another video or click to choose one."</p>
                        </div>
                    }.into_any(),
                    DropState::Hover => view! {
                        <div class="drop-body">
                            <p class="eyebrow">"release to load"</p>
                            <h2 class="drop-title">"Drop it"</h2>
                        </div>
                    }.into_any(),
                    DropState::Idle => view! {
                        <div class="drop-body">
                            <svg class="crosshair" viewBox="0 0 48 48" aria-hidden="true">
                                <circle cx="24" cy="24" r="9" />
                                <path d="M24 2v12M24 34v12M2 24h12M34 24h12" />
                            </svg>
                            <h2 class="drop-title">"Drop a video of your room"</h2>
                            <p class="drop-sub">"or click to choose a file · mp4 mov mkv webm avi"</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </section>
    }
}
