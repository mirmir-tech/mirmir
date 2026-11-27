//! What Download posts, and what the row shows while it runs.

use serde_json::json;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlSelectElement;

use super::*;
use crate::testing::{Answer, dispatch, find, release, serve, settle, until};

/// The exact listing's default replaces the estimated one in the select,
/// and Download posts the variant the select shows.
#[wasm_bindgen_test]
async fn the_download_takes_the_variant_the_select_shows() {
    serve(&routes(vec![Answer::ok(exact())]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    assert_eq!(select.value(), "Q8_0");

    dispatch(&select, "focus");
    until("the exact sizes are listed", || listed(&row.root)).await;
    settle(20).await;
    assert_eq!(select.value(), "Q4_K_M");
    assert_eq!(download(&row).await, json!({"repo_id": "demo/model:Q4_K_M", "revision": null}));
}

/// A variant picked while the listing is in flight outlasts its default.
#[wasm_bindgen_test]
async fn a_pick_outlasts_the_exact_default() {
    serve(&routes(vec![Answer::held(exact())]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the listing is requested", || listings() == 1).await;
    select.set_value("Q8_0");
    dispatch(&select, "change");
    release("/catalog/variants");
    until("the exact sizes are listed", || listed(&row.root)).await;
    settle(20).await;

    assert_eq!(select.value(), "Q8_0");
    assert_eq!(download(&row).await, json!({"repo_id": "demo/model:Q8_0", "revision": null}));
}

/// A download started while the listing is in flight keeps its variant
/// in the row, busy, when the listing answers with another default.
#[wasm_bindgen_test]
async fn a_download_during_the_listing_keeps_its_variant() {
    serve(&routes(vec![Answer::held(exact())]));
    let row = row();
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the listing is requested", || listings() == 1).await;
    assert_eq!(download(&row).await, json!({"repo_id": "demo/model:Q8_0", "revision": null}));

    release("/catalog/variants");
    until("the exact sizes are listed", || listed(&row.root)).await;
    settle(20).await;
    assert_eq!(select.value(), "Q8_0");
    assert!(button(&row.root).disabled(), "the download is still in flight");
    button(&row.root).click();
    settle(50).await;
    assert_eq!(pulls().len(), 1, "no second download");
}
