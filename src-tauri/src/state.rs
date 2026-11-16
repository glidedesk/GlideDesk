// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// GlideDesk: Shared thread-safe application state (Tauri v2 + Rust).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;

use crate::mesh::{MeshSession, MeshState, Role};

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
    #[serde(rename = "tlsFingerprint", default)]
    pub tls_fingerprint: Option<String>,
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
    #[serde(default)]
    pub along: f64,   // offset along the side (points), 0 = starts aligned
    /// Which device is behind each side of this screen (`mesh/edges.rs`).
    ///
    /// `side`/`along` above stay as the two-device app's single seam and keep working exactly as
    /// before; `edges` is what several devices use, and is empty until one is assigned or learned
    /// from a peer's `kvm.layout`. Both live in `layout.json`, so an older build still reads the
    /// file it expects.
    #[serde(default)]
    pub edges: crate::mesh::Edges,
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
        // The other computer starts out beside this one, where the two screens usually
        // meet: a Windows PC keeps the edge it crosses on its right, a Mac on its left.
        // A first run on the same kind of computer still settles at first contact,
        // because the pair turns the larger-id side round when both claim one edge.
        let side = if cfg!(target_os = "windows") {
            "right"
        } else {
            "left"
        };
        Self {
            edges: crate::mesh::Edges::default(),
            side: side.to_string(),
            along: 0.0,
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
    #[serde(rename = "swapCtrlCmd", default)]
    pub swap_ctrl_cmd: bool,
}

impl Default for KeysState {
    fn default() -> Self {
        Self {
            hotkey: "AltRight".to_string(),
            switch_on_edge: true,
            lock_keys: String::new(),
            swap_ctrl_cmd: false,
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
    #[serde(rename = "lockError", default)]
    pub lock_error: String,
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
            lock_error: String::new(),
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
    /// The other computer's virtual desktop size, in its own pointer units
    /// (pixels on Windows, points on macOS), as it last reported in
    /// `kvm.layout` / `hello`. Motion is scaled by the ratio between the two
    /// so the whole of the other screen can be reached from this one.
    #[serde(rename = "peerScreen", default)]
    pub peer_screen: Option<(f64, f64)>,
    #[serde(default = "default_true")]
    pub ble: bool,
    /// How much longer than the base hold the pointer must stay pushed into the
    /// edge before control is handed over. In milliseconds: the pointer stops at
    /// the edge, so extra distance cannot be measured there, but extra time can.
    #[serde(rename = "resistanceMs", default)]
    pub resistance_ms: u32,
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
            // The other computer sits on the edge this one does not cross.
            pc_side: Some(
                if cfg!(target_os = "windows") { "left" } else { "right" }.to_string(),
            ),
            peer_screen: None,
            ble: true,
            resistance_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetState {
    pub ble: bool,
    #[serde(rename = "blePc")]
    pub ble_pc: bool,
    #[serde(rename = "bleWifiOff", default = "default_true")]
    pub ble_wifi_off: bool,
    pub transport: String,
    pub state: String,
    pub rtt: Option<u32>,
    #[serde(rename = "allowPlaintext", default)]
    pub allow_plaintext: bool,
    /// "on" | "off" | "unsupported" | "" (not checked yet)
    #[serde(default)]
    pub radio: String,
    #[serde(rename = "bleOff", default)]
    pub ble_off: bool,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub linked: bool,
    #[serde(default)]
    pub why: Vec<String>,
    #[serde(rename = "hotspotSsid", default)]
    pub hotspot_ssid: String,
    #[serde(rename = "hotspotSaved", default)]
    pub hotspot_saved: bool,
    #[serde(default)]
    pub location: bool,
}

impl Default for NetState {
    fn default() -> Self {
        Self {
            // Bluetooth proximity discovery is off until the user turns it on.
            ble: false,
            ble_pc: false,
            ble_wifi_off: true,
            transport: "wifi".to_string(),
            state: "idle".to_string(),
            rtt: None,
            allow_plaintext: std::env::var("GLIDEDESK_ALLOW_PLAINTEXT")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            radio: String::new(),
            ble_off: false,
            error: String::new(),
            ssid: String::new(),
            linked: false,
            why: Vec::new(),
            hotspot_ssid: String::new(),
            hotspot_saved: false,
            location: false,
        }
    }
}

fn default_off() -> String { "off".to_string() }
fn default_max_height() -> u32 { 1080 }
fn default_bitrate() -> u32 { 20 }
fn default_extend_size() -> String { "mac".to_string() }
fn default_extend_px() -> (u32, u32) { (1920, 1080) }
fn default_1920() -> u32 { 1920 }
fn default_1080() -> u32 { 1080 }
fn default_60() -> u32 { 60 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamState {
    pub state: String,
    pub share: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub watching: bool,
    #[serde(rename = "displayId", default)]
    pub display_id: Option<String>,
    #[serde(rename = "maxHeight", default = "default_max_height")]
    pub max_height: u32,
    #[serde(rename = "bitrateMbps", default = "default_bitrate")]
    pub bitrate_mbps: u32,
    #[serde(default)]
    pub audio: bool,
    #[serde(rename = "streamInput", default = "default_true")]
    pub stream_input: bool,
    #[serde(default)]
    pub remote: Option<serde_json::Value>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub stats: Option<serde_json::Value>,
}

impl Default for StreamState {
    fn default() -> Self {
        Self {
            state: "off".to_string(),
            share: "off".to_string(),
            name: "Display 1".to_string(),
            watching: false,
            display_id: None,
            max_height: 1080,
            bitrate_mbps: 20,
            audio: false,
            stream_input: true,
            remote: None,
            width: 0,
            height: 0,
            stats: None,
        }
    }
}

/// The screen being watched here: sound, playback and frame generation settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorViewState {
    #[serde(default)]
    pub mute: bool,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default = "default_true")]
    pub framegen: bool,
    #[serde(default = "default_pacing")]
    pub pacing: String,
}

impl Default for MonitorViewState {
    fn default() -> Self {
        Self { mute: false, volume: 1.0, framegen: true, pacing: default_pacing() }
    }
}

/// App-level UI settings kept in the state so switches survive a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUiState {
    #[serde(default = "default_true")]
    pub hud: bool,
}

impl Default for AppUiState {
    fn default() -> Self {
        Self { hud: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipboardItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub text: String,
    #[serde(rename = "at", default)]
    pub timestamp: u64,
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub image: Option<serde_json::Value>,
    #[serde(default)]
    pub app: String,
    #[serde(rename = "appName", default)]
    pub app_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub body: String,
}

fn default_clip_mode() -> String { "instant".to_string() }
fn default_max_kb() -> u32 { 64 }
fn default_history_size() -> u32 { 20 }
fn default_volume() -> f64 { 1.0 }
fn default_pacing() -> String { "fast".to_string() }

/// Settings shared by the audio, clipboard and notification tabs. The three tabs
/// each read their own subset; the extra fields are harmless to the others.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureToggleState {
    pub receive: bool,
    pub share: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub send: bool,
    #[serde(default)]
    pub history: Vec<ClipboardItem>,
    #[serde(default)]
    pub muted: Vec<String>,

    // Clipboard
    #[serde(default = "default_clip_mode")]
    pub mode: String,
    #[serde(rename = "skipConcealed", default)]
    pub skip_concealed: bool,
    #[serde(rename = "maxKB", default = "default_max_kb")]
    pub max_kb: u32,
    #[serde(rename = "historySize", default = "default_history_size")]
    pub history_size: u32,
    #[serde(default = "default_true")]
    pub images: bool,
    #[serde(rename = "imageSupport", default = "default_true")]
    pub image_support: bool,
    #[serde(rename = "peerAccepts", default = "default_true")]
    pub peer_accepts: bool,
    #[serde(rename = "lastSkip", default)]
    pub last_skip: String,

    // Audio
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(rename = "muteLocal", default)]
    pub mute_local: bool,
    #[serde(default)]
    pub receiving: bool,
    #[serde(default)]
    pub sending: bool,
    #[serde(rename = "macAccepts", default = "default_true")]
    pub mac_accepts: bool,
    #[serde(default)]
    pub error: String,
    /// The phone's media controls on the Phone page (General and Audio).
    #[serde(rename = "phoneMedia", default = "default_true")]
    pub phone_media: bool,
    /// Play a paired phone's sound here over Bluetooth.
    #[serde(rename = "phoneBt", default = "default_true")]
    pub phone_bt: bool,

    // Notifications
    #[serde(rename = "hideText", default)]
    pub hide_text: bool,
    #[serde(default = "default_true")]
    pub readable: bool,
    #[serde(default)]
    pub sent: u32,
    #[serde(default)]
    pub shown: u32,
    #[serde(rename = "historyIcons", default)]
    pub history_icons: HashMap<String, String>,
    #[serde(default)]
    pub apps: Vec<NotifApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifApp {
    pub app: String,
    #[serde(rename = "appName", default)]
    pub app_name: String,
}

impl Default for FeatureToggleState {
    fn default() -> Self {
        Self {
            receive: true,
            share: true,
            enabled: true,
            send: true,
            history: Vec::new(),
            muted: Vec::new(),
            mode: default_clip_mode(),
            skip_concealed: false,
            max_kb: default_max_kb(),
            history_size: default_history_size(),
            images: true,
            image_support: true,
            peer_accepts: true,
            last_skip: String::new(),
            volume: default_volume(),
            mute_local: false,
            receiving: false,
            sending: false,
            mac_accepts: true,
            error: String::new(),
            phone_media: true,
            phone_bt: true,
            hide_text: false,
            readable: true,
            sent: 0,
            shown: 0,
            history_icons: HashMap::new(),
            apps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesState {
    #[serde(rename = "mountMac")]
    pub mount_mac: bool,
    #[serde(rename = "shareMac")]
    pub share_mac: bool,
    #[serde(default)]
    pub receive: bool,
    #[serde(default)]
    pub share: bool,
    #[serde(default)]
    pub path: String,
    #[serde(rename = "shareOn", default)]
    pub share_on: bool,
    #[serde(default)]
    pub shortcuts: bool,
    #[serde(rename = "copyKeys", default)]
    pub copy_keys: String,
    #[serde(rename = "pasteKeys", default)]
    pub paste_keys: String,
    #[serde(rename = "edgeDrag", default)]
    pub edge_drag: bool,
    #[serde(rename = "macDrags", default = "default_true")]
    pub mac_drags: bool,
    #[serde(rename = "acceptFromMac", default = "default_true")]
    pub accept_from_mac: bool,
    #[serde(rename = "dropToMac", default = "default_true")]
    pub drop_to_mac: bool,
    #[serde(default)]
    pub received: String,
    #[serde(rename = "receivedFolder", default)]
    pub received_folder: String,
    #[serde(default)]
    pub smb: SmbState,
    #[serde(default)]
    pub batch: Option<FilesBatch>,
    #[serde(default)]
    pub clip: FilesClip,
    #[serde(default)]
    pub current: Option<FilesCurrent>,
    #[serde(default)]
    pub queued: u32,
    #[serde(default)]
    pub last: Option<FilesEvent>,
    #[serde(default)]
    pub incoming: FilesIncoming,
    #[serde(default)]
    pub drag: FilesDrag,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmbState {
    #[serde(default)]
    pub drive: String,
    #[serde(default)]
    pub wifi: bool,
    #[serde(rename = "mountMessage", default)]
    pub mount_message: String,
    #[serde(default)]
    pub busy: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub peer: Option<SmbPeer>,
    /// Whether this computer is really serving a folder right now, how many items
    /// are at its top, and why not when it is not.
    #[serde(default)]
    pub sharing: bool,
    #[serde(default)]
    pub count: u32,
    #[serde(rename = "shareMessage", default)]
    pub share_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmbPeer {
    #[serde(default)]
    pub mounted: bool,
    #[serde(default)]
    pub error: String,
    /// Whether the other computer is sharing a folder this one can open.
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesBatch {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub files: u32,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub sent: u64,
    #[serde(default)]
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesClip {
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub pasting: bool,
    #[serde(default)]
    pub incoming: Option<FilesClipIncoming>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesClipIncoming {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub items: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesCurrent {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub sent: u64,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesEvent {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesIncoming {
    #[serde(default)]
    pub receiving: u32,
    #[serde(default)]
    pub last: Option<FilesEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesDrag {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub last: Option<FilesEvent>,
}

impl Default for FilesState {
    fn default() -> Self {
        Self {
            mount_mac: false,
            share_mac: false,
            receive: true,
            share: true,
            path: String::new(),
            share_on: true,
            shortcuts: false,
            copy_keys: String::new(),
            paste_keys: String::new(),
            edge_drag: false,
            mac_drags: true,
            accept_from_mac: true,
            drop_to_mac: true,
            received: "Downloads".to_string(),
            received_folder: String::new(),
            smb: SmbState::default(),
            batch: None,
            clip: FilesClip::default(),
            current: None,
            queued: 0,
            last: None,
            incoming: FilesIncoming::default(),
            drag: FilesDrag::default(),
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
    #[serde(default)]
    pub assets: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeState {
    pub peers: HashMap<String, serde_json::Value>,
    pub waking: Option<String>,
    #[serde(default)]
    pub on: Option<bool>,
    #[serde(default = "default_true")]
    pub checked: bool,
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub error: String,
}

impl Default for WakeState {
    fn default() -> Self {
        Self {
            peers: HashMap::new(),
            waking: None,
            on: Some(false),
            checked: true,
            busy: false,
            error: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalState {
    #[serde(rename = "allowMac")]
    pub allow_mac: bool,
    pub enabled: bool,
    #[serde(default)]
    pub theme: String,
    /// "off" | "no_mac" | "waiting" | "ready" | "unavailable" | "error"
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub server: TerminalServerState,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalServerState {
    #[serde(default)]
    pub busy: bool,
    /// "running" | "stopped" | "missing" | "unknown"
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub info: TerminalServerInfo,
    #[serde(rename = "macAsked", default)]
    pub mac_asked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalServerInfo {
    #[serde(default)]
    pub enabled: bool,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            allow_mac: true,
            enabled: true,
            theme: String::new(),
            status: "unavailable".to_string(),
            server: TerminalServerState::default(),
            user: String::new(),
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppsState {
    pub open: u32,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub list: Vec<AppEntry>,
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub message: String,
    #[serde(rename = "peerPc", default)]
    pub peer_pc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VddInfo {
    #[serde(default)]
    pub installed: bool,
    #[serde(default = "default_true")]
    pub checked: bool,
    #[serde(default)]
    pub busy: String,
    #[serde(default)]
    pub message: String,
}

impl Default for VddInfo {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            installed: std::path::Path::new("/Library/CoreMediaIO/Plug-Ins/DAL").exists(),
            #[cfg(target_os = "windows")]
            installed: std::path::Path::new("C:\\Windows\\System32\\drivers\\IddCx.sys").exists(),
            #[cfg(target_os = "linux")]
            installed: std::path::Path::new("/dev/video0").exists(),
            checked: true,
            busy: String::new(),
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacAccept {
    #[serde(default = "default_true")]
    pub accept: bool,
}

impl Default for MacAccept {
    fn default() -> Self {
        Self { accept: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_1920")]
    pub width: u32,
    #[serde(default = "default_1080")]
    pub height: u32,
    #[serde(rename = "hiDPI", default)]
    pub hi_dpi: bool,
    #[serde(default = "default_60")]
    pub hz: u32,
}

impl Default for VirtualSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 1920,
            height: 1080,
            hi_dpi: false,
            hz: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcScreenState {
    pub mode: String,
    #[serde(default)]
    pub busy: bool,
    #[serde(default = "default_off")]
    pub status: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub vdd: VddInfo,
    #[serde(default)]
    pub monitors: Vec<MonitorInfo>,
    #[serde(default)]
    pub monitor: Option<String>,
    #[serde(rename = "extendSize", default = "default_extend_size")]
    pub extend_size: String,
    #[serde(rename = "extendPx", default = "default_extend_px")]
    pub extend_px: (u32, u32),
    #[serde(rename = "maxHeight", default = "default_max_height")]
    pub max_height: u32,
    #[serde(default = "default_bitrate")]
    pub mbps: u32,
    #[serde(default)]
    pub sound: bool,
    #[serde(default = "default_true")]
    pub input: bool,
    #[serde(default = "default_true")]
    pub remote: bool,
    #[serde(default)]
    pub mac: MacAccept,
    #[serde(default)]
    pub stats: Option<serde_json::Value>,
    #[serde(default)]
    pub virtual_settings: VirtualSettings,
}

impl Default for PcScreenState {
    fn default() -> Self {
        Self {
            mode: "off".to_string(),
            busy: false,
            status: "off".to_string(),
            error: String::new(),
            vdd: VddInfo::default(),
            monitors: Vec::new(),
            monitor: None,
            extend_size: "mac".to_string(),
            extend_px: (1920, 1080),
            max_height: 1080,
            mbps: 20,
            sound: false,
            input: true,
            remote: true,
            mac: MacAccept::default(),
            stats: None,
            virtual_settings: VirtualSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcAppsState {
    pub allow: bool,
    #[serde(rename = "macAccepts", default = "default_true")]
    pub mac_accepts: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub open: u32,
    #[serde(default)]
    pub streaming: u32,
}

impl Default for PcAppsState {
    fn default() -> Self {
        Self { allow: true, mac_accepts: true, message: String::new(), open: 0, streaming: 0 }
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
    #[serde(default)]
    pub via: String,
}

impl Default for MountState {
    fn default() -> Self {
        Self {
            on: false,
            mounted: String::new(),
            busy: String::new(),
            message: String::new(),
            drive: String::new(),
            via: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallState {
    pub ok: bool,
    pub checking: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub error: String,
}

impl Default for FirewallState {
    fn default() -> Self {
        Self { ok: true, checking: false, message: String::new(), error: String::new() }
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
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatesView {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub checking: bool,
    pub error: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: f32,
    #[serde(default, rename = "downloadedBytes")]
    pub downloaded_bytes: u64,
    #[serde(default, rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(default, rename = "statusText")]
    pub status_text: String,
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
            current: env!("CARGO_PKG_VERSION").to_string(),
            latest: None,
            available: false,
            checking: false,
            error: String::new(),
            status: String::new(),
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
            status_text: String::new(),
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
    /// Who is holding the session when this computer can't have it, e.g. "In use on the other computer".
    #[serde(default)]
    pub busy: String,
    /// The user pressed Disconnect: don't redial by ourselves until they connect again.
    #[serde(rename = "manualOff", default)]
    pub manual_off: bool,
}

impl Default for LinkState {
    fn default() -> Self {
        Self {
            state: "disconnected".to_string(),
            rtt: None,
            message: None,
            mac_name: None,
            peer_name: None,
            busy: String::new(),
            manual_off: false,
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
    /// "Let the other computer see this camera" (settings.camera.share).
    #[serde(default)]
    pub share: bool,
    #[serde(rename = "shareError", default)]
    pub share_error: String,
    /// The cameras this computer has, for the picker.
    #[serde(default)]
    pub devices: Vec<CameraDeviceInfo>,
    #[serde(default)]
    pub device: String,
    /// "off" | "asking" | "live" | "refused" | "failed" | "ended"
    #[serde(rename = "viewStatus", default)]
    pub view_status: String,
    #[serde(rename = "viewError", default)]
    pub view_error: String,
    #[serde(rename = "viewShown", default)]
    pub view_shown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CameraDeviceInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
}

/// The virtual camera other apps can pick ("GlideDesk camera").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcamState {
    #[serde(default = "default_vcam_name")]
    pub name: String,
    #[serde(default = "default_true")]
    pub checked: bool,
    #[serde(default = "default_true")]
    pub supported: bool,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub busy: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub live: bool,
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub removable: bool,
    #[serde(default)]
    pub targets: Vec<VcamTargetInfo>,
    #[serde(default = "default_vcam_target")]
    pub target: String,
    #[serde(default)]
    pub rotate: u32,
    #[serde(default)]
    pub picture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcamTargetInfo {
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub label: String,
}

fn default_vcam_name() -> String { "GlideDesk camera".to_string() }
fn default_vcam_target() -> String { "glidedesk".to_string() }

impl Default for VcamState {
    fn default() -> Self {
        Self {
            name: default_vcam_name(),
            checked: true,
            supported: true,
            installed: false,
            busy: String::new(),
            message: String::new(),
            ready: false,
            live: false,
            on: false,
            removable: true,
            targets: Vec::new(),
            target: default_vcam_target(),
            rotate: 0,
            picture: false,
        }
    }
}

/// A phone used as this computer's remote (the Remote tab). Only while a phone is connected.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteState {
    #[serde(default = "default_true")]
    pub allow: bool,
    #[serde(default)]
    pub using: bool,
    #[serde(default)]
    pub opened: bool,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub pad: RemotePadState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePadState {
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub busy: String,
    #[serde(default)]
    pub live: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub message: String,
}

impl Default for RemotePadState {
    fn default() -> Self {
        Self { on: false, supported: false, busy: String::new(), live: false, name: String::new(), message: String::new() }
    }
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
            share: false,
            share_error: String::new(),
            devices: Vec::new(),
            device: String::new(),
            view_status: "off".to_string(),
            view_error: String::new(),
            view_shown: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewSettings {
    #[serde(default = "default_true")]
    pub media: bool,
    #[serde(default = "default_true")]
    pub qs: bool,
    #[serde(default = "default_true")]
    pub notifs: bool,
    #[serde(default = "default_true")]
    pub apps: bool,
    #[serde(default = "default_true")]
    pub clipboard: bool,
    #[serde(default = "default_true")]
    pub files: bool,
}

impl Default for OverviewSettings {
    fn default() -> Self {
        Self {
            media: true,
            qs: true,
            notifs: true,
            apps: true,
            clipboard: true,
            files: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdbQrState {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub done: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NowPlayingState {
    pub app: String,
    #[serde(rename = "appName", default)]
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub duration: u64,
    pub position: u64,
    #[serde(default)]
    pub speed: f32,
    #[serde(default)]
    pub next: bool,
    #[serde(default)]
    pub prev: bool,
    #[serde(default)]
    pub seek: bool,
    #[serde(default)]
    pub art: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RezumeSettings {
    #[serde(default)]
    pub on: bool,
    #[serde(default = "default_true")]
    pub send: bool,
    #[serde(default = "default_true")]
    pub receive: bool,
    #[serde(default = "default_true")]
    pub web: bool,
    #[serde(default = "default_true")]
    pub docs: bool,
    #[serde(default = "default_true")]
    pub apps: bool,
    #[serde(default = "default_true")]
    pub mirror: bool,
    #[serde(default = "default_true")]
    pub push: bool,
    #[serde(default)]
    pub notify: bool,
    #[serde(default = "default_true")]
    pub taskbar: bool,
    #[serde(default = "default_true")]
    pub tray: bool,
    #[serde(default = "default_true")]
    pub tab: bool,
    #[serde(default = "default_true")]
    pub video: bool,
}

impl Default for RezumeSettings {
    fn default() -> Self {
        Self {
            on: false,
            send: true,
            receive: true,
            web: true,
            docs: true,
            apps: true,
            mirror: true,
            push: true,
            notify: false,
            taskbar: true,
            tray: true,
            tab: true,
            video: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RezumeItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String, // "web" | "doc" | "app" | "video"
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub pkg: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub at: u64,
    #[serde(default)]
    pub checksum_sha256: Option<String>,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RezumeState {
    #[serde(default)]
    pub incoming: Option<RezumeItem>,
    #[serde(default)]
    pub shelf: Vec<RezumeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiSettings {
    #[serde(default)]
    pub chats: bool,
    #[serde(default)]
    pub tab: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatItem {
    pub at: u64,
    pub from: String,
    pub text: String,
    pub me: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatConversation {
    pub key: String,
    pub app: String,
    #[serde(rename = "appName", default)]
    pub app_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group: bool,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub reply: bool,
    #[serde(default)]
    pub at: u64,
    #[serde(default)]
    pub items: Vec<ChatItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullState {
    pub device: DeviceInfo,
    pub paired: Option<PairedPeer>,
    /// Every device this app has paired with, beside the primary `paired` one, so a desk of
    /// several screens can be dialled again after a restart without a new code on each.
    ///
    /// Deliberately not in the window's state: it carries the tokens, which never leave the
    /// process (`PairedPeer::token` is already skipped when serialising).
    #[serde(skip)]
    pub known_peers: Vec<PairedPeer>,
    /// Devices the person removed here. A `mesh.roster` from another computer cannot reintroduce
    /// one (`FR-022`), and neither can an automatic dial.
    #[serde(skip)]
    pub forgotten: Vec<String>,
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
    #[serde(default)]
    pub monitor: MonitorViewState,
    #[serde(rename = "macInput", default = "default_true")]
    pub mac_input: bool,
    #[serde(default)]
    pub app: AppUiState,
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
    /// Multi-device mesh: which devices are connected, and who is driving whom.
    /// Dormant (and invisible) until two devices are connected (docs/mesh.md).
    #[serde(default)]
    pub mesh: MeshState,
    #[serde(default)]
    pub camera: CameraState,
    #[serde(default)]
    pub vcam: VcamState,
    #[serde(default)]
    pub remote: RemoteState,
    #[serde(rename = "adbQr", default)]
    pub adb_qr: Option<AdbQrState>,
    #[serde(default)]
    pub overview: OverviewSettings,
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub chats: Option<Vec<ChatConversation>>,
    #[serde(default)]
    pub rezume: RezumeSettings,
    #[serde(rename = "rezumeShelf", default)]
    pub rezume_shelf: RezumeState,
    #[serde(rename = "nowPlaying", default)]
    pub now_playing: Option<NowPlayingState>,
    #[serde(rename = "peerFingerprints", default)]
    pub peer_fingerprints: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub startup: bool,
}

/// Do two pairing tokens name the same secret?
///
/// A token is written as `BEAM_AUTH_V1:<secret>:<fingerprint>` by newer builds and as the bare
/// secret by older ones, so both spellings are compared on the secret alone.
pub fn tokens_match(expected: &str, received: &str) -> bool {
    let secret = |t: &str| -> String {
        t.strip_prefix("BEAM_AUTH_V1:")
            .and_then(|s| s.split(':').next())
            .unwrap_or(t)
            .to_string()
    };
    expected == received || secret(expected) == secret(received)
}

/// Record a device that has just connected in the mesh registry.
///
/// The mesh is dormant until two devices are connected, so this runs for the single peer too:
/// the registry is what decides dormancy. Nothing on the wire changes while it holds one entry.
fn note_session_connected(s: &mut FullState, c: &PeerInfo) -> bool {
    let now = crate::mesh::session::now_ms();
    let mut sess = MeshSession::connected(&c.id, &c.name, &c.os, c.address.clone(), now);
    // A reconnect keeps what the previous session learned about the peer's capabilities: it is
    // the same device, and re-learning them costs a round trip nothing here needs.
    if let Some((mesh, pool)) = s.mesh.session(&c.id).map(|o| (o.mesh, o.pool)) {
        sess.mesh = mesh;
        sess.pool = pool;
    }
    let self_id = s.about.pc.host.to_lowercase();
    s.mesh.upsert(sess, &self_id)
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
            known_peers: Vec::new(),
            forgotten: Vec::new(),
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
                    version: env!("CARGO_PKG_VERSION").to_string(),
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
            version: env!("CARGO_PKG_VERSION").to_string(),

            kvm_state: KvmRuntimeState::default(),
            kvm: KvmSettings::default(),
            net: NetState::default(),
            stream: StreamState::default(),
            monitor: MonitorViewState::default(),
            mac_input: true,
            app: AppUiState::default(),
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
            mesh: MeshState::default(),
            camera: CameraState::default(),
            vcam: VcamState::default(),
            remote: RemoteState::default(),
            adb_qr: None,
            overview: OverviewSettings::default(),
            ui: UiSettings::default(),
            chats: Some(Vec::new()),
            rezume: RezumeSettings::default(),
            rezume_shelf: RezumeState::default(),
            now_playing: None,
            peer_fingerprints: std::collections::HashMap::new(),
            startup: crate::parity::is_startup_enabled(),
        };

        // A paired computer stays paired: the token and fingerprints come back from
        // the last run instead of asking for a new code on every launch. Tests stay
        // hermetic: they never read (or write) the developer's real pairing file.
        #[cfg(not(test))]
        let (paired, fingerprints) = crate::pairstore::load();
        #[cfg(test)]
        let (paired, fingerprints) = (None, std::collections::HashMap::new());
        // The other paired devices and the ones the person removed, so "pair once, meet everyone"
        // survives a restart and a forgotten device stays forgotten.
        #[cfg(not(test))]
        let (known_peers, forgotten) = (
            crate::pairstore::load_peers(),
            crate::pairstore::load_forgotten(),
        );
        #[cfg(test)]
        let (known_peers, forgotten) = (Vec::new(), Vec::new());
        let mut initial_state = initial_state;
        initial_state.paired = paired;
        initial_state.known_peers = known_peers;
        initial_state.forgotten = forgotten;
        initial_state.peer_fingerprints = fingerprints;
        // The roster needs to know which entry is this device before the first session arrives.
        let self_id = initial_state.about.pc.host.to_lowercase();
        initial_state.mesh.bind_self_id(&self_id);

        // The screen arrangement the user last chose, so a restart does not come back
        // on the wrong edge.
        #[cfg(not(test))]
        if let Some((side, along, edges)) = crate::pairstore::load_layout() {
            initial_state.layout.side = side;
            initial_state.layout.along = along;
            initial_state.layout.edges = edges;
        }
        // A setup from before edges existed: one paired device and one seam. Its device keeps the
        // edge it had, so nothing about the pair changes on the first launch after the update.
        #[cfg(not(test))]
        {
            let known: Vec<String> = initial_state
                .paired
                .as_ref()
                .map(|p| vec![p.mac_id.clone()])
                .unwrap_or_default();
            if initial_state.layout.edges.is_empty() {
                initial_state.layout.edges = initial_state
                    .layout
                    .edges
                    .migrate(initial_state.kvm.pc_side.as_deref(), &known);
            }
        }

        // Pointer speed and the other Cursor preferences, so a slider that was moved
        // is still where it was left next time the app opens.
        #[cfg(not(test))]
        if let Some(k) = crate::pairstore::load_kvm() {
            initial_state.kvm.speed = k.speed;
            initial_state.kvm.reverse_scroll = k.reverse_scroll;
            initial_state.kvm.gestures = k.gestures;
            initial_state.kvm.resistance_ms = k.resistance_ms;
        }

        // The Files page choices (folder shared, sharing on or off, received folder,
        // direction switches), so a restart keeps the setup instead of asking again.
        #[cfg(not(test))]
        if let Some(f) = crate::pairstore::load_files() {
            initial_state.files.share_on = f.share_on;
            initial_state.files.smb.folder = f.folder;
            initial_state.files.mount_mac = f.mount_mac;
            initial_state.files.received_folder = f.received_folder;
            initial_state.files.shortcuts = f.shortcuts;
            initial_state.files.edge_drag = f.edge_drag;
            initial_state.files.mac_drags = f.mac_drags;
            initial_state.files.accept_from_mac = f.accept_from_mac;
            initial_state.files.drop_to_mac = f.drop_to_mac;
        }

        // The Bluetooth switches, which are choices and not runtime state: a switch
        // that comes back off after a restart looks like it did nothing when it was
        // turned on.
        #[cfg(not(test))]
        if let Some(n) = crate::pairstore::load_net() {
            initial_state.net.ble = n.ble;
            initial_state.net.ble_pc = n.ble_pc;
            initial_state.net.ble_wifi_off = n.ble_wifi_off;
        }

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

    /// Remember one clipboard item for the Recent list, newest first.
    ///
    /// `dir` is "out" for something copied here and sent, "in" for something that
    /// arrived from the other computer. Repeats are ignored; the list is trimmed to
    /// the size chosen on the Clipboard tab. Returns whether anything was added, so
    /// the caller only refreshes the window when the list really changed.
    pub fn record_clipboard_item(
        &self,
        text: String,
        dir: &str,
        size: u64,
        image: Option<serde_json::Value>,
    ) -> bool {
        let mut s = self.inner.write();
        if let Some(prev) = s.clipboard.history.first() {
            if prev.dir == dir && prev.text == text && prev.size == size {
                return false;
            }
        }
        let cap = s.clipboard.history_size.max(1) as usize;
        let now = chrono::Utc::now().timestamp_millis() as u64;
        s.clipboard.history.insert(
            0,
            ClipboardItem {
                id: format!("clip_{}", now),
                text,
                timestamp: now,
                dir: dir.to_string(),
                size,
                image,
                ..Default::default()
            },
        );
        s.clipboard.history.truncate(cap);
        true
    }

    /// Note why the newest clipboard item was not sent, so the Clipboard tab can
    /// say so in one line. An empty string clears the note.
    pub fn set_clip_last_skip(&self, why: &str) -> bool {
        let mut s = self.inner.write();
        if s.clipboard.last_skip == why {
            return false;
        }
        s.clipboard.last_skip = why.to_string();
        true
    }

    pub fn set_client(&self, client: Option<PeerInfo>) {
        let mut s = self.inner.write();
        s.pairing = None;
        match client {
            Some(c) => {
                s.client = Some(c.clone());
                s.peer = Some(c.clone());
                let fp = s.peer_fingerprints.get(&c.id).cloned();
                // A live client only fills in who is here; it must never overwrite a pairing
                // (that would throw away its token and leave the pair unable to reconnect).
                if s.paired.is_none() {
                    s.paired = Some(PairedPeer {
                        mac_id: c.id.clone(),
                        mac_name: c.name.clone(),
                        os: c.os.clone(),
                        address: c.address.clone(),
                        token: None,
                        tls_fingerprint: fp,
                    });
                }
                s.link.state = "connected".to_string();
                s.link.mac_name = Some(c.name.clone());
                s.link.peer_name = Some(c.name.clone());
                if !s.clients.iter().any(|p| p.id == c.id) {
                    s.clients.push(c.clone());
                }
                note_session_connected(&mut s, &c);
            }
            None => {
                s.peer = None;
                s.client = None;
                // The pairing stays: a disconnect is not a "forget".
                s.link = LinkState::default();
                s.clients.clear();
                s.mesh.clear_partner();
                s.mesh.sessions.clear();
                s.mesh.refresh_active();
                s.input = "home".to_string();
                s.kvm_state.mode = "home".to_string();
            }
        }
    }

    pub fn set_pairing(&self, pairing: Option<PairingState>) {
        let mut s = self.inner.write();
        s.pairing = pairing;
    }

    /// Record a device that has connected. Returns false when the device could not be added because
    /// the desk is full (`MAX_DEVICES`); the caller tells the person and closes that link.
    pub fn add_client(&self, client: PeerInfo) -> bool {
        let mut s = self.inner.write();
        s.pairing = None;
        // A device the person removed here is not added back by a reconnect or by a peer's
        // recommendation (`FR-022`).
        if s.forgotten.iter().any(|f| f.eq_ignore_ascii_case(&client.id)) {
            return false;
        }
        if !note_session_connected(&mut s, &client) {
            // Nothing is added to any view either: a device that cannot be given input must not
            // appear as connected, or the person would type into a screen nobody is driving.
            s.mesh.refusal(&client.id, "limit");
            return false;
        }
        if let Some(ref mut p) = s.paired {
            if p.mac_id.eq_ignore_ascii_case(&client.id) {
                p.address = client.address.clone();
            }
        }
        if let Some(pos) = s.clients.iter().position(|p| p.id == client.id) {
            s.clients[pos] = client.clone();
        } else {
            s.clients.push(client.clone());
        }
        // The device this one is driving (or driven by) stays in front, so every existing page
        // that reads `client`/`peer` keeps pointing at the device the person is actually using.
        if let Some(pid) = s.mesh.partner.clone() {
            if let Some(p) = s.clients.iter().find(|p| p.id == pid).cloned() {
                s.client = Some(p.clone());
                s.peer = Some(p);
            }
        } else {
            s.client = Some(client.clone());
            s.peer = Some(client);
        }
        s.link.state = "connected".to_string();
        let shown = s.client.as_ref().map(|p| p.name.clone());
        if let Some(name) = shown {
            s.link.mac_name = Some(name.clone());
            s.link.peer_name = Some(name);
        }
        true
    }

    pub fn remove_client(&self, client_id: &str) {
        let mut s = self.inner.write();
        s.clients.retain(|p| p.id != client_id && p.address.as_deref() != Some(client_id));
        s.mesh.remove(client_id);
        // Nobody drives or is driven once the device holding the role has gone.
        if s.mesh.partner.is_none() && s.input != "home" {
            s.input = "home".to_string();
            s.kvm_state.mode = "home".to_string();
        }
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
                    // The other computer went away; the pairing itself stays.
                    s.link = LinkState::default();
                }
            }
        }
    }

    /// True once two or more devices are connected: the mesh is no longer dormant.
    pub fn mesh_active(&self) -> bool {
        self.inner.read().mesh.active
    }

    /// This device's id, as it appears on the wire (the host name, lowercased).
    pub fn local_id(&self) -> String {
        self.inner.read().about.pc.host.to_lowercase()
    }

    /// The device the pointer is with, as the peer the existing pages expect.
    ///
    /// This is the mesh-aware replacement for "the other computer": the partner if a handover is
    /// in progress, else the single connected device, else the first one connected. With one
    /// device it is exactly what `client` was before the mesh existed.
    pub fn partner(&self) -> Option<PeerInfo> {
        let s = self.inner.read();
        s.mesh
            .partner_id()
            .and_then(|id| s.clients.iter().find(|p| p.id == id).cloned())
            .or_else(|| s.clients.first().cloned())
    }

    /// Learn what a device can do from its `hello`/`welcome` (`network.rs`).
    ///
    /// `mesh` false means an older app: it is never sent a stamped relay and never asked to pass a
    /// crossing on. A peer that does not say anything keeps the safe value.
    pub fn learn_session_capabilities(&self, id: &str, mesh: bool) {
        let mut s = self.inner.write();
        if let Some(sess) = s
            .mesh
            .sessions
            .iter_mut()
            .find(|x| x.id.eq_ignore_ascii_case(id))
        {
            sess.mesh = mesh;
        }
    }

    /// Remember a device this app is paired with, so it can be dialled again on a later launch.
    ///
    /// `pairs.json` stays the single source of pairing trust: a device only gets here after it
    /// proved it holds a token, and a device the person forgot never gets here at all.
    pub fn remember_peer(
        &self,
        id: &str,
        name: &str,
        os: &str,
        address: Option<String>,
        token: Option<String>,
    ) {
        if id.is_empty() {
            return;
        }
        let self_id = self.local_id();
        if id.eq_ignore_ascii_case(&self_id) {
            return;
        }
        let changed = {
            let mut s = self.inner.write();
            if s.forgotten.iter().any(|f| f.eq_ignore_ascii_case(id)) {
                return;
            }
            match s
                .known_peers
                .iter_mut()
                .find(|p| p.mac_id.eq_ignore_ascii_case(id))
            {
                Some(existing) => {
                    let mut changed = false;
                    if !name.is_empty() && existing.mac_name != name {
                        existing.mac_name = name.to_string();
                        changed = true;
                    }
                    if !os.is_empty() && existing.os != os {
                        existing.os = os.to_string();
                        changed = true;
                    }
                    if address.is_some() && existing.address != address {
                        existing.address = address;
                        changed = true;
                    }
                    // A token is only ever added, never cleared by a hello that did not carry one.
                    if token.is_some() && existing.token != token {
                        existing.token = token;
                        changed = true;
                    }
                    changed
                }
                None => {
                    let tls_fingerprint = s.peer_fingerprints.get(id).cloned();
                    s.known_peers.push(PairedPeer {
                        mac_id: id.to_string(),
                        mac_name: name.to_string(),
                        os: os.to_string(),
                        address,
                        token,
                        tls_fingerprint,
                    });
                    true
                }
            }
        };
        if changed {
            self.persist_peers();
        }
    }

    /// Prove who a connecting device is, from the token it presents.
    ///
    /// Every paired device has its own copy of the secret, so a desk with several screens does not
    /// depend on which one paired last. Returns the id of the device that matched, which is the id
    /// the session is recorded under, even when the peer only named its address.
    pub fn authenticate_peer(
        &self,
        claimed_id: Option<&str>,
        token: Option<&str>,
        peer_ip: Option<&str>,
    ) -> Option<String> {
        let recv = token?;
        let id_ok = |id: &str| -> bool {
            match claimed_id.map(str::trim).filter(|s| !s.is_empty()) {
                Some(claimed) => {
                    claimed.eq_ignore_ascii_case(id)
                        || peer_ip.map(|ip| ip == claimed).unwrap_or(false)
                }
                None => true,
            }
        };
        let s = self.inner.read();
        for p in &s.known_peers {
            if let Some(exp) = p.token.as_deref() {
                if tokens_match(exp, recv) && id_ok(&p.mac_id) {
                    return Some(p.mac_id.clone());
                }
            }
        }
        if let Some(p) = &s.paired {
            if let Some(exp) = p.token.as_deref() {
                if tokens_match(exp, recv) && id_ok(&p.mac_id) {
                    return Some(p.mac_id.clone());
                }
            }
        }
        None
    }

    /// The token this app shares with `peer_id`, if it has one.
    ///
    /// Both devices hold the same secret, so the same answer is given to a peer that dialled this
    /// one and to a peer this one dials. With several devices paired, each keeps its own: a
    /// second computer must not be refused because the first one paired last.
    pub fn token_for(&self, peer_id: &str) -> Option<String> {
        let s = self.inner.read();
        if let Some(p) = s
            .known_peers
            .iter()
            .find(|p| p.mac_id.eq_ignore_ascii_case(peer_id))
        {
            if p.token.is_some() {
                return p.token.clone();
            }
        }
        s.paired
            .as_ref()
            .filter(|p| p.mac_id.eq_ignore_ascii_case(peer_id))
            .and_then(|p| p.token.clone())
            .or_else(|| s.paired.as_ref().and_then(|p| p.token.clone()))
    }

    /// The paired device that lives at `address`, if this desk knows one.
    ///
    /// A dial is not always to the primary pair: `dial_known_peers` reaches every paired screen,
    /// and each keeps its own token and fingerprint. Without this, the hello would offer the
    /// primary pair's token to a device that shares a different secret and be refused.
    pub fn paired_at(&self, address: &str) -> Option<PairedPeer> {
        let host = address.trim().split(':').next().unwrap_or(address);
        let at = |p: &PairedPeer| -> bool {
            let Some(a) = p.address.as_deref() else {
                return false;
            };
            let a = a.trim();
            a == address || a.split(':').next().unwrap_or(a) == host
        };
        let s = self.inner.read();
        if let Some(p) = s.known_peers.iter().find(|p| at(p)) {
            return Some(p.clone());
        }
        s.paired.as_ref().filter(|p| at(p)).cloned()
    }

    /// Devices the person removed here.
    pub fn forgotten_ids(&self) -> Vec<String> {
        self.inner.read().forgotten.clone()
    }

    /// Forget a device: its pairing, its edges, and any claim another computer makes about it.
    ///
    /// Forgetting wins over a roster update (`contracts/wire.md` rule 5): the id goes on the
    /// forgotten list, so a peers' `mesh.roster` and an automatic dial both leave it alone.
    pub fn forget_peer(&self, id: &str) -> crate::mesh::Edges {
        let mut s = self.inner.write();
        if s.paired
            .as_ref()
            .map(|p| p.mac_id.eq_ignore_ascii_case(id))
            .unwrap_or(false)
        {
            s.paired = None;
        }
        s.known_peers
            .retain(|p| !p.mac_id.eq_ignore_ascii_case(id));
        if !s.forgotten.iter().any(|f| f.eq_ignore_ascii_case(id)) {
            s.forgotten.push(id.to_string());
        }
        s.mesh.remove(id);
        s.mesh.roster.remove(id);
        s.mesh.refresh_roster();
        s.clients
            .retain(|p| !p.id.eq_ignore_ascii_case(id) && p.address.as_deref() != Some(id));
        s.guests
            .retain(|p| !p.id.eq_ignore_ascii_case(id) && p.address.as_deref() != Some(id));
        s.found
            .retain(|p| !p.id.eq_ignore_ascii_case(id) && p.address.as_deref() != Some(id));
        let edges = s.layout.edges.clear_peer(id);
        s.layout.edges = edges.clone();
        edges
    }

    /// The paired devices this app can dial: an id and an address, skipping the ones already
    /// connected and anything the person removed.
    pub fn dial_targets(&self) -> Vec<(String, String)> {
        let s = self.inner.read();
        let mut out: Vec<(String, String)> = Vec::new();
        let mut push = |id: &str, address: Option<&String>| {
            let Some(addr) = address else { return };
            if addr.trim().is_empty() {
                return;
            }
            if s.forgotten.iter().any(|f| f.eq_ignore_ascii_case(id)) {
                return;
            }
            if s.mesh.session(id).is_some() {
                return;
            }
            if out.iter().any(|(x, _)| x.eq_ignore_ascii_case(id)) {
                return;
            }
            out.push((id.to_string(), addr.clone()));
        };
        for p in &s.known_peers {
            push(&p.mac_id, p.address.as_ref());
        }
        if let Some(p) = &s.paired {
            push(&p.mac_id, p.address.as_ref());
        }
        out
    }

    /// Write every paired device back, so the next launch can dial them.
    fn persist_peers(&self) {
        let peers = self.inner.read().known_peers.clone();
        #[cfg(not(test))]
        crate::pairstore::save_peers(&peers);
        #[cfg(test)]
        let _ = peers;
    }

    /// Write the forgotten list back.
    pub fn persist_forgotten(&self) {
        let ids = self.inner.read().forgotten.clone();
        #[cfg(not(test))]
        crate::pairstore::save_forgotten(&ids);
        #[cfg(test)]
        let _ = ids;
    }

    /// The connected devices as the pool sees them: id, whether it speaks the mesh, and whether it
    /// accepts pooled items.
    pub fn pool_peers(&self) -> Vec<crate::mesh::pool::PoolPeer> {
        self.inner
            .read()
            .mesh
            .sessions
            .iter()
            .map(|s| crate::mesh::pool::PoolPeer::new(&s.id, s.mesh, s.pool))
            .collect()
    }

    /// Record why a crossing or a copy did not happen, for the page to show (`FR-023`).
    pub fn note_refusal(&self, device: &str, why: &str) {
        self.inner.write().mesh.refusal(device, why);
    }

    /// Hand the pointer (and keyboard) to `id`, or take it back with `None`.
    pub fn set_mesh_partner(&self, id: Option<&str>, role: Role) -> bool {
        let mut s = self.inner.write();
        let ok = match id {
            Some(id) => s.mesh.set_partner(id, role),
            None => {
                s.mesh.clear_partner();
                true
            }
        };
        let mode = s.mesh.role.clone();
        s.input = mode.clone();
        s.kvm_state.mode = mode;
        ok
    }

    /// Put a device on a side of this screen, or clear the side with `None`.
    ///
    /// Refused with a sentence, never silently: the side already leads to another device, the side
    /// is not a side, or the id is not one. The answer is what the page shows, so it names the
    /// device that is in the way — a person should never have to guess why a click did nothing.
    pub fn assign_edge(&self, side: &str, device_id: Option<&str>) -> Result<crate::mesh::Edges, String> {
        if !crate::mesh::is_side(side) {
            return Err(format!("'{}' is not a side of the screen", side));
        }
        let name_of = |id: &str| -> String {
            let s = self.inner.read();
            s.mesh
                .session(id)
                .map(|m| m.name.clone())
                .or_else(|| s.clients.iter().find(|p| p.id == id).map(|p| p.name.clone()))
                .unwrap_or_else(|| id.to_string())
        };
        let (next, side_now) = {
            let s = self.inner.read();
            (s.layout.edges.assign(side, device_id), s.layout.edges.get(side).map(|d| d.to_string()))
        };
        let next = match next {
            Some(next) => next,
            None => {
                return Err(match (device_id, side_now) {
                    (Some(id), Some(holder)) if !id.trim().is_empty() => format!(
                        "The {} edge already leads to {}. Clear it first, or move {} there.",
                        side,
                        name_of(&holder),
                        name_of(id)
                    ),
                    _ => format!("'{}' cannot go on the {} edge", device_id.unwrap_or(""), side),
                });
            }
        };
        {
            let mut s = self.inner.write();
            s.layout.edges = next.clone();
        }
        let (seam, along) = {
            let s = self.inner.read();
            (s.layout.side.clone(), s.layout.along)
        };
        crate::pairstore::save_layout(&seam, along, &next);
        Ok(next)
    }

    /// The next handover route, monotonic on this machine.
    pub fn next_mesh_route(&self) -> u64 {
        let mut s = self.inner.write();
        s.mesh.route += 1;
        s.mesh.route
    }

    /// The handover in progress, as the wire spells it.
    pub fn mesh_route(&self) -> u64 {
        self.inner.read().mesh.route
    }

    /// The route to name when this device ends the handover itself.
    ///
    /// Driving: the route it took the pointer with, which is what the other side compares against.
    /// Driven: the route the driver used, because that is the answer its check expects. Either
    /// way the message names the handover being ended, so a late one from an older handover is
    /// ignored instead of taking the pointer away from a newer one.
    pub fn mesh_end_route(&self) -> u64 {
        let s = self.inner.read();
        if s.mesh.role == "driven" {
            s.mesh.peer_route
        } else {
            s.mesh.route
        }
    }

    /// Who a crossing on `side` belongs to.
    ///
    /// With two or more devices the answer comes from the edges the person assigned (or that a peer
    /// taught us), never from a guess: an unassigned side leads nowhere on a busy desk. With one
    /// device the mesh is dormant and the side is the two-device seam the app has always had, so a
    /// crossing is exactly the crossing it was before this feature existed.
    pub fn crossing_target(&self, side: &str) -> crate::mesh::CrossingTarget {
        let s = self.inner.read();
        if !s.mesh.active {
            if side != s.layout.side {
                return crate::mesh::CrossingTarget::Nobody;
            }
            return match s.mesh.sessions.first() {
                Some(only) => crate::mesh::CrossingTarget::Device(only.id.clone()),
                None => crate::mesh::CrossingTarget::Nobody,
            };
        }
        match s.layout.edges.get(side) {
            Some(id) => {
                if s.mesh.session(id).is_some() {
                    crate::mesh::CrossingTarget::Device(id.to_string())
                } else {
                    crate::mesh::CrossingTarget::Missing(id.to_string())
                }
            }
            // No edge assigned for this side. The seam the two-device app used still leads
            // somewhere: the device holding the pointer, or the first device that connected, so
            // the desk behaves as it did before the person arranges it.
            None if side == s.layout.side => match s.mesh.partner_id() {
                Some(id) => crate::mesh::CrossingTarget::Device(id.to_string()),
                None => match s.mesh.sessions.first() {
                    Some(first) => crate::mesh::CrossingTarget::Device(first.id.clone()),
                    None => crate::mesh::CrossingTarget::Nobody,
                },
            },
            None => crate::mesh::CrossingTarget::Nobody,
        }
    }

    /// Say why a crossing did not happen, once.
    ///
    /// The pointer can sit in an edge for seconds, and every event would otherwise write the same
    /// sentence again. A repeat of the refusal already showing changes nothing and returns false,
    /// which is what "shows the reason once" means in practice (`FR-014`, `FR-023`).
    pub fn note_crossing_refusal(&self, device: &str, why: &str) -> bool {
        let mut s = self.inner.write();
        if let Some(last) = &s.mesh.last_refusal {
            if last.device == device && last.why == why {
                return false;
            }
        }
        let name = s
            .mesh
            .session(device)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| device.to_string());
        s.mesh.refusal(device, why);
        s.link.message = Some(crate::mesh::refusal_text(why, &name));
        true
    }

    /// True when the pointer was asked for and never answered, and has now been brought home.
    ///
    /// Only the take still waiting can time out, so an answer that arrived first wins the race:
    /// the pointer stays where the person put it.
    pub fn mesh_take_timeout(&self) -> bool {
        let now = crate::mesh::session::now_ms();
        let mut s = self.inner.write();
        if !s.mesh.take_expired(now, crate::mesh::TAKE_TIMEOUT_MS) {
            return false;
        }
        let target = s.mesh.pending_take().map(|p| p.target.clone());
        s.mesh.pending = None;
        s.mesh.clear_partner();
        if let Some(target) = target {
            s.mesh.refusal(&target, "not_ready");
        }
        s.input = "home".to_string();
        s.kvm_state.mode = "home".to_string();
        true
    }

    // ---- Handover plumbing the native input paths call --------------------------------------

    /// Take the pointer towards `target` and wait for its answer. Returns this handover's route,
    /// or `None` when the device is not connected (nothing was asked, so nothing is waited on).
    pub fn begin_mesh_take(&self, target: &str) -> Option<u64> {
        let route = self.next_mesh_route();
        self.begin_mesh_take_at(target, route)
    }

    /// The same, on a route the caller has already taken (a chain hands one route to two devices).
    pub fn begin_mesh_take_at(&self, target: &str, route: u64) -> Option<u64> {
        let now = crate::mesh::session::now_ms();
        let mut s = self.inner.write();
        if !s.mesh.begin_take(target, route, now) {
            return None;
        }
        let role = s.mesh.role.clone();
        s.input = role.clone();
        s.kvm_state.mode = role;
        Some(route)
    }

    /// The device we took has answered: the handover stands.
    pub fn confirm_mesh_take(&self, from: &str, route: u64) -> bool {
        let mut s = self.inner.write();
        s.mesh.confirm_take(from, route)
    }

    /// The device we took has refused: nobody is driving anything, and the person is told why.
    pub fn refuse_mesh_take(&self, from: &str, route: u64, why: &str) -> bool {
        let mut s = self.inner.write();
        if !s.mesh.refuse_take(from, route) {
            return false;
        }
        s.mesh.refusal(from, why);
        s.input = "home".to_string();
        s.kvm_state.mode = "home".to_string();
        true
    }

    /// What to answer a `kvm.take` that arrived from `from`.
    pub fn decide_mesh_take(&self, from: &str, route: u64, allowed: bool) -> crate::mesh::TakeDecision {
        self.inner.read().mesh.decide_take(from, route, allowed)
    }

    /// Take the pointer because a device asked for it.
    pub fn accept_mesh_take(&self, from: &str, route: u64) -> bool {
        let mut s = self.inner.write();
        if !s.mesh.accept_take(from, route) {
            return false;
        }
        s.input = "driven".to_string();
        s.kvm_state.mode = "driven".to_string();
        true
    }

    /// `kvm.release` from the device driving this one.
    pub fn release_mesh_take(&self, from: &str, route: u64) -> bool {
        let mut s = self.inner.write();
        if !s.mesh.release(from, route) {
            return false;
        }
        s.input = "home".to_string();
        s.kvm_state.mode = "home".to_string();
        true
    }

    /// `kvm.home` from the device this one is driving: it took control back.
    pub fn mesh_home_from_driven(&self, from: &str, route: u64) -> bool {
        let mut s = self.inner.write();
        if !s.mesh.home_from_driven(from, route) {
            return false;
        }
        s.input = "home".to_string();
        s.kvm_state.mode = "home".to_string();
        true
    }

    /// The person at this device took control back by hand while another device was driving it.
    ///
    /// Returns true when there was a handover to give up, so the caller knows to tell the device
    /// that was driving: `kvm.home` with the route it used. Always safe to call: with nothing to
    /// give up it changes nothing.
    pub fn mesh_local_takeover(&self) -> bool {
        self.inner.write().mesh.local_takeover()
    }

    /// The device a packet came from, as the id the mesh registry knows it by.
    ///
    /// Connections are known by address and devices by id; a handover has to name the device, or
    /// the partner bookkeeping would hold a spelling no session answers to (and the next take would
    /// be refused as coming from a stranger).
    pub fn device_id_for(&self, peer_id: Option<&str>, address: &str) -> String {
        let known = {
            let s = self.inner.read();
            s.mesh
                .session_by_address(address)
                .map(|m| m.id.clone())
                .or_else(|| s.clients.iter().find(|c| {
                    c.address.as_deref() == Some(address)
                }).map(|c| c.id.clone()))
        };
        known
            .or_else(|| peer_id.filter(|p| !p.is_empty()).map(|p| p.to_string()))
            .unwrap_or_else(|| address.to_string())
    }

    /// The id of the device holding the pointer right now, if a handover is in progress.
    pub fn mesh_partner(&self) -> Option<String> {
        self.inner.read().mesh.partner.clone()
    }

    pub fn get_peer_fingerprint(&self, peer_id: &str) -> Option<String> {
        let s = self.inner.read();
        if let Some(ref p) = s.paired {
            if p.mac_id.eq_ignore_ascii_case(peer_id) {
                if let Some(ref fp) = p.tls_fingerprint {
                    return Some(fp.clone());
                }
            }
        }
        s.peer_fingerprints.get(peer_id).cloned()
    }

    pub fn record_peer_fingerprint(&self, peer_id: &str, fingerprint: &str) -> Result<(), String> {
        {
            let mut s = self.inner.write();
            let clean_new = fingerprint.replace(':', "").to_uppercase();
            if let Some(existing) = s.peer_fingerprints.get(peer_id) {
                let clean_existing = existing.replace(':', "").to_uppercase();
                if clean_existing != clean_new {
                    return Err(format!(
                        "certificate fingerprint changed for peer {}; possible MITM — re-pair to trust",
                        peer_id
                    ));
                }
            } else {
                s.peer_fingerprints.insert(peer_id.to_string(), fingerprint.to_string());
            }
            if let Some(ref mut p) = s.paired {
                if p.mac_id.eq_ignore_ascii_case(peer_id) {
                    p.tls_fingerprint = Some(fingerprint.to_string());
                }
            }
        }
        let (paired, fps) = {
            let s = self.inner.read();
            (s.paired.clone(), s.peer_fingerprints.clone())
        };
        #[cfg(not(test))]
        crate::pairstore::save(&paired, &fps);
        Ok(())
    }

    /// Write the paired computer (and the fingerprints) to disk, so it survives a restart.
    pub fn persist_pairs(&self) {
        let (paired, fps) = {
            let s = self.inner.read();
            (s.paired.clone(), s.peer_fingerprints.clone())
        };
        #[cfg(not(test))]
        crate::pairstore::save(&paired, &fps);
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
    fn layout_defaults_put_the_pair_side_by_side() {
        // A Windows PC and a Mac must come up on opposite edges without the user
        // touching the layout grid: the pointer crosses to the right on Windows, to
        // the left on a Mac. Changing one default without the other mirrors the pair.
        let d = LayoutState::default();
        let expected = if cfg!(target_os = "windows") { "right" } else { "left" };
        assert_eq!(d.side, expected);
        assert_eq!(d.along, 0.0);
    }

    #[test]
    fn clipboard_history_keeps_newest_first_up_to_the_chosen_size() {
        let state = AppState::new();
        state.inner.write().clipboard.history_size = 3;

        assert!(state.record_clipboard_item("one".into(), "out", 3, None));
        assert!(state.record_clipboard_item("two".into(), "in", 3, None));
        // A repeat of the newest item is not recorded twice.
        assert!(!state.record_clipboard_item("two".into(), "in", 3, None));
        assert!(state.record_clipboard_item("three".into(), "out", 5, None));
        assert!(state.record_clipboard_item("four".into(), "out", 4, None));

        let history = state.inner.read().clipboard.history.clone();
        assert_eq!(history.len(), 3, "the list must stay at the chosen size");
        assert_eq!(history[0].text, "four");
        assert_eq!(history[1].text, "three");
        assert_eq!(history[2].text, "two");
        assert_eq!(history[0].dir, "out");
        assert!(history[2].timestamp > 0);
    }

    #[test]
    fn clipboard_history_records_pictures_with_a_preview() {
        let state = AppState::new();
        let preview = serde_json::json!({ "thumb": "data:image/png;base64,AA", "w": 10, "h": 4 });
        assert!(state.record_clipboard_item("Picture · 10 × 4".into(), "in", 900, Some(preview)));

        let history = state.inner.read().clipboard.history.clone();
        assert_eq!(history[0].image.as_ref().unwrap()["w"], 10);
        assert_eq!(history[0].size, 900);
    }

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
    fn pairing_survives_disconnect_and_client_updates() {
        let app_state = AppState::new();
        {
            let mut s = app_state.inner.write();
            s.paired = Some(PairedPeer {
                mac_id: "vasanth".to_string(),
                mac_name: "VASANTH".to_string(),
                os: "win".to_string(),
                address: Some("192.168.4.9".to_string()),
                token: Some("BEAM_AUTH_V1:aa:bb".to_string()),
                tls_fingerprint: Some("AA:BB".to_string()),
            });
        }
        let peer = PeerInfo {
            id: "vasanth".to_string(),
            name: "VASANTH".to_string(),
            os: "win".to_string(),
            address: Some("192.168.4.9".to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        };

        // A live client must not overwrite the pairing (or its token).
        app_state.set_client(Some(peer));
        let snap = app_state.snapshot();
        assert_eq!(snap.link.state, "connected");
        assert_eq!(
            snap.paired.as_ref().and_then(|p| p.token.clone()).as_deref(),
            Some("BEAM_AUTH_V1:aa:bb")
        );

        // Nor does the other computer going away undo the pairing.
        app_state.remove_client("192.168.4.9");
        assert_eq!(app_state.snapshot().link.state, "disconnected");
        let after = app_state.snapshot();
        assert_eq!(after.paired.as_ref().map(|p| p.mac_id.as_str()), Some("vasanth"));
        assert_eq!(
            after.paired.as_ref().and_then(|p| p.token.clone()).as_deref(),
            Some("BEAM_AUTH_V1:aa:bb")
        );

        // An explicit disconnect keeps it too; only "forget" clears it.
        app_state.set_client(None);
        assert!(app_state.snapshot().paired.is_some());
    }

    #[test]
    fn test_layout_and_kvm_defaults_and_sync() {
        let app_state = AppState::new();
        let snap = app_state.snapshot();
        let side = if cfg!(target_os = "windows") { "right" } else { "left" };
        let other = if side == "right" { "left" } else { "right" };
        assert_eq!(snap.layout.side, side);
        assert_eq!(snap.kvm.pc_side.as_deref(), Some(other));
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

    fn peer(id: &str, address: &str) -> PeerInfo {
        PeerInfo {
            id: id.to_string(),
            name: id.to_uppercase(),
            os: "win".to_string(),
            address: Some(address.to_string()),
            port: Some(47800),
            last_seen: None,
            invited: None,
        }
    }

    #[test]
    fn mesh_is_dormant_with_one_device_and_activates_on_the_second() {
        let app_state = AppState::new();
        assert!(!app_state.mesh_active(), "no device: dormant");
        assert!(app_state.snapshot().mesh.sessions.is_empty());

        app_state.add_client(peer("desktop", "192.168.4.11"));
        let one = app_state.snapshot();
        assert!(!app_state.mesh_active(), "one device is the two-device app");
        assert_eq!(one.mesh.sessions.len(), 1);
        assert_eq!(one.mesh.sessions[0].id, "desktop");
        assert_eq!(one.mesh.role, "home");
        assert_eq!(one.client.as_ref().map(|p| p.id.as_str()), Some("desktop"));

        app_state.add_client(peer("laptop", "192.168.4.12"));
        let two = app_state.snapshot();
        assert!(app_state.mesh_active(), "two devices activate the mesh");
        assert_eq!(two.mesh.active, true);
        assert_eq!(two.mesh.sessions.len(), 2);
        assert_eq!(two.clients.len(), 2, "the previous view still lists every device");

        // Losing one goes back to the dormant state, with the other still connected.
        app_state.remove_client("laptop");
        let back = app_state.snapshot();
        assert!(!app_state.mesh_active());
        assert_eq!(back.mesh.sessions.len(), 1);
        assert_eq!(back.link.state, "connected");
    }

    #[test]
    fn activating_the_mesh_changes_no_setting() {
        // FR-004: turning the mesh on (a second device appearing) must not touch anything the
        // person chose. Compare the whole settings surface before and after.
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        let before = app_state.snapshot();
        app_state.add_client(peer("laptop", "192.168.4.12"));
        let after = app_state.snapshot();

        // Compare what the pages actually see, so a change anywhere in these settings fails.
        let settings = |s: &FullState| {
            let v = serde_json::to_value(s).expect("serialize state");
            (
                v["kvm"].clone(),
                v["clipboard"].clone(),
                v["net"].clone(),
                v["layout"].clone(),
                v["input"].clone(),
                v["paired"].clone(),
                v["activeTab"].clone(),
            )
        };
        assert_eq!(settings(&before), settings(&after));

        app_state.remove_client("laptop");
        let back = app_state.snapshot();
        assert_eq!(settings(&before), settings(&back));
    }

    #[test]
    fn the_mesh_payload_has_every_field_the_page_reads() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        let json = serde_json::to_value(app_state.snapshot()).expect("serialize state");
        let mesh = json.get("mesh").expect("the state carries a mesh payload");
        for key in ["active", "partner", "role", "route", "lastRefusal", "sessions"] {
            assert!(mesh.get(key).is_some(), "mesh.{} must be present", key);
        }
        let session = &mesh["sessions"][0];
        for key in ["id", "name", "os", "address", "mesh", "pool", "role", "route", "connectedAt", "lastSeen"] {
            assert!(session.get(key).is_some(), "mesh.sessions[].{} must be present", key);
        }
        assert_eq!(session["role"], "idle");
        assert_eq!(session["pool"], true);
    }

    #[test]
    fn a_handover_puts_the_partner_in_front_and_losing_it_returns_home() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        app_state.add_client(peer("laptop", "192.168.4.12"));

        assert!(app_state.set_mesh_partner(Some("laptop"), Role::Driving));
        let snap = app_state.snapshot();
        assert_eq!(snap.mesh.partner.as_deref(), Some("laptop"));
        assert_eq!(snap.mesh.role, "driving");
        assert_eq!(snap.input, "driving", "the machine-level mode follows the handover");
        assert_eq!(snap.kvm_state.mode, "driving");
        assert_eq!(snap.client.as_ref().map(|p| p.id.as_str()), Some("laptop"));

        // The device we were driving goes away: nobody drives anything.
        app_state.remove_client("laptop");
        let gone = app_state.snapshot();
        assert!(gone.mesh.partner.is_none());
        assert_eq!(gone.mesh.role, "home");
        assert_eq!(gone.input, "home");
        assert_eq!(gone.kvm_state.mode, "home");
    }

    #[test]
    fn an_explicit_disconnect_clears_the_mesh() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        app_state.add_client(peer("laptop", "192.168.4.12"));
        app_state.set_mesh_partner(Some("laptop"), Role::Driven);
        assert_eq!(app_state.snapshot().input, "driven");

        app_state.set_client(None);
        let snap = app_state.snapshot();
        assert!(snap.mesh.sessions.is_empty());
        assert!(snap.mesh.partner.is_none());
        assert_eq!(snap.input, "home");
        assert!(!app_state.mesh_active());
    }

    #[test]
    fn the_partner_helper_prefers_the_handover_target() {
        let app_state = AppState::new();
        assert!(app_state.partner().is_none());
        app_state.add_client(peer("desktop", "192.168.4.11"));
        assert_eq!(app_state.partner().map(|p| p.id), Some("desktop".to_string()));
        app_state.add_client(peer("laptop", "192.168.4.12"));
        app_state.set_mesh_partner(Some("laptop"), Role::Driving);
        assert_eq!(
            app_state.partner().map(|p| p.id),
            Some("laptop".to_string()),
            "with two devices the pointer's device wins over whichever connected last"
        );
    }

    /// A device is an older app until its `hello`/`welcome` says otherwise, and the pool must see
    /// exactly that: an older app is never sent a stamped relay or asked to pass a crossing on
    /// (`FR-016`, `contracts/wire.md` rule 4).
    #[test]
    /// Two devices with a session each, and a phone paired with one of them only: the phone
    /// appears on the other through the roster, without a second pairing (`FR-021`, US4).
    #[test]
    fn a_device_paired_with_one_computer_appears_on_the_other() {
        let a = AppState::new();
        let b = AppState::new();
        let phone = AppState::new();
        // A and B know each other; the phone is paired with B alone.
        a.add_client(peer("b", "192.168.4.11"));
        b.add_client(peer("a", "192.168.4.10"));
        b.add_client(peer("phone", "192.168.4.12"));
        phone.add_client(peer("b", "192.168.4.11"));

        let msg = { b.inner.read().mesh.roster_message() };
        assert_eq!(
            msg.peers,
            Some(vec!["a".to_string(), "phone".to_string()]),
            "B reports its own direct links"
        );
        let taken = {
            let mut s = a.inner.write();
            s.mesh.handle_roster("b", &msg, &[])
        };
        assert!(taken, "a roster from a connected device is taken");

        let snap = a.snapshot();
        let learned = snap
            .mesh
            .known
            .iter()
            .find(|d| d.id == "phone")
            .expect("the phone is known on A without a pairing");
        assert!(!learned.direct, "A has no session with it");
        assert_eq!(learned.via, vec!["b".to_string()]);
        assert!(snap.mesh.known.iter().any(|d| d.id == "b" && d.direct));
        assert!(
            snap.mesh.known_links.contains(&"phone".to_string()),
            "the older field still carries the ids it documents"
        );
        // Advisory only: knowing about it does not create a session (routing needs one).
        assert!(a.inner.read().mesh.session("phone").is_none());
    }

    /// A stale roster cannot undo a newer one, and one from a device with no session is ignored.
    #[test]
    fn only_a_newer_roster_from_a_connected_device_is_taken() {
        let a = AppState::new();
        a.add_client(peer("b", "192.168.4.11"));
        let mut msg = crate::network::WireMessage {
            t: "mesh.roster".to_string(),
            v: Some(1),
            reckon: Some(4),
            peers: Some(vec!["a".to_string(), "phone".to_string()]),
            ..Default::default()
        };
        {
            let mut s = a.inner.write();
            assert!(
                !s.mesh.handle_roster("stranger", &msg, &[]),
                "a roster from a device with no session teaches nothing"
            );
            assert!(s.mesh.handle_roster("b", &msg, &[]));
        }
        msg.reckon = Some(3);
        msg.peers = Some(vec!["a".to_string()]);
        {
            let mut s = a.inner.write();
            assert!(!s.mesh.handle_roster("b", &msg, &[]), "an older revision is ignored");
        }
        assert!(a
            .inner
            .read()
            .mesh
            .known
            .iter()
            .any(|d| d.id == "phone"));
    }

    /// Forgetting wins: the device is not added back, not dialled, and not learned again from a
    /// peer's roster (`FR-022`, `contracts/wire.md` rule 5).
    #[test]
    fn a_forgotten_device_stays_forgotten() {
        let a = AppState::new();
        a.add_client(peer("b", "192.168.4.11"));
        a.remember_peer("phone", "Phone", "android", Some("192.168.4.12".to_string()), Some("tok".to_string()));
        assert_eq!(a.dial_targets().len(), 1, "the paired phone can be dialled");

        let edges = a.forget_peer("phone");
        assert!(edges.is_empty(), "no edge pointed at it");
        assert!(a.forgotten_ids().contains(&"phone".to_string()));
        assert!(a.dial_targets().is_empty(), "a forgotten device is never dialled");

        // A reconnect cannot add it back, even holding a token.
        assert!(!a.add_client(peer("phone", "192.168.4.12")));
        assert!(a.inner.read().mesh.session("phone").is_none());

        // And a peer's roster cannot reintroduce it.
        let mut msg = crate::network::WireMessage {
            t: "mesh.roster".to_string(),
            v: Some(1),
            reckon: Some(9),
            peers: Some(vec!["a".to_string(), "phone".to_string()]),
            members: Some(vec![crate::network::WireRosterMember {
                id: "phone".to_string(),
                name: Some("Phone".to_string()),
                os: Some("android".to_string()),
            }]),
            ..Default::default()
        };
        let forgotten = a.forgotten_ids();
        {
            let mut s = a.inner.write();
            assert!(s.mesh.handle_roster("b", &msg, &forgotten));
        }
        msg.reckon = None;
        let snap = a.snapshot();
        assert!(
            snap.mesh.known.iter().all(|d| d.id != "phone"),
            "a forgotten device is not listed again"
        );
    }

    /// A dial reaches a named device, so the hello has to carry that device's token, not the
    /// primary pair's: each pair keeps its own secret.
    #[test]
    fn a_dial_finds_the_paired_device_at_that_address() {
        let a = AppState::new();
        a.remember_peer(
            "desktop",
            "Desktop",
            "win",
            Some("192.168.4.11".to_string()),
            Some("tok-a".to_string()),
        );
        a.remember_peer(
            "laptop",
            "Laptop",
            "linux",
            Some("192.168.4.12:47800".to_string()),
            Some("tok-b".to_string()),
        );

        let laptop = a.paired_at("192.168.4.12").expect("the laptop has a pairing");
        assert_eq!(laptop.mac_id, "laptop");
        assert_eq!(laptop.token.as_deref(), Some("tok-b"));
        assert_eq!(
            a.paired_at("192.168.4.12:47800").map(|p| p.mac_id),
            Some("laptop".to_string()),
            "an address with its port still names the same device"
        );
        assert!(
            a.paired_at("192.168.4.99").is_none(),
            "an unknown address has no pairing of its own"
        );
        assert_eq!(a.dial_targets().len(), 2, "both paired screens are dialled");
        assert!(a
            .dial_targets()
            .iter()
            .any(|(id, addr)| id == "laptop" && addr == "192.168.4.12:47800"));
    }

    /// Every paired device keeps its own token: a second computer must not be refused because the
    /// first one paired last (`pairs.json` holds them all).
    #[test]
    fn each_paired_device_authenticates_with_its_own_token() {
        let a = AppState::new();
        a.remember_peer(
            "desktop",
            "Desktop",
            "win",
            Some("192.168.4.11".to_string()),
            Some("BEAM_AUTH_V1:tok-a:AA:BB".to_string()),
        );
        a.remember_peer(
            "laptop",
            "Laptop",
            "linux",
            Some("192.168.4.12".to_string()),
            Some("BEAM_AUTH_V1:tok-b:CC:DD".to_string()),
        );

        assert_eq!(
            a.authenticate_peer(Some("desktop"), Some("tok-a"), None),
            Some("desktop".to_string()),
            "the bare secret of the older spelling also proves it"
        );
        assert_eq!(
            a.authenticate_peer(Some("laptop"), Some("BEAM_AUTH_V1:tok-b:CC:DD"), None),
            Some("laptop".to_string())
        );
        assert_eq!(
            a.authenticate_peer(Some("desktop"), Some("tok-b"), None),
            None,
            "a token belongs to one device only"
        );
        assert_eq!(
            a.authenticate_peer(None, Some("tok-a"), Some("192.168.4.11")),
            Some("desktop".to_string()),
            "a peer that named only its address is still recognised"
        );
        assert_eq!(a.token_for("laptop").as_deref(), Some("BEAM_AUTH_V1:tok-b:CC:DD"));
        assert_eq!(a.token_for("stranger"), None);

        // A device the person forgot cannot authenticate.
        a.forget_peer("laptop");
        assert_eq!(a.authenticate_peer(Some("laptop"), Some("tok-b"), None), None);
    }

    fn a_device_is_legacy_until_it_says_it_speaks_the_mesh() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        app_state.add_client(peer("phone", "192.168.4.12"));

        let peers = app_state.pool_peers();
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().all(|p| !p.mesh), "nothing said: treated as an older app");
        assert!(peers.iter().all(|p| p.pool), "pooled items are on until a peer says otherwise");

        app_state.learn_session_capabilities("desktop", true);
        let peers = app_state.pool_peers();
        let desktop = peers.iter().find(|p| p.id == "desktop").expect("desktop");
        let phone = peers.iter().find(|p| p.id == "phone").expect("phone");
        assert!(desktop.mesh, "it said it speaks the mesh");
        assert!(!phone.mesh, "it said nothing, so it is still an older app");

        // A reconnect must not lose what was learned: the same device, the same wire.
        app_state.add_client(peer("desktop", "192.168.4.11"));
        let peers = app_state.pool_peers();
        assert!(
            peers.iter().find(|p| p.id == "desktop").expect("desktop").mesh,
            "learning survives a reconnect"
        );
    }

    /// The whole handover across three devices, in one process: A drives B, B's edge leads to C,
    /// so the pointer walks A → B → C without ever being broadcast to everyone (`FR-011`, `SC-006`).
    #[test]
    fn three_devices_hand_the_pointer_along_a_chain() {
        let a = AppState::new();
        let b = AppState::new();
        let c = AppState::new();
        // Each machine sees the other two.
        for (me, others) in [
            (&a, [("b", "10.0.0.2"), ("c", "10.0.0.3")]),
            (&b, [("a", "10.0.0.1"), ("c", "10.0.0.3")]),
            (&c, [("a", "10.0.0.1"), ("b", "10.0.0.2")]),
        ] {
            for (id, addr) in others {
                me.add_client(peer(id, addr));
            }
        }
        // B sits between A and C: C is on B's right.
        b.assign_edge("right", Some("c")).expect("B's edge to C");

        // A crosses into B.
        let route1 = a.begin_mesh_take("b").expect("A takes B");
        assert_eq!(route1, 1);
        assert_eq!(b.decide_mesh_take("a", route1, true), crate::mesh::TakeDecision::Accept { first: true });
        assert!(b.accept_mesh_take("a", route1));
        assert!(a.confirm_mesh_take("b", route1));
        assert_eq!(a.snapshot().input, "driving");
        assert_eq!(b.snapshot().input, "driven");

        // On B the pointer leaves on the right, where C is.
        assert_eq!(
            b.crossing_target("right"),
            crate::mesh::CrossingTarget::Device("c".to_string())
        );

        // B tells A, and A takes C instead of B on one new route.
        let route2 = a.next_mesh_route();
        assert_eq!(route2, 2);
        assert!(a.begin_mesh_take_at("c", route2).is_some());
        assert!(b.release_mesh_take("a", route2), "B is released before A takes C");
        assert_eq!(b.snapshot().input, "home");
        assert_eq!(
            c.decide_mesh_take("a", route2, true),
            crate::mesh::TakeDecision::Accept { first: true }
        );
        assert!(c.accept_mesh_take("a", route2));
        assert!(a.confirm_mesh_take("c", route2));

        // Exactly one device holds the pointer, and it is the last one in the chain.
        let snap = a.snapshot();
        assert_eq!(snap.input, "driving");
        assert_eq!(snap.mesh.partner.as_deref(), Some("c"));
        assert_eq!(b.snapshot().input, "home");
        assert_eq!(c.snapshot().input, "driven");
        assert!(c.mesh_partner().is_some());
    }

    #[test]
    fn one_device_crosses_on_the_seam_and_nowhere_else() {
        // The two-device app: the side facing the other computer crosses to it, every other side
        // stays put. This is the crossing that existed before the mesh, and it must not change.
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        let seam = app_state.inner.read().layout.side.clone();
        let other = crate::commands::opposite_direction(&seam);
        assert_eq!(
            app_state.crossing_target(&seam),
            crate::mesh::CrossingTarget::Device("desktop".to_string())
        );
        assert_eq!(
            app_state.crossing_target(other),
            crate::mesh::CrossingTarget::Nobody
        );

        // A single device that has not labelled itself yet is not a crossing target either.
        let empty = AppState::new();
        assert_eq!(
            empty.crossing_target(&empty.inner.read().layout.side.clone()),
            crate::mesh::CrossingTarget::Nobody
        );
    }

    #[test]
    fn a_side_that_leads_to_a_device_which_left_crosses_nowhere_and_says_why_once() {
        // `FR-014`: an edge mapped to a device that is not connected crosses nothing, the person is
        // told once, and nothing is retried.
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        app_state.add_client(peer("laptop", "192.168.4.12"));
        app_state.add_client(peer("tablet", "192.168.4.14"));
        app_state.assign_edge("right", Some("laptop")).expect("assign");

        // The laptop goes: the edge still names it, so the answer says so by name.
        app_state.remove_client("laptop");
        assert_eq!(
            app_state.crossing_target("right"),
            crate::mesh::CrossingTarget::Missing("laptop".to_string())
        );

        assert!(app_state.note_crossing_refusal("laptop", "not_connected"));
        let snap = app_state.snapshot();
        assert_eq!(
            snap.mesh.last_refusal.as_ref().map(|r| r.why.as_str()),
            Some("not_connected")
        );
        assert!(
            snap.link
                .message
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("laptop"),
            "the reason names the device: {:?}",
            snap.link.message
        );

        // The pointer can sit in the edge: the same refusal is not written again.
        assert!(!app_state.note_crossing_refusal("laptop", "not_connected"));
        // A different reason is news.
        assert!(app_state.note_crossing_refusal("laptop", "not_allowed"));
    }

    #[test]
    fn a_busy_desk_crosses_on_the_seam_until_edges_are_assigned() {
        // Three devices and no arrangement yet: the seam still leads somewhere, and it is the
        // device holding the pointer (or the first one that connected), never a broadcast.
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        app_state.add_client(peer("phone", "192.168.4.13"));
        let seam = app_state.inner.read().layout.side.clone();
        assert_eq!(
            app_state.crossing_target(&seam),
            crate::mesh::CrossingTarget::Device("desktop".to_string())
        );
        // A side nobody assigned leads nowhere on a busy desk.
        assert_eq!(
            app_state.crossing_target(crate::commands::opposite_direction(&seam)),
            crate::mesh::CrossingTarget::Nobody
        );
    }

    #[test]
    fn a_take_that_is_never_answered_brings_the_pointer_home() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        assert!(app_state.begin_mesh_take("desktop").is_some());
        assert_eq!(app_state.snapshot().input, "driving");
        assert_eq!(app_state.mesh_partner().as_deref(), Some("desktop"));

        // The timeout is the reference's one second; nothing has waited that long yet.
        assert!(!app_state.mesh_take_timeout());
        assert_eq!(app_state.snapshot().input, "driving");

        // Pretend the router took a second and a half to answer nothing.
        {
            let mut s = app_state.inner.write();
            let pending = s.mesh.pending.as_mut().expect("a take is waiting");
            pending.since -= crate::mesh::TAKE_TIMEOUT_MS + 500;
        }
        assert!(app_state.mesh_take_timeout());
        let snap = app_state.snapshot();
        assert_eq!(snap.input, "home");
        assert!(snap.mesh.partner.is_none());
        assert_eq!(
            snap.mesh.last_refusal.as_ref().map(|r| r.why.as_str()),
            Some("not_ready")
        );
        // Only once: an answered or settled take cannot time out a second time.
        assert!(!app_state.mesh_take_timeout());
    }

    #[test]
    fn a_packet_from_a_socket_is_named_by_the_device_id_the_mesh_uses() {
        // Roles are stored with device ids, so an incoming handover has to be resolved back from
        // the address it arrived on, not left as a socket string.
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        assert_eq!(
            app_state.device_id_for(None, "192.168.4.11:47800"),
            "desktop"
        );
        assert_eq!(app_state.device_id_for(None, "192.168.4.11"), "desktop");
        // A device the registry has never seen still gets an answer its own socket names.
        assert_eq!(
            app_state.device_id_for(Some("laptop"), "192.168.4.99:47800"),
            "laptop"
        );
        assert_eq!(app_state.device_id_for(None, "192.168.4.99:47800"), "192.168.4.99:47800");
    }

    #[test]
    fn an_edge_names_one_device_and_refuses_to_overwrite_another() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));
        app_state.add_client(peer("laptop", "192.168.4.12"));

        let edges = app_state.assign_edge("right", Some("desktop")).expect("assign right");
        assert_eq!(edges.right.as_deref(), Some("desktop"));
        assert_eq!(app_state.snapshot().layout.edges.right.as_deref(), Some("desktop"));

        // A different device on the same side is refused, and the reason names the device in the
        // way: a click that does nothing must say why.
        let refused = app_state
            .assign_edge("right", Some("laptop"))
            .expect_err("the side is taken");
        assert!(refused.contains("DESKTOP"), "the reason names the holder: {}", refused);
        assert_eq!(
            app_state.snapshot().layout.edges.right.as_deref(),
            Some("desktop"),
            "a refused assignment changes nothing"
        );

        // Moving the device that is already there is how the person rearranges the desk.
        let moved = app_state.assign_edge("bottom", Some("desktop")).expect("move to bottom");
        assert_eq!(moved.right, None, "the old side is left empty");
        assert_eq!(moved.bottom.as_deref(), Some("desktop"));

        // Clearing, and an unknown side.
        let cleared = app_state.assign_edge("bottom", None).expect("clear");
        assert!(cleared.is_empty());
        assert!(app_state.assign_edge("diagonal", Some("desktop")).is_err());
    }

    #[test]
    fn a_device_already_connected_can_be_placed_and_learned_edges_are_kept() {
        let app_state = AppState::new();
        app_state.add_client(peer("desktop", "192.168.4.11"));

        // A peer says its left edge leads here, so it is on our right.
        {
            let mut s = app_state.inner.write();
            s.layout.edges = s
                .layout
                .edges
                .set_remote("left", "desktop")
                .expect("learn the edge");
        }
        assert_eq!(app_state.snapshot().layout.edges.right.as_deref(), Some("desktop"));

        // The person's own choice is not undone by another device's claim on the same side.
        app_state.assign_edge("top", Some("desktop")).expect("assign top");
        let refused = app_state
            .inner
            .write()
            .layout
            .edges
            .set_remote("bottom", "another-device");
        assert!(refused.is_none(), "a claim that would overwrite the side is refused");
        assert_eq!(
            app_state.snapshot().layout.edges.top.as_deref(),
            Some("desktop"),
            "the person's arrangement stands"
        );
    }

    #[test]
    fn three_devices_stay_connected_and_none_is_pushed_away() {
        // FR-001: connecting a device must not disconnect one that is already there.
        let app_state = AppState::new();
        for (i, id) in ["alpha", "beta", "gamma"].iter().enumerate() {
            assert!(app_state.add_client(peer(id, &format!("192.168.4.{}", 20 + i))));
            let snap = app_state.snapshot();
            assert_eq!(
                snap.mesh.sessions.len(),
                i + 1,
                "connecting {} must not drop an earlier device",
                id
            );
            for earlier in &["alpha", "beta", "gamma"][..=i] {
                assert!(
                    snap.mesh.sessions.iter().any(|s| s.id == *earlier),
                    "{} was disconnected by a later device",
                    earlier
                );
            }
        }
        let snap = app_state.snapshot();
        assert_eq!(snap.clients.len(), 3);
        assert!(snap.mesh.active);
        assert!(snap.mesh.last_refusal.is_none());
    }

    #[test]
    fn a_second_session_through_a_different_address_does_not_duplicate() {
        // The same device can arrive twice (a reconnect, or both ends dialling at once). It must
        // stay one session, not two, or the count would reach the limit with half the devices.
        let app_state = AppState::new();
        assert!(app_state.add_client(peer("desktop", "192.168.4.11")));
        assert!(app_state.add_client(peer("desktop", "192.168.4.11")));
        assert_eq!(app_state.snapshot().mesh.sessions.len(), 1);
        assert!(!app_state.mesh_active());
    }

    #[test]
    fn the_eleventh_device_is_refused_with_a_reason() {
        // FR-005: ten stay working, the eleventh is told why, and it is not half-added.
        let app_state = AppState::new();
        for i in 0..crate::mesh::MAX_DEVICES {
            assert!(app_state.add_client(peer(&format!("dev{}", i), &format!("192.168.4.{}", 10 + i))));
        }
        assert!(app_state.mesh_active());
        assert!(!app_state.add_client(peer("eleventh", "192.168.4.99")), "the desk is full");

        let snap = app_state.snapshot();
        assert_eq!(snap.mesh.sessions.len(), crate::mesh::MAX_DEVICES);
        assert_eq!(snap.clients.len(), crate::mesh::MAX_DEVICES, "the refused device is not listed");
        assert!(!snap.clients.iter().any(|p| p.id == "eleventh"));
        let refusal = snap.mesh.last_refusal.expect("the refusal is recorded");
        assert_eq!(refusal.device, "eleventh");
        assert_eq!(refusal.why, "limit");
        let text = crate::mesh::refusal_text(&refusal.why, "Eleventh");
        assert!(text.contains("ten devices connected"), "the reason is actionable: {}", text);
        assert!(text.contains("Disconnect one"), "the reason says what to do: {}", text);
    }

    #[test]
    fn losing_one_device_leaves_the_others_connected() {
        // SC-002: a device going to sleep must not disturb the rest, and coming back finds them.
        let app_state = AppState::new();
        app_state.add_client(peer("alpha", "192.168.4.20"));
        app_state.add_client(peer("beta", "192.168.4.21"));
        app_state.add_client(peer("gamma", "192.168.4.22"));

        app_state.remove_client("beta");
        let mid = app_state.snapshot();
        assert_eq!(mid.mesh.sessions.len(), 2);
        assert!(mid.mesh.sessions.iter().any(|s| s.id == "alpha"));
        assert!(mid.mesh.sessions.iter().any(|s| s.id == "gamma"));
        assert_eq!(mid.link.state, "connected");

        // Back from sleep: it rejoins without disturbing the other two.
        assert!(app_state.add_client(peer("beta", "192.168.4.21")));
        let back = app_state.snapshot();
        assert_eq!(back.mesh.sessions.len(), 3);
        assert!(back.mesh.active);
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
        let side = if cfg!(target_os = "windows") { "right" } else { "left" };
        let other = if side == "right" { "left" } else { "right" };
        let layout = serde_json::to_value(&snap.layout).expect("layout to value");
        assert_eq!(layout["side"].as_str(), Some(side));
        assert_eq!(layout["along"].as_f64(), Some(0.0));
        // The per-device edges travel with it and start empty: a state from before the mesh and a
        // state from after it are the same shape.
        assert_eq!(
            layout["edges"],
            serde_json::json!({"left": null, "right": null, "top": null, "bottom": null})
        );
        assert!(json.contains(&format!("\"pcSide\":\"{other}\"")));
        assert!(json.contains("\"beDriven\":true"));
        assert!(json.contains("\"driveOther\":true"));
        assert!(json.contains("\"macName\":"));
        assert!(json.contains("\"activeTab\":\"overview\""));

        let deserialized: FullState = serde_json::from_str(&json).expect("deserialize FullState");
        assert_eq!(deserialized.layout.side, side);
        assert_eq!(deserialized.kvm.pc_side.as_deref(), Some(other));
        assert_eq!(deserialized.input, "home");
        assert_eq!(deserialized.startup, snap.startup);
    }

    #[test]
    fn test_net_defaults_keep_bluetooth_off() {
        let net = NetState::default();
        assert!(!net.ble, "Bluetooth with phones must be off by default");
        assert!(!net.ble_pc);
        assert!(net.ble_wifi_off, "Bluetooth-off-on-Wi-Fi defaults on");

        // Wire names stay stable for the frontend, and an older snapshot that
        // predates bleWifiOff still loads with the safe default.
        let json = serde_json::to_string(&net).expect("serialize NetState");
        assert!(json.contains("\"ble\":false"));
        assert!(json.contains("\"blePc\":false"));
        assert!(json.contains("\"bleWifiOff\":true"));

        let old: NetState = serde_json::from_str(
            r#"{"ble":false,"blePc":false,"transport":"wifi","state":"idle","rtt":null}"#,
        )
        .expect("deserialize NetState without bleWifiOff");
        assert!(old.ble_wifi_off);
    }

    #[test]
    fn test_updates_default_current_matches_crate_version() {
        // The updater must compare against the running build's version, not a
        // hard-coded literal (regression: this was pinned to "1.1.7", so every
        // install reported a permanent phantom update).
        assert_eq!(UpdatesView::default().current, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_state_carries_every_field_the_ui_reads() {
        // Every tab's update() reads these paths directly. A missing one used to
        // render "undefined"/"NaN" or throw and freeze the rest of the tab.
        let json = serde_json::to_value(AppState::new().snapshot()).expect("snapshot");

        fn has(root: &serde_json::Value, path: &str) -> bool {
            let mut cur = root;
            for part in path.split('.') {
                match cur.get(part) {
                    Some(v) => cur = v,
                    None => return false,
                }
            }
            true
        }

        let paths = [
            "clipboard.mode", "clipboard.skipConcealed", "clipboard.maxKB", "clipboard.historySize",
            "clipboard.images", "clipboard.imageSupport", "clipboard.peerAccepts", "clipboard.lastSkip",
            "audio.volume", "audio.muteLocal", "audio.receiving", "audio.sending",
            "audio.macAccepts", "audio.error",
            "audio.phoneMedia", "audio.phoneBt",
            "vcam.name", "vcam.checked", "vcam.supported", "vcam.installed", "vcam.busy",
            "vcam.message", "vcam.ready", "vcam.live", "vcam.on", "vcam.targets", "vcam.target",
            "vcam.picture",
            "camera.share", "camera.shareError", "camera.devices", "camera.device",
            "camera.viewStatus", "camera.viewError", "camera.viewShown",
            "remote.allow", "remote.using", "remote.opened", "remote.why",
            "remote.pad.on", "remote.pad.supported", "remote.pad.busy", "remote.pad.live",
            "notifs.apps", "notifs.hideText", "notifs.historyIcons", "notifs.readable",
            "notifs.sent", "notifs.shown",
            "keys.swapCtrlCmd",
            "kvm.ble", "kvm.resistanceMs",
            "kvmState.lockError",
            "wake.on", "wake.checked", "wake.busy", "wake.error",
            "firewall.message", "firewall.error",
            "pcScreen.vdd.checked", "pcScreen.vdd.busy", "pcScreen.vdd.message",
            "terminal.status", "terminal.server", "terminal.user", "terminal.message",
            "terminal.server.busy", "terminal.server.service", "terminal.server.message",
            "terminal.server.info.enabled", "terminal.server.macAsked",
            "apps.list", "apps.loading", "apps.message", "apps.peerPc",
            "pcApps.macAccepts", "pcApps.message", "pcApps.open", "pcApps.streaming",
            "files.smb.drive", "files.smb.wifi", "files.smb.mountMessage", "files.smb.busy",
            "files.smb.message", "files.smb.folder", "files.smb.sharing",
            "files.smb.count", "files.smb.shareMessage", "files.smb.peer",
            "files.clip.error", "files.clip.pasting", "files.incoming.receiving",
            "files.drag.state", "files.drag.name", "files.queued",
            "files.shareOn", "files.shortcuts", "files.copyKeys", "files.pasteKeys",
            "files.edgeDrag", "files.macDrags", "files.acceptFromMac", "files.dropToMac",
            "files.received", "files.receivedFolder",
            "net.radio", "net.bleOff", "net.error", "net.ssid", "net.linked", "net.why",
            "net.hotspotSsid", "net.hotspotSaved", "net.location",
            "mount.via",
            "phone.assets",
            "stream.width", "stream.height", "stream.stats",
            "link.busy",
            "app.hud", "monitor.mute", "monitor.volume", "monitor.framegen", "monitor.pacing",
            "macInput",
        ];
        let missing: Vec<&str> = paths.into_iter().filter(|p| !has(&json, p)).collect();
        assert!(missing.is_empty(), "state is missing UI fields: {:?}", missing);
    }
}
