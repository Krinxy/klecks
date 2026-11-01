// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-11-01
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::flow::ScanFlow;
use crate::format::Text;
use crate::model::{RoomOverview, SettingsView};

/// One film of the room and whether it is ticked for this scan.
#[derive(Clone)]
struct Film {
    name: String,
    size_mb: f64,
    photo: bool,
    was_scanned: bool,
    ticked: RwSignal<bool>,
}

/// The question before a scan: which films go into the room, and what that will cost. Nothing starts before
/// the person confirms; the ticks are remembered for the next time.
#[component]
pub fn ScanPlan(room_id: String, label: String, flow: ScanFlow, #[prop(into)] blocker: Signal<Option<String>>) -> impl IntoView {
    let films = RwSignal::new(None::<Result<Vec<Film>, String>>);
    let quality = RwSignal::new(None::<(String, u32)>);
    {
        let room_id = room_id.clone();
        leptos::task::spawn_local(async move {
            match TauriBridge::call::<RoomOverview>("room_overview", TauriBridge::map(&[("room", &room_id)])).await {
                Ok(data) => {
                    let list = data
                        .room
                        .videos
                        .iter()
                        .map(|video| (video, false))
                        .chain(data.room.photos.iter().map(|photo| (photo, true)))
                        .map(|(video, photo)| {
                            let known = data.videos.iter().find(|share| share.name == video.name);
                            Film {
                                name: video.name.clone(),
                                size_mb: video.size_bytes as f64 / (1024.0 * 1024.0),
                                photo,
                                was_scanned: known.is_some_and(|share| share.scanned),
                                ticked: RwSignal::new(known.is_none_or(|share| share.included)),
                            }
                        })
                        .collect();
                    films.set(Some(Ok(list)));
                }
                Err(message) => films.set(Some(Err(message))),
            }
            if let Ok(settings) = TauriBridge::call::<SettingsView>("get_settings", JsValue::UNDEFINED).await {
                let minutes = settings.presets.iter().find(|preset| preset.id == settings.preset).map_or(0, |preset| preset.minutes);
                quality.set(Some((Text::capitalised(&settings.preset), minutes)));
            }
        });
    }

    let chosen = move || {
        films
            .get()
            .and_then(Result::ok)
            .map(|list| list.into_iter().filter(|film| film.ticked.get()).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let start_room = room_id.clone();
    let start_label = label.clone();
    let start = move |_| {
        let names: Vec<String> = chosen().into_iter().map(|film| film.name).collect();
        flow.cancel_ask();
        flow.scan_room_with(start_room.clone(), start_label.clone(), names);
    };
    let set_all = move |on: bool| {
        if let Some(Ok(list)) = films.get_untracked() {
            for film in list {
                film.ticked.set(on);
            }
        }
    };

    view! {
        <div class="modal" role="dialog" aria-modal="true" aria-label="Choose the films for the scan">
            <div class="plate modal-card">
                <h2 class="modal-title">{format!("Reconstruct {label}")}</h2>
                <p class="modal-text">"Tick the films and photos that should go into the room. What you leave out stays in the folder and is not used. Sharp photos from many spots make the room crisper than films."</p>
                {move || match films.get() {
                    None => view! { <p class="screen-note">"Looking at the films…"</p> }.into_any(),
                    Some(Err(message)) => view! { <p class="room-blocker">{message}</p> }.into_any(),
                    Some(Ok(list)) => view! {
                        <div class="film-tools">
                            <button class="btn btn-ghost" on:click=move |_| set_all(true)>"All"</button>
                            <button class="btn btn-ghost" on:click=move |_| set_all(false)>"None"</button>
                        </div>
                        <ul class="film-list">
                            {list.into_iter().map(|film| {
                                let ticked = film.ticked;
                                view! {
                                    <li>
                                        <label class="film" class:is-off=move || !ticked.get()>
                                            <input
                                                type="checkbox"
                                                prop:checked=move || ticked.get()
                                                on:change=move |event| ticked.set(event_target_checked(&event))
                                            />
                                            <span class="film-name">{film.name.clone()}</span>
                                            <span class="film-meta">
                                                {format!("{}{:.0} MB", if film.photo { "Photo, " } else { "" }, film.size_mb)}
                                                {if film.was_scanned { "" } else { ", new" }}
                                            </span>
                                        </label>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                }}
                <p class="modal-summary">
                    {move || {
                        let picked = chosen();
                        let total: f64 = picked.iter().map(|film| film.size_mb).sum();
                        let all = films.get().and_then(Result::ok).map_or(0, |list| list.len());
                        let time = quality
                            .get()
                            .map(|(name, minutes)| format!(" Quality {name}: about {minutes} minutes of training, plus a few minutes to reconstruct."))
                            .unwrap_or_default();
                        format!("{} of {} ticked, {total:.0} MB.{time}", picked.len(), Text::count_of(all, "file", "files"))
                    }}
                </p>
                {move || blocker.get().map(|reason| view! { <p class="room-blocker">{reason}</p> })}
                <div class="modal-actions">
                    <button class="btn btn-primary" disabled=move || chosen().is_empty() || blocker.get().is_some() on:click=start>"Start the scan"</button>
                    <button class="btn btn-ghost" on:click=move |_| flow.cancel_ask()>"Cancel"</button>
                </div>
            </div>
        </div>
    }
}
