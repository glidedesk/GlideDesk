// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// GlideDesk: Shared thread-safe application state (Tauri v2 + Rust).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;

fn default_true() -> bool {
    true
}

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
    #[serde(default, skip_serializing)]
    pub token: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingState {
    pub phase: String, // "connecting" | "starting" | "code" | "waiting" | "request" | "failed"
    pub role: String,  // "host" | "guest"
    #[serde(rename = "macName")]
    pub mac_name: String,
    pub os: String,
    pub code: String,
    pub allowed: bool,
    #[serde(default)]
    pub wired: bool,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub error: String,
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
    #[serde(rename = "pcSide", default)]
    pub pc_side: Option<String>,
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
            pc_side: Some("left".to_string()),
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
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub send: bool,
}

impl Default for FeatureToggleState {
    fn default() -> Self {
        Self {
            receive: true,
            share: true,
            enabled: true,
            send: true,
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
pub struct PhoneConnectedDevice {
    pub id: String,
    pub name: String,
    pub usb: bool,
    #[serde(rename = "rttMs")]
    pub rtt_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhoneInfoDetails {
    pub model: Option<String>,
    pub maker: Option<String>,
    pub release: Option<String>,
    pub sdk: Option<u32>,
    pub battery: Option<u32>,
    pub charging: bool,
    pub colors: Vec<String>,
    pub wallpaper: Option<String>,
    pub display: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneBridgeInfo {
    pub state: String, // 'off' | 'missing' | 'waitingWifi' | 'needsPair' | 'connecting' | 'ready' | 'error'
    pub message: String,
    pub serial: Option<String>,
    pub available: bool,
    #[serde(rename = "canPair")]
    pub can_pair: bool,
    #[serde(rename = "inputBlocked")]
    pub input_blocked: bool,
    pub via: Option<String>,
    pub input: Option<String>,
    #[serde(rename = "inputMode")]
    pub input_mode: Option<String>,
    pub control: bool,
    pub driving: bool,
    pub install: Option<serde_json::Value>,
}

impl Default for PhoneBridgeInfo {
    fn default() -> Self {
        Self {
            state: "off".to_string(),
            message: String::new(),
            serial: None,
            available: true,
            can_pair: false,
            input_blocked: false,
            via: None,
            input: Some("adb".to_string()),
            input_mode: Some("adb".to_string()),
            control: false,
            driving: false,
            install: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhoneAdbInfo {
    #[serde(rename = "devOptions")]
    pub dev_options: bool,
    pub wifi: bool,
    #[serde(rename = "canEnable")]
    pub can_enable: bool,
    pub address: Option<String>,
    pub full: bool,
    pub port: u16,
    pub net: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhoneNearbyDevice {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub usb: bool,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhoneAppItem {
    pub id: String,
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneSettingsData {
    pub input: String,
    #[serde(rename = "inputPicked")]
    pub input_picked: bool,
    pub sound: bool,
    #[serde(rename = "bitrateMbps")]
    pub bitrate_mbps: u32,
    pub wallpaper: bool,
    #[serde(rename = "enterSends")]
    pub enter_sends: bool,
    #[serde(rename = "arrowSwipe")]
    pub arrow_swipe: bool,
    pub controls: String,
}

impl Default for PhoneSettingsData {
    fn default() -> Self {
        Self {
            input: "adb".to_string(),
            input_picked: false,
            sound: false,
            bitrate_mbps: 8,
            wallpaper: true,
            enter_sends: true,
            arrow_swipe: true,
            controls: "side".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhoneState {
    pub connected: Option<PhoneConnectedDevice>,
    pub info: Option<PhoneInfoDetails>,
    pub bridge: PhoneBridgeInfo,
    pub nearby: Vec<PhoneNearbyDevice>,
    pub adb: Option<PhoneAdbInfo>,
    pub settings: PhoneSettingsData,
    pub apps: Vec<PhoneAppItem>,
    pub windows: Vec<serde_json::Value>,
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
pub struct MacAppsState {
    pub on: bool,
}

impl Default for MacAppsState {
    fn default() -> Self {
        Self { on: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountState {
    pub on: bool,
    #[serde(default)]
    pub mounted: String,
    #[serde(default)]
    pub busy: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub drive: String,
}

impl Default for MountState {
    fn default() -> Self {
        Self {
            on: false,
            mounted: String::new(),
            busy: String::new(),
            message: String::new(),
            drive: String::new(),
        }
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
            current: "1.1.7".to_string(),
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
    #[serde(rename = "macName", default)]
    pub mac_name: Option<String>,
    #[serde(rename = "peerName", default)]
    pub peer_name: Option<String>,
}

impl Default for LinkState {
    fn default() -> Self {
        Self {
            state: "disconnected".to_string(),
            rtt: None,
            message: None,
            mac_name: None,
            peer_name: None,
        }
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
pub struct CameraState {
    pub on: bool,
    pub viewing: bool,
    pub sending: bool,
    pub connected: bool,
    pub ble: bool,
    #[serde(rename = "peerShares")]
    pub peer_shares: bool,
    #[serde(rename = "virtual")]
    pub virtual_cam: bool,
    pub rotate: u32,
    pub target: String,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            on: false,
            viewing: false,
            sending: false,
            connected: true,
            ble: false,
            peer_shares: true,
            virtual_cam: false,
            rotate: 0,
            target: "obs".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullState {
    pub device: DeviceInfo,
    pub paired: Option<PairedPeer>,
    #[serde(default)]
    pub pairing: Option<PairingState>,
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
    #[serde(rename = "macApps", default)]
    pub mac_apps: MacAppsState,
    #[serde(default)]
    pub mount: MountState,
    #[serde(default)]
    pub setup: SetupState,
    #[serde(default)]
    pub camera: CameraState,
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
            pairing: None,
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
                    version: "1.1.9".to_string(),
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
            version: "1.1.9".to_string(),

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
            mac_apps: MacAppsState::default(),
            mount: MountState::default(),
            setup: SetupState::default(),
            camera: CameraState::default(),
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
        s.pairing = None;
        s.client = client.clone();
        if let Some(c) = client {
            s.peer = Some(c.clone());
            s.paired = Some(PairedPeer {
                mac_id: c.id.clone(),
                mac_name: c.name.clone(),
                os: c.os.clone(),
                address: c.address.clone(),
                token: None,
            });
            s.link.state = "connected".to_string();
            s.link.mac_name = Some(c.name.clone());
            s.link.peer_name = Some(c.name.clone());
            if !s.clients.iter().any(|p| p.id == c.id) {
                s.clients.push(c);
            }
        } else {
            s.peer = None;
            s.paired = None;
            s.link = LinkState::default();
            s.clients.clear();
        }
    }

    pub fn set_pairing(&self, pairing: Option<PairingState>) {
        let mut s = self.inner.write();
        s.pairing = pairing;
    }

    pub fn add_client(&self, client: PeerInfo) {
        let mut s = self.inner.write();
        s.pairing = None;
        s.client = Some(client.clone());
        s.peer = Some(client.clone());
        if let Some(ref mut p) = s.paired {
            if p.mac_id.eq_ignore_ascii_case(&client.id) {
                p.address = client.address.clone();
            }
        }
        s.link.state = "connected".to_string();
        s.link.mac_name = Some(client.name.clone());
        s.link.peer_name = Some(client.name.clone());

        if let Some(pos) = s.clients.iter().position(|p| p.id == client.id) {
            s.clients[pos] = client;
        } else {
            s.clients.push(client);
        }
    }

    pub fn remove_client(&self, client_id: &str) {
        let mut s = self.inner.write();
        s.clients.retain(|p| p.id != client_id && p.address.as_deref() != Some(client_id));
        let match_target = |p: &PeerInfo| -> bool {
            p.id == client_id || p.address.as_deref() == Some(client_id)
        };
        if let Some(ref c) = s.client {
            if match_target(c) {
                s.client = s.clients.first().cloned();
                s.peer = s.clients.first().cloned();
                let peer_name = s.peer.as_ref().map(|p| p.name.clone());
                if let Some(name) = peer_name {
                    s.link.state = "connected".to_string();
                    s.link.mac_name = Some(name.clone());
                    s.link.peer_name = Some(name);
                } else {
                    s.link = LinkState::default();
                    s.paired = None;
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
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "Mac".to_string()
            } else if cfg!(target_os = "windows") {
                "Windows-PC".to_string()
            } else {
                "Linux-PC".to_string()
            }
        })
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

    #[test]
    fn test_mutual_connection_state_lifecycle() {
        let app_state = AppState::new();
        assert_eq!(app_state.snapshot().link.state, "disconnected");
        assert_eq!(app_state.snapshot().link.mac_name, None);

        let peer = PeerInfo {
            id: "vasanth".to_string(),
            name: "VASANTH".to_string(),
            os: "win".to_string(),
            address: Some("192.168.4.9".to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        };

        // Adding client populates link.state and peer/macName
        app_state.add_client(peer);
        let snap = app_state.snapshot();
        assert_eq!(snap.link.state, "connected");
        assert_eq!(snap.link.mac_name.as_deref(), Some("VASANTH"));
        assert_eq!(snap.peer.as_ref().map(|p| p.name.as_str()), Some("VASANTH"));

        // Removing by IP address cleanly disconnects and clears link state
        app_state.remove_client("192.168.4.9");
        let disc_snap = app_state.snapshot();
        assert_eq!(disc_snap.link.state, "disconnected");
        assert_eq!(disc_snap.link.mac_name, None);
        assert!(disc_snap.peer.is_none());
    }

    #[test]
    fn test_layout_and_kvm_defaults_and_sync() {
        let app_state = AppState::new();
        let snap = app_state.snapshot();
        assert_eq!(snap.layout.side, "right");
        assert_eq!(snap.kvm.pc_side.as_deref(), Some("left"));
        assert!(snap.kvm.mouse);
        assert!(snap.kvm.keyboard);
        assert!(snap.kvm.linked);
        assert!(snap.kvm.be_driven);
        assert!(snap.kvm.drive_other);
        assert!(snap.kvm.gestures);

        {
            let mut s = app_state.inner.write();
            s.layout.side = "left".to_string();
            s.kvm.pc_side = Some("right".to_string());
        }
        let updated = app_state.snapshot();
        assert_eq!(updated.layout.side, "left");
        assert_eq!(updated.kvm.pc_side.as_deref(), Some("right"));
    }

    #[test]
    fn test_add_and_remove_multiple_clients() {
        let app_state = AppState::new();
        let peer1 = PeerInfo {
            id: "peer1".to_string(),
            name: "Peer One".to_string(),
            os: "win".to_string(),
            address: Some("192.168.4.10".to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        };
        let peer2 = PeerInfo {
            id: "peer2".to_string(),
            name: "Peer Two".to_string(),
            os: "macos".to_string(),
            address: Some("192.168.4.20".to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        };

        app_state.add_client(peer1);
        app_state.add_client(peer2);

        let snap = app_state.snapshot();
        assert_eq!(snap.clients.len(), 2);
        assert_eq!(snap.link.state, "connected");

        // Remove first peer
        app_state.remove_client("192.168.4.10");
        let snap2 = app_state.snapshot();
        assert_eq!(snap2.clients.len(), 1);
        assert_eq!(snap2.clients[0].id, "peer2");
        assert_eq!(snap2.link.state, "connected");

        // Remove second peer
        app_state.remove_client("peer2");
        let snap3 = app_state.snapshot();
        assert_eq!(snap3.clients.len(), 0);
        assert_eq!(snap3.link.state, "disconnected");
        assert!(snap3.peer.is_none());
    }

    #[test]
    fn test_where_input_state_transitions() {
        let app_state = AppState::new();
        assert_eq!(app_state.snapshot().input, "home");

        app_state.set_where_input("driving");
        assert_eq!(app_state.snapshot().input, "driving");
        assert_eq!(app_state.snapshot().kvm_state.mode, "driving");

        app_state.set_where_input("driven");
        assert_eq!(app_state.snapshot().input, "driven");
        assert_eq!(app_state.snapshot().kvm_state.mode, "driven");

        app_state.set_where_input("home");
        assert_eq!(app_state.snapshot().input, "home");
        assert_eq!(app_state.snapshot().kvm_state.mode, "home");
    }

    #[test]
    fn test_full_state_serialization_roundtrip() {
        let app_state = AppState::new();
        let snap = app_state.snapshot();
        let json = serde_json::to_string(&snap).expect("serialize FullState");

        // Verify key structural elements expected by JS frontend
        assert!(json.contains("\"layout\":{\"side\":\"right\"}"));
        assert!(json.contains("\"pcSide\":\"left\""));
        assert!(json.contains("\"beDriven\":true"));
        assert!(json.contains("\"driveOther\":true"));
        assert!(json.contains("\"macName\":"));
        assert!(json.contains("\"activeTab\":\"overview\""));

        let deserialized: FullState = serde_json::from_str(&json).expect("deserialize FullState");
        assert_eq!(deserialized.layout.side, "right");
        assert_eq!(deserialized.kvm.pc_side.as_deref(), Some("left"));
        assert_eq!(deserialized.input, "home");
    }
}
