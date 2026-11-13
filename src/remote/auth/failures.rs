use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Failed attempts tolerated from one host within a window before its
/// requests are refused until the window ends.
const MAX_FAILURES: u32 = 20;
const WINDOW: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Failures {
    hosts: Mutex<HashMap<Option<IpAddr>, Window>>,
}

struct Window {
    started: Instant,
    failures: u32,
}

impl Failures {
    pub fn record(&self, host: Option<IpAddr>) {
        let Ok(mut hosts) = self.hosts.lock() else {
            return;
        };
        let now = Instant::now();
        hosts.retain(|_, window| now.duration_since(window.started) < WINDOW);
        let window = hosts.entry(host).or_insert(Window { started: now, failures: 0 });
        window.failures = window.failures.saturating_add(1);
        if window.failures == MAX_FAILURES {
            tracing::warn!(host = ?host, "blocking remote authentication attempts for a minute");
        }
    }

    #[must_use]
    pub fn blocked(&self, host: Option<IpAddr>) -> bool {
        let Ok(hosts) = self.hosts.lock() else {
            return true;
        };
        hosts.get(&host).is_some_and(|window| {
            window.failures >= MAX_FAILURES && window.started.elapsed() < WINDOW
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_a_host_after_repeated_failures_only() {
        let failures = Failures::default();
        let attacker = Some(IpAddr::from([10, 0, 0, 9]));
        let other = Some(IpAddr::from([10, 0, 0, 10]));
        for _ in 0..MAX_FAILURES - 1 {
            failures.record(attacker);
        }
        assert!(!failures.blocked(attacker));
        failures.record(attacker);
        assert!(failures.blocked(attacker));
        assert!(!failures.blocked(other));
    }
}
