// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::stage::Stage;
use crate::flow::ScanFlow;
use crate::format::count_of;
use crate::model::{Asset, RoomView, View};

const ITEMS: [&str; 5] = ["Rooms", "Catalog", "New scan", "Settings", "Quit"];

/// The start menu, in the manner of a game: big words, arrow keys and Enter.
#[component]
pub fn MainMenu(flow: ScanFlow) -> impl IntoView {
    let selected = RwSignal::new(0usize);
    let rooms = RwSignal::new(None::<usize>);
    let assets = RwSignal::new(None::<usize>);

    leptos::task::spawn_local(async move {
        if let Ok(list) = TauriBridge::call::<Vec<RoomView>>("list_rooms", JsValue::UNDEFINED).await {
            rooms.set(Some(list.len()));
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
            "ArrowDown" => selected.update(|index| *index = (*index + 1) % ITEMS.len()),
            "ArrowUp" => selected.update(|index| *index = (*index + ITEMS.len() - 1) % ITEMS.len()),
            "Enter" => choose(selected.get_untracked()),
            _ => return,
        }
        event.prevent_default();
    });
    on_cleanup(move || listener.remove());

    let detail = move |index: usize| match index {
        0 => rooms.get().map(|n| count_of(n, "room", "rooms")).unwrap_or_default(),
        1 => assets.get().map(|n| count_of(n, "item", "items")).unwrap_or_default(),
        2 => "loose videos".to_string(),
        3 => "quality, folders".to_string(),
        _ => String::new(),
    };

    view! {
        <Stage mood="">
            <nav class="menu" aria-label="Main menu">
                <p class="wordmark">"klecks"</p>
                <p class="tagline">"Rebuild a room from video."</p>
                <ul class="menu-list">
                    {ITEMS
                        .iter()
                        .enumerate()
                        .map(|(index, label)| view! {
                            <li>
                                <button
                                    class="menu-item"
                                    class:is-selected=move || selected.get() == index
                                    on:mouseenter=move |_| selected.set(index)
                                    on:focus=move |_| selected.set(index)
                                    on:click=move |_| choose(index)
                                >
                                    <span class="menu-label">{*label}</span>
                                    <span class="menu-detail">{move || detail(index)}</span>
                                </button>
                            </li>
                        })
                        .collect_view()}
                </ul>
                <p class="menu-keys">"Up and down to move, Enter to choose"</p>
            </nav>
        </Stage>
    }
}
