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

fn reveal(kind: &'static str) {
    leptos::task::spawn_local(async move {
        let _ = TauriBridge::fire_with("reveal_folder", TauriBridge::map(&[("kind", kind)])).await;
    });
}

/// "280 × 160 × 85 cm" or a hint on how to add the size.
fn size_text(asset: &Asset) -> String {
    match asset.size {
        Some(size) => format!("{} × {} × {} cm", size.width_cm, size.depth_cm, size.height_cm),
        None => "Size missing".to_string(),
    }
}

/// The furniture catalog: one card per folder in `media/assets/`.
#[component]
pub fn CatalogScreen(flow: ScanFlow) -> impl IntoView {
    let assets = RwSignal::new(None::<Vec<Asset>>);
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
            <button class="btn btn-ghost" on:click=move |_| refresh()>"Refresh"</button>
            <button class="btn btn-ghost" on:click=move |_| reveal("assets")>"Open catalog folder"</button>
        }
    };

    view! {
        <Screen title="Catalog" flow=flow actions=actions>
            {move || match assets.get() {
                None => view! { <p class="screen-note">"Looking for furniture…"</p> }.into_any(),
                Some(list) if list.is_empty() => view! {
                    <div class="empty">
                        <h2 class="empty-title">"No furniture yet"</h2>
                        <p class="empty-text">
                            "Make one folder per piece of furniture, for example "
                            <code>"assets/sofa_grey"</code>
                            ", and put photos in it. Add an "
                            <code>"asset.yaml"</code>
                            " with name, category and the size in centimetres (width_cm, depth_cm, height_cm)."
                        </p>
                        <button class="btn btn-primary" on:click=move |_| reveal("assets")>"Open catalog folder"</button>
                    </div>
                }.into_any(),
                Some(list) => view! {
                    <ul class="asset-grid">{list.into_iter().map(asset_card).collect_view()}</ul>
                    <p class="screen-note">"Placing furniture in a scan comes next."</p>
                }.into_any(),
            }}
        </Screen>
    }
}

fn asset_card(asset: Asset) -> impl IntoView {
    let photo = asset.images.first().map(|path| TauriBridge::file_url(path));
    let size_known = asset.size.is_some();
    let size = size_text(&asset);
    let photos = count_of(asset.images.len(), "photo", "photos");
    let category = (!asset.category.is_empty()).then(|| capitalised(&asset.category));
    view! {
        <li class="asset-card">
            <div class="asset-photo">
                {match photo {
                    Some(url) => view! { <img src=url alt=asset.name.clone() loading="lazy" /> }.into_any(),
                    None => view! { <span class="asset-nophoto">"No photo yet"</span> }.into_any(),
                }}
            </div>
            <div class="asset-info">
                <h2 class="asset-name">{asset.name.clone()}</h2>
                <p class="asset-size" class:is-missing=!size_known>{size}</p>
                <p class="asset-meta">
                    {category.map(|text| format!("{text}, ")).unwrap_or_default()}
                    {photos}
                </p>
            </div>
        </li>
    }
}
