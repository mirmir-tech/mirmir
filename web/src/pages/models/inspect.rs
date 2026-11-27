//! Model inspection for the load dialog, re-run as the context changes.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::ModelUi;
use crate::{
    api,
    state::RuntimeState,
    types::{
        Model,
        catalog::{ContextChoice, Inspection},
    },
};

/// The pause after the last context change before memory is re-estimated.
const DEBOUNCE_MS: u32 = 300;

/// Opens the load dialog for `model` and fills it from a fresh inspection.
pub fn inspect(model: Model) {
    let ui = expect_context::<ModelUi>();
    let state = expect_context::<RuntimeState>();
    ui.load_target.set(Some(model.clone()));
    ui.inspection.set(None);
    ui.inspect_error.set(None);
    let request = next_request(ui);
    spawn_local(async move {
        let result = fetch(&model.selector, None).await;
        if !current(ui, request, &model.selector) {
            return;
        }
        match result {
            Ok(inspection) => {
                ui.settings.set(inspection.settings.clone().unwrap_or_default());
                ui.context.set(
                    inspection
                        .context
                        .as_ref()
                        .map_or_else(ContextChoice::default, |context| context.saved),
                );
                ui.inspection.set(Some(inspection));
            },
            Err(error) => {
                ui.inspect_error.set(Some(error.clone()));
                state.notify(error, true);
            },
        }
    });
}

/// Chooses `choice` and re-estimates memory for it once the choice settles.
/// Generation parameters keep their edited values; stale answers are dropped.
pub fn choose_context(ui: ModelUi, choice: ContextChoice) {
    if ui.context.get_untracked() == choice {
        return;
    }
    ui.context.set(choice);
    let Some(model) = ui.load_target.get_untracked() else {
        return;
    };
    let request = next_request(ui);
    spawn_local(async move {
        TimeoutFuture::new(DEBOUNCE_MS).await;
        if !current(ui, request, &model.selector) {
            return;
        }
        let result = fetch(&model.selector, Some(choice)).await;
        if !current(ui, request, &model.selector) {
            return;
        }
        match result {
            Ok(inspection) => {
                ui.inspect_error.set(None);
                ui.inspection.set(Some(inspection));
            },
            Err(error) => ui.inspect_error.set(Some(error)),
        }
    });
}

async fn fetch(selector: &str, context: Option<ContextChoice>) -> Result<Inspection, String> {
    let mut path = format!("/models/inspect?selector={}", urlencoding::encode(selector));
    if let Some(context) = context {
        path.push_str("&context=");
        path.push_str(&context.query());
    }
    api::get::<Inspection>(&path).await
}

fn next_request(ui: ModelUi) -> u64 {
    ui.request.update_value(|request| *request += 1);
    ui.request.get_value()
}

/// Whether `request` is still the dialog's latest for `selector`; not once
/// the models page that asked has closed.
fn current(ui: ModelUi, request: u64, selector: &str) -> bool {
    ui.request.try_get_value() == Some(request)
        && ui
            .load_target
            .try_with_untracked(|target| {
                target.as_ref().is_some_and(|model| model.selector == selector)
            })
            .unwrap_or(false)
}
