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
use crate::format::{capitalised, count_of};
use crate::model::Asset;

/// The four sides a shop photo can show; a file is assigned by the word in its name (`sofa_front.jpg`).
const SIDES: [&str; 4] = ["front", "back", "left", "right"];
/// Categories that cover a wall or floor instead of standing in the room.
const SURFACES: [&str; 3] = ["wallpaper", "floor", "floors"];

fn reveal(kind: &'static str) {
    leptos::task::spawn_local(async move {
        let _ = TauriBridge::fire_with("reveal_folder", TauriBridge::map(&[("kind", kind)])).await;
    });
}

/// "280 x 160 x 85 cm" or a hint on how to add the size.
fn size_text(asset: &Asset) -> String {
    match asset.size {
        Some(size) => format!("{} x {} x {} cm", size.width_cm, size.depth_cm, size.height_cm),
        None => "Size missing".to_string(),
    }
}

fn side_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase();
    SIDES.into_iter().find(|side| name.contains(side))
}

fn is_surface(asset: &Asset) -> bool {
    SURFACES.contains(&asset.category.to_lowercase().as_str())
}

fn matches(asset: &Asset, category: &Option<String>, tag: &Option<String>, query: &str) -> bool {
    if category.as_ref().is_some_and(|wanted| !asset.category.eq_ignore_ascii_case(wanted)) {
        return false;
    }
    if tag.as_ref().is_some_and(|wanted| !asset.tags.contains(wanted)) {
        return false;
    }
    let query = query.trim().to_lowercase();
    query.is_empty()
        || query.split_whitespace().all(|word| {
            asset.name.to_lowercase().contains(word)
                || asset.category.to_lowercase().contains(word)
                || asset.notes.to_lowercase().contains(word)
                || asset.tags.iter().any(|tag| tag.contains(word))
        })
}

/// The catalog: categories and tags on the left, what matches in the middle, the picked piece on the right.
#[component]
pub fn CatalogScreen(flow: ScanFlow) -> impl IntoView {
    let assets = RwSignal::new(None::<Vec<Asset>>);
    let category = RwSignal::new(None::<String>);
    let tag = RwSignal::new(None::<String>);
    let query = RwSignal::new(String::new());
    let picked = RwSignal::new(None::<String>);

    let refresh = move || {
        leptos::task::spawn_local(async move {
            assets.set(Some(
                TauriBridge::call::<Vec<Asset>>("list_assets", JsValue::UNDEFINED).await.unwrap_or_default(),
            ));
        });
    };
    refresh();

    let actions = move || {
        view! {
            <input
                class="input search"
                type="search"
                placeholder="Search: sofa grey, oak, lamp"
                spellcheck="false"
                prop:value=move || query.get()
                on:input=move |event| query.set(event_target_value(&event))
            />
            <button class="btn btn-ghost" on:click=move |_| refresh()>"Refresh"</button>
            <button class="btn btn-ghost" on:click=move |_| reveal("assets")>"Open catalog folder"</button>
        }
    };

    view! {
        <Screen title="Catalog" subtitle="Furniture and surfaces for your rooms." flow=flow actions=actions>
            {move || match assets.get() {
                None => view! { <p class="screen-note">"Looking for furniture…"</p> }.into_any(),
                Some(list) if list.is_empty() => view! {
                    <div class="plate empty">
                        <h2 class="empty-title">"No furniture yet"</h2>
                        <p class="empty-text">
                            "Make one folder per piece, for example "
                            <code>"assets/sofa_grey"</code>
                            ", and put photos in it. Name them "
                            <code>"front"</code>", "<code>"back"</code>", "<code>"left"</code>" and "<code>"right"</code>
                            " to get the four views. An "
                            <code>"asset.yaml"</code>
                            " holds name, category, tags and the size in centimetres. A category called "
                            <code>"wallpaper"</code>
                            " is treated as a surface."
                        </p>
                        <button class="btn btn-primary" on:click=move |_| reveal("assets")>"Open catalog folder"</button>
                    </div>
                }.into_any(),
                Some(list) => catalog_body(list, category, tag, query, picked).into_any(),
            }}
        </Screen>
    }
}

fn catalog_body(
    list: Vec<Asset>,
    category: RwSignal<Option<String>>,
    tag: RwSignal<Option<String>>,
    query: RwSignal<String>,
    picked: RwSignal<Option<String>>,
) -> impl IntoView {
    let mut categories: Vec<(String, usize)> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    for asset in &list {
        let name = asset.category.to_lowercase();
        if !name.is_empty() {
            match categories.iter_mut().find(|(existing, _)| *existing == name) {
                Some((_, count)) => *count += 1,
                None => categories.push((name, 1)),
            }
        }
        for word in &asset.tags {
            if !tags.contains(word) {
                tags.push(word.clone());
            }
        }
    }
    categories.sort();
    tags.sort();
    let total = list.len();
    let list = StoredValue::new(list);

    let shown = move || {
        list.with_value(|all| {
            all.iter().filter(|asset| matches(asset, &category.get(), &tag.get(), &query.get())).cloned().collect::<Vec<_>>()
        })
    };

    view! {
        <div class="catalog">
            <aside class="rail">
                <h2 class="rail-title">"Category"</h2>
                <ul class="cats">
                    <li>
                        <button class="cat" class:is-selected=move || category.get().is_none() on:click=move |_| category.set(None)>
                            <span>"All"</span><span class="cat-count">{total}</span>
                        </button>
                    </li>
                    {categories.into_iter().map(|(name, count)| {
                        let id = name.clone();
                        let id_select = name.clone();
                        view! {
                            <li>
                                <button
                                    class="cat"
                                    class:is-selected=move || category.get().as_deref() == Some(id.as_str())
                                    on:click=move |_| category.set(Some(id_select.clone()))
                                >
                                    <span>{capitalised(&name)}</span><span class="cat-count">{count}</span>
                                </button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                {(!tags.is_empty()).then(|| view! {
                    <h2 class="rail-title">"Tags"</h2>
                    <div class="tag-cloud">
                        {tags.into_iter().map(|word| {
                            let id = word.clone();
                            let id_select = word.clone();
                            view! {
                                <button
                                    class="tag"
                                    class:is-selected=move || tag.get().as_deref() == Some(id.as_str())
                                    on:click=move |_| tag.update(|current| {
                                        *current = if current.as_deref() == Some(id_select.as_str()) { None } else { Some(id_select.clone()) };
                                    })
                                >
                                    {word}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                })}
            </aside>

            <section class="asset-column">
                {move || {
                    let found = shown();
                    if found.is_empty() {
                        view! { <p class="screen-note">"Nothing matches. Clear the search, category or tag."</p> }.into_any()
                    } else {
                        view! {
                            <ul class="asset-grid">
                                {found.into_iter().map(|asset| asset_card(asset, picked)).collect_view()}
                            </ul>
                        }.into_any()
                    }
                }}
            </section>

            <aside class="plate asset-detail">
                {move || {
                    let chosen = picked.get().and_then(|id| list.with_value(|all| all.iter().find(|asset| asset.id == id).cloned()));
                    match chosen.or_else(|| shown().into_iter().next()) {
                        Some(asset) => asset_detail(asset).into_any(),
                        None => view! { <p class="screen-note">"Pick a piece to see it from all sides."</p> }.into_any(),
                    }
                }}
            </aside>
        </div>
    }
}

fn asset_card(asset: Asset, picked: RwSignal<Option<String>>) -> impl IntoView {
    let photo = asset
        .images
        .iter()
        .find(|path| side_of(path) == Some("front"))
        .or_else(|| asset.images.first())
        .map(|path| TauriBridge::file_url(path));
    let id_class = asset.id.clone();
    let id_click = asset.id.clone();
    let size_known = asset.size.is_some();
    let surface = is_surface(&asset);
    view! {
        <li>
            <button
                class="asset-card"
                class:is-selected=move || picked.get().as_deref() == Some(id_class.as_str())
                on:click=move |_| picked.set(Some(id_click.clone()))
            >
                <span class="asset-photo" class:is-surface=surface>
                    {match photo {
                        Some(url) => view! { <img src=url alt=asset.name.clone() loading="lazy" /> }.into_any(),
                        None => view! { <span class="asset-nophoto">"No photo yet"</span> }.into_any(),
                    }}
                </span>
                <span class="asset-info">
                    <span class="asset-name">{asset.name.clone()}</span>
                    <span class="asset-size" class:is-missing=!size_known>{if surface { "Surface".to_string() } else { size_text(&asset) }}</span>
                </span>
            </button>
        </li>
    }
}

fn asset_detail(asset: Asset) -> impl IntoView {
    let surface = is_surface(&asset);
    let views: Vec<(String, Option<String>)> = if surface {
        Vec::new()
    } else {
        SIDES
            .iter()
            .map(|side| {
                let path = asset.images.iter().find(|path| side_of(path) == Some(*side)).map(|path| TauriBridge::file_url(path));
                ((*side).to_string(), path)
            })
            .collect()
    };
    let any_side = views.iter().any(|(_, path)| path.is_some());
    // Photos that are not named after a side are still shown, so nothing in the folder is hidden.
    let others: Vec<String> = asset
        .images
        .iter()
        .filter(|path| surface || !any_side || side_of(path).is_none())
        .map(|path| TauriBridge::file_url(path))
        .collect();
    let photos = count_of(asset.images.len(), "photo", "photos");
    view! {
        <div class="detail-head">
            <h2 class="detail-name small">{asset.name.clone()}</h2>
            <span class="detail-kind">{if asset.category.is_empty() { "No category".to_string() } else { capitalised(&asset.category) }}</span>
        </div>
        <p class="detail-size" class:is-missing=asset.size.is_none()>{if surface { photos.clone() } else { size_text(&asset) }}</p>

        {(!surface).then(|| view! {
            <div class="sides">
                {views.into_iter().map(|(side, path)| { let missing = path.is_none(); view! {
                    <figure class="side" class:is-missing=missing>
                        {match path {
                            Some(url) => view! { <img src=url alt=side.clone() loading="lazy" /> }.into_any(),
                            None => view! { <span>"missing"</span> }.into_any(),
                        }}
                        <figcaption>{capitalised(&side)}</figcaption>
                    </figure>
                } }).collect_view()}
            </div>
            <p class="detail-note">
                {if any_side { "Four sides like in a shop. The 3D body is built from them and the size." } else { "Name photos front, back, left and right to see all four sides." }}
            </p>
        })}
        {(!others.is_empty()).then(|| view! {
            <div class="sides other">
                {others.into_iter().map(|url| view! { <figure class="side"><img src=url alt="" loading="lazy" /></figure> }).collect_view()}
            </div>
        })}

        {(!asset.tags.is_empty()).then(|| view! {
            <div class="tag-cloud static">
                {asset.tags.iter().map(|word| view! { <span class="tag">{word.clone()}</span> }).collect_view()}
            </div>
        })}
        {(!asset.notes.is_empty()).then(|| view! { <p class="detail-note">{asset.notes.clone()}</p> })}

        <p class="detail-note">
            {if surface {
                "Surfaces are not placed in rooms yet."
            } else {
                "To put it in a room, open the room and choose the Furniture tab."
            }}
        </p>
    }
}
