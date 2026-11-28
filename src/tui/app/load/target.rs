use super::{App, LoadTarget};
use crate::{
    config::model_key,
    tui::app::{LocalRow, actions::selector},
};

impl App {
    pub(in crate::tui::app) fn load_target(&mut self) -> Option<LoadTarget> {
        if self.searching_models() {
            return self.catalog_load_target();
        }
        let model = self.selected_local_model()?;
        if model.state.is_missing() || model.state.is_ready() {
            self.action_message = Some(format!("model is already {}", model.state.as_str()));
            return None;
        }
        if !model.loadable {
            self.action_message = Some(model.load_unavailable_reason.clone());
            return None;
        }
        Some(local_load_target(model))
    }

    fn catalog_load_target(&mut self) -> Option<LoadTarget> {
        let model = self.selected_catalog_model()?;
        // The chosen variant, which the row's default may not be.
        let Some(target) = self.catalog_local_target(model) else {
            self.action_message = Some("download the model before loading it".to_owned());
            return None;
        };
        if let Some(local) = self.local_catalog_model(model) {
            if local.state.is_missing() || local.state.is_ready() {
                self.action_message = Some(format!("model is already {}", local.state.as_str()));
                return None;
            }
            if !local.loadable {
                self.action_message = Some(local.load_unavailable_reason.clone());
                return None;
            }
            return Some(local_load_target(local));
        }
        Some(LoadTarget {
            selector: target.clone(),
            config_id: model_key(&target).unwrap_or_else(|_| target.clone()),
            repo_id: target,
            revision: String::new(),
            commit: String::new(),
        })
    }
}

fn local_load_target(model: &LocalRow) -> LoadTarget {
    LoadTarget {
        selector: selector(model).to_owned(),
        config_id: model.id.clone(),
        repo_id: model.repo_id.clone(),
        revision: model.revision.clone(),
        commit: model.commit.clone(),
    }
}
