// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

use crate::model::AppStatus;

/// Bottom strip: GPU guard result, quality preset, config problems.
#[component]
pub fn StatusBar(status: RwSignal<Option<AppStatus>>) -> impl IntoView {
    view! {
        <footer class="statusbar">
            {move || match status.get() {
                None => view! { <span class="chip chip-dim">"checking GPU…"</span> }.into_any(),
                Some(s) => {
                    let gpu_chip = match (&s.gpu, &s.gpu_error) {
                        (Some(gpu), _) => view! {
                            <span class="chip chip-ok" title=format!("driver {}", gpu.driver)>
                                <i class="dot"></i>
                                {format!("{} · {} · {}", gpu.name, gpu.backend, gpu.vendor)}
                            </span>
                        }
                        .into_any(),
                        (None, error) => view! {
                            <span class="chip chip-bad">
                                <i class="dot"></i>
                                {error.clone().unwrap_or_else(|| "no GPU".into())}
                            </span>
                        }
                        .into_any(),
                    };
                    let preset = s.preset.clone().map(|p| view! {
                        <span class="chip">{format!("preset {p}")}</span>
                    });
                    let config = s.config_error.clone().map(|e| view! {
                        <span class="chip chip-bad">{e}</span>
                    });
                    view! { {gpu_chip} {preset} {config} }.into_any()
                }
            }}
            <span class="spacer"></span>
            <span class="chip chip-dim">"offline"</span>
        </footer>
    }
}
