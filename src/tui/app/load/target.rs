use super::{App, LoadTarget};
use crate::{config::model_key, rpc::proto, tui::app::actions::selector};

impl App {
    pub(super) fn load_target(&mut self) -> Option<LoadTarget> {
        if self.searching_models() {
            return self.catalog_load_target();
        }
        let model = self.selected_local_model()?;
        if model.state == "missing" || model.state == "ready" {
            self.action_message = Some(format!("model is already {}", model.state));
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
        if !matches!(model.local_source.as_str(), "mirmir" | "hf_cache") {
            self.action_message = Some("download the model before loading it".to_owned());
            return None;
        }
        if let Some(local) = self.local_catalog_model(model) {
            if local.state == "missing" || local.state == "ready" {
                self.action_message = Some(format!("model is already {}", local.state));
                return None;
            }
            if !local.loadable {
                self.action_message = Some(local.load_unavailable_reason.clone());
                return None;
            }
            return Some(local_load_target(local));
        }
        Some(LoadTarget {
            selector: model.id.clone(),
            config_id: model_key(&model.id).unwrap_or_else(|_| model.id.clone()),
            repo_id: model.id.clone(),
            revision: String::new(),
            commit: String::new(),
        })
    }
}

fn local_load_target(model: &proto::LocalModelInfo) -> LoadTarget {
    LoadTarget {
        selector: selector(model).to_owned(),
        config_id: model.id.clone(),
        repo_id: model.repo_id.clone(),
        revision: model.revision.clone(),
        commit: model.commit.clone(),
    }
}
