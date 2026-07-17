use crossterm::event::{KeyCode, KeyEvent};

use super::{App, CatalogFilter};
use crate::rpc::{Client, proto};

impl App {
    pub(super) async fn handle_search_key(&mut self, key: KeyEvent, client: &mut Client) {
        match key.code {
            KeyCode::Esc => self.editing_search = false,
            KeyCode::Enter => {
                self.editing_search = false;
                self.search(client).await;
            },
            KeyCode::Backspace => {
                self.search_query.pop();
                self.reset_search_results();
            },
            KeyCode::Char(character) => {
                self.search_query.push(character);
                self.reset_search_results();
            },
            _ => {},
        }
    }

    async fn search(&mut self, client: &mut Client) {
        if self.search_query.trim().is_empty() {
            return;
        }
        let request = proto::SearchModelsRequest {
            query: self.search_query.trim().to_owned(),
            limit: 25,
            cursor: None,
        };
        match client.search_models(request).await {
            Ok(response) => {
                let response = response.into_inner();
                self.catalog = response.models;
                self.catalog_selected = 0;
                self.catalog_error = None;
                self.catalog_next_cursor = response.next_cursor;
                self.memory_source = response.memory_source;
                self.total_memory_bytes = response.total_memory_bytes;
                self.available_memory_bytes = response.available_memory_bytes;
            },
            Err(error) => self.catalog_error = Some(error.to_string()),
        }
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
                || (model.compatibility == "supported" && model.memory_fit != "does_not_fit")
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
            KeyCode::Char('n' | 'N') => self.load_next_catalog_page(client).await,
            KeyCode::Enter => self.activate_selected_model(client).await,
            KeyCode::Char('d') => self.start_pull(client),
            KeyCode::Char('r') => self.open_remove_dialog(),
            KeyCode::Char('l') => self.open_load_dialog(client),
            KeyCode::Char('u') => self.unload_selected(client).await,
            _ => {},
        }
    }

    async fn load_next_catalog_page(&mut self, client: &mut Client) {
        let Some(cursor) = self.catalog_next_cursor.clone() else {
            self.action_message = Some("no more Hugging Face results".to_owned());
            return;
        };
        let request = proto::SearchModelsRequest {
            query: self.search_query.trim().to_owned(),
            limit: 25,
            cursor: Some(cursor),
        };
        match client.search_models(request).await {
            Ok(response) => {
                let response = response.into_inner();
                self.catalog.extend(response.models);
                self.catalog.sort_by(|left, right| {
                    catalog_rank(left)
                        .cmp(&catalog_rank(right))
                        .then_with(|| right.downloads.cmp(&left.downloads))
                        .then_with(|| left.id.cmp(&right.id))
                });
                self.catalog_next_cursor = response.next_cursor;
                self.action_message = Some(format!("{} search results", self.catalog.len()));
            },
            Err(error) => self.catalog_error = Some(error.to_string()),
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
}

fn catalog_rank(model: &proto::CatalogModel) -> u8 {
    match (model.compatibility.as_str(), model.memory_fit.as_str()) {
        ("supported", "fits") => 0,
        ("supported", "tight") => 1,
        ("supported", "unknown") => 2,
        ("supported", "does_not_fit") => 3,
        ("unknown", _) => 4,
        _ => 5,
    }
}
