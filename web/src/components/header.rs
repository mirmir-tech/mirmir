use leptos::prelude::*;

use crate::state::RuntimeState;

#[component]
pub fn Header() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! {
        <header class="topbar">
            <h1>{move || state.page.get().label()}</h1>
            <div class="topbar-meta">
                <span><b>"SERVER "</b>{move || state.server_version.get()}</span>
                <span><b>"PROTOCOL "</b>{move || state.protocol_version.get()}</span>
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
