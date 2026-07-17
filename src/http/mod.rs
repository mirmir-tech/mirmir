mod error;
mod handlers;
mod server;
mod stream;
mod types;

use crate::{rpc::RuntimeService, web::Sessions};

#[derive(Clone)]
pub struct ApiState {
    service: RuntimeService,
    api_key: Option<String>,
    sessions: Sessions,
}

impl ApiState {
    fn new(service: RuntimeService, api_key: Option<String>) -> Self {
        Self {
            service,
            api_key,
            sessions: Sessions::default(),
        }
    }

    pub const fn service(&self) -> &RuntimeService {
        &self.service
    }

    pub const fn sessions(&self) -> &Sessions {
        &self.sessions
    }
}

pub use server::start;

#[cfg(test)]
mod tests;
