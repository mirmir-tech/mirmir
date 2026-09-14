use leptos::prelude::*;

use crate::state::{Page, RuntimeState};

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <aside class="sidebar">
            <a class="brand" href="/ui/" aria-label="MiRMiR runtime dashboard"><img class="logo-dark" src="/ui/assets/brand/lockup.svg" alt="MiRMiR" /><img class="logo-light" src="/ui/assets/brand/lockup-on-light.svg" alt="MiRMiR" /></a>
            <p class="rail-label">"Workspace"</p>
            <nav class="workspace-nav" aria-label="Dashboard views">
                <NavButton page=Page::Overview label="Dashboard" />
                <NavButton page=Page::Models label="Models" />
                <NavButton page=Page::Chat label="Chat" />
                <NavButton page=Page::Configuration label="Settings" />
            </nav><div class="appearance"><button class="appearance-toggle" type="button" data-appearance="true" aria-label="Switch theme"><svg class="theme-sun" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4" /><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5" /></svg><svg class="theme-moon" viewBox="0 0 24 24" aria-hidden="true"><path d="M20.5 14A8.5 8.5 0 0 1 10 3.5 8.5 8.5 0 1 0 20.5 14Z" /></svg></button></div>
        </aside>
    }
}

#[component]
fn NavButton(page: Page, label: &'static str) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let icon = match page {
        Page::Overview => "monitoring",
        Page::Models => "models",
        Page::Chat => "reasoning",
        Page::Configuration => "settings",
    };
    view! {
        <button
            class="tab"
            class:active=move || state.page.get() == page
            aria-current=move || (state.page.get() == page).then_some("page")
            on:click=move |_| state.page.set(page)
        >
            <svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true"><use href=format!("/ui/assets/icons.svg#{icon}") /></svg><span>{label}</span>
        </button>
    }
}
