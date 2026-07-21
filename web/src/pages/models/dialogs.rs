use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use super::{ModelUi, row::run_action};
use crate::{
    api,
    state::{RuntimeState, bytes},
    types::{
        Model,
        catalog::{GenerationSettings, Inspection, Removed},
    },
};

pub fn inspect(model: Model) {
    let ui = expect_context::<ModelUi>();
    let state = expect_context::<RuntimeState>();
    ui.load_target.set(Some(model.clone()));
    ui.inspection.set(None);
    ui.inspect_error.set(None);
    spawn_local(async move {
        let path = format!("/models/inspect?selector={}", urlencoding::encode(&model.selector));
        match api::get::<Inspection>(&path).await {
            Ok(inspection) => ui.inspection.set(Some(inspection)),
            Err(error) => {
                ui.inspect_error.set(Some(error.clone()));
                state.notify(error, true);
            },
        }
    });
}

#[component]
pub fn LoadDialog() -> impl IntoView {
    let ui = expect_context::<ModelUi>();
    let state = expect_context::<RuntimeState>();
    let settings = RwSignal::new(GenerationSettings::default());
    Effect::new(move || {
        if let Some(value) = ui.inspection.get().and_then(|item| item.settings) {
            settings.set(value);
        }
    });
    view! { <Show when=move || ui.load_target.get().is_some()>
        <dialog class="modal load-modal" open>
            <form on:submit=move |event| { event.prevent_default(); submit_load(state, ui, settings.get_untracked()); }>
                <p class="eyebrow">{move || ui.inspection.get().map_or("Inspecting model", |item| if item.task == "generation" { "Generation defaults" } else { "Model settings" })}</p>
                <h2>"Load model"</h2>
                <p class="modal-target">{move || ui.load_target.get().map(|model| model.selector).unwrap_or_default()}</p>
                <p class="modal-note">{move || inspection_note(ui)}</p>
                <Show when=move || ui.inspection.get().is_some_and(|item| item.task == "generation" || item.settings.is_some())>
                    <div class="generation-controls">
                        <ParameterU64 label="Max tokens" value=RwSignal::new(settings.get_untracked().max_tokens) min=1 max=move || max_tokens(ui) on_change=move |value| settings.update(|item| item.max_tokens = value) />
                        <ParameterF32 label="Temperature" value=RwSignal::new(settings.get_untracked().temperature) min=0.0 max=2.0 step=0.01 on_change=move |value| settings.update(|item| item.temperature = value) />
                        <ParameterF32 label="Top P" value=RwSignal::new(settings.get_untracked().top_p) min=0.0 max=1.0 step=0.01 on_change=move |value| settings.update(|item| item.top_p = value) />
                        <ParameterU64 label="Top K" value=RwSignal::new(settings.get_untracked().top_k) min=0 max=move || 200 on_change=move |value| settings.update(|item| item.top_k = value) />
                        <ParameterF32 label="Repetition penalty" value=RwSignal::new(settings.get_untracked().repetition_penalty) min=0.01 max=2.0 step=0.01 on_change=move |value| settings.update(|item| item.repetition_penalty = value) />
                    </div>
                </Show>
                <label class="check"><input id="load-force" type="checkbox" />"Allow load despite memory-fit rejection"</label>
                <div class="dialog-actions"><button class="quiet" type="button" on:click=move |_| ui.load_target.set(None)>"Cancel"</button><button type="submit" disabled=move || ui.inspection.get().is_none()>"Load model"</button></div>
            </form>
        </dialog>
    </Show> }
}

#[component]
fn ParameterU64<F, M>(
    label: &'static str,
    value: RwSignal<u64>,
    min: u64,
    max: M,
    on_change: F,
) -> impl IntoView
where
    F: Fn(u64) + Copy + Send + Sync + 'static,
    M: Fn() -> u64 + Copy + Send + Sync + 'static,
{
    view! { <div class="parameter-control"><label>{label}<input type="number" min=min max=max prop:value=move || value.get() on:input=move |event| if let Ok(next) = event_target_value(&event).parse() { value.set(next); on_change(next); } /></label><input class="parameter-slider" type="range" min=min max=max prop:value=move || value.get() on:input=move |event| if let Ok(next) = event_target_value(&event).parse() { value.set(next); on_change(next); } /></div> }
}

#[component]
fn ParameterF32<F>(
    label: &'static str,
    value: RwSignal<f32>,
    min: f32,
    max: f32,
    step: f32,
    on_change: F,
) -> impl IntoView
where
    F: Fn(f32) + Copy + Send + Sync + 'static,
{
    view! { <div class="parameter-control"><label>{label}<input type="number" min=min max=max step=step prop:value=move || value.get() on:input=move |event| if let Ok(next) = event_target_value(&event).parse() { value.set(next); on_change(next); } /></label><input class="parameter-slider" type="range" min=min max=max step=step prop:value=move || value.get() on:input=move |event| if let Ok(next) = event_target_value(&event).parse() { value.set(next); on_change(next); } /></div> }
}

#[component]
pub fn RemoveDialog() -> impl IntoView {
    let ui = expect_context::<ModelUi>();
    let state = expect_context::<RuntimeState>();
    view! { <Show when=move || ui.remove_target.get().is_some()><dialog class="modal confirm-modal" open><form on:submit=move |event| { event.prevent_default(); remove(state, ui); }><p class="eyebrow">"Destructive action"</p><h2>"Remove model?"</h2><p class="modal-target">{move || ui.remove_target.get().unwrap_or_default()}</p><p class="modal-note">"The local Hugging Face cache and MiRMiR model record will be removed."</p><div class="dialog-actions"><button class="quiet" type="button" on:click=move |_| ui.remove_target.set(None)>"Keep model"</button><button class="danger" type="submit">"Remove model"</button></div></form></dialog></Show> }
}

fn submit_load(state: RuntimeState, ui: ModelUi, settings: GenerationSettings) {
    let Some(model) = ui.load_target.get_untracked() else {
        return;
    };
    let generation = ui
        .inspection
        .get_untracked()
        .is_some_and(|item| item.task == "generation" || item.settings.is_some());
    let force = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("load-force"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
        .is_some_and(|input| input.checked());
    let body = json!({ "selector": model.selector, "settings": generation.then_some(settings), "config_id": model.id, "repo_id": model.repo_id, "revision": model.revision, "commit": model.commit, "force": force });
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

fn inspection_note(ui: ModelUi) -> String {
    if let Some(error) = ui.inspect_error.get() {
        return error;
    }
    ui.inspection.get().map_or_else(
        || "Inspecting model and memory…".to_owned(),
        |item| {
            format!(
                "{} · {} required · {}",
                item.memory.fit,
                bytes(item.memory.required_bytes),
                item.memory.memory_source
            )
        },
    )
}

fn max_tokens(ui: ModelUi) -> u64 {
    ui.inspection.get().map_or(32_768, |item| {
        item.memory
            .max_safe_context_tokens
            .unwrap_or_else(|| item.memory.configured_cache_tokens.max(32_768))
    })
}

use wasm_bindgen::JsCast;
