//! Counts how often a shown row's model notifies it after the row first
//! rendered, so a browser test can tell which changes re-render rows.

use std::cell::Cell;

use leptos::prelude::*;

use crate::types::catalog::CatalogModel;

thread_local! {
    static NOTIFIED: Cell<usize> = const { Cell::new(0) };
}

/// Counts every later notification of `model` for as long as its row lives.
pub fn watch(model: Signal<CatalogModel>) {
    Effect::new(move |previous: Option<()>| {
        model.track();
        if previous.is_some() {
            NOTIFIED.with(|count| count.set(count.get() + 1));
        }
    });
}

/// The notifications counted since the last call, which restarts the count.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn take() -> usize {
    NOTIFIED.with(|count| count.replace(0))
}
