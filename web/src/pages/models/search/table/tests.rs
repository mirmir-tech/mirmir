use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use leptos::prelude::*;

use super::{Row, Table};
use crate::types::catalog::{CatalogModel, CatalogResults};

fn model(id: &str, default: &str) -> CatalogModel {
    CatalogModel {
        id: id.to_owned(),
        default_variant: Some(default.to_owned()),
        ..Default::default()
    }
}

fn answer(models: &[CatalogModel]) -> CatalogResults {
    CatalogResults {
        models: models.to_vec(),
        next_cursor: None,
    }
}

/// Shows `models` as the answer to a new search for `query`.
fn found(table: Table, query: &str, models: &[CatalogModel]) {
    let ticket = table.update(|results| results.search(query)).flatten().expect("a request");
    table.update(|results| results.found(ticket, query, Ok(answer(models))));
}

fn row(table: Table, id: &str) -> Row {
    table
        .with_rows(|rows| rows.iter().find(|row| row.id == id).cloned())
        .expect("a row")
}

/// Counts how often what `read` reads notifies it, as a rendered row would.
fn runs<T: PartialEq + Send + Sync + 'static>(
    read: impl Fn() -> T + Send + Sync + 'static,
) -> impl Fn() -> usize {
    let count = Arc::new(AtomicUsize::new(0));
    let memo = ArcMemo::new({
        let count = Arc::clone(&count);
        move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            read()
        }
    });
    move || {
        memo.with_untracked(|_| ());
        count.load(Ordering::Relaxed) - 1
    }
}

/// A keystroke that supersedes the search shows it loading but notifies
/// neither a row nor the list of rows.
#[test]
fn a_new_request_notifies_no_row() {
    let table = Table::new();
    found(table, "qwen", &[model("a", "Q4_K_M"), model("b", "Q4_K_M")]);
    let first = row(table, "a").model;
    let row_runs = runs(move || first.get());
    let list_runs = runs(move || table.with_rows(<[Row]>::len));
    assert_eq!((row_runs(), list_runs()), (0, 0));

    assert!(table.update(|results| results.search("qwen ")).flatten().is_some());
    assert!(table.loading.get_untracked());
    assert_eq!((row_runs(), list_runs()), (0, 0));
}

/// An answer that repeats a row's model leaves it alone; one that changes
/// it refreshes the row the page already renders, wherever it moved.
#[test]
fn an_answer_notifies_only_the_rows_it_changes() {
    let table = Table::new();
    found(table, "qwen", &[model("a", "Q4_K_M"), model("b", "Q4_K_M")]);
    let (a, b) = (row(table, "a"), row(table, "b"));
    let (a_model, b_model) = (a.model.clone(), b.model.clone());
    let (a_runs, b_runs) = (runs(move || a_model.get()), runs(move || b_model.get()));

    found(table, "qwen", &[model("a", "Q4_K_M"), model("b", "Q4_K_M")]);
    assert_eq!((a_runs(), b_runs()), (0, 0));

    found(table, "qwen", &[model("c", "Q4_K_M"), model("b", "Q8_0"), model("a", "Q4_K_M")]);
    assert_eq!((a_runs(), b_runs()), (0, 1));
    assert!(row(table, "a") == a && row(table, "b") == b, "shown rows keep their signals");
    assert_eq!(
        b.model.with_untracked(|model| model.default_variant.clone()).as_deref(),
        Some("Q8_0")
    );
    let ids = table.with_rows(|rows| rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>());
    assert_eq!(ids, ["c", "b", "a"]);

    assert!(table.update(|results| results.search("")).flatten().is_none());
    assert!(table.with_rows(<[Row]>::is_empty) && !table.loading.get_untracked());
}
