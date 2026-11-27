use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

mod results;
mod row;
#[cfg(test)]
mod tests;

use self::{
    results::{Results, Settled},
    row::CatalogRow,
};
use super::ModelUi;
use crate::{
    api,
    state::RuntimeState,
    types::{
        Model,
        catalog::{CatalogModel, CatalogResults},
    },
};

/// The pause after the last keystroke before the catalog is searched.
const DEBOUNCE_MS: u32 = 320;

#[component]
pub fn SearchPopup() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let ui = expect_context::<ModelUi>();
    let query = RwSignal::new(String::new());
    let results = RwSignal::new(Results::default());
    let loading = move || results.with(Results::loading);
    let show_incompatible = RwSignal::new(false);
    let visible_results = move || {
        let show_incompatible = show_incompatible.get();
        results.with(|results| {
            results
                .models()
                .iter()
                .filter(|model| show_incompatible || row_fits(model))
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    let search = move |value: String| {
        query.set(value.clone());
        ui.search_open.set(true);
        let Some(ticket) = results.try_update(|results| results.search(&value)).flatten() else {
            return;
        };
        spawn_local(async move {
            TimeoutFuture::new(DEBOUNCE_MS).await;
            if !results.try_with_untracked(|results| results.current(ticket)).unwrap_or(false) {
                return;
            }
            let path =
                format!("/catalog/search?query={}&limit=20", urlencoding::encode(value.trim()));
            let answer = api::get::<CatalogResults>(&path).await;
            settle(state, results, |results| results.found(ticket, &value, answer));
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
                    <div class="catalog-toolbar"><p class="catalog-status" class:loading=loading>{move || if loading() { "Searching Hugging Face…" } else if query.get().is_empty() { "Start typing to search Hugging Face." } else { "Search results" }}</p><label class="search-toggle"><input type="checkbox" on:change=move |event| show_incompatible.set(event_target_checked(&event)) />"Show incompatible models"</label></div>
                    <div class="table-wrap catalog-table" on:scroll=move |event| {
                        let element = event_target::<web_sys::HtmlElement>(&event);
                        if element.scroll_height() - element.scroll_top() - element.client_height() < 160 { load_more(state, results); }
                    }>
                        <table><thead><tr><th>Name</th><th>Type</th><th>Size</th><th>Features</th><th>State</th><th><span class="visually-hidden">Actions</span></th></tr></thead>
                        <tbody><For each=visible_results key=|model| model.id.clone() children=move |model| view! { <CatalogRow model=model /> } /></tbody></table>
                        <Show when=move || !loading() && !query.get().is_empty() && results.with(|results| results.models().is_empty())><p class="empty">"No results."</p></Show>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Appends the next page of the shown results, unless a search or page is
/// in flight or none is left.
fn load_more(state: RuntimeState, results: RwSignal<Results>) {
    let Some(page) = results
        .try_maybe_update(|results| {
            let page = results.page();
            (page.is_some(), page)
        })
        .flatten()
    else {
        return;
    };
    spawn_local(async move {
        let path = format!(
            "/catalog/search?query={}&limit=20&cursor={}",
            urlencoding::encode(&page.query),
            urlencoding::encode(&page.cursor)
        );
        let answer = api::get::<CatalogResults>(&path).await;
        settle(state, results, |results| results.paged(page.ticket, answer));
    });
}

/// Applies an answer unless a later request superseded it, and reports the
/// failure of the latest one.
fn settle(
    state: RuntimeState,
    results: RwSignal<Results>,
    answer: impl FnOnce(&mut Results) -> Settled,
) {
    let settled = results.try_maybe_update(|results| {
        let settled = answer(results);
        (settled != Settled::Stale, settled)
    });
    if let Some(Settled::Failed(error)) = settled {
        state.notify(error, true);
    }
}

/// Whether the server lists `target`, a repository or a GGUF variant as
/// `id:name`, downloaded whole, in any case the Hub resolves it in: a
/// paused, missing or failed download is not.
pub(super) fn installed(local: &[Model], target: &str) -> bool {
    local
        .iter()
        .any(|item| item.state.installed() && item.repo_id.eq_ignore_ascii_case(target))
}

/// Whether a catalog row stays visible under the compatible filter: a GGUF
/// repository stays listed when any offered variant fits, matching the TUI.
pub(super) fn row_fits(model: &CatalogModel) -> bool {
    if model.variants.is_empty() {
        !does_not_fit(&model.memory_fit)
    } else {
        model.variants.iter().any(|variant| !does_not_fit(&variant.memory_fit))
    }
}

pub(super) fn does_not_fit(state: &str) -> bool {
    matches!(state, "does_not_fit" | "does-not-fit")
}
