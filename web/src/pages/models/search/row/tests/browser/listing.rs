//! When the variants are listed, and what a listing tells the user.

use serde_json::json;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, HtmlSelectElement};

use super::*;
use crate::testing::{Answer, dispatch, find, release, serve, settle, until};

/// A pick the listing no longer offers falls back to its default, and the
/// user is told.
#[wasm_bindgen_test]
async fn a_withdrawn_pick_is_reported() {
    let listing = json!({
        "variants": [{"name": "Q4_K_M", "size_kind": "exact", "weight_bytes": 5 * GIB, "memory_fit": "fits"}],
        "default_variant": "Q4_K_M",
    });
    serve(&routes(vec![Answer::held(listing)]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the listing is requested", || listings() == 1).await;
    select.set_value("Q8_0");
    dispatch(&select, "change");
    release("/catalog/variants");
    until("the user is told", || {
        toasted(row.state, "demo/model:Q8_0 is no longer offered; chose demo/model:Q4_K_M")
    })
    .await;
    assert_eq!(toast(row.state, "no longer offered"), Some(false), "informational");
    settle(20).await;
    assert_eq!(select.value(), "Q4_K_M");
}

/// An empty listing keeps the estimates and the choice, like a failed one,
/// and the next look lists again.
#[wasm_bindgen_test]
async fn an_empty_listing_is_retried() {
    let empty = json!({"variants": [], "default_variant": null});
    serve(&routes(vec![Answer::ok(empty), Answer::ok(exact())]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the empty listing is reported", || {
        toasted(row.state, "listed no quantizations")
    })
    .await;
    settle(20).await;
    assert_eq!(select.value(), "Q8_0");
    assert_eq!(select.length(), 2, "the estimates stay");
    assert!(!listed(&row.root));

    dispatch(&select, "blur");
    dispatch(&select, "focus");
    until("the exact sizes are listed", || listed(&row.root)).await;
    assert_eq!(listings(), 2);
    settle(20).await;
    assert_eq!(select.value(), "Q4_K_M");
    assert_eq!(download(&row).await, json!({"repo_id": "demo/model:Q4_K_M", "revision": null}));
}

/// A listing that failed is requested again by the next look, which takes
/// the exact sizes; a successful one is not repeated.
#[wasm_bindgen_test]
async fn a_failed_listing_is_retried_on_the_next_focus() {
    serve(&routes(vec![Answer::status(503), Answer::ok(exact())]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the failure is reported", || toasted(row.state, "HTTP 503")).await;
    assert_eq!(listings(), 1);
    assert!(!listed(&row.root), "a failed listing keeps the estimates");

    dispatch(&select, "blur");
    dispatch(&select, "focus");
    until("the exact sizes are listed", || listed(&row.root)).await;
    assert_eq!(listings(), 2);
    dispatch(&select, "blur");
    dispatch(&select, "focus");
    settle(50).await;
    assert_eq!(listings(), 2, "a successful listing is not repeated");
    assert_eq!(
        find::<HtmlElement>(&row.root, ".model-size").text_content().as_deref(),
        Some("5.0 GiB")
    );
}

/// Selecting a variant whose fit disagrees with the search's original
/// default updates both the badge and its tooltip/subtitle together.
#[wasm_bindgen_test]
async fn badge_and_tooltip_follow_the_selected_variant() {
    serve(&routes(vec![Answer::ok(exact())]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the exact sizes are listed", || listed(&row.root)).await;
    settle(20).await;

    select.set_value("Q8_0");
    dispatch(&select, "change");
    settle(20).await;

    let pill = find::<HtmlElement>(&row.root, ".state-pill");
    assert_eq!(pill.text_content().as_deref(), Some("does not fit"));
    let tooltip = pill.get_attribute("data-tooltip").unwrap_or_default();
    assert!(tooltip.contains("exceed"), "tooltip contradicted badge: {tooltip}");
    let subtitle = find::<HtmlElement>(&row.root, ".model-name small")
        .text_content()
        .unwrap_or_default();
    assert!(subtitle.contains("exceed"), "subtitle contradicted badge: {subtitle}");
}
