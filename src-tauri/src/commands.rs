// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust Proof-of-Concept: IPC commands matching window.macshare contract.

use crate::network::NetworkManager;
use crate::state::{AppState, FullState, UpdateDownload};
use serde::Deserialize;
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
            s.kvm.locked = locked;
            s.kvm_state.locked = locked;
        }
        if let Some(mouse) = partial.get("mouse").and_then(|v| v.as_bool()) {
            s.kvm.mouse = mouse;
        }
        if let Some(keyboard) = partial.get("keyboard").and_then(|v| v.as_bool()) {
            s.kvm.keyboard = keyboard;
        }
        if let Some(gestures) = partial.get("gestures").and_then(|v| v.as_bool()) {
            s.kvm.gestures = gestures;
        }
        if let Some(speed) = partial.get("speed").and_then(|v| v.as_f64()) {
            s.kvm.speed = speed as f32;
        }
        if let Some(rev) = partial.get("reverseScroll").and_then(|v| v.as_bool()) {
            s.kvm.reverse_scroll = rev;
        }
        if let Some(lock_screen) = partial.get("lockScreen").and_then(|v| v.as_bool()) {
            s.kvm.lock_screen = lock_screen;
        }
        if let Some(be_driven) = partial.get("beDriven").and_then(|v| v.as_bool()) {
            s.kvm.be_driven = be_driven;
        }
        if let Some(drive_other) = partial.get("driveOther").and_then(|v| v.as_bool()) {
            s.kvm.drive_other = drive_other;
        }
        if let Some(hotkey_switch) = partial.get("hotkeySwitch").and_then(|v| v.as_bool()) {
            s.kvm.hotkey_switch = hotkey_switch;
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

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[allow(dead_code)]
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    #[allow(dead_code)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

fn pick_asset(assets: &[GithubAsset]) -> Option<UpdateDownload> {
    #[cfg(target_os = "macos")]
    {
        let arch = std::env::consts::ARCH;
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".dmg") && a.name.to_lowercase().contains("universal")) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
        let arch_match = if arch == "aarch64" { "aarch64" } else { "x64" };
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".dmg") && a.name.to_lowercase().contains(arch_match)) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".dmg")) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(a) = assets.iter().find(|a| a.name.ends_with("-setup.exe") || a.name.ends_with(".exe")) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".msi")) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".AppImage")) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".deb")) {
            return Some(UpdateDownload {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size.unwrap_or(0),
            });
        }
    }

    #[allow(unreachable_code)]
    None
}

fn parse_version(v: &str) -> Option<(Vec<u64>, String)> {
    let clean = v.trim().trim_start_matches('v');
    let mut parts = clean.splitn(2, '-');
    let num_part = parts.next()?;
    let pre = parts.next().unwrap_or("").to_string();
    let nums: Vec<u64> = num_part.split('.').filter_map(|s| s.parse().ok()).collect();
    if nums.is_empty() {
        return None;
    }
    Some((nums, pre))
}

fn is_version_newer(candidate: &str, current: &str) -> bool {
    let (cand_nums, cand_pre) = match parse_version(candidate) {
        Some(v) => v,
        None => return false,
    };
    let (curr_nums, curr_pre) = match parse_version(current) {
        Some(v) => v,
        None => return false,
    };
    let max_len = cand_nums.len().max(curr_nums.len());
    for i in 0..max_len {
        let c = *cand_nums.get(i).unwrap_or(&0);
        let cur = *curr_nums.get(i).unwrap_or(&0);
        if c > cur {
            return true;
        } else if c < cur {
            return false;
        }
    }
    if cand_pre == curr_pre {
        return false;
    }
    if cand_pre.is_empty() && !curr_pre.is_empty() {
        return true;
    }
    if !cand_pre.is_empty() && curr_pre.is_empty() {
        return false;
    }
    cand_pre > curr_pre
}

#[tauri::command]
pub async fn check_updates(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.updates.checking = true;
        s.updates.error.clear();
    }
    push_state(&app, &state);

    let client = reqwest::Client::builder()
        .user_agent("GlideDesk")
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            let mut s = state.inner.write();
            s.updates.checking = false;
            s.updates.error = format!("HTTP client error: {}", e);
            push_state(&app, &state);
            return Err(e.to_string());
        }
    };

    let url = "https://api.github.com/repos/glidedesk/GlideDesk/releases?per_page=5";
    let resp = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await;

    let res = match resp {
        Ok(r) => r,
        Err(e) => {
            let mut s = state.inner.write();
            s.updates.checking = false;
            s.updates.error = format!("Network error: {}", e);
            s.updates.checked_at = chrono::Utc::now().timestamp_millis() as u64;
            push_state(&app, &state);
            return Err(e.to_string());
        }
    };

    if !res.status().is_success() {
        let err_msg = format!("GitHub answered {}", res.status());
        let mut s = state.inner.write();
        s.updates.checking = false;
        s.updates.error = err_msg.clone();
        s.updates.checked_at = chrono::Utc::now().timestamp_millis() as u64;
        push_state(&app, &state);
        return Err(err_msg);
    }

    let releases: Vec<GithubRelease> = match res.json().await {
        Ok(r) => r,
        Err(e) => {
            let mut s = state.inner.write();
            s.updates.checking = false;
            s.updates.error = format!("JSON parse error: {}", e);
            s.updates.checked_at = chrono::Utc::now().timestamp_millis() as u64;
            push_state(&app, &state);
            return Err(e.to_string());
        }
    };

    let candidate = releases.into_iter().find(|r| !r.draft);

    {
        let mut s = state.inner.write();
        s.updates.checking = false;
        s.updates.checked_at = chrono::Utc::now().timestamp_millis() as u64;

        if let Some(rel) = candidate {
            let latest_tag = rel.tag_name.clone();
            let clean_latest = latest_tag.trim_start_matches('v');
            let is_newer = is_version_newer(clean_latest, &s.updates.current);

            s.updates.latest = Some(latest_tag);
            s.updates.release_url = rel.html_url.clone();
            s.updates.release_notes = rel.body.unwrap_or_default();
            s.updates.published_at = rel.published_at.unwrap_or_default();
            s.updates.download = pick_asset(&rel.assets);
            s.updates.available = is_newer;
            s.updates.no_releases = false;
        } else {
            s.updates.no_releases = true;
            s.updates.available = false;
        }
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
    let download_url = {
        let mut s = state.inner.write();
        s.updates.checking = true;
        s.updates.download.as_ref().map(|d| d.url.clone())
    };
    push_state(&app, &state);

    tauri::async_runtime::spawn(async move {
        #[cfg(target_os = "macos")]
        {
            let explicit_url = download_url.unwrap_or_default();
            let script = format!(r#"
                REPO="glidedesk/GlideDesk"
                ARCH="$(uname -m)"
                DOWNLOAD_URL="{}"
                if [ -z "${{DOWNLOAD_URL}}" ]; then
                    RELEASES_JSON="$(curl -sSL -H "User-Agent: GlideDesk" "https://api.github.com/repos/${{REPO}}/releases?per_page=5" 2>/dev/null || true)"
                    DOWNLOAD_URL="$(echo "${{RELEASES_JSON}}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*_universal\.dmg" | head -n 1 || true)"
                    if [ -z "${{DOWNLOAD_URL}}" ]; then
                        if [ "${{ARCH}}" = "arm64" ]; then
                            DOWNLOAD_URL="$(echo "${{RELEASES_JSON}}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*_(arm64|aarch64)\.dmg" | head -n 1 || true)"
                        else
                            DOWNLOAD_URL="$(echo "${{RELEASES_JSON}}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*_x64\.dmg" | head -n 1 || true)"
                        fi
                    fi
                    if [ -z "${{DOWNLOAD_URL}}" ]; then
                        DOWNLOAD_URL="$(echo "${{RELEASES_JSON}}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*\.dmg" | head -n 1 || true)"
                    fi
                fi
                if [ -n "${{DOWNLOAD_URL}}" ]; then
                    TMP_DIR="$(mktemp -d)"
                    DMG_PATH="${{TMP_DIR}}/update.dmg"
                    MOUNT_POINT="${{TMP_DIR}}/mount"
                    curl -fSL "${{DOWNLOAD_URL}}" -o "${{DMG_PATH}}"
                    mkdir -p "${{MOUNT_POINT}}"
                    hdiutil attach "${{DMG_PATH}}" -mountpoint "${{MOUNT_POINT}}" -nobrowse -quiet
                    if [ -d "${{MOUNT_POINT}}/GlideDesk.app" ]; then
                        rm -rf "/Applications/GlideDesk.app"
                        cp -R "${{MOUNT_POINT}}/GlideDesk.app" "/Applications/GlideDesk.app"
                        xattr -cr "/Applications/GlideDesk.app"
                        codesign --force --deep --sign - --identifier "com.glidedesk.desktop" "/Applications/GlideDesk.app" 2>/dev/null || true
                        hdiutil detach "${{MOUNT_POINT}}" -quiet || true
                        rm -rf "${{TMP_DIR}}"
                        open -a "/Applications/GlideDesk.app"
                        pkill -ix "glidedesk" || true
                    fi
                fi
            "#, explicit_url);
            let _ = std::process::Command::new("bash").arg("-c").arg(script).spawn();
        }

        #[cfg(target_os = "windows")]
        {
            let explicit_url = download_url.unwrap_or_default();
            let script = format!(r#"
                $repo = "glidedesk/GlideDesk"
                $downloadUrl = "{}"
                if (-not $downloadUrl) {{
                    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases?per_page=5" -Headers @{{ 'User-Agent' = 'GlideDesk' }}
                    $release = if ($releases -is [array]) {{ $releases[0] }} else {{ $releases }}
                    $asset = $release.assets | Where-Object {{ $_.name -like "*x64-setup.exe" -or $_.name -like "*.exe" }} | Select-Object -First 1
                    if ($asset) {{ $downloadUrl = $asset.browser_download_url }}
                }}
                if ($downloadUrl) {{
                    $tmpPath = "$env:TEMP\GlideDesk-update.exe"
                    Invoke-WebRequest -Uri $downloadUrl -OutFile $tmpPath
                    Start-Process -FilePath $tmpPath -ArgumentList "/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS"
                }}
            "#, explicit_url);
            let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).spawn();
        }

        #[cfg(target_os = "linux")]
        {
            let explicit_url = download_url.unwrap_or_default();
            let script = format!(r#"
                REPO="glidedesk/GlideDesk"
                DOWNLOAD_URL="{}"
                if [ -z "${{DOWNLOAD_URL}}" ]; then
                    RELEASES_JSON="$(curl -sSL -H "User-Agent: GlideDesk" "https://api.github.com/repos/${{REPO}}/releases?per_page=5" 2>/dev/null || true)"
                    DOWNLOAD_URL="$(echo "${{RELEASES_JSON}}" | grep -o 'https://[^"]*' | grep -E "GlideDesk_.*\.AppImage" | head -n 1 || true)"
                fi
                if [ -n "${{DOWNLOAD_URL}}" ]; then
                    curl -fSL "${{DOWNLOAD_URL}}" -o "/tmp/GlideDesk.AppImage"
                    chmod +x "/tmp/GlideDesk.AppImage"
                    /tmp/GlideDesk.AppImage &
                    pkill -ix "glidedesk" || true
                fi
            "#, explicit_url);
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
        "github" => "https://github.com/glidedesk/GlideDesk",
        "releases" => "https://github.com/glidedesk/GlideDesk/releases",
        "licence" => "https://github.com/glidedesk/GlideDesk/blob/main/LICENSE",
        _ => "https://github.com/glidedesk/GlideDesk",
    };
    app.shell().open(url, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pair(host: String, network: State<'_, NetworkManager>, state: State<'_, AppState>) -> Result<(), String> {
    let target = {
        let s = state.inner.read();
        if let Some(peer) = s.found.iter().find(|p| p.id.eq_ignore_ascii_case(&host) || p.address.as_deref() == Some(&host)) {
            peer.address.clone().map(|addr| {
                if let Some(port) = peer.port {
                    format!("{}:{}", addr, port)
                } else {
                    addr
                }
            })
        } else {
            None
        }
    }.unwrap_or(host);
    network.connect_to(&target).await
}

#[tauri::command]
pub async fn connect_address(addr: String, network: State<'_, NetworkManager>) -> Result<(), String> {
    network.connect_to(&addr).await
}

#[tauri::command]
pub async fn connect_again(network: State<'_, NetworkManager>, state: State<'_, AppState>) -> Result<(), String> {
    let target = {
        let s = state.inner.read();
        s.paired.as_ref().and_then(|p| p.address.clone())
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
        s.paired = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparisons() {
        assert!(is_version_newer("1.1.2-beta.1", "1.1.1"));
        assert!(!is_version_newer("1.1.2-beta.1", "1.1.2-beta.1"));
        assert!(is_version_newer("1.1.2", "1.1.2-beta.1"));
        assert!(is_version_newer("1.1.2-beta.2", "1.1.2-beta.1"));
        assert!(!is_version_newer("1.1.2-beta.1", "1.1.2"));
        assert!(is_version_newer("1.1.3", "1.1.2"));
        assert!(!is_version_newer("1.1.2", "1.1.3"));
        assert!(is_version_newer("2.0.0", "1.9.9"));
    }

    #[test]
    fn test_pick_asset_resolution() {
        let assets = vec![
            GithubAsset {
                name: "GlideDesk_1.1.2_universal.dmg".to_string(),
                browser_download_url: "https://example.com/GlideDesk_1.1.2_universal.dmg".to_string(),
                size: Some(15000000),
            },
            GithubAsset {
                name: "GlideDesk_1.1.2_x64-setup.exe".to_string(),
                browser_download_url: "https://example.com/GlideDesk_1.1.2_x64-setup.exe".to_string(),
                size: Some(5000000),
            },
            GithubAsset {
                name: "GlideDesk_1.1.2_amd64.AppImage".to_string(),
                browser_download_url: "https://example.com/GlideDesk_1.1.2_amd64.AppImage".to_string(),
                size: Some(80000000),
            },
        ];

        let picked = pick_asset(&assets);
        assert!(picked.is_some());
        let asset = picked.unwrap();
        #[cfg(target_os = "macos")]
        assert_eq!(asset.name, "GlideDesk_1.1.2_universal.dmg");
        #[cfg(target_os = "windows")]
        assert_eq!(asset.name, "GlideDesk_1.1.2_x64-setup.exe");
        #[cfg(target_os = "linux")]
        assert_eq!(asset.name, "GlideDesk_1.1.2_amd64.AppImage");
    }
}
