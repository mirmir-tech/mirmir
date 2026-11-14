//! Throttling of failed authentication attempts per peer network.
//!
//! IPv4 peers are counted per address, so clients behind one NAT share a
//! budget. IPv6 peers are counted per /64, the smallest prefix a host is
//! usually given, so one host cannot reset its budget by rotating addresses;
//! the trade-off is that hosts sharing a /64 share a budget too. Neither ever
//! blocks a valid key: the throttle applies only to attempts that fail.

use std::{
    collections::{HashMap, hash_map::Entry},
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Failed attempts tolerated from one network within a window; later failed
/// attempts are refused until the window ends.
pub const MAX_FAILURES: u32 = 20;
const WINDOW: Duration = Duration::from_secs(60);
/// Networks tracked at once. Beyond it, new networks are not tracked until
/// expired windows are pruned, which bounds memory under address rotation.
const MAX_NETWORKS: usize = 4_096;

pub struct Failures {
    state: Mutex<State>,
}

struct State {
    networks: HashMap<IpAddr, Window>,
    /// When expired windows were last pruned; pruning runs at most once per
    /// window instead of on every attempt.
    pruned: Instant,
}

struct Window {
    started: Instant,
    failures: u32,
}

impl Default for Failures {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                networks: HashMap::new(),
                pruned: Instant::now(),
            }),
        }
    }
}

impl Failures {
    /// Counts a failed attempt from `host` and reports whether its network
    /// has exceeded its budget for the current window.
    pub fn record(&self, host: IpAddr) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return true;
        };
        let now = Instant::now();
        if now.duration_since(state.pruned) >= WINDOW {
            state.networks.retain(|_, window| now.duration_since(window.started) < WINDOW);
            state.pruned = now;
        }
        let full = state.networks.len() >= MAX_NETWORKS;
        let window = match state.networks.entry(network(host)) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) if !full => entry.insert(Window { started: now, failures: 0 }),
            Entry::Vacant(_) => return false,
        };
        if now.duration_since(window.started) >= WINDOW {
            *window = Window { started: now, failures: 0 };
        }
        window.failures = window.failures.saturating_add(1);
        if window.failures == MAX_FAILURES {
            tracing::warn!(%host, "refusing failed remote authentication attempts for a minute");
        }
        window.failures > MAX_FAILURES
    }
}

/// The network a peer's failures count against.
fn network(host: IpAddr) -> IpAddr {
    match host {
        IpAddr::V4(_) => host,
        IpAddr::V6(address) => address.to_ipv4_mapped().map_or_else(
            || IpAddr::V6((u128::from(address) & !u128::from(u64::MAX)).into()),
            IpAddr::V4,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    fn exhaust(failures: &Failures, host: IpAddr) -> bool {
        (0..MAX_FAILURES).fold(false, |blocked, _| failures.record(host) || blocked)
    }

    #[test]
    fn refuses_a_network_only_after_its_budget() {
        let failures = Failures::default();
        let attacker = IpAddr::from([10, 0, 0, 9]);
        assert!(!exhaust(&failures, attacker), "the budget itself is not refused");
        assert!(failures.record(attacker));
        assert!(!failures.record(IpAddr::from([10, 0, 0, 10])));
    }

    #[test]
    fn groups_ipv6_peers_by_their_64_prefix() {
        let failures = Failures::default();
        let host = |last: u16| IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 1, 0, 0, 0, last));
        for last in 0..u16::try_from(MAX_FAILURES).expect("small") {
            assert!(!failures.record(host(last)));
        }
        assert!(failures.record(host(999)), "rotating within the /64 does not help");
        let elsewhere = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 2, 0, 0, 0, 1));
        assert!(!failures.record(elsewhere));
        let mapped = IpAddr::V6(std::net::Ipv4Addr::new(10, 0, 0, 9).to_ipv6_mapped());
        assert_eq!(network(mapped), IpAddr::from([10, 0, 0, 9]));
    }

    #[test]
    fn tracks_a_bounded_number_of_networks() {
        let failures = Failures::default();
        for index in 0..u32::try_from(MAX_NETWORKS).expect("small") + 10 {
            failures.record(IpAddr::from(index.to_be_bytes()));
        }
        let state = failures.state.lock().expect("unpoisoned");
        assert_eq!(state.networks.len(), MAX_NETWORKS);
    }
}
