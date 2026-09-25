use libmir::ProgressCount;
use tokio::sync::watch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartupStatus {
    Starting {
        detail: String,
    },
    Restoring {
        total: u64,
        detail: String,
    },
    Loading {
        target: String,
        detail: String,
        progress: Option<ProgressCount>,
    },
    Ready {
        detail: String,
    },
    Failed {
        detail: String,
    },
}

impl StartupStatus {
    #[must_use]
    pub const fn phase(&self) -> &'static str {
        match self {
            Self::Starting { .. } => "starting",
            Self::Restoring { .. } => "restoring",
            Self::Loading { .. } => "loading",
            Self::Ready { .. } => "ready",
            Self::Failed { .. } => "failed",
        }
    }

    #[must_use]
    pub fn target(&self) -> &str {
        match self {
            Self::Starting { .. } | Self::Ready { .. } => "runtime",
            Self::Restoring { .. } => "active models",
            Self::Loading { target, .. } => target,
            Self::Failed { .. } => "runtime startup",
        }
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::Starting { detail }
            | Self::Restoring { detail, .. }
            | Self::Loading { detail, .. }
            | Self::Ready { detail }
            | Self::Failed { detail } => detail,
        }
    }

    #[must_use]
    pub const fn progress(&self) -> Option<ProgressCount> {
        match self {
            Self::Restoring { total, .. } => Some(ProgressCount::new(0, *total)),
            Self::Loading { progress, .. } => *progress,
            Self::Starting { .. } | Self::Ready { .. } | Self::Failed { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

#[derive(Clone)]
pub struct Startup {
    state: watch::Sender<StartupStatus>,
}

impl Startup {
    pub fn new() -> Self {
        let (state, _receiver) = watch::channel(StartupStatus::Starting {
            detail: "preparing runtime services".to_owned(),
        });
        Self { state }
    }

    pub fn status(&self) -> StartupStatus {
        self.state.borrow().clone()
    }

    pub fn ensure_ready(&self) -> super::Result<()> {
        if self.status().is_ready() {
            Ok(())
        } else {
            Err(super::Error::RuntimeNotReady)
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<StartupStatus> {
        self.state.subscribe()
    }

    pub fn restoring(&self, total: usize) {
        self.state.send_replace(StartupStatus::Restoring {
            total: u64::try_from(total).unwrap_or(u64::MAX),
            detail: if total == 0 {
                "checking saved runtime state".to_owned()
            } else {
                "restoring saved models".to_owned()
            },
        });
    }

    pub fn loading(&self, target: &str, detail: &str, progress: Option<ProgressCount>) {
        self.state.send_replace(StartupStatus::Loading {
            target: target.to_owned(),
            detail: detail.to_owned(),
            progress,
        });
    }

    pub fn ready(&self, detail: impl Into<String>) {
        self.state.send_replace(StartupStatus::Ready { detail: detail.into() });
    }

    pub fn failed(&self, detail: impl Into<String>) {
        self.state.send_replace(StartupStatus::Failed { detail: detail.into() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_loading_and_ready_states() {
        let startup = Startup::new();
        assert!(matches!(startup.ensure_ready(), Err(super::super::Error::RuntimeNotReady)));
        let mut receiver = startup.subscribe();
        startup.loading("model", "weights", Some(ProgressCount::new(2, 4)));
        assert!(startup.ensure_ready().is_err());
        assert!(receiver.has_changed().expect("startup sender should remain alive"));
        assert!(matches!(&*receiver.borrow_and_update(), StartupStatus::Loading { .. }));
        startup.ready("runtime ready");
        assert!(startup.status().is_ready());
        assert!(startup.ensure_ready().is_ok());
        startup.failed("restoration failed");
        assert!(startup.ensure_ready().is_err());
    }
}
