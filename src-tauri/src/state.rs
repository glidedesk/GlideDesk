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
    #[serde(default = "default_true")]
    pub ble: bool,
    #[serde(rename = "resistancePx", default)]
    pub resistance_px: u32,
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
            ble: true,
            resistance_px: 0,
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
            share_on: false,
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
pub struct MultiState {
    pub enabled: bool,
    #[serde(default)]
    pub mode: String,
}

impl Default for MultiState {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "mesh".to_string(),
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
    #[serde(default)]
    pub multi: MultiState,
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
            multi: MultiState::default(),
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
        let mut initial_state = initial_state;
        initial_state.paired = paired;
        initial_state.peer_fingerprints = fingerprints;

        // The screen arrangement the user last chose, so a restart does not come back
        // on the wrong edge.
        #[cfg(not(test))]
        if let Some((side, along)) = crate::pairstore::load_layout() {
            initial_state.layout.side = side;
            initial_state.layout.along = along;
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
        s.client = client.clone();
        if let Some(c) = client {
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
                s.clients.push(c);
            }
        } else {
            s.peer = None;
            // The pairing stays: a disconnect is not a "forget".
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
                    // The other computer went away; the pairing itself stays.
                    s.link = LinkState::default();
                }
            }
        }
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
        assert!(json.contains(&format!("\"layout\":{{\"side\":\"{side}\",\"along\":0.0}}")));
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
            "kvm.ble", "kvm.resistancePx",
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
