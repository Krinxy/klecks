// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-11-01
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::room_picture::{add_videos_flow, kind_icon, NewRoom, RoomPicture};
use crate::components::screen::Screen;
use crate::flow::ScanFlow;
use crate::format::Text;
use crate::model::{Apartment, RoomOverview, RoomView, ScanEntry, ScanListItem, View};

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
    let apartment = RwSignal::new(None::<Apartment>);
    let adding = RwSignal::new(false);

    let refresh = move || {
        leptos::task::spawn_local(async move {
            let list = TauriBridge::call::<Vec<RoomView>>("list_rooms", JsValue::UNDEFINED).await.unwrap_or_default();
            if selected.get_untracked().is_none() {
                selected.set(list.first().map(|entry| entry.room.id.clone()));
            }
            rooms.set(Some(list));
            loose.set(TauriBridge::call::<Vec<ScanListItem>>("list_scans", JsValue::UNDEFINED).await.unwrap_or_default());
            apartment.set(TauriBridge::call::<Apartment>("apartment", JsValue::UNDEFINED).await.ok());
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
                            "Make a room for each space you film: living room, bedroom, office, kitchen, hallway. "
                            "All films of a room become one scene."
                        </p>
                        <button class="btn btn-primary" on:click=move |_| adding.set(true)>"Make the first room"</button>
                    </div>
                }.into_any(),
                Some(list) => view! {
                    <div class="rooms">
                        <aside class="rail">
                            <ul class="rail-list">
                                {list.into_iter().map(|entry| rail_item(entry, selected)).collect_view()}
                                <li>
                                    <button class="slab slab-sm slab-add" on:click=move |_| adding.set(true)>
                                        <span><span class="slab-label">"+ New room"</span></span>
                                    </button>
                                </li>
                            </ul>
                            {move || apartment.get().filter(|map| map.rooms.len() > 1).map(|map| apartment_map(map, selected))}
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
                                                        <span class="loose-meta">{format!("{kind}, {}", Text::when(scan.modified_ms))}</span>
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
            {move || adding.get().then(|| view! {
                <NewRoom
                    on_close=Callback::new(move |_| adding.set(false))
                    on_created=Callback::new(move |id: String| {
                        adding.set(false);
                        selected.set(Some(id));
                        refresh();
                    })
                />
            })}
        </Screen>
    }
}

fn rail_item(entry: RoomView, selected: RwSignal<Option<String>>) -> impl IntoView {
    let id = entry.room.id.clone();
    let id_class = id.clone();
    let id_click = id.clone();
    let id_hover = id;
    let media = crate::format::Text::media_count(entry.room.videos.len(), entry.room.photos.len());
    let note = match (&entry.latest_scan, entry.room.videos.is_empty() && entry.room.photos.is_empty()) {
        (_, true) => "No films or photos".to_string(),
        (Some(_), _) => media,
        (None, _) => format!("{media}, not scanned"),
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
                    <span class="slab-label"><span class="slab-icon">{kind_icon(&entry.room.kind)}</span>{entry.room.name.clone()}</span>
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
    let has_videos = !data.room.videos.is_empty() || !data.room.photos.is_empty();
    let scanned = data.scan.is_some();
    let id = data.room.id.clone();
    let name = data.room.name.clone();
    let open_id = id.clone();
    let scan_id = id.clone();
    let scan_name = name.clone();
    let cover = data.cover.clone();
    let kind = data.room.kind.clone();
    let picture_room = data.room.id.clone();
    let add_room = data.room.id.clone();
    let reload = flow;
    let status = match &data.scan {
        Some(scan) => format!(
            "{} from {}",
            if scan.result.kind == "splats" { "Splat scene" } else { "Point cloud" },
            Text::when(scan.modified_ms)
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
                <RoomPicture room_id=picture_room cover=cover kind=kind has_videos=has_videos />
            </div>
            <div class="detail-main">
                <h2 class="detail-name">{data.room.name.clone()}</h2>
                <p class="detail-status" class:is-done=scanned>{status}</p>
                <dl class="stats">
                    <div><dt>"Films and photos"</dt><dd>{data.room.videos.len() + data.room.photos.len()}</dd></div>
                    <div><dt>"Frames in scene"</dt><dd>{if scanned { data.frames.to_string() } else { "-".to_string() }}</dd></div>
                    <div><dt>"Well covered"</dt><dd>{covered.map(|pct| format!("{pct}%")).unwrap_or_else(|| "-".into())}</dd></div>
                    <div><dt>"Scans"</dt><dd>{tries}</dd></div>
                </dl>
                <div class="detail-actions">
                    {scanned.then(|| view! {
                        <button class="btn btn-primary" on:click=move |_| flow.go(View::Room(open_id.clone()))>"Open room"</button>
                    })}
                    <button class="btn btn-ghost" on:click=move |_| {
                        let room = add_room.clone();
                        let again = room.clone();
                        add_videos_flow(room, move |result| {
                            if result.is_ok() {
                                reload.go(View::Rooms);
                                reload.go(View::Room(again));
                            }
                        });
                    }>"Add films or photos"</button>
                    <button
                        class=if scanned { "btn btn-ghost" } else { "btn btn-primary" }
                        disabled=move || !can_scan()
                        on:click=move |_| flow.ask_scan(scan_id.clone(), scan_name.clone())
                    >
                        {if scanned { "Scan again" } else { "Reconstruct" }}
                    </button>
                </div>
                {move || has_videos.then(|| blocker.get()).flatten().map(|reason| view! { <p class="room-blocker">{reason}</p> })}
                {(!has_videos).then(|| view! { <p class="room-blocker">"This room has no films or photos yet. Add some."</p> })}
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
        format!("{} frames, took {}", entry.frames, Text::clock(entry.seconds))
    };
    let retry_label = if stopped { "Retry" } else { "Render again" };
    view! {
        <li class="hist-row" class:is-stopped=stopped>
            <span class="hist-state" class:is-splat=entry.state == "splats">{label}</span>
            <span class="hist-main">
                <span class="hist-when">{Text::when(entry.modified_ms)}{newest.then(|| view! { <em class="hist-new">"latest"</em> })}</span>
                <span class="hist-facts">{facts}</span>
            </span>
            <span class="hist-actions">
                {entry.scan.clone().map(|scan| view! {
                    <button class="btn btn-ghost" on:click=move |_| flow.show(scan.clone())>"Open"</button>
                })}
                <button
                    class="btn btn-ghost"
                    disabled=move || !has_videos || blocker.get().is_some()
                    on:click=move |_| flow.ask_scan(room_id.clone(), room_name.clone())
                >
                    {retry_label}
                </button>
            </span>
        </li>
    }
}

/// The apartment as a small map: a room is a node, a confirmed door that leads to another room is a line.
/// The rooms are not placed to scale yet; that needs the doors to line the plans up.
fn apartment_map(map: Apartment, selected: RwSignal<Option<String>>) -> impl IntoView {
    const CENTER: (f64, f64) = (120.0, 80.0);
    const RADIUS: f64 = 56.0;
    let count = map.rooms.len() as f64;
    let spot = |index: usize| {
        let angle = -std::f64::consts::FRAC_PI_2 + index as f64 * std::f64::consts::TAU / count;
        (CENTER.0 + RADIUS * 1.5 * angle.cos(), CENTER.1 + RADIUS * angle.sin())
    };
    let position = |id: &str| map.rooms.iter().position(|room| room.id == id).map(spot);
    let lines = map
        .links
        .iter()
        .filter_map(|(a, b)| Some((position(a)?, position(b)?)))
        .map(|((x1, y1), (x2, y2))| view! { <line x1=x1 y1=y1 x2=x2 y2=y2 class="map-link" /> })
        .collect_view();
    let linked = !map.links.is_empty();
    let nodes = map
        .rooms
        .iter()
        .enumerate()
        .map(|(index, room)| {
            let (x, y) = spot(index);
            let width = 18.0 + room.name.chars().count() as f64 * 6.6;
            let id = room.id.clone();
            let id_click = room.id.clone();
            view! {
                <g
                    class="map-node"
                    class:is-selected=move || selected.get().as_deref() == Some(id.as_str())
                    transform=format!("translate({x} {y})")
                    on:click=move |_| selected.set(Some(id_click.clone()))
                >
                    <polygon points=format!("{},-12 {},-12 {},12 {},12", -width / 2.0 + 6.0, width / 2.0, width / 2.0 - 6.0, -width / 2.0) />
                    <text y="4" text-anchor="middle">{room.name.clone()}</text>
                </g>
            }
        })
        .collect_view();
    view! {
        <h2 class="rail-title">"Apartment"</h2>
        <svg class="map" viewBox="0 0 240 160" role="img" aria-label="Which rooms the doors connect">
            {lines}
            {nodes}
        </svg>
        {(!linked).then(|| view! { <p class="map-note">"Confirm a door in a room and say where it leads to connect the rooms."</p> })}
    }
}
