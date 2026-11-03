// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// ReZume Shelf & Handoff Module (Mac, PC, Linux, and Android Phone)

use sha2::{Digest, Sha256};
use std::path::Path;
use tauri::{AppHandle, State};

use crate::commands::push_state;
use crate::network::NetworkManager;
use crate::phone::PhoneManager;
use crate::state::AppState;

/// Helper to open target URL or file path cross-platform.
pub fn open_target(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open target '{}': {}", target, e))?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .spawn()
            .map_err(|e| format!("Failed to open target '{}': {}", target, e))?;
        Ok(())
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open target '{}': {}", target, e))?;
        Ok(())
    }
}

/// Verify SHA-256 integrity of a file before launching or completing handoff.
pub fn verify_file_checksum(file_path: &Path, expected_hex: &str) -> Result<(), String> {
    if !file_path.exists() {
        return Err(format!("File does not exist: {}", file_path.display()));
    }
    let data = std::fs::read(file_path)
        .map_err(|e| format!("Failed to read file for checksum verification: {}", e))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let calculated = format!("{:x}", hasher.finalize());
    let clean_expected = expected_hex.trim().to_lowercase();
    if calculated != clean_expected {
        return Err(format!(
            "Checksum mismatch: expected {}, calculated {}",
            clean_expected, calculated
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn set_rezume(
    partial: serde_json::Value,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[ReZume] set_rezume: {:?}", partial);
    {
        let mut s = state.inner.write();
        if let Some(on) = partial.get("on").and_then(|v| v.as_bool()) {
            s.rezume.on = on;
        }
        if let Some(send) = partial.get("send").and_then(|v| v.as_bool()) {
            s.rezume.send = send;
        }
        if let Some(receive) = partial.get("receive").and_then(|v| v.as_bool()) {
            s.rezume.receive = receive;
        }
        if let Some(web) = partial.get("web").and_then(|v| v.as_bool()) {
            s.rezume.web = web;
        }
        if let Some(docs) = partial.get("docs").and_then(|v| v.as_bool()) {
            s.rezume.docs = docs;
        }
        if let Some(apps) = partial.get("apps").and_then(|v| v.as_bool()) {
            s.rezume.apps = apps;
        }
        if let Some(mirror) = partial.get("mirror").and_then(|v| v.as_bool()) {
            s.rezume.mirror = mirror;
        }
        if let Some(push) = partial.get("push").and_then(|v| v.as_bool()) {
            s.rezume.push = push;
        }
        if let Some(notify) = partial.get("notify").and_then(|v| v.as_bool()) {
            s.rezume.notify = notify;
        }
        if let Some(taskbar) = partial.get("taskbar").and_then(|v| v.as_bool()) {
            s.rezume.taskbar = taskbar;
        }
        if let Some(tray) = partial.get("tray").and_then(|v| v.as_bool()) {
            s.rezume.tray = tray;
        }
        if let Some(tab) = partial.get("tab").and_then(|v| v.as_bool()) {
            s.rezume.tab = tab;
        }
        if let Some(video) = partial.get("video").and_then(|v| v.as_bool()) {
            s.rezume.video = video;
        }
    }
    push_state(&app, &state);
    Ok(())
}

#[tauri::command]
pub async fn rezume_do(
    action: String,
    id: Option<String>,
    state: State<'_, AppState>,
    network: State<'_, NetworkManager>,
    phone_mgr: State<'_, PhoneManager>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[ReZume] rezume_do: action={}, id={:?}", action, id);
    match action.as_str() {
        "take" => {
            let target_item = {
                let s = state.inner.read();
                if let Some(ref target_id) = id {
                    s.rezume_shelf
                        .shelf
                        .iter()
                        .find(|x| &x.id == target_id)
                        .cloned()
                        .or_else(|| {
                            s.rezume_shelf
                                .incoming
                                .as_ref()
                                .filter(|x| &x.id == target_id)
                                .cloned()
                        })
                } else {
                    s.rezume_shelf.incoming.clone()
                }
            };

            let item = target_item.ok_or_else(|| "No active rezume offer to take".to_string())?;

            match item.item_type.as_str() {
                "web" | "video" => {
                    if let Some(ref url) = item.url {
                        open_target(url)?;
                    } else {
                        return Err("No URL specified for web/video item".to_string());
                    }
                }
                "doc" => {
                    if let Some(ref path_str) = item.path {
                        let path = Path::new(path_str);
                        if !path.exists() {
                            return Err(format!("Document file does not exist: {}", path_str));
                        }
                        if let Some(ref expected_hash) = item.checksum_sha256 {
                            verify_file_checksum(path, expected_hash)?;
                        }
                        open_target(path_str)?;
                    } else {
                        return Err("No file path specified for document item".to_string());
                    }
                }
                "app" => {
                    if let Some(ref pkg) = item.pkg {
                        let serial = {
                            let s_guard = phone_mgr.session.read();
                            s_guard.as_ref().map(|s| s.serial.clone())
                        };
                        if let Some(s) = serial {
                            let cmd = format!("monkey -p {} -c android.intent.category.LAUNCHER 1", pkg);
                            phone_mgr.adb.shell(&s, &cmd)
                                .map_err(|e| format!("Failed to launch phone app '{}': {}", pkg, e))?;
                        } else {
                            return Err("Phone app handoff requires active Android phone session".to_string());
                        }
                    } else {
                        return Err("No package name specified for app item".to_string());
                    }
                }
                other => {
                    return Err(format!("Unsupported rezume item type: '{}'", other));
                }
            }

            // Mark completed and clear incoming if it was the incoming item
            {
                let mut s = state.inner.write();
                if let Some(ref mut inc) = s.rezume_shelf.incoming {
                    if inc.id == item.id {
                        inc.completed = true;
                    }
                }
                for sh in s.rezume_shelf.shelf.iter_mut() {
                    if sh.id == item.id {
                        sh.completed = true;
                    }
                }
                if s.rezume_shelf
                    .incoming
                    .as_ref()
                    .map(|x| x.id == item.id)
                    .unwrap_or(false)
                {
                    s.rezume_shelf.incoming = None;
                }
            }

            network
                .send_packet(crate::network::WireMessage {
                    t: "rz.take".to_string(),
                    id: Some(item.id),
                    ..Default::default()
                })
                .await;

            push_state(&app, &state);
            Ok(())
        }
        "dismiss" => {
            {
                let mut s = state.inner.write();
                s.rezume_shelf.incoming = None;
            }
            push_state(&app, &state);
            Ok(())
        }
        "push" => {
            let active_item = {
                let s = state.inner.read();
                s.rezume_shelf
                    .incoming
                    .clone()
                    .or_else(|| s.rezume_shelf.shelf.first().cloned())
            };
            if let Some(item) = active_item {
                network
                    .send_packet(crate::network::WireMessage {
                        t: "rz.offer".to_string(),
                        rezume_item: Some(item),
                        ..Default::default()
                    })
                    .await;
                push_state(&app, &state);
                Ok(())
            } else {
                Err("No item available to push".to_string())
            }
        }
        "clear" => {
            {
                let mut s = state.inner.write();
                s.rezume_shelf.incoming = None;
                s.rezume_shelf.shelf.clear();
            }
            push_state(&app, &state);
            Ok(())
        }
        other => Err(format!("Unknown rezume action: '{}'", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RezumeItem;

    #[test]
    fn test_rezume_settings_partial_merge() {
        let app_state = AppState::new();
        {
            let s = app_state.inner.write();
            assert!(!s.rezume.on);
            assert!(s.rezume.docs);
            assert!(s.rezume.web);
        }

        // Merge partial updates
        let partial = serde_json::json!({
            "on": true,
            "docs": false,
            "notify": true
        });

        {
            let mut s = app_state.inner.write();
            if let Some(on) = partial.get("on").and_then(|v| v.as_bool()) {
                s.rezume.on = on;
            }
            if let Some(docs) = partial.get("docs").and_then(|v| v.as_bool()) {
                s.rezume.docs = docs;
            }
            if let Some(notify) = partial.get("notify").and_then(|v| v.as_bool()) {
                s.rezume.notify = notify;
            }
        }

        let snap = app_state.snapshot();
        assert!(snap.rezume.on);
        assert!(!snap.rezume.docs);
        assert!(snap.rezume.notify);
        assert!(snap.rezume.web); // preserved
    }

    #[test]
    fn test_rezume_file_checksum_verification() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("rezume_test_file.txt");
        let content = b"GlideDesk ReZume Verified Document Content";
        std::fs::write(&test_file, content).expect("write test file");

        let mut hasher = Sha256::new();
        hasher.update(content);
        let correct_hash = format!("{:x}", hasher.finalize());

        // Verification succeeds with correct hash
        assert!(verify_file_checksum(&test_file, &correct_hash).is_ok());

        // Verification fails with mismatched hash
        let bad_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(verify_file_checksum(&test_file, bad_hash).is_err());

        // Verification fails on non-existent file
        let missing_file = temp_dir.join("non_existent_file_xyz.txt");
        assert!(verify_file_checksum(&missing_file, &correct_hash).is_err());

        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_rezume_shelf_capacity_and_eviction() {
        let mut shelf: Vec<RezumeItem> = Vec::new();

        for i in 0..60 {
            shelf.insert(
                0,
                RezumeItem {
                    id: format!("item_{}", i),
                    item_type: "web".to_string(),
                    title: format!("Title {}", i),
                    url: Some(format!("https://example.com/{}", i)),
                    path: None,
                    pkg: None,
                    name: None,
                    icon: None,
                    size: None,
                    at: i as u64,
                    checksum_sha256: None,
                    completed: false,
                },
            );
            if shelf.len() > 50 {
                shelf.truncate(50);
            }
        }

        assert_eq!(shelf.len(), 50);
        assert_eq!(shelf[0].id, "item_59"); // most recent
        assert_eq!(shelf[49].id, "item_10"); // oldest preserved
    }
}
