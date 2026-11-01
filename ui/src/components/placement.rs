// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-11-01
// ---
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen::{JsCast, JsValue};

use crate::bridge::{TauriBridge, Viewer3d};
use crate::format::Text;
use crate::model::{Asset, Cut, Layout, Placed, Transform};

/// Edge length of a piece without a known size, in centimetres.
const UNKNOWN_CM: f64 = 60.0;
/// How far up a new cut-out reaches, and how much it takes below the floor so no floor splats are left.
const CUT_TOP_CM: f64 = 130.0;
const CUT_BELOW_FLOOR_CM: f64 = 10.0;
const CUT_STEP_CM: f64 = 20.0;
const TURN_STEP: f64 = 15.0;
const SCALE_STEP: f64 = 1.05;
/// A dragged rectangle smaller than this many cells is a click, not a cut-out.
const MIN_CUT_CELLS: f64 = 2.0;

/// What a click on the plan does right now.
#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    None,
    Door,
    Cut,
}

/// Everything the plan and the furniture panel share: the layout of the room, what is selected and what is being dragged.
#[derive(Clone, Copy)]
pub struct Placement {
    room: StoredValue<String>,
    transform: StoredValue<Option<Transform>>,
    pub layout: RwSignal<Layout>,
    pub selected: RwSignal<Option<u32>>,
    pub tool: RwSignal<Tool>,
    pub assets: RwSignal<Vec<Asset>>,
    /// The catalog card being dragged towards the 3D view.
    pub dragging: RwSignal<Option<String>>,
    /// (item, grab offset in cells) while an item is dragged.
    drag: RwSignal<Option<(u32, f64, f64)>>,
    /// The rectangle being drawn for a cut-out: (x0, y0, x1, y1).
    sketch: RwSignal<Option<(f64, f64, f64, f64)>>,
    plan_size: StoredValue<(f64, f64)>,
}

impl Placement {
    pub fn new(room: String, layout: Layout, transform: Option<Transform>, plan_size: (f64, f64)) -> Self {
        let this = Self {
            room: StoredValue::new(room),
            transform: StoredValue::new(transform),
            layout: RwSignal::new(layout),
            selected: RwSignal::new(None),
            tool: RwSignal::new(Tool::None),
            assets: RwSignal::new(Vec::new()),
            dragging: RwSignal::new(None),
            drag: RwSignal::new(None),
            sketch: RwSignal::new(None),
            plan_size: StoredValue::new(plan_size),
        };
        leptos::task::spawn_local(async move {
            if let Ok(list) = TauriBridge::call::<Vec<Asset>>("list_assets", JsValue::UNDEFINED).await {
                this.assets.set(list);
                this.sync_viewer();
            }
        });
        this.sync_viewer();
        this
    }

    /// False for scans from before the plan knew its scale and orientation.
    pub fn can_place(&self) -> bool {
        self.cm_per_cell().is_some() && self.transform.with_value(|t| t.as_ref().is_some_and(|t| t.basis.len() == 3 && t.scale > 0.0))
    }

    /// Size of a plan cell in centimetres: the person's own number, else the estimate.
    pub fn cm_per_cell(&self) -> Option<f64> {
        self.layout.with(|layout| layout.cm_per_cell).or_else(|| self.transform.with_value(|t| t.as_ref().and_then(|t| t.cm_per_cell)))
    }

    /// Width, height and depth in centimetres; a piece without a size is a cube.
    pub fn size_of(&self, asset: &str) -> (f64, f64, f64) {
        self.assets.with(|list| {
            list.iter()
                .find(|candidate| candidate.id == asset)
                .and_then(|found| found.size)
                .map(|size| (size.width_cm as f64, size.height_cm as f64, size.depth_cm as f64))
                .unwrap_or((UNKNOWN_CM, UNKNOWN_CM, UNKNOWN_CM))
        })
    }

    pub fn name_of(&self, asset: &str) -> String {
        self.assets.with(|list| list.iter().find(|candidate| candidate.id == asset).map(|found| found.name.clone())).unwrap_or_else(|| Text::capitalised(asset))
    }

    /// The catalog picture of a piece (the one named front, else the first), as an address the view can load.
    fn image_of(&self, asset: &str) -> Option<String> {
        self.assets.with_untracked(|list| {
            let found = list.iter().find(|candidate| candidate.id == asset)?;
            let path = found.images.iter().find(|path| path.to_lowercase().contains("front")).or_else(|| found.images.first())?;
            Some(TauriBridge::file_url(path))
        })
    }

    /// Where a spot of the scan lies on the plan, in cells.
    fn cell_of_world(&self, point: [f64; 3]) -> Option<(f64, f64)> {
        let transform = self.transform.get_value()?;
        if transform.basis.len() != 3 || transform.scale <= 0.0 {
            return None;
        }
        let across = |index: usize| (0..3).map(|row| transform.basis[row][index] * point[row]).sum::<f64>();
        Some(((across(0) - transform.low[0]) * transform.scale, (across(1) - transform.low[1]) * transform.scale))
    }

    /// Puts a piece where it was dropped on the floor of the 3D view.
    pub fn drop_at(&self, asset: &str, client: (f64, f64)) {
        match Viewer3d::floor_point(client.0, client.1).and_then(|point| self.cell_of_world(point)) {
            Some(cell) => self.place_at(asset, cell),
            None => self.place(asset),
        }
    }

    /// Puts a piece on the floor in the middle of the 3D view (a click on a card of the catalog).
    pub fn place_in_view(&self, asset: &str) {
        match Viewer3d::floor_center().and_then(|point| self.cell_of_world(point)) {
            Some(cell) => self.place_at(asset, cell),
            None => self.place(asset),
        }
    }

    /// What the 3D view says about the pieces: one was picked, is being dragged over the floor, or was let go.
    pub fn piece_event(&self, detail: &JsValue) {
        let field = |name: &str| js_sys::Reflect::get(detail, &name.into()).unwrap_or(JsValue::UNDEFINED);
        let kind = field("kind").as_string().unwrap_or_default();
        let index = field("index").as_f64().map(|value| value as usize);
        let id = index.and_then(|index| self.layout.with_untracked(|layout| layout.items.get(index).map(|item| item.id)));
        let Some(id) = id else { return };
        match kind.as_str() {
            "select" => {
                self.selected.set(Some(id));
                self.sync_viewer();
            }
            "move" => {
                let point: Option<[f64; 3]> = serde_wasm_bindgen::from_value(field("point")).ok();
                if let Some((x, y)) = point.and_then(|point| self.cell_of_world(point)) {
                    let (width, height) = self.plan_size.get_value();
                    self.layout.update(|layout| {
                        if let Some(item) = layout.items.iter_mut().find(|item| item.id == id) {
                            item.x = x.clamp(0.0, width);
                            item.y = y.clamp(0.0, height);
                        }
                    });
                    self.sync_viewer();
                }
            }
            "done" => self.changed(),
            _ => {}
        }
    }

    /// Puts a piece in the middle of the plan and selects it.
    pub fn place(&self, asset: &str) {
        let (width, height) = self.plan_size.get_value();
        self.place_at(asset, (width / 2.0, height / 2.0));
    }

    /// Puts a piece at a spot of the plan and selects it.
    pub fn place_at(&self, asset: &str, cell: (f64, f64)) {
        let (width, height) = self.plan_size.get_value();
        let id = self.layout.with(|layout| layout.items.iter().map(|item| item.id).max().unwrap_or(0) + 1);
        let (x, y) = (cell.0.clamp(0.0, width), cell.1.clamp(0.0, height));
        self.layout.update(|layout| layout.items.push(Placed { id, asset: asset.to_string(), x, y, turn: 0.0 }));
        self.selected.set(Some(id));
        self.changed();
    }

    pub fn turn(&self, id: u32, degrees: f64) {
        self.layout.update(|layout| {
            if let Some(item) = layout.items.iter_mut().find(|item| item.id == id) {
                item.turn = (item.turn + degrees).rem_euclid(360.0);
            }
        });
        self.changed();
    }

    pub fn remove(&self, id: u32) {
        self.layout.update(|layout| layout.items.retain(|item| item.id != id));
        if self.selected.get_untracked() == Some(id) {
            self.selected.set(None);
        }
        self.changed();
    }

    pub fn remove_cut(&self, id: u32) {
        self.layout.update(|layout| layout.cuts.retain(|cut| cut.id != id));
        self.changed();
    }

    pub fn raise_cut(&self, id: u32, by_cm: f64) {
        self.layout.update(|layout| {
            if let Some(cut) = layout.cuts.iter_mut().find(|cut| cut.id == id) {
                cut.top_cm = (cut.top_cm + by_cm).clamp(CUT_STEP_CM, 400.0);
            }
        });
        self.changed();
    }

    /// Makes the plan's cells smaller or larger when the estimated size of the room is off.
    pub fn rescale(&self, factor: Option<f64>) {
        let current = self.cm_per_cell();
        self.layout.update(|layout| {
            layout.cm_per_cell = match (factor, current) {
                (Some(factor), Some(now)) => Some((now * factor).clamp(1.0, 50.0)),
                _ => None,
            };
        });
        self.changed();
    }

    pub fn scale_step_up(&self) -> f64 {
        SCALE_STEP
    }

    pub fn scale_step_down(&self) -> f64 {
        1.0 / SCALE_STEP
    }

    pub fn turn_step(&self) -> f64 {
        TURN_STEP
    }

    pub fn cut_step(&self) -> f64 {
        CUT_STEP_CM
    }

    /// Saves and shows the change.
    fn changed(&self) {
        self.save();
        self.sync_viewer();
    }

    fn save(&self) {
        let room = self.room.get_value();
        let layout = self.layout.get_untracked();
        leptos::task::spawn_local(async move {
            let Ok(value) = serde_wasm_bindgen::to_value(&layout) else { return };
            let args = TauriBridge::object(&[("room", JsValue::from_str(&room)), ("layout", value)]);
            if let Err(message) = TauriBridge::fire_with("save_layout", args).await {
                leptos::logging::error!("save_layout failed: {message}");
            }
        });
    }

    /// Tells the 3D view what stands in the room and what is hidden.
    pub fn sync_viewer(&self) {
        // Untracked: this is called from inside reactive closures, which must not re-run when the layout changes.
        let Some(layout) = untrack(|| self.viewer_layout()) else { return };
        if let Ok(value) = serde_wasm_bindgen::to_value(&layout) {
            Viewer3d::set_layout(&value);
        }
    }

    fn viewer_layout(&self) -> Option<ViewerLayout> {
        let cm = self.cm_per_cell()?;
        let transform = self.transform.get_value()?;
        if transform.basis.len() != 3 || transform.scale <= 0.0 {
            return None;
        }
        // scan units per centimetre
        let per_cm = 1.0 / transform.scale / cm;
        let selected = self.selected.get_untracked();
        let layout = self.layout.get_untracked();
        let items = layout
            .items
            .iter()
            .map(|item| {
                let (width, height, depth) = self.size_of(&item.asset);
                ViewerBox {
                    pos: world(&transform, item.x, item.y, height * per_cm / 2.0),
                    size: [width * per_cm, height * per_cm, depth * per_cm],
                    turn: item.turn,
                    selected: selected == Some(item.id),
                    image: self.image_of(&item.asset),
                }
            })
            .collect();
        let cuts = layout
            .cuts
            .iter()
            .map(|cut| {
                let reach = (cut.top_cm + CUT_BELOW_FLOOR_CM) * per_cm;
                ViewerBox {
                    pos: world(&transform, cut.x + cut.w / 2.0, cut.y + cut.h / 2.0, (cut.top_cm - CUT_BELOW_FLOOR_CM) * per_cm / 2.0),
                    size: [cut.w / transform.scale, reach, cut.h / transform.scale],
                    turn: 0.0,
                    selected: false,
                    image: None,
                }
            })
            .collect();
        let column = |index: usize| [transform.basis[0][index], transform.basis[1][index], transform.basis[2][index]];
        let (plan_width, plan_height) = self.plan_size.get_value();
        let floor = world(&transform, plan_width / 2.0, plan_height / 2.0, 0.0);
        Some(ViewerLayout { floor, frame: ViewerFrame { a: column(0), b: column(1), up: transform.up }, items, cuts })
    }

    // ── pointer handling on the plan ──────────────────────────────────────────────────────────

    /// Something under the pointer was pressed: start dragging a piece.
    pub fn grab(&self, id: u32, at: (f64, f64)) {
        self.selected.set(Some(id));
        if let Some(item) = self.layout.with_untracked(|layout| layout.items.iter().find(|item| item.id == id).cloned()) {
            self.drag.set(Some((id, item.x - at.0, item.y - at.1)));
        }
        self.sync_viewer();
    }

    /// The pointer is pressed on the empty plan: start a cut-out when that is the tool.
    pub fn press(&self, at: (f64, f64)) {
        if self.tool.get_untracked() == Tool::Cut {
            self.sketch.set(Some((at.0, at.1, at.0, at.1)));
        } else {
            self.selected.set(None);
            self.sync_viewer();
        }
    }

    pub fn moved(&self, at: (f64, f64)) {
        if let Some((id, dx, dy)) = self.drag.get_untracked() {
            let (width, height) = self.plan_size.get_value();
            self.layout.update(|layout| {
                if let Some(item) = layout.items.iter_mut().find(|item| item.id == id) {
                    item.x = (at.0 + dx).clamp(0.0, width);
                    item.y = (at.1 + dy).clamp(0.0, height);
                }
            });
            self.sync_viewer();
        } else if let Some((x0, y0, _, _)) = self.sketch.get_untracked() {
            self.sketch.set(Some((x0, y0, at.0, at.1)));
        }
    }

    pub fn released(&self) {
        if self.drag.get_untracked().is_some() {
            self.drag.set(None);
            self.changed();
        }
        if let Some((x0, y0, x1, y1)) = self.sketch.get_untracked() {
            self.sketch.set(None);
            self.tool.set(Tool::None);
            let (x, y, w, h) = (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
            if w >= MIN_CUT_CELLS && h >= MIN_CUT_CELLS {
                let id = self.layout.with_untracked(|layout| layout.cuts.iter().map(|cut| cut.id).max().unwrap_or(0) + 1);
                self.layout.update(|layout| layout.cuts.push(Cut { id, x, y, w, h, top_cm: CUT_TOP_CM }));
                self.changed();
            }
        }
    }
}

#[derive(Serialize)]
struct ViewerFrame {
    a: [f64; 3],
    b: [f64; 3],
    up: [f64; 3],
}

#[derive(Serialize)]
struct ViewerBox {
    pos: [f64; 3],
    size: [f64; 3],
    turn: f64,
    selected: bool,
    /// The catalog picture shown on the piece.
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
}

#[derive(Serialize)]
struct ViewerLayout {
    /// Any point on the floor: with `frame.up` it is the floor plane the pointer is cast onto.
    floor: [f64; 3],
    frame: ViewerFrame,
    items: Vec<ViewerBox>,
    cuts: Vec<ViewerBox>,
}

/// A spot on the plan, `lift` above the floor, in scan coordinates.
fn world(transform: &Transform, x: f64, y: f64, lift: f64) -> [f64; 3] {
    let ground = [transform.low[0] + x / transform.scale, transform.low[1] + y / transform.scale];
    let mut point = [0.0; 3];
    for (index, value) in point.iter_mut().enumerate() {
        *value = transform.basis[index][0] * ground[0] + transform.basis[index][1] * ground[1] + transform.up[index] * (transform.floor + lift);
    }
    point
}

/// Where the pointer is on the plan, in cells. The SVG keeps its aspect ratio, so the picture sits in the middle of
/// the element and the borders have to be taken off first.
pub fn point_in_plan(svg: &JsValue, client: (f64, f64), view_width: f64, view_height: f64) -> Option<(f64, f64)> {
    use wasm_bindgen::JsCast;
    let measure = js_sys::Reflect::get(svg, &"getBoundingClientRect".into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    let rect = measure.call0(svg).ok()?;
    let side = |name: &str| js_sys::Reflect::get(&rect, &name.into()).ok().and_then(|value| value.as_f64());
    let (left_edge, top_edge, width, height) = (side("left")?, side("top")?, side("width")?, side("height")?);
    let scale = (width / view_width).min(height / view_height);
    if scale <= 0.0 {
        return None;
    }
    let left = (width - view_width * scale) / 2.0;
    let top = (height - view_height * scale) / 2.0;
    Some(((client.0 - left_edge - left) / scale - 2.0, (client.1 - top_edge - top) / scale - 2.0))
}

// ── drawing on the plan ───────────────────────────────────────────────────────────────────────

/// The furniture and cut-outs as shapes inside the plan's SVG.
pub fn plan_layer(placement: Placement, svg: NodeRef<leptos::svg::Svg>, view_size: (f64, f64)) -> impl IntoView {
    let pointer_down = move |id: u32, event: leptos::ev::PointerEvent| {
        event.stop_propagation();
        let Some(element) = svg.get() else { return };
        let element: &JsValue = element.as_ref();
        if let Some(at) = point_in_plan(element, (event.client_x() as f64, event.client_y() as f64), view_size.0, view_size.1) {
            placement.grab(id, at);
        }
    };
    view! {
        {move || {
            let Some(cm) = placement.cm_per_cell() else { return ().into_any() };
            let selected = placement.selected.get();
            let items = placement
                .layout
                .get()
                .items
                .into_iter()
                .map(|item| {
                    let (width, _, depth) = placement.size_of(&item.asset);
                    let (w, d) = (width / cm, depth / cm);
                    let id = item.id;
                    let label = placement.name_of(&item.asset);
                    view! {
                        <g
                            class="piece"
                            class:is-selected=selected == Some(id)
                            transform=format!("translate({} {}) rotate({})", item.x, item.y, item.turn)
                            on:pointerdown=move |event| pointer_down(id, event)
                        >
                            <rect x=-w / 2.0 y=-d / 2.0 width=w height=d />
                            <line class="piece-front" x1=-w / 2.0 y1=d / 2.0 x2=w / 2.0 y2=d / 2.0 />
                            <text transform=format!("rotate({})", -item.turn) text-anchor="middle" y="0.9">{label}</text>
                        </g>
                    }
                })
                .collect_view();
            view! { {items} }.into_any()
        }}
        {move || placement.layout.get().cuts.into_iter().map(|cut| view! {
            <rect class="cut" x=cut.x y=cut.y width=cut.w height=cut.h />
        }).collect_view()}
        {move || placement.sketch.get().map(|(x0, y0, x1, y1)| view! {
            <rect class="cut is-sketch" x=x0.min(x1) y=y0.min(y1) width=(x1 - x0).abs() height=(y1 - y0).abs() />
        })}
    }
}

/// The furniture tab: pick pieces from the catalog, turn or remove the selected one, cut parts out of the scan.
pub fn furniture_panel(placement: Placement) -> impl IntoView {
    view! {
        {move || (!placement.can_place()).then(|| view! {
            <p class="media-note">"This scan has no size or orientation yet. Scan the room again, then furniture can be placed."</p>
        })}
        <h3 class="panel-title side-head">"Place a piece"</h3>
        {move || {
            let list = placement.assets.get();
            if list.is_empty() {
                return view! { <p class="media-note">"The catalog is empty. Add folders to media/assets first."</p> }.into_any();
            }
            view! {
                <ul class="pick-grid">
                    {list.into_iter().map(|asset| {
                        let id = asset.id.clone();
                        let size = asset.size.map(|size| format!("{} x {} cm", size.width_cm, size.depth_cm)).unwrap_or_else(|| "size missing".into());
                        view! {
                            <li>
                                <button class="pick" disabled=move || !placement.can_place() on:click=move |_| placement.place(&id)>
                                    <span class="pick-name">{asset.name.clone()}</span>
                                    <span class="pick-size">{size}</span>
                                </button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            }.into_any()
        }}

        {move || {
            let chosen = placement.selected.get().and_then(|id| placement.layout.with(|layout| layout.items.iter().find(|item| item.id == id).cloned()));
            chosen.map(|item| {
                let id = item.id;
                let (width, height, depth) = placement.size_of(&item.asset);
                view! {
                    <div class="chosen">
                        <h3 class="panel-title side-head">{placement.name_of(&item.asset)}</h3>
                        <p class="panel-sub">{format!("{width:.0} x {depth:.0} x {height:.0} cm. Drag it on the plan.")}</p>
                        <div class="detail-actions">
                            <button class="btn btn-ghost" on:click=move |_| placement.turn(id, -placement.turn_step())>"Turn left"</button>
                            <button class="btn btn-ghost" on:click=move |_| placement.turn(id, placement.turn_step())>"Turn right"</button>
                            <button class="btn btn-ghost" on:click=move |_| placement.turn(id, 90.0)>"Quarter turn"</button>
                            <button class="btn btn-ghost" on:click=move |_| placement.remove(id)>"Remove"</button>
                        </div>
                    </div>
                }
            })
        }}

        {move || {
            let items = placement.layout.get().items;
            (!items.is_empty()).then(|| view! {
                <h3 class="panel-title side-head">"In the room"</h3>
                <ul class="placed-list">
                    {items.into_iter().map(|item| {
                        let id = item.id;
                        view! {
                            <li>
                                <button
                                    class="placed"
                                    class:is-selected=move || placement.selected.get() == Some(id)
                                    on:click=move |_| { placement.selected.set(Some(id)); placement.sync_viewer(); }
                                >
                                    {placement.name_of(&item.asset)}
                                </button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            })
        }}

        <h3 class="panel-title side-head">"Hide parts of the scan"</h3>
        <p class="panel-sub">"Draw a rectangle on the plan to take furniture out of the picture, for example the old sofa."</p>
        <div class="detail-actions">
            <button
                class="btn"
                class:btn-primary=move || placement.tool.get() == Tool::Cut
                class:btn-ghost=move || placement.tool.get() != Tool::Cut
                disabled=move || !placement.can_place()
                on:click=move |_| placement.tool.update(|tool| *tool = if *tool == Tool::Cut { Tool::None } else { Tool::Cut })
            >
                {move || if placement.tool.get() == Tool::Cut { "Drag on the plan..." } else { "Cut out an area" }}
            </button>
        </div>
        {move || {
            let cuts = placement.layout.get().cuts;
            (!cuts.is_empty()).then(|| view! {
                <ul class="placed-list">
                    {cuts.into_iter().enumerate().map(|(number, cut)| {
                        let id = cut.id;
                        view! {
                            <li class="cut-row">
                                <span>{format!("Area {} up to {:.0} cm", number + 1, cut.top_cm)}</span>
                                <span class="door-actions">
                                    <button class="btn btn-ghost" on:click=move |_| placement.raise_cut(id, -placement.cut_step())>"Lower"</button>
                                    <button class="btn btn-ghost" on:click=move |_| placement.raise_cut(id, placement.cut_step())>"Higher"</button>
                                    <button class="btn btn-ghost" on:click=move |_| placement.remove_cut(id)>"Show again"</button>
                                </span>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            })
        }}

        <h3 class="panel-title side-head">"Size of the room"</h3>
        <p class="panel-sub">
            {move || match placement.cm_per_cell() {
                Some(cm) => format!("One cell of the plan is about {cm:.1} cm. This is estimated from the height you held the phone at. Change it if furniture looks too small or too big."),
                None => "Not known for this scan.".to_string(),
            }}
        </p>
        <div class="detail-actions">
            <button class="btn btn-ghost" on:click=move |_| placement.rescale(Some(placement.scale_step_down()))>"Smaller"</button>
            <button class="btn btn-ghost" on:click=move |_| placement.rescale(Some(placement.scale_step_up()))>"Larger"</button>
            <button class="btn btn-ghost" on:click=move |_| placement.rescale(None)>"Use the estimate"</button>
        </div>
    }
}

/// The catalog as a sidebar over the free camera: click a card to put the piece in the middle of the view, or drag
/// it into the room and let go where it should stand. A placed piece can be dragged around in the room too.
pub fn freecam_sidebar(placement: Placement) -> impl IntoView {
    let open = RwSignal::new(true);
    let query = RwSignal::new(String::new());
    view! {
        <aside class="freecam-side" class:is-closed=move || !open.get()>
            <button class="side-toggle" on:click=move |_| open.update(|on| *on = !*on) title="Show or hide the catalog">
                {move || if open.get() { "Catalog >" } else { "< Catalog" }}
            </button>
            <div class="side-content" hidden=move || !open.get()>
                <input
                    class="input"
                    type="search"
                    placeholder="Search the catalog"
                    spellcheck="false"
                    prop:value=move || query.get()
                    on:input=move |event| query.set(event_target_value(&event))
                />
                {move || (!placement.can_place()).then(|| view! {
                    <p class="side-note">"Scan this room again to place furniture: this scan has no size yet."</p>
                })}
                <ul class="side-cards">
                    {move || {
                        let word = query.get().trim().to_lowercase();
                        placement
                            .assets
                            .get()
                            .into_iter()
                            .filter(|asset| word.is_empty() || asset.name.to_lowercase().contains(&word) || asset.tags.iter().any(|tag| tag.contains(&word)))
                            .map(|asset| {
                                let id = asset.id.clone();
                                let id_drag = asset.id.clone();
                                let photo = asset
                                    .images
                                    .iter()
                                    .find(|path| path.to_lowercase().contains("front"))
                                    .or_else(|| asset.images.first())
                                    .map(|path| TauriBridge::file_url(path));
                                view! {
                                    <li
                                        class="side-card"
                                        draggable="true"
                                        title="Click to place in the middle, or drag into the room"
                                        on:dragstart=move |event| {
                                            if let Some(transfer) = js_sys::Reflect::get(&event, &"dataTransfer".into()).ok().filter(|value| !value.is_null() && !value.is_undefined()) {
                                                if let Ok(set) = js_sys::Reflect::get(&transfer, &"setData".into()).and_then(|f| f.dyn_into::<js_sys::Function>()) {
                                                    let _ = set.call2(&transfer, &"text/plain".into(), &id_drag.as_str().into());
                                                }
                                            }
                                            placement.dragging.set(Some(id_drag.clone()));
                                        }
                                        on:dragend=move |_| placement.dragging.set(None)
                                        on:click=move |_| placement.place_in_view(&id)
                                    >
                                        <span class="side-photo">
                                            {photo.map(|src| view! { <img src=src alt="" draggable="false" /> })}
                                        </span>
                                        <span class="side-name">{asset.name.clone()}</span>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
                {move || {
                    let chosen = placement.selected.get().and_then(|id| placement.layout.with(|layout| layout.items.iter().find(|item| item.id == id).cloned()));
                    chosen.map(|item| {
                        let id = item.id;
                        view! {
                            <div class="side-chosen">
                                <span class="side-name">{placement.name_of(&item.asset)}</span>
                                <div class="side-buttons">
                                    <button class="btn btn-ghost" on:click=move |_| placement.turn(id, -placement.turn_step())>"Turn left"</button>
                                    <button class="btn btn-ghost" on:click=move |_| placement.turn(id, placement.turn_step())>"Turn right"</button>
                                    <button class="btn btn-ghost" on:click=move |_| placement.remove(id)>"Remove"</button>
                                </div>
                            </div>
                        }
                    })
                }}
            </div>
        </aside>
    }
}
