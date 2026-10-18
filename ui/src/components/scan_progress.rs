// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;

use crate::components::stage::Stage;
use crate::flow::ScanFlow;

/// (stage key sent by the backend, what the person reads)
const STEPS: [(&str, &str); 5] = [
    ("frames", "Preparing frames"),
    ("model", "Loading the model"),
    ("inference", "Reconstructing the room"),
    ("cloud", "Preparing the training"),
    ("train", "Training the 3D scene"),
];

/// Shown while a scan runs: the steps as slabs on the left, the percentage on the right, and a way out.
#[component]
pub fn ScanProgress(name: String, flow: ScanFlow) -> impl IntoView {
    // "done" is past the last step so every step shows as finished.
    let current = move || {
        flow.progress
            .get()
            .map(|event| STEPS.iter().position(|(key, _)| *key == event.stage).unwrap_or(STEPS.len()))
            .unwrap_or(0)
    };
    let percent = move || flow.progress.get().map(|event| event.pct).unwrap_or(0);

    view! {
        <Stage mood="is-busy">
            <div class="progress">
                <ol class="steps">
                    {STEPS
                        .iter()
                        .enumerate()
                        .map(|(index, (_, label))| view! {
                            <li class="slab slab-step" class:is-done=move || index < current() class:is-selected=move || index == current()>
                                <span>
                                    <span class="slab-label">{*label}</span>
                                    <span class="slab-detail">{move || if index < current() { "done" } else { "" }}</span>
                                </span>
                            </li>
                        })
                        .collect_view()}
                </ol>
                <div class="plate progress-side">
                    <p class="progress-room">{name}</p>
                    <p class="progress-number">{move || format!("{}", percent())}<span>"%"</span></p>
                    <div class="bar"><span style=move || format!("width: {}%", percent())></span></div>
                    <p class="progress-note">
                        {move || flow.progress.get().map(|event| event.message).unwrap_or_else(|| "Starting…".into())}
                    </p>
                    <button class="btn btn-ghost" on:click=move |_| flow.cancel()>"Cancel"</button>
                </div>
            </div>
        </Stage>
    }
}
