// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-11-01
// ---
use leptos::prelude::*;
use wasm_bindgen::JsValue;

use crate::bridge::TauriBridge;
use crate::components::screen::Screen;
use crate::flow::ScanFlow;
use crate::format::Text;
use crate::model::{AppStatus, FolderView, PresetView, SettingsView};

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Quality,
    Folders,
    System,
    About,
}

const SECTIONS: [(Section, &str); 4] = [
    (Section::Quality, "Quality"),
    (Section::Folders, "Folders"),
    (Section::System, "System"),
    (Section::About, "About"),
];

/// What the last save said: Ok(text) or Err(text).
type Notice = RwSignal<Option<Result<String, String>>>;

/// Saves a setting through the backend and shows the result.
fn save(view: RwSignal<Option<SettingsView>>, notice: Notice, command: &'static str, key: String, value: String) {
    leptos::task::spawn_local(async move {
        let args = TauriBridge::map(&[("key", &key), ("value", &value)]);
        match TauriBridge::call::<SettingsView>(command, args).await {
            Ok(updated) => {
                view.set(Some(updated));
                notice.set(Some(Ok("Saved. It applies to the next scan.".into())));
            }
            Err(message) => notice.set(Some(Err(message))),
        }
    });
}

fn reveal(kind: String) {
    leptos::task::spawn_local(async move {
        let _ = TauriBridge::fire_with("reveal_folder", TauriBridge::map(&[("kind", &kind)])).await;
    });
}

/// Settings in the manner of a game: sections on the left, the choices of one section on the right.
#[component]
pub fn SettingsScreen(flow: ScanFlow, status: RwSignal<Option<AppStatus>>) -> impl IntoView {
    let settings = RwSignal::new(None::<SettingsView>);
    let section = RwSignal::new(Section::Quality);
    let notice: Notice = RwSignal::new(None);

    leptos::task::spawn_local(async move {
        match TauriBridge::call::<SettingsView>("get_settings", JsValue::UNDEFINED).await {
            Ok(loaded) => settings.set(Some(loaded)),
            Err(message) => notice.set(Some(Err(message))),
        }
    });

    view! {
        <Screen title="Settings" flow=flow>
            <div class="settings">
                <ul class="settings-nav" role="tablist">
                    {SECTIONS
                        .iter()
                        .map(|(this, label)| {
                            let this = *this;
                            view! {
                                <li>
                                    <button
                                        class="slab slab-sm"
                                        role="tab"
                                        class:is-selected=move || section.get() == this
                                        on:click=move |_| { section.set(this); notice.set(None); }
                                    >
                                        <span><span class="slab-label">{*label}</span></span>
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
                <div class="plate settings-pane">
                    {move || match settings.get() {
                        None => view! { <p class="screen-note">"Loading…"</p> }.into_any(),
                        Some(loaded) => match section.get() {
                            Section::Quality => quality(loaded, settings, notice).into_any(),
                            Section::Folders => folders(loaded, settings, notice).into_any(),
                            Section::System => system(loaded, status).into_any(),
                            Section::About => about().into_any(),
                        },
                    }}
                    {move || notice.get().map(|result| match result {
                        Ok(text) => view! { <p class="notice notice-ok">{text}</p> }.into_any(),
                        Err(text) => view! { <p class="notice notice-bad">{text}</p> }.into_any(),
                    })}
                </div>
            </div>
        </Screen>
    }
}

fn quality(loaded: SettingsView, settings: RwSignal<Option<SettingsView>>, notice: Notice) -> impl IntoView {
    let chosen = loaded.preset.clone();
    view! {
        <h2 class="pane-title">"Quality"</h2>
        <p class="pane-text">"A longer training makes the scene sharper where the videos looked. It does not add what was never filmed."</p>
        <ul class="tiles">
            {loaded.presets.into_iter().map(|preset: PresetView| {
                let id = preset.id.clone();
                let selected = preset.id == chosen;
                view! {
                    <li>
                        <button
                            class="tile"
                            class:is-selected=selected
                            on:click=move |_| save(settings, notice, "set_setting", "pipeline.preset".into(), id.clone())
                        >
                            <span class="tile-name">{Text::capitalised(&preset.id)}</span>
                            <span class="tile-big">{format!("{} min", preset.minutes)}</span>
                            <span class="tile-detail">{format!("up to {} splats", Text::millions(preset.max_splats))}</span>
                            <span class="tile-detail">{format!("{} training steps", preset.train_iterations)}</span>
                        </button>
                    </li>
                }
            }).collect_view()}
        </ul>
        <p class="pane-text pane-faint">
            {if loaded.upscale_model.is_empty() {
                "Sharpening the training images with an upscaling model is off. On the test videos it made no measurable difference."
            } else {
                "Training images are sharpened with an upscaling model."
            }}
        </p>
    }
}

fn folders(loaded: SettingsView, settings: RwSignal<Option<SettingsView>>, notice: Notice) -> impl IntoView {
    view! {
        <h2 class="pane-title">"Folders"</h2>
        <p class="pane-text">"Rooms and furniture live in the media library. Scans and models can be large, so they can go on another drive."</p>
        <ul class="folder-list">
            {loaded.folders.into_iter().map(|folder| folder_row(folder, settings, notice)).collect_view()}
        </ul>
    }
}

fn folder_row(folder: FolderView, settings: RwSignal<Option<SettingsView>>, notice: Notice) -> impl IntoView {
    let text = RwSignal::new(folder.path.clone());
    let kind = folder.kind.clone();
    view! {
        <li class="folder-row">
            <label class="folder-label">{folder.label.clone()}</label>
            {match folder.setting {
                Some(key) => {
                    let save_key = key.clone();
                    view! {
                        <div class="folder-edit">
                            <input
                                class="input"
                                type="text"
                                spellcheck="false"
                                prop:value=move || text.get()
                                on:input=move |event| text.set(event_target_value(&event))
                            />
                            <button class="btn btn-ghost" on:click=move |_| save(settings, notice, "set_setting", save_key.clone(), text.get_untracked())>"Save"</button>
                            <button class="btn btn-ghost" title="Use the default folder" on:click=move |_| save(settings, notice, "reset_setting", key.clone(), String::new())>"Default"</button>
                        </div>
                    }.into_any()
                }
                None => view! { <p class="folder-fixed">{folder.path.clone()}</p> }.into_any(),
            }}
            <button class="btn btn-ghost" on:click=move |_| reveal(kind.clone())>"Open"</button>
        </li>
    }
}

fn system(loaded: SettingsView, status: RwSignal<Option<AppStatus>>) -> impl IntoView {
    let gpu = move || match status.get() {
        None => "Checking…".to_string(),
        Some(s) => match (s.gpu, s.gpu_error) {
            (Some(gpu), _) => format!("{} ({}, {}, driver {})", gpu.name, gpu.vendor, gpu.backend, gpu.driver),
            (None, error) => error.unwrap_or_else(|| "No GPU found".into()),
        },
    };
    let reconstruction = match &loaded.reconstruct_error {
        None => "Ready".to_string(),
        Some(problem) => problem.clone(),
    };
    let trainer = if loaded.brush.is_empty() {
        "Not set: scans give a point cloud instead of a splat".to_string()
    } else {
        loaded.brush.clone()
    };
    view! {
        <h2 class="pane-title">"System"</h2>
        <dl class="facts">
            <dt>"Graphics card"</dt><dd>{gpu}</dd>
            <dt>"Reconstruction"</dt><dd class:is-bad=loaded.reconstruct_error.is_some()>{reconstruction}</dd>
            <dt>"Splat trainer"</dt><dd>{trainer}</dd>
            <dt>"Quality"</dt><dd>{Text::capitalised(&loaded.preset)}</dd>
        </dl>
        <p class="pane-text pane-faint">"klecks never calculates on the processor: without an allowed graphics card a scan does not start."</p>
    }
}

fn about() -> impl IntoView {
    view! {
        <h2 class="pane-title">"About"</h2>
        <p class="pane-text">"klecks rebuilds a room from videos of it, entirely on your computer."</p>
        <dl class="facts">
            <dt>"Cameras and depth"</dt><dd>"VGGT, non-commercial licence"</dd>
            <dt>"Splat training"</dt><dd>"Brush, Apache-2.0"</dd>
            <dt>"3D view"</dt><dd>"three.js and Spark, MIT"</dd>
        </dl>
    }
}
