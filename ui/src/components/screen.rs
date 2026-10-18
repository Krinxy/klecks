// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;

use crate::components::stage::Stage;
use crate::flow::ScanFlow;
use crate::model::View;

/// Top-left way back: to the start menu, or to another screen.
#[component]
pub fn BackButton(flow: ScanFlow, #[prop(optional, into)] to: Option<View>, #[prop(default = "Menu")] label: &'static str) -> impl IntoView {
    let target = to.unwrap_or(View::Menu);
    view! {
        <button class="back" title=format!("Back to {} (Esc)", label.to_lowercase()) on:click=move |_| flow.go(target.clone())>
            <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M7.5 2L3.5 6l4 4" /></svg>
            {label}
        </button>
    }
}

/// Frame of a menu screen: the backdrop, a back button, a title with its khaki bar, and room for actions
/// on the right of the title. What goes inside is up to the screen.
#[component]
pub fn Screen(
    title: &'static str,
    flow: ScanFlow,
    #[prop(optional, into)] actions: ViewFn,
    #[prop(optional)] subtitle: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <Stage mood="">
            <BackButton flow=flow />
            <div class="page">
                <header class="page-head">
                    <div>
                        <h1 class="page-title">{title}</h1>
                        <span class="page-bar"></span>
                        {subtitle.map(|text| view! { <p class="page-sub">{text}</p> })}
                    </div>
                    <div class="page-actions">{actions.run()}</div>
                </header>
                <div class="page-body">{children()}</div>
            </div>
        </Stage>
    }
}
