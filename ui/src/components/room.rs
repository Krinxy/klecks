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
use crate::model::{Plan, RoomOverview, VideoShare, View};

/// How many of the six density steps a cell shows (0 = empty).
const LEVELS: u32 = 5;
/// A heading tick every this many cameras, so the path does not turn into a hedgehog.
const HEADING_EVERY: usize = 4;
const HEADING_LENGTH: f64 = 1.8;

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
    let videos = data.videos.clone();
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
                    <p class="panel-sub">"Frames each film gave to the scene. A film that saw parts nobody else saw is worth more."</p>
                    <ul class="media-list">
                        {videos.into_iter().enumerate().map(|(rank, video)| media_row(video, rank)).collect_view()}
                    </ul>
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

fn media_row(video: VideoShare, rank: usize) -> impl IntoView {
    let thumb = video.thumb.as_deref().map(TauriBridge::file_url);
    let detail = if video.scanned {
        format!("{}, {:.0} MB", count_of(video.frames as usize, "frame", "frames"), video.size_mb)
    } else {
        format!("New, {:.0} MB", video.size_mb)
    };
    view! {
        <li class="media-row" class:is-new=!video.scanned>
            <span class=format!("media-thumb walk-{}", video.index.unwrap_or(0) % 6)>{thumb.map(|src| view! { <img src=src alt="" /> })}</span>
            <span class="media-main">
                <span class="media-name">{video.name.clone()}</span>
                <span class="media-detail">{detail}</span>
                <span class="media-bar"><span style=format!("width: {}%", video.share_pct.max(if video.scanned { 2 } else { 0 }))></span></span>
            </span>
            <span class="media-share" class:is-top=rank == 0 && video.scanned>
                {if video.scanned { format!("{}%", video.share_pct) } else { "new".to_string() }}
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
                <rect x="-2" y="-2" width=width + 4.0 height=height + 4.0 class="plan-ground" />
                {cells}
                {paths}
            </svg>
            <p class="plan-note">"The filled area is what the films saw. Lines are the way you walked, one colour per film. Doors are not detected yet."</p>
        </div>
    }
    .into_any()
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
