// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// GlideDesk: Shared thread-safe application state (Tauri v2 + Rust).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedPeer {
    #[serde(rename = "macId")]
    pub mac_id: String,
    #[serde(rename = "macName")]
    pub mac_name: String,
    pub os: String,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub os: String,
    pub address: Option<String>,
    pub port: Option<u16>,
    #[serde(rename = "lastSeen")]
    pub last_seen: Option<u64>,
    pub invited: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutState {
    pub side: String, // "right" | "left" | "top" | "bottom"
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            side: "right".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysState {
    pub hotkey: String,
    #[serde(rename = "switchOnEdge")]
    pub switch_on_edge: bool,
    #[serde(rename = "lockKeys")]
    pub lock_keys: String,
}

impl Default for KeysState {
    fn default() -> Self {
        Self {
            hotkey: "AltRight".to_string(),
            switch_on_edge: true,
            lock_keys: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvmRuntimeState {
    pub mode: String,
    pub locked: bool,
    pub linked: bool,
    pub paused: bool,
    pub helper: String,
    pub message: String,
}

impl Default for KvmRuntimeState {
    fn default() -> Self {
        Self {
            mode: "home".to_string(),
            locked: false,
            linked: true,
            paused: false,
            helper: String::new(),
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvmSettings {
    pub mouse: bool,
    pub keyboard: bool,
    pub linked: bool,
    #[serde(rename = "beDriven")]
    pub be_driven: bool,
    #[serde(rename = "driveOther")]
    pub drive_other: bool,
    #[serde(rename = "hotkeySwitch")]
    pub hotkey_switch: bool,
    pub locked: bool,
    pub speed: f32,
    #[serde(rename = "reverseScroll")]
    pub reverse_scroll: bool,
    #[serde(rename = "lockScreen")]
    pub lock_screen: bool,
    pub gestures: bool,
}

impl Default for KvmSettings {
    fn default() -> Self {
        Self {
            mouse: true,
            keyboard: true,
            linked: true,
            be_driven: true,
            drive_other: true,
            hotkey_switch: false,
            locked: false,
            speed: 1.0,
            reverse_scroll: false,
            lock_screen: true,
            gestures: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetState {
    pub ble: bool,
    #[serde(rename = "blePc")]
    pub ble_pc: bool,
    pub transport: String,
    pub state: String,
    pub rtt: Option<u32>,
}

impl Default for NetState {
    fn default() -> Self {
        Self {
            ble: true,
            ble_pc: false,
            transport: "wifi".to_string(),
            state: "idle".to_string(),
            rtt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamState {
    pub state: String,
    pub share: String,
}

impl Default for StreamState {
    fn default() -> Self {
        Self {
            state: "off".to_string(),
            share: "off".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureToggleState {
    pub receive: bool,
    pub share: bool,
}

impl Default for FeatureToggleState {
    fn default() -> Self {
        Self {
            receive: true,
            share: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesState {
    #[serde(rename = "mountMac")]
    pub mount_mac: bool,
    #[serde(rename = "shareMac")]
    pub share_mac: bool,
}

impl Default for FilesState {
    fn default() -> Self {
        Self {
            mount_mac: false,
            share_mac: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneState {
    pub connected: bool,
}

impl Default for PhoneState {
    fn default() -> Self {
        Self { connected: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeState {
    pub peers: HashMap<String, serde_json::Value>,
    pub waking: Option<String>,
}

impl Default for WakeState {
    fn default() -> Self {
        Self {
            peers: HashMap::new(),
            waking: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalState {
    #[serde(rename = "allowMac")]
    pub allow_mac: bool,
    pub enabled: bool,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self { allow_mac: true, enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppsState {
    pub open: u32,
}

impl Default for AppsState {
    fn default() -> Self {
        Self { open: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcScreenState {
    pub mode: String,
}

impl Default for PcScreenState {
    fn default() -> Self {
        Self { mode: "off".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcAppsState {
    pub allow: bool,
}

impl Default for PcAppsState {
    fn default() -> Self {
        Self { allow: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallState {
    pub ok: bool,
    pub checking: bool,
}

impl Default for FirewallState {
    fn default() -> Self {
        Self { ok: true, checking: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupState {
    pub show: bool,
    pub missing: Vec<String>,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            show: false,
            missing: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDownload {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatesView {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub checking: bool,
    pub error: String,
    #[serde(rename = "checkedAt")]
    pub checked_at: u64,
    #[serde(rename = "releaseUrl")]
    pub release_url: String,
    #[serde(rename = "releaseNotes")]
    pub release_notes: String,
    #[serde(rename = "publishedAt")]
    pub published_at: String,
    pub skipped: bool,
    #[serde(rename = "skippedVersion")]
    pub skipped_version: String,
    #[serde(rename = "promptUpdate")]
    pub prompt_update: bool,
    pub auto: bool,
    #[serde(default)]
    pub download: Option<UpdateDownload>,
    #[serde(rename = "noReleases", default)]
    pub no_releases: bool,
}

impl Default for UpdatesView {
    fn default() -> Self {
        Self {
            current: "1.1.2-beta.1".to_string(),
            latest: None,
            available: false,
            checking: false,
            error: String::new(),
            checked_at: 0,
            release_url: "https://github.com/glidedesk/GlideDesk/releases".to_string(),
            release_notes: String::new(),
            published_at: String::new(),
            skipped: false,
            skipped_version: String::new(),
            prompt_update: false,
            auto: true,
            download: None,
            no_releases: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub version: String,
    pub name: String,
    pub arch: String,
    pub runtime: String, // "Tauri v2 + Rust"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcInfo {
    pub host: String,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AboutState {
    pub app: AppInfo,
    pub pc: PcInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(rename = "scaleFactor")]
    pub scale_factor: f64,
    #[serde(rename = "isPrimary")]
    pub is_primary: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopBounds {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkState {
    pub state: String, // "disconnected"|"connecting"|"connected"|"reconnecting"|"identity_changed"
    pub rtt: Option<u32>,
    pub message: Option<String>,
}

impl Default for LinkState {
    fn default() -> Self {
        Self { state: "disconnected".to_string(), rtt: None, message: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryState {
    pub on: bool,
    pub error: String,
}

impl Default for DiscoveryState {
    fn default() -> Self {
        Self { on: true, error: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullState {
    pub device: DeviceInfo,
    pub paired: Option<PairedPeer>,
    pub guests: Vec<PeerInfo>,
    pub found: Vec<PeerInfo>,
    pub peer: Option<PeerInfo>,
    pub client: Option<PeerInfo>,
    #[serde(default)]
    pub clients: Vec<PeerInfo>,
    pub link: LinkState,
    pub discovery: DiscoveryState,
    pub addresses: Vec<String>,
    pub input: String, // "home" | "driving" | "driven"
    pub layout: LayoutState,
    pub keys: KeysState,
    pub updates: UpdatesView,
    pub about: AboutState,
    pub displays: Vec<MonitorInfo>,
    #[serde(rename = "macName")]
    pub mac_name: String,
    #[serde(rename = "activeTab")]
    pub active_tab: String,
    pub version: String,

    #[serde(rename = "kvmState")]
    pub kvm_state: KvmRuntimeState,
    pub kvm: KvmSettings,
    pub net: NetState,
    pub stream: StreamState,
    pub audio: FeatureToggleState,
    pub clipboard: FeatureToggleState,
    pub notifs: FeatureToggleState,
    pub files: FilesState,
    pub phone: PhoneState,
    pub wake: WakeState,
    pub firewall: FirewallState,
    pub terminal: TerminalState,
    pub apps: AppsState,
    #[serde(rename = "pcScreen")]
    pub pc_screen: PcScreenState,
    #[serde(rename = "pcApps")]
    pub pc_apps: PcAppsState,
    #[serde(default)]
    pub setup: SetupState,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<RwLock<FullState>>,
}

impl AppState {
    pub fn new() -> Self {
        let os_name = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };

        let arch = std::env::consts::ARCH.to_string();
        let host = whoami_host();

        let initial_state = FullState {
            device: DeviceInfo {
                name: host.clone(),
            },
            paired: None,
            guests: Vec::new(),
            found: Vec::new(),
            peer: None,
            client: None,
            clients: Vec::new(),
            link: LinkState::default(),
            discovery: DiscoveryState::default(),
            addresses: local_ip_addresses(),
            input: "home".to_string(),
            layout: LayoutState::default(),
            keys: KeysState::default(),
            updates: UpdatesView::default(),
            about: AboutState {
                app: AppInfo {
                    version: "1.1.2-beta.1".to_string(),
                    name: "GlideDesk".to_string(),
                    arch: arch.clone(),
                    runtime: "GlideDesk (Tauri v2 + Rust Native Webview)".to_string(),
                },
                pc: PcInfo {
                    host: host.clone(),
                    os: os_name.to_string(),
                    arch,
                },
            },
            displays: Vec::new(),
            mac_name: host,
            active_tab: "overview".to_string(),
            version: "1.1.2".to_string(),

            kvm_state: KvmRuntimeState::default(),
            kvm: KvmSettings::default(),
            net: NetState::default(),
            stream: StreamState::default(),
            audio: FeatureToggleState::default(),
            clipboard: FeatureToggleState::default(),
            notifs: FeatureToggleState::default(),
            files: FilesState::default(),
            phone: PhoneState::default(),
            wake: WakeState::default(),
            firewall: FirewallState::default(),
            terminal: TerminalState::default(),
            apps: AppsState::default(),
            pc_screen: PcScreenState::default(),
            pc_apps: PcAppsState::default(),
            setup: SetupState::default(),
        };

        Self {
            inner: Arc::new(RwLock::new(initial_state)),
        }
    }

    pub fn snapshot(&self) -> FullState {
        let mut snap = self.inner.read().clone();
        snap.addresses = local_ip_addresses();
        snap
    }

    pub fn update_found_peer(&self, peer: PeerInfo) -> bool {
        let mut s = self.inner.write();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let mut peer = peer;
        peer.last_seen = Some(now);

        let match_idx = s.found.iter().position(|p| {
            p.id.eq_ignore_ascii_case(&peer.id)
                || (peer.address.is_some() && p.address.is_some() && p.address == peer.address)
                || (p.name.eq_ignore_ascii_case(&peer.name) && p.os.eq_ignore_ascii_case(&peer.os))
        });

        if let Some(idx) = match_idx {
            let existing = &mut s.found[idx];
            let changed = existing.address != peer.address || existing.name != peer.name || existing.id != peer.id;
            existing.id = peer.id.clone();
            existing.name = peer.name.clone();
            existing.address = peer.address.clone();
            existing.port = peer.port;
            existing.os = peer.os.clone();
            existing.last_seen = Some(now);

            // Clean up any remaining duplicate entries that match this IP or ID
            let keep_id = peer.id.clone();
            let keep_addr = peer.address.clone();
            let mut i = 0;
            while i < s.found.len() {
                if i != idx {
                    let dup = &s.found[i];
                    let is_dup = dup.id.eq_ignore_ascii_case(&keep_id)
                        || (keep_addr.is_some() && dup.address.is_some() && dup.address == keep_addr);
                    if is_dup {
                        s.found.remove(i);
                        continue;
                    }
                }
                i += 1;
            }

            changed
        } else {
            s.found.push(peer);
            true
        }
    }

    pub fn prune_stale_peers(&self, timeout_ms: u64) -> bool {
        let mut s = self.inner.write();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let initial_len = s.found.len();
        s.found.retain(|p| {
            if let Some(last) = p.last_seen {
                now.saturating_sub(last) < timeout_ms
            } else {
                false
            }
        });
        s.found.len() != initial_len
    }

    pub fn set_where_input(&self, where_val: &str) {
        let mut s = self.inner.write();
        s.input = where_val.to_string();
        s.kvm_state.mode = where_val.to_string();
    }

    pub fn set_client(&self, client: Option<PeerInfo>) {
        let mut s = self.inner.write();
        s.client = client.clone();
        if let Some(c) = client {
            s.peer = Some(c.clone());
            s.paired = Some(PairedPeer {
                mac_id: c.id.clone(),
                mac_name: c.name.clone(),
                os: c.os.clone(),
                address: c.address.clone(),
            });
            s.link.state = "connected".to_string();
            if !s.clients.iter().any(|p| p.id == c.id) {
                s.clients.push(c);
            }
        } else {
            s.peer = None;
            s.link = LinkState::default();
            s.clients.clear();
        }
    }

    pub fn add_client(&self, client: PeerInfo) {
        let mut s = self.inner.write();
        if s.client.is_none() {
            s.client = Some(client.clone());
            s.peer = Some(client.clone());
            s.paired = Some(PairedPeer {
                mac_id: client.id.clone(),
                mac_name: client.name.clone(),
                os: client.os.clone(),
                address: client.address.clone(),
            });
            s.link.state = "connected".to_string();
        }
        if let Some(pos) = s.clients.iter().position(|p| p.id == client.id) {
            s.clients[pos] = client;
        } else {
            s.clients.push(client);
        }
    }

    pub fn remove_client(&self, client_id: &str) {
        let mut s = self.inner.write();
        s.clients.retain(|p| p.id != client_id);
        if let Some(ref c) = s.client {
            if c.id == client_id {
                s.client = s.clients.first().cloned();
                s.peer = s.clients.first().cloned();
                if s.peer.is_none() {
                    s.link = LinkState::default();
                }
            }
        }
    }
}

/// Collect all non-loopback LAN IPv4 addresses this machine is reachable on.
pub fn local_ip_addresses() -> Vec<String> {
    let mut addrs = Vec::new();
    // 1. Direct UDP connection probe (gets default gateway LAN IP without sending packets)
    let targets = ["8.8.8.8:80", "1.1.1.1:80", "224.0.0.1:80", "192.168.1.1:80", "10.0.0.1:80"];
    for target in &targets {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(target).is_ok() {
                if let Ok(local) = sock.local_addr() {
                    let ip = local.ip().to_string();
                    if !ip.starts_with("127.") && !addrs.contains(&ip) {
                        addrs.push(ip);
                    }
                }
            }
        }
    }

    // 2. Platform-specific fallback
    #[cfg(target_os = "macos")]
    if addrs.is_empty() {
        for iface in ["en0", "en1", "en2", "en3"] {
            if let Ok(output) = std::process::Command::new("ipconfig").args(["getifaddr", iface]).output() {
                let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !ip.is_empty() && !ip.starts_with("127.") && !addrs.contains(&ip) {
                    addrs.push(ip);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    if addrs.is_empty() {
        if let Ok(output) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-NetIPAddress -AddressFamily IPv4 -InterfaceAlias 'Wi-Fi*','Ethernet*' | Select-Object -ExpandProperty IPAddress"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let ip = line.trim().to_string();
                if !ip.is_empty() && !ip.starts_with("127.") && !ip.starts_with("169.254.") && !addrs.contains(&ip) {
                    addrs.push(ip);
                }
            }
        }
    }

    if addrs.is_empty() {
        addrs.push("127.0.0.1".to_string());
    }
    addrs
}

pub fn whoami_host() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "GlideDesk-PC".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_found_peer_deduplication() {
        let app_state = AppState::new();

        let peer1 = PeerInfo {
            id: "Vasanth".to_string(),
            name: "Vasanth".to_string(),
            os: "win".to_string(),
            address: Some("192.168.4.9".to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        };
        app_state.update_found_peer(peer1);
        assert_eq!(app_state.inner.read().found.len(), 1);

        // Case-variant ID from same machine
        let peer2 = PeerInfo {
            id: "VASANTH".to_string(),
            name: "VASANTH".to_string(),
            os: "win".to_string(),
            address: Some("192.168.4.9".to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        };
        app_state.update_found_peer(peer2);

        // Must NOT create duplicate entry; should update existing
        assert_eq!(app_state.inner.read().found.len(), 1);
        let current = &app_state.inner.read().found[0];
        assert_eq!(current.address.as_deref(), Some("192.168.4.9"));
        assert_eq!(current.id, "VASANTH");
    }

    #[test]
    fn test_full_state_includes_setup() {
        let app_state = AppState::new();
        let state = app_state.snapshot();
        assert!(!state.setup.show);
        assert!(state.setup.missing.is_empty());

        let json = serde_json::to_string(&state).expect("serialize FullState");
        assert!(json.contains("\"setup\":{\"show\":false,\"missing\":[]}"));
    }
}
