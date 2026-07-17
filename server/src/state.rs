use std::{sync::Arc, time::SystemTime};

use libmir::{Library, Model, RuntimeConfig};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    started_at: SystemTime,
    library: Library,
    model: Option<Model>,
}

impl AppState {
    #[must_use]
    pub fn new(library: Library, model: Option<Model>) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                started_at: SystemTime::now(),
                library,
                model,
            }),
        }
    }

    #[must_use]
    pub fn started_at(&self) -> SystemTime {
        self.inner.started_at
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        &self.inner.library
    }

    #[must_use]
    pub fn model(&self) -> Option<Model> {
        self.inner.model.clone()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(Library::new(RuntimeConfig::default()), None)
    }
}
