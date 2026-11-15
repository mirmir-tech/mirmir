//! The load dialog's memory estimate for the chosen context.

use leptos::prelude::*;

use super::{ModelUi, context::scale::grouped};
use crate::{
    state::bytes,
    types::catalog::{Fit, Memory},
};

#[component]
pub fn MemoryNote() -> impl IntoView {
    let ui = expect_context::<ModelUi>();
    let memory = move || ui.inspection.with(|item| item.as_ref().map(|item| item.memory.clone()));
    let fit = move || memory().map_or(Fit::Unknown, |memory| memory.fit);
    view! {
        <Show when=move || ui.inspect_error.get().is_none() && memory().is_some() fallback=move || view! { <p class="modal-note">{move || ui.inspect_error.get().unwrap_or_else(|| "Inspecting model and memory…".to_owned())}</p> }>
            <div class="memory-note">
                <p class="modal-note"><span class=move || format!("state-pill {}", fit().class())>{move || fit().label()}</span>{move || memory().map(|memory| summary(&memory))}</p>
                <p class="modal-note">{move || memory().map(|memory| breakdown(&memory))}</p>
                <Show when=move || fit() == Fit::Exceeds>
                    <p class="modal-note fit-warning">{move || memory().map(|memory| warning(&memory))}</p>
                </Show>
            </div>
        </Show>
    }
}

fn summary(memory: &Memory) -> String {
    let limit = memory.budget_bytes.map_or_else(
        || memory.available_bytes.map(|value| format!(" of {} available", bytes(value))),
        |value| Some(format!(" of {} budget", bytes(value))),
    );
    format!(
        "{} required{} · {}",
        bytes(memory.required_bytes),
        limit.unwrap_or_default(),
        memory.memory_source
    )
}

fn breakdown(memory: &Memory) -> String {
    format!(
        "Weights {} · K/V {} · Workspace {}",
        bytes(memory.weight_bytes),
        bytes(memory.kv_cache_bytes),
        bytes(memory.workspace_bytes)
    )
}

fn warning(memory: &Memory) -> String {
    memory.max_safe_context_tokens.map_or_else(
        || "This context does not fit the memory budget.".to_owned(),
        |tokens| {
            format!(
                "This context does not fit the memory budget; at most {} tokens fit.",
                grouped(tokens)
            )
        },
    )
}
