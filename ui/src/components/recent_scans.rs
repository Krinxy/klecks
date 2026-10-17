// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::flow::ScanFlow;

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "IMG_7695-1792234171" -> "IMG_7695" (the folder name carries a unix time suffix).
fn scan_label(folder: &str) -> String {
    match folder.rsplit_once('-') {
        Some((name, suffix)) if !name.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) => name.to_string(),
        _ => folder.to_string(),
    }
}

/// "17 Oct, 10:25" in the user's local time.
fn when(modified_ms: u64) -> String {
    let date = js_sys::Date::new(&JsValue::from_f64(modified_ms as f64));
    format!(
        "{} {}, {:02}:{:02}",
        date.get_date(),
        MONTHS[date.get_month() as usize % 12],
        date.get_hours(),
        date.get_minutes()
    )
}

/// The latest finished scans, so a scan survives closing the app.
#[component]
pub fn RecentScans(flow: ScanFlow) -> impl IntoView {
    flow.refresh_recent();
    view! {
        <Show when=move || !flow.recent.get().is_empty()>
            <div class="recent">
                <p class="recent-title">"Earlier scans"</p>
                <ul class="recent-list">
                    <For
                        each=move || flow.recent.get()
                        key=|scan| scan.name.clone()
                        children=move |scan| {
                            let result = scan.result.clone();
                            view! {
                                <li>
                                    <button class="recent-row" on:click=move |_| flow.show(result.clone())>
                                        <span>{scan_label(&scan.name)}</span>
                                        <span class="recent-meta">{when(scan.modified_ms)}</span>
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
            </div>
        </Show>
    }
}
