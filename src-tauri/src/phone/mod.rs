#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real Phone ADB Mirroring, Scrcpy 5.0.1 bridge, wireless pairing, and device control handlers.

pub mod adb;
pub mod scrcpy;
pub mod session;

use std::sync::Arc;
use parking_lot::RwLock;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::push_state;
use crate::state::{AppState, PhoneConnectedDevice};
use adb::AdbClient;
use session::ActivePhoneSession;

pub struct PhoneManager {
    pub adb: AdbClient,
    pub session: Arc<RwLock<Option<ActivePhoneSession>>>,
}

impl PhoneManager {
    pub fn new() -> Self {
        Self {
            adb: AdbClient::new(),
            session: Arc::new(RwLock::new(None)),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.session.read().is_some()
    }
}

#[tauri::command]
pub async fn connect_phone(
    id: String,
    phone_mgr: State<'_, PhoneManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Connecting to phone device id: {}", id);

    if !phone_mgr.adb.is_available() {
        let mut s = state.inner.write();
        s.phone.bridge.state = "missing".to_string();
        s.phone.bridge.message = "adb binary not found. Install Android platform-tools to use Phone mirroring.".to_string();
        drop(s);
        push_state(&app, &state);
        return Err("adb is not available on this system".to_string());
    }

    // Determine target serial
    let target_serial = if id.is_empty() {
        let devices = phone_mgr.adb.devices()?;
        let first_device = devices.into_iter().find(|d| d.state == "device")
            .ok_or_else(|| "No authorized Android device connected over USB or Wi-Fi".to_string())?;
        first_device.serial
    } else {
        id
    };

    // Update state to connecting
    {
        let mut s = state.inner.write();
        s.phone.bridge.state = "connecting".to_string();
        s.phone.bridge.message = format!("Starting screen mirroring on {}", target_serial);
    }
    push_state(&app, &state);

    // Stop existing session if any
    {
        let mut guard = phone_mgr.session.write();
        if let Some(existing) = guard.take() {
            existing.stop(&phone_mgr.adb);
        }
    }

    match ActivePhoneSession::start(&phone_mgr.adb, &target_serial, app.clone()).await {
        Ok(sess) => {
            let scid = sess.scid.clone();
            let is_usb = !target_serial.contains(':');
            {
                let mut guard = phone_mgr.session.write();
                *guard = Some(sess);
            }

            // Update app state to connected / ready
            {
                let mut s = state.inner.write();
                s.phone.connected = Some(PhoneConnectedDevice {
                    id: target_serial.clone(),
                    name: format!("Android Device ({})", target_serial),
                    usb: is_usb,
                    rtt_ms: Some(5),
                });
                s.phone.bridge.state = "ready".to_string();
                s.phone.bridge.message = format!("Active scrcpy session {}", scid);
                s.phone.bridge.serial = Some(target_serial.clone());
                s.phone.bridge.control = true;
                s.phone.bridge.via = Some(if is_usb { "usb".to_string() } else { "wifi".to_string() });
            }
            push_state(&app, &state);
            log::info!("[PhoneADB] Successfully connected and started scrcpy session for {}", target_serial);
            Ok(())
        }
        Err(e) => {
            log::error!("[PhoneADB] Failed to start scrcpy session: {}", e);
            {
                let mut s = state.inner.write();
                s.phone.bridge.state = "error".to_string();
                s.phone.bridge.message = e.clone();
            }
            push_state(&app, &state);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn disconnect_phone(
    phone_mgr: State<'_, PhoneManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Disconnecting active phone session");
    {
        let mut guard = phone_mgr.session.write();
        if let Some(existing) = guard.take() {
            existing.stop(&phone_mgr.adb);
        }
    }

    {
        let mut s = state.inner.write();
        s.phone.connected = None;
        s.phone.bridge.state = "off".to_string();
        s.phone.bridge.message = String::new();
        s.phone.bridge.serial = None;
        s.phone.bridge.control = false;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn phone_screen(
    show: bool,
    _phone_mgr: State<'_, PhoneManager>,
    _state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Setting phone screen mirroring visibility: {}", show);
    if let Some(win) = app.get_webview_window("projection") {
        if show {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        } else {
            let _ = win.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn phone_retry(
    phone_mgr: State<'_, PhoneManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Retrying phone bridge discovery");
    if !phone_mgr.adb.is_available() {
        let mut s = state.inner.write();
        s.phone.bridge.state = "missing".to_string();
        s.phone.bridge.message = "adb binary not found".to_string();
        drop(s);
        push_state(&app, &state);
        return Err("adb is not available".to_string());
    }

    match phone_mgr.adb.devices() {
        Ok(devices) => {
            let mut s = state.inner.write();
            s.phone.nearby = devices.into_iter().map(|d| crate::state::PhoneNearbyDevice {
                id: d.serial.clone(),
                name: d.model.unwrap_or_else(|| d.serial.clone()),
                connected: false,
                usb: d.usb,
                address: if d.usb { None } else { Some(d.serial) },
            }).collect();
            s.phone.bridge.state = if s.phone.nearby.is_empty() { "needsPair".to_string() } else { "ready".to_string() };
            drop(s);
            push_state(&app, &state);
            Ok(())
        }
        Err(e) => {
            let mut s = state.inner.write();
            s.phone.bridge.state = "error".to_string();
            s.phone.bridge.message = e.clone();
            drop(s);
            push_state(&app, &state);
            Err(e)
        }
    }
}

#[tauri::command]
pub fn phone_pair(
    opts: serde_json::Value,
    phone_mgr: State<'_, PhoneManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Pairing phone via wireless debugging: {:?}", opts);
    let code = opts.get("code").and_then(|v| v.as_str()).unwrap_or("").trim();
    if code.is_empty() {
        return Err("Pairing code cannot be empty".to_string());
    }

    let where_addr = opts.get("where").and_then(|v| v.as_str()).unwrap_or("").trim();
    let (addr, port) = if !where_addr.is_empty() {
        if let Some((a, p)) = where_addr.split_once(':') {
            (a.to_string(), p.parse::<u16>().map_err(|e| format!("Invalid port: {}", e))?)
        } else {
            return Err("Address must be in format host:port".to_string());
        }
    } else {
        // Query mDNS pairing service
        let services = phone_mgr.adb.mdns_services().unwrap_or_default();
        let pair_svc = services.into_iter().find(|s| s.service_type == "pairing")
            .ok_or_else(|| "No wireless debugging pairing service discovered via mDNS. Please enter Address:port manually.".to_string())?;
        (pair_svc.address, pair_svc.port)
    };

    let result = phone_mgr.adb.pair(&addr, port, code)?;
    log::info!("[PhoneADB] adb pair result: {}", result);

    {
        let mut s = state.inner.write();
        s.phone.bridge.can_pair = true;
        s.phone.bridge.message = format!("Paired with {}:{}", addr, port);
    }
    push_state(&app, &state);
    Ok(())
}

pub fn get_phone_assets(state: Option<&AppState>) -> serde_json::Value {
    if let Some(st) = state {
        let s = st.inner.read();
        let wallpaper = s.phone.info.as_ref().and_then(|i| i.wallpaper.clone()).unwrap_or_default();
        let apps = serde_json::to_value(&s.phone.apps).unwrap_or(serde_json::json!([]));
        serde_json::json!({
            "apps": apps,
            "wallpaper": wallpaper,
        })
    } else {
        serde_json::json!({
            "apps": [],
            "wallpaper": "",
        })
    }
}

#[tauri::command]
pub fn phone_assets(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(get_phone_assets(Some(&state)))
}

#[tauri::command]
pub fn set_phone(
    partial: serde_json::Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Setting phone state: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(input) = partial.get("input").and_then(|v| v.as_str()) {
            s.phone.settings.input = input.to_string();
        }
        if let Some(picked) = partial.get("inputPicked").and_then(|v| v.as_bool()) {
            s.phone.settings.input_picked = picked;
        }
        if let Some(sound) = partial.get("sound").and_then(|v| v.as_bool()) {
            s.phone.settings.sound = sound;
        }
        if let Some(bitrate) = partial.get("bitrateMbps").and_then(|v| v.as_u64()) {
            s.phone.settings.bitrate_mbps = bitrate as u32;
        }
        if let Some(wall) = partial.get("wallpaper").and_then(|v| v.as_bool()) {
            s.phone.settings.wallpaper = wall;
        }
        if let Some(enter) = partial.get("enterSends").and_then(|v| v.as_bool()) {
            s.phone.settings.enter_sends = enter;
        }
        if let Some(arrow) = partial.get("arrowSwipe").and_then(|v| v.as_bool()) {
            s.phone.settings.arrow_swipe = arrow;
        }
        if let Some(controls) = partial.get("controls").and_then(|v| v.as_str()) {
            s.phone.settings.controls = controls.to_string();
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn phone_open_app(
    pkg: String,
    phone_mgr: State<'_, PhoneManager>,
    _app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Opening phone application: {}", pkg);
    let guard = phone_mgr.session.read();
    if let Some(ref sess) = *guard {
        let msg = scrcpy::ControlMessage::start_app(&pkg);
        sess.send_control(&msg)?;
        Ok(())
    } else {
        Err("No active phone session connected".to_string())
    }
}

#[tauri::command]
pub fn phone_app_menu(phone_mgr: State<'_, PhoneManager>) -> Result<(), String> {
    log::info!("[PhoneADB] Triggering phone app menu (Recents / App Switcher)");
    let guard = phone_mgr.session.read();
    if let Some(ref sess) = *guard {
        // Android KEYCODE_APP_SWITCH = 187
        sess.inject_key(187, true)?;
        sess.inject_key(187, false)?;
        Ok(())
    } else {
        Err("No active phone session connected".to_string())
    }
}

#[tauri::command]
pub fn phone_close_view(phone_mgr: State<'_, PhoneManager>) -> Result<(), String> {
    log::info!("[PhoneADB] Inverting phone view / back button");
    let guard = phone_mgr.session.read();
    if let Some(ref sess) = *guard {
        // Android KEYCODE_BACK = 4
        sess.inject_key(4, true)?;
        sess.inject_key(4, false)?;
        Ok(())
    } else {
        Err("No active phone session connected".to_string())
    }
}

#[tauri::command]
pub fn phone_install_full(
    phone_mgr: State<'_, PhoneManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] Installing full phone companion APK via ADB");
    let guard = phone_mgr.session.read();
    let serial = guard.as_ref().map(|s| s.serial.clone()).ok_or_else(|| "No active phone connected".to_string())?;

    // Look for APK
    let candidates = [
        "dist/GlideDesk-1.1.1-full.apk",
        "dist/GlideDesk-1.1.1.apk",
        "/Volumes/External/bensonmac/Documents/BensonProject/GlideDesk/dist/GlideDesk-1.1.1-full.apk",
    ];
    let apk_path = candidates.iter().map(std::path::PathBuf::from).find(|p| p.is_file())
        .ok_or_else(|| "Full Android companion APK not found in dist/".to_string())?;

    let apk_str = apk_path.to_str().unwrap();
    let _ = phone_mgr.adb.shell(&serial, &format!("pm install -r {}", apk_str));

    {
        let mut s = state.inner.write();
        if let Some(ref mut adb) = s.phone.adb {
            adb.full = true;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn phone_media(action: String, phone_mgr: State<'_, PhoneManager>) -> Result<(), String> {
    log::info!("[PhoneADB] Phone media action: {}", action);
    let guard = phone_mgr.session.read();
    if let Some(ref sess) = *guard {
        let keycode = match action.as_str() {
            "play" | "pause" | "toggle" => 85, // KEYCODE_MEDIA_PLAY_PAUSE
            "next" => 87,                      // KEYCODE_MEDIA_NEXT
            "prev" | "previous" => 88,         // KEYCODE_MEDIA_PREVIOUS
            "volup" => 24,                     // KEYCODE_VOLUME_UP
            "voldn" => 25,                     // KEYCODE_VOLUME_DOWN
            _ => return Err(format!("Unsupported media action: {}", action)),
        };
        sess.inject_key(keycode, true)?;
        sess.inject_key(keycode, false)?;
        Ok(())
    } else {
        Err("No active phone session connected".to_string())
    }
}

#[tauri::command]
pub fn phone_bt_pick(_app: AppHandle) -> Result<(), String> {
    log::info!("[PhoneADB] Opening Bluetooth settings");
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.BluetoothSettings")
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:bluetooth"])
            .spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn hub_message(
    msg: serde_json::Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Hub] Hub message received: {:?}", msg);
    // Emit hub message to frontends
    let _ = app.emit("phone-hub-message", &msg);
    push_state(&app, &state);
    Ok(())
}
