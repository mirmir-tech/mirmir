use tokio::sync::watch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub phase: String,
    pub target: String,
    pub detail: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
    pub ready: bool,
}

#[derive(Clone)]
pub struct Startup {
    state: watch::Sender<Snapshot>,
}

impl Startup {
    pub fn new() -> Self {
        let (state, _receiver) = watch::channel(Snapshot {
            phase: "starting".to_owned(),
            target: "runtime".to_owned(),
            detail: "preparing runtime services".to_owned(),
            current: None,
            total: None,
            ready: false,
        });
        Self { state }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.state.subscribe()
    }

    pub fn restoring(&self, total: usize) {
        self.publish(
            "restoring",
            "active models",
            if total == 0 {
                "checking saved runtime state"
            } else {
                "restoring saved models"
            },
            Some(0),
            Some(u64::try_from(total).unwrap_or(u64::MAX)),
            false,
        );
    }

    pub fn loading(&self, target: &str, detail: &str, current: Option<u64>, total: Option<u64>) {
        self.publish("loading", target, detail, current, total, false);
    }

    pub fn ready(&self, detail: impl Into<String>) {
        self.publish("ready", "runtime", detail, None, None, true);
    }

    pub fn failed(&self, detail: impl Into<String>) {
        self.publish("failed", "runtime startup", detail, None, None, false);
    }

    fn publish(
        &self,
        phase: &str,
        target: &str,
        detail: impl Into<String>,
        current: Option<u64>,
        total: Option<u64>,
        ready: bool,
    ) {
        self.state.send_replace(Snapshot {
            phase: phase.to_owned(),
            target: target.to_owned(),
            detail: detail.into(),
            current,
            total,
            ready,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_loading_and_ready_states() {
        let startup = Startup::new();
        let mut receiver = startup.subscribe();
        startup.loading("model", "weights", Some(2), Some(4));
        assert!(receiver.has_changed().expect("startup sender should remain alive"));
        assert_eq!(receiver.borrow_and_update().phase, "loading");
        startup.ready("runtime ready");
        assert!(startup.snapshot().ready);
    }
}
