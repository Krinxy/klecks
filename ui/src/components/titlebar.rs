// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::bridge::TauriBridge;
use crate::flow::ScanFlow;
use crate::model::View;

/// Where the title bar lets you jump to, from every screen.
const PLACES: [&str; 5] = ["Menu", "Rooms", "Catalog", "New scan", "Settings"];

fn place_view(index: usize) -> View {
    match index {
        0 => View::Menu,
        1 => View::Rooms,
        2 => View::Catalog,
        3 => View::NewScan,
        _ => View::Settings,
    }
}

fn current_place(view: &View) -> Option<usize> {
    match view {
        View::Menu => Some(0),
        View::Rooms | View::Room(_) => Some(1),
        View::Catalog => Some(2),
        View::NewScan => Some(3),
        View::Settings => Some(4),
        _ => None,
    }
}

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
pub fn TitleBar(#[prop(into)] version: Signal<String>, flow: ScanFlow) -> impl IntoView {
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
            {move || {
                // Not on the start menu itself and not while a scan runs: there is nothing to jump away from or the job owns the screen.
                let view = flow.view.get();
                (!matches!(view, View::Menu | View::Scanning(_))).then(|| {
                    let here = current_place(&view);
                    view! {
                        <nav class="topnav" aria-label="Go to">
                            {PLACES.iter().enumerate().map(|(index, label)| view! {
                                <button class="topnav-item" class:is-current=here == Some(index) on:click=move |_| flow.go(place_view(index))>{*label}</button>
                            }).collect_view()}
                        </nav>
                    }
                })
            }}
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
