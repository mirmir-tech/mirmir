mod error;
mod handlers;
mod media;
mod server;
mod stream;
mod types;

use tokio::sync::{broadcast, watch};

use crate::{rpc::RuntimeService, web::Sessions};

#[derive(Clone)]
pub struct ApiState {
    service: RuntimeService,
    api_key: Option<String>,
    sessions: Sessions,
    shutdown: watch::Receiver<bool>,
    updates: broadcast::Sender<DashboardUpdate>,
}

#[derive(Clone, Copy)]
pub enum DashboardUpdate {
    Configuration,
}

impl ApiState {
    fn new(
        service: RuntimeService,
        api_key: Option<String>,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        let (updates, _receiver) = broadcast::channel(32);
        Self {
            service,
            api_key,
            sessions: Sessions::default(),
            shutdown,
            updates,
        }
    }

    pub const fn service(&self) -> &RuntimeService {
        &self.service
    }

    pub const fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    pub fn shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown.clone()
    }

    pub fn updates(&self) -> broadcast::Receiver<DashboardUpdate> {
        self.updates.subscribe()
    }

    pub fn configuration_changed(&self) {
        drop(self.updates.send(DashboardUpdate::Configuration));
    }
}

pub use server::start;

#[cfg(test)]
mod tests;
