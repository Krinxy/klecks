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

async fn validate(paths: Vec<String>, state: RwSignal<DropState>) {
    match TauriBridge::call::<Vec<VideoInfo>>("accept_videos", TauriBridge::args("paths", &paths)).await {
        Ok(videos) => state.set(DropState::Accepted(videos)),
        Err(message) => state.set(DropState::Rejected(message)),
    }
}

/// "IMG_7695.MP4" for one video, "5 videos" for several.
fn label(videos: &[VideoInfo]) -> String {
    match videos {
        [single] => single.name.clone(),
        many => format!("{} videos", many.len()),
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
            let paths = dropped_paths(&event);
            if paths.is_empty() {
                state.set(DropState::Idle);
            } else {
                leptos::task::spawn_local(validate(paths, state));
            }
        })
        .await;
    });

    let pick = move |_| {
        leptos::task::spawn_local(async move {
            match TauriBridge::call::<Vec<String>>("pick_videos", JsValue::UNDEFINED).await {
                Ok(paths) if !paths.is_empty() => validate(paths, state).await,
                Ok(_) => {}
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
                    DropState::Accepted(videos) => {
                        let title = label(&videos);
                        let paths: Vec<String> = videos.iter().map(|video| video.path.clone()).collect();
                        let total_mb: f64 = videos.iter().map(|video| video.size_mb).sum();
                        let names = videos.iter().map(|video| video.name.clone()).collect::<Vec<_>>().join(", ");
                        let scan_label = title.clone();
                        let several = videos.len() > 1;
                        view! {
                            <h1 class="card-title">{title}</h1>
                            <p class="card-text">
                                {if several {
                                    format!("{total_mb:.1} MB in total, all of the same room: {names}.")
                                } else {
                                    format!("{total_mb:.1} MB, ready to scan.")
                                }}
                            </p>
                            <div class="card-actions">
                                <button
                                    class="btn btn-primary"
                                    disabled=move || blocker.get().is_some()
                                    on:click=move |_| flow.start(paths.clone(), scan_label.clone())
                                >
                                    "Reconstruct room"
                                </button>
                                <button class="btn btn-ghost" on:click=pick>"Choose other videos"</button>
                            </div>
                            {move || blocker.get().map(|reason| view! { <p class="card-note card-note-bad">{reason}</p> })}
                        }.into_any()
                    }
                    DropState::Rejected(message) => view! {
                        <h1 class="card-title">{message}</h1>
                        <p class="card-text">"Drop other videos or choose them from your computer."</p>
                        <div class="card-actions">
                            <button class="btn btn-primary" on:click=pick>"Choose videos"</button>
                        </div>
                    }.into_any(),
                    DropState::Hover => view! {
                        <h1 class="card-title">"Release to load it"</h1>
                        <p class="card-text">"Drop the videos anywhere in this window."</p>
                    }.into_any(),
                    DropState::Idle => view! {
                        <h1 class="card-title">"Drop videos of your room"</h1>
                        <p class="card-text">
                            "Films of the same room belong together: drop several files or a whole folder and they become one scene. mp4, mov, mkv, webm and avi work."
                        </p>
                        <div class="card-actions">
                            <button class="btn btn-primary" on:click=pick>"Choose videos"</button>
                        </div>
                        <RecentScans flow=flow />
                    }.into_any(),
                }}
            </div>
        </Stage>
    }
}
