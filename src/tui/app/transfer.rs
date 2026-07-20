use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::App;
use crate::rpc::{Client, proto};

impl App {
    pub(super) fn start_pull(&mut self, client: &Client) {
        if !self.searching_models() {
            self.action_message = Some("search Hugging Face before downloading a model".to_owned());
            return;
        }
        let Some(model) = self.selected_catalog_model() else {
            return;
        };
        if model.downloaded {
            self.action_message = Some("model is already downloaded".to_owned());
            return;
        }
        if model.local_source == "hf_cache" {
            self.action_message =
                Some("model is already available in the external HF cache".to_owned());
            return;
        }
        if model.compatibility == "unsupported" {
            self.action_message =
                Some("this remote weight format cannot be inspected by libmir".to_owned());
            return;
        }
        if model.memory_fit == "does_not_fit" {
            self.action_message =
                Some("estimated model memory exceeds the current device budget".to_owned());
            return;
        }
        let repo_id = model.id.clone();
        let request = proto::PullModelRequest { repo_id: repo_id.clone(), revision: None };
        let sender = self.transfer_tx.clone();
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            match client.pull_model(request).await {
                Ok(response) => forward(response.into_inner(), sender).await,
                Err(error) => drop(sender.send(Err(error.to_string())).await),
            }
        }));
        self.transfer_repo = Some(repo_id);
        self.transfer_phase = Some("queued".to_owned());
        self.transfer_downloaded_bytes = 0;
        self.transfer_total_bytes = None;
        self.action_message = None;
    }

    pub fn poll_operations(&mut self, client: &Client) {
        self.poll_load_settings();
        self.poll_download();
        self.poll_removal();
        self.poll_lifecycle();
        self.poll_restore(client);
    }

    fn poll_download(&mut self) {
        loop {
            match self.transfer_rx.try_recv() {
                Ok(Ok(event)) => self.apply_transfer(event),
                Ok(Err(error)) => self.action_message = Some(error),
                Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                    break;
                },
            }
        }
    }

    fn poll_lifecycle(&mut self) {
        let Some(result) = self.lifecycle_rx.as_mut().map(mpsc::Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(event)) => self.apply_load_event(event),
            Ok(Err(error)) => self.fail_load(error),
            Err(mpsc::error::TryRecvError::Empty) => {},
            Err(mpsc::error::TryRecvError::Disconnected) => self.finish_load_stream(),
        }
    }

    fn apply_transfer(&mut self, event: proto::ModelTransferEvent) {
        self.transfer_repo = Some(event.repo_id.clone());
        self.transfer_phase = Some(event.phase.clone());
        if event.downloaded_bytes > 0 {
            self.transfer_downloaded_bytes = event.downloaded_bytes;
        }
        self.transfer_total_bytes = event.total_bytes.or(self.transfer_total_bytes);
        self.action_message = Some(event.message);
        if event.phase == "available"
            && let Some(model) = self.catalog.iter_mut().find(|model| model.id == event.repo_id)
        {
            model.downloaded = true;
            "mirmir".clone_into(&mut model.local_source);
        }
    }
}

async fn forward(
    mut stream: tonic::Streaming<proto::ModelTransferEvent>,
    sender: mpsc::Sender<Result<proto::ModelTransferEvent, String>>,
) {
    while let Some(event) = stream.next().await {
        let event = event.map_err(|error| error.to_string());
        if sender.send(event).await.is_err() {
            break;
        }
    }
}
