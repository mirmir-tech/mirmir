//! A download whose variant the listing withdraws: kept while it runs and
//! once it completes, released after it fails or is cancelled.

use serde_json::json;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlOptionElement, HtmlSelectElement};

use super::*;
use crate::{
    testing::{Answer, dispatch, find, release, serve, settle, until},
    types::{Activity, Model, ModelState},
};

/// Starts a download of `Q8_0` while the listing is held, then releases a
/// listing that offers only `Q4_K_M`.
async fn keep_q8(row: &Row) -> HtmlSelectElement {
    let select = find::<HtmlSelectElement>(&row.root, "select");
    dispatch(&select, "focus");
    until("the listing is requested", || listings() == 1).await;
    assert_eq!(download(row).await, json!({"repo_id": "demo/model:Q8_0", "revision": null}));
    release("/catalog/variants");
    until("the user is told", || toasted(row.state, "its download continues")).await;
    settle(20).await;
    select
}

fn serve_q4_only() {
    let listing = json!({
        "variants": [{"name": "Q4_K_M", "size_kind": "exact", "weight_bytes": 5 * GIB, "memory_fit": "fits"}],
        "default_variant": "Q4_K_M",
    });
    serve(&routes(vec![Answer::held(listing)]));
}

fn pull_ends(row: &Row, state: &str) {
    row.state.apply_activity(&Activity {
        operation_id: "pull-1".to_owned(),
        kind: "pull".to_owned(),
        target: "demo/model:Q8_0".to_owned(),
        state: state.to_owned(),
        ..Default::default()
    });
}

/// The kept variant stays busy and marked as not offered, cannot be picked
/// again, and the user is told its download continues.
#[wasm_bindgen_test]
async fn a_withdrawn_download_is_kept() {
    serve_q4_only();
    let row = row();
    let select = keep_q8(&row).await;
    assert_eq!(toast(row.state, "no longer offered"), Some(false), "informational");
    assert!(!toasted(row.state, "chose"), "the running download did not change");
    assert_eq!(select.value(), "Q8_0");
    let text = select.text_content().unwrap_or_default();
    assert!(text.contains("Q8_0 · ~7.0 GiB · not offered"), "{text}");
    assert!(find::<HtmlOptionElement>(&row.root, "option[value=Q8_0]").disabled());
    assert!(button(&row.root).disabled(), "the download is still in flight");
    button(&row.root).click();
    settle(50).await;
    assert_eq!(pulls().len(), 1, "no second download");
}

/// After a cancelled pull the row falls back to the listing's default, and
/// Download posts that, never the withdrawn variant.
#[wasm_bindgen_test]
async fn a_cancelled_kept_download_falls_back_to_the_default() {
    serve_q4_only();
    let row = row();
    let select = keep_q8(&row).await;
    pull_ends(&row, "cancelled");
    until("the row falls back", || select.value() == "Q4_K_M").await;
    assert!(row.root.query_selector("option[value=Q8_0]").ok().flatten().is_none());
    assert!(!button(&row.root).disabled());

    button(&row.root).click();
    until("the default's download is accepted", || {
        toasted(row.state, "Download accepted for demo/model:Q4_K_M")
    })
    .await;
    let pulls = pulls();
    assert_eq!(pulls.len(), 2, "{pulls:?}");
    let body: serde_json::Value =
        serde_json::from_str(pulls[1].body.as_deref().expect("a body")).expect("JSON");
    assert_eq!(body, json!({"repo_id": "demo/model:Q4_K_M", "revision": null}));
}

/// A completed pull keeps its variant, with Download disabled, before the
/// models list shows it downloaded and after.
#[wasm_bindgen_test]
async fn a_completed_kept_download_stays() {
    serve_q4_only();
    let row = row();
    let select = keep_q8(&row).await;
    pull_ends(&row, "completed");
    settle(50).await;
    assert_eq!(select.value(), "Q8_0", "kept before the models list shows it");
    assert!(button(&row.root).disabled());

    row.state.models.set(vec![Model {
        repo_id: "demo/model:Q8_0".to_owned(),
        state: ModelState::Available,
        ..Default::default()
    }]);
    settle(50).await;
    assert_eq!(select.value(), "Q8_0");
    assert!(button(&row.root).disabled());
    assert_eq!(button(&row.root).get_attribute("aria-label").as_deref(), Some("Downloaded"));
    assert_eq!(pulls().len(), 1);
}
