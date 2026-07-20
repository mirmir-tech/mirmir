use crossterm::event::{KeyCode, KeyEvent};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

pub(super) struct ActivityTask(Option<tokio::task::JoinHandle<()>>);

impl ActivityTask {
    const fn new(task: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(task))
    }

    fn abort(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }

    async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            drop(task.await);
        }
    }
}

impl Drop for ActivityTask {
    fn drop(&mut self) {
        self.abort();
    }
}

impl App {
    pub fn begin_activity(&mut self, client: &Client) {
        if let Some(mut task) = self.activity_task.take() {
            task.abort();
        }
        let (sender, receiver) = mpsc::channel(128);
        let mut client = client.clone();
        let task = tokio::spawn(async move {
            let request = proto::WatchActivityRequest { include_history: true };
            let result = client.watch_activity(request).await;
            match result {
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
        });
        self.activity_rx = Some(receiver);
        self.activity_task = Some(ActivityTask::new(task));
    }

    pub fn poll_activity(&mut self) {
        loop {
            let Some(result) = self.activity_rx.as_mut().map(mpsc::Receiver::try_recv) else {
                return;
            };
            match result {
                Ok(Ok(event)) => self.apply_activity(event),
                Ok(Err(error)) => {
                    self.activity_error = Some(error);
                    self.activity_rx = None;
                    return;
                },
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.activity_rx = None;
                    return;
                },
            }
        }
    }

    pub async fn stop_activity(&mut self) {
        if let Some(task) = self.activity_task.take() {
            task.stop().await;
        }
    }

    pub(super) async fn handle_activity_key(&mut self, key: KeyEvent, client: &mut Client) {
        match key.code {
            KeyCode::Up => self.activity_selected = self.activity_selected.saturating_sub(1),
            KeyCode::Down => {
                self.activity_selected = self
                    .activity_selected
                    .saturating_add(1)
                    .min(self.activities.len().saturating_sub(1));
            },
            KeyCode::Char('x' | 'X') => self.cancel_selected_activity(client).await,
            _ => {},
        }
    }

    async fn cancel_selected_activity(&mut self, client: &mut Client) {
        let Some(event) = self.activities.get(self.activity_selected) else {
            return;
        };
        if !event.cancellable
            || !matches!(event.state.as_str(), "queued" | "running" | "cancelling")
        {
            self.activity_error = Some("selected operation cannot be cancelled".to_owned());
            return;
        }
        let request = proto::CancelOperationRequest { operation_id: event.operation_id.clone() };
        match client.cancel_operation(request).await {
            Ok(response) if response.get_ref().accepted => self.activity_error = None,
            Ok(response) => {
                self.activity_error =
                    Some(format!("cancellation was not accepted ({})", response.get_ref().state));
            },
            Err(error) => self.activity_error = Some(error.to_string()),
        }
    }

    fn apply_activity(&mut self, event: proto::ActivityEvent) {
        let selected = self
            .activities
            .get(self.activity_selected)
            .map(|event| event.operation_id.clone());
        if let Some(existing) = self
            .activities
            .iter_mut()
            .find(|existing| existing.operation_id == event.operation_id)
        {
            *existing = event;
        } else {
            self.activities.push(event);
        }
        self.activities.sort_by_key(|event| std::cmp::Reverse(event.updated_at_unix_ms));
        self.activity_selected = selected
            .and_then(|id| self.activities.iter().position(|event| event.operation_id == id))
            .unwrap_or(0)
            .min(self.activities.len().saturating_sub(1));
    }
}
