mod dialog;

use crossterm::event::{KeyCode, KeyEvent};
use dialog::{CHAT_FIELD_COUNT, settings_fields};
pub use dialog::{ChatParameters, ChatSettingsDialog, ChatSettingsStatus};
use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

pub(super) enum ChatSettingsEvent {
    Inspected(proto::InspectModelResponse),
    Saved(proto::UpdateModelGenerationResponse),
}

impl App {
    pub(super) fn open_chat_settings(&mut self, client: &Client) {
        let Some(model) = self.selected_chat_model().map(|model| model.id.clone()) else {
            self.chat_error = Some("load a model before configuring chat".to_owned());
            return;
        };
        if let Some(parameters) = self.chat_parameters.as_ref().filter(|value| value.model == model)
        {
            self.chat_settings_dialog = Some(ChatSettingsDialog::from_parameters(parameters));
            return;
        }
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        let selector = model.clone();
        drop(tokio::spawn(async move {
            let result = client
                .inspect_model(proto::InspectModelRequest { selector })
                .await
                .map(tonic::Response::into_inner)
                .map(ChatSettingsEvent::Inspected)
                .map_err(|error| error.to_string());
            drop(sender.send(result).await);
        }));
        self.chat_settings_dialog = Some(ChatSettingsDialog::inspecting(model));
        self.chat_settings_rx = Some(receiver);
    }

    pub(super) fn handle_chat_settings_key(&mut self, key: KeyEvent, client: &Client) {
        let Some(status) = self.chat_settings_dialog.as_ref().map(|dialog| dialog.status) else {
            return;
        };
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('x' | 'X')) {
            self.chat_settings_dialog = None;
            self.chat_settings_rx = None;
            return;
        }
        if status != ChatSettingsStatus::Editing {
            return;
        }
        match key.code {
            KeyCode::Enter => self.apply_chat_settings(client),
            KeyCode::Char('s' | 'S') => {
                if let Some(dialog) = self.chat_settings_dialog.as_mut() {
                    dialog.save_default = !dialog.save_default;
                }
            },
            KeyCode::Up => self.move_chat_setting(false),
            KeyCode::Down | KeyCode::Tab => self.move_chat_setting(true),
            KeyCode::Backspace => self.edit_chat_setting(None),
            KeyCode::Char(character) if character.is_ascii_digit() || character == '.' => {
                self.edit_chat_setting(Some(character));
            },
            _ => {},
        }
    }

    pub fn poll_chat_settings(&mut self) {
        let Some(result) = self.chat_settings_rx.as_mut().map(mpsc::Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(ChatSettingsEvent::Inspected(response))) => self.apply_chat_inspection(&response),
            Ok(Ok(ChatSettingsEvent::Saved(response))) => {
                self.chat_error = Some(format!("defaults saved for {}", response.model_id));
                self.chat_settings_dialog = None;
            },
            Ok(Err(error)) => self.fail_chat_settings(error),
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.fail_chat_settings("chat settings request ended unexpectedly".to_owned());
            },
        }
        self.chat_settings_rx = None;
    }

    pub(super) fn apply_chat_parameters(&self, request: &mut proto::GenerateRequest) {
        let Some(value) =
            self.chat_parameters.as_ref().filter(|value| value.model == request.model)
        else {
            return;
        };
        request.max_tokens = Some(value.max_tokens);
        request.temperature = Some(value.temperature);
        request.top_p = Some(value.top_p);
        request.top_k = Some(value.top_k);
        request.repetition_penalty = Some(value.repetition_penalty);
        request.seed = value.seed;
    }

    fn apply_chat_inspection(&mut self, response: &proto::InspectModelResponse) {
        let Some(settings) = response.settings.as_ref() else {
            self.fail_chat_settings("server returned no model settings".to_owned());
            return;
        };
        if let Some(dialog) = self.chat_settings_dialog.as_mut() {
            dialog.fields = settings_fields(settings, None);
            dialog.persisted = response.has_mirmir_overrides;
            dialog.status = ChatSettingsStatus::Editing;
            dialog.error = None;
        }
    }

    fn apply_chat_settings(&mut self, client: &Client) {
        let parameters =
            match self.chat_settings_dialog.as_ref().map(ChatSettingsDialog::parameters) {
                Some(Ok(parameters)) => parameters,
                Some(Err(error)) => return self.fail_chat_settings(error),
                None => return,
            };
        self.chat_parameters = Some(parameters.clone());
        if !self.chat_settings_dialog.as_ref().is_some_and(|dialog| dialog.save_default) {
            self.chat_settings_dialog = None;
            return;
        }
        let request = proto::UpdateModelGenerationRequest {
            selector: parameters.model.clone(),
            settings: Some(parameters.settings()),
        };
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            let result = client
                .update_model_generation(request)
                .await
                .map(tonic::Response::into_inner)
                .map(ChatSettingsEvent::Saved)
                .map_err(|error| error.to_string());
            drop(sender.send(result).await);
        }));
        if let Some(dialog) = self.chat_settings_dialog.as_mut() {
            dialog.status = ChatSettingsStatus::Saving;
            dialog.error = None;
        }
        self.chat_settings_rx = Some(receiver);
    }

    fn fail_chat_settings(&mut self, error: String) {
        if let Some(dialog) = self.chat_settings_dialog.as_mut() {
            dialog.status = ChatSettingsStatus::Editing;
            dialog.error = Some(error);
        }
    }

    fn move_chat_setting(&mut self, forward: bool) {
        if let Some(dialog) = self.chat_settings_dialog.as_mut() {
            dialog.selected = if forward {
                dialog.selected.saturating_add(1).min(CHAT_FIELD_COUNT - 1)
            } else {
                dialog.selected.saturating_sub(1)
            };
        }
    }

    fn edit_chat_setting(&mut self, character: Option<char>) {
        if let Some(dialog) = self.chat_settings_dialog.as_mut() {
            let field = &mut dialog.fields[dialog.selected];
            if let Some(character) = character {
                field.push(character);
            } else {
                let _removed = field.pop();
            }
            dialog.error = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_parameters_only_to_the_matching_model() {
        let mut app = App::new(true);
        app.chat_parameters = Some(ChatParameters {
            model: "selected".to_owned(),
            max_tokens: 64,
            temperature: 0.5,
            top_p: 0.8,
            top_k: 20,
            repetition_penalty: 1.1,
            seed: Some(42),
        });
        let mut request = proto::GenerateRequest {
            model: "selected".to_owned(),
            ..Default::default()
        };
        app.apply_chat_parameters(&mut request);
        assert_eq!(request.max_tokens, Some(64));
        assert_eq!(request.seed, Some(42));
        request.model = "different".to_owned();
        request.max_tokens = None;
        app.apply_chat_parameters(&mut request);
        assert_eq!(request.max_tokens, None);
    }
}
