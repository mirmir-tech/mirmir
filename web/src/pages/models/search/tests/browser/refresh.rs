//! Browser regressions for refreshed GGUF rows, pagination across filtered-out
//! pages, and reopening the popup with the compatibility filter enabled.

use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlButtonElement, HtmlInputElement, HtmlSelectElement};

use super::{popup, route, rows, type_query};
use crate::testing::{Answer, Route, dispatch, find, requests, serve, until};

const PULL: &str = "/api/mirmir/v1/models/pull";

fn gguf_page(variant: &str) -> Value {
    json!({
        "models": [{
            "id": "demo/updated",
            "memory_fit": "fits",
            "reason": "",
            "ecosystem": "GGUF",
            "container": "gguf",
            "metal_compatibility": "supported",
            "cuda_compatibility": "supported",
            "tool_use": false,
            "thinking": false,
            "vision": false,
            "default_variant": variant,
            "variants": [
                {"name": variant, "size_kind": "estimated", "weight_bytes": 4_u64 << 30, "memory_fit": "fits"}
            ]
        }],
        "next_cursor": null
    })
}

fn unsupported_row(id: &str) -> Value {
    json!({
        "id": id,
        "compatibility": "unsupported",
        "memory_fit": "fits",
        "reason": "architecture is not supported",
        "ecosystem": "GGUF",
        "container": "gguf",
        "metal_compatibility": "unknown",
        "cuda_compatibility": "unknown",
        "tool_use": false,
        "thinking": false,
        "vision": false,
    })
}

/// A search that returns the same repository with a new variant updates the
/// reused row so Download submits the latest offered variant, not a withdrawn
/// one from the previous search.
#[wasm_bindgen_test]
async fn refreshed_row_submits_only_the_latest_variant() {
    serve(&[
        Route {
            method: "GET",
            prefix: "/api/mirmir/v1/catalog/search?query=updated",
            answers: vec![Answer::ok(gguf_page("Q8_0")), Answer::ok(gguf_page("Q4_K_M"))],
        },
        Route {
            method: "POST",
            prefix: PULL,
            answers: vec![Answer::ok(json!({}))],
        },
    ]);
    let (root, _view) = popup();
    type_query(&root, "updated");
    until("first search shows Q8_0", || {
        rows(&root) == ["demo/updated"]
            && find::<HtmlSelectElement>(&root, "select.variant-select").value() == "Q8_0"
    })
    .await;

    type_query(&root, "updated ");
    until("second search refreshes the row to Q4_K_M", || {
        find::<HtmlSelectElement>(&root, "select.variant-select").value() == "Q4_K_M"
    })
    .await;

    find::<HtmlButtonElement>(&root, ".model-actions button").click();
    until("pull request is sent", || {
        requests().iter().any(|request| request.method == "POST")
    })
    .await;
    let body = requests()
        .into_iter()
        .find(|request| request.method == "POST")
        .and_then(|request| request.body)
        .expect("a pull body");
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("JSON"),
        json!({"repo_id": "demo/updated:Q4_K_M", "revision": null})
    );
}

/// When the compatibility filter hides every row on a page, Load more lets the
/// user fetch subsequent pages without needing the table to scroll.
#[wasm_bindgen_test]
async fn load_more_reaches_compatible_results_after_a_filtered_page() {
    let page_one = json!({
        "models": [unsupported_row("demo/unsupported-1"), unsupported_row("demo/unsupported-2")],
        "next_cursor": "page-2",
    });
    let page_two = json!({
        "models": [{
            "id": "demo/compatible-2",
            "memory_fit": "fits",
            "reason": "",
            "ecosystem": "MLX",
            "container": "safetensors",
            "metal_compatibility": "supported",
            "cuda_compatibility": "supported",
            "tool_use": false,
            "thinking": false,
            "vision": false,
        }],
        "next_cursor": null,
    });
    serve(&[
        route(
            "/api/mirmir/v1/catalog/search?query=paged&limit=20&cursor=page-2",
            Answer::ok(page_two),
        ),
        route("/api/mirmir/v1/catalog/search?query=paged", Answer::ok(page_one)),
    ]);
    let (root, _view) = popup();
    type_query(&root, "paged");
    until("filtered empty message and Load more button show", || {
        root.query_selector(".catalog-load-more button").ok().flatten().is_some()
            && root
                .query_selector(".empty")
                .ok()
                .flatten()
                .and_then(|element| element.text_content())
                .as_deref()
                == Some("No compatible models on this page.")
    })
    .await;

    find::<HtmlButtonElement>(&root, ".catalog-load-more button").click();
    until("compatible model from page 2 appears", || rows(&root) == ["demo/compatible-2"]).await;
}

/// Reopening the search popup keeps the checkbox checked when incompatible
/// models are shown.
#[wasm_bindgen_test]
async fn reopening_search_keeps_incompatible_checkbox_checked() {
    let answer = json!({
        "models": [unsupported_row("demo/unsupported-bert")],
        "next_cursor": null,
    });
    serve(&[route("/api/mirmir/v1/catalog/search?query=bert", Answer::ok(answer))]);
    let (root, _view) = popup();
    type_query(&root, "bert");
    until("empty compatible state settles", || {
        root.query_selector(".empty").ok().flatten().is_some()
    })
    .await;

    let toggle = find::<HtmlInputElement>(&root, ".search-toggle input[type='checkbox']");
    toggle.set_checked(true);
    dispatch(&toggle, "change");
    until("unsupported row becomes visible", || rows(&root) == ["demo/unsupported-bert"]).await;

    find::<HtmlButtonElement>(&root, ".search-dismiss").click();
    until("popup closes", || {
        root.query_selector(".catalog-popup").ok().flatten().is_none()
    })
    .await;

    let input = find::<HtmlInputElement>(&root, ".search-field input");
    dispatch(&input, "focus");
    until("popup reopens with checkbox checked and row visible", || {
        root.query_selector(".search-toggle input[type='checkbox']")
            .ok()
            .flatten()
            .is_some()
            && find::<HtmlInputElement>(&root, ".search-toggle input[type='checkbox']").checked()
            && rows(&root) == ["demo/unsupported-bert"]
    })
    .await;
}
