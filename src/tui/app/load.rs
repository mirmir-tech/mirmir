mod dialog;
mod target;

use crossterm::event::{KeyCode, KeyEvent};
use dialog::FIELD_COUNT;
pub use dialog::{LoadDialog, LoadStatus, LoadTarget, RestorePosition};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

impl App {
    pub(super) fn open_load_dialog(&mut self, client: &Client) {
        if self.lifecycle_rx.is_some() || self.settings_rx.is_some() {
            self.action_message = Some("another model lifecycle operation is active".to_owned());
            return;
        }
        let Some(target) = self.load_target() else {
            return;
        };
        let request = proto::InspectModelRequest { selector: target.selector.clone() };
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            let result = client
                .inspect_model(request)
                .await
                .map(tonic::Response::into_inner)
                .map_err(|error| error.to_string());
            drop(sender.send(result).await);
        }));
        self.load_dialog = Some(LoadDialog {
            target,
            status: LoadStatus::Inspecting,
            fields: std::array::from_fn(|_| String::new()),
            selected: 0,
            has_mirmir_overrides: false,
            memory: None,
            force: false,
            progress: None,
            error: None,
            restore: None,
        });
        self.settings_rx = Some(receiver);
    }

    pub(super) fn handle_load_key(&mut self, key: KeyEvent, client: &Client) {
        let Some(status) = self.load_dialog.as_ref().map(|dialog| dialog.status) else {
            return;
        };
        if status == LoadStatus::Loading {
            return;
        }
        if key.code == KeyCode::Char('x') {
            self.load_dialog = None;
            self.settings_rx = None;
            return;
        }
        if status != LoadStatus::Editing {
            return;
        }
        match key.code {
            KeyCode::Enter => self.confirm_load(client),
            KeyCode::Char('f') => self.toggle_force_load(),
            KeyCode::Up => self.select_previous_setting(),
            KeyCode::Down | KeyCode::Tab => self.select_next_setting(),
            KeyCode::Backspace => self.edit_setting(None),
            KeyCode::Char(character) if character.is_ascii_digit() || character == '.' => {
                self.edit_setting(Some(character));
            },
            _ => {},
        }
    }

    pub(super) fn poll_load_settings(&mut self) {
        let Some(result) = self.settings_rx.as_mut().map(mpsc::Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(response)) => self.apply_inspection(response),
            Ok(Err(error)) => self.fail_inspection(error),
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.fail_inspection("model inspection ended unexpectedly".to_owned());
            },
        }
        self.settings_rx = None;
    }

    fn apply_inspection(&mut self, response: proto::InspectModelResponse) {
        let Some(settings) = response.settings else {
            self.fail_inspection("server returned no model settings".to_owned());
            return;
        };
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.fields = [
                settings.max_tokens.to_string(),
                settings.temperature.to_string(),
                settings.top_p.to_string(),
                settings.top_k.to_string(),
                settings.repetition_penalty.to_string(),
            ];
            dialog.has_mirmir_overrides = response.has_mirmir_overrides;
            dialog.memory = response.memory;
            dialog.status = LoadStatus::Editing;
            dialog.error = None;
        }
    }

    fn fail_inspection(&mut self, error: String) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.status = LoadStatus::Editing;
            dialog.error = Some(error);
        }
    }

    fn confirm_load(&mut self, client: &Client) {
        let request = match self.load_dialog.as_ref().map(LoadDialog::request) {
            Some(Ok(request)) => request,
            Some(Err(error)) => {
                if let Some(dialog) = self.load_dialog.as_mut() {
                    dialog.error = Some(error);
                }
                return;
            },
            None => return,
        };
        self.start_load_request(client, request);
    }

    pub(super) fn start_load_request(&mut self, client: &Client, request: proto::LoadModelRequest) {
        let (sender, receiver) = mpsc::channel(64);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            match client.load_model(request).await {
                Ok(response) => forward_lifecycle(response.into_inner(), sender).await,
                Err(error) => drop(sender.send(Err(error.to_string())).await),
            }
        }));
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.status = LoadStatus::Loading;
            dialog.progress = None;
            dialog.error = None;
        }
        self.lifecycle_rx = Some(receiver);
    }

    pub(super) fn apply_load_event(&mut self, event: proto::ModelLifecycleEvent) {
        self.action_message = Some(format!("{} · {}", event.phase, event.detail));
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.progress = Some(event.clone());
        }
        let Some(model) = event.model else {
            return;
        };
        if let Some(local) = self
            .local_models
            .iter_mut()
            .find(|local| local.id == model.id || local.path == model.path)
        {
            "ready".clone_into(&mut local.state);
        }
        if !self.models.iter().any(|loaded| loaded.id == model.id) {
            self.models.push(model);
        }
        if self.load_dialog.as_ref().is_some_and(|dialog| dialog.restore.is_some()) {
            self.restore_completed = self.restore_completed.saturating_add(1);
        }
        self.load_dialog = None;
    }

    pub(super) fn fail_load(&mut self, error: String) {
        self.action_message = Some(error.clone());
        if self.load_dialog.as_ref().is_some_and(|dialog| dialog.restore.is_some()) {
            self.restore_completed = self.restore_completed.saturating_add(1);
            self.load_dialog = None;
        } else if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.status = LoadStatus::Editing;
            dialog.error = Some(error);
        }
        self.lifecycle_rx = None;
    }

    pub(super) fn finish_load_stream(&mut self) {
        self.lifecycle_rx = None;
        if self
            .load_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.status == LoadStatus::Loading)
        {
            self.fail_load("model load ended before the model became ready".to_owned());
        }
    }

    const fn select_previous_setting(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.selected = dialog.selected.saturating_sub(1);
        }
    }

    fn select_next_setting(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.selected = dialog.selected.saturating_add(1).min(FIELD_COUNT - 1);
        }
    }

    fn edit_setting(&mut self, character: Option<char>) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            let field = &mut dialog.fields[dialog.selected];
            if let Some(character) = character {
                field.push(character);
            } else {
                let _removed = field.pop();
            }
        }
    }

    fn toggle_force_load(&mut self) {
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.force = !dialog.force;
            dialog.error = None;
        }
    }
}

async fn forward_lifecycle(
    mut stream: tonic::Streaming<proto::ModelLifecycleEvent>,
    sender: mpsc::Sender<Result<proto::ModelLifecycleEvent, String>>,
) {
    while let Some(event) = stream.next().await {
        if sender.send(event.map_err(|error| error.to_string())).await.is_err() {
            break;
        }
    }
}
