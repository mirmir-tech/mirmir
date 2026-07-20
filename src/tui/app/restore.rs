use super::{
    App,
    load::{LoadDialog, LoadStatus, LoadTarget, RestorePosition},
};
use crate::rpc::{Client, proto};

impl App {
    pub async fn begin_restore(&mut self, client: &mut Client) {
        let response = client.list_active_models(proto::ListActiveModelsRequest {}).await;
        let selectors = match response {
            Ok(response) => response.into_inner().selectors,
            Err(error) => {
                self.action_message = Some(format!("cannot read active models: {error}"));
                return;
            },
        };
        self.restore_queue = selectors
            .into_iter()
            .filter(|selector| !self.models.iter().any(|model| model.id == *selector))
            .collect();
        self.restore_total = self.restore_queue.len();
        self.restore_completed = 0;
        self.start_next_restore(client);
    }

    pub(super) fn poll_restore(&mut self, client: &Client) {
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
