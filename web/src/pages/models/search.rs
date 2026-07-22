use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{ModelUi, row::cancel};
use crate::{
    api,
    components::{FeaturePills, Icon, StatePill, TypePill},
    state::{RuntimeState, bytes},
    types::{
        Activity, Model,
        catalog::{CatalogModel, CatalogResults},
    },
};

#[component]
pub fn SearchPopup() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let ui = expect_context::<ModelUi>();
    let query = RwSignal::new(String::new());
    let results = RwSignal::new(CatalogResults::default());
    let loading = RwSignal::new(false);
    let show_incompatible = RwSignal::new(false);
    let generation = RwSignal::new(0_u64);
    Effect::new(move || {
        let local = state.models.get();
        results.update(|current| sync_downloaded(current, &local));
    });
    let visible_results = move || {
        results
            .get()
            .models
            .into_iter()
            .filter(|model| show_incompatible.get() || !does_not_fit(&model.memory_fit))
            .collect::<Vec<_>>()
    };
    let search = move |value: String| {
        query.set(value.clone());
        ui.search_open.set(true);
        let ticket = generation.get_untracked() + 1;
        generation.set(ticket);
        loading.set(!value.trim().is_empty());
        spawn_local(async move {
            TimeoutFuture::new(320).await;
            if generation.get_untracked() != ticket {
                return;
            }
            if value.trim().is_empty() {
                results.set(CatalogResults::default());
                loading.set(false);
                return;
            }
            let path =
                format!("/catalog/search?query={}&limit=20", urlencoding::encode(value.trim()));
            match api::get(&path).await {
                Ok(mut found) => {
                    sync_downloaded(&mut found, &state.models.get_untracked());
                    results.set(found);
                },
                Err(error) => state.notify(error, true),
            }
            loading.set(false);
        });
    };
    view! {
        <div class="model-search-launch" class:open=move || ui.search_open.get()>
            <div class="search-field"><span aria-hidden="true"></span>
                <input placeholder="Search by owner or model name" aria-label="Search Hugging Face" prop:value=move || query.get() on:focus=move |_| ui.search_open.set(true) on:input=move |event| search(event_target_value(&event)) on:keydown=move |event| { if event.key() == "Escape" { event.prevent_default(); ui.search_open.set(false); } } />
                <Show when=move || ui.search_open.get()><button class="search-dismiss" type="button" data-tooltip="Close model search" aria-label="Close model search" on:click=move |_| ui.search_open.set(false)>"×"</button></Show>
            </div>
            <Show when=move || ui.search_open.get()>
                <button class="catalog-dismiss-layer" type="button" aria-label="Close model search overlay" on:click=move |_| ui.search_open.set(false)></button>
                <div class="catalog-popup">
                    <div class="catalog-toolbar"><p class="catalog-status" class:loading=move || loading.get()>{move || if loading.get() { "Searching Hugging Face…" } else if query.get().is_empty() { "Start typing to search Hugging Face." } else { "Search results" }}</p><label class="search-toggle"><input type="checkbox" on:change=move |event| show_incompatible.set(event_target_checked(&event)) />"Show incompatible models"</label></div>
                    <div class="table-wrap catalog-table" on:scroll=move |event| {
                        let element = event_target::<web_sys::HtmlElement>(&event);
                        if element.scroll_height() - element.scroll_top() - element.client_height() < 160 { load_more(state, query, results, loading); }
                    }>
                        <table><thead><tr><th>Name</th><th>Type</th><th>Size</th><th>Features</th><th>State</th><th><span class="visually-hidden">Actions</span></th></tr></thead>
                        <tbody><For each=visible_results key=|model| (model.id.clone(), model.downloaded) children=move |model| view! { <CatalogRow model=model /> } /></tbody></table>
                        <Show when=move || !loading.get() && !query.get().is_empty() && results.get().models.is_empty()><p class="empty">"No results."</p></Show>
                    </div>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn CatalogRow(model: CatalogModel) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let id = StoredValue::new(model.id.clone());
    let reason = StoredValue::new(model.reason.clone());
    let stored_model = StoredValue::new(model.clone());
    let downloaded = model.downloaded;
    let fit = if downloaded {
        "downloaded".to_owned()
    } else {
        model.memory_fit.clone()
    };
    let operation = move || pull_activity(state, &id.get_value());
    let busy = move || state.busy.get().contains_key(&id.get_value());
    let display_state = move || {
        if operation().is_some() || busy() {
            "downloading".to_owned()
        } else {
            fit.clone()
        }
    };
    let detail = move || operation().map_or_else(|| reason.get_value(), |item| item.detail);
    let progress = move || {
        operation().and_then(|item| {
            item.current
                .zip(item.total)
                .map(|(current, total)| 100.0 * current as f64 / total.max(1) as f64)
        })
    };
    let format = if model.encoding.is_empty() || model.encoding == "Unknown" {
        model.ecosystem.clone()
    } else {
        model.encoding.clone()
    };
    let format_detail = format!("{} · {}", model.ecosystem, model.container);
    view! { <tr><td><span class="model-name"><strong>{model.id.clone()}</strong><small>{reason.get_value()}</small></span></td><td><TypePill value=format /><small>{format_detail}</small></td><td class="model-size">{model.estimated_weight_bytes.map_or_else(|| "—".to_owned(), bytes)}</td><td><FeaturePills tool_use=model.tool_use thinking=model.thinking vision=model.vision /></td><td><StatePill state=display_state detail=detail progress=progress /></td><td class="model-actions">
        <Show when=move || operation().is_some_and(|item| item.cancellable) fallback=move || view! {
            <button class="icon-action" class:downloaded=downloaded disabled=move || downloaded || busy() data-tooltip=if downloaded { "Downloaded" } else { "Download model" } aria-label=if downloaded { "Downloaded" } else { "Download model" } on:click=move |_| start_download(state, &stored_model.get_value())><Icon name=if downloaded { "downloaded" } else { "download" } /></button>
        }>
            <button class="icon-action" data-tooltip="Cancel download" aria-label="Cancel download" on:click=move |_| if let Some(item) = operation() { cancel(state, item.operation_id); }><Icon name="cancel" /></button>
        </Show>
    </td></tr> }
}

fn load_more(
    state: RuntimeState,
    query: RwSignal<String>,
    results: RwSignal<CatalogResults>,
    loading: RwSignal<bool>,
) {
    let Some(cursor) = results.get_untracked().next_cursor else {
        return;
    };
    if loading.get_untracked() {
        return;
    }
    loading.set(true);
    spawn_local(async move {
        let path = format!(
            "/catalog/search?query={}&limit=20&cursor={}",
            urlencoding::encode(&query.get_untracked()),
            urlencoding::encode(&cursor)
        );
        match api::get::<CatalogResults>(&path).await {
            Ok(mut page) => {
                sync_downloaded(&mut page, &state.models.get_untracked());
                results.update(|current| {
                    current.models.extend(page.models);
                    current.next_cursor = page.next_cursor;
                });
            },
            Err(error) => state.notify(error, true),
        }
        loading.set(false);
    });
}

fn pull_activity(state: RuntimeState, id: &str) -> Option<Activity> {
    state.activities.get().into_iter().find(|item| {
        item.target == id
            && item.kind == "pull"
            && matches!(item.state.as_str(), "queued" | "running" | "cancelling")
    })
}

fn sync_downloaded(results: &mut CatalogResults, local: &[Model]) {
    for model in &mut results.models {
        model.downloaded =
            local.iter().any(|item| item.repo_id == model.id && item.state != "downloading");
    }
}

fn start_download(state: RuntimeState, model: &CatalogModel) {
    let repo_id = model.id.clone();
    let inserted = !state.models.get_untracked().iter().any(|item| item.repo_id == repo_id);
    if inserted {
        state.models.update(|items| items.push(download_placeholder(model)));
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

fn download_placeholder(model: &CatalogModel) -> Model {
    Model {
        id: model.id.clone(),
        repo_id: model.id.clone(),
        revision: "main".to_owned(),
        state: "downloading".to_owned(),
        selector: model.id.clone(),
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

fn does_not_fit(state: &str) -> bool {
    matches!(state, "does_not_fit" | "does-not-fit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_model_refresh_marks_matching_catalog_result_as_downloaded() {
        let mut results = CatalogResults {
            models: vec![CatalogModel {
                id: "Qwen/Test".to_owned(),
                ..Default::default()
            }],
            next_cursor: None,
        };
        let local = Model {
            repo_id: "Qwen/Test".to_owned(),
            ..Default::default()
        };

        sync_downloaded(&mut results, &[local]);

        assert!(results.models[0].downloaded);
    }
}
