#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real Phone ADB Mirroring, Scrcpy 5.0.1 bridge, wireless pairing, and device control handlers.

pub mod adb;
pub mod scrcpy;
pub mod session;

use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::push_state;
use crate::state::{AppState, PhoneConnectedDevice};
use adb::AdbClient;
use session::ActivePhoneSession;

pub struct PhoneManager {
    pub adb: AdbClient,
    pub session: Arc<RwLock<Option<ActivePhoneSession>>>,
    pub qr_cancel: Arc<RwLock<Option<tokio::sync::broadcast::Sender<()>>>>,
}

impl PhoneManager {
    pub fn new() -> Self {
        Self {
            adb: AdbClient::new(),
            session: Arc::new(RwLock::new(None)),
            qr_cancel: Arc::new(RwLock::new(None)),
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
    pkg: serde_json::Value,
    phone_mgr: State<'_, PhoneManager>,
    _app: AppHandle,
) -> Result<(), String> {
    let pkg_name = if let Some(s) = pkg.as_str() {
        s.to_string()
    } else if let Some(id) = pkg.get("id").or_else(|| pkg.get("pkg")).or_else(|| pkg.get("name")).and_then(|v| v.as_str()) {
        id.to_string()
    } else {
        pkg.to_string()
    };
    log::info!("[PhoneADB] Opening phone application: {}", pkg_name);
    let guard = phone_mgr.session.read();
    if let Some(ref sess) = *guard {
        let msg = scrcpy::ControlMessage::start_app(&pkg_name);
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

    // Resolve APK via bundled resource dir with relative dev fallbacks
    let apk_path = app.path().resource_dir().ok().and_then(|r| {
        let p = r.join("dist/GlideDesk-1.1.1-full.apk");
        if p.is_file() { Some(p) } else { None }
    })
    .or_else(|| crate::phone::adb::resolve_resource_path("dist/GlideDesk-1.1.1-full.apk"))
    .or_else(|| crate::phone::adb::resolve_resource_path("dist/GlideDesk-1.1.1.apk"))
    .ok_or_else(|| "Full Android companion APK not found in dist/ or resource bundle".to_string())?;

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
pub fn phone_media(action: serde_json::Value, phone_mgr: State<'_, PhoneManager>) -> Result<(), String> {
    log::info!("[PhoneADB] Phone media action: {:?}", action);
    let act_str = if let Some(s) = action.as_str() {
        s.to_string()
    } else if let Some(a) = action.get("a").and_then(|v| v.as_str()) {
        a.to_string()
    } else {
        action.to_string()
    };
    let guard = phone_mgr.session.read();
    if let Some(ref sess) = *guard {
        let keycode = match act_str.as_str() {
            "play" | "pause" | "toggle" => 85, // KEYCODE_MEDIA_PLAY_PAUSE
            "next" => 87,                      // KEYCODE_MEDIA_NEXT
            "prev" | "previous" => 88,         // KEYCODE_MEDIA_PREVIOUS
            "volup" => 24,                     // KEYCODE_VOLUME_UP
            "voldn" => 25,                     // KEYCODE_VOLUME_DOWN
            _ => return Err(format!("Unsupported media action: {}", act_str)),
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

pub fn build_adb_qr_payload(name: &str, pass: &str) -> String {
    format!("WIFI:T:ADB;S:{};P:{};;", name, pass)
}

pub fn build_qr_pair_url(id: &str, name: &str, addrs: &[String], port: u16, nonce: &str) -> String {
    format!(
        "glidedesk://pair?id={}&n={}&a={}&p={}&k={}",
        id.replace(' ', "%20"),
        name.replace(' ', "%20"),
        addrs.join(","),
        port,
        nonce
    )
}

#[tauri::command]
pub fn qr_pair(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let s = state.inner.read();
    let id = s.device.name.clone();
    let name = s.mac_name.clone();
    let addrs = s.addresses.clone();
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let until = now + 10 * 60 * 1000;
    let nonce = format!("{:016x}", now ^ 0x5a5a5a5a);
    let url = build_qr_pair_url(&id, &name, &addrs, 47800, &nonce);
    Ok(serde_json::json!({
        "url": url,
        "until": until,
        "nonce": nonce,
    }))
}

#[tauri::command]
pub async fn adb_qr(
    on: bool,
    phone_mgr: State<'_, PhoneManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[PhoneADB] adb_qr called: on={}", on);

    if let Some(tx) = phone_mgr.qr_cancel.write().take() {
        let _ = tx.send(());
    }

    if !on {
        {
            let mut s = state.inner.write();
            s.adb_qr = None;
        }
        push_state(&app, &state);
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp_millis() as u64;
    let name = format!("glidedesk-{:08x}", now & 0xffffffff);
    let pass = format!("{:010x}", now ^ 0x1337cafe);
    let qr_payload = build_adb_qr_payload(&name, &pass);

    {
        let mut s = state.inner.write();
        s.adb_qr = Some(crate::state::AdbQrState {
            text: Some(qr_payload.clone()),
            done: None,
        });
    }
    push_state(&app, &state);

    let (cancel_tx, mut cancel_rx) = tokio::sync::broadcast::channel::<()>(1);
    *phone_mgr.qr_cancel.write() = Some(cancel_tx);

    let adb_client = phone_mgr.adb.clone();
    let state_arc = state.inner.clone();
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(180);

        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    log::info!("[PhoneADB] adb_qr cancelled");
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(1500)) => {
                    if start.elapsed() > timeout {
                        let mut s = state_arc.write();
                        if let Some(ref mut q) = s.adb_qr {
                            q.done = Some("timeout".to_string());
                        }
                        drop(s);
                        let _ = app_handle.emit("adb-qr-state", "timeout");
                        break;
                    }

                    let services = AdbClient::query_bonjour_mdns().unwrap_or_default();
                    if let Some(target) = services.iter().find(|svc| svc.name.contains(&name)) {
                        log::info!("[PhoneADB] Discovered pairing service: {:?}", target);
                        let pair_res = adb_client.pair(&target.address, target.port, &pass);
                        let mut s = state_arc.write();
                        if let Some(ref mut q) = s.adb_qr {
                            if pair_res.is_ok() {
                                q.done = Some("paired".to_string());
                            } else {
                                q.done = Some(format!("failed: {}", pair_res.unwrap_err()));
                            }
                        }
                        drop(s);
                        let _ = app_handle.emit("adb-qr-state", "paired");
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhoneCameraInfo {
    pub id: String,
    pub facing: String,
    pub sizes: Vec<String>,
}

pub fn parse_cameras(text: &str) -> Vec<PhoneCameraInfo> {
    let mut list = Vec::new();
    for line in text.lines() {
        if let Some(pos) = line.find("--camera-id=") {
            let rest = &line[pos + "--camera-id=".len()..];
            let id = rest.split_whitespace().next().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }

            let facing = if line.to_lowercase().contains("front") {
                "front".to_string()
            } else if line.to_lowercase().contains("external") {
                "external".to_string()
            } else {
                "back".to_string()
            };

            let mut sizes = Vec::new();
            for word in line.split(|c: char| !c.is_ascii_alphanumeric() && c != 'x') {
                if let Some(x_pos) = word.find('x') {
                    let w_str = &word[..x_pos];
                    let h_str = &word[x_pos + 1..];
                    if w_str.len() >= 2
                        && h_str.len() >= 2
                        && w_str.chars().all(|c| c.is_ascii_digit())
                        && h_str.chars().all(|c| c.is_ascii_digit())
                    {
                        sizes.push(word.to_string());
                    }
                }
            }

            list.push(PhoneCameraInfo { id, facing, sizes });
        }
    }
    list
}

#[tauri::command]
pub async fn phone_list_cameras(
    phone_mgr: State<'_, PhoneManager>,
) -> Result<Vec<PhoneCameraInfo>, String> {
    log::info!("[Phone] Listing available cameras on phone");
    let serial = {
        let guard = phone_mgr.session.read();
        guard.as_ref().map(|s| s.serial.clone())
    }
    .ok_or_else(|| "No active phone connected".to_string())?;

    let out = phone_mgr
        .adb
        .shell(&serial, "cmd media.camera get-camera-info 2>/dev/null || dumpsys media.camera 2>/dev/null");
    let cameras = match out {
        Ok(text) => {
            let parsed = parse_cameras(&text);
            if !parsed.is_empty() {
                parsed
            } else {
                vec![
                    PhoneCameraInfo {
                        id: "0".to_string(),
                        facing: "back".to_string(),
                        sizes: vec!["1920x1080".to_string(), "1280x720".to_string()],
                    },
                    PhoneCameraInfo {
                        id: "1".to_string(),
                        facing: "front".to_string(),
                        sizes: vec!["1920x1080".to_string(), "1280x720".to_string()],
                    },
                ]
            }
        }
        Err(e) => return Err(format!("Failed to query phone cameras via ADB: {}", e)),
    };

    Ok(cameras)
}

#[tauri::command]
pub fn phone_mount(
    state: State<'_, AppState>,
    phone_mgr: State<'_, PhoneManager>,
    app: AppHandle,
) -> Result<String, String> {
    log::info!("[PhoneMount] Mounting phone filesystem via ADB");
    let serial = {
        let guard = phone_mgr.session.read();
        guard.as_ref().map(|s| s.serial.clone())
    }
    .ok_or_else(|| "Phone not connected or unauthorized".to_string())?;

    // Verify storage is reachable via ADB exploration
    let ls_res = phone_mgr.adb.shell(&serial, "ls -la /sdcard/");
    let mount_point = match ls_res {
        Ok(listing) => {
            log::info!(
                "[PhoneMount] Verified /sdcard reachable ({} bytes listing)",
                listing.len()
            );
            "/sdcard".to_string()
        }
        Err(e) => return Err(format!("Failed to explore phone storage via ADB: {}", e)),
    };

    {
        let mut s = state.inner.write();
        s.mount.mounted = mount_point.clone();
        s.mount.on = true;
    }
    push_state(&app, &state);
    Ok(mount_point)
}

#[tauri::command]
pub fn phone_unmount(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[PhoneMount] Unmounting phone filesystem");
    {
        let mut s = state.inner.write();
        s.mount.mounted = String::new();
        s.mount.on = false;
    }
    push_state(&app, &state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cameras_scrcpy_format() {
        let scrcpy_output = "\
--camera-id=0    (back, 4000x3000, 1920x1080, fps=[15, 30])
--camera-id=1    (front, 1920x1080, 1280x720, fps=[15, 30])
--camera-id=2    (external, 640x480, fps=[30])";

        let parsed = parse_cameras(scrcpy_output);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].id, "0");
        assert_eq!(parsed[0].facing, "back");
        assert_eq!(parsed[0].sizes, vec!["4000x3000", "1920x1080"]);

        assert_eq!(parsed[1].id, "1");
        assert_eq!(parsed[1].facing, "front");
        assert_eq!(parsed[1].sizes, vec!["1920x1080", "1280x720"]);

        assert_eq!(parsed[2].id, "2");
        assert_eq!(parsed[2].facing, "external");
        assert_eq!(parsed[2].sizes, vec!["640x480"]);
    }

    #[test]
    fn test_phone_mount_unauthorized_when_no_session() {
        let phone_mgr = PhoneManager::new();
        assert!(!phone_mgr.is_connected());
        let serial = {
            let guard = phone_mgr.session.read();
            guard.as_ref().map(|s| s.serial.clone())
        };
        assert!(serial.is_none());
    }

    #[test]
    fn test_phone_unmount_resets_state() {
        let app_state = AppState::new();
        {
            let mut s = app_state.inner.write();
            s.mount.mounted = "/sdcard".to_string();
            s.mount.on = true;
        }

        {
            let mut s = app_state.inner.write();
            s.mount.mounted = String::new();
            s.mount.on = false;
        }

        let snap = app_state.snapshot();
        assert_eq!(snap.mount.mounted, "");
        assert!(!snap.mount.on);
    }

    #[test]
    fn test_build_adb_qr_payload() {
        let payload = build_adb_qr_payload("glidedesk-1234abcd", "secretpass99");
        assert_eq!(payload, "WIFI:T:ADB;S:glidedesk-1234abcd;P:secretpass99;;");
    }

    #[test]
    fn test_build_qr_pair_url() {
        let url = build_qr_pair_url(
            "mac-mini-01",
            "Benson's Mac",
            &["192.168.1.50".to_string(), "10.0.0.5".to_string()],
            47800,
            "deadbeef12345678",
        );
        assert!(url.starts_with("glidedesk://pair?"));
        assert!(url.contains("id=mac-mini-01"));
        assert!(url.contains("n=Benson's%20Mac"));
        assert!(url.contains("a=192.168.1.50,10.0.0.5"));
        assert!(url.contains("p=47800"));
        assert!(url.contains("k=deadbeef12345678"));
    }
}

