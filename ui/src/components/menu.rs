// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-11-01
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::room_picture::RoomPicture;
use crate::flow::ScanFlow;
use crate::format::Text;
use crate::model::{Asset, RoomView, View};

const ITEMS: [&str; 5] = ["Rooms", "Catalog", "New scan", "Settings", "Quit"];
const QUIT: usize = 4;

/// The start menu, in the manner of a game: skewed slabs, arrow keys and Enter. The newest room
/// sits on a tilted card on the right.
#[component]
pub fn MainMenu(flow: ScanFlow) -> impl IntoView {
    let selected = RwSignal::new(0usize);
    let rooms = RwSignal::new(None::<Vec<RoomView>>);
    let assets = RwSignal::new(None::<usize>);

    leptos::task::spawn_local(async move {
        if let Ok(list) = TauriBridge::call::<Vec<RoomView>>("list_rooms", JsValue::UNDEFINED).await {
            rooms.set(Some(list));
        }
        if let Ok(list) = TauriBridge::call::<Vec<Asset>>("list_assets", JsValue::UNDEFINED).await {
            assets.set(Some(list.len()));
        }
    });

    let choose = move |index: usize| match index {
        0 => flow.go(View::Rooms),
        1 => flow.go(View::Catalog),
        2 => flow.go(View::NewScan),
        3 => flow.go(View::Settings),
        _ => leptos::task::spawn_local(async {
            let _ = TauriBridge::fire("window_close").await;
        }),
    };

    let listener = window_event_listener(leptos::ev::keydown, move |event| {
        match event.key().as_str() {
            "ArrowDown" | "s" => selected.update(|index| *index = (*index + 1) % ITEMS.len()),
            "ArrowUp" | "w" => selected.update(|index| *index = (*index + ITEMS.len() - 1) % ITEMS.len()),
            "Enter" => choose(selected.get_untracked()),
            _ => return,
        }
        event.prevent_default();
    });
    on_cleanup(move || listener.remove());

    let detail = move |index: usize| match index {
        0 => rooms.get().map(|list| Text::count_of(list.len(), "room", "rooms")).unwrap_or_default(),
        1 => assets.get().map(|n| Text::count_of(n, "item", "items")).unwrap_or_default(),
        2 => "loose videos".to_string(),
        _ => String::new(),
    };

    // The room on the card: the one scanned last, else the first.
    let featured = move || {
        rooms.get().and_then(|list| {
            let newest = list
                .iter()
                .filter_map(|entry| entry.latest_scan.as_ref().map(|scan| (scan.modified_ms, entry.room.id.clone())))
                .max()
                .map(|(_, id)| id);
            list.into_iter().find(|entry| Some(&entry.room.id) == newest.as_ref())
        })
    };

    view! {
        <section class="slab-stage">
            <span class="slab-shape slab-shape-dark"></span>
            <span class="slab-shape slab-shape-khaki"></span>
            <span class="slab-shape slab-shape-black"></span>
            <span class="slab-shape slab-shape-paper"></span>

            <div class="slab-brand">
                <svg viewBox="0 0 24 24" aria-hidden="true">
                    <circle cx="12" cy="13" r="6.5" /><circle cx="6.5" cy="8" r="2.6" />
                    <circle cx="18" cy="7" r="2" /><circle cx="17.5" cy="18.5" r="1.6" />
                </svg>
                <span>"klecks"</span>
            </div>

            <nav aria-label="Main menu">
                <ul class="slab-list">
                    {ITEMS
                        .iter()
                        .enumerate()
                        .map(|(index, label)| view! {
                            <li>
                                <button
                                    class="slab"
                                    class:is-selected=move || selected.get() == index
                                    class:is-quiet=move || index == QUIT
                                    on:mouseenter=move |_| selected.set(index)
                                    on:focus=move |_| selected.set(index)
                                    on:click=move |_| choose(index)
                                >
                                    <span>
                                        <span class="slab-label">{*label}</span>
                                        <span class="slab-detail">{move || detail(index)}</span>
                                    </span>
                                </button>
                            </li>
                        })
                        .collect_view()}
                </ul>
            </nav>

            {move || {
                let entry = featured().or_else(|| rooms.get().and_then(|list| list.into_iter().next()));
                entry.map(|entry| {
                    let note = match &entry.latest_scan {
                        Some(scan) => format!(
                            "{}, scanned {}",
                            if scan.result.kind == "splats" { "Splat scene" } else { "Point cloud" },
                            Text::when(scan.modified_ms)
                        ),
                        None => "Not scanned yet".to_string(),
                    };
                    view! {
                        <div class="slab-room">
                            <div class="slab-room-photo">
                                <RoomPicture room_id=entry.room.id.clone() cover=entry.cover.clone() kind=entry.room.kind.clone() has_videos=!entry.room.videos.is_empty() || !entry.room.photos.is_empty() />
                            </div>
                            <div class="slab-room-line">
                                <span class="slab-room-name">{entry.room.name.clone()}</span>
                                <span class="slab-room-count">{crate::format::Text::media_count(entry.room.videos.len(), entry.room.photos.len())}</span>
                            </div>
                            <p class="slab-room-note">{note}</p>
                        </div>
                    }
                })
            }}

            <p class="slab-keys">
                <span>"W S or arrows"</span>
                <span>"Enter to choose"</span>
            </p>
        </section>
    }
}
