use super::App;
use crate::rpc::{Client, proto};

impl App {
    pub(super) async fn activate_selected_model(&mut self, client: &mut Client) {
        let action = if self.searching_models() {
            self.selected_catalog_model().map_or(ModelAction::None, |model| {
                if model.local_source == "remote" {
                    ModelAction::Download
                } else if self
                    .local_catalog_model(model)
                    .is_some_and(|local| local.state == "ready")
                {
                    ModelAction::Unload
                } else {
                    ModelAction::Load
                }
            })
        } else {
            self.selected_local_model().map_or(ModelAction::None, |model| {
                match model.state.as_str() {
                    "ready" => ModelAction::Unload,
                    "available" => ModelAction::Load,
                    _ => ModelAction::None,
                }
            })
        };
        match action {
            ModelAction::Download => self.start_pull(client),
            ModelAction::Load => self.open_load_dialog(client),
            ModelAction::Unload => self.unload_selected(client).await,
            ModelAction::None => {},
        }
    }

    pub(super) async fn unload_selected(&mut self, client: &mut Client) {
        let selector = if self.searching_models() {
            let catalog = self.selected_catalog_model();
            catalog.map(|model| self.local_catalog_model(model).map_or(model.id.as_str(), selector))
        } else {
            self.selected_local_model().map(selector)
        };
        let Some(selector) = selector.map(ToOwned::to_owned) else {
            return;
        };
        match client
            .unload_model(proto::UnloadModelRequest { selector: selector.clone() })
            .await
        {
            Ok(response) => self.apply_unload(response.into_inner().unloaded, &selector),
            Err(error) => self.action_message = Some(error.to_string()),
        }
    }

    fn apply_unload(&mut self, unloaded: bool, selector: &str) {
        if !unloaded {
            self.action_message = Some("model was not loaded".to_owned());
            return;
        }
        let key = crate::config::model_key(selector).unwrap_or_else(|_| selector.to_owned());
        self.models.retain(|loaded| loaded.id != key);
        if let Some(local) = self
            .local_models
            .iter_mut()
            .find(|model| model.id == key || model.selector == selector)
        {
            "available".clone_into(&mut local.state);
        }
        self.action_message = Some(format!("unloaded {selector}"));
    }
}

enum ModelAction {
    Download,
    Load,
    Unload,
    None,
}

pub(super) fn selector(model: &proto::LocalModelInfo) -> &str {
    if model.selector.is_empty() {
        &model.id
    } else {
        &model.selector
    }
}
