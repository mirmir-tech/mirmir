use leptos::prelude::*;

#[cfg(not(feature = "capture"))]
use crate::connection;
use crate::{
    cleanup,
    components::{Header, Navigation, Toasts, TooltipSurface},
    pages::{ChatPage, DashboardPage, ModelsPage, SettingsPage},
    state::{Page, RuntimeState},
};

#[component]
pub fn App() -> impl IntoView {
    let state = RuntimeState::new();
    provide_context(state);
    cleanup::remove_legacy_service_workers();
    #[cfg(feature = "capture")]
    crate::demo::populate(state);
    #[cfg(not(feature = "capture"))]
    connection::connect(state);

    view! {
        <TooltipSurface>
            <div class="app-shell">
                <Navigation />
                <div class="workspace">
                    <Header />
                    <main class="content">
                        <Show when=move || state.page.get() == Page::Overview><DashboardPage /></Show>
                        <Show when=move || state.page.get() == Page::Models><ModelsPage /></Show>
                        <Show when=move || state.page.get() == Page::Chat><ChatPage /></Show>
                        <Show when=move || state.page.get() == Page::Configuration><SettingsPage /></Show>
                    </main>
                </div>
            </div>
            <Toasts />
        </TooltipSurface>
    }
}
