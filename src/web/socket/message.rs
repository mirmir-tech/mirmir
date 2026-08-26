use serde::Serialize;

use super::super::{configuration::Configuration, types};
use crate::application::StartupSnapshot;

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

impl From<StartupSnapshot> for Startup {
    fn from(snapshot: StartupSnapshot) -> Self {
        Self {
            phase: snapshot.phase,
            target: snapshot.target,
            detail: snapshot.detail,
            current: snapshot.current,
            total: snapshot.total,
            ready: snapshot.ready,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_message_has_a_stable_tagged_contract() {
        let message = ServerMessage::Startup {
            startup: StartupSnapshot {
                phase: "loading".to_owned(),
                target: "model".to_owned(),
                detail: "weights".to_owned(),
                current: Some(1),
                total: Some(2),
                ready: false,
            }
            .into(),
        };
        let json = serde_json::to_value(message).expect("message should serialize");
        assert_eq!(json["type"], "startup");
        assert_eq!(json["startup"]["phase"], "loading");
    }
}
