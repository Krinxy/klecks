// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::components::stage::Stage;
use crate::flow::ScanFlow;

/// (stage key sent by the backend, what the person reads)
const STEPS: [(&str, &str); 6] = [
    ("frames", "Preparing frames"),
    ("model", "Loading the model"),
    ("inference", "Estimating cameras and depth"),
    ("detail", "Sharpening the details"),
    ("cloud", "Preparing the training"),
    ("train", "Training the 3D scene"),
];

/// Shown while a scan runs: the steps, a bar, and a way out.
#[component]
pub fn ScanProgress(name: String, flow: ScanFlow) -> impl IntoView {
    // "done" is past the last step so every step shows as finished.
    let current = move || {
        flow.progress
            .get()
            .map(|event| {
                STEPS
                    .iter()
                    .position(|(key, _)| *key == event.stage)
                    .unwrap_or(STEPS.len())
            })
            .unwrap_or(0)
    };
    let percent = move || flow.progress.get().map(|event| event.pct).unwrap_or(0);

    view! {
        <Stage mood="is-busy">
            <div class="card">
                <h1 class="card-title">"Reconstructing your room"</h1>
                <p class="card-text">{name}</p>
                <ol class="steps">
                    {STEPS
                        .iter()
                        .enumerate()
                        .map(|(index, (_, label))| view! {
                            <li class="step" class:is-done=move || index < current() class:is-current=move || index == current()>
                                <i class="step-dot"></i>
                                <span>{*label}</span>
                            </li>
                        })
                        .collect_view()}
                </ol>
                <div class="bar"><span style=move || format!("width: {}%", percent())></span></div>
                <p class="card-note">
                    {move || flow.progress.get().map(|event| event.message).unwrap_or_else(|| "Starting…".into())}
                </p>
                <div class="card-actions">
                    <button class="btn btn-ghost" on:click=move |_| flow.cancel()>"Cancel"</button>
                </div>
            </div>
        </Stage>
    }
}
