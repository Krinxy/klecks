// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;

use crate::bridge::TauriBridge;
use crate::components::placement::{furniture_panel, plan_layer, point_in_plan, Placement, Tool};
use crate::components::scene_viewer::SceneViewer;
use crate::components::stage::Stage;
use crate::flow::ScanFlow;
use crate::format::{count_of, when};
use crate::model::{DoorView, Plan, RoomOverview, RoomRef, Thin, VideoScore, VideoShare, View};
use wasm_bindgen::JsValue;

/// How many of the six density steps a cell shows (0 = empty).
const LEVELS: u32 = 5;
/// A heading tick every this many cameras, so the path does not turn into a hedgehog.
const HEADING_EVERY: usize = 4;
const HEADING_LENGTH: f64 = 1.8;
/// Keep in step with THIN_FRAMES in lab/room_plan.py.
const THIN_FRAMES: u32 = 4;

#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Plan,
    Camera,
}

/// What the right-hand panel shows.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Media,
    Doors,
    Furniture,
}

/// One room: the plan of what the films saw, the free camera, and what each film gave the scene.
#[component]
pub fn RoomScreen(id: String, flow: ScanFlow, #[prop(into)] blocker: Signal<Option<String>>) -> impl IntoView {
    let overview = RwSignal::new(None::<Result<RoomOverview, String>>);
    let pane = RwSignal::new(Pane::Plan);
    {
        let id = id.clone();
        leptos::task::spawn_local(async move {
            overview.set(Some(
                TauriBridge::call::<RoomOverview>("room_overview", TauriBridge::map(&[("room", &id)])).await,
            ));
        });
    }

    view! {
        <Stage mood="">
            <button class="back" title="Back to the rooms (Esc)" on:click=move |_| flow.go(View::Rooms)>
                <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M7.5 2L3.5 6l4 4" /></svg>
                "Rooms"
            </button>
            {move || match overview.get() {
                None => view! { <p class="screen-note room-wait">"Opening the room…"</p> }.into_any(),
                Some(Err(message)) => view! { <p class="screen-note room-wait">{message}</p> }.into_any(),
                Some(Ok(data)) => room_body(data, flow, blocker, pane).into_any(),
            }}
        </Stage>
    }
}

fn room_body(data: RoomOverview, flow: ScanFlow, blocker: Signal<Option<String>>, pane: RwSignal<Pane>) -> impl IntoView {
    let has_videos = !data.room.videos.is_empty();
    let scanned = data.scan.is_some();
    let room_id = data.room.id.clone();
    let room_name = data.room.name.clone();
    let subtitle = match &data.scan {
        Some(scan) => format!(
            "{}, {} in the scene, {} from {}",
            count_of(data.room.videos.len(), "video", "videos"),
            count_of(data.frames as usize, "frame", "frames"),
            if scan.result.kind == "splats" { "splat scene" } else { "point cloud" },
            when(scan.modified_ms)
        ),
        None => format!("{}, not scanned yet", count_of(data.room.videos.len(), "video", "videos")),
    };
    let newer = data.videos.iter().filter(|video| !video.scanned).count();
    let scan_result = data.scan.as_ref().map(|scan| scan.result.clone());
    let plan = data.plan.clone();
    let plan_missing = plan.is_none();
    let mut videos = data.videos.clone();
    let footprint = data.plan.as_ref().map(|plan| plan.footprint).unwrap_or(0);
    let scores: Vec<VideoScore> = data.plan.as_ref().map(|plan| plan.videos.clone()).unwrap_or_default();
    // The film that shows the most of the room first; without a plan, the one with the most frames.
    videos.sort_by_key(|video| {
        let seen = video.index.and_then(|index| scores.get(index)).map_or(0, |score| score.seen);
        std::cmp::Reverse((seen, video.frames))
    });
    let next_best = data.plan.as_ref().and_then(|plan| plan.thin.as_ref()).map(thin_hint);
    let can_scan = move || has_videos && blocker.get().is_none();
    let scan_room = room_id.clone();
    let scan_label = room_name.clone();
    let doors = RwSignal::new(data.doors.clone());
    let plan_size = data.plan.as_ref().map(|plan| (plan.width as f64, plan.height as f64)).unwrap_or((0.0, 0.0));
    let placement = Placement::new(room_id.clone(), data.layout.clone(), data.plan.as_ref().and_then(|plan| plan.transform.clone()), plan_size);
    let side = RwSignal::new(Side::Media);
    let other_rooms = data.other_rooms.clone();
    let plan_room = room_id.clone();
    let list_room = room_id.clone();

    view! {
        <div class="room">
            <header class="room-head">
                <div>
                    <h1 class="room-title">{room_name}</h1>
                    <p class="room-sub">{subtitle}</p>
                </div>
                <div class="room-head-actions">
                    <button class="btn btn-ghost" on:click=move |_| {
                        leptos::task::spawn_local(async {
                            let _ = TauriBridge::fire_with("reveal_folder", TauriBridge::map(&[("kind", "rooms")])).await;
                        });
                    }>"Add videos"</button>
                    <button
                        class="btn btn-primary"
                        disabled=move || !can_scan()
                        on:click=move |_| flow.ask_scan(scan_room.clone(), scan_label.clone())
                    >
                        {if scanned { "Scan again" } else { "Reconstruct" }}
                    </button>
                </div>
            </header>
            {move || (has_videos).then(|| blocker.get()).flatten().map(|reason| view! { <p class="room-blocker room-blocker-wide">{reason}</p> })}

            <div class="room-grid">
                <section class="panel panel-view">
                    <div class="tabs" role="tablist">
                        <button class="tab" class:is-selected=move || pane.get() == Pane::Plan role="tab" on:click=move |_| pane.set(Pane::Plan)>"Floor plan"</button>
                        <button class="tab" class:is-selected=move || pane.get() == Pane::Camera role="tab"
                            disabled=scan_result.is_none() on:click=move |_| pane.set(Pane::Camera)>"Free camera"</button>
                        <button class="tab tab-add" class:is-selected=move || placement.tool.get() == Tool::Door class:is-hidden=move || pane.get() != Pane::Plan || plan_missing
                            title="Click the plan to place a door yourself"
                            on:click=move |_| placement.tool.update(|tool| *tool = if *tool == Tool::Door { Tool::None } else { Tool::Door })>
                            {move || if placement.tool.get() == Tool::Door { "Click the plan..." } else { "Add door" }}
                        </button>
                    </div>
                    <div class="panel-stage">
                        {move || match (pane.get(), scan_result.clone()) {
                            (Pane::Camera, Some(scan)) => {
                                placement.sync_viewer();
                                view! { <SceneViewer scan=scan flow=flow embedded=true /> }.into_any()
                            }
                            _ => plan_view(plan.clone(), scanned, doors, placement, plan_room.clone()).into_any(),
                        }}
                    </div>
                </section>

                <section class="panel panel-media">
                    <div class="tabs side-tabs" role="tablist">
                        <button class="tab" class:is-selected=move || side.get() == Side::Media role="tab" on:click=move |_| side.set(Side::Media)>"Media"</button>
                        <button class="tab" class:is-selected=move || side.get() == Side::Doors role="tab" on:click=move |_| side.set(Side::Doors)>
                            {move || {
                                let open = doors.get().iter().filter(|door| door.state == "detected").count();
                                if open > 0 { format!("Doors ({open} to check)") } else { "Doors".to_string() }
                            }}
                        </button>
                        <button class="tab" class:is-selected=move || side.get() == Side::Furniture role="tab" on:click=move |_| side.set(Side::Furniture)>"Furniture"</button>
                    </div>
                    <div class="side-body">
                        {move || match side.get() {
                            Side::Media => view! {
                                <h2 class="panel-title">"Media that built this room"</h2>
                                <p class="panel-sub">"The bar is how much of the room a film saw. The number is what only that film saw. Both are estimates from where the camera stood."</p>
                                <ul class="media-list">
                                    {videos.clone().into_iter().enumerate().map(|(rank, video)| {
                                        let score = video.index.and_then(|index| scores.get(index)).cloned();
                                        media_row(video, rank, score, footprint)
                                    }).collect_view()}
                                </ul>
                                {next_best.clone().map(|text| view! {
                                    <div class="hint-box">
                                        <h3 class="hint-title">"Next best film"</h3>
                                        <p>{text}</p>
                                    </div>
                                })}
                                {(newer > 0).then(|| view! {
                                    <p class="media-note">{format!("{} not in the scene yet. Scan again to add {}.",
                                        count_of(newer, "new video is", "new videos are"), if newer == 1 { "it" } else { "them" })}</p>
                                })}
                                {(!has_videos).then(|| view! { <p class="media-note">"This room has no videos. Put them in its folder, then reconstruct."</p> })}
                            }.into_any(),
                            Side::Doors => door_list(doors, other_rooms.clone(), list_room.clone()).into_any(),
                            Side::Furniture => furniture_panel(placement).into_any(),
                        }}
                    </div>
                </section>
            </div>
        </div>
    }
}

/// "The upper left part of the plan is seen by few frames (12% of it). A short film from there adds the most."
fn thin_hint(thin: &Thin) -> String {
    let across = if thin.x < 0.34 { "left" } else if thin.x > 0.66 { "right" } else { "middle" };
    let down = if thin.y < 0.34 { "upper" } else if thin.y > 0.66 { "lower" } else { "central" };
    let place = match (down, across) {
        ("central", "middle") => "middle of the plan".to_string(),
        ("central", side) => format!("{side} side of the plan"),
        (row, "middle") => format!("{row} part of the plan"),
        (row, side) => format!("{row} {side} part of the plan"),
    };
    format!(
        "The {place} is seen by few frames ({}% of the room). A short, slow film that looks at it from a different spot adds the most.",
        thin.share_pct
    )
}

fn media_row(video: VideoShare, rank: usize, score: Option<VideoScore>, footprint: u32) -> impl IntoView {
    let thumb = video.thumb.as_deref().map(TauriBridge::file_url);
    let percent = |cells: u32| (cells * 100).checked_div(footprint).unwrap_or(0) as u8;
    let sees = score.as_ref().map(|score| percent(score.seen));
    let only = score.as_ref().map(|score| percent(score.unique));
    let bar = sees.unwrap_or(video.share_pct);
    let detail = if !video.scanned {
        format!("New, {:.0} MB", video.size_mb)
    } else if let Some(sees) = sees {
        format!("{}, {:.0} MB, sees {sees}% of the room", count_of(video.frames as usize, "frame", "frames"), video.size_mb)
    } else {
        format!("{}, {:.0} MB", count_of(video.frames as usize, "frame", "frames"), video.size_mb)
    };
    let number = if !video.scanned {
        "new".to_string()
    } else if let Some(only) = only {
        format!("+{only}")
    } else {
        format!("{}%", video.share_pct)
    };
    view! {
        <li class="media-row" class:is-new=!video.scanned>
            <span class=format!("media-thumb walk-{}", video.index.unwrap_or(0) % 6)>{thumb.map(|src| view! { <img src=src alt="" /> })}</span>
            <span class="media-main">
                <span class="media-name">{video.name.clone()}</span>
                <span class="media-detail">{detail}</span>
                <span class="media-bar"><span style=format!("width: {}%", bar.max(if video.scanned { 2 } else { 0 }))></span></span>
            </span>
            <span class="media-share" class:is-top=rank == 0 && video.scanned title="Share of the room only this film saw">
                {number}
            </span>
        </li>
    }
}

/// The footprint of the room seen from above, with the way the camera walked over it.
fn plan_view(plan: Option<Plan>, scanned: bool, doors: RwSignal<Vec<DoorView>>, placement: Placement, room: String) -> impl IntoView {
    let Some(plan) = plan else {
        let text = if scanned {
            "This scan has no plan yet. Scan the room again and it will be drawn."
        } else {
            "The plan appears after the first scan."
        };
        return view! { <div class="plan-empty">{text}</div> }.into_any();
    };
    let (width, height) = (plan.width as f64, plan.height as f64);
    let svg = NodeRef::<leptos::svg::Svg>::new();
    let cells = plan_cells(&plan);
    let hatch = plan_thin(&plan);
    let covered = plan.covered_pct;
    let videos = plan.cameras.iter().map(|camera| camera[2] as usize).max().map_or(0, |last| last + 1);
    let paths = (0..videos)
        .map(|video| {
            let points: Vec<&[f64; 5]> = plan.cameras.iter().filter(|camera| camera[2] as usize == video).collect();
            let line = points.iter().map(|camera| format!("{:.2},{:.2}", camera[0], camera[1])).collect::<Vec<_>>().join(" ");
            let ticks = points
                .iter()
                .step_by(HEADING_EVERY)
                .map(|camera| {
                    format!(
                        "M{:.2} {:.2}l{:.2} {:.2}",
                        camera[0], camera[1], camera[3] * HEADING_LENGTH, camera[4] * HEADING_LENGTH
                    )
                })
                .collect::<String>();
            let start = points.first().map(|camera| (camera[0], camera[1]));
            view! {
                <g class=format!("walk walk-{}", video % 6)>
                    <polyline points=line />
                    <path d=ticks />
                    {start.map(|(x, y)| view! { <circle cx=x cy=y r="1.1" /> })}
                </g>
            }
        })
        .collect_view();

    view! {
        <div class="plan">
            <svg
                node_ref=svg
                class:is-adding=move || placement.tool.get() != Tool::None
                viewBox=format!("-2 -2 {} {}", width + 4.0, height + 4.0)
                role="img"
                aria-label="Floor plan seen from above"
                on:click=move |event| {
                    if placement.tool.get_untracked() != Tool::Door {
                        return;
                    }
                    if let Some(at) = pointer_at(svg, event.client_x(), event.client_y(), width + 4.0, height + 4.0) {
                        placement.tool.set(Tool::None);
                        save_door(room.clone(), at.0, at.1, "manual", None, doors);
                    }
                }
                on:pointerdown=move |event| {
                    if let Some(at) = pointer_at(svg, event.client_x(), event.client_y(), width + 4.0, height + 4.0) {
                        placement.press(at);
                    }
                }
                on:pointermove=move |event| {
                    if let Some(at) = pointer_at(svg, event.client_x(), event.client_y(), width + 4.0, height + 4.0) {
                        placement.moved(at);
                    }
                }
                on:pointerup=move |_| placement.released()
                on:pointerleave=move |_| placement.released()
            >
                <defs>
                    <pattern id="thin-hatch" width="1.6" height="1.6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                        <line x1="0" y1="0" x2="0" y2="1.6" class="hatch-line" />
                    </pattern>
                </defs>
                <rect x="-2" y="-2" width=width + 4.0 height=height + 4.0 class="plan-ground" />
                {cells}
                {hatch}
                {paths}
                {plan_layer(placement, svg, (width + 4.0, height + 4.0))}
                {move || door_markers(doors.get())}
            </svg>
            <p class="plan-note">
                {format!("{covered}% is seen by enough frames. Hatched parts are seen by few. ")}
                "Lines are the way you walked, one colour per film. Numbered circles are doors: dashed ones are guesses, confirm them on the right."
            </p>
        </div>
    }
    .into_any()
}

/// Footprint cells that few frames saw, as hatched runs on top of the plan.
fn plan_thin(plan: &Plan) -> impl IntoView {
    let thin = |y: usize, x: usize| {
        let inside = plan.cells.get(y).and_then(|row| row.get(x)).is_some_and(|value| *value > 0);
        inside && plan.seen.get(y).and_then(|row| row.get(x)).is_some_and(|frames| *frames < THIN_FRAMES)
    };
    let mut runs = Vec::new();
    for y in 0..plan.cells.len() {
        let width = plan.cells[y].len();
        let mut x = 0;
        while x < width {
            if thin(y, x) {
                let start = x;
                while x < width && thin(y, x) {
                    x += 1;
                }
                runs.push((start, y, x - start));
            } else {
                x += 1;
            }
        }
    }
    runs.into_iter()
        .map(|(x, y, run)| view! { <rect x=x y=y width=run height="1.02" fill="url(#thin-hatch)" /> })
        .collect_view()
}

/// Density cells as a handful of rectangles: runs of the same level in a row become one.
fn plan_cells(plan: &Plan) -> impl IntoView {
    let level_of = |value: u32| (value * LEVELS).div_ceil(256).min(LEVELS);
    let mut rects = Vec::new();
    for (y, row) in plan.cells.iter().enumerate() {
        let mut x = 0;
        while x < row.len() {
            let level = level_of(row[x]);
            let mut end = x + 1;
            while end < row.len() && level_of(row[end]) == level {
                end += 1;
            }
            if level > 0 {
                rects.push((x, y, end - x, level));
            }
            x = end;
        }
    }
    rects
        .into_iter()
        .map(|(x, y, run, level)| view! { <rect x=x y=y width=run height="1.02" class=format!("cell cell-{level}") /> })
        .collect_view()
}

/// Where the pointer is on the plan, in cells.
fn pointer_at(svg: NodeRef<leptos::svg::Svg>, x: i32, y: i32, view_width: f64, view_height: f64) -> Option<(f64, f64)> {
    let element = svg.get()?;
    point_in_plan(element.as_ref(), (x as f64, y as f64), view_width, view_height)
}

/// Saves what the person decided about a door and shows the answer.
fn save_door(room: String, x: f64, y: f64, status: &'static str, to: Option<String>, doors: RwSignal<Vec<DoorView>>) {
    leptos::task::spawn_local(async move {
        let args = TauriBridge::object(&[
            ("room", JsValue::from_str(&room)),
            ("x", JsValue::from_f64(x)),
            ("y", JsValue::from_f64(y)),
            ("status", JsValue::from_str(status)),
            ("to", to.map(|id| JsValue::from_str(&id)).unwrap_or(JsValue::NULL)),
        ]);
        match TauriBridge::call::<RoomOverview>("set_door", args).await {
            Ok(updated) => doors.set(updated.doors),
            Err(message) => leptos::logging::error!("set_door failed: {message}"),
        }
    });
}

/// Rejected doors stay out of sight; the rest are numbered in the order of the list.
fn visible_doors(doors: Vec<DoorView>) -> Vec<DoorView> {
    doors.into_iter().filter(|door| door.state != "rejected").collect()
}

fn door_markers(doors: Vec<DoorView>) -> impl IntoView {
    visible_doors(doors)
        .into_iter()
        .enumerate()
        .map(|(index, door)| {
            let (x, y) = (door.x, door.y);
            view! {
                <g class=format!("door door-{}", door.state) transform=format!("translate({x} {y})")>
                    <circle r="2.6" />
                    <text y="0.9" text-anchor="middle">{index + 1}</text>
                </g>
            }
        })
        .collect_view()
}

/// The doors as a list: confirm or reject what the detector found, say where a door leads.
fn door_list(doors: RwSignal<Vec<DoorView>>, other_rooms: Vec<RoomRef>, room: String) -> impl IntoView {
    view! {
        <h3 class="panel-title doors-title">"Doors"</h3>
        {move || {
            let shown = visible_doors(doors.get());
            if shown.is_empty() {
                return view! { <p class="media-note">"No doors found yet. Add one on the floor plan."</p> }.into_any();
            }
            let rows = shown
                .into_iter()
                .enumerate()
                .map(|(index, door)| door_row(index + 1, door, other_rooms.clone(), room.clone(), doors))
                .collect_view();
            view! { <ul class="door-list">{rows}</ul> }.into_any()
        }}
    }
}

fn door_row(number: usize, door: DoorView, other_rooms: Vec<RoomRef>, room: String, doors: RwSignal<Vec<DoorView>>) -> impl IntoView {
    let (x, y) = (door.x, door.y);
    let reviewed = door.state != "detected";
    let note = match door.state.as_str() {
        "detected" => format!("Seen in {}. Is it a door?", count_of(door.frames as usize, "frame", "frames")),
        "manual" => "Placed by you".to_string(),
        _ => "Confirmed".to_string(),
    };
    let current = door.to.clone().unwrap_or_default();
    let keep = door.state.clone();
    let (confirm_room, reject_room, change_room) = (room.clone(), room.clone(), room);
    view! {
        <li class="door-row">
            <span class="door-number">{number}</span>
            <span class="door-main">
                <span class="door-note">{note}</span>
                {reviewed.then(|| view! {
                    <select
                        class="input select"
                        aria-label="Where this door leads"
                        on:change=move |event| {
                            let target = event_target_value(&event);
                            let status = if keep == "manual" { "manual" } else { "confirmed" };
                            save_door(change_room.clone(), x, y, status, Some(target), doors);
                        }
                    >
                        <option value="" selected=current.is_empty()>"Leads nowhere yet"</option>
                        {other_rooms.iter().map(|other| view! {
                            <option value=other.id.clone() selected=current == other.id>{format!("Leads to {}", other.name)}</option>
                        }).collect_view()}
                    </select>
                })}
            </span>
            <span class="door-actions">
                {(!reviewed).then(|| view! {
                    <button class="btn btn-primary" on:click=move |_| save_door(confirm_room.clone(), x, y, "confirmed", None, doors)>"Yes"</button>
                })}
                <button class="btn btn-ghost" on:click=move |_| save_door(reject_room.clone(), x, y, "rejected", None, doors)>
                    {if reviewed { "Remove" } else { "No" }}
                </button>
            </span>
        </li>
    }
}
