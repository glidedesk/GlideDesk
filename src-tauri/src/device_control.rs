// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Device Control (devctl), Overview Settings, and HUD Display Module (TwinDesk Phase 4.3 Parity)

use tauri::{AppHandle, Emitter, Manager, State};
use crate::state::AppState;
use crate::commands::push_state;
use crate::network::{NetworkManager, WireMessage};
use crate::phone::PhoneManager;
use std::process::Command;

pub const VALID_DEVCTL_ACTIONS: &[&str] = &[
    "ring", "ringStop", "lock", "sleep", "bluetooth", "dnd", "volume", "qs", "ringer", "mediaHere"
];

pub fn validate_devctl_action(act: &str) -> Result<String, String> {
    if VALID_DEVCTL_ACTIONS.contains(&act) {
        Ok(act.to_string())
    } else {
        Err(format!("Unsupported device action: '{}'. Valid actions: {:?}", act, VALID_DEVCTL_ACTIONS))
    }
}

pub fn update_overview_settings(cur: &mut crate::state::OverviewSettings, obj: &serde_json::Map<String, serde_json::Value>) {
    if let Some(v) = obj.get("media").and_then(|x| x.as_bool()) { cur.media = v; }
    if let Some(v) = obj.get("qs").and_then(|x| x.as_bool()) { cur.qs = v; }
    if let Some(v) = obj.get("notifs").and_then(|x| x.as_bool()) { cur.notifs = v; }
    if let Some(v) = obj.get("apps").and_then(|x| x.as_bool()) { cur.apps = v; }
    if let Some(v) = obj.get("clipboard").and_then(|x| x.as_bool()) { cur.clipboard = v; }
    if let Some(v) = obj.get("files").and_then(|x| x.as_bool()) { cur.files = v; }
}

#[tauri::command]
pub fn set_overview(
    overview: serde_json::Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Overview] set_overview: {:?}", overview);
    if let Some(obj) = overview.as_object() {
        let mut s = state.inner.write();
        update_overview_settings(&mut s.overview, obj);
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn device_do(
    c: serde_json::Value,
    state: State<'_, AppState>,
    phone_mgr: State<'_, PhoneManager>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[DeviceControl] device_do request: {:?}", c);
    let obj = c.as_object().ok_or_else(|| "device_do expects an object".to_string())?;
    let raw_act = obj.get("do").and_then(|v| v.as_str()).unwrap_or_default();
    let action = validate_devctl_action(raw_act)?;

    // 1. If phone is connected, route via ADB when applicable
    let phone_serial = phone_mgr.session.read().as_ref().map(|s| s.serial.clone());
    if let Some(serial) = phone_serial {
        match action.as_str() {
            "lock" => {
                let _ = phone_mgr.adb.shell(&serial, "input keyevent 26");
                return Ok(());
            }
            "qs" => {
                if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
                    let on = obj.get("on").and_then(|v| v.as_bool()).unwrap_or(true);
                    let flag = if on { "enable" } else { "disable" };
                    match id {
                        "wifi" => { let _ = phone_mgr.adb.shell(&serial, &format!("svc wifi {}", flag)); }
                        "data" => { let _ = phone_mgr.adb.shell(&serial, &format!("svc data {}", flag)); }
                        "bluetooth" => { let _ = phone_mgr.adb.shell(&serial, &format!("svc bluetooth {}", flag)); }
                        _ => {}
                    }
                }
                return Ok(());
            }
            "volume" => {
                if let Some(lvl) = obj.get("level").and_then(|v| v.as_f64()) {
                    let stream = obj.get("stream").and_then(|v| v.as_str()).unwrap_or("3");
                    let steps = (lvl * 15.0).round() as u32;
                    let cmd = format!("media volume --set {} --stream {}", steps, stream);
                    let _ = phone_mgr.adb.shell(&serial, &cmd);
                }
                return Ok(());
            }
            _ => {}
        }
    }

    // 2. Local OS Actions
    match action.as_str() {
        "lock" => {
            #[cfg(target_os = "macos")]
            {
                let _ = Command::new("pmset").arg("displaysleepnow").spawn();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("rundll32.exe").args(["user32.dll,LockWorkStation"]).spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("loginctl").args(["lock-session"]).spawn();
            }
        }
        "sleep" => {
            #[cfg(target_os = "macos")]
            {
                let _ = Command::new("pmset").arg("sleepnow").spawn();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("rundll32.exe").args(["powrprof.dll,SetSuspendState", "0,1,0"]).spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("systemctl").arg("suspend").spawn();
            }
        }
        "volume" => {
            if let Some(lvl) = obj.get("level").and_then(|v| v.as_f64()) {
                let pct = (lvl * 100.0).clamp(0.0, 100.0) as u32;
                #[cfg(target_os = "macos")]
                {
                    let script = format!("set volume output volume {}", pct);
                    let _ = Command::new("osascript").args(["-e", &script]).spawn();
                }
                #[cfg(target_os = "linux")]
                {
                    let arg = format!("{}%", pct);
                    let _ = Command::new("amixer").args(["set", "Master", &arg]).spawn();
                }
            }
        }
        _ => {}
    }

    // 3. Broadcast wire packet to remote peers if connected
    if let Some(net) = app.try_state::<NetworkManager>() {
        let msg = WireMessage {
            t: "dev.do".to_string(),
            text: Some(c.to_string()),
            ..Default::default()
        };
        net.send_packet_sync(msg);
    }

    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn show_hud(
    kind: String,
    text: String,
    sub: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[HUD] show_hud: kind={}, text={}, sub={:?}", kind, text, sub);
    let payload = serde_json::json!({
        "kind": kind,
        "text": text,
        "sub": sub.unwrap_or_default(),
    });
    let _ = app.emit("hud-show", &payload);
    Ok(())
}

#[tauri::command]
pub fn hide_hud(app: AppHandle) -> Result<(), String> {
    log::info!("[HUD] hide_hud");
    let _ = app.emit("hud-hide", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_device_action() {
        assert_eq!(validate_devctl_action("lock").unwrap(), "lock");
        assert_eq!(validate_devctl_action("sleep").unwrap(), "sleep");
        assert_eq!(validate_devctl_action("volume").unwrap(), "volume");
        assert_eq!(validate_devctl_action("qs").unwrap(), "qs");
        assert_eq!(validate_devctl_action("ring").unwrap(), "ring");
        assert!(validate_devctl_action("hackSystem").is_err());
    }

    #[test]
    fn test_overview_settings_mutation() {
        let mut ov = crate::state::OverviewSettings::default();
        assert!(ov.media);
        assert!(ov.qs);
        assert!(ov.notifs);

        let mut patch = serde_json::Map::new();
        patch.insert("media".to_string(), serde_json::Value::Bool(false));
        patch.insert("qs".to_string(), serde_json::Value::Bool(false));

        update_overview_settings(&mut ov, &patch);
        assert!(!ov.media);
        assert!(!ov.qs);
        assert!(ov.notifs);
        assert!(ov.clipboard);
    }
}
