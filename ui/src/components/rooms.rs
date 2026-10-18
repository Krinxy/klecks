// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::screen::Screen;
use crate::flow::ScanFlow;
use crate::format::{count_of, when};
use crate::model::{RoomView, ScanListItem};

/// "IMG_7694.MP4, IMG_7695.MP4 and 3 more"
fn video_names(room: &RoomView) -> String {
    let names: Vec<&str> = room.room.videos.iter().map(|video| video.name.as_str()).collect();
    if names.len() <= 3 {
        names.join(", ")
    } else {
        format!("{}, {} and {} more", names[0], names[1], names.len() - 2)
    }
}

fn reveal(kind: &'static str) {
    leptos::task::spawn_local(async move {
        let _ = TauriBridge::fire_with("reveal_folder", TauriBridge::map(&[("kind", kind)])).await;
    });
}

/// The rooms of the media library: what is scanned, what isn't, and a way to start or open each.
#[component]
pub fn RoomsScreen(flow: ScanFlow, #[prop(into)] blocker: Signal<Option<String>>) -> impl IntoView {
    let rooms = RwSignal::new(None::<Vec<RoomView>>);
    let loose = RwSignal::new(Vec::<ScanListItem>::new());
    let refresh = move || {
        leptos::task::spawn_local(async move {
            rooms.set(Some(
                TauriBridge::call::<Vec<RoomView>>("list_rooms", JsValue::UNDEFINED).await.unwrap_or_default(),
            ));
            loose.set(
                TauriBridge::call::<Vec<ScanListItem>>("list_scans", JsValue::UNDEFINED).await.unwrap_or_default(),
            );
        });
    };
    refresh();

    let actions = move || {
        view! {
            <button class="btn btn-ghost" on:click=move |_| refresh()>"Refresh"</button>
            <button class="btn btn-ghost" on:click=move |_| reveal("rooms")>"Open rooms folder"</button>
        }
    };

    view! {
        <Screen title="Rooms" flow=flow actions=actions>
            {move || match rooms.get() {
                None => view! { <p class="screen-note">"Looking for rooms…"</p> }.into_any(),
                Some(list) if list.is_empty() => view! {
                    <div class="empty">
                        <h2 class="empty-title">"No rooms yet"</h2>
                        <p class="empty-text">
                            "Make one folder per room in your media library and put that room's videos inside, for example "
                            <code>"rooms/livingroom"</code>
                            ". All videos of a folder become one scene."
                        </p>
                        <button class="btn btn-primary" on:click=move |_| reveal("rooms")>"Open rooms folder"</button>
                    </div>
                }.into_any(),
                Some(list) => view! {
                    <ul class="room-list">
                        {list.into_iter().map(|entry| room_card(entry, flow, blocker)).collect_view()}
                    </ul>
                }.into_any(),
            }}
            <Show when=move || !loose.get().is_empty()>
                <h2 class="section-title">"Other scans"</h2>
                <ul class="scan-rows">
                    <For
                        each=move || loose.get()
                        key=|scan| scan.name.clone()
                        children=move |scan| {
                            let result = scan.result.clone();
                            let kind = if scan.result.kind == "splats" { "Splat" } else { "Point cloud" };
                            view! {
                                <li>
                                    <button class="scan-row" on:click=move |_| flow.show(result.clone())>
                                        <span>{scan.room.clone()}</span>
                                        <span class="scan-meta">{format!("{kind}, {}", when(scan.modified_ms))}</span>
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
            </Show>
        </Screen>
    }
}

fn room_card(entry: RoomView, flow: ScanFlow, blocker: Signal<Option<String>>) -> impl IntoView {
    let has_videos = !entry.room.videos.is_empty();
    let id = entry.room.id.clone();
    let label = entry.room.name.clone();
    let names = video_names(&entry);
    let meta = if has_videos {
        format!("{}, {:.0} MB", count_of(entry.room.videos.len(), "video", "videos"), entry.total_mb)
    } else {
        "No videos yet".to_string()
    };
    let status = match &entry.latest_scan {
        Some(scan) => format!(
            "{} from {}",
            if scan.result.kind == "splats" { "Splat scene" } else { "Point cloud" },
            when(scan.modified_ms)
        ),
        None => "Not scanned yet".to_string(),
    };
    let scanned = entry.latest_scan.is_some();
    let open = entry.latest_scan.as_ref().map(|scan| scan.result.clone());
    let scan_id = id.clone();
    let scan_label = label.clone();
    let can_scan = move || has_videos && blocker.get().is_none();

    view! {
        <li class="room-card">
            <div class="room-main">
                <h2 class="room-name">{label}</h2>
                <p class="room-meta">{meta}</p>
                <p class="room-videos">{names}</p>
            </div>
            <div class="room-side">
                <span class="chip" class:chip-ok=scanned>{status}</span>
                <div class="room-actions">
                    {open.map(|result| view! {
                        <button class="btn btn-primary" on:click=move |_| flow.show(result.clone())>"Open"</button>
                    })}
                    <button
                        class=if scanned { "btn btn-ghost" } else { "btn btn-primary" }
                        disabled=move || !can_scan()
                        on:click=move |_| flow.scan_room(scan_id.clone(), scan_label.clone())
                    >
                        {if scanned { "Scan again" } else { "Reconstruct" }}
                    </button>
                </div>
                {move || (has_videos).then(|| blocker.get()).flatten().map(|reason| view! { <p class="room-blocker">{reason}</p> })}
            </div>
        </li>
    }
}
