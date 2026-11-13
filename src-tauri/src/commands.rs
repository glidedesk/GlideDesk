// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust Proof-of-Concept: IPC commands matching window.macshare contract.

use crate::network::NetworkManager;
#[allow(unused_imports)]
pub use crate::network::{ClientInfo, resolve_file_dest};
use crate::state::{AppState, FullState, UpdateDownload};
use std::path::{Path, PathBuf};
use serde::Deserialize;
use serde_json::Value;
use base64::Engine;
use tauri::{AppHandle, Emitter, Manager, State};
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
        if let Some(ble) = partial.get("ble").and_then(|v| v.as_bool()) {
            s.kvm.ble = ble;
        }
        if let Some(px) = partial.get("resistancePx").and_then(|v| v.as_u64()) {
            s.kvm.resistance_px = px.min(u32::MAX as u64) as u32;
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
pub fn set_layout(side: String, along: Option<f64>, state: State<'_, AppState>, network: State<'_, NetworkManager>, app: AppHandle) -> Result<(), String> {
    let opp = opposite_direction(&side);
    let along_val = along.unwrap_or(0.0);
    {
        let mut s = state.inner.write();
        s.layout.side = side.clone();
        s.layout.along = along_val;
        s.kvm.pc_side = Some(opp.to_string());
    }
    crate::pairstore::save_layout(&side, along_val);
    push_state(&app, &state);

    // The other computer flips this to work out where we sit on its screen, so send the
    // side we chose here unchanged. The desktop size travels too, so it can scale
    // pointer movement to its own screen.
    let (sw, sh) = crate::kvm::virtual_desktop_size();
    network.send_packet_sync(crate::network::WireMessage {
        t: "kvm.layout".to_string(),
        v: Some(1),
        side: Some(side.clone()),
        pc_side: Some(side),
        sw: Some(sw),
        sh: Some(sh),
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
        if let Some(swap) = partial.get("swapCtrlCmd").and_then(|v| v.as_bool()) {
            s.keys.swap_ctrl_cmd = swap;
        }
        if let Some(keys) = partial.get("lockKeys").and_then(|v| v.as_str()) {
            s.keys.lock_keys = keys.to_string();
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub fn set_net(partial: Value, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        if let Some(b) = partial.get("ble").and_then(|v| v.as_bool()) {
            s.net.ble = b;
        }
        if let Some(b) = partial.get("blePc").and_then(|v| v.as_bool()) {
            s.net.ble_pc = b;
        }
        if let Some(b) = partial.get("bleWifiOff").and_then(|v| v.as_bool()) {
            s.net.ble_wifi_off = b;
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
    log::info!("[Clipboard] set: {:?}", partial);
    // The master switch (and its legacy name "share"). A direction switch (`send`/`receive`)
    // must never turn the whole tab off: `send` is only one of the two ways clipboard crosses.
    if let Some(enabled) = partial
        .get("enabled")
        .or_else(|| partial.get("share"))
        .or_else(|| partial.get("on"))
        .and_then(|v| v.as_bool())
    {
        let mut s = state.inner.write();
        s.clipboard.enabled = enabled;
        s.clipboard.share = enabled;
    }
    if let Some(send) = partial.get("send").and_then(|v| v.as_bool()) {
        let mut s = state.inner.write();
        s.clipboard.send = send;
    }
    if let Some(receive) = partial.get("receive").and_then(|v| v.as_bool()) {
        let mut s = state.inner.write();
        s.clipboard.receive = receive;
    }
    if let Some(mode) = partial.get("mode").and_then(|v| v.as_str()) {
        let mut s = state.inner.write();
        s.clipboard.mode = mode.to_string();
    }
    if let Some(b) = partial.get("skipConcealed").and_then(|v| v.as_bool()) {
        let mut s = state.inner.write();
        s.clipboard.skip_concealed = b;
    }
    if let Some(n) = partial.get("maxKB").and_then(|v| v.as_u64()) {
        let mut s = state.inner.write();
        s.clipboard.max_kb = n.min(u32::MAX as u64) as u32;
    }
    if let Some(n) = partial.get("historySize").and_then(|v| v.as_u64()) {
        let mut s = state.inner.write();
        s.clipboard.history_size = n.min(u32::MAX as u64) as u32;
    }
    if let Some(b) = partial.get("images").and_then(|v| v.as_bool()) {
        let mut s = state.inner.write();
        s.clipboard.images = b;
    }
    if let Some(text) = partial.get("text").and_then(|v| v.as_str()) {
        crate::kvm::set_shared_clipboard_text(text);

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
    crate::kvm::set_shared_clipboard_text(&text);
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
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

fn pick_first<F>(assets: &[GithubAsset], pred: F) -> Option<UpdateDownload>
where
    F: Fn(&GithubAsset) -> bool,
{
    assets.iter().find(|a| pred(a)).map(|a| UpdateDownload {
        name: a.name.clone(),
        url: a.browser_download_url.clone(),
        size: a.size.unwrap_or(0),
        sha256: None,
    })
}

pub fn verify_download(
    expected_size: u64,
    actual_size: u64,
    expected_sha256: Option<&str>,
    actual_sha256: Option<&str>,
) -> Result<(), String> {
    if actual_size == 0 {
        return Err("Downloaded update file is empty (0 bytes)".to_string());
    }
    if expected_size > 0 && actual_size != expected_size {
        return Err(format!(
            "Download size mismatch: expected {} bytes, got {} bytes",
            expected_size, actual_size
        ));
    }
    if let (Some(expected), Some(actual)) = (expected_sha256, actual_sha256) {
        let expected = expected.trim();
        let actual = actual.trim();
        if !expected.is_empty() && !expected.eq_ignore_ascii_case(actual) {
            return Err(format!(
                "SHA-256 checksum mismatch: expected {}, got {}",
                expected, actual
            ));
        }
    }
    Ok(())
}

fn find_published_sha256(asset_name: &str, body: Option<&str>) -> Option<String> {
    if let Some(text) = body {
        for line in text.lines() {
            let line = line.trim();
            if line.contains(asset_name) {
                for token in line.split_whitespace() {
                    let clean = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
                    if clean.len() == 64 && clean.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Some(clean.to_lowercase());
                    }
                }
            }
        }
    }
    None
}

pub async fn download_update_asset(
    download: &UpdateDownload,
    app: &AppHandle,
    state: &AppState,
) -> Result<(std::path::PathBuf, String), String> {
    let client = reqwest::Client::builder()
        .user_agent("GlideDesk")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut resp = client
        .get(&download.url)
        .header("Accept", "application/octet-stream")
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Download server returned HTTP {}", resp.status()));
    }

    let total_size = resp.content_length().unwrap_or(download.size);

    let temp_dir = std::env::temp_dir().join("glidedesk-updates");
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("Failed to create temporary directory {:?}: {}", temp_dir, e))?;
    let dest_path = temp_dir.join(&download.name);

    let mut file = tokio::fs::File::create(&dest_path)
        .await
        .map_err(|e| format!("Failed to create file at {:?}: {}", dest_path, e))?;

    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_progress_emit = std::time::Instant::now();

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Error while downloading stream: {}", e))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed writing to {:?}: {}", dest_path, e))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;

        if last_progress_emit.elapsed().as_millis() >= 100 || (total_size > 0 && downloaded >= total_size) {
            last_progress_emit = std::time::Instant::now();
            let pct = if total_size > 0 {
                ((downloaded as f64 / total_size as f64) * 100.0).min(100.0) as f32
            } else {
                0.0
            };
            let dl_mb = downloaded as f64 / (1024.0 * 1024.0);
            let tot_mb = total_size as f64 / (1024.0 * 1024.0);
            let status_text = if total_size > 0 {
                format!("Downloading… {:.0}% ({:.1} MB / {:.1} MB)", pct, dl_mb, tot_mb)
            } else {
                format!("Downloading… {:.1} MB", dl_mb)
            };

            {
                let mut s = state.inner.write();
                s.updates.status = "downloading".to_string();
                s.updates.progress = pct;
                s.updates.downloaded_bytes = downloaded;
                s.updates.total_bytes = total_size;
                s.updates.status_text = status_text;
            }
            push_state(app, state);
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed flushing {:?}: {}", dest_path, e))?;
    drop(file);

    let actual_sha256 = format!("{:x}", hasher.finalize());

    {
        let mut s = state.inner.write();
        s.updates.status = "verifying".to_string();
        s.updates.progress = 100.0;
        s.updates.downloaded_bytes = downloaded;
        s.updates.total_bytes = downloaded;
        s.updates.status_text = "Verifying download…".to_string();
    }
    push_state(app, state);

    if let Err(err) = verify_download(
        download.size,
        downloaded,
        download.sha256.as_deref(),
        Some(&actual_sha256),
    ) {
        let _ = tokio::fs::remove_file(&dest_path).await;
        return Err(err);
    }

    Ok((dest_path, actual_sha256))
}

/// The best download for a platform ("macos" | "windows" | "linux"), or None.
/// Kept platform-parameterized so every branch is compiled and unit-tested on
/// every host, not just the one it runs on.
fn pick_asset_for(platform: &str, assets: &[GithubAsset]) -> Option<UpdateDownload> {
    match platform {
        "macos" => {
            let arch = std::env::consts::ARCH;
            let arch_match = if arch == "aarch64" { "aarch64" } else { "x64" };
            pick_first(assets, |a| {
                a.name.ends_with(".dmg") && a.name.to_lowercase().contains("universal")
            })
            .or_else(|| {
                pick_first(assets, |a| {
                    a.name.ends_with(".dmg") && a.name.to_lowercase().contains(arch_match)
                })
            })
            .or_else(|| pick_first(assets, |a| a.name.ends_with(".dmg")))
        }
        // Prefer the NSIS setup or MSI installer. Never choose the zero-install
        // "portable" exe: it ignores /SILENT and would not actually update.
        "windows" => pick_first(assets, |a| a.name.ends_with("-setup.exe"))
            .or_else(|| pick_first(assets, |a| a.name.ends_with(".msi")))
            .or_else(|| {
                pick_first(assets, |a| {
                    a.name.ends_with(".exe") && !a.name.to_lowercase().contains("portable")
                })
            }),
        "linux" => pick_first(assets, |a| a.name.ends_with(".AppImage"))
            .or_else(|| pick_first(assets, |a| a.name.ends_with(".deb"))),
        _ => None,
    }
}

fn pick_asset(assets: &[GithubAsset]) -> Option<UpdateDownload> {
    pick_asset_for(std::env::consts::OS, assets)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub platform: String,
    pub installer_path: PathBuf,
    pub target_path: PathBuf,
    pub program: String,
    pub args: Vec<String>,
    pub script: String,
}

pub fn install_plan_for(
    platform: &str,
    file_path: &Path,
    target_dir: Option<&Path>,
) -> Result<InstallPlan, String> {
    let file_str = file_path.to_string_lossy();
    let file_name = file_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or("");

    match platform {
        "macos" | "darwin" => {
            if ext != "dmg" {
                return Err(format!("Expected a .dmg installer file on macOS, got: {}", file_name));
            }
            let target = target_dir
                .map(|d| d.join("GlideDesk.app"))
                .unwrap_or_else(|| PathBuf::from("/Applications/GlideDesk.app"));
            let target_str = target.to_string_lossy();

            let script = format!(
                r#"TARGET_APP="{target}"
TARGET_DIR="$(dirname "${{TARGET_APP}}")"
if [ ! -w "${{TARGET_DIR}}" ]; then
    echo "Cannot write to ${{TARGET_DIR}} — please move GlideDesk to /Applications or check folder permissions" >&2
    exit 1
fi
DMG_PATH="{file}"
TMP_DIR="$(mktemp -d)"
MOUNT_POINT="${{TMP_DIR}}/mount"
mkdir -p "${{MOUNT_POINT}}"
if ! hdiutil attach "${{DMG_PATH}}" -mountpoint "${{MOUNT_POINT}}" -nobrowse -quiet; then
    echo "Failed to mount DMG: ${{DMG_PATH}}" >&2
    rm -rf "${{TMP_DIR}}"
    exit 1
fi
APP_SRC=$(find "${{MOUNT_POINT}}" -maxdepth 2 -name "GlideDesk.app" -print -quit)
if [ -z "${{APP_SRC}}" ] || [ ! -d "${{APP_SRC}}" ]; then
    echo "GlideDesk.app not found in DMG" >&2
    hdiutil detach "${{MOUNT_POINT}}" -quiet || true
    rm -rf "${{TMP_DIR}}"
    exit 1
fi
rm -rf "${{TARGET_APP}}"
cp -R "${{APP_SRC}}" "${{TARGET_APP}}"
xattr -cr "${{TARGET_APP}}" || true
codesign --force --deep --sign - --identifier "com.glidedesk.desktop" "${{TARGET_APP}}" 2>/dev/null || true
hdiutil detach "${{MOUNT_POINT}}" -quiet || true
rm -rf "${{TMP_DIR}}"
rm -f "${{DMG_PATH}}"
pkill -ix "glidedesk" || true
sleep 1
open -a "${{TARGET_APP}}"
"#,
                target = target_str,
                file = file_str
            );

            Ok(InstallPlan {
                platform: "macos".to_string(),
                installer_path: file_path.to_path_buf(),
                target_path: target,
                program: "bash".to_string(),
                args: vec!["-c".to_string(), script.clone()],
                script,
            })
        }
        "windows" => {
            if file_name.to_lowercase().contains("portable") {
                return Err(format!(
                    "Portable executable cannot be installed automatically; please use the installer setup package: {}",
                    file_name
                ));
            }
            if ext != "exe" && ext != "msi" {
                return Err(format!("Expected .exe or .msi installer on Windows, got: {}", file_name));
            }
            let target = target_dir
                .map(|d| d.join("GlideDesk"))
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\GlideDesk"));

            let script = format!(
                r#"$installerPath = "{file}"
$proc = Start-Process -FilePath $installerPath -ArgumentList "/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS" -Wait -PassThru
if ($proc.ExitCode -ne 0) {{
    Write-Error "Installer failed with exit code $($proc.ExitCode)"
    exit $proc.ExitCode
}}
"#,
                file = file_str
            );

            Ok(InstallPlan {
                platform: "windows".to_string(),
                installer_path: file_path.to_path_buf(),
                target_path: target,
                program: "powershell".to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    script.clone(),
                ],
                script,
            })
        }
        "linux" => {
            if ext != "AppImage" && ext != "deb" {
                return Err(format!("Expected .AppImage or .deb on Linux, got: {}", file_name));
            }
            let target = target_dir
                .map(|d| d.join("GlideDesk.AppImage"))
                .unwrap_or_else(|| PathBuf::from("~/.local/bin/GlideDesk.AppImage"));
            let target_str = target.to_string_lossy();

            let script = format!(
                r#"SRC_PATH="{file}"
TARGET_FILE="{target}"
TARGET_DIR="$(dirname "${{TARGET_FILE}}")"
mkdir -p "${{TARGET_DIR}}"
if [ ! -w "${{TARGET_DIR}}" ]; then
    echo "Cannot write to target directory: ${{TARGET_DIR}}" >&2
    exit 1
fi
cp -f "${{SRC_PATH}}" "${{TARGET_FILE}}"
chmod +x "${{TARGET_FILE}}"
rm -f "${{SRC_PATH}}"
"${{TARGET_FILE}}" &
sleep 0.5
pkill -ix "glidedesk" || pkill -f "GlideDesk.AppImage" || true
"#,
                target = target_str,
                file = file_str
            );

            Ok(InstallPlan {
                platform: "linux".to_string(),
                installer_path: file_path.to_path_buf(),
                target_path: target,
                program: "bash".to_string(),
                args: vec!["-c".to_string(), script.clone()],
                script,
            })
        }
        other => Err(format!("Unsupported platform for auto-update: {}", other)),
    }
}

pub fn execute_install_plan(plan: &InstallPlan) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let target_dir = plan.target_path.parent().unwrap_or_else(|| Path::new("/Applications"));
        let test_file = target_dir.join(format!(".glidedesk_perm_check_{}", std::process::id()));
        match std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&test_file) {
            Ok(_) => {
                let _ = std::fs::remove_file(test_file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err("Cannot write to /Applications — please move GlideDesk to /Applications or check folder permissions".to_string());
            }
            Err(_) => {}
        }

        let mut cmd = std::process::Command::new(&plan.program);
        cmd.args(&plan.args);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        cmd.spawn().map_err(|e| format!("Failed to spawn updater script: {}", e))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new(&plan.program);
        cmd.args(&plan.args);
        cmd.spawn().map_err(|e| format!("Failed to launch Windows installer: {}", e))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new(&plan.program);
        cmd.args(&plan.args);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        cmd.spawn().map_err(|e| format!("Failed to launch Linux updater: {}", e))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(format!("Auto-update execution not supported on {}", plan.platform))
    }
}

fn resolve_linux_target_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/bin")))
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

/// Choose the release to offer: skip drafts, and prefer a stable release over a
/// pre-release. The GitHub releases API returns newest first, so `min_by_key`
/// keeps the newest release for the preferred kind (stable before pre-release),
/// falling back to a pre-release only when no stable release exists.
fn pick_candidate(releases: Vec<GithubRelease>) -> Option<GithubRelease> {
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .min_by_key(|r| r.prerelease)
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

    let candidate = pick_candidate(releases);

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
            let body_text = rel.body.unwrap_or_default();
            s.updates.release_notes = body_text.clone();
            s.updates.published_at = rel.published_at.unwrap_or_default();
            let mut dl = pick_asset(&rel.assets);
            if let Some(ref mut d) = dl {
                d.sha256 = find_published_sha256(&d.name, Some(&body_text));
            }
            s.updates.download = dl;
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
    let download = {
        let mut s = state.inner.write();
        s.updates.checking = true;
        s.updates.download.clone()
    };
    push_state(&app, &state);

    let download = match download {
        Some(d) if !d.url.trim().is_empty() => d,
        _ => {
            let mut s = state.inner.write();
            s.updates.checking = false;
            s.updates.error = "No update asset available for download".to_string();
            push_state(&app, &state);
            return Err("No update asset available for download".to_string());
        }
    };
    let app_handle = app.clone();
    let state_inner = (*state).clone();

    tauri::async_runtime::spawn(async move {
        let (downloaded_path, _sha) = match download_update_asset(&download, &app_handle, &state_inner).await {
            Ok(res) => res,
            Err(e) => {
                log::error!("[Updater] Download failed: {}", e);
                let mut s = state_inner.inner.write();
                s.updates.checking = false;
                s.updates.status = "error".to_string();
                s.updates.error = e.clone();
                s.updates.status_text = format!("Update failed: {}", e);
                drop(s);
                push_state(&app_handle, &state_inner);
                return;
            }
        };

        let current_platform = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unknown"
        };

        let target_dir = if cfg!(target_os = "linux") {
            resolve_linux_target_dir()
        } else {
            None
        };

        let plan = match install_plan_for(current_platform, &downloaded_path, target_dir.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                log::error!("[Updater] Failed to generate install plan: {}", e);
                let mut s = state_inner.inner.write();
                s.updates.checking = false;
                s.updates.status = "error".to_string();
                s.updates.error = e.clone();
                s.updates.status_text = format!("Update failed: {}", e);
                drop(s);
                push_state(&app_handle, &state_inner);
                return;
            }
        };

        {
            let mut s = state_inner.inner.write();
            s.updates.status = "installing".to_string();
            s.updates.status_text = "Installing update…".to_string();
        }
        push_state(&app_handle, &state_inner);

        match execute_install_plan(&plan) {
            Ok(()) => {
                let mut s = state_inner.inner.write();
                s.updates.checking = false;
                s.updates.status = "restarting".to_string();
                s.updates.status_text = "Update installed — restarting".to_string();
                drop(s);
                push_state(&app_handle, &state_inner);
            }
            Err(e) => {
                log::error!("[Updater] Install execution failed: {}", e);
                let mut s = state_inner.inner.write();
                s.updates.checking = false;
                s.updates.status = "error".to_string();
                s.updates.error = e.clone();
                s.updates.status_text = format!("Update failed: {}", e);
                drop(s);
                push_state(&app_handle, &state_inner);
            }
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
pub async fn refresh_found(
    state: State<'_, AppState>,
    network: State<'_, NetworkManager>,
    app: AppHandle,
) -> Result<(), String> {
    {
        let mut s = state.inner.write();
        s.found.clear();
    }
    push_state(&app, &state);
    network.refresh_discovery().await;
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
pub fn set_app(partial: Value, state: State<'_, AppState>, network: State<'_, NetworkManager>, app: AppHandle) -> Result<(), String> {
    log::info!("Updating application settings: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(audio) = partial.get("audio") {
            if let Some(b) = audio.get("receive").and_then(|v| v.as_bool()) { s.audio.receive = b; }
            if let Some(b) = audio.get("share").and_then(|v| v.as_bool()) { s.audio.share = b; }
            if let Some(b) = audio.get("send").and_then(|v| v.as_bool()) { s.audio.send = b; }
            if let Some(b) = audio.get("enabled").and_then(|v| v.as_bool()) { s.audio.enabled = b; }
            if let Some(b) = audio.get("on").and_then(|v| v.as_bool()) { s.audio.enabled = b; }
            if let Some(b) = audio.get("muteLocal").and_then(|v| v.as_bool()) { s.audio.mute_local = b; }
            if let Some(v) = audio.get("volume").and_then(|v| v.as_f64()) { s.audio.volume = v.clamp(0.0, 1.0); }
            if let Some(b) = audio.get("phoneMedia").and_then(|v| v.as_bool()) { s.audio.phone_media = b; }
            if let Some(b) = audio.get("phoneBt").and_then(|v| v.as_bool()) { s.audio.phone_bt = b; }
        }
        if let Some(files) = partial.get("files") {
            if let Some(b) = files.get("receive").and_then(|v| v.as_bool()) { s.files.receive = b; }
            if let Some(b) = files.get("share").and_then(|v| v.as_bool()) { s.files.share = b; }
            if let Some(p) = files.get("path").and_then(|v| v.as_str()) { s.files.path = p.to_string(); }
            if let Some(b) = files.get("mountMac").and_then(|v| v.as_bool()) { s.files.mount_mac = b; }
            if let Some(b) = files.get("shareMac").and_then(|v| v.as_bool()) { s.files.share_mac = b; }
            if let Some(b) = files.get("shareOn").and_then(|v| v.as_bool()) { s.files.share_on = b; }
            if let Some(b) = files.get("acceptFromMac").and_then(|v| v.as_bool()) { s.files.accept_from_mac = b; }
            if let Some(b) = files.get("dropToMac").and_then(|v| v.as_bool()) { s.files.drop_to_mac = b; }
            if let Some(b) = files.get("shortcuts").and_then(|v| v.as_bool()) { s.files.shortcuts = b; }
            if let Some(b) = files.get("edgeDrag").and_then(|v| v.as_bool()) { s.files.edge_drag = b; }
            if let Some(k) = files.get("copyKeys").and_then(|v| v.as_str()) { s.files.copy_keys = k.to_string(); }
            if let Some(k) = files.get("pasteKeys").and_then(|v| v.as_str()) { s.files.paste_keys = k.to_string(); }
            // A send that could not start is reported here, so the Files page can say
            // why instead of only the transfer bar going quiet.
            if let Some(last) = files.get("last") {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                s.files.last = Some(crate::state::FilesEvent {
                    ok: last.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
                    name: last.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    text: last.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    at: last.get("at").and_then(|v| v.as_u64()).unwrap_or(now),
                });
            }
        }
        if let Some(notifs) = partial.get("notifs") {
            if let Some(b) = notifs.get("enabled").and_then(|v| v.as_bool()) { s.notifs.enabled = b; }
            if let Some(b) = notifs.get("receive").and_then(|v| v.as_bool()) { s.notifs.receive = b; }
            if let Some(b) = notifs.get("send").and_then(|v| v.as_bool()) { s.notifs.send = b; }
            if let Some(b) = notifs.get("share").and_then(|v| v.as_bool()) { s.notifs.share = b; }
            if let Some(b) = notifs.get("hideText").and_then(|v| v.as_bool()) { s.notifs.hide_text = b; }
        }
        if let Some(terminal) = partial.get("terminal") {
            if let Some(t) = terminal.get("theme").and_then(|v| v.as_str()) { s.terminal.theme = t.to_string(); }
            if let Some(b) = terminal.get("allowMac").and_then(|v| v.as_bool()) { s.terminal.allow_mac = b; }
            if let Some(b) = terminal.get("enabled").and_then(|v| v.as_bool()) { s.terminal.enabled = b; }
        }
        if let Some(mode) = partial.get("mode").and_then(|v| v.as_str()) {
            s.pc_screen.mode = mode.to_string();
        }
        if let Some(allow) = partial.get("allow").and_then(|v| v.as_bool()) {
            s.pc_apps.allow = allow;
        }
        if let Some(hud) = partial.get("hud").and_then(|v| v.as_bool()) {
            s.app.hud = hud;
        }
        if let Some(on) = partial.get("on").and_then(|v| v.as_bool()) {
            s.audio.enabled = on;
        }
    }
    push_state(&app, &state);
    // Notifications: the other computer sends only while this one accepts them.
    if partial.get("notifs").is_some() {
        crate::notifications::send_policy(network.inner(), &state);
    }
    // Sound: start or stop capture and playback here, and tell the other computer
    // whether this one plays its sound.
    if partial.get("audio").is_some() || partial.get("on").is_some() {
        crate::media::bridge::send_policy(network.inner(), &state);
        crate::media::bridge::apply_audio(&app, &state);
    }
    // The Files page's choices are the setup a person should only do once: keep them
    // for the next launch.
    if partial.get("files").is_some() {
        let files = state.inner.read().files.clone();
        crate::pairstore::save_files(&files);
    }
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
        if let Some(mute) = partial.get("mute").and_then(|v| v.as_bool()) {
            s.monitor.mute = mute;
        }
        if let Some(volume) = partial.get("volume").and_then(|v| v.as_f64()) {
            s.monitor.volume = volume.clamp(0.0, 1.0);
        }
        if let Some(framegen) = partial.get("framegen").and_then(|v| v.as_bool()) {
            s.monitor.framegen = framegen;
        }
        if let Some(pacing) = partial.get("pacing").and_then(|v| v.as_str()) {
            s.monitor.pacing = pacing.to_string();
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
        crate::kvm::set_shared_clipboard_text(&item.text);
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
    state.persist_pairs();
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
    send_files_to_peer(files, target, &network, &state, &phone_mgr, &transfer_mgr, &app).await
}

/// Send files over the paired link.
///
/// Shared by the Files page command and the Ctrl+Alt+V file paste, which runs from
/// the network loop and has no Tauri command context of its own.
pub async fn send_files_to_peer(
    files: Vec<String>,
    target: Option<String>,
    network: &NetworkManager,
    state: &AppState,
    phone_mgr: &crate::phone::PhoneManager,
    transfer_mgr: &crate::transport::transfer::TransferManager,
    app: &AppHandle,
) -> Result<String, String> {
    if files.is_empty() {
        return Err("No files specified for transfer".to_string());
    }

    let target_str = target.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let dest = resolve_file_dest(target_str, &network.get_active_clients_info())?;
    log::info!("[SendFiles] dest={:?}", dest);

    // 1. Select optimal transfer route from real state inputs.
    // Use the same trimmed target the destination resolver saw, otherwise a padded
    // target ("  peer_b  ") silently falls back to an unrelated peer's route.
    let route = crate::transport::route::select_route_for_peer(target_str, state, phone_mgr);
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
    let target_id = target_str.unwrap_or(dest[0].as_str());
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

    // Say on the Files page what was sent and where it went, so the bar that only
    // shows during the transfer is not the only sign it happened.
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let name = files
            .first()
            .map(|f| {
                std::path::Path::new(f)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| f.clone())
            })
            .unwrap_or_default();
        let shown = if files.len() > 1 {
            format!("{} and {} more", name, files.len() - 1)
        } else {
            name.clone()
        };
        {
            let mut s = state.inner.write();
            s.files.last = Some(crate::state::FilesEvent {
                ok: true,
                name: shown,
                text: format!("to {}", dest[0]),
                at: now,
            });
        }
        push_state(app, state);
    }

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

/// Show the system's "send these files" picker and return what was chosen.
///
/// This waits for a person, so it must not run on the thread that pumps this app's
/// windows: a main thread parked in that wait cannot route the click that dismisses
/// the very picker it just opened, and the window looks frozen.
fn pick_files_for_sending(app: &AppHandle) -> Result<Vec<String>, String> {
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    #[cfg(target_os = "macos")]
    {
        // Ask our own process for the panel. Driving `osascript` here would hand the
        // sheet to whatever app happens to be frontmost, which cannot answer it for
        // us: the dialog opens, the list works, and the button that closes it is dead.
        use tauri_plugin_dialog::DialogExt;
        // Present the panel as a sheet on our own window. A free-floating app panel
        // puts its browser in a separate system process, and that view stops taking
        // clicks as soon as the folder changes, leaving a dialog that cannot be used.
        let mut dialog = app.dialog().file();
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let picked = dialog.blocking_pick_files();
        return Ok(picked
            .unwrap_or_default()
            .into_iter()
            .filter_map(|f| f.into_path().ok())
            .map(|p| p.to_string_lossy().to_string())
            .collect());
    }

    #[cfg(target_os = "windows")]
    {
        let ps_cmd = "[System.Reflection.Assembly]::LoadWithPartialName('System.windows.forms') | Out-Null; $dlg = New-Object System.Windows.Forms.OpenFileDialog; $dlg.Multiselect = $true; if ($dlg.ShowDialog() -eq 'OK') { [string]::Join(\"`n\", $dlg.FileNames) }";
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", ps_cmd])
            .output()
            .map_err(|e| format!("Failed to launch Windows file dialog: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect());
    }

    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("zenity")
            .args(["--file-selection", "--multiple", "--separator=\n"])
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return Ok(stdout
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect());
        }
        Err("File selection dialog not supported or zenity not found on Linux".to_string())
    }
}

#[tauri::command]
pub async fn choose_send_files(app: AppHandle) -> Result<Vec<String>, String> {
    log::info!("Opening native file selection dialog for choose_send_files");
    let picker = app.clone();
    let files = tauri::async_runtime::spawn_blocking(move || pick_files_for_sending(&picker))
        .await
        .map_err(|e| format!("File dialog task failed: {}", e))??;
    if !files.is_empty() {
        log::info!("[ChooseFiles] {} file(s) chosen for sending", files.len());
        let _ = app.emit("files-selected", &files);
    }
    Ok(files)
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
pub async fn hide_projection_window(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    // Closing the screen window stops watching, and stops the other computer sharing
    // when it lets us ask, so nothing keeps encoding in the background.
    crate::parity::stop_watching(&state, &app);
    Ok(())
}

#[tauri::command]
pub fn set_projection_fullscreen(on: bool, app: AppHandle) -> Result<(), String> {
    // F11 in the screen window: fill the screen, or go back to a window.
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("projection") {
        win.set_fullscreen(on).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn push_state(app: &AppHandle, state: &AppState) {
    {
        // Keep the shown received folder equal to where files really go, so the
        // General page never promises a folder that is not used.
        let mut s = state.inner.write();
        let chosen = s.files.received_folder.clone();
        s.files.received = crate::transport::transfer::effective_received_dir(&chosen)
            .to_string_lossy()
            .to_string();
    }
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
                // The real release lists the portable exe first; on Windows it
                // must never be chosen over the -setup.exe installer.
                name: "GlideDesk-portable-x64.exe".to_string(),
                browser_download_url: "https://example.com/GlideDesk-portable-x64.exe".to_string(),
                size: Some(23000000),
            },
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

        // Every platform branch is exercised on every host.
        assert_eq!(
            pick_asset_for("macos", &assets).unwrap().name,
            "GlideDesk_1.1.2_universal.dmg"
        );
        assert_eq!(
            pick_asset_for("windows", &assets).unwrap().name,
            "GlideDesk_1.1.2_x64-setup.exe"
        );
        assert_eq!(
            pick_asset_for("linux", &assets).unwrap().name,
            "GlideDesk_1.1.2_amd64.AppImage"
        );
        // The current-platform wrapper still resolves to a file.
        assert!(pick_asset(&assets).is_some());

        // macOS fallback hierarchy: universal -> arch -> any dmg
        let arch_dmg_only = vec![
            GithubAsset {
                name: "GlideDesk_1.1.2_x64.dmg".to_string(),
                browser_download_url: "https://example.com/GlideDesk_1.1.2_x64.dmg".to_string(),
                size: Some(15000000),
            },
            GithubAsset {
                name: "GlideDesk_1.1.2_aarch64.dmg".to_string(),
                browser_download_url: "https://example.com/GlideDesk_1.1.2_aarch64.dmg".to_string(),
                size: Some(15000000),
            },
        ];
        assert!(pick_asset_for("macos", &arch_dmg_only).is_some());

        let any_dmg_only = vec![GithubAsset {
            name: "GlideDesk-legacy.dmg".to_string(),
            browser_download_url: "https://example.com/GlideDesk-legacy.dmg".to_string(),
            size: Some(15000000),
        }];
        assert_eq!(
            pick_asset_for("macos", &any_dmg_only).unwrap().name,
            "GlideDesk-legacy.dmg"
        );

        // Windows fallback hierarchy: -setup.exe -> .msi -> non-portable .exe -> reject portable
        let msi_only = vec![
            GithubAsset {
                name: "GlideDesk-portable-x64.exe".to_string(),
                browser_download_url: "https://example.com/portable.exe".to_string(),
                size: Some(20000000),
            },
            GithubAsset {
                name: "GlideDesk_1.1.2_x64.msi".to_string(),
                browser_download_url: "https://example.com/GlideDesk.msi".to_string(),
                size: Some(6000000),
            },
        ];
        assert_eq!(
            pick_asset_for("windows", &msi_only).unwrap().name,
            "GlideDesk_1.1.2_x64.msi"
        );

        let non_portable_exe_only = vec![
            GithubAsset {
                name: "GlideDesk-portable-x64.exe".to_string(),
                browser_download_url: "https://example.com/portable.exe".to_string(),
                size: Some(20000000),
            },
            GithubAsset {
                name: "GlideDesk-Installer-v1.1.2.exe".to_string(),
                browser_download_url: "https://example.com/installer.exe".to_string(),
                size: Some(7000000),
            },
        ];
        assert_eq!(
            pick_asset_for("windows", &non_portable_exe_only).unwrap().name,
            "GlideDesk-Installer-v1.1.2.exe"
        );

        let portable_only = vec![GithubAsset {
            name: "GlideDesk-portable-x64.exe".to_string(),
            browser_download_url: "https://example.com/portable.exe".to_string(),
            size: Some(20000000),
        }];
        assert!(pick_asset_for("windows", &portable_only).is_none());

        // Linux fallback hierarchy: .AppImage -> .deb
        let deb_only = vec![GithubAsset {
            name: "GlideDesk_1.1.2_amd64.deb".to_string(),
            browser_download_url: "https://example.com/GlideDesk.deb".to_string(),
            size: Some(10000000),
        }];
        assert_eq!(
            pick_asset_for("linux", &deb_only).unwrap().name,
            "GlideDesk_1.1.2_amd64.deb"
        );
    }

    #[test]
    fn test_verify_download() {
        let expected_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let actual_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

        // Successful match with size + hash
        assert!(verify_download(100, 100, Some(expected_hash), Some(actual_hash)).is_ok());

        // Successful match with size only (no expected hash)
        assert!(verify_download(100, 100, None, Some(actual_hash)).is_ok());

        // Successful match when expected size is 0 (dynamic/unknown content length)
        assert!(verify_download(0, 50, None, Some(actual_hash)).is_ok());

        // Error: 0 bytes downloaded
        let err_empty = verify_download(100, 0, None, None);
        assert!(err_empty.is_err());
        assert!(err_empty.unwrap_err().contains("empty"));

        // Error: size mismatch
        let err_size = verify_download(100, 99, None, None);
        assert!(err_size.is_err());
        assert!(err_size.unwrap_err().contains("size mismatch"));

        // Error: hash mismatch
        let wrong_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let err_hash = verify_download(100, 100, Some(expected_hash), Some(wrong_hash));
        assert!(err_hash.is_err());
        assert!(err_hash.unwrap_err().contains("checksum mismatch"));
    }

    #[test]
    fn test_find_published_sha256() {
        let body = "Release notes\nSHA256 checksums:\nGlideDesk_3.0.0_universal.dmg: 33f31ad383c76166cbc730129426750c3213efff06fd760c8ee6601cacf0eb26\n";
        let found = find_published_sha256("GlideDesk_3.0.0_universal.dmg", Some(body));
        assert_eq!(
            found.as_deref(),
            Some("33f31ad383c76166cbc730129426750c3213efff06fd760c8ee6601cacf0eb26")
        );

        assert!(find_published_sha256("other.dmg", Some(body)).is_none());
        assert!(find_published_sha256("GlideDesk_3.0.0_universal.dmg", None).is_none());
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
            }
            assert!(!s.clipboard.enabled);
            assert!(!s.clipboard.share);

            // 3. A direction switch is not the master: turning "Copy on this PC,
            //    paste on the other one" off must leave the tab on (regression: it
            //    used to fall through the master chain and hide the whole tab).
            s.clipboard.enabled = true;
            s.clipboard.share = true;
            let dir_payload = serde_json::json!({ "send": false });
            if let Some(enabled) = dir_payload
                .get("enabled")
                .or_else(|| dir_payload.get("share"))
                .or_else(|| dir_payload.get("on"))
                .and_then(|v| v.as_bool())
            {
                s.clipboard.enabled = enabled;
                s.clipboard.share = enabled;
            }
            if let Some(send) = dir_payload.get("send").and_then(|v| v.as_bool()) {
                s.clipboard.send = send;
            }
            assert!(s.clipboard.enabled);
            assert!(!s.clipboard.send);
        }
    }

    #[test]
    fn test_pick_candidate_prefers_stable_and_skips_drafts() {
        fn rel(tag: &str, draft: bool, prerelease: bool) -> GithubRelease {
            GithubRelease {
                tag_name: tag.to_string(),
                name: None,
                html_url: String::new(),
                body: None,
                published_at: None,
                draft,
                prerelease,
                assets: Vec::new(),
            }
        }

        // Newest is a draft (skipped); the newest stable wins over the prerelease.
        let picked = pick_candidate(vec![
            rel("v3.1.0", true, false),
            rel("v3.0.1-beta.1", false, true),
            rel("v3.0.0", false, false),
        ]);
        assert_eq!(picked.expect("a candidate").tag_name, "v3.0.0");

        // No stable release: fall back to the newest pre-release.
        let picked = pick_candidate(vec![rel("v3.1.0-beta.1", false, true)]);
        assert_eq!(picked.expect("a candidate").tag_name, "v3.1.0-beta.1");

        // Only drafts: nothing to offer.
        assert!(pick_candidate(vec![rel("v3.1.0", true, false)]).is_none());
    }

    #[test]
    fn test_install_plan_macos() {
        let dmg = Path::new("/tmp/downloads/GlideDesk-3.0.0.dmg");
        let plan = install_plan_for("macos", dmg, None).expect("macos install plan");
        assert_eq!(plan.platform, "macos");
        assert_eq!(plan.installer_path, dmg);
        assert_eq!(plan.target_path, PathBuf::from("/Applications/GlideDesk.app"));
        assert_eq!(plan.program, "bash");
        assert!(plan.script.contains("hdiutil attach"));
        assert!(plan.script.contains("/Applications/GlideDesk.app"));
        assert!(plan.script.contains("xattr -cr"));
        assert!(plan.script.contains("pkill -ix \"glidedesk\""));
        assert!(plan.script.contains("open -a"));
        assert!(plan.script.contains("Cannot write to"));

        // Custom target directory
        let custom_dir = Path::new("/Users/developer/Applications");
        let plan_custom = install_plan_for("macos", dmg, Some(custom_dir)).expect("custom macos plan");
        assert_eq!(plan_custom.target_path, PathBuf::from("/Users/developer/Applications/GlideDesk.app"));

        // Reject non-dmg on macOS
        let exe = Path::new("/tmp/downloads/GlideDesk-3.0.0.exe");
        assert!(install_plan_for("macos", exe, None).is_err());
    }

    #[test]
    fn test_install_plan_windows() {
        let setup_exe = Path::new(r"C:\Users\User\AppData\Local\Temp\GlideDesk-3.0.0-setup.exe");
        let plan = install_plan_for("windows", setup_exe, None).expect("windows install plan");
        assert_eq!(plan.platform, "windows");
        assert_eq!(plan.installer_path, setup_exe);
        assert_eq!(plan.target_path, PathBuf::from(r"C:\Program Files\GlideDesk"));
        assert_eq!(plan.program, "powershell");
        assert!(plan.script.contains("Start-Process"));
        assert!(plan.script.contains("/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS"));
        assert!(plan.script.contains("$proc.ExitCode"));

        // Rejection of portable executable on Windows
        let portable = Path::new(r"C:\temp\GlideDesk-portable-x64.exe");
        let err = install_plan_for("windows", portable, None).unwrap_err();
        assert!(err.contains("Portable executable cannot be installed automatically"));

        // Reject non-exe/msi
        let zip = Path::new(r"C:\temp\GlideDesk.zip");
        assert!(install_plan_for("windows", zip, None).is_err());
    }

    #[test]
    fn test_install_plan_linux() {
        let appimage = Path::new("/tmp/GlideDesk-3.0.0.AppImage");
        let custom_dir = Path::new("/home/user/.local/bin");
        let plan = install_plan_for("linux", appimage, Some(custom_dir)).expect("linux install plan");
        assert_eq!(plan.platform, "linux");
        assert_eq!(plan.installer_path, appimage);
        assert_eq!(plan.target_path, PathBuf::from("/home/user/.local/bin/GlideDesk.AppImage"));
        assert_eq!(plan.program, "bash");
        assert!(plan.script.contains("chmod +x"));
        assert!(plan.script.contains("GlideDesk.AppImage"));
        assert!(plan.script.contains("pkill -ix \"glidedesk\" || pkill -f \"GlideDesk.AppImage\""));

        // Default target directory on Linux
        let default_plan = install_plan_for("linux", appimage, None).expect("linux default plan");
        assert_eq!(default_plan.target_path, PathBuf::from("~/.local/bin/GlideDesk.AppImage"));

        // Reject non-AppImage / non-deb
        let tar = Path::new("/tmp/GlideDesk.tar.gz");
        assert!(install_plan_for("linux", tar, None).is_err());
    }

    #[test]
    fn test_install_plan_unsupported_platform() {
        let file = Path::new("/tmp/GlideDesk.pkg");
        assert!(install_plan_for("unsupported_os", file, None).is_err());
    }
}
