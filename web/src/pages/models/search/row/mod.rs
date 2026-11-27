//! One catalog result: a GGUF repository shows its quantizations in a
//! select, sized and fitted one by one, and downloads the chosen one as
//! `id:name`; other repositories download whole.

mod quantizations;
mod select;
#[cfg(test)]
mod tests;

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use self::{quantizations::Quantizations, select::VariantSelect};
use super::{super::row::cancel, installed};
use crate::{
    api,
    components::{FeaturePills, Icon, StatePill, TypePill},
    state::{RuntimeState, bytes},
    types::{
        Activity, Model, ModelState,
        catalog::{CatalogModel, CatalogVariant},
    },
};

#[component]
pub(super) fn CatalogRow(model: CatalogModel) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let quantizations = RwSignal::new(Quantizations::estimated(
        model.variants.clone(),
        model.default_variant.clone(),
    ));
    let gguf = !model.variants.is_empty();
    let format = if gguf {
        "GGUF".to_owned()
    } else {
        model.encoding.clone().unwrap_or_else(|| model.ecosystem.clone())
    };
    let format_detail = format!("{} · {}", model.ecosystem, model.container);
    let id = model.id.clone();
    let id_for_select = model.id.clone();
    let (tool_use, thinking, vision) = (model.tool_use, model.thinking, model.vision);
    let stored = StoredValue::new(model);
    // The select shows, and a download takes, this one resolved choice.
    let variant = move || quantizations.with(|item| item.chosen().cloned());
    let target =
        move || stored.with_value(|model| quantizations.with(|item| item.target(&model.id)));
    // A download pins its variant, so a listing that answers later keeps
    // showing the variant being downloaded.
    let pull = move || {
        let target = stored.with_value(|model| {
            quantizations.try_update(|item| {
                item.pin_chosen();
                item.target(&model.id)
            })
        });
        if let Some(target) = target {
            start_download(state, &stored.get_value(), target);
        }
    };
    // From the server's live list, which a pull or removal of any variant
    // changes, not from the search result.
    let downloaded = move || state.models.with(|models| installed(models, &target()));
    let operation = move || pull_activity(state, &target());
    let busy = move || state.busy.get().contains_key(&target());
    // A variant the listing withdrew stays while its download runs and
    // once it completes, even before the models list shows it downloaded;
    // after a failed or cancelled pull, or none, the row takes the default.
    Effect::new(move || {
        let kept = quantizations.with(|item| item.kept().is_some());
        if kept
            && !busy()
            && operation().is_none()
            && !downloaded()
            && pull_ended_without_install(last_pull(state, &target()).as_deref())
        {
            quantizations.update(Quantizations::release_kept);
        }
    });
    let display_state = move || {
        if operation().is_some() || busy() {
            "downloading".to_owned()
        } else if downloaded() {
            "downloaded".to_owned()
        } else {
            variant().map_or_else(
                || stored.with_value(|model| model.memory_fit.clone()),
                |variant| variant.memory_fit,
            )
        }
    };
    let reason = move || stored.with_value(|model| variant_reason(model, variant().as_ref()));
    let detail = move || {
        operation().map_or_else(
            || {
                if downloaded() {
                    "Downloaded".to_owned()
                } else {
                    reason()
                }
            },
            |item| item.detail,
        )
    };
    let progress = move || {
        operation().and_then(|item| {
            item.current
                .zip(item.total)
                .map(|(current, total)| 100.0 * current as f64 / total.max(1) as f64)
        })
    };
    let size = move || {
        variant().map_or_else(
            || {
                stored
                    .with_value(|model| model.estimated_weight_bytes)
                    .map_or_else(|| "—".to_owned(), bytes)
            },
            |variant| size_label(&variant),
        )
    };
    view! { <tr><td><span class="model-name"><strong>{id}</strong><small>{reason}</small></span></td><td>{move || if gguf { view! { <VariantSelect repo=id_for_select.clone() quantizations=quantizations /> }.into_any() } else { view! { <TypePill value=format.clone() /><small>{format_detail.clone()}</small> }.into_any() }}</td><td class="model-size">{size}</td><td><FeaturePills tool_use=tool_use thinking=thinking vision=vision /></td><td><StatePill state=display_state detail=detail progress=progress /></td><td class="model-actions">
        <Show when=move || operation().is_some_and(|item| item.cancellable) fallback=move || view! {
            <button class="icon-action" class:downloaded=downloaded disabled=move || downloaded() || busy() || quantizations.with(|item| item.kept().is_some()) data-tooltip=move || if downloaded() { "Downloaded".to_owned() } else { format!("Download {}", target()) } aria-label=move || if downloaded() { "Downloaded" } else { "Download model" } on:click=move |_| pull()>{move || view! { <Icon name=if downloaded() { "downloaded" } else { "download" } /> }}</button>
        }>
            <button class="icon-action" data-tooltip="Cancel download" aria-label="Cancel download" on:click=move |_| if let Some(item) = operation() { cancel(state, item.operation_id); }><Icon name="cancel" /></button>
        </Show>
    </td></tr> }
}

/// Memory explanation matching the resolved variant and its size precision,
/// or the repository's own reason when it has no variants or is unsupported.
pub(super) fn variant_reason(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    let Some(variant) = variant else {
        return model.reason.clone();
    };
    if model.metal_compatibility == "unsupported" && model.cuda_compatibility == "unsupported" {
        return model.reason.clone();
    }
    let exact = variant.size_kind == "exact";
    match (variant.memory_fit.as_str(), exact) {
        ("fits", true) => "weights and runtime reserve fit the current budget".to_owned(),
        ("fits", false) => {
            "estimated weights and runtime reserve fit the current budget".to_owned()
        },
        ("tight", true) => "weights and runtime reserve leave less than 15% headroom".to_owned(),
        ("tight", false) => "estimated usage leaves less than 15% headroom".to_owned(),
        ("does_not_fit", true) => {
            "weights and runtime reserve exceed the current memory budget".to_owned()
        },
        ("does_not_fit", false) => "estimated usage exceeds the current memory budget".to_owned(),
        ("unknown", _) => "insufficient Hub or device metadata for a reliable estimate".to_owned(),
        (other, _) if other == model.memory_fit => model.reason.clone(),
        _ => String::new(),
    }
}

/// The variant's size; `~` marks one estimated before its files are listed.
pub(super) fn size_label(variant: &CatalogVariant) -> String {
    let estimated = if variant.size_kind == "estimated" {
        "~"
    } else {
        ""
    };
    variant
        .weight_bytes
        .map_or_else(|| "—".to_owned(), |weight| format!("{estimated}{}", bytes(weight)))
}

fn pull_activity(state: RuntimeState, id: &str) -> Option<Activity> {
    state.activities.get().into_iter().find(|item| live_pull(item, id))
}

/// The state of the latest pull of `target` the server reported.
fn last_pull(state: RuntimeState, target: &str) -> Option<String> {
    state.activities.with(|items| {
        items
            .iter()
            .find(|item| item.target == target && item.kind == "pull")
            .map(|item| item.state.clone())
    })
}

/// Whether a pull that is no longer running left nothing to install: it
/// failed, was cancelled, or was never reported. A completed pull did
/// install, though the models list may not show it yet.
fn pull_ended_without_install(last: Option<&str>) -> bool {
    matches!(last, None | Some("failed" | "cancelled"))
}

/// Whether `target` is being downloaded, as the row shows it: busy since
/// the click, or a live pull.
fn pulling(state: RuntimeState, target: &str) -> bool {
    state.busy.with_untracked(|busy| busy.contains_key(target))
        || state
            .activities
            .with_untracked(|items| items.iter().any(|item| live_pull(item, target)))
}

fn live_pull(item: &Activity, id: &str) -> bool {
    item.target == id
        && item.kind == "pull"
        && matches!(item.state.as_str(), "queued" | "running" | "cancelling")
}

fn start_download(state: RuntimeState, model: &CatalogModel, repo_id: String) {
    let inserted = !state.models.get_untracked().iter().any(|item| item.repo_id == repo_id);
    if inserted {
        state.models.update(|items| items.push(download_placeholder(model, &repo_id)));
    }
    state.set_busy(&repo_id, Some("/models/pull"));
    spawn_local(async move {
        match api::post::<serde_json::Value, _>(state, "/models/pull", &pull_body(&repo_id)).await {
            Ok(_) => state.notify(format!("Download accepted for {repo_id}"), false),
            Err(error) => {
                state.set_busy(&repo_id, None);
                if inserted {
                    state.models.update(|items| items.retain(|item| item.repo_id != repo_id));
                }
                state.notify(error, true);
            },
        }
    });
}

/// The `/models/pull` request for `repo_id`, a repository or `id:name`.
fn pull_body(repo_id: &str) -> serde_json::Value {
    serde_json::json!({"repo_id": repo_id, "revision": null})
}

fn download_placeholder(model: &CatalogModel, repo_id: &str) -> Model {
    Model {
        id: repo_id.to_owned(),
        repo_id: repo_id.to_owned(),
        revision: "main".to_owned(),
        state: ModelState::Downloading,
        selector: repo_id.to_owned(),
        ecosystem: model.ecosystem.clone(),
        container: model.container.clone(),
        encoding: model.encoding.clone(),
        metal_compatibility: model.metal_compatibility.clone(),
        cuda_compatibility: model.cuda_compatibility.clone(),
        size_bytes: model.estimated_weight_bytes.unwrap_or_default(),
        tool_use: model.tool_use,
        thinking: model.thinking,
        vision: model.vision,
        ..Default::default()
    }
}
