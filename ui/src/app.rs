// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::{dropzone::DropZone, statusbar::StatusBar, titlebar::TitleBar};
use crate::model::AppStatus;

#[component]
pub fn App() -> impl IntoView {
    let status = RwSignal::new(None::<AppStatus>);

    leptos::task::spawn_local(async move {
        match TauriBridge::call::<AppStatus>("app_status", JsValue::UNDEFINED).await {
            Ok(loaded) => status.set(Some(loaded)),
            Err(error) => leptos::logging::error!("app_status failed: {error}"),
        }
    });

    let version = Signal::derive(move || {
        status
            .get()
            .map(|s| format!("v{}", s.version))
            .unwrap_or_default()
    });
    let gpu_ready = Signal::derive(move || status.get().is_some_and(|s| s.gpu.is_some()));

    view! {
        <div class="shell">
            <TitleBar version=version />
            <main class="canvas">
                <DropZone gpu_ready=gpu_ready />
            </main>
            <StatusBar status=status />
        </div>
    }
}
