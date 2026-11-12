// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Paired computers, kept across restarts.
//
// The full state is what the window reads, and it deliberately never carries a pairing
// token (see PairedPeer::token, skip_serializing). The token lives here, in a small file
// only this process reads, so a paired computer stays paired next time GlideDesk opens
// instead of asking for a new code every launch.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::state::PairedPeer;

/// One paired computer as it is written to disk. The token is included here and only here.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPeer {
    #[serde(rename = "macId")]
    mac_id: String,
    #[serde(rename = "macName", default)]
    mac_name: String,
    #[serde(default)]
    os: String,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    token: Option<String>,
    #[serde(rename = "tlsFingerprint", default)]
    tls_fingerprint: Option<String>,
}

impl From<&PairedPeer> for StoredPeer {
    fn from(p: &PairedPeer) -> Self {
        Self {
            mac_id: p.mac_id.clone(),
            mac_name: p.mac_name.clone(),
            os: p.os.clone(),
            address: p.address.clone(),
            token: p.token.clone(),
            tls_fingerprint: p.tls_fingerprint.clone(),
        }
    }
}

impl StoredPeer {
    fn into_peer(self) -> PairedPeer {
        PairedPeer {
            mac_id: self.mac_id,
            mac_name: self.mac_name,
            os: self.os,
            address: self.address,
            token: self.token,
            tls_fingerprint: self.tls_fingerprint,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PairFile {
    #[serde(default)]
    paired: Option<StoredPeer>,
    #[serde(rename = "peerFingerprints", default)]
    peer_fingerprints: HashMap<String, String>,
}

/// Where the pairing file lives: the same per-user place on every platform.
pub fn store_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/GlideDesk").join("pairs.json");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("GlideDesk").join("pairs.json");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("glidedesk").join("pairs.json");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join("glidedesk").join("pairs.json");
        }
    }
    PathBuf::from("pairs.json")
}

/// Where this install's TLS identity (certificate and key) is kept.
pub fn identity_path() -> PathBuf {
    let mut p = store_path();
    p.set_file_name("identity.json");
    p
}

/// Where the screen arrangement is kept: the edge this computer shares with the other
/// one. Without it the pair would fall back to its default on every launch and could
/// come up mirrored, with the pointer crossing towards the wall.
pub fn layout_path() -> PathBuf {
    let mut p = store_path();
    p.set_file_name("layout.json");
    p
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LayoutFile {
    #[serde(default = "default_layout_side")]
    side: String,
    #[serde(default)]
    along: f64,
}

fn default_layout_side() -> String {
    // Matches LayoutState::default: Windows keeps the pointer's right edge, a Mac the
    // left one, so the two screens meet where they physically sit.
    if cfg!(target_os = "windows") {
        "right".to_string()
    } else {
        "left".to_string()
    }
}

/// The last arrangement chosen, if one was ever chosen.
pub fn load_layout() -> Option<(String, f64)> {
    let text = std::fs::read_to_string(layout_path()).ok()?;
    match serde_json::from_str::<LayoutFile>(&text) {
        Ok(f) => Some((f.side, f.along)),
        Err(e) => {
            log::warn!("[Layout] Ignoring unreadable layout file: {}", e);
            None
        }
    }
}

/// Remember the arrangement, so a restart comes back on the same edge.
pub fn save_layout(side: &str, along: f64) {
    if cfg!(test) {
        return; // tests never touch the developer's real files
    }
    let path = layout_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = LayoutFile { side: side.to_string(), along };
    if let Ok(text) = serde_json::to_string_pretty(&file) {
        if let Err(e) = std::fs::write(&path, text) {
            log::warn!("[Layout] Could not write {}: {}", path.display(), e);
        }
    }
}

fn read_file() -> PairFile {
    let path = store_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return PairFile::default();
    };
    match serde_json::from_str::<PairFile>(&text) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("[Pairs] Ignoring unreadable {}: {}", path.display(), e);
            PairFile::default()
        }
    }
}

/// The paired computer and the fingerprints already trusted, from the last run.
pub fn load() -> (Option<PairedPeer>, HashMap<String, String>) {
    let f = read_file();
    (f.paired.map(StoredPeer::into_peer), f.peer_fingerprints)
}

/// Write both back. Called whenever either changes (paired, forgotten, fingerprint learned).
pub fn save(paired: &Option<PairedPeer>, fingerprints: &HashMap<String, String>) {
    let path = store_path();
    let file = PairFile {
        paired: paired.as_ref().map(StoredPeer::from),
        peer_fingerprints: fingerprints.clone(),
    };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("[Pairs] Could not create {}: {}", dir.display(), e);
            return;
        }
    }
    match serde_json::to_string_pretty(&file) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                log::warn!("[Pairs] Could not write {}: {}", path.display(), e);
            }
        }
        Err(e) => log::warn!("[Pairs] Could not encode pair store: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_peer_keeps_the_token_that_the_window_never_sees() {
        let peer = PairedPeer {
            mac_id: "peer-1".to_string(),
            mac_name: "VASANTH".to_string(),
            os: "Windows".to_string(),
            address: Some("192.168.4.11".to_string()),
            token: Some("secret".to_string()),
            tls_fingerprint: Some("AA:BB".to_string()),
        };
        // The window's copy hides the token...
        let visible = serde_json::to_value(&peer).unwrap();
        assert!(visible.get("token").is_none());
        // ...but the stored copy round-trips it.
        let stored = serde_json::to_string(&StoredPeer::from(&peer)).unwrap();
        assert!(stored.contains("secret"));
        let back: StoredPeer = serde_json::from_str(&stored).unwrap();
        let back = back.into_peer();
        assert_eq!(back.token.as_deref(), Some("secret"));
        assert_eq!(back.mac_id, "peer-1");
    }

    #[test]
    fn missing_file_is_simply_no_pairs() {
        let f = PairFile::default();
        assert!(f.paired.is_none());
        assert!(f.peer_fingerprints.is_empty());
    }
}
