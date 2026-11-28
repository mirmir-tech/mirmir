//! The row in a browser: what the select shows, what a download posts,
//! when the variants are listed, and what the user is told.

mod download;
mod kept;
mod listing;

use leptos::prelude::*;
use serde_json::{Value, json};
use web_sys::{HtmlButtonElement, HtmlElement};

use super::super::CatalogRow;
use crate::{
    state::RuntimeState,
    testing::{Answer, Recorded, Route, find, mount, requests, until},
    types::{
        Compatibility,
        catalog::{CatalogFit, CatalogModel, CatalogVariant, SizeKind},
    },
};

const GIB: u64 = 1 << 30;
const VARIANTS: &str = "/api/mirmir/v1/catalog/variants";
const PULL: &str = "/api/mirmir/v1/models/pull";

fn variant(name: &str, gib: u64) -> CatalogVariant {
    CatalogVariant {
        name: name.to_owned(),
        size_kind: SizeKind::Estimated,
        weight_bytes: Some(gib * GIB),
        memory_fit: CatalogFit::Fits,
    }
}

/// The search estimated `Q8_0` as the default.
fn model() -> CatalogModel {
    CatalogModel {
        id: "demo/model".to_owned(),
        memory_fit: CatalogFit::Fits,
        reason: "estimated weights and runtime reserve fit the current budget".to_owned(),
        metal_compatibility: Compatibility::Supported,
        cuda_compatibility: Compatibility::Supported,
        variants: vec![variant("Q4_K_M", 4), variant("Q8_0", 7)],
        default_variant: Some("Q8_0".to_owned()),
        ..Default::default()
    }
}

/// The exact listing finds `Q8_0` too large and defaults to `Q4_K_M`.
fn exact() -> Value {
    json!({
        "variants": [
            {"name": "Q4_K_M", "size_kind": "exact", "weight_bytes": 5 * GIB, "memory_fit": "fits"},
            {"name": "Q8_0", "size_kind": "exact", "weight_bytes": 9 * GIB, "memory_fit": "does_not_fit"},
        ],
        "default_variant": "Q4_K_M",
    })
}

/// A mounted row with the state it reports to.
struct Row {
    root: HtmlElement,
    state: RuntimeState,
    _view: Box<dyn std::any::Any>,
}

fn row() -> Row {
    let state = RuntimeState::new();
    let (root, view) = mount(move || {
        provide_context(state);
        view! { <table><tbody><CatalogRow model=Signal::stored(model()) /></tbody></table> }
    });
    Row { root, state, _view: Box::new(view) }
}

fn listed(root: &HtmlElement) -> bool {
    root.text_content()
        .is_some_and(|text| text.contains("Q8_0") && !text.contains('~'))
}

fn listings() -> usize {
    requests()
        .iter()
        .filter(|request| request.path.starts_with("/catalog/variants"))
        .count()
}

fn pulls() -> Vec<Recorded> {
    requests().into_iter().filter(|request| request.method == "POST").collect()
}

fn toasted(state: RuntimeState, part: &str) -> bool {
    toast(state, part).is_some()
}

/// Whether the toast naming `part` is an error; `None` without one.
fn toast(state: RuntimeState, part: &str) -> Option<bool> {
    state.toasts.with_untracked(|toasts| {
        toasts
            .iter()
            .find(|toast| toast.message.contains(part))
            .map(|toast| toast.error)
    })
}

fn button(root: &HtmlElement) -> HtmlButtonElement {
    find(root, ".model-actions button")
}

/// Clicks Download, waits until the server's acceptance is reported, and
/// returns the body it posted.
async fn download(row: &Row) -> Value {
    button(&row.root).click();
    until("the download is accepted", || toasted(row.state, "Download accepted")).await;
    let pulls = pulls();
    assert_eq!(pulls.len(), 1, "{pulls:?}");
    serde_json::from_str(pulls[0].body.as_deref().expect("a body")).expect("JSON")
}

fn routes(listing: Vec<Answer>) -> [Route; 2] {
    [
        Route {
            method: "GET",
            prefix: VARIANTS,
            answers: listing,
        },
        Route {
            method: "POST",
            prefix: PULL,
            answers: vec![Answer::ok(json!({}))],
        },
    ]
}
