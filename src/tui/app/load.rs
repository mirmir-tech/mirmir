mod dialog;
mod slider;
mod target;

use crossterm::event::{KeyCode, KeyEvent};
pub use dialog::{LoadDialog, LoadStatus, LoadTarget};
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
            let result = crate::tui::string_result(client.inspect_model(request).await)
                .map(tonic::Response::into_inner);
            drop(sender.send(result).await);
        }));
        self.load_dialog = Some(LoadDialog {
            target,
            task: String::new(),
            capabilities: None,
            status: LoadStatus::Inspecting,
            fields: std::array::from_fn(|_| String::new()),
            selected: 0,
            has_mirmir_overrides: false,
            memory: None,
            force: false,
            progress: None,
            error: None,
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
            KeyCode::Left => self.adjust_setting(-1),
            KeyCode::Right => self.adjust_setting(1),
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
        let settings = response.settings;
        let task = if response.task.is_empty() && settings.is_some() {
            "generation".to_owned()
        } else {
            response.task
        };
        if task == "generation" && settings.is_none() {
            self.fail_inspection("server returned no model settings".to_owned());
            return;
        }
        if let Some(dialog) = self.load_dialog.as_mut() {
            dialog.task = task;
            dialog.capabilities = response.capabilities;
            if let Some(settings) = settings {
                dialog.fields = [
                    settings.max_tokens.to_string(),
                    settings.temperature.to_string(),
                    settings.top_p.to_string(),
                    settings.top_k.to_string(),
                    settings.repetition_penalty.to_string(),
                ];
            }
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
        self.restore_in_flight = None;
        self.set_local_state(&request.selector, "loading");
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
        self.restore_in_flight = None;
        self.load_dialog = None;
    }

    pub(super) fn fail_load(&mut self, error: String) {
        let selector = self
            .load_dialog
            .as_ref()
            .map(|dialog| dialog.target.selector.clone())
            .or_else(|| self.restore_in_flight.clone());
        if let Some(selector) = selector {
            self.set_local_state(&selector, "available");
        }
        self.action_message = Some(error.clone());
        if self.restore_in_flight.take().is_none()
            && let Some(dialog) = self.load_dialog.as_mut()
        {
            dialog.status = LoadStatus::Editing;
            dialog.error = Some(error);
        }
        self.lifecycle_rx = None;
    }

    pub(super) fn finish_load_stream(&mut self) {
        self.lifecycle_rx = None;
        if self.restore_in_flight.is_some()
            || self
                .load_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.status == LoadStatus::Loading)
        {
            self.fail_load("model load ended before the model became ready".to_owned());
        }
    }
}

pub(super) async fn forward_lifecycle(
    mut stream: tonic::Streaming<proto::ModelLifecycleEvent>,
    sender: mpsc::Sender<Result<proto::ModelLifecycleEvent, String>>,
) {
    while let Some(event) = stream.next().await {
        if sender.send(crate::tui::string_result(event)).await.is_err() {
            break;
        }
    }
}
