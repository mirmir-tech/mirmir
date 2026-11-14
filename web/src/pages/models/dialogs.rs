use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::{
    ModelUi,
    context::{ContextControl, chosen_tokens},
    memory::MemoryNote,
    row::run_action,
};
use crate::{
    api,
    components::{
        RangeParameter,
        parameter::{
            Range,
            range::{
                REPETITION_PENALTY, TEMPERATURE, TOP_K, TOP_P, max_tokens_limit, single, whole,
            },
        },
    },
    state::{RuntimeState, bytes},
    types::catalog::{GenerationSettings, Removed},
};

#[component]
pub fn LoadDialog() -> impl IntoView {
    let ui = expect_context::<ModelUi>();
    let state = expect_context::<RuntimeState>();
    let settings = ui.settings;
    let read = move |field: fn(&GenerationSettings) -> f64| {
        Signal::derive(move || Some(settings.with(field)))
    };
    let write = move |update: fn(&mut GenerationSettings, f64)| {
        move |value: Option<f64>| {
            if let Some(value) = value {
                settings.update(|item| update(item, value));
            }
        }
    };
    let token_range = Signal::derive(move || Range::max_tokens(token_limit(ui)));
    view! { <Show when=move || ui.load_target.get().is_some()>
        <dialog class="modal load-modal" open>
            <form on:submit=move |event| { event.prevent_default(); submit_load(state, ui); }>
                <p class="eyebrow">{move || ui.inspection.get().map_or("Inspecting model", |item| if item.task == "generation" { "Generation defaults" } else { "Model settings" })}</p>
                <h2>"Load model"</h2>
                <p class="modal-target">{move || ui.load_target.get().map(|model| model.selector).unwrap_or_default()}</p>
                <MemoryNote />
                <ContextControl />
                <Show when=move || generation(ui)>
                    <div class="generation-controls">
                        <RangeParameter label="Max tokens" range=token_range value=read(|item| item.max_tokens as f64) on_change=write(|item, value| item.max_tokens = whole(value)) />
                        <RangeParameter label="Temperature" range=TEMPERATURE value=read(|item| f64::from(item.temperature)) on_change=write(|item, value| item.temperature = single(value)) />
                        <RangeParameter label="Top P" range=TOP_P value=read(|item| f64::from(item.top_p)) on_change=write(|item, value| item.top_p = single(value)) />
                        <RangeParameter label="Top K" range=TOP_K value=read(|item| item.top_k as f64) on_change=write(|item, value| item.top_k = whole(value)) />
                        <RangeParameter label="Repetition penalty" range=REPETITION_PENALTY value=read(|item| f64::from(item.repetition_penalty)) on_change=write(|item, value| item.repetition_penalty = single(value)) />
                    </div>
                </Show>
                <label class="check"><input id="load-force" type="checkbox" />"Allow load despite memory-fit rejection"</label>
                <div class="dialog-actions"><button class="quiet" type="button" on:click=move |_| ui.load_target.set(None)>"Cancel"</button><button type="submit" disabled=move || ui.inspection.get().is_none()>"Load model"</button></div>
            </form>
        </dialog>
    </Show> }
}

#[component]
pub fn RemoveDialog() -> impl IntoView {
    let ui = expect_context::<ModelUi>();
    let state = expect_context::<RuntimeState>();
    view! { <Show when=move || ui.remove_target.get().is_some()><dialog class="modal confirm-modal" open><form on:submit=move |event| { event.prevent_default(); remove(state, ui); }><p class="eyebrow">"Destructive action"</p><h2>"Remove model?"</h2><p class="modal-target">{move || ui.remove_target.get().unwrap_or_default()}</p><p class="modal-note">"The local Hugging Face cache and MiRMiR model record will be removed."</p><div class="dialog-actions"><button class="quiet" type="button" on:click=move |_| ui.remove_target.set(None)>"Keep model"</button><button class="danger" type="submit">"Remove model"</button></div></form></dialog></Show> }
}

fn submit_load(state: RuntimeState, ui: ModelUi) {
    let Some(model) = ui.load_target.get_untracked() else {
        return;
    };
    let mut settings = ui.settings.get_untracked();
    settings.max_tokens =
        whole(Range::max_tokens(token_limit(ui)).clamp(settings.max_tokens as f64));
    let context = ui.inspection.with_untracked(|item| {
        item.as_ref()
            .and_then(|item| item.context.as_ref())
            .map(|_| ui.context.get_untracked())
    });
    let force = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("load-force"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
        .is_some_and(|input| input.checked());
    let body = json!({ "selector": model.selector, "settings": generation(ui).then_some(settings), "config_id": model.id, "repo_id": model.repo_id, "revision": model.revision, "commit": model.commit, "force": force, "context": context });
    run_action(state, model.selector, "/models/load", body);
    ui.load_target.set(None);
}

fn remove(state: RuntimeState, ui: ModelUi) {
    let Some(repo_id) = ui.remove_target.get_untracked() else {
        return;
    };
    ui.remove_target.set(None);
    state.set_busy(&repo_id, Some("remove"));
    spawn_local(async move {
        match api::post::<Removed, _>(state, "/models/remove", &json!({"repo_id": repo_id})).await {
            Ok(result) => {
                if result.removed {
                    state.models.update(|items| items.retain(|item| item.repo_id != repo_id));
                }
                state.notify(
                    format!("Removed {repo_id}; freed {}", bytes(result.freed_bytes)),
                    false,
                );
            },
            Err(error) => state.notify(error, true),
        }
        state.set_busy(&repo_id, None);
    });
}

fn generation(ui: ModelUi) -> bool {
    ui.inspection.with(|item| {
        item.as_ref()
            .is_some_and(|item| item.task == "generation" || item.settings.is_some())
    })
}

/// Max tokens stay within the chosen context and the model's input limit.
fn token_limit(ui: ModelUi) -> u64 {
    let context = chosen_tokens(ui);
    ui.inspection.with(|item| {
        item.as_ref().map_or_else(
            || max_tokens_limit(None, None),
            |item| {
                let max_input =
                    item.capabilities.as_ref().map(|capabilities| capabilities.max_input_tokens);
                max_tokens_limit(context.or(item.memory.max_safe_context_tokens), max_input)
            },
        )
    })
}
