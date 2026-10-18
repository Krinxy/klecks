// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;

use crate::bridge::TauriBridge;
use crate::components::scene_viewer::SceneViewer;
use crate::components::stage::Stage;
use crate::flow::ScanFlow;
use crate::format::{count_of, when};
use crate::model::{Plan, RoomOverview, Thin, VideoScore, VideoShare, View};

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
                        on:click=move |_| flow.scan_room(scan_room.clone(), scan_label.clone())
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
                    </div>
                    <div class="panel-stage">
                        {move || match (pane.get(), scan_result.clone()) {
                            (Pane::Camera, Some(scan)) => view! { <SceneViewer scan=scan flow=flow embedded=true /> }.into_any(),
                            _ => plan_view(plan.clone(), scanned).into_any(),
                        }}
                    </div>
                </section>

                <section class="panel panel-media">
                    <h2 class="panel-title">"Media that built this room"</h2>
                    <p class="panel-sub">"The bar is how much of the room a film saw. The number is what only that film saw. Both are estimates from where the camera stood."</p>
                    <ul class="media-list">
                        {videos.into_iter().enumerate().map(|(rank, video)| {
                            let score = video.index.and_then(|index| scores.get(index)).cloned();
                            media_row(video, rank, score, footprint)
                        }).collect_view()}
                    </ul>
                    {next_best.map(|text| view! {
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
fn plan_view(plan: Option<Plan>, scanned: bool) -> impl IntoView {
    let Some(plan) = plan else {
        let text = if scanned {
            "This scan has no plan yet. Scan the room again and it will be drawn."
        } else {
            "The plan appears after the first scan."
        };
        return view! { <div class="plan-empty">{text}</div> }.into_any();
    };
    let (width, height) = (plan.width as f64, plan.height as f64);
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
            <svg viewBox=format!("-2 -2 {} {}", width + 4.0, height + 4.0) role="img" aria-label="Floor plan seen from above">
                <defs>
                    <pattern id="thin-hatch" width="1.6" height="1.6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                        <line x1="0" y1="0" x2="0" y2="1.6" class="hatch-line" />
                    </pattern>
                </defs>
                <rect x="-2" y="-2" width=width + 4.0 height=height + 4.0 class="plan-ground" />
                {cells}
                {hatch}
                {paths}
            </svg>
            <p class="plan-note">
                {format!("{covered}% is seen by enough frames. Hatched parts are seen by few. ")}
                "Lines are the way you walked, one colour per film. Doors are not detected yet."
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
