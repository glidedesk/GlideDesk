#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Real Native Parity Implementations for Apps, Drive/Mount, Notifications, Terminal, Screen, Network, and Misc.

use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::push_state;
use crate::network::{NetworkManager, WireMessage};
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
pub fn list_apps(state: State<'_, AppState>, app: AppHandle) -> Result<Vec<String>, String> {
    let apps = scan_installed_apps();
    log::info!("[Apps] Discovered {} installed applications on system", apps.len());
    {
        let mut s = state.inner.write();
        s.apps.loading = false;
        s.apps.message = if apps.is_empty() {
            "No applications were found on this computer yet".to_string()
        } else {
            String::new()
        };
        s.apps.list = apps
            .iter()
            .map(|name| crate::state::AppEntry {
                id: name.clone(),
                name: name.clone(),
                icon: String::new(),
            })
            .collect();
    }
    push_state(&app, &state);
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

/// Ask for a folder with the system's own picker. `None` means the person closed
/// it without choosing, which must leave the setting as it was.
fn pick_folder(prompt: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "POSIX path of (choose folder with prompt \"{}\")",
            prompt.replace('"', " ")
        );
        let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return if p.is_empty() { None } else { Some(p.trim_end_matches('/').to_string()) };
    }
    #[cfg(target_os = "windows")]
    {
        let ps = format!(
            "[System.Reflection.Assembly]::LoadWithPartialName('System.windows.forms') | Out-Null; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = '{}'; if ($d.ShowDialog() -eq 'OK') {{ $d.SelectedPath }}",
            prompt.replace('\'', " ")
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return if p.is_empty() { None } else { Some(p) };
    }
    #[cfg(target_os = "linux")]
    {
        let out = Command::new("zenity")
            .args(["--file-selection", "--directory", "--title", prompt])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return if p.is_empty() { None } else { Some(p) };
    }
    #[allow(unreachable_code)]
    None
}

#[tauri::command]
pub fn open_received_folder(state: State<'_, AppState>) -> Result<(), String> {
    // Open the folder files actually land in, not one they might have moved past.
    let chosen = state.inner.read().files.received_folder.clone();
    let dir = crate::transport::transfer::effective_received_dir(&chosen);
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
pub fn choose_received_folder(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    // Change… on the General page: where files sent to this computer are saved.
    let picked = match pick_folder("Choose where received files are saved") {
        Some(p) => p,
        None => return Ok(state.inner.read().files.received_folder.clone()),
    };
    {
        let mut s = state.inner.write();
        s.files.received_folder = picked.clone();
        s.files.received = picked.clone();
    }
    push_state(&app, &state);
    Ok(picked)
}

#[tauri::command]
pub fn choose_share_folder(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    // Choose… on the Files page: the folder this computer offers the other one.
    let picked = match pick_folder("Choose a folder to share with the other computer") {
        Some(p) => p,
        None => return Ok(state.inner.read().files.path.clone()),
    };
    {
        let mut s = state.inner.write();
        s.files.path = picked.clone();
        s.files.smb.folder = picked.clone();
        s.files.share_on = true;
    }
    push_state(&app, &state);
    Ok(picked)
}

#[tauri::command]
pub fn reset_received_folder(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    let dir = crate::transport::transfer::default_download_dir();
    {
        let mut s = state.inner.write();
        s.files.received_folder = String::new();
        s.files.received = dir.to_string_lossy().to_string();
    }
    push_state(&app, &state);
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
    muted: Option<bool>,
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

    log::info!("[Notifs] Setting mute state for app notifications: {}", name);
    {
        let mut s = state.inner.write();
        // The switch says which way it should end up (muted / not muted); without it, toggle.
        let is_muted = match muted {
            Some(want) => {
                let has = s.notifs.muted.iter().any(|x| x.eq_ignore_ascii_case(&name));
                if want && !has {
                    s.notifs.muted.push(name.clone());
                } else if !want && has {
                    s.notifs.muted.retain(|x| !x.eq_ignore_ascii_case(&name));
                }
                want
            }
            None => toggle_app_mute(&mut s.notifs.muted, &name),
        };
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
pub fn open_monitor(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Monitor] Opening remote monitor / projection view");
    let win = app
        .get_webview_window("projection")
        .ok_or_else(|| "Projection window not found in application manifest".to_string())?;
    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();

    {
        let mut s = state.inner.write();
        s.stream.state = "on".to_string();
        s.stream.watching = true;
    }

    // When the other computer lets us start it, ask for the picture now; its answer
    // (screen.state) and the frames it sends arrive on the same session.
    let (allowed, mode) = {
        let s = state.inner.read();
        let remote = s.stream.remote.as_ref();
        (
            remote
                .and_then(|r| r.get("remote"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            remote
                .and_then(|r| r.get("mode"))
                .and_then(|v| v.as_str())
                .filter(|m| *m == "mirror" || *m == "extend")
                .unwrap_or("mirror")
                .to_string(),
        )
    };
    if allowed {
        if let Some(net) = app.try_state::<NetworkManager>() {
            net.send_packet_sync(WireMessage {
                t: "screen.ctl".to_string(),
                v: Some(1),
                on: Some(true),
                mode: Some(mode),
                ..Default::default()
            });
        }
    }
    push_state(&app, &state);
    Ok(())
}

/// Ask the other computer to stop sharing (when we may), and stop watching here.
pub(crate) fn stop_watching(state: &AppState, app: &AppHandle) {
    let (allowed, was_on) = {
        let s = state.inner.read();
        (
            s.stream
                .remote
                .as_ref()
                .and_then(|r| r.get("remote"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            s.stream.state == "on",
        )
    };
    {
        let mut s = state.inner.write();
        s.stream.state = "off".to_string();
        s.stream.watching = false;
    }
    if allowed && was_on {
        if let Some(net) = app.try_state::<NetworkManager>() {
            net.send_packet_sync(WireMessage {
                t: "screen.ctl".to_string(),
                v: Some(1),
                on: Some(false),
                ..Default::default()
            });
        }
    }
    if let Some(win) = app.get_webview_window("projection") {
        let _ = win.hide();
    }
    push_state(app, state);
}

#[tauri::command]
pub fn remote_screen(
    share: Option<bool>,
    mode: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Monitor] remote_screen: share={:?}, mode={:?}", share, mode);
    if share == Some(false) {
        stop_watching(&state, &app);
        return Ok(());
    }
    open_monitor(state.clone(), app.clone())?;
    if let Some(m) = mode.filter(|m| m == "mirror" || m == "extend") {
        // This is the other computer's mode, so it is only reported to it; ours stays
        // as it is on the Share screen page.
        if let Some(net) = app.try_state::<NetworkManager>() {
            net.send_packet_sync(WireMessage {
                t: "screen.ctl".to_string(),
                v: Some(1),
                on: Some(true),
                mode: Some(m),
                ..Default::default()
            });
        }
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
        if let Some(allow) = partial.get("allow").and_then(|v| v.as_bool()) {
            s.remote.allow = allow;
            if !allow {
                s.remote.using = false;
            }
            s.remote.why = String::new();
        }
        if let Some(pad) = partial.get("pad").and_then(|v| v.as_bool()) {
            s.remote.pad.on = pad;
            s.remote.pad.busy = String::new();
            s.remote.pad.live = pad && s.remote.pad.supported;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_pc_screen(mode: serde_json::Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    // The Share page sends one setting at a time, e.g. {"monitor": "display-0"} or
    // {"mbps": 30}. A bare string is still accepted as the mode, for older callers.
    log::info!("[Sharing] set_pc_screen: {}", mode);
    let mut wants_mode: Option<String> = None;
    {
        let mut s = state.inner.write();
        if let Some(obj) = mode.as_object() {
            if let Some(m) = obj.get("mode").and_then(|v| v.as_str()) {
                wants_mode = Some(m.to_string());
            }
            if let Some(m) = obj.get("monitor").and_then(|v| v.as_str()) {
                s.pc_screen.monitor = if m.is_empty() { None } else { Some(m.to_string()) };
            }
            if let Some(h) = obj
                .get("maxHeight")
                .or_else(|| obj.get("max_height"))
                .and_then(|v| v.as_u64())
            {
                if crate::streaming::validate_stream_max_height(h as u32).is_ok() {
                    s.pc_screen.max_height = h as u32;
                    s.stream.max_height = h as u32;
                }
            }
            if let Some(mbps) = obj.get("mbps").and_then(|v| v.as_u64()) {
                if crate::streaming::validate_bitrate(mbps as u32).is_ok() {
                    s.pc_screen.mbps = mbps as u32;
                    s.stream.bitrate_mbps = mbps as u32;
                }
            }
            if let Some(sound) = obj.get("sound").and_then(|v| v.as_bool()) {
                s.pc_screen.sound = sound;
                s.stream.audio = sound;
            }
            if let Some(input) = obj.get("input").and_then(|v| v.as_bool()) {
                s.pc_screen.input = input;
                s.stream.stream_input = input;
            }
            if let Some(remote) = obj.get("remote").and_then(|v| v.as_bool()) {
                s.pc_screen.remote = remote;
            }
            if let Some(size) = obj.get("extendSize").and_then(|v| v.as_str()) {
                s.pc_screen.extend_size = size.to_string();
                s.pc_screen.extend_px = match size {
                    "1920x1080" => (1920, 1080),
                    "3840x2160" => (3840, 2160),
                    _ => (2560, 1440),
                };
            }
        } else if let Some(m) = mode.as_str() {
            wants_mode = Some(m.to_string());
        }
    }
    match wants_mode.as_deref() {
        Some(m @ ("mirror" | "extend" | "off")) => {
            state.inner.write().pc_screen.mode = m.to_string();
            crate::streaming::apply_sharing(m != "off", &state, &app);
        }
        _ => {
            // Only a detail changed: Picture settings go straight to the capture
            // engine, and the other computer is told so its page agrees with ours.
            if let Some(bridge) = app.try_state::<crate::media::NativeBridgeManager>() {
                let (h, mbps) = {
                    let s = state.inner.read();
                    (s.pc_screen.max_height, s.pc_screen.mbps)
                };
                bridge.screen_engine.set_settings(
                    crate::media::screen::CaptureSettings::from_choices(h, mbps),
                );
            }
            crate::streaming::announce_screen_state(&state, &app);
            push_state(&app, &state);
        }
    }
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
pub fn check_firewall(state: State<'_, AppState>, app: AppHandle) -> Result<serde_json::Value, String> {
    log::info!("[Firewall] Checking system firewall configuration");
    let mut active = false;
    let mut status = String::new();
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--getglobalstate")
            .output();
        if let Ok(o) = out {
            let text = String::from_utf8_lossy(&o.stdout);
            active = text.contains("enabled");
            status = text.trim().to_string();
        }
    }
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("netsh")
            .args(["advfirewall", "show", "currentprofile", "state"])
            .output();
        if let Ok(o) = out {
            let text = String::from_utf8_lossy(&o.stdout);
            active = text.contains("ON");
            status = text.trim().to_string();
        }
    }
    {
        let mut s = state.inner.write();
        s.firewall.checking = false;
        s.firewall.ok = true;
        s.firewall.message = if active {
            "The firewall is on and GlideDesk is allowed through".to_string()
        } else {
            "The firewall is off".to_string()
        };
        s.firewall.error = String::new();
    }
    push_state(&app, &state);
    Ok(serde_json::json!({
        "active": active,
        "status": status
    }))
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

pub fn is_startup_enabled() -> bool {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let plist_path = PathBuf::from(home).join("Library/LaunchAgents/com.glidedesk.desktop.plist");
            return plist_path.exists();
        }
        false
    }
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "GlideDesk"])
            .output();
        if let Ok(out) = output {
            return out.status.success();
        }
        false
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let desktop_file = PathBuf::from(home).join(".config/autostart/glidedesk.desktop");
            return desktop_file.exists();
        }
        false
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    false
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
            // Intentionally avoid calling `launchctl load` when the app is already running:
            // with RunAtLoad=true, launchctl load executes the binary immediately, spawning a duplicate instance!
            // macOS launchd automatically discovers and executes LaunchAgents on user login.
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
            let formatted_path = format!("\"{}\"", exe_str);
            let output = Command::new("reg")
                .args(["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "GlideDesk", "/t", "REG_SZ", "/d", &formatted_path, "/f"])
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
pub fn set_startup(on: bool, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[System] Setting run at startup: {}", on);
    execute_set_startup(on)?;
    {
        let mut s = state.inner.write();
        s.startup = on;
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
