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
use crate::format::{clock, count_of, when};
use crate::model::{RoomOverview, RoomView, ScanEntry, ScanListItem, View};

fn reveal(kind: &'static str) {
    leptos::task::spawn_local(async move {
        let _ = TauriBridge::fire_with("reveal_folder", TauriBridge::map(&[("kind", kind)])).await;
    });
}

/// The rooms of the media library: pick one on the left, see on the right what was rendered of it so far
/// and start, repeat or open any of those scans.
#[component]
pub fn RoomsScreen(flow: ScanFlow, #[prop(into)] blocker: Signal<Option<String>>) -> impl IntoView {
    let rooms = RwSignal::new(None::<Vec<RoomView>>);
    let loose = RwSignal::new(Vec::<ScanListItem>::new());
    let selected = RwSignal::new(None::<String>);
    let detail = RwSignal::new(None::<Result<RoomOverview, String>>);

    let refresh = move || {
        leptos::task::spawn_local(async move {
            let list = TauriBridge::call::<Vec<RoomView>>("list_rooms", JsValue::UNDEFINED).await.unwrap_or_default();
            if selected.get_untracked().is_none() {
                selected.set(list.first().map(|entry| entry.room.id.clone()));
            }
            rooms.set(Some(list));
            loose.set(TauriBridge::call::<Vec<ScanListItem>>("list_scans", JsValue::UNDEFINED).await.unwrap_or_default());
        });
    };
    refresh();

    Effect::new(move |_| {
        let Some(id) = selected.get() else {
            detail.set(None);
            return;
        };
        leptos::task::spawn_local(async move {
            let loaded = TauriBridge::call::<RoomOverview>("room_overview", TauriBridge::map(&[("room", &id)])).await;
            if selected.get_untracked().as_deref() == Some(id.as_str()) {
                detail.set(Some(loaded));
            }
        });
    });

    let actions = move || {
        view! {
            <button class="btn btn-ghost" on:click=move |_| refresh()>"Refresh"</button>
            <button class="btn btn-ghost" on:click=move |_| reveal("rooms")>"Open rooms folder"</button>
        }
    };

    view! {
        <Screen title="Rooms" subtitle="Every folder in your media library is one room." flow=flow actions=actions>
            {move || match rooms.get() {
                None => view! { <p class="screen-note">"Looking for rooms…"</p> }.into_any(),
                Some(list) if list.is_empty() => view! {
                    <div class="plate empty">
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
                    <div class="rooms">
                        <aside class="rail">
                            <ul class="rail-list">
                                {list.into_iter().map(|entry| rail_item(entry, selected)).collect_view()}
                            </ul>
                            <Show when=move || !loose.get().is_empty()>
                                <h2 class="rail-title">"Other scans"</h2>
                                <ul class="loose">
                                    <For
                                        each=move || loose.get()
                                        key=|scan| scan.name.clone()
                                        children=move |scan| {
                                            let result = scan.result.clone();
                                            let kind = if scan.result.kind == "splats" { "Splat" } else { "Points" };
                                            view! {
                                                <li>
                                                    <button class="loose-row" on:click=move |_| flow.show(result.clone())>
                                                        <span>{scan.room.clone()}</span>
                                                        <span class="loose-meta">{format!("{kind}, {}", when(scan.modified_ms))}</span>
                                                    </button>
                                                </li>
                                            }
                                        }
                                    />
                                </ul>
                            </Show>
                        </aside>
                        <section class="plate room-detail">
                            {move || match detail.get() {
                                None => view! { <p class="screen-note">"Pick a room."</p> }.into_any(),
                                Some(Err(message)) => view! { <p class="screen-note">{message}</p> }.into_any(),
                                Some(Ok(data)) => room_detail(data, flow, blocker).into_any(),
                            }}
                        </section>
                    </div>
                }.into_any(),
            }}
        </Screen>
    }
}

fn rail_item(entry: RoomView, selected: RwSignal<Option<String>>) -> impl IntoView {
    let id = entry.room.id.clone();
    let id_class = id.clone();
    let id_click = id.clone();
    let id_hover = id;
    let note = match (&entry.latest_scan, entry.room.videos.is_empty()) {
        (_, true) => "No videos".to_string(),
        (Some(_), _) => count_of(entry.room.videos.len(), "video", "videos"),
        (None, _) => format!("{}, not scanned", count_of(entry.room.videos.len(), "video", "videos")),
    };
    let scanned = entry.latest_scan.is_some();
    view! {
        <li>
            <button
                class="slab slab-sm"
                class:is-selected=move || selected.get().as_deref() == Some(id_class.as_str())
                on:click=move |_| selected.set(Some(id_click.clone()))
                on:mouseenter=move |_| selected.set(Some(id_hover.clone()))
            >
                <span>
                    <span class="slab-label">{entry.room.name.clone()}</span>
                    <span class="slab-detail">
                        <i class="state-dot" class:is-done=scanned></i>
                        {note}
                    </span>
                </span>
            </button>
        </li>
    }
}

fn room_detail(data: RoomOverview, flow: ScanFlow, blocker: Signal<Option<String>>) -> impl IntoView {
    let has_videos = !data.room.videos.is_empty();
    let scanned = data.scan.is_some();
    let id = data.room.id.clone();
    let name = data.room.name.clone();
    let open_id = id.clone();
    let scan_id = id.clone();
    let scan_name = name.clone();
    let photo = data.videos.iter().find_map(|video| video.thumb.clone()).map(|path| TauriBridge::file_url(&path));
    let status = match &data.scan {
        Some(scan) => format!(
            "{} from {}",
            if scan.result.kind == "splats" { "Splat scene" } else { "Point cloud" },
            when(scan.modified_ms)
        ),
        None => "Not scanned yet".to_string(),
    };
    let covered = data.plan.as_ref().map(|plan| plan.covered_pct).filter(|pct| *pct > 0);
    let can_scan = move || has_videos && blocker.get().is_none();
    let tries = data.history.len();
    let rows = data
        .history
        .clone()
        .into_iter()
        .enumerate()
        .map(|(rank, entry)| history_row(entry, rank == 0, id.clone(), name.clone(), flow, blocker, has_videos))
        .collect_view();

    view! {
        <div class="detail-top">
            <div class="polaroid">
                {match photo {
                    Some(src) => view! { <img src=src alt="" /> }.into_any(),
                    None => view! { <span>"No picture yet"</span> }.into_any(),
                }}
            </div>
            <div class="detail-main">
                <h2 class="detail-name">{data.room.name.clone()}</h2>
                <p class="detail-status" class:is-done=scanned>{status}</p>
                <dl class="stats">
                    <div><dt>"Videos"</dt><dd>{data.room.videos.len()}</dd></div>
                    <div><dt>"Frames in scene"</dt><dd>{if scanned { data.frames.to_string() } else { "-".to_string() }}</dd></div>
                    <div><dt>"Well covered"</dt><dd>{covered.map(|pct| format!("{pct}%")).unwrap_or_else(|| "-".into())}</dd></div>
                    <div><dt>"Scans"</dt><dd>{tries}</dd></div>
                </dl>
                <div class="detail-actions">
                    {scanned.then(|| view! {
                        <button class="btn btn-primary" on:click=move |_| flow.go(View::Room(open_id.clone()))>"Open room"</button>
                    })}
                    <button
                        class=if scanned { "btn btn-ghost" } else { "btn btn-primary" }
                        disabled=move || !can_scan()
                        on:click=move |_| flow.scan_room(scan_id.clone(), scan_name.clone())
                    >
                        {if scanned { "Scan again" } else { "Reconstruct" }}
                    </button>
                </div>
                {move || has_videos.then(|| blocker.get()).flatten().map(|reason| view! { <p class="room-blocker">{reason}</p> })}
                {(!has_videos).then(|| view! { <p class="room-blocker">"This room has no videos yet. Put them in its folder."</p> })}
            </div>
        </div>

        <h3 class="detail-title">"Rendered so far"</h3>
        {if tries == 0 {
            view! { <p class="screen-note">"Nothing yet. The first scan shows up here."</p> }.into_any()
        } else {
            view! { <ul class="history">{rows}</ul> }.into_any()
        }}
    }
}

fn history_row(
    entry: ScanEntry,
    newest: bool,
    room_id: String,
    room_name: String,
    flow: ScanFlow,
    blocker: Signal<Option<String>>,
    has_videos: bool,
) -> impl IntoView {
    let stopped = entry.state == "unfinished";
    let label = match entry.state.as_str() {
        "splats" => "Splat scene",
        "points" => "Point cloud only",
        _ => "Stopped before the end",
    };
    let facts = if stopped {
        if entry.note.is_empty() { "No result was saved".to_string() } else { entry.note.clone() }
    } else {
        format!("{} frames, took {}", entry.frames, clock(entry.seconds))
    };
    let retry_label = if stopped { "Retry" } else { "Render again" };
    view! {
        <li class="hist-row" class:is-stopped=stopped>
            <span class="hist-state" class:is-splat=entry.state == "splats">{label}</span>
            <span class="hist-main">
                <span class="hist-when">{when(entry.modified_ms)}{newest.then(|| view! { <em class="hist-new">"latest"</em> })}</span>
                <span class="hist-facts">{facts}</span>
            </span>
            <span class="hist-actions">
                {entry.scan.clone().map(|scan| view! {
                    <button class="btn btn-ghost" on:click=move |_| flow.show(scan.clone())>"Open"</button>
                })}
                <button
                    class="btn btn-ghost"
                    disabled=move || !has_videos || blocker.get().is_some()
                    on:click=move |_| flow.scan_room(room_id.clone(), room_name.clone())
                >
                    {retry_label}
                </button>
            </span>
        </li>
    }
}
