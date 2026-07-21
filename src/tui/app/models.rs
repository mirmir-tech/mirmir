use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use super::{App, CatalogFilter};
use crate::rpc::{Client, proto};

pub(super) struct CatalogSearchEvent {
    query: String,
    append: bool,
    result: Result<proto::SearchModelsResponse, String>,
}

impl App {
    pub(super) async fn handle_search_key(&mut self, key: KeyEvent, client: &mut Client) {
        match key.code {
            KeyCode::Esc => self.close_search(),
            KeyCode::Enter if self.catalog.is_empty() => self.queue_search(client, false),
            KeyCode::Enter => self.activate_selected_model(client).await,
            KeyCode::Backspace => {
                self.search_query.pop();
                self.reset_search_results();
                self.queue_search(client, false);
            },
            KeyCode::Char(character) => {
                self.search_query.push(character);
                self.reset_search_results();
                self.queue_search(client, false);
            },
            _ => {},
        }
    }

    fn queue_search(&mut self, client: &Client, append: bool) {
        if self.search_query.trim().is_empty() {
            return;
        }
        let query = self.search_query.trim().to_owned();
        let request = proto::SearchModelsRequest {
            query: query.clone(),
            limit: 25,
            cursor: append.then(|| self.catalog_next_cursor.clone()).flatten(),
        };
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            if !append {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            let result = client
                .search_models(request)
                .await
                .map(tonic::Response::into_inner)
                .map_err(|error| error.to_string());
            drop(sender.send(CatalogSearchEvent { query, append, result }).await);
        }));
        self.catalog_search_rx = Some(receiver);
    }

    pub(super) fn poll_catalog_search(&mut self) {
        let Some(result) = self.catalog_search_rx.as_mut().map(mpsc::Receiver::try_recv) else {
            return;
        };
        let event = match result {
            Ok(event) => event,
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.catalog_search_rx = None;
                return;
            },
        };
        self.catalog_search_rx = None;
        if event.query != self.search_query.trim() {
            return;
        }
        match event.result {
            Ok(response) => self.apply_search_response(response, event.append),
            Err(error) => self.catalog_error = Some(error),
        }
    }

    fn apply_search_response(&mut self, response: proto::SearchModelsResponse, append: bool) {
        if append {
            self.catalog.extend(response.models);
        } else {
            self.catalog = response.models;
            self.catalog_selected = 0;
        }
        self.catalog.sort_by(|left, right| {
            catalog_rank(left)
                .cmp(&catalog_rank(right))
                .then_with(|| right.downloads.cmp(&left.downloads))
                .then_with(|| left.id.cmp(&right.id))
        });
        self.catalog_error = None;
        self.catalog_next_cursor = response.next_cursor;
        self.memory_source = response.memory_source;
        self.total_memory_bytes = response.total_memory_bytes;
        self.available_memory_bytes = response.available_memory_bytes;
    }

    fn close_search(&mut self) {
        self.editing_search = false;
        self.search_query.clear();
        self.reset_search_results();
        self.catalog_search_rx = None;
    }

    pub(crate) fn searching_models(&self) -> bool {
        !self.search_query.trim().is_empty()
    }

    pub(crate) fn local_model_count(&self) -> usize {
        self.visible_local_models().count()
    }

    pub(crate) fn visible_local_models(&self) -> impl Iterator<Item = &proto::LocalModelInfo> {
        self.local_models.iter().filter(|model| model.state != "missing")
    }

    pub(crate) fn selected_local_model(&self) -> Option<&proto::LocalModelInfo> {
        self.visible_local_models().nth(self.local_selected)
    }

    pub(crate) fn selected_catalog_model(&self) -> Option<&proto::CatalogModel> {
        self.visible_catalog_models().nth(self.catalog_selected)
    }

    pub(crate) fn visible_catalog_models(&self) -> impl Iterator<Item = &proto::CatalogModel> {
        self.catalog.iter().filter(|model| {
            self.catalog_filter == CatalogFilter::All
                || model.downloaded
                || model.local_source != "remote"
                || (model.compatibility != "unsupported" && model.memory_fit != "does_not_fit")
        })
    }

    pub(crate) fn visible_catalog_count(&self) -> usize {
        self.visible_catalog_models().count()
    }

    pub(super) const fn toggle_incompatible_models(&mut self) {
        self.catalog_filter = match self.catalog_filter {
            CatalogFilter::Compatible => CatalogFilter::All,
            CatalogFilter::All => CatalogFilter::Compatible,
        };
        self.catalog_selected = 0;
    }

    pub(super) async fn handle_models_key(&mut self, key: KeyEvent, client: &mut Client) {
        match key.code {
            KeyCode::Char('/') => self.editing_search = true,
            KeyCode::Char('i' | 'I') => self.toggle_incompatible_models(),
            KeyCode::Enter => self.activate_selected_model(client).await,
            KeyCode::Char('d') => self.start_pull(client),
            KeyCode::Char('x' | 'X') => self.cancel_selected_model_operation(client).await,
            KeyCode::Char('r') => self.open_remove_dialog(),
            KeyCode::Char('l') => self.open_load_dialog(client),
            KeyCode::Char('u') => self.unload_selected(client).await,
            _ => {},
        }
    }

    async fn cancel_selected_model_operation(&mut self, client: &mut Client) {
        let targets = self
            .selected_local_model()
            .map(|model| [model.id.clone(), model.repo_id.clone()]);
        let Some(event) = targets.as_ref().and_then(|targets| {
            self.activities.iter().find(|event| {
                targets.contains(&event.target)
                    && event.cancellable
                    && matches!(event.state.as_str(), "queued" | "running")
            })
        }) else {
            self.action_message = Some("selected model has no cancellable operation".to_owned());
            return;
        };
        let request = proto::CancelOperationRequest { operation_id: event.operation_id.clone() };
        match client.cancel_operation(request).await {
            Ok(response) if response.get_ref().accepted => {
                self.action_message = Some("cancellation requested".to_owned());
            },
            Ok(response) => {
                self.action_message =
                    Some(format!("cancellation rejected: {}", response.get_ref().state));
            },
            Err(error) => self.action_message = Some(error.to_string()),
        }
    }

    pub(super) fn load_more_if_needed(&mut self, client: &Client) {
        if self.searching_models()
            && !self.catalog_loading()
            && self.catalog_next_cursor.is_some()
            && self.catalog_selected.saturating_add(4) >= self.visible_catalog_count()
        {
            self.queue_search(client, true);
        }
    }

    pub(super) fn local_catalog_model(
        &self,
        catalog: &proto::CatalogModel,
    ) -> Option<&proto::LocalModelInfo> {
        self.local_models.iter().find(|local| local.repo_id == catalog.id)
    }

    pub(super) fn reset_search_results(&mut self) {
        self.catalog.clear();
        self.catalog_selected = 0;
        self.catalog_error = None;
        self.catalog_next_cursor = None;
    }

    pub(crate) const fn catalog_loading(&self) -> bool {
        self.catalog_search_rx.is_some()
    }
}

fn catalog_rank(model: &proto::CatalogModel) -> u8 {
    match (model.compatibility.as_str(), model.memory_fit.as_str()) {
        ("supported", "fits") => 0,
        ("supported", "tight") => 1,
        ("supported", "unknown") => 2,
        ("supported", "does_not_fit") => 3,
        ("unknown", "fits") => 4,
        ("unknown", "tight") => 5,
        ("unknown", "unknown") => 6,
        ("unknown", "does_not_fit") => 7,
        _ => 8,
    }
}
