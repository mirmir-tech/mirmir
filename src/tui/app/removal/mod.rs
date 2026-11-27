use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveDialog {
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub source: String,
}

impl App {
    pub(super) fn open_remove_dialog(&mut self) {
        if self.removal_rx.is_some() {
            self.action_message = Some("another model removal is already active".to_owned());
            return;
        }
        match self.remove_target() {
            Ok(target) => self.remove_dialog = Some(target),
            Err(message) => self.action_message = Some(message),
        }
    }

    pub(super) fn handle_remove_key(&mut self, key: KeyEvent, client: &Client) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n' | 'N') => self.remove_dialog = None,
            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                if let Some(target) = self.remove_dialog.take() {
                    self.start_removal(client, target);
                }
            },
            _ => {},
        }
    }

    fn remove_target(&self) -> Result<RemoveDialog, String> {
        if self.searching_models() {
            return self.catalog_remove_target();
        }
        let model = self
            .selected_local_model()
            .ok_or_else(|| "no local model is selected".to_owned())?;
        if model.state == "ready" {
            return Err("unload the model before removing it".to_owned());
        }
        if model.repo_id.is_empty() {
            return Err("this local path is not a Hugging Face cache entry".to_owned());
        }
        Ok(RemoveDialog {
            id: model.id.clone(),
            repo_id: model.repo_id.clone(),
            path: model.path.clone(),
            source: source_name(model.managed),
        })
    }

    fn catalog_remove_target(&self) -> Result<RemoveDialog, String> {
        let model = self
            .selected_catalog_model()
            .ok_or_else(|| "no search result is selected".to_owned())?;
        // The chosen variant alone, never the repository with its others,
        // unless another tool's cache holds the repository whole.
        let target = self
            .catalog_local_target(model)
            .ok_or_else(|| "model is not downloaded".to_owned())?;
        let local = self.local_catalog_model(model);
        if local.is_some_and(|model| model.state == "ready") {
            return Err("unload the model before removing it".to_owned());
        }
        // A variant is mirmir's own download; another tool's cache holds
        // repositories whole.
        let managed = target != model.id
            || local.map_or(model.local_source == "mirmir", |local| local.managed);
        Ok(RemoveDialog {
            id: local.map_or_else(|| target.clone(), |local| local.id.clone()),
            repo_id: target,
            path: local.map_or_else(
                || "cache path will be resolved by the server".to_owned(),
                |local| local.path.clone(),
            ),
            source: source_name(managed),
        })
    }

    fn start_removal(&mut self, client: &Client, target: RemoveDialog) {
        self.set_local_state(&target.id, "removing");
        let request = proto::RemoveModelRequest { repo_id: target.repo_id.clone() };
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            // A refusal says why in its message, without the status's code.
            let result = client
                .remove_model(request)
                .await
                .map(tonic::Response::into_inner)
                .map_err(|status| status.message().to_owned());
            drop(sender.send(result).await);
        }));
        self.action_message = Some(format!("removing {}…", target.id));
        self.pending_removal = Some(target);
        self.removal_rx = Some(receiver);
    }

    pub(super) fn poll_removal(&mut self) {
        let Some(result) = self.removal_rx.as_mut().map(mpsc::Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(response)) => self.finish_removal(response),
            Ok(Err(error)) => {
                if let Some(target) = self.pending_removal.as_ref() {
                    let id = target.id.clone();
                    self.set_local_state(&id, "available");
                }
                self.action_message = Some(error);
            },
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.action_message = Some("model removal ended unexpectedly".to_owned());
            },
        }
        self.pending_removal = None;
        self.removal_rx = None;
    }

    fn finish_removal(&mut self, response: proto::RemoveModelResponse) {
        let Some(target) = self.pending_removal.as_ref() else {
            return;
        };
        if !response.removed {
            self.action_message = Some("model was not found in a local cache".to_owned());
            return;
        }
        let repo_id = target.repo_id.clone();
        self.apply_removal(&repo_id);
        self.action_message = Some(format!("removed model; freed {} bytes", response.freed_bytes));
    }

    fn apply_removal(&mut self, repo_id: &str) {
        self.mark_downloaded(repo_id, false);
        self.local_models
            .retain(|model| !super::variants::removes(repo_id, &model.repo_id));
        self.local_selected = self.local_selected.min(self.local_model_count().saturating_sub(1));
        // A row no longer downloaded may leave the compatible filter.
        let rows = self.visible_catalog_count().saturating_sub(1);
        self.catalog_selected = self.catalog_selected.min(rows);
    }
}

fn source_name(managed: bool) -> String {
    if managed {
        "MIRMIR"
    } else {
        "HF CACHE"
    }
    .to_owned()
}

#[cfg(test)]
mod tests;
