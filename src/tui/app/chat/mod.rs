use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

mod image;
mod metrics;
pub(super) mod settings;

pub use metrics::ChatLiveMetrics;
pub use settings::{ChatParameters, ChatSettingsDialog, ChatSettingsStatus};

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub thought: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatStatus {
    Idle,
    Generating,
}

impl App {
    pub(super) fn handle_chat_key(&mut self, key: KeyEvent, client: &Client) {
        match key.code {
            KeyCode::Up => return self.scroll_chat_up(1),
            KeyCode::Down => return self.scroll_chat_down(1),
            KeyCode::PageUp => return self.scroll_chat_up(8),
            KeyCode::PageDown => return self.scroll_chat_down(8),
            KeyCode::Home => return self.chat_scroll = usize::MAX,
            KeyCode::End => return self.chat_scroll = 0,
            _ => {},
        }
        if self.chat_status == ChatStatus::Generating {
            if matches!(key.code, KeyCode::Char('x' | 'X')) {
                self.cancel_chat(client);
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Left => self.previous_chat_model(),
                KeyCode::Right => self.next_chat_model(),
                KeyCode::Char('k' | 'K') => self.clear_chat(),
                KeyCode::Char('d' | 'D') => self.chat_image = None,
                KeyCode::Char('p' | 'P') => self.open_chat_settings(client),
                KeyCode::Char('t' | 'T') => {
                    self.chat_reasoning = self.chat_reasoning.toggled();
                },
                _ => {},
            }
            return;
        }
        match key.code {
            KeyCode::Enter => self.start_chat(client),
            KeyCode::Backspace => {
                self.chat_input.pop();
            },
            KeyCode::Char(character) => self.chat_input.push(character),
            _ => {},
        }
    }

    pub fn poll_chat(&mut self) {
        const MAX_EVENTS_PER_FRAME: usize = 256;
        for _ in 0..MAX_EVENTS_PER_FRAME {
            let Some(result) = self.chat_rx.as_mut().map(mpsc::Receiver::try_recv) else {
                return;
            };
            match result {
                Ok(Ok(event)) => self.apply_chat_event(event),
                Ok(Err(error)) => {
                    self.chat_error = Some(error);
                    self.finish_chat_stream();
                    return;
                },
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.finish_chat_stream();
                    return;
                },
            }
        }
    }

    pub(super) const fn handle_chat_mouse(&mut self, kind: MouseEventKind) {
        match kind {
            MouseEventKind::ScrollUp => self.scroll_chat_up(3),
            MouseEventKind::ScrollDown => self.scroll_chat_down(3),
            MouseEventKind::Down(_) => {
                self.chat_reasoning = self.chat_reasoning.toggled();
            },
            _ => {},
        }
    }

    pub(super) const fn previous_chat_model(&mut self) {
        self.chat_model_index = self.chat_model_index.saturating_sub(1);
    }

    pub(super) fn next_chat_model(&mut self) {
        self.chat_model_index =
            self.chat_model_index.saturating_add(1).min(self.models.len().saturating_sub(1));
    }

    pub(super) fn clear_chat(&mut self) {
        if self.chat_status == ChatStatus::Idle {
            self.chat_messages.clear();
            self.chat_image = None;
            self.chat_scroll = 0;
            self.chat_metrics = None;
            self.chat_live_metrics = None;
            self.chat_error = None;
            self.chat_reasoning = super::ReasoningView::Collapsed;
        }
    }

    pub fn selected_chat_model(&self) -> Option<&proto::ModelInfo> {
        self.models.get(self.chat_model_index)
    }

    fn spawn_chat(&mut self, client: &Client, request: proto::GenerateRequest) {
        let (sender, receiver) = mpsc::channel(128);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            match client.generate(request).await {
                Ok(response) => {
                    let mut stream = response.into_inner();
                    while let Some(event) = stream.next().await {
                        if sender.send(event.map_err(|error| error.to_string())).await.is_err() {
                            break;
                        }
                    }
                },
                Err(error) => drop(sender.send(Err(error.to_string())).await),
            }
        }));
        self.chat_rx = Some(receiver);
        self.chat_status = ChatStatus::Generating;
        self.chat_operation_id = None;
        self.chat_error = None;
        self.chat_metrics = None;
        self.chat_live_metrics = Some(super::ChatLiveMetrics::default());
    }

    fn cancel_chat(&mut self, client: &Client) {
        let Some(operation_id) = self.chat_operation_id.clone() else {
            self.chat_error = Some("generation is starting; try cancellation again".to_owned());
            return;
        };
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            drop(client.cancel_operation(proto::CancelOperationRequest { operation_id }).await);
        }));
        self.chat_error = Some("cancellation requested".to_owned());
    }

    fn apply_chat_event(&mut self, event: proto::GenerateEvent) {
        match event.event {
            Some(proto::generate_event::Event::Started(started)) => {
                self.chat_operation_id = Some(started.operation_id);
            },
            Some(proto::generate_event::Event::Token(token)) => {
                if let Some(message) = self.chat_messages.last_mut() {
                    if token.reasoning {
                        message.thought.push_str(&token.text);
                    } else {
                        message.content.push_str(&token.text);
                    }
                }
            },
            Some(proto::generate_event::Event::Completion(completion)) => {
                if let Some(message) = self.chat_messages.last_mut() {
                    message.content.clone_from(&completion.text);
                    message.thought.clone_from(&completion.reasoning);
                }
                self.chat_metrics = Some(completion);
                self.chat_live_metrics = None;
                self.chat_status = ChatStatus::Idle;
                self.chat_operation_id = None;
            },
            None => {},
        }
    }

    fn finish_chat_stream(&mut self) {
        self.chat_rx = None;
        self.chat_status = ChatStatus::Idle;
        self.chat_operation_id = None;
        self.chat_live_metrics = None;
    }

    const fn scroll_chat_up(&mut self, lines: usize) {
        self.chat_scroll = self.chat_scroll.saturating_add(lines);
    }

    const fn scroll_chat_down(&mut self, lines: usize) {
        self.chat_scroll = self.chat_scroll.saturating_sub(lines);
    }
}

#[cfg(test)]
mod tests;
