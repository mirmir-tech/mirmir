use leptos::prelude::*;

use crate::state::{Page, RuntimeState};

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <aside class="sidebar">
            <a class="brand" href="/ui/" aria-label="MiRMiR runtime dashboard"><img src="/ui/assets/brand/lockup.svg" alt="MiRMiR" /></a>
            <p class="rail-label">"Workspace"</p>
            <nav class="workspace-nav" aria-label="Dashboard views">
                <NavButton page=Page::Overview label="Dashboard" />
                <NavButton page=Page::Models label="Models" />
                <NavButton page=Page::Chat label="Chat" />
                <NavButton page=Page::Configuration label="Settings" />
            </nav>
        </aside>
    }
}

#[component]
fn NavButton(page: Page, label: &'static str) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! {
        <button
            class="tab"
            class:active=move || state.page.get() == page
            aria-current=move || (state.page.get() == page).then_some("page")
            on:click=move |_| state.page.set(page)
        >
            <span class="nav-dot"></span><span>{label}</span>
        </button>
    }
}
