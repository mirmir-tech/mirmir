use leptos::prelude::*;

use crate::state::RuntimeState;

#[component]
pub fn Header() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let active_model = move || {
        state
            .models
            .get()
            .into_iter()
            .find(|model| model.state == "active")
            .map(|model| model.id)
    };
    view! {
        <header class="topbar">
            <h1>{move || state.page.get().label()}</h1>
            <span class="runtime-stage" data-active=move || state.generating().to_string()>
                {move || if state.generating() { "Generating" } else { "Idle" }}
            </span>
            <div class="topbar-context">
                <Show when=move || active_model().is_some()>
                    <span class="context-chip">
                        <svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true"><use href="/ui/assets/icons.svg#models" /></svg>
                        {move || active_model().unwrap_or_default()}
                    </span>
                </Show>
                <span class="context-device">{move || state.device_caption()}</span>
            </div>
            <div class="topbar-meta">
                <span
                    class="connection-state"
                    data-state=move || state.connection.get()
                >
                    {move || state.connection.get().to_uppercase()}
                </span>
            </div>
        </header>
    }
}
