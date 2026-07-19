mod error;
mod handlers;
mod media;
mod server;
mod stream;
mod types;

use tokio::sync::watch;

use crate::{rpc::RuntimeService, web::Sessions};

#[derive(Clone)]
pub struct ApiState {
    service: RuntimeService,
    api_key: Option<String>,
    sessions: Sessions,
    shutdown: watch::Receiver<bool>,
}

impl ApiState {
    fn new(
        service: RuntimeService,
        api_key: Option<String>,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        Self {
            service,
            api_key,
            sessions: Sessions::default(),
            shutdown,
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
}

pub use server::start;

#[cfg(test)]
mod tests;
