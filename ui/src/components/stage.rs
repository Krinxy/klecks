// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;

/// The sheet behind a screen: torn shapes in black, khaki and paper, the same language as the start
/// menu but quiet. `mood` adds a CSS class (`is-hover`, `is-accepted`, `is-rejected`, `is-busy`) so the
/// shapes answer what is happening. They are plain boxes: nothing animates, the GPU stays free for scans.
#[component]
pub fn Stage(#[prop(into)] mood: Signal<&'static str>, children: Children) -> impl IntoView {
    view! {
        <section class=move || format!("stage {}", mood.get())>
            <div class="backdrop" aria-hidden="true">
                <span class="bd bd-dark"></span>
                <span class="bd bd-khaki"></span>
                <span class="bd bd-paper"></span>
            </div>
            {children()}
        </section>
    }
}
