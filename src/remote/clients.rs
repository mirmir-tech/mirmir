use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
};

use super::{KeyRecord, Scope, keys::now_unix_ms};

/// How long a client stays listed after its last request.
const RECENT_MS: u64 = 15 * 60 * 1_000;
/// How often clients older than [`RECENT_MS`] are pruned at most.
const PRUNE_EVERY_MS: u64 = 60 * 1_000;
/// Clients kept at once; the least recently seen one makes room beyond it.
const MAX_CLIENTS: usize = 1_024;

/// A remote client seen recently: one per key and peer host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub key_id: String,
    pub key_name: String,
    pub scope: Scope,
    pub peer: SocketAddr,
    pub first_seen_unix_ms: u64,
    pub last_seen_unix_ms: u64,
    pub requests: u64,
}

#[derive(Default)]
pub struct Clients {
    entries: Mutex<Entries>,
}

#[derive(Default)]
struct Entries {
    clients: HashMap<(String, IpAddr), Client>,
    /// Each key's last request, kept apart from the pruned client list so it
    /// survives until the key is revoked or the server restarts.
    last_used: HashMap<String, u64>,
    pruned_unix_ms: u64,
}

impl Clients {
    pub fn record(&self, key: &KeyRecord, peer: SocketAddr) {
        let now = now_unix_ms();
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.prune(now);
        entries.last_used.insert(key.id.clone(), now);
        let identity = (key.id.clone(), peer.ip());
        if !entries.clients.contains_key(&identity) {
            entries.make_room();
            tracing::info!(key = %key.name, scope = key.scope.as_str(), %peer, "remote client connected");
        }
        let client = entries.clients.entry(identity).or_insert_with(|| Client {
            key_id: key.id.clone(),
            key_name: key.name.clone(),
            scope: key.scope,
            peer,
            first_seen_unix_ms: now,
            last_seen_unix_ms: now,
            requests: 0,
        });
        client.peer = peer;
        client.last_seen_unix_ms = now;
        client.requests += 1;
    }

    /// Clients seen in the last 15 minutes, most recent first.
    #[must_use]
    pub fn recent(&self) -> Vec<Client> {
        let cutoff = now_unix_ms().saturating_sub(RECENT_MS);
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        let mut recent: Vec<Client> = entries
            .clients
            .values()
            .filter(|client| client.last_seen_unix_ms >= cutoff)
            .cloned()
            .collect();
        recent.sort_by_key(|client| std::cmp::Reverse(client.last_seen_unix_ms));
        recent
    }

    #[must_use]
    pub fn last_used(&self, key_id: &str) -> Option<u64> {
        self.entries.lock().ok()?.last_used.get(key_id).copied()
    }

    pub fn forget_key(&self, key_id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clients.retain(|(id, _), _| id != key_id);
            entries.last_used.remove(key_id);
        }
    }
}

impl Entries {
    /// Drops clients no longer recent, at most once per [`PRUNE_EVERY_MS`].
    fn prune(&mut self, now: u64) {
        if now.saturating_sub(self.pruned_unix_ms) < PRUNE_EVERY_MS {
            return;
        }
        let cutoff = now.saturating_sub(RECENT_MS);
        self.clients.retain(|_, client| client.last_seen_unix_ms >= cutoff);
        self.pruned_unix_ms = now;
    }

    /// Evicts the least recently seen client when the list is full.
    fn make_room(&mut self) {
        if self.clients.len() < MAX_CLIENTS {
            return;
        }
        let oldest = self
            .clients
            .iter()
            .min_by_key(|(_, client)| client.last_seen_unix_ms)
            .map(|(identity, _)| identity.clone());
        if let Some(oldest) = oldest {
            self.clients.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str) -> KeyRecord {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": id,
            "scope": "use",
            "created_at_unix_ms": 0,
            "digest": "",
        }))
        .expect("a key record")
    }

    #[test]
    fn keeps_a_bounded_client_list_and_each_keys_last_use() {
        let clients = Clients::default();
        let first = key("first");
        clients.record(&first, SocketAddr::from(([10, 0, 0, 1], 1)));
        let other = key("other");
        for index in 0..u32::try_from(MAX_CLIENTS).expect("small") {
            let peer = SocketAddr::from((IpAddr::from((index + 1_000).to_be_bytes()), 2));
            clients.record(&other, peer);
        }
        assert_eq!(clients.recent().len(), MAX_CLIENTS);
        assert!(clients.last_used(&first.id).is_some(), "last use outlives eviction");
        clients.forget_key(&first.id);
        assert_eq!(clients.last_used(&first.id), None);
    }

    #[test]
    fn prunes_clients_that_are_no_longer_recent() {
        let clients = Clients::default();
        let stale = key("stale");
        clients.record(&stale, SocketAddr::from(([10, 0, 0, 1], 1)));
        if let Ok(mut entries) = clients.entries.lock() {
            for client in entries.clients.values_mut() {
                client.last_seen_unix_ms = 0;
            }
            entries.pruned_unix_ms = 0;
        }
        clients.record(&key("fresh"), SocketAddr::from(([10, 0, 0, 2], 1)));
        assert_eq!(clients.recent().len(), 1);
        assert!(clients.last_used("stale").is_some());
        let kept = clients.entries.lock().map(|entries| entries.clients.len());
        assert_eq!(kept.ok(), Some(1), "the stale client is pruned, not only hidden");
    }
}
