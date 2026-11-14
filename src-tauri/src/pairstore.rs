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

/// Where the Files page's choices are kept: the folder this computer shares, whether
/// it is sharing, where received files go, and the direction switches. Without it a
/// restart made everyone pick the folder again.
pub fn files_path() -> PathBuf {
    let mut p = store_path();
    p.set_file_name("files.json");
    p
}

/// Where the Bluetooth switches are kept. They are choices, not runtime state: a
/// switch that comes back off after every restart looks broken, and the person who
/// turned it on goes looking for the fault somewhere else.
pub fn net_path() -> PathBuf {
    let mut p = store_path();
    p.set_file_name("net.json");
    p
}

fn yes() -> bool {
    true
}

/// The Files page choices as written to disk. Runtime fields (the transfer bar, the
/// peer's answer, the browse list) are deliberately not here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesFile {
    #[serde(rename = "shareOn", default)]
    pub share_on: bool,
    #[serde(default)]
    pub folder: String,
    #[serde(rename = "mountMac", default)]
    pub mount_mac: bool,
    #[serde(rename = "receivedFolder", default)]
    pub received_folder: String,
    #[serde(default)]
    pub shortcuts: bool,
    #[serde(rename = "edgeDrag", default)]
    pub edge_drag: bool,
    #[serde(rename = "macDrags", default = "yes")]
    pub mac_drags: bool,
    #[serde(rename = "acceptFromMac", default = "yes")]
    pub accept_from_mac: bool,
    #[serde(rename = "dropToMac", default = "yes")]
    pub drop_to_mac: bool,
}

/// The last Files page choices, if any were ever made.
pub fn load_files() -> Option<FilesFile> {
    let path = files_path();
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<FilesFile>(&text) {
        Ok(f) => Some(f),
        Err(e) => {
            log::warn!("[Files] Ignoring unreadable {}: {}", path.display(), e);
            None
        }
    }
}

/// Remember the Files page choices, so a restart comes back sharing the same folder.
pub fn save_files(files: &crate::state::FilesState) {
    if cfg!(test) {
        return; // tests never touch the developer's real files
    }
    let path = files_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = FilesFile {
        share_on: files.share_on,
        folder: files.smb.folder.clone(),
        mount_mac: files.mount_mac,
        received_folder: files.received_folder.clone(),
        shortcuts: files.shortcuts,
        edge_drag: files.edge_drag,
        mac_drags: files.mac_drags,
        accept_from_mac: files.accept_from_mac,
        drop_to_mac: files.drop_to_mac,
    };
    match serde_json::to_string_pretty(&file) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                log::warn!("[Files] Could not write {}: {}", path.display(), e);
            }
        }
        Err(e) => log::warn!("[Files] Could not encode files store: {}", e),
    }
}

/// The Bluetooth switches as written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetFile {
    #[serde(default)]
    pub ble: bool,
    #[serde(rename = "blePc", default)]
    pub ble_pc: bool,
    #[serde(rename = "bleWifiOff", default = "yes")]
    pub ble_wifi_off: bool,
}

/// The last Bluetooth choices, if any were ever made.
pub fn load_net() -> Option<NetFile> {
    let path = net_path();
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<NetFile>(&text) {
        Ok(n) => Some(n),
        Err(e) => {
            log::warn!("[Net] Ignoring unreadable {}: {}", path.display(), e);
            None
        }
    }
}

/// Remember the Bluetooth switches, so a restart comes back as the person left it.
pub fn save_net(net: &crate::state::NetState) {
    if cfg!(test) {
        return; // tests never touch the developer's real files
    }
    let path = net_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = NetFile {
        ble: net.ble,
        ble_pc: net.ble_pc,
        ble_wifi_off: net.ble_wifi_off,
    };
    match serde_json::to_string_pretty(&file) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                log::warn!("[Net] Could not write {}: {}", path.display(), e);
            }
        }
        Err(e) => log::warn!("[Net] Could not encode net store: {}", e),
    }
}

/// Where the pointer settings are kept: pointer speed, scroll direction, gestures
/// and edge resistance.
///
/// These are the person's preferences, not runtime state. They used to live only in
/// memory, so a slider that was moved came back at its old position after every
/// restart and looked like it had done nothing.
pub fn kvm_path() -> PathBuf {
    let mut p = store_path();
    p.set_file_name("kvm.json");
    p
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvmFile {
    #[serde(default = "one")]
    pub speed: f32,
    #[serde(rename = "reverseScroll", default)]
    pub reverse_scroll: bool,
    #[serde(default = "yes")]
    pub gestures: bool,
    #[serde(rename = "resistanceMs", default)]
    pub resistance_ms: u32,
}

fn one() -> f32 {
    1.0
}

/// The last pointer settings, if any were ever made.
pub fn load_kvm() -> Option<KvmFile> {
    let path = kvm_path();
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<KvmFile>(&text) {
        Ok(k) => Some(k),
        Err(e) => {
            log::warn!("[KVM] Ignoring unreadable {}: {}", path.display(), e);
            None
        }
    }
}

/// Remember the pointer settings, so a restart comes back as the person left them.
pub fn save_kvm(kvm: &crate::state::KvmSettings) {
    if cfg!(test) {
        return; // tests never touch the developer's real files
    }
    let path = kvm_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = KvmFile {
        speed: kvm.speed,
        reverse_scroll: kvm.reverse_scroll,
        gestures: kvm.gestures,
        resistance_ms: kvm.resistance_ms,
    };
    match serde_json::to_string_pretty(&file) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                log::warn!("[KVM] Could not write {}: {}", path.display(), e);
            }
        }
        Err(e) => log::warn!("[KVM] Could not encode pointer settings: {}", e),
    }
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

    #[test]
    fn net_store_keeps_the_switches_and_defaults_the_rest() {
        // A file written before a switch existed still loads, with that switch at
        // its default, so an upgrade does not turn Bluetooth on by surprise.
        let old: NetFile = serde_json::from_str(r#"{"ble":true}"#).unwrap();
        assert!(old.ble);
        assert!(!old.ble_pc, "companions stay off until switched on");
        assert!(old.ble_wifi_off, "Bluetooth off on Wi-Fi is the default");

        let all: NetFile =
            serde_json::from_str(r#"{"ble":true,"blePc":true,"bleWifiOff":false}"#).unwrap();
        assert!(all.ble && all.ble_pc && !all.ble_wifi_off);
    }
}
