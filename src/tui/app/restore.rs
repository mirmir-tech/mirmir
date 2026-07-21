use tokio::sync::mpsc;

use super::{
    App,
    load::{LoadDialog, LoadStatus, LoadTarget, RestorePosition},
};
use crate::rpc::{Client, proto};

impl App {
    pub fn begin_restore(&mut self, client: &Client) {
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            let result = client
                .list_active_models(proto::ListActiveModelsRequest {})
                .await
                .map(|response| response.into_inner().selectors)
                .map_err(|error| error.to_string());
            drop(sender.send(result).await);
        }));
        self.restore_rx = Some(receiver);
    }

    fn apply_restore_list(&mut self, selectors: Vec<String>, client: &Client) {
        self.restore_queue = selectors
            .into_iter()
            .filter(|selector| !self.models.iter().any(|model| model.id == *selector))
            .collect();
        self.restore_total = self.restore_queue.len();
        self.restore_completed = 0;
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
        if self.lifecycle_rx.is_none() && self.load_dialog.is_none() {
            self.start_next_restore(client);
        }
    }

    fn start_next_restore(&mut self, client: &Client) {
        let Some(selector) = self.restore_queue.pop_front() else {
            return;
        };
        let position = RestorePosition {
            current: self.restore_completed.saturating_add(1),
            total: self.restore_total,
        };
        self.load_dialog = Some(LoadDialog {
            target: LoadTarget {
                selector: selector.clone(),
                config_id: selector.clone(),
                repo_id: String::new(),
                revision: String::new(),
                commit: String::new(),
            },
            task: String::new(),
            capabilities: None,
            status: LoadStatus::Loading,
            fields: std::array::from_fn(|_| String::new()),
            selected: 0,
            has_mirmir_overrides: true,
            memory: None,
            force: false,
            progress: None,
            error: None,
            restore: Some(position),
        });
        self.start_load_request(client, proto::LoadModelRequest { selector, ..Default::default() });
    }
}
