use serde::Serialize;

use super::super::{configuration::Configuration, types};
use crate::application::StartupStatus;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Startup { startup: Startup },
    Overview { overview: Box<types::Overview> },
    Models { models: types::Models },
    Configuration { configuration: Configuration },
    Activity { activity: types::Activity },
    Error { message: String },
}

#[derive(Serialize)]
pub struct Startup {
    phase: String,
    target: String,
    detail: String,
    current: Option<u64>,
    total: Option<u64>,
    ready: bool,
}

impl From<StartupStatus> for Startup {
    fn from(status: StartupStatus) -> Self {
        let progress = status.progress();
        Self {
            phase: status.phase().to_owned(),
            target: status.target().to_owned(),
            detail: status.detail().to_owned(),
            current: progress.map(libmir::ProgressCount::current),
            total: progress.map(libmir::ProgressCount::total),
            ready: status.is_ready(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_message_has_a_stable_tagged_contract() {
        let message = ServerMessage::Startup {
            startup: StartupStatus::Loading {
                target: "model".to_owned(),
                detail: "weights".to_owned(),
                progress: Some(libmir::ProgressCount::new(1, 2)),
            }
            .into(),
        };
        let json = serde_json::to_value(message).expect("message should serialize");
        assert_eq!(json["type"], "startup");
        assert_eq!(json["startup"]["phase"], "loading");
    }
}
