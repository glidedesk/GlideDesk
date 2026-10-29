// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust Proof-of-Concept: IPC commands matching window.macshare contract.

use crate::network::NetworkManager;
use crate::state::{AppState, FullState};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::ShellExt;

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> FullState {
    state.snapshot()
}

#[tauri::command]
pub fn set_kvm(partial: Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        if let Some(locked) = partial.get("locked").and_then(|v| v.as_bool()) {
            s.input.locked = locked;
        }
        if let Some(mouse) = partial.get("mouse").and_then(|v| v.as_bool()) {
            s.input.mouse = mouse;
        }
        if let Some(keyboard) = partial.get("keyboard").and_then(|v| v.as_bool()) {
            s.input.keyboard = keyboard;
        }
        if let Some(gestures) = partial.get("gestures").and_then(|v| v.as_bool()) {
            s.input.gestures = gestures;
        }
        if let Some(speed) = partial.get("speed").and_then(|v| v.as_f64()) {
            s.input.speed = speed as f32;
        }
        if let Some(rev) = partial.get("reverseScroll").and_then(|v| v.as_bool()) {
            s.input.reverse_scroll = rev;
        }
        if let Some(lock_screen) = partial.get("lockScreen").and_then(|v| v.as_bool()) {
            s.input.lock_screen = lock_screen;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn get_displays(app: AppHandle) -> Result<Vec<crate::state::MonitorInfo>, String> {
    let mut monitors = Vec::new();
    if let Ok(available) = app.available_monitors() {
        let primary = app.primary_monitor().ok().flatten();
        let primary_name = primary.as_ref().and_then(|p| p.name().map(|n| n.to_string()));

        for (idx, m) in available.iter().enumerate() {
            let name = m.name().map(|n| n.to_string()).unwrap_or_else(|| format!("Display {}", idx + 1));
            let pos = m.position();
            let size = m.size();
            let is_primary = primary_name.as_deref().map_or(idx == 0, |pn| pn == name);

            monitors.push(crate::state::MonitorInfo {
                id: format!("display-{}", idx),
                name,
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
                scale_factor: m.scale_factor(),
                is_primary,
            });
        }
    }
    Ok(monitors)
}

#[tauri::command]
pub fn set_layout(side: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.layout.side = side;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_keys(partial: Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        if let Some(hotkey) = partial.get("hotkey").and_then(|v| v.as_str()) {
            s.keys.hotkey = hotkey.to_string();
        }
        if let Some(edge) = partial.get("switchOnEdge").and_then(|v| v.as_bool()) {
            s.keys.switch_on_edge = edge;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_tab(tab: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.active_tab = tab;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_clipboard(partial: Value, network: State<'_, NetworkManager>) -> Result<(), String> {
    if let Some(text) = partial.get("text").and_then(|v| v.as_str()) {
        let mut cb = crate::kvm::NativeClipboard::new();
        cb.set_text(text);

        let msg = crate::network::WireMessage {
            t: "clip".to_string(),
            v: Some(1),
            text: Some(text.to_string()),
            ts: Some(chrono::Utc::now().timestamp_millis() as u64),
            ..Default::default()
        };
        let net = network.inner().clone();
        tauri::async_runtime::spawn(async move {
            net.send_packet(msg).await;
        });
    }
    Ok(())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    let mut cb = crate::kvm::NativeClipboard::new();
    cb.set_text(&text);
    Ok(())
}

#[tauri::command]
pub fn set_updates(partial: Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        if let Some(auto) = partial.get("auto").and_then(|v| v.as_bool()) {
            s.updates.auto = auto;
        }
        if let Some(skipped) = partial.get("skipped").and_then(|v| v.as_str()) {
            s.updates.skipped_version = skipped.to_string();
            s.updates.skipped = !skipped.is_empty();
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn skip_update(version: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let clean = version.trim().trim_start_matches('v').to_string();
    {
        let mut s = state.inner.write();
        s.updates.skipped_version = clean;
        s.updates.skipped = true;
        s.updates.prompt_update = false;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn unskip_update(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.updates.skipped_version.clear();
        s.updates.skipped = false;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn check_updates(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.updates.checking = true;
        s.updates.error.clear();
    }
    push_state(&app, &state);

    tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

    {
        let mut s = state.inner.write();
        s.updates.checking = false;
        s.updates.checked_at = chrono::Utc::now().timestamp_millis() as u64;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
#[allow(deprecated)]
pub fn open_update(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let url = state.inner.read().updates.release_url.clone();
    app.shell().open(url, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_update(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("[Updater] Direct in-app update initiated (no browser required)");
    {
        let mut s = state.inner.write();
        s.updates.checking = true;
    }
    push_state(&app, &state);

    tauri::async_runtime::spawn(async move {
        #[cfg(target_os = "macos")]
        {
            let script = r#"
                REPO="glidedesk/glidedesk"
                ARCH="$(uname -m)"
                if [ "${ARCH}" = "arm64" ]; then
                    ASSET_MATCH="GlideDesk_.*_aarch64\.dmg"
                else
                    ASSET_MATCH="GlideDesk_.*_x64\.dmg"
                fi
                RELEASE_JSON="$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest")"
                DOWNLOAD_URL="$(echo "${RELEASE_JSON}" | grep -o 'https://[^"]*' | grep -E "${ASSET_MATCH}" | head -n 1 || true)"
                if [ -z "${DOWNLOAD_URL}" ]; then
                    DOWNLOAD_URL="$(echo "${RELEASE_JSON}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*\.dmg" | head -n 1 || true)"
                fi
                if [ -n "${DOWNLOAD_URL}" ]; then
                    TMP_DIR="$(mktemp -d)"
                    DMG_PATH="${TMP_DIR}/update.dmg"
                    MOUNT_POINT="${TMP_DIR}/mount"
                    curl -fSL "${DOWNLOAD_URL}" -o "${DMG_PATH}"
                    mkdir -p "${MOUNT_POINT}"
                    hdiutil attach "${DMG_PATH}" -mountpoint "${MOUNT_POINT}" -nobrowse -quiet
                    if [ -d "${MOUNT_POINT}/GlideDesk.app" ]; then
                        rm -rf "/Applications/GlideDesk.app"
                        cp -R "${MOUNT_POINT}/GlideDesk.app" "/Applications/GlideDesk.app"
                        xattr -cr "/Applications/GlideDesk.app"
                        hdiutil detach "${MOUNT_POINT}" -quiet || true
                        rm -rf "${TMP_DIR}"
                        open -a "/Applications/GlideDesk.app"
                        pkill -ix "glidedesk" || true
                    fi
                fi
            "#;
            let _ = std::process::Command::new("bash").arg("-c").arg(script).spawn();
        }

        #[cfg(target_os = "windows")]
        {
            let script = r#"
                $repo = "glidedesk/glidedesk"
                $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
                $asset = $release.assets | Where-Object { $_.name -like "*x64-setup.exe" } | Select-Object -First 1
                if ($asset) {
                    $tmpPath = "$env:TEMP\GlideDesk-update.exe"
                    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmpPath
                    Start-Process -FilePath $tmpPath -ArgumentList "/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS"
                }
            "#;
            let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", script]).spawn();
        }

        #[cfg(target_os = "linux")]
        {
            let script = r#"
                REPO="glidedesk/glidedesk"
                RELEASE_JSON="$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest")"
                DOWNLOAD_URL="$(echo "${RELEASE_JSON}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*\.AppImage" | head -n 1 || true)"
                if [ -n "${DOWNLOAD_URL}" ]; then
                    curl -fSL "${DOWNLOAD_URL}" -o "/tmp/GlideDesk.AppImage"
                    chmod +x "/tmp/GlideDesk.AppImage"
                    /tmp/GlideDesk.AppImage &
                    pkill -ix "glidedesk" || true
                fi
            "#;
            let _ = std::process::Command::new("bash").arg("-c").arg(script).spawn();
        }
    });

    Ok(())
}

#[tauri::command]
#[allow(deprecated)]
pub fn open_link(key: String, app: AppHandle) -> Result<(), String> {
    let url = match key.as_str() {
        "portfolio" => "https://github.com/glidedesk",
        "github" => "https://github.com/glidedesk/glidedesk",
        "releases" => "https://github.com/glidedesk/glidedesk/releases",
        "licence" => "https://github.com/glidedesk/glidedesk/blob/main/LICENSE",
        _ => "https://github.com/glidedesk/glidedesk",
    };
    app.shell().open(url, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pair(host: String, network: State<'_, NetworkManager>) -> Result<(), String> {
    network.connect_to(&host).await
}

#[tauri::command]
pub async fn connect_address(addr: String, network: State<'_, NetworkManager>) -> Result<(), String> {
    network.connect_to(&addr).await
}

#[tauri::command]
pub async fn connect_again(network: State<'_, NetworkManager>, state: State<'_, AppState>) -> Result<(), String> {
    let target = {
        let s = state.inner.read();
        s.paired.first().and_then(|p| p.address.clone())
    };
    if let Some(addr) = target {
        network.connect_to(&addr).await?;
    }
    Ok(())
}

#[tauri::command]
pub fn refresh_found(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.found.clear();
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn disconnect(
    network: State<'_, NetworkManager>,
    registry: State<'_, crate::window_manager::ParkedWindowRegistry>,
) -> Result<(), String> {
    network.disconnect().await;
    let restored = registry.restore_all();
    if !restored.is_empty() {
        log::info!("[WindowHandoff] User disconnect: restored {} parked window(s)", restored.len());
    }
    Ok(())
}

#[tauri::command]
pub fn pair_respond(allowed: bool, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Pair response: allowed = {}", allowed);
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_app(_partial: Value) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn set_monitor(_partial: Value) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn copy_history(_index: usize) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn clear_history() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn set_multi(_on: bool) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn skip_setup() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn confirm_code() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn cancel_pair() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn forget(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.paired.retain(|p| p.id != id);
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn wake_peer(_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn send_files(files: Vec<String>, target: Option<String>, network: State<'_, NetworkManager>) -> Result<(), String> {
    let msg = crate::network::WireMessage {
        t: "files.batch".to_string(),
        v: Some(1),
        files: Some(files),
        target,
        ..Default::default()
    };
    network.send_packet(msg).await;
    Ok(())
}

#[tauri::command]
pub async fn send_wire_message(
    pkt: crate::network::WireMessage,
    network: State<'_, NetworkManager>,
) -> Result<(), String> {
    network.send_packet(pkt).await;
    Ok(())
}

#[tauri::command]
pub fn choose_send_files() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn handoff_window_at_point(
    x: f64,
    y: f64,
    registry: State<'_, crate::window_manager::ParkedWindowRegistry>,
    network: State<'_, NetworkManager>,
) -> Result<bool, String> {
    if let Some((id, title, wx, wy, w, h)) = crate::window_manager::find_window_at_point(x, y) {
        registry.park(id.clone(), title.clone(), wx, wy, w, h, None);
        let msg = crate::network::WireMessage {
            t: "window.handoff.offer".to_string(),
            v: Some(1),
            id: Some(id),
            name: Some(title),
            w: Some(w),
            height: Some(h),
            ..Default::default()
        };
        network.send_packet(msg).await;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn handoff_window(
    id: String,
    title: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    registry: State<'_, crate::window_manager::ParkedWindowRegistry>,
    network: State<'_, NetworkManager>,
) -> Result<(), String> {
    registry.park(id.clone(), title.clone(), x, y, width, height, None);

    let msg = crate::network::WireMessage {
        t: "window.handoff.offer".to_string(),
        v: Some(1),
        id: Some(id),
        name: Some(title),
        w: Some(width),
        height: Some(height),
        ..Default::default()
    };
    network.send_packet(msg).await;
    Ok(())
}

#[tauri::command]
pub async fn return_window(
    id: String,
    registry: State<'_, crate::window_manager::ParkedWindowRegistry>,
    network: State<'_, NetworkManager>,
) -> Result<(), String> {
    registry.unpark(&id);

    let msg = crate::network::WireMessage {
        t: "window.handoff.return".to_string(),
        v: Some(1),
        id: Some(id),
        ..Default::default()
    };
    network.send_packet(msg).await;
    Ok(())
}

#[tauri::command]
pub fn get_parked_windows(
    registry: State<'_, crate::window_manager::ParkedWindowRegistry>,
) -> Result<Vec<crate::window_manager::WindowParkState>, String> {
    Ok(registry.list())
}

#[tauri::command]
pub async fn show_projection_window(
    app: AppHandle,
    id: String,
    title: String,
    width: u32,
    height: u32,
) -> Result<(), String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("projection") {
        let _ = win.set_title(&format!("GlideDesk — {}", title));
        let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: (width as f64).clamp(320.0, 3840.0),
            height: (height as f64).clamp(240.0, 2160.0),
        }));
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.emit("projection-setup", serde_json::json!({
            "id": id,
            "title": title,
            "width": width,
            "height": height,
        }));
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_projection_window(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("projection") {
        let _ = win.hide();
    }
    Ok(())
}

pub fn push_state(app: &AppHandle, state: &AppState) {
    let snapshot = state.snapshot();
    let _ = app.emit("state-change", &snapshot);
}
