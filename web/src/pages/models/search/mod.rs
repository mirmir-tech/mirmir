use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

mod row;

use row::CatalogRow;

use super::ModelUi;
use crate::{
    api,
    state::RuntimeState,
    types::{Model, catalog::CatalogResults},
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

/// Marks what the server lists as downloaded: a repository, or a GGUF
/// repository's variants by their `id:name`.
fn sync_downloaded(results: &mut CatalogResults, local: &[Model]) {
    let on_server =
        |id: &str| local.iter().any(|item| item.repo_id == id && item.state != "downloading");
    for model in &mut results.models {
        for variant in &mut model.variants {
            variant.downloaded = on_server(&format!("{}:{}", model.id, variant.name));
        }
        model.downloaded =
            on_server(&model.id) || model.variants.iter().any(|variant| variant.downloaded);
    }
}

pub(super) fn does_not_fit(state: &str) -> bool {
    matches!(state, "does_not_fit" | "does-not-fit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::catalog::{CatalogModel, CatalogVariant};

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

    #[test]
    fn a_downloaded_variant_marks_its_repository() {
        let variant = |name: &str| CatalogVariant {
            name: name.to_owned(),
            ..Default::default()
        };
        let mut results = CatalogResults {
            models: vec![CatalogModel {
                id: "org/m-GGUF".to_owned(),
                variants: vec![variant("Q4_K_M"), variant("Q8_0")],
                ..Default::default()
            }],
            next_cursor: None,
        };
        let local = Model {
            repo_id: "org/m-GGUF:Q8_0".to_owned(),
            ..Default::default()
        };

        sync_downloaded(&mut results, &[local]);

        let model = &results.models[0];
        assert!(model.downloaded && !model.variants[0].downloaded && model.variants[1].downloaded);
    }
}
