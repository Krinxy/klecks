// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-18
// ---
use leptos::prelude::*;

use crate::components::stage::Stage;
use crate::flow::ScanFlow;

/// Top-left way back to the start menu.
#[component]
pub fn BackButton(flow: ScanFlow) -> impl IntoView {
    view! {
        <button class="back" title="Back to the menu (Esc)" on:click=move |_| flow.home()>
            <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M7.5 2L3.5 6l4 4" /></svg>
            "Menu"
        </button>
    }
}

/// Frame of a menu screen: the splat field behind, a back button, a title and room for actions
/// on the right of the title.
#[component]
pub fn Screen(
    title: &'static str,
    flow: ScanFlow,
    #[prop(optional, into)] actions: ViewFn,
    children: Children,
) -> impl IntoView {
    view! {
        <Stage mood="">
            <BackButton flow=flow />
            <div class="screen">
                <header class="screen-head">
                    <h1 class="screen-title">{title}</h1>
                    <div class="screen-actions">{actions.run()}</div>
                </header>
                <div class="screen-body">{children()}</div>
            </div>
        </Stage>
    }
}
