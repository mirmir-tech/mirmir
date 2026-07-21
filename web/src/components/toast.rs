use leptos::prelude::*;

use crate::state::RuntimeState;

#[component]
pub fn Toasts() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! {
        <div class="toast-region" aria-live="polite">
            <For
                each=move || state.toasts.get()
                key=|toast| toast.id
                children=move |toast| {
                    let id = toast.id;
                    view! {
                        <div class="toast" class:error=toast.error role=if toast.error { "alert" } else { "status" }>
                            <span>{toast.message}</span>
                            <button type="button" aria-label="Dismiss notification" on:click=move |_| state.dismiss(id)>"×"</button>
                        </div>
                    }
                }
            />
        </div>
    }
}
