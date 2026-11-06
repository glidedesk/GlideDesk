// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Two-way settings synchronization state machine (pairsync).
//
// Shared settings across connected peers: clipboard, notifications, PC apps,
// audio routing, cursor driving/driven permissions, ReZume state, terminal,
// and file drops. Flip a setting on either computer and the other follows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairSyncMessage {
    pub t: String, // "pair.sync"
    #[serde(default)]
    pub feature: Option<String>,
    #[serde(default)]
    pub direction: Option<String>, // "out" or "in"
    #[serde(default)]
    pub value: Option<bool>,
    #[serde(default)]
    pub all: Option<serde_json::Value>,
}

pub struct PairSyncManager {
    pub enabled: bool,
}

impl Default for PairSyncManager {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl PairSyncManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a sync packet for a feature change.
    pub fn make_sync_packet(&self, feature: &str, direction: &str, value: bool) -> PairSyncMessage {
        PairSyncMessage {
            t: "pair.sync".to_string(),
            feature: Some(feature.to_string()),
            direction: Some(direction.to_string()),
            value: Some(value),
            all: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairsync_packet_creation() {
        let mgr = PairSyncManager::new();
        let pkt = mgr.make_sync_packet("clipboard", "out", true);
        assert_eq!(pkt.t, "pair.sync");
        assert_eq!(pkt.feature.as_deref(), Some("clipboard"));
        assert_eq!(pkt.direction.as_deref(), Some("out"));
        assert_eq!(pkt.value, Some(true));
    }
}
