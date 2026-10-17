// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::bridge::TauriBridge;

fn run_window_command(command: &'static str) {
    leptos::task::spawn_local(async move {
        if let Err(error) = TauriBridge::fire(command).await {
            leptos::logging::error!("{command} failed: {error}");
        }
    });
}

/// Frameless window chrome: the whole bar drags the window (Tauri's drag region),
/// the three buttons are ours instead of the OS ones.
#[component]
pub fn TitleBar(#[prop(into)] version: Signal<String>) -> impl IntoView {
    view! {
        <header class="titlebar" data-tauri-drag-region>
            <div class="brand" data-tauri-drag-region>
                <svg class="brand-mark" viewBox="0 0 24 24" aria-hidden="true">
                    <circle cx="12" cy="13" r="6.5" />
                    <circle cx="6.5" cy="8" r="2.6" />
                    <circle cx="18" cy="7" r="2" />
                    <circle cx="17.5" cy="18.5" r="1.6" />
                </svg>
                <span class="brand-name" data-tauri-drag-region>"klecks"</span>
                <span class="brand-version" data-tauri-drag-region>{move || version.get()}</span>
            </div>
            <div class="window-controls">
                <button class="wc" title="Minimize" on:click=move |_| run_window_command("window_minimize")>
                    <svg viewBox="0 0 12 12"><path d="M2 6h8" /></svg>
                </button>
                <button class="wc" title="Maximize" on:click=move |_| run_window_command("window_toggle_maximize")>
                    <svg viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" /></svg>
                </button>
                <button class="wc wc-close" title="Close" on:click=move |_| run_window_command("window_close")>
                    <svg viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" /></svg>
                </button>
            </div>
        </header>
    }
}
