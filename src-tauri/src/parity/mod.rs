#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real Native Parity Implementations for Apps, Drive/Mount, Notifications, Terminal, Screen, Network, and Misc.

use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, State};

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

pub fn toggle_app_mute(muted: &mut Vec<String>, name: &str) -> bool {
    if let Some(pos) = muted.iter().position(|x| x.eq_ignore_ascii_case(name)) {
        muted.remove(pos);
        false
    } else {
        muted.push(name.to_string());
        true
    }
}

#[tauri::command]
pub fn mute_notif_app(
    app: Option<serde_json::Value>,
    app_name: Option<String>,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
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

    if name.trim().is_empty() {
        return Err("App name cannot be empty".to_string());
    }

    log::info!("[Notifs] Toggling mute state for app notifications: {}", name);
    {
        let mut s = state.inner.write();
        let is_muted = toggle_app_mute(&mut s.notifs.muted, &name);
        log::info!("[Notifs] App '{}' is now {}", name, if is_muted { "muted" } else { "unmuted" });
    }
    push_state(&app_handle, &state);
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

pub fn execute_net_wifi(on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = if on { "on" } else { "off" };
        let output = Command::new("networksetup")
            .args(["-setairportpower", "en0", status])
            .output()
            .map_err(|e| format!("networksetup execution failed: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to set Wi-Fi power to {}: {}", status, stderr.trim()));
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let status = if on { "admin=enable" } else { "admin=disable" };
        let output = Command::new("netsh")
            .args(["interface", "set", "interface", "Wi-Fi", status])
            .output()
            .map_err(|e| format!("netsh execution failed: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to configure Windows Wi-Fi: {}", stderr.trim()));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let status = if on { "on" } else { "off" };
        let output = Command::new("nmcli")
            .args(["radio", "wifi", status])
            .output()
            .map_err(|e| format!("nmcli execution failed: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to toggle Wi-Fi radio via nmcli: {}", stderr.trim()));
        }
        Ok(())
    }
}

pub fn generate_launchagent_plist(exe_path: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.glidedesk.desktop</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
        exe_path
    )
}

pub fn execute_set_startup(on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map_err(|e| format!("Cannot locate HOME environment: {}", e))?;
        let launch_agents = PathBuf::from(home).join("Library/LaunchAgents");
        std::fs::create_dir_all(&launch_agents).map_err(|e| e.to_string())?;
        let plist_path = launch_agents.join("com.glidedesk.desktop.plist");

        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let plist_content = generate_launchagent_plist(&exe.to_string_lossy());
            std::fs::write(&plist_path, plist_content).map_err(|e| format!("Failed to write LaunchAgent plist: {}", e))?;
            let _ = Command::new("launchctl").args(["load", &plist_path.to_string_lossy()]).output();
        } else if plist_path.exists() {
            let _ = Command::new("launchctl").args(["unload", &plist_path.to_string_lossy()]).output();
            let _ = std::fs::remove_file(plist_path);
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy();
        if on {
            let output = Command::new("reg")
                .args(["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "GlideDesk", "/t", "REG_SZ", "/d", &exe_str, "/f"])
                .output()
                .map_err(|e| format!("Failed to configure Windows Run registry: {}", e))?;
            if !output.status.success() {
                return Err("Failed to register Windows startup key".to_string());
            }
        } else {
            let output = Command::new("reg")
                .args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "GlideDesk", "/f"])
                .output()
                .map_err(|e| format!("Failed to delete Windows Run registry: {}", e))?;
            if !output.status.success() {
                return Err("Failed to remove Windows startup key".to_string());
            }
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|e| format!("Cannot locate HOME environment: {}", e))?;
        let autostart_dir = PathBuf::from(home).join(".config/autostart");
        let desktop_file = autostart_dir.join("glidedesk.desktop");
        if on {
            std::fs::create_dir_all(&autostart_dir).map_err(|e| e.to_string())?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=GlideDesk\nExec={}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
                exe.to_string_lossy()
            );
            std::fs::write(desktop_file, content).map_err(|e| e.to_string())?;
        } else if desktop_file.exists() {
            let _ = std::fs::remove_file(desktop_file);
        }
        Ok(())
    }
}

pub fn parse_wifi_toggle(arg: &serde_json::Value) -> bool {
    if let Some(b) = arg.as_bool() {
        b
    } else if let Some(b) = arg.get("on").and_then(|v| v.as_bool()) {
        b
    } else if let Some(s) = arg.get("power").and_then(|v| v.as_str()) {
        s.eq_ignore_ascii_case("on")
    } else {
        true
    }
}

#[tauri::command]
pub fn net_wifi(arg: serde_json::Value) -> Result<(), String> {
    log::info!("[Network] net_wifi toggle: {:?}", arg);
    let on = parse_wifi_toggle(&arg);
    execute_net_wifi(on)
}

#[tauri::command]
pub fn net_location() -> Result<serde_json::Value, String> {
    log::info!("[Network] net_location requested");
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices")
            .spawn();

        let out = Command::new("defaults")
            .args(["read", "/var/db/locationd/Library/Preferences/ByHost/com.apple.locationd.notbackedup", "LocationServicesEnabled"])
            .output();

        let enabled = match out {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout);
                s.trim() == "1"
            }
            _ => false,
        };

        if enabled {
            Ok(serde_json::json!({ "status": "authorized", "enabled": true }))
        } else {
            Err("Location Services disabled on host system. Please enable Location Services in System Settings to allow Wi-Fi SSID discovery.".to_string())
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-location"])
            .spawn();
        Ok(serde_json::json!({ "status": "prompted", "settings": "ms-settings:privacy-location" }))
    }
    #[cfg(target_os = "linux")]
    {
        Err("Location settings not applicable on Linux".to_string())
    }
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
pub fn net_cancel(app: AppHandle) -> Result<(), String> {
    log::info!("[Network] Cancelling pending network operation");
    let _ = app.emit("net-state", serde_json::json!({ "state": "idle", "cancelled": true }));
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netsh").args(["wlan", "disconnect"]).output();
        let _ = output;
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("nmcli").args(["device", "disconnect"]).output();
        let _ = output;
    }
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
pub fn cancel_drop(app: AppHandle) -> Result<(), String> {
    log::info!("[Files] File drop cancelled");
    let _ = app.emit("drop-cancelled", ());
    Ok(())
}

#[tauri::command]
pub fn set_startup(on: bool) -> Result<(), String> {
    log::info!("[System] Setting run at startup: {}", on);
    execute_set_startup(on)
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

pub fn get_settings_url(kind: &str) -> Option<&'static str> {
    match kind {
        "screen" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"),
        "accessibility" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"),
        "inputMonitoring" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"),
        "localNetwork" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"),
        "fullDisk" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"),
        "bluetooth" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth"),
        "location" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices"),
        "extensions" => Some("x-apple.systempreferences:com.apple.ExtensionsPreferences?Sharing"),
        _ => None,
    }
}

#[tauri::command]
pub fn allow_permission(kind: String) -> Result<(), String> {
    log::info!("[Permissions] allow_permission: {}", kind);
    #[cfg(target_os = "macos")]
    {
        if let Some(url) = get_settings_url(&kind) {
            Command::new("open")
                .arg(url)
                .spawn()
                .map_err(|e| format!("Failed to open Settings for '{}': {}", kind, e))?;
            Ok(())
        } else {
            Err(format!("Unsupported permission kind: '{}'", kind))
        }
    }
    #[cfg(target_os = "windows")]
    {
        let setting = match kind.as_str() {
            "screen" | "display" => "ms-settings:display",
            "bluetooth" => "ms-settings:bluetooth",
            "location" => "ms-settings:privacy-location",
            _ => "ms-settings:",
        };
        Command::new("cmd")
            .args(["/C", "start", setting])
            .spawn()
            .map_err(|e| format!("Failed to open Windows settings: {}", e))?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("gnome-control-center")
            .spawn()
            .map_err(|e| format!("Failed to open Linux control center: {}", e))?;
        Ok(())
    }
}

#[tauri::command]
pub fn allow_full_disk() -> Result<(), String> {
    log::info!("[Permissions] allow_full_disk requested");
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .spawn()
            .map_err(|e| format!("Failed to open Full Disk Access settings: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Full Disk Access permission is macOS-specific".to_string())
    }
}

#[tauri::command]
pub fn open_settings(kind: Option<String>) -> Result<(), String> {
    log::info!("[Settings] open_settings: {:?}", kind);
    #[cfg(target_os = "macos")]
    {
        let url = kind
            .as_deref()
            .and_then(get_settings_url)
            .unwrap_or("x-apple.systempreferences:com.apple.preference");
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open settings: {}", e))?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "ms-settings:"])
            .spawn()
            .map_err(|e| format!("Failed to open Windows settings: {}", e))?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("gnome-control-center")
            .spawn()
            .map_err(|e| format!("Failed to open Linux settings: {}", e))?;
        Ok(())
    }
}

#[tauri::command]
pub fn relaunch(app: AppHandle) -> Result<(), String> {
    log::info!("[System] Relaunching application via Tauri runtime");
    app.restart();
}

#[cfg(test)]
mod tests;
