use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

impl App {
    pub fn begin_restore(&mut self, client: &Client) {
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            let result = crate::tui::string_result(
                client.list_active_models(proto::ListActiveModelsRequest {}).await,
            )
            .map(|response| response.into_inner().selectors);
            drop(sender.send(result).await);
        }));
        self.restore_rx = Some(receiver);
    }

    fn apply_restore_list(&mut self, selectors: Vec<String>, client: &Client) {
        self.restore_queue = selectors
            .into_iter()
            .filter(|selector| !self.models.iter().any(|model| model.id == *selector))
            .collect();
        self.start_next_restore(client);
    }

    pub(super) fn poll_restore(&mut self, client: &Client) {
        if let Some(result) = self.restore_rx.as_mut().map(mpsc::Receiver::try_recv) {
            match result {
                Ok(Ok(selectors)) => self.apply_restore_list(selectors, client),
                Ok(Err(error)) => {
                    self.action_message = Some(format!("cannot read active models: {error}"));
                },
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.action_message =
                        Some("cannot read active models: request ended".to_owned());
                },
            }
            self.restore_rx = None;
        }
        if self.lifecycle_rx.is_none()
            && self.restore_in_flight.is_none()
            && self.load_dialog.is_none()
        {
            self.start_next_restore(client);
        }
    }

    fn start_next_restore(&mut self, client: &Client) {
        let Some(selector) = self.restore_queue.pop_front() else {
            return;
        };
        self.restore_in_flight = Some(selector.clone());
        self.set_local_state(&selector, "loading");
        self.start_background_restore(
            client,
            proto::LoadModelRequest { selector, ..Default::default() },
        );
    }

    fn start_background_restore(&mut self, client: &Client, request: proto::LoadModelRequest) {
        let (sender, receiver) = mpsc::channel(64);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            match client.load_model(request).await {
                Ok(response) => super::load::forward_lifecycle(response.into_inner(), sender).await,
                Err(error) => drop(sender.send(Err(error.to_string())).await),
            }
        }));
        self.lifecycle_rx = Some(receiver);
    }
}
