//! Browser regressions for how much of the table a keystroke or an answer
//! re-renders: the rows follow their own models, not the request in flight.

use serde_json::{Value, json};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, HtmlSelectElement};

use super::{super::probe, popup, rows, status, type_query};
use crate::testing::{Answer, Route, serve, settle, until};

const ROWS: usize = 40;
const VARIANTS: [&str; 5] = ["Q2_K", "Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0"];

/// `ROWS` GGUF repositories of five variants each; `changed` offers a
/// different default for the first.
fn page(changed: bool) -> Value {
    let models = (0..ROWS)
        .map(|index| {
            let default = if changed && index == 0 { "Q8_0" } else { "Q4_K_M" };
            let variants = VARIANTS
                .iter()
                .map(|name| {
                    json!({"name": name, "size_kind": "estimated", "weight_bytes": 4_u64 << 30, "memory_fit": "fits"})
                })
                .collect::<Vec<_>>();
            json!({
                "id": format!("demo/many-{index:02}-GGUF"), "memory_fit": "fits", "reason": "",
                "ecosystem": "GGUF", "container": "gguf", "metal_compatibility": "supported",
                "cuda_compatibility": "supported", "tool_use": false, "thinking": false,
                "vision": false, "default_variant": default, "variants": variants,
            })
        })
        .collect::<Vec<_>>();
    json!({"models": models, "next_cursor": null})
}

fn first_variant(root: &HtmlElement) -> String {
    root.query_selector("select.variant-select")
        .ok()
        .flatten()
        .map(|select| wasm_bindgen::JsCast::unchecked_into::<HtmlSelectElement>(select).value())
        .unwrap_or_default()
}

/// A keystroke that supersedes the search, and an answer that repeats the
/// shown models, notify no row; an answer that changes one model notifies
/// that row alone, which then shows its new default.
#[wasm_bindgen_test]
async fn only_a_changed_model_re_renders_its_row() {
    serve(&[Route {
        method: "GET",
        prefix: "/api/mirmir/v1/catalog/search?query=many",
        answers: vec![Answer::ok(page(false)), Answer::ok(page(false)), Answer::ok(page(true))],
    }]);
    let (root, _view) = popup();
    type_query(&root, "many");
    until("every row shows", || {
        rows(&root).len() == ROWS && status(&root) == "Search results"
    })
    .await;
    settle(50).await;
    probe::take();

    type_query(&root, "many ");
    settle(50).await;
    assert_eq!(status(&root), "Searching Hugging Face…");
    assert_eq!(probe::take(), 0, "a keystroke re-rendered rows");

    until("the repeated answer settles", || status(&root) == "Search results").await;
    settle(50).await;
    assert_eq!(rows(&root).len(), ROWS);
    assert_eq!(probe::take(), 0, "an unchanged answer re-rendered rows");

    type_query(&root, "many  ");
    until("the first row shows its new default", || first_variant(&root) == "Q8_0").await;
    settle(50).await;
    assert_eq!(probe::take(), 1, "only the changed row re-renders");
}
