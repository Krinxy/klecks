// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use leptos::prelude::*;

/// The sheet behind a card: a field of soft sage "splats". `mood` adds a CSS class
/// (`is-hover`, `is-accepted`, `is-rejected`, `is-busy`) so the field reacts to what is happening.
#[component]
pub fn Stage(#[prop(into)] mood: Signal<&'static str>, children: Children) -> impl IntoView {
    view! {
        <section class=move || format!("stage {}", mood.get())>
            <div class="splatfield" aria-hidden="true">
                <span class="splat s1"></span>
                <span class="splat s2"></span>
                <span class="splat s3"></span>
                <span class="splat s4"></span>
                <span class="splat s5"></span>
                <span class="splat s6"></span>
            </div>
            {children()}
        </section>
    }
}
