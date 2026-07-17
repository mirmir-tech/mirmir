use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use crate::rpc::{Client, proto};

#[derive(Debug, Clone)]
pub enum ConfigurationTarget {
    Value(String),
    HuggingFaceToken,
    HttpApiKey,
}

#[derive(Debug, Clone)]
pub struct ConfigurationEdit {
    pub target: ConfigurationTarget,
    pub input: String,
}

impl ConfigurationEdit {
    #[must_use]
    pub const fn secret(&self) -> bool {
        matches!(
            self.target,
            ConfigurationTarget::HuggingFaceToken | ConfigurationTarget::HttpApiKey
        )
    }
}

impl App {
    pub(super) async fn handle_configuration_key(&mut self, key: KeyEvent, client: &mut Client) {
        match key.code {
            KeyCode::Up => self.previous_configuration(),
            KeyCode::Down => self.next_configuration(),
            KeyCode::Enter => self.begin_configuration_edit(),
            KeyCode::Char('t') if self.configuration_selected == 0 => {
                self.update_configuration(client, test_hf_token()).await;
            },
            KeyCode::Char('r') if self.configuration_selected < 2 => {
                let operation = if self.configuration_selected == 0 {
                    remove_hf_token()
                } else {
                    remove_http_api_key()
                };
                self.update_configuration(client, operation).await;
            },
            _ => {},
        }
    }

    pub(super) async fn handle_configuration_edit(&mut self, key: KeyEvent, client: &mut Client) {
        match key.code {
            KeyCode::Esc => self.configuration_edit = None,
            KeyCode::Backspace => {
                if let Some(edit) = self.configuration_edit.as_mut() {
                    edit.input.pop();
                }
            },
            KeyCode::Char(character) => {
                if let Some(edit) = self.configuration_edit.as_mut() {
                    edit.input.push(character);
                }
            },
            KeyCode::Enter => self.submit_configuration_edit(client).await,
            _ => {},
        }
    }

    pub(super) fn begin_configuration_edit(&mut self) {
        let target = match self.configuration_selected {
            0 => ConfigurationTarget::HuggingFaceToken,
            1 => ConfigurationTarget::HttpApiKey,
            selected => {
                let Some(value) = self
                    .configuration
                    .as_ref()
                    .and_then(|config| config.values.get(selected.saturating_sub(2)))
                    .filter(|value| value.editable)
                else {
                    self.configuration_message = Some("selected value is read-only".to_owned());
                    return;
                };
                ConfigurationTarget::Value(value.key.clone())
            },
        };
        let input = match &target {
            ConfigurationTarget::Value(key) => self
                .configuration
                .as_ref()
                .and_then(|config| config.values.iter().find(|value| &value.key == key))
                .map_or_else(String::new, |value| value.value.clone()),
            ConfigurationTarget::HuggingFaceToken | ConfigurationTarget::HttpApiKey => {
                String::new()
            },
        };
        self.configuration_edit = Some(ConfigurationEdit { target, input });
        self.configuration_message = None;
    }

    async fn submit_configuration_edit(&mut self, client: &mut Client) {
        let Some(edit) = self.configuration_edit.take() else {
            return;
        };
        let operation = match edit.target {
            ConfigurationTarget::Value(key) => {
                proto::update_configuration_request::Operation::SetValue(
                    proto::SetConfigurationValue { key, value: edit.input },
                )
            },
            ConfigurationTarget::HuggingFaceToken => {
                proto::update_configuration_request::Operation::SetHfToken(proto::SetHfToken {
                    token: edit.input,
                })
            },
            ConfigurationTarget::HttpApiKey => {
                proto::update_configuration_request::Operation::SetHttpApiKey(
                    proto::SetHttpApiKey { key: edit.input },
                )
            },
        };
        self.update_configuration(client, operation).await;
    }

    async fn update_configuration(
        &mut self,
        client: &mut Client,
        operation: proto::update_configuration_request::Operation,
    ) {
        match client
            .update_configuration(proto::UpdateConfigurationRequest { operation: Some(operation) })
            .await
        {
            Ok(response) => {
                let response = response.into_inner();
                self.configuration = response.configuration;
                self.configuration_message = Some(if response.restart_required {
                    format!("{}; restart required", response.message)
                } else {
                    response.message
                });
            },
            Err(error) => self.configuration_message = Some(error.to_string()),
        }
    }

    fn previous_configuration(&mut self) {
        let count = self.configuration_count();
        self.configuration_selected = self
            .configuration_selected
            .checked_sub(1)
            .unwrap_or_else(|| count.saturating_sub(1));
    }

    fn next_configuration(&mut self) {
        let count = self.configuration_count();
        if count > 0 {
            self.configuration_selected = (self.configuration_selected + 1) % count;
        }
    }

    pub fn configuration_count(&self) -> usize {
        self.configuration.as_ref().map_or(2, |config| config.values.len() + 2)
    }
}

const fn test_hf_token() -> proto::update_configuration_request::Operation {
    proto::update_configuration_request::Operation::TestHfToken(proto::TestHfToken {})
}

const fn remove_hf_token() -> proto::update_configuration_request::Operation {
    proto::update_configuration_request::Operation::RemoveHfToken(proto::RemoveHfToken {})
}

const fn remove_http_api_key() -> proto::update_configuration_request::Operation {
    proto::update_configuration_request::Operation::RemoveHttpApiKey(proto::RemoveHttpApiKey {})
}
