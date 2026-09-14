use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use super::ModelUi;
use crate::{
    api,
    components::{FeaturePills, Icon, StatePill, TypePill},
    state::{Page, RuntimeState, bytes},
    types::{Activity, Model},
};

#[component]
pub fn LocalRow(model: Model) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let ui = expect_context::<ModelUi>();
    let format = format_label(&model);
    let format_detail = format!(
        "{} · {} · Metal {} · CUDA {}",
        model.ecosystem, model.container, model.metal_compatibility, model.cuda_compatibility
    );
    let id = StoredValue::new(model.id.clone());
    let selector = StoredValue::new(model.selector.clone());
    let repo_id = StoredValue::new(model.repo_id.clone());
    let stored_model = StoredValue::new(model.clone());
    let model_state = model.state.clone();
    let can_unload = model_state == "ready" || model_state == "active";
    let activity =
        move || activity_for(state, &id.get_value(), &selector.get_value(), &repo_id.get_value());
    let operation = move || activity().filter(live);
    let busy_operation =
        move || busy_for(state, &id.get_value(), &selector.get_value(), &repo_id.get_value());
    let display_state = move || {
        operation().as_ref().map_or_else(
            || {
                activity().filter(|item| item.state == "failed").map_or_else(
                    || {
                        busy_operation()
                            .as_deref()
                            .map_or_else(|| model_state.clone(), operation_state)
                    },
                    |_| "error".to_owned(),
                )
            },
            |item| operation_state(&item.kind),
        )
    };
    let detail = move || {
        activity().map_or_else(
            || {
                busy_operation().map_or_else(
                    || model.load_unavailable_reason.clone(),
                    |kind| format!("{} accepted", operation_state(&kind)),
                )
            },
            |item| item.detail,
        )
    };
    let progress = move || {
        operation().and_then(|item| {
            item.current.zip(item.total).map(|(a, b)| 100.0 * a as f64 / b.max(1) as f64)
        })
    };
    let busy = move || operation().is_some() || busy_operation().is_some();
    view! {
        <tr class:removing=move || state.busy.get().get(&repo_id.get_value()).is_some_and(|value| value == "remove")>
            <td><span class="model-name"><strong>{model.id.clone()}</strong><small>{model.repo_id.clone()}</small></span></td>
            <td class="model-format"><TypePill value=format /><span>{format_detail}</span></td>
            <td class="model-features"><FeaturePills tool_use=model.tool_use thinking=model.thinking vision=model.vision || model.image_input /></td>
            <td class="model-size">{bytes(model.size_bytes)}</td>
            <td><StatePill state=display_state detail=detail progress=progress /></td>
            <td class="model-actions"><div class="model-action-group">
                <Show when=move || can_unload><button class="open-chat" on:click=move |_| { state.chat_model.set(selector.get_value()); state.page.set(Page::Chat); }>"Open chat"</button></Show>
                <Show when=move || can_unload fallback=move || {
                    view! { <button class="icon-action" data-tooltip="Load model" aria-label="Load model" disabled=busy on:click=move |_| super::dialogs::inspect(stored_model.get_value())><Icon name="load" /><span>"Load"</span></button> }
                }>
                    <button class="icon-action" data-tooltip="Unload model" aria-label="Unload model" disabled=busy on:click=move |_| { let target = selector.get_value(); run_action(state, target.clone(), "/models/unload", json!({"selector": target})); }><Icon name="unload" /></button>
                </Show>
                <Show when=move || !can_unload && !busy()>
                    <button class="icon-action danger" data-tooltip="Remove model" aria-label="Remove model" on:click=move |_| ui.remove_target.set(Some(repo_id.get_value()))><Icon name="remove" /></button>
                </Show>
                <Show when=move || operation().is_some_and(|item| item.cancellable)>
                    <button class="icon-action" data-tooltip="Cancel operation" aria-label="Cancel operation" on:click=move |_| {
                        if let Some(item) = operation() { cancel(state, item.operation_id); }
                    }><Icon name="cancel" /></button>
                </Show>
            </div></td>
        </tr>
    }
}

fn format_label(model: &Model) -> String {
    if model.encoding.is_empty() || model.encoding == "Unknown" {
        model.ecosystem.clone()
    } else {
        model.encoding.clone()
    }
}

pub fn run_action(
    state: RuntimeState,
    target: String,
    path: &'static str,
    body: serde_json::Value,
) {
    state.set_busy(&target, Some(path));
    spawn_local(async move {
        match api::post::<serde_json::Value, _>(state, path, &body).await {
            Ok(_) => state.notify(format!("Operation accepted for {target}"), false),
            Err(error) => {
                state.set_busy(&target, None);
                state.notify(error, true);
            },
        }
    });
}

pub fn cancel(state: RuntimeState, operation_id: String) {
    spawn_local(async move {
        let body = json!({ "operation_id": operation_id });
        if let Err(error) =
            api::post::<serde_json::Value, _>(state, "/activity/cancel", &body).await
        {
            state.notify(error, true);
        }
    });
}

fn activity_for(state: RuntimeState, id: &str, selector: &str, repo_id: &str) -> Option<Activity> {
    state.activities.get().into_iter().find(|item| {
        model_operation(&item.kind)
            && (item.target == id || item.target == selector || item.target == repo_id)
    })
}

fn busy_for(state: RuntimeState, id: &str, selector: &str, repo_id: &str) -> Option<String> {
    let busy = state.busy.get();
    [id, selector, repo_id].into_iter().find_map(|target| busy.get(target).cloned())
}

fn live(activity: &Activity) -> bool {
    matches!(activity.state.as_str(), "queued" | "running" | "cancelling")
}

fn model_operation(kind: &str) -> bool {
    matches!(kind, "load" | "restore" | "unload" | "pull" | "remove")
}

fn operation_state(kind: &str) -> String {
    match kind {
        "pull" | "/models/pull" => "downloading",
        "unload" | "/models/unload" => "unloading",
        "remove" => "removing",
        _ => "loading",
    }
    .to_owned()
}
