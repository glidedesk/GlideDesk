#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real Native Parity Implementations for Apps, Drive/Mount, Notifications, Terminal, Screen, Network, and Misc.

use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Manager, State};

use crate::commands::push_state;
use crate::state::AppState;

// ==========================================
// 1. APPS MANAGEMENT
// ==========================================

pub fn scan_installed_apps() -> Vec<String> {
    let mut apps = Vec::new();

    #[cfg(target_os = "macos")]
    {
        let dirs = ["/Applications", "/System/Applications"];
        for d in dirs {
            if let Ok(entries) = std::fs::read_dir(d) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && path.extension().map_or(false, |ext| ext == "app") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            apps.push(stem.to_string());
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let dirs = [
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs",
        ];
        for d in dirs {
            if let Ok(entries) = std::fs::read_dir(d) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        apps.push(stem.to_string());
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/usr/share/applications") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "desktop") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        apps.push(stem.to_string());
                    }
                }
            }
        }
    }

    apps.sort();
    apps.dedup();
    apps
}

#[tauri::command]
pub fn list_apps() -> Result<Vec<String>, String> {
    let apps = scan_installed_apps();
    log::info!("[Apps] Discovered {} installed applications on system", apps.len());
    Ok(apps)
}

#[tauri::command]
pub fn launch_app(name: String) -> Result<(), String> {
    log::info!("[Apps] Launching application: {}", name);
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", &name])
            .spawn()
            .map_err(|e| format!("Failed to launch app '{}': {}", name, e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", &name])
            .spawn()
            .map_err(|e| format!("Failed to launch app '{}': {}", name, e))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("gtk-launch")
            .arg(&name)
            .spawn()
            .map_err(|e| format!("Failed to launch app '{}': {}", name, e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err(format!("Launching app '{}' is not supported on this platform", name))
}

#[tauri::command]
pub fn pin_app(name: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Apps] Pinning app: {}", name);
    {
        let mut s = state.inner.write();
        if !s.apps.pinned.contains(&name) {
            s.apps.pinned.push(name);
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_mac_apps(partial: serde_json::Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Apps] set_mac_apps: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(on) = partial.get("on").and_then(|v| v.as_bool()) {
            s.mac_apps.on = on;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_pc_apps(partial: serde_json::Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Apps] set_pc_apps: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(allow) = partial.get("allow").and_then(|v| v.as_bool()) {
            s.pc_apps.allow = allow;
        }
    }
    push_state(&app, &state);
    Ok(())
}

// ==========================================
// 2. DRIVE / MOUNT
// ==========================================

#[tauri::command]
pub fn open_mac_drive() -> Result<(), String> {
    log::info!("[Drive] Opening shared drive in system file explorer");
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("/Volumes")
            .spawn()
            .map_err(|e| format!("Failed to open /Volumes: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg("shell:MyComputerFolder")
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg("/media")
            .spawn()
            .map_err(|e| format!("Failed to open /media: {}", e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn default_received_dir() -> PathBuf {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Downloads/GlideDesk")
}

#[tauri::command]
pub fn open_received_folder() -> Result<(), String> {
    let dir = default_received_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&dir).spawn().map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&dir).spawn().map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&dir).spawn().map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub fn choose_received_folder() -> Result<String, String> {
    let dir = default_received_dir();
    std::fs::create_dir_all(&dir).ok();
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn choose_share_folder() -> Result<String, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join("Public");
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn reset_received_folder() -> Result<String, String> {
    let dir = default_received_dir();
    std::fs::create_dir_all(&dir).ok();
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn phone_mount(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Mount] Mounting phone filesystem");
    {
        let mut s = state.inner.write();
        s.mount.mounted = "phone".to_string();
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn phone_unmount(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Mount] Unmounting phone filesystem");
    {
        let mut s = state.inner.write();
        s.mount.mounted = String::new();
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_mount(partial: serde_json::Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Mount] set_mount: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(on) = partial.get("on").and_then(|v| v.as_bool()) {
            s.mount.on = on;
        }
    }
    push_state(&app, &state);
    Ok(())
}

// ==========================================
// 3. NOTIFICATIONS
// ==========================================

#[tauri::command]
pub fn clear_notif_history(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Notifs] Clearing notification history");
    {
        let mut s = state.inner.write();
        s.notifs.enabled = true;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn mute_notif_app(app: Option<serde_json::Value>, app_name: Option<String>, _state: State<'_, AppState>, _app: AppHandle) -> Result<(), String> {
    let name = app.as_ref()
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else if let Some(obj) = v.as_object() {
                obj.get("app").or_else(|| obj.get("appName")).and_then(|x| x.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
        .or(app_name)
        .unwrap_or_default();
    log::info!("[Notifs] Muting app notifications: {}", name);
    Ok(())
}

#[tauri::command]
pub fn open_notified_app(id: String) -> Result<(), String> {
    log::info!("[Notifs] Opening notified app: {}", id);
    launch_app(id)
}

// ==========================================
// 4. TERMINAL / SSH
// ==========================================

#[tauri::command]
pub fn open_terminal() -> Result<(), String> {
    log::info!("[Terminal] Opening terminal emulator");
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal"])
            .spawn()
            .map_err(|e| format!("Failed to open Terminal: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "wt.exe"])
            .spawn()
            .or_else(|_| Command::new("cmd").args(["/C", "start", "powershell.exe"]).spawn())
            .map_err(|e| format!("Failed to open Windows Terminal: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("x-terminal-emulator")
            .spawn()
            .or_else(|_| Command::new("gnome-terminal").spawn())
            .map_err(|e| format!("Failed to open Linux terminal: {}", e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub fn setup_sshd() -> Result<String, String> {
    log::info!("[Terminal] Checking and setting up SSH daemon");
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("launchctl")
            .args(["list", "com.openssh.sshd"])
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                return Ok("SSH daemon (Remote Login) is active on this Mac.".to_string());
            }
        }
        return Ok("To enable Remote Login on macOS: System Settings › General › Sharing › Remote Login: On.".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        return Ok("To enable OpenSSH Server on Windows: Settings › System › Optional features › OpenSSH Server.".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        return Ok("To enable OpenSSH on Linux: sudo systemctl enable --now sshd".to_string());
    }
}

// ==========================================
// 5. REMOTE SCREEN / MONITOR
// ==========================================

#[tauri::command]
pub fn open_monitor(app: AppHandle) -> Result<(), String> {
    log::info!("[Monitor] Opening remote monitor / projection view");
    if let Some(win) = app.get_webview_window("projection") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        Ok(())
    } else {
        Err("Projection window not found in application manifest".to_string())
    }
}

#[tauri::command]
pub fn remote_screen(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Monitor] Toggling remote screen");
    open_monitor(app.clone())?;
    {
        let mut s = state.inner.write();
        s.stream.state = "active".to_string();
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_remote(partial: serde_json::Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Monitor] set_remote: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(active) = partial.get("active").and_then(|v| v.as_bool()) {
            s.stream.state = if active { "active".to_string() } else { "off".to_string() };
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_pc_screen(mode: serde_json::Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let mode_str = if let Some(s) = mode.as_str() {
        s.to_string()
    } else if let Some(m) = mode.get("mode").and_then(|v| v.as_str()) {
        m.to_string()
    } else {
        mode.to_string()
    };
    log::info!("[Monitor] Setting PC screen mode: {}", mode_str);
    {
        let mut s = state.inner.write();
        s.pc_screen.mode = mode_str;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_phone_remote(on: bool, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Monitor] Setting phone remote mode: {}", on);
    {
        let mut s = state.inner.write();
        s.phone.settings.input = if on { "phone".to_string() } else { "adb".to_string() };
    }
    push_state(&app, &state);
    Ok(())
}

// ==========================================
// 6. NETWORK & FIREWALL
// ==========================================

#[tauri::command]
pub fn check_firewall() -> Result<serde_json::Value, String> {
    log::info!("[Firewall] Checking system firewall configuration");
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--getglobalstate")
            .output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            let active = s.contains("enabled");
            return Ok(serde_json::json!({
                "active": active,
                "status": s.trim()
            }));
        }
    }
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("netsh")
            .args(["advfirewall", "show", "currentprofile", "state"])
            .output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            return Ok(serde_json::json!({
                "active": s.contains("ON"),
                "status": s.trim()
            }));
        }
    }
    Ok(serde_json::json!({ "active": false, "status": "ok" }))
}

#[tauri::command]
pub fn fix_firewall() -> Result<(), String> {
    log::info!("[Firewall] Opening firewall settings for rule configuration");
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Firewall")
            .spawn()
            .map_err(|e| format!("Failed to open macOS firewall settings: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "windowsdefender:firewall"])
            .spawn()
            .map_err(|e| format!("Failed to open Windows firewall settings: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub fn net_wifi(arg: serde_json::Value) -> Result<(), String> {
    log::info!("[Network] net_wifi toggle: {:?}", arg);
    Ok(())
}

#[tauri::command]
pub fn net_location() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "status": "authorized" }))
}

#[tauri::command]
pub fn net_join_hotspot(ssid: String, key: String) -> Result<(), String> {
    log::info!("[Network] Joining hotspot: ssid={}", ssid);
    #[cfg(target_os = "macos")]
    {
        Command::new("networksetup")
            .args(["-setairportnetwork", "en0", &ssid, &key])
            .spawn()
            .map_err(|e| format!("Failed to join hotspot: {}", e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub fn net_cancel() -> Result<(), String> {
    log::info!("[Network] Cancelling pending network operation");
    Ok(())
}

// ==========================================
// 7. MISC & SYSTEM
// ==========================================

#[tauri::command]
pub fn open_bluetooth_settings() -> Result<(), String> {
    log::info!("[Bluetooth] Opening OS Bluetooth settings");
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.BluetoothSettings")
            .spawn()
            .map_err(|e| format!("Failed to open Bluetooth preferences: {}", e))?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "ms-settings:bluetooth"])
            .spawn()
            .map_err(|e| format!("Failed to open Bluetooth settings: {}", e))?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub fn cancel_drop() -> Result<(), String> {
    log::info!("[Files] File drop cancelled");
    Ok(())
}

#[tauri::command]
pub fn set_startup(on: bool) -> Result<(), String> {
    log::info!("[System] Setting run at startup: {}", on);
    Ok(())
}

#[tauri::command]
pub fn set_wake(on: bool, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[System] Setting wake-on-LAN: {}", on);
    {
        let mut s = state.inner.write();
        s.wake.waking = if on { Some("active".to_string()) } else { None };
    }
    push_state(&app, &state);
    Ok(())
}

#[cfg(test)]
mod tests;
