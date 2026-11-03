// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Streaming and Screen Sharing Configuration Module (TwinDesk Phase 4.1 Parity)

use tauri::{AppHandle, State, Manager};
use crate::state::AppState;
use crate::commands::push_state;
use crate::media::NativeBridgeManager;
use crate::network::{NetworkManager, WireMessage};
use std::process::Command;

pub fn validate_display_mode(mode: &str) -> Result<&'static str, String> {
    match mode.to_ascii_lowercase().as_str() {
        "off" => Ok("off"),
        "extend" => Ok("extend"),
        "mirror" => Ok("mirror"),
        _ => Err(format!("Invalid display mode '{}'. Must be 'off', 'extend', or 'mirror'", mode)),
    }
}

pub fn validate_stream_max_height(h: u32) -> Result<u32, String> {
    match h {
        0 | 720 | 1080 | 1440 | 2160 => Ok(h),
        _ => Err(format!("Unsupported stream max height: {}p. Must be 0 (native), 720, 1080, 1440, or 2160", h)),
    }
}

pub fn validate_bitrate(mbps: u32) -> Result<u32, String> {
    if (2..=100).contains(&mbps) {
        Ok(mbps)
    } else {
        Err(format!("Bitrate must be between 2 and 100 Mbps, got {}", mbps))
    }
}

#[tauri::command]
pub fn set_sharing(
    on: bool,
    by: Option<String>,
    state: State<'_, AppState>,
    bridge: State<'_, NativeBridgeManager>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Sharing] set_sharing: on={}, by={:?}", on, by);
    {
        let mut s = state.inner.write();
        s.stream.share = if on { "on".to_string() } else { "off".to_string() };
        s.stream.state = if on { "on".to_string() } else { "off".to_string() };
        s.pc_screen.status = if on { "live".to_string() } else { "off".to_string() };
        if !on {
            s.pc_screen.mode = "off".to_string();
        }
    }

    if on {
        bridge.start_broadcasting(app.clone());
        let _ = bridge.screen_engine.start_streaming(30);
    } else {
        bridge.screen_engine.stop_streaming();
    }

    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_sharing_opts(
    opts: serde_json::Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Sharing] set_sharing_opts: {:?}", opts);
    {
        let mut s = state.inner.write();
        if let Some(obj) = opts.as_object() {
            if let Some(remote) = obj.get("remote").and_then(|v| v.as_bool()) {
                s.pc_screen.remote = remote;
            }
            s.stream.remote = Some(opts.clone());
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_stream_input(
    on: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Sharing] set_stream_input: {}", on);
    {
        let mut s = state.inner.write();
        s.stream.stream_input = on;
        s.pc_screen.input = on;
    }
    if let Some(net) = app.try_state::<NetworkManager>() {
        net.send_packet_sync(WireMessage {
            t: "stream.input".to_string(),
            on: Some(on),
            ..Default::default()
        });
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn select_display(
    display_id: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Sharing] select_display: {:?}", display_id);

    if let Some(ref id) = display_id {
        let available = app.available_monitors().map_err(|e| e.to_string())?;
        let mut found = false;
        for (idx, m) in available.iter().enumerate() {
            let def_id = format!("display-{}", idx);
            let idx_str = idx.to_string();
            let name = m.name().map(|n| n.to_string()).unwrap_or_default();
            if id == &def_id || id == &idx_str || id == &name {
                found = true;
                break;
            }
        }
        if !found && !available.is_empty() {
            return Err(format!("Display '{}' not found among available monitors", id));
        }
    }

    {
        let mut s = state.inner.write();
        s.stream.display_id = display_id.clone();
        s.pc_screen.monitor = display_id;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_display_mode(
    mode: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let validated = validate_display_mode(&mode)?;
    log::info!("[Sharing] set_display_mode: {}", validated);
    {
        let mut s = state.inner.write();
        s.pc_screen.mode = validated.to_string();
        if validated == "mirror" {
            s.pc_screen.virtual_settings.enabled = false;
            s.stream.display_id = Some("display-0".to_string());
            s.pc_screen.monitor = Some("display-0".to_string());
        } else if validated == "extend" {
            s.pc_screen.virtual_settings.enabled = true;
            s.stream.display_id = None;
            s.pc_screen.monitor = None;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_virtual(
    partial: serde_json::Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Sharing] set_virtual: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(obj) = partial.as_object() {
            if let Some(en) = obj.get("enabled").and_then(|v| v.as_bool()) {
                s.pc_screen.virtual_settings.enabled = en;
            }
            if let Some(w) = obj.get("width").and_then(|v| v.as_u64()) {
                s.pc_screen.virtual_settings.width = w as u32;
            }
            if let Some(h) = obj.get("height").and_then(|v| v.as_u64()) {
                s.pc_screen.virtual_settings.height = h as u32;
            }
            if let Some(hz) = obj.get("hz").and_then(|v| v.as_u64()) {
                s.pc_screen.virtual_settings.hz = hz as u32;
            }
            s.pc_screen.extend_px = (
                s.pc_screen.virtual_settings.width,
                s.pc_screen.virtual_settings.height,
            );
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_stream_max_height(
    h: u32,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let validated = validate_stream_max_height(h)?;
    log::info!("[Sharing] set_stream_max_height: {}", validated);
    {
        let mut s = state.inner.write();
        s.stream.max_height = validated;
        s.pc_screen.max_height = validated;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_bitrate(
    mbps: u32,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    let validated = validate_bitrate(mbps)?;
    log::info!("[Sharing] set_bitrate: {}", validated);
    {
        let mut s = state.inner.write();
        s.stream.bitrate_mbps = validated;
        s.pc_screen.mbps = validated;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_screen_sound(
    on: bool,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Sharing] set_screen_sound: {}", on);
    {
        let mut s = state.inner.write();
        s.stream.audio = on;
        s.pc_screen.sound = on;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn open_sharing_settings() -> Result<(), String> {
    log::info!("[Sharing] open_sharing_settings");
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Sharing-Settings.extension")
            .spawn()
            .map_err(|e| format!("Failed to open macOS Sharing Settings: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "ms-settings:display"])
            .spawn()
            .map_err(|e| format!("Failed to open Windows Display Settings: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("gnome-control-center")
            .arg("display")
            .spawn()
            .map_err(|e| format!("Failed to open Linux Display Settings: {}", e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_display_mode() {
        assert_eq!(validate_display_mode("mirror").unwrap(), "mirror");
        assert_eq!(validate_display_mode("extend").unwrap(), "extend");
        assert_eq!(validate_display_mode("off").unwrap(), "off");
        assert_eq!(validate_display_mode("MIRROR").unwrap(), "mirror");
        assert!(validate_display_mode("invalid_mode").is_err());
    }

    #[test]
    fn test_validate_stream_max_height() {
        assert_eq!(validate_stream_max_height(0).unwrap(), 0);
        assert_eq!(validate_stream_max_height(720).unwrap(), 720);
        assert_eq!(validate_stream_max_height(1080).unwrap(), 1080);
        assert_eq!(validate_stream_max_height(1440).unwrap(), 1440);
        assert_eq!(validate_stream_max_height(2160).unwrap(), 2160);
        assert!(validate_stream_max_height(999).is_err());
        assert!(validate_stream_max_height(480).is_err());
    }

    #[test]
    fn test_validate_bitrate() {
        assert_eq!(validate_bitrate(2).unwrap(), 2);
        assert_eq!(validate_bitrate(20).unwrap(), 20);
        assert_eq!(validate_bitrate(100).unwrap(), 100);
        assert!(validate_bitrate(1).is_err());
        assert!(validate_bitrate(0).is_err());
        assert!(validate_bitrate(101).is_err());
    }
}
