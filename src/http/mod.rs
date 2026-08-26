mod error;
mod handlers;
mod media;
mod server;
mod stream;
mod task;
mod types;

use tokio::sync::{broadcast, watch};

use crate::{application::Application, web::Sessions};

#[derive(Clone)]
pub struct ApiState {
    application: Application,
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
        application: Application,
        api_key: Option<String>,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        let (updates, _receiver) = broadcast::channel(32);
        Self {
            application,
            api_key,
            sessions: Sessions::default(),
            shutdown,
            updates,
        }
    }

    pub const fn application(&self) -> &Application {
        &self.application
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
