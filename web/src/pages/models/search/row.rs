//! One catalog result: a GGUF repository shows its quantizations in a
//! select, sized and fitted one by one, and downloads the chosen one as
//! `id:name`; other repositories download whole.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{super::row::cancel, installed};
use crate::{
    api,
    components::{FeaturePills, Icon, StatePill, TypePill},
    state::{RuntimeState, bytes},
    types::{
        Activity, Model, ModelState,
        catalog::{CatalogModel, CatalogVariant, ModelVariants},
    },
};

#[component]
pub(super) fn CatalogRow(model: CatalogModel) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let variants = RwSignal::new(model.variants.clone());
    let chosen = RwSignal::new(model.default_variant.clone());
    let gguf = !model.variants.is_empty();
    let format = if gguf {
        "GGUF".to_owned()
    } else {
        model.encoding.clone().unwrap_or_else(|| model.ecosystem.clone())
    };
    let format_detail = format!("{} · {}", model.ecosystem, model.container);
    let (id, reason) = (model.id.clone(), model.reason.clone());
    let id_for_select = model.id.clone();
    let (tool_use, thinking, vision) = (model.tool_use, model.thinking, model.vision);
    let stored = StoredValue::new(model);
    let variant = move || {
        let name = chosen.get()?;
        variants.get().into_iter().find(|variant| variant.name == name)
    };
    let target = move || {
        let id = stored.with_value(|model| model.id.clone());
        variant().map_or_else(|| id.clone(), |variant| format!("{id}:{}", variant.name))
    };
    // From the server's live list, which a pull or removal of any variant
    // changes, not from the search result.
    let downloaded = move || state.models.with(|models| installed(models, &target()));
    let operation = move || pull_activity(state, &target());
    let busy = move || state.busy.get().contains_key(&target());
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
    let detail = move || {
        operation()
            .map_or_else(|| stored.with_value(|model| model.reason.clone()), |item| item.detail)
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
    view! { <tr><td><span class="model-name"><strong>{id}</strong><small>{reason}</small></span></td><td>{move || if gguf { view! { <VariantSelect repo=id_for_select.clone() variants=variants chosen=chosen /> }.into_any() } else { view! { <TypePill value=format.clone() /><small>{format_detail.clone()}</small> }.into_any() }}</td><td class="model-size">{size}</td><td><FeaturePills tool_use=tool_use thinking=thinking vision=vision /></td><td><StatePill state=display_state detail=detail progress=progress /></td><td class="model-actions">
        <Show when=move || operation().is_some_and(|item| item.cancellable) fallback=move || view! {
            <button class="icon-action" class:downloaded=downloaded disabled=move || downloaded() || busy() data-tooltip=move || if downloaded() { "Downloaded".to_owned() } else { format!("Download {}", target()) } aria-label=move || if downloaded() { "Downloaded" } else { "Download model" } on:click=move |_| start_download(state, &stored.get_value(), target())>{move || view! { <Icon name=if downloaded() { "downloaded" } else { "download" } /> }}</button>
        }>
            <button class="icon-action" data-tooltip="Cancel download" aria-label="Cancel download" on:click=move |_| if let Some(item) = operation() { cancel(state, item.operation_id); }><Icon name="cancel" /></button>
        </Show>
    </td></tr> }
}

/// A GGUF repository's quantizations. The search estimates their sizes from
/// the parameter count; the first look at the select lists the files once,
/// with their exact sizes.
#[component]
fn VariantSelect(
    repo: String,
    variants: RwSignal<Vec<CatalogVariant>>,
    chosen: RwSignal<Option<String>>,
) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let listed = RwSignal::new(false);
    let path = format!("/catalog/variants?repo_id={}", urlencoding::encode(&repo));
    let list = move |_| {
        if listed.get_untracked() {
            return;
        }
        listed.set(true);
        let path = path.clone();
        spawn_local(async move {
            match api::get::<ModelVariants>(&path).await {
                Ok(found) => {
                    variants.set(found.variants);
                    if chosen.get_untracked().is_none() {
                        chosen.set(found.default_variant);
                    }
                },
                Err(error) => state.notify(error, true),
            }
        });
    };
    view! {
        <select class="variant-select" aria-label="Quantization" data-tooltip="Quantization to download" prop:value=move || chosen.get().unwrap_or_default() on:focus=list on:change=move |event| chosen.set(Some(event_target_value(&event)))>
            <For each=move || variants.get() key=|variant| (variant.name.clone(), variant.size_kind.clone()) children=move |variant| { let target = format!("{repo}:{}", variant.name); let label = format!("{} · {}", variant.name, size_label(&variant)); let mark = move || if state.models.with(|models| installed(models, &target)) { " ✓" } else { "" }; view! { <option value=variant.name>{label}{mark}</option> } } />
        </select>
    }
}

/// The variant's size; `~` marks one estimated before its files are listed.
fn size_label(variant: &CatalogVariant) -> String {
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
    state.activities.get().into_iter().find(|item| {
        item.target == id
            && item.kind == "pull"
            && matches!(item.state.as_str(), "queued" | "running" | "cancelling")
    })
}

fn start_download(state: RuntimeState, model: &CatalogModel, repo_id: String) {
    let inserted = !state.models.get_untracked().iter().any(|item| item.repo_id == repo_id);
    if inserted {
        state.models.update(|items| items.push(download_placeholder(model, &repo_id)));
    }
    state.set_busy(&repo_id, Some("/models/pull"));
    spawn_local(async move {
        let body = serde_json::json!({"repo_id": repo_id, "revision": null});
        match api::post::<serde_json::Value, _>(state, "/models/pull", &body).await {
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
