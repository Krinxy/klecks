// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;

/// The kinds of room the "+" offers: (id kept in room.json, what the person reads).
pub const KINDS: [(&str, &str); 8] = [
    ("livingroom", "Living room"),
    ("bedroom", "Bedroom"),
    ("office", "Office"),
    ("kitchen", "Kitchen"),
    ("hallway", "Hallway"),
    ("bathroom", "Bathroom"),
    ("dining", "Dining room"),
    ("other", "Other"),
];

/// A line drawing for a kind of room: what stands in for a picture until the room has one.
pub fn kind_icon(kind: &str) -> impl IntoView {
    let shapes = match kind {
        "livingroom" => view! {
            <path d="M8 28v-8a4 4 0 0 1 4-4h24a4 4 0 0 1 4 4v8" /><path d="M6 28h36v8H6z" /><path d="M10 36v4M38 36v4" />
        }
        .into_any(),
        "bedroom" => view! {
            <path d="M6 38V12M6 30h36v8M42 38V26a6 6 0 0 0-6-6H20v10" /><circle cx="13" cy="24" r="4" />
        }
        .into_any(),
        "office" => view! {
            <rect x="10" y="8" width="28" height="18" rx="1" /><path d="M24 26v7M16 33h16M6 41h36" />
        }
        .into_any(),
        "kitchen" => view! {
            <path d="M10 22h28v14a4 4 0 0 1-4 4H14a4 4 0 0 1-4-4z" /><path d="M5 22h5M38 22h5M18 15c0-3 3-3 3-7M27 15c0-3 3-3 3-7" />
        }
        .into_any(),
        "hallway" => view! {
            <rect x="14" y="6" width="20" height="35" rx="1" /><circle cx="29" cy="24" r="1.6" /><path d="M8 42h32" />
        }
        .into_any(),
        "bathroom" => view! {
            <path d="M6 24h36v6a8 8 0 0 1-8 8H14a8 8 0 0 1-8-8z" /><path d="M12 24V13a4 4 0 0 1 8 0M10 42l2-4M38 42l-2-4" />
        }
        .into_any(),
        "dining" => view! {
            <path d="M8 20h32M13 20v19M35 20v19M18 20v-9h12v9" />
        }
        .into_any(),
        _ => view! {
            <path d="M6 23L24 8l18 15M12 20v20h24V20" /><path d="M20 40V29h8v11" />
        }
        .into_any(),
    };
    view! { <svg class="kind-icon" viewBox="0 0 48 48" aria-hidden="true">{shapes}</svg> }
}

/// The picture of a room: its cover when there is one (made on first sight from a frame of the scan or of a film),
/// else the drawing of its kind.
#[component]
pub fn RoomPicture(room_id: String, cover: Option<String>, kind: String, has_videos: bool) -> impl IntoView {
    let path = RwSignal::new(cover);
    if path.get_untracked().is_none() && has_videos {
        let room_id = room_id.clone();
        leptos::task::spawn_local(async move {
            if let Ok(Some(made)) = TauriBridge::call::<Option<String>>("room_cover", TauriBridge::map(&[("room", &room_id)])).await {
                path.set(Some(made));
            }
        });
    }
    view! {
        {move || match path.get() {
            Some(file) => view! { <img src=TauriBridge::file_url(&file) alt="" /> }.into_any(),
            None => view! { <span class="picture-empty">{kind_icon(&kind)}</span> }.into_any(),
        }}
    }
}

/// Picks films with the file dialog and copies them into the room. `done` gets how many were added.
pub fn add_videos_flow(room_id: String, done: impl FnOnce(Result<usize, String>) + 'static) {
    leptos::task::spawn_local(async move {
        let picked = match TauriBridge::call::<Vec<String>>("pick_videos", JsValue::UNDEFINED).await {
            Ok(list) if !list.is_empty() => list,
            Ok(_) => return,
            Err(message) => return done(Err(message)),
        };
        let list = js_sys::Array::new();
        for path in &picked {
            list.push(&path.into());
        }
        let args = TauriBridge::object(&[("room", room_id.as_str().into()), ("paths", list.into())]);
        done(TauriBridge::call::<usize>("add_videos", args).await);
    });
}

/// The question for a new room: its name and what kind of room it is.
#[component]
pub fn NewRoom(on_close: Callback<()>, on_created: Callback<String>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let kind = RwSignal::new("livingroom".to_string());
    let problem = RwSignal::new(None::<String>);
    let create = move |_| {
        let name = name.get_untracked();
        let kind = kind.get_untracked();
        leptos::task::spawn_local(async move {
            match TauriBridge::call::<String>("create_room", TauriBridge::map(&[("name", &name), ("kind", &kind)])).await {
                Ok(id) => on_created.run(id),
                Err(message) => problem.set(Some(message)),
            }
        });
    };
    view! {
        <div class="modal" role="dialog" aria-modal="true" aria-label="New room">
            <div class="plate modal-card">
                <h2 class="modal-title">"New room"</h2>
                <p class="modal-text">"It becomes a folder in your media library. Put its films in it, or add them afterwards."</p>
                <input
                    class="input room-name-input"
                    type="text"
                    placeholder="Name, for example Master bedroom"
                    spellcheck="false"
                    maxlength="40"
                    prop:value=move || name.get()
                    on:input=move |event| name.set(event_target_value(&event))
                />
                <ul class="kind-grid">
                    {KINDS.iter().map(|(id, label)| {
                        let id = *id;
                        view! {
                            <li>
                                <button class="kind-tile" class:is-selected=move || kind.get() == id on:click=move |_| kind.set(id.to_string())>
                                    {kind_icon(id)}
                                    <span>{*label}</span>
                                </button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                {move || problem.get().map(|text| view! { <p class="room-blocker">{text}</p> })}
                <div class="modal-actions">
                    <button class="btn btn-primary" disabled=move || name.get().trim().is_empty() on:click=create>"Make the room"</button>
                    <button class="btn btn-ghost" on:click=move |_| on_close.run(())>"Cancel"</button>
                </div>
            </div>
        </div>
    }
}
