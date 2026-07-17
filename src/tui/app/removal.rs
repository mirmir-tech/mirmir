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
        if !matches!(model.local_source.as_str(), "mirmir" | "hf_cache") {
            return Err("model is not downloaded".to_owned());
        }
        let local = self.local_catalog_model(model);
        Ok(RemoveDialog {
            id: model.id.clone(),
            repo_id: model.id.clone(),
            path: local.map_or_else(
                || "cache path will be resolved by the server".to_owned(),
                |local| local.path.clone(),
            ),
            source: source_name(model.local_source == "mirmir"),
        })
    }

    fn start_removal(&mut self, client: &Client, target: RemoveDialog) {
        let request = proto::RemoveModelRequest { repo_id: target.repo_id.clone() };
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            let result = client
                .remove_model(request)
                .await
                .map(tonic::Response::into_inner)
                .map_err(|error| error.to_string());
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
            Ok(Err(error)) => self.action_message = Some(error),
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
        if let Some(model) = self.catalog.iter_mut().find(|model| model.id == repo_id) {
            model.downloaded = false;
            "remote".clone_into(&mut model.local_source);
        }
        self.local_models.retain(|model| model.repo_id != repo_id);
        self.local_selected = self.local_selected.min(self.local_model_count().saturating_sub(1));
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
mod tests {
    use super::*;

    #[test]
    fn external_cache_model_opens_removal_confirmation() {
        let mut app = App::new(true);
        app.local_models.push(proto::LocalModelInfo {
            id: "Qwen--External".to_owned(),
            repo_id: "Qwen/External".to_owned(),
            revision: "main".to_owned(),
            commit: "abc123".to_owned(),
            path: "/hf/models--Qwen--External/snapshots/abc123".to_owned(),
            state: "available".to_owned(),
            recent_rank: None,
            selector: "/hf/models--Qwen--External/snapshots/abc123".to_owned(),
            managed: false,
        });

        app.open_remove_dialog();

        let dialog = app.remove_dialog.expect("confirmation should open");
        assert_eq!(dialog.repo_id, "Qwen/External");
        assert_eq!(dialog.source, "HF CACHE");
    }
}
