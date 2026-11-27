//! The search table as the popup renders it. [`Results`] keeps the request
//! bookkeeping and the answered models outside the reactive graph; the
//! table publishes what the page shows into signals that change only when
//! their own value does. A keystroke that supersedes a request notifies the
//! status, never a row, and an answer re-renders only the rows whose model
//! it changed.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use leptos::prelude::*;

use super::results::{Results, Ticket};
use crate::types::catalog::CatalogModel;

/// One shown result. A row keeps its signal for as long as its repository
/// stays in the results, so the rendered row follows every later answer.
#[derive(Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub model: ArcRwSignal<CatalogModel>,
}

impl Row {
    fn new(model: &CatalogModel) -> Self {
        Self {
            id: model.id.clone(),
            model: ArcRwSignal::new(model.clone()),
        }
    }

    /// Takes `model` and notifies the row, unless it shows `model` already.
    fn refresh(&self, model: &CatalogModel) {
        self.model.maybe_update(|current| {
            let changed = current != model;
            if changed {
                model.clone_into(current);
            }
            changed
        });
    }
}

#[derive(Clone, Copy)]
pub struct Table {
    results: StoredValue<Results>,
    loading: RwSignal<bool>,
    has_more: RwSignal<bool>,
    /// Changes when a row is added, removed or moved, not when a model does.
    rows: RwSignal<Vec<Row>>,
}

impl Table {
    pub fn new() -> Self {
        Self {
            results: StoredValue::new(Results::default()),
            loading: RwSignal::new(false),
            has_more: RwSignal::new(false),
            rows: RwSignal::new(Vec::new()),
        }
    }

    /// Whether a search or page is in flight.
    pub fn loading(self) -> bool {
        self.loading.get()
    }

    /// Whether the server reported another page after the shown results.
    pub fn has_more(self) -> bool {
        self.has_more.get()
    }

    pub fn with_rows<U>(self, read: impl FnOnce(&[Row]) -> U) -> U {
        self.rows.with(|rows| read(rows))
    }

    /// Whether `ticket` is the latest request, without subscribing to it.
    pub fn current(self, ticket: Ticket) -> bool {
        self.results.try_with_value(|results| results.current(ticket)).unwrap_or(false)
    }

    /// Changes the results and publishes what that changed; `None` once the
    /// popup is gone.
    pub fn update<U>(self, change: impl FnOnce(&mut Results) -> U) -> Option<U> {
        let output = self.results.try_update_value(change)?;
        self.results.try_with_value(|results| {
            publish(self.loading, results.loading());
            publish(self.has_more, results.has_more());
            self.rows.maybe_update(|rows| reconcile(rows, results.models()));
        });
        Some(output)
    }
}

fn publish<T: PartialEq + Send + Sync + 'static>(signal: RwSignal<T>, value: T) {
    signal.maybe_update(|current| {
        let changed = *current != value;
        if changed {
            *current = value;
        }
        changed
    });
}

/// Brings `rows` to `models`: a repository already shown keeps its row and
/// is refreshed in place; whether the list itself changed is returned.
fn reconcile(rows: &mut Vec<Row>, models: &[CatalogModel]) -> bool {
    let same = rows.len() == models.len()
        && rows.iter().zip(models).all(|(row, model)| row.id == model.id);
    if same {
        for (row, model) in rows.iter().zip(models) {
            row.refresh(model);
        }
        return false;
    }
    let mut shown = rows.drain(..).map(|row| (row.id.clone(), row)).collect::<HashMap<_, _>>();
    rows.extend(models.iter().map(|model| {
        shown.remove(&model.id).map_or_else(
            || Row::new(model),
            |row| {
                row.refresh(model);
                row
            },
        )
    }));
    true
}
