use leptos::prelude::*;

use crate::state::RuntimeState;

#[component]
pub fn Header() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! {
        <header class="topbar">
            <h1>{move || state.page.get().label()}</h1>
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
