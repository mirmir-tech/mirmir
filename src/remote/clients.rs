use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
};

use super::{KeyRecord, Scope, keys::now_unix_ms};

/// How long a client stays listed after its last request.
const RECENT_MS: u64 = 15 * 60 * 1_000;

/// A remote client seen since the server started: one per key and peer host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub key_id: String,
    pub key_name: String,
    pub scope: Scope,
    pub peer: Option<SocketAddr>,
    pub first_seen_unix_ms: u64,
    pub last_seen_unix_ms: u64,
    pub requests: u64,
}

#[derive(Default)]
pub struct Clients {
    entries: Mutex<HashMap<(String, Option<IpAddr>), Client>>,
}

impl Clients {
    pub fn record(&self, key: &KeyRecord, peer: Option<SocketAddr>) {
        let now = now_unix_ms();
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let client = entries.entry((key.id.clone(), peer.map(|peer| peer.ip()))).or_insert_with(|| {
            tracing::info!(key = %key.name, scope = key.scope.as_str(), peer = ?peer, "remote client connected");
            Client {
                key_id: key.id.clone(),
                key_name: key.name.clone(),
                scope: key.scope,
                peer,
                first_seen_unix_ms: now,
                last_seen_unix_ms: now,
                requests: 0,
            }
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
            .values()
            .filter(|client| client.last_seen_unix_ms >= cutoff)
            .cloned()
            .collect();
        recent.sort_by_key(|client| std::cmp::Reverse(client.last_seen_unix_ms));
        recent
    }

    #[must_use]
    pub fn last_used(&self, key_id: &str) -> Option<u64> {
        let entries = self.entries.lock().ok()?;
        entries
            .values()
            .filter(|client| client.key_id == key_id)
            .map(|client| client.last_seen_unix_ms)
            .max()
    }

    pub fn forget_key(&self, key_id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(id, _), _| id != key_id);
        }
    }
}
