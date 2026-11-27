//! The search popup in a browser, with the catalog's answers held so an
//! older one arrives after a newer one.

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, HtmlInputElement};

use super::super::{super::ModelUi, SearchPopup};
use crate::{
    state::RuntimeState,
    testing::{Answer, Route, dispatch, find, mount, release, requests, serve, settle, until},
    types::catalog::{ContextChoice, GenerationSettings},
};

const SEARCH: &str = "/api/mirmir/v1/catalog/search";

fn page(ids: &[&str], cursor: Option<&str>) -> Value {
    let models = ids
        .iter()
        .map(|id| {
            json!({
                "id": id, "memory_fit": "fits", "reason": "", "ecosystem": "MLX",
                "container": "safetensors", "metal_compatibility": "supported",
                "cuda_compatibility": "supported", "tool_use": false, "thinking": false,
                "vision": false,
            })
        })
        .collect::<Vec<_>>();
    json!({"models": models, "next_cursor": cursor})
}

fn route(prefix: &'static str, answer: Answer) -> Route {
    Route {
        method: "GET",
        prefix,
        answers: vec![answer],
    }
}

fn popup() -> (HtmlElement, impl Sized) {
    mount(|| {
        provide_context(RuntimeState::new());
        provide_context(ModelUi {
            search_open: RwSignal::new(false),
            load_target: RwSignal::new(None),
            inspection: RwSignal::new(None),
            inspect_error: RwSignal::new(None),
            settings: RwSignal::new(GenerationSettings::default()),
            context: RwSignal::new(ContextChoice::default()),
            request: StoredValue::new(0),
            remove_target: RwSignal::new(None),
        });
        view! { <SearchPopup /> }
    })
}

fn type_query(root: &HtmlElement, query: &str) {
    let input = find::<HtmlInputElement>(root, ".search-field input");
    input.set_value(query);
    dispatch(&input, "input");
}

fn rows(root: &HtmlElement) -> Vec<String> {
    let rows = root.query_selector_all("tbody strong").expect("a valid selector");
    (0..rows.length())
        .filter_map(|index| rows.item(index).and_then(|node| node.text_content()))
        .collect()
}

fn sent(part: &str) -> bool {
    requests().iter().any(|request| request.path.contains(part))
}

fn status(root: &HtmlElement) -> String {
    find::<HtmlElement>(root, ".catalog-status").text_content().unwrap_or_default()
}

/// The answer for `old`, held until `new` has answered, neither replaces
/// `new`'s results nor shows them as loading again.
#[wasm_bindgen_test]
async fn an_older_search_answer_does_not_replace_the_current_results() {
    serve(&[
        route(
            "/api/mirmir/v1/catalog/search?query=old",
            Answer::held(page(&["old/model"], None)),
        ),
        route(
            "/api/mirmir/v1/catalog/search?query=new",
            Answer::ok(page(&["new/model"], None)),
        ),
    ]);
    let (root, _view) = popup();
    type_query(&root, "old");
    until("old is searched", || sent("query=old")).await;
    type_query(&root, "new");
    until("new's results show", || rows(&root) == ["new/model"]).await;

    release("query=old");
    settle(100).await;
    assert_eq!(rows(&root), ["new/model"]);
    assert_eq!(status(&root), "Search results");
}

/// A page of the older search, held until a new search has answered,
/// neither appends its models nor replaces the new search's cursor.
#[wasm_bindgen_test]
async fn a_page_of_an_older_search_is_not_appended() {
    serve(&[
        route(
            "/api/mirmir/v1/catalog/search?query=old&limit=20&cursor=",
            Answer::held(page(&["old/2"], Some("old-3"))),
        ),
        route(
            "/api/mirmir/v1/catalog/search?query=old",
            Answer::ok(page(&["old/1"], Some("old-2"))),
        ),
        route(
            "/api/mirmir/v1/catalog/search?query=new&limit=20&cursor=",
            Answer::held(page(&["new/2"], None)),
        ),
        route(
            "/api/mirmir/v1/catalog/search?query=new",
            Answer::ok(page(&["new/1"], Some("new-2"))),
        ),
        route(SEARCH, Answer::status(404)),
    ]);
    let (root, _view) = popup();
    type_query(&root, "old");
    until("old's results show", || rows(&root) == ["old/1"]).await;
    let table = find::<HtmlElement>(&root, ".catalog-table");
    dispatch(&table, "scroll");
    until("old's next page is requested", || sent("cursor=old-2")).await;

    type_query(&root, "new");
    until("new's results show", || rows(&root) == ["new/1"]).await;
    release("cursor=old-2");
    settle(100).await;
    assert_eq!(rows(&root), ["new/1"]);
    assert_eq!(status(&root), "Search results");

    dispatch(&table, "scroll");
    until("new's next page is requested", || sent("cursor=new-2")).await;
    assert!(!sent("cursor=old-3"));
}
