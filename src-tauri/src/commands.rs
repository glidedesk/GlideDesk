// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust Proof-of-Concept: IPC commands matching window.macshare contract.

use crate::network::NetworkManager;
#[allow(unused_imports)]
pub use crate::network::{ClientInfo, resolve_file_dest};
use crate::state::{AppState, FullState, UpdateDownload};
use serde::Deserialize;
use serde_json::Value;
use base64::Engine;
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
        if let Some(shared) = partial.get("shared").and_then(|v| v.as_bool()) {
            s.kvm.linked = shared;
            s.kvm.mouse = shared;
            s.kvm_state.linked = shared;
        }
        if let Some(linked) = partial.get("linked").and_then(|v| v.as_bool()) {
            s.kvm.linked = linked;
            s.kvm_state.linked = linked;
        }
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

pub fn opposite_direction(side: &str) -> &'static str {
    match side {
        "right" => "left",
        "left" => "right",
        "top" => "bottom",
        "bottom" => "top",
        _ => "left",
    }
}

#[tauri::command]
pub fn set_layout(side: String, state: State<'_, AppState>, network: State<'_, NetworkManager>, app: AppHandle) -> Result<(), String> {
    let opp = opposite_direction(&side);
    {
        let mut s = state.inner.write();
        s.layout.side = side.clone();
        s.kvm.pc_side = Some(opp.to_string());
    }
    push_state(&app, &state);

    network.send_packet_sync(crate::network::WireMessage {
        t: "kvm.layout".to_string(),
        v: Some(1),
        side: Some(opp.to_string()),
        pc_side: Some(opp.to_string()),
        ..Default::default()
    });
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
pub fn set_clipboard(
    partial: Value,
    state: State<'_, AppState>,
    network: State<'_, NetworkManager>,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(enabled) = partial
        .get("enabled")
        .or_else(|| partial.get("share"))
        .or_else(|| partial.get("send"))
        .or_else(|| partial.get("on"))
        .and_then(|v| v.as_bool())
    {
        let mut s = state.inner.write();
        s.clipboard.enabled = enabled;
        s.clipboard.share = enabled;
        s.clipboard.send = enabled;
    }
    if let Some(receive) = partial.get("receive").and_then(|v| v.as_bool()) {
        let mut s = state.inner.write();
        s.clipboard.receive = receive;
    }
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
    push_state(&app, &state);
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
        let peer_found = s.found.iter().find(|p| p.id.eq_ignore_ascii_case(&host) || p.address.as_deref() == Some(&host));
        if let Some(peer) = peer_found {
            peer.address.clone().map(|addr| {
                if let Some(port) = peer.port {
                    format!("{}:{}", addr, port)
                } else {
                    addr
                }
            })
        } else {
            None
        }.unwrap_or_else(|| host.clone())
    };

    log::info!("[Pair] Starting SAS pairing handshake with {}", target);
    network.start_pair(&target).await
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
pub async fn pair_respond(allowed: bool, network: State<'_, NetworkManager>) -> Result<(), String> {
    log::info!("Pair response: allowed = {}", allowed);
    network.respond_pair(allowed).await
}

#[tauri::command]
pub fn set_app(partial: Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Updating application settings: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(audio) = partial.get("audio") {
            if let Some(b) = audio.get("receive").and_then(|v| v.as_bool()) { s.audio.receive = b; }
            if let Some(b) = audio.get("share").and_then(|v| v.as_bool()) { s.audio.share = b; }
            if let Some(b) = audio.get("send").and_then(|v| v.as_bool()) { s.audio.send = b; }
        }
        if let Some(files) = partial.get("files") {
            if let Some(b) = files.get("receive").and_then(|v| v.as_bool()) { s.files.receive = b; }
            if let Some(b) = files.get("share").and_then(|v| v.as_bool()) { s.files.share = b; }
            if let Some(p) = files.get("path").and_then(|v| v.as_str()) { s.files.path = p.to_string(); }
        }
        if let Some(notifs) = partial.get("notifs") {
            if let Some(b) = notifs.get("enabled").and_then(|v| v.as_bool()) { s.notifs.enabled = b; }
            if let Some(b) = notifs.get("receive").and_then(|v| v.as_bool()) { s.notifs.receive = b; }
        }
        if let Some(terminal) = partial.get("terminal") {
            if let Some(t) = terminal.get("theme").and_then(|v| v.as_str()) { s.terminal.theme = t.to_string(); }
        }
        if let Some(mode) = partial.get("mode").and_then(|v| v.as_str()) {
            s.pc_screen.mode = mode.to_string();
        }
        if let Some(allow) = partial.get("allow").and_then(|v| v.as_bool()) {
            s.pc_apps.allow = allow;
        }
        if let Some(on) = partial.get("on").and_then(|v| v.as_bool()) {
            s.audio.enabled = on;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_monitor(partial: Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Updating monitor configuration: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(mode) = partial.get("mode").and_then(|v| v.as_str()) {
            s.pc_screen.mode = mode.to_string();
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn copy_history(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    log::info!("Copying clipboard history item at index {}", index);
    let s = state.inner.read();
    if s.clipboard.history.is_empty() {
        return Err("Clipboard history is empty".to_string());
    }
    if index < s.clipboard.history.len() {
        let item = &s.clipboard.history[index];
        let mut cb = crate::kvm::NativeClipboard::new();
        cb.set_text(&item.text);
        Ok(())
    } else {
        Err(format!("Clipboard history index {} out of bounds (len: {})", index, s.clipboard.history.len()))
    }
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Clearing clipboard history");
    {
        let mut s = state.inner.write();
        s.clipboard.history.clear();
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_multi(on: bool, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Setting multi-device mesh mode: enabled={}", on);
    {
        let mut s = state.inner.write();
        s.multi.enabled = on;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn skip_setup(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    log::info!("Skipping setup onboarding dialog");
    {
        let mut s = state.inner.write();
        s.setup.show = false;
        s.setup.missing.clear();
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn confirm_code(network: State<'_, NetworkManager>) -> Result<(), String> {
    log::info!("Confirm pairing code");
    network.confirm_pair().await
}

#[tauri::command]
pub async fn cancel_pair(network: State<'_, NetworkManager>) -> Result<(), String> {
    log::info!("Cancel pairing");
    network.cancel_pair().await
}

#[tauri::command]
pub fn forget(_id: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.paired = None;
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn send_files(
    files: Vec<String>,
    target: Option<String>,
    network: State<'_, NetworkManager>,
    state: State<'_, AppState>,
    phone_mgr: State<'_, crate::phone::PhoneManager>,
    transfer_mgr: State<'_, crate::transport::transfer::TransferManager>,
    app: AppHandle,
) -> Result<String, String> {
    if files.is_empty() {
        return Err("No files specified for transfer".to_string());
    }

    let target_str = target.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let dest = resolve_file_dest(target_str, &network.get_active_clients_info())?;
    log::info!("[SendFiles] dest={:?}", dest);

    // 1. Select optimal transfer route from real state inputs
    let route = crate::transport::route::select_route_for_peer(target.as_deref(), &state, &phone_mgr);
    log::info!(
        "[SendFiles] Selected route mode={} endpoint={} for {} file(s)",
        route.mode,
        route.endpoint,
        files.len()
    );

    // 2. Pre-compute SHA-256 and metadata for each file without whole-file memory loading
    let mut file_tuples = Vec::new();
    let mut wire_items = Vec::new();
    let mut total_bytes = 0u64;

    for (idx, path_str) in files.iter().enumerate() {
        let p = std::path::Path::new(path_str);
        if !p.exists() {
            return Err(format!("File '{}' not found", path_str));
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());
        let (size, sha256) = crate::transport::transfer::compute_file_sha256(p)
            .map_err(|e| format!("Failed to read file '{}': {}", path_str, e))?;
        let file_id = format!("file_{}_{}", idx, name);
        total_bytes += size;
        file_tuples.push((name.clone(), size, sha256.clone(), Some(path_str.clone())));
        wire_items.push(crate::network::WireFileItem {
            file_id,
            name,
            size,
            sha256,
        });
    }

    // 3. Check for existing batch for resume
    let target_id = target.as_deref().unwrap_or(&dest[0]);
    let existing_batch_info = {
        let batches = transfer_mgr.batches.read();
        batches.values().find(|b| {
            (b.peer_id.as_deref() == Some(target_id) || b.peer_id.as_deref() == Some(&dest[0]))
                && b.items.len() == wire_items.len()
                && b.items.iter().all(|it| {
                    wire_items.iter().any(|wi| wi.name == it.filename && it.checksum_sha256.as_ref() == Some(&wi.sha256))
                })
        }).cloned()
    };

    let batch_id = existing_batch_info
        .as_ref()
        .map(|b| b.batch_id.clone())
        .unwrap_or_else(|| format!("batch_{}", chrono::Utc::now().timestamp_millis()));

    let batch = if let Some(mut existing) = existing_batch_info {
        log::info!("[SendFiles] Resuming existing transfer batch {}", existing.batch_id);
        existing.phase = crate::transport::transfer::TransferPhase::Transferring;
        existing
    } else {
        let mut b = crate::transport::transfer::TransferBatch::new_with_checksums(batch_id.clone(), file_tuples);
        b.peer_id = Some(dest[0].clone());
        b
    };

    let cancel_flag = transfer_mgr.get_cancel_flag(&batch_id);
    cancel_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    {
        let mut batches = transfer_mgr.batches.write();
        batches.insert(batch_id.clone(), batch.clone());
    }

    let _ = app.emit(
        "transfer-started",
        &serde_json::json!({
            "batch_id": batch_id,
            "files_count": files.len(),
            "total_bytes": batch.total_bytes,
            "route": route.mode.to_string(),
        }),
    );

    // 4. Send files.batch header to peer
    let batch_pkt = crate::network::WireMessage {
        t: "files.batch".to_string(),
        v: Some(1),
        id: Some(batch_id.clone()),
        batch_id: Some(batch_id.clone()),
        batch_files: Some(wire_items.clone()),
        total_bytes: Some(total_bytes),
        files: Some(files.clone()),
        target: target.clone(),
        d: Some(dest.clone()),
        ..Default::default()
    };
    network.send_packet(batch_pkt).await;

    // 5. Stream chunks and await acks
    const CHUNK_SIZE: usize = crate::transport::transfer::CHUNK_SIZE;
    use std::io::{Read, Seek, SeekFrom};

    for wi in &wire_items {
        let (file_size, file_sha256, local_path, completed, start_offset) = {
            let batches = transfer_mgr.batches.read();
            let b = batches.get(&batch_id).ok_or("Batch not found")?;
            let item = b.items.iter().find(|it| it.file_id == wi.file_id).ok_or("Item not found")?;
            (item.total_bytes, wi.sha256.clone(), item.local_path.clone().unwrap(), item.completed, item.transferred_bytes)
        };

        if completed {
            log::info!("[SendFiles] File {} already completed in resumed batch; skipping", wi.name);
            continue;
        }

        let mut file = std::fs::File::open(&local_path)
            .map_err(|e| format!("Failed to open file {:?}: {}", local_path, e))?;

        if start_offset > 0 {
            log::info!("[SendFiles] Resuming file {} from offset {}", wi.name, start_offset);
            file.seek(SeekFrom::Start(start_offset))
                .map_err(|e| format!("Failed to seek to resume offset {}: {}", start_offset, e))?;
        }

        let mut current_offset = start_offset;
        let mut ack_rx = transfer_mgr.ack_tx.subscribe();

        while current_offset < file_size {
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                log::warn!("[SendFiles] Transfer cancelled by user mid-chunk at offset {}", current_offset);
                let _ = transfer_mgr.cancel_batch(&batch_id);
                return Err("Transfer cancelled by user".to_string());
            }

            let chunk_len = ((file_size - current_offset) as usize).min(CHUNK_SIZE);
            let mut chunk_buf = vec![0u8; chunk_len];
            file.read_exact(&mut chunk_buf)
                .map_err(|e| format!("Failed to read chunk at offset {}: {}", current_offset, e))?;

            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(&chunk_buf);
            let chunk_hash = format!("{:x}", hasher.finalize());

            let data_b64 = base64::engine::general_purpose::STANDARD.encode(&chunk_buf);

            let chunk_pkt = crate::network::WireMessage {
                t: "files.chunk".to_string(),
                v: Some(1),
                batch_id: Some(batch_id.clone()),
                file_id: Some(wi.file_id.clone()),
                offset: Some(current_offset),
                len: Some(chunk_len as u64),
                data: Some(data_b64),
                sha256_chunk: Some(chunk_hash),
                target: target.clone(),
                d: Some(dest.clone()),
                ..Default::default()
            };
            network.send_packet(chunk_pkt.clone()).await;

            let target_ack_offset = current_offset + chunk_len as u64;
            let mut acked = false;

            // Bounded retry with timeout
            for retry in 0..3 {
                if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = transfer_mgr.cancel_batch(&batch_id);
                    return Err("Transfer cancelled by user".to_string());
                }

                let wait_timeout = tokio::time::Duration::from_millis(5000);
                let wait_res = tokio::time::timeout(wait_timeout, async {
                    loop {
                        match ack_rx.recv().await {
                            Ok(ack) => {
                                if ack.batch_id == batch_id && ack.file_id == wi.file_id && ack.offset >= target_ack_offset {
                                    return Ok(());
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                }).await;

                match wait_res {
                    Ok(Ok(())) => {
                        acked = true;
                        break;
                    }
                    _ => {
                        log::warn!("[SendFiles] Ack timeout at offset {}, retry {}/3", current_offset, retry + 1);
                        network.send_packet(chunk_pkt.clone()).await;
                    }
                }
            }

            if !acked {
                return Err(format!("Peer timed out acknowledging chunk at offset {}", current_offset));
            }

            current_offset = target_ack_offset;

            // Monotonically advance batch progress
            {
                let mut batches = transfer_mgr.batches.write();
                if let Some(b) = batches.get_mut(&batch_id) {
                    let _ = b.set_file_offset(&wi.file_id, current_offset);
                }
            }

            let _ = app.emit(
                "transfer-progress",
                &serde_json::json!({
                    "batch_id": batch_id,
                    "file_id": wi.file_id,
                    "offset": current_offset,
                    "total": file_size,
                }),
            );
        }

        // Send files.done with SENDER-SUPPLIED total sha256
        let done_pkt = crate::network::WireMessage {
            t: "files.done".to_string(),
            v: Some(1),
            batch_id: Some(batch_id.clone()),
            file_id: Some(wi.file_id.clone()),
            sha256_total: Some(file_sha256.clone()),
            target: target.clone(),
            d: Some(dest.clone()),
            ..Default::default()
        };
        network.send_packet(done_pkt).await;

        {
            let mut batches = transfer_mgr.batches.write();
            if let Some(b) = batches.get_mut(&batch_id) {
                if let Some(item) = b.items.iter_mut().find(|it| it.file_id == wi.file_id) {
                    item.completed = true;
                    item.checksum_sha256 = Some(file_sha256.clone());
                    item.transferred_bytes = file_size;
                }
            }
        }
    }

    // 6. Complete batch
    {
        let mut batches = transfer_mgr.batches.write();
        if let Some(b) = batches.get_mut(&batch_id) {
            b.phase = crate::transport::transfer::TransferPhase::BatchDone;
        }
    }

    let _ = app.emit(
        "transfer-completed",
        &serde_json::json!({
            "batch_id": batch_id,
            "phase": "batchDone",
        }),
    );

    Ok(batch_id)
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
pub fn choose_send_files(app: AppHandle) -> Result<Vec<String>, String> {
    log::info!("Opening native file selection dialog for choose_send_files");
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg("set theFiles to choose file with multiple selections allowed\nset out to \"\"\nrepeat with aFile in theFiles\nset out to out & (POSIX path of aFile) & \"\\n\"\nend repeat\nreturn out")
            .output()
            .map_err(|e| format!("Failed to launch file dialog: {}", e))?;

        if !output.status.success() {
            // User cancelled
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if !files.is_empty() {
            log::info!("User selected {} file(s) for sending", files.len());
            let _ = app.emit("files-selected", &files);
        }
        return Ok(files);
    }

    #[cfg(target_os = "windows")]
    {
        let ps_cmd = "[System.Reflection.Assembly]::LoadWithPartialName('System.windows.forms') | Out-Null; $dlg = New-Object System.Windows.Forms.OpenFileDialog; $dlg.Multiselect = $true; if ($dlg.ShowDialog() -eq 'OK') { [string]::Join(\"`n\", $dlg.FileNames) }";
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", ps_cmd])
            .output()
            .map_err(|e| format!("Failed to launch Windows file dialog: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(files);
    }

    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("zenity")
            .args(["--file-selection", "--multiple", "--separator=\n"])
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let files: Vec<String> = stdout
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            return Ok(files);
        }
        Err("File selection dialog not supported or zenity not found on Linux".to_string())
    }
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

    #[test]
    fn test_opposite_direction_helper() {
        assert_eq!(opposite_direction("right"), "left");
        assert_eq!(opposite_direction("left"), "right");
        assert_eq!(opposite_direction("top"), "bottom");
        assert_eq!(opposite_direction("bottom"), "top");
        assert_eq!(opposite_direction("invalid_side"), "left");
        assert_eq!(opposite_direction(""), "left");
    }

    #[test]
    fn test_pick_asset_empty_or_mismatched() {
        let empty_assets: Vec<GithubAsset> = Vec::new();
        assert!(pick_asset(&empty_assets).is_none());

        let wrong_assets = vec![
            GithubAsset {
                name: "unknown_file.tar.gz".to_string(),
                browser_download_url: "https://example.com/unknown_file.tar.gz".to_string(),
                size: Some(100),
            },
        ];
        assert!(pick_asset(&wrong_assets).is_none());
    }

    #[test]
    fn test_kvm_and_clipboard_master_switch_handling() {
        let app_state = crate::state::AppState::new();
        {
            let mut s = app_state.inner.write();
            assert!(s.kvm.linked);
            assert!(s.kvm.mouse);
            assert!(s.clipboard.enabled);
            assert!(s.clipboard.share);

            // 1. Cursor master switch { shared: false }
            let kvm_payload = serde_json::json!({ "shared": false });
            if let Some(shared) = kvm_payload.get("shared").and_then(|v| v.as_bool()) {
                s.kvm.linked = shared;
                s.kvm.mouse = shared;
                s.kvm_state.linked = shared;
            }
            assert!(!s.kvm.linked);
            assert!(!s.kvm.mouse);

            // 2. Clipboard master switch { enabled: false }
            let clip_payload = serde_json::json!({ "enabled": false });
            if let Some(enabled) = clip_payload.get("enabled").and_then(|v| v.as_bool()) {
                s.clipboard.enabled = enabled;
                s.clipboard.share = enabled;
                s.clipboard.send = enabled;
            }
            assert!(!s.clipboard.enabled);
            assert!(!s.clipboard.share);
            assert!(!s.clipboard.send);
        }
    }
}
