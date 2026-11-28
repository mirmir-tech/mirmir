//! Pull activity lookup and download initiation for one catalog row.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    api,
    state::RuntimeState,
    types::{Activity, ActivityKind, ActivityState, Model, ModelState, catalog::CatalogModel},
};

pub(super) fn pull_activity(state: RuntimeState, id: &str) -> Option<Activity> {
    state.activities.get().into_iter().find(|item| live_pull(item, id))
}

/// The state of the latest pull of `target` the server reported.
pub(super) fn last_pull(state: RuntimeState, target: &str) -> Option<ActivityState> {
    state.activities.with(|items| {
        items
            .iter()
            .find(|item| item.target == target && item.kind == ActivityKind::Pull)
            .map(|item| item.state)
    })
}

/// Whether a pull that is no longer running left nothing to install: it
/// failed, was cancelled, or was never reported. A completed pull did
/// install, though the models list may not show it yet.
pub(super) const fn pull_ended_without_install(last: Option<ActivityState>) -> bool {
    matches!(last, None | Some(ActivityState::Failed | ActivityState::Cancelled))
}

/// Whether `target` is being downloaded, as the row shows it: busy since
/// the click, or a live pull.
pub(super) fn pulling(state: RuntimeState, target: &str) -> bool {
    state.busy.with_untracked(|busy| busy.contains_key(target))
        || state
            .activities
            .with_untracked(|items| items.iter().any(|item| live_pull(item, target)))
}

fn live_pull(item: &Activity, id: &str) -> bool {
    item.target == id && item.kind == ActivityKind::Pull && item.state.is_live()
}

pub(super) fn start_download(state: RuntimeState, model: &CatalogModel, repo_id: String) {
    let inserted = !state.models.get_untracked().iter().any(|item| item.repo_id == repo_id);
    if inserted {
        state.models.update(|items| items.push(download_placeholder(model, &repo_id)));
    }
    state.set_busy(&repo_id, Some("/models/pull"));
    spawn_local(async move {
        match api::post::<serde_json::Value, _>(state, "/models/pull", &pull_body(&repo_id)).await {
            Ok(_) => state.notify(format!("Download accepted for {repo_id}"), false),
            Err(error) => {
                state.set_busy(&repo_id, None);
                if inserted {
                    state.models.update(|items| items.retain(|item| item.repo_id != repo_id));
                }
                state.notify(error, true);
            },
        }
    });
}

/// The `/models/pull` request for `repo_id`, a repository or `id:name`.
pub(super) fn pull_body(repo_id: &str) -> serde_json::Value {
    serde_json::json!({"repo_id": repo_id, "revision": null})
}

fn download_placeholder(model: &CatalogModel, repo_id: &str) -> Model {
    Model {
        id: repo_id.to_owned(),
        repo_id: repo_id.to_owned(),
        revision: "main".to_owned(),
        state: ModelState::Downloading,
        selector: repo_id.to_owned(),
        ecosystem: model.ecosystem.clone(),
        container: model.container.clone(),
        encoding: model.encoding.clone(),
        metal_compatibility: model.metal_compatibility,
        cuda_compatibility: model.cuda_compatibility,
        size_bytes: model.estimated_weight_bytes.unwrap_or_default(),
        tool_use: model.tool_use,
        thinking: model.thinking,
        vision: model.vision,
        ..Default::default()
    }
}
