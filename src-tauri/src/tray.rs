// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust: Full System Tray & Menu Bar Integration (TwinDesk-equivalent feature set)

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use crate::state::AppState;

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Status header
    let status_i = MenuItem::with_id(app, "status", "GlideDesk: Ready", false, None::<&str>)?;

    // 2. Connect To Submenu
    let addr_i = MenuItem::with_id(app, "by_address", "Connect by Address…", true, None::<&str>)?;
    let look_i = MenuItem::with_id(app, "look_again", "Look Again", true, None::<&str>)?;
    let connect_menu = Submenu::with_items(app, "Connect To", true, &[&addr_i, &look_i])?;

    // 3. Quick Cursor Sharing toggle
    let cursor_sharing_i = CheckMenuItem::with_id(app, "cursor_sharing", "Cursor Sharing", true, true, None::<&str>)?;

    // 4. Mouse and Keyboard Submenu
    let share_kb_i = CheckMenuItem::with_id(app, "share_keyboard", "Share Keyboard", true, true, None::<&str>)?;
    let lock_cursor_i = CheckMenuItem::with_id(app, "lock_cursor", "Lock Cursor to Screen", true, false, None::<&str>)?;
    let sep_mk1 = PredefinedMenuItem::separator(app)?;
    let drive_other_i = CheckMenuItem::with_id(app, "drive_other", "Control Other Computer", true, true, None::<&str>)?;
    let be_driven_i = CheckMenuItem::with_id(app, "be_driven", "Allow Being Controlled", true, true, None::<&str>)?;
    let sep_mk2 = PredefinedMenuItem::separator(app)?;
    let show_popup_i = CheckMenuItem::with_id(app, "show_popup", "Show a Popup When Switching", true, true, None::<&str>)?;
    let mk_menu = Submenu::with_items(
        app,
        "Mouse and Keyboard",
        true,
        &[&share_kb_i, &lock_cursor_i, &sep_mk1, &drive_other_i, &be_driven_i, &sep_mk2, &show_popup_i],
    )?;

    // 5. Screen Submenu
    let share_screen_i = CheckMenuItem::with_id(app, "share_screen", "Share Screen", true, false, None::<&str>)?;
    let recv_screen_i = CheckMenuItem::with_id(app, "receive_screen", "Receive Screen", true, false, None::<&str>)?;
    let screen_menu = Submenu::with_items(app, "Screen", true, &[&share_screen_i, &recv_screen_i])?;

    // 6. Clipboard and Sound Submenu
    let share_clip_i = CheckMenuItem::with_id(app, "share_clipboard", "Share Clipboard", true, true, None::<&str>)?;
    let send_sound_i = CheckMenuItem::with_id(app, "send_sound", "Send Sound", true, true, None::<&str>)?;
    let clip_menu = Submenu::with_items(app, "Clipboard and Sound", true, &[&share_clip_i, &send_sound_i])?;

    // 7. Files Submenu
    let drag_files_i = CheckMenuItem::with_id(app, "drag_files", "Drag Files Across the Screen Edge", true, true, None::<&str>)?;
    let mount_files_i = CheckMenuItem::with_id(app, "mount_files", "Mount Shared Drives", true, false, None::<&str>)?;
    let files_menu = Submenu::with_items(app, "Files", true, &[&drag_files_i, &mount_files_i])?;

    // 8. General actions
    let sep_top = PredefinedMenuItem::separator(app)?;
    let sep_mid = PredefinedMenuItem::separator(app)?;
    let sep_bot = PredefinedMenuItem::separator(app)?;
    let show_i = MenuItem::with_id(app, "show", "Open GlideDesk…", true, None::<&str>)?;
    let updates_i = MenuItem::with_id(app, "check_updates", "Check for Updates…", true, None::<&str>)?;
    let logs_i = MenuItem::with_id(app, "open_logs", "Open Log Folder", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit GlideDesk", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_i,
            &connect_menu,
            &sep_top,
            &cursor_sharing_i,
            &mk_menu,
            &screen_menu,
            &clip_menu,
            &files_menu,
            &sep_mid,
            &show_i,
            &updates_i,
            &logs_i,
            &sep_bot,
            &quit_i,
        ],
    )?;

    let tray_icon = if let Ok(rgba) = image::load_from_memory(include_bytes!("../../ui/assets/trayTemplate@2x.png"))
        .map(|img| img.to_rgba8())
    {
        let (w, h) = rgba.dimensions();
        Image::new_owned(rgba.into_raw(), w, h)
    } else {
        app.default_window_icon().unwrap().clone()
    };

    let _tray = TrayIconBuilder::new()
        .icon(tray_icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "by_address" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = window.emit("go-tab", "overview");
                }
            }
            "look_again" => {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let _ = crate::commands::refresh_found(state, app_handle.clone());
                });
            }
            "cursor_sharing" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.kvm.linked = !full.kvm.linked;
                full.kvm_state.linked = full.kvm.linked;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "share_keyboard" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.kvm.keyboard = !full.kvm.keyboard;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "lock_cursor" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.kvm.locked = !full.kvm.locked;
                full.kvm_state.locked = full.kvm.locked;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "drive_other" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.kvm.drive_other = !full.kvm.drive_other;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "be_driven" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.kvm.be_driven = !full.kvm.be_driven;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "share_screen" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.stream.share = if full.stream.share == "on" { "off".to_string() } else { "on".to_string() };
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "receive_screen" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.stream.state = if full.stream.state == "on" { "off".to_string() } else { "on".to_string() };
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "share_clipboard" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.clipboard.share = !full.clipboard.share;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "send_sound" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.audio.share = !full.audio.share;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "drag_files" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.files.share_mac = !full.files.share_mac;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "mount_files" => {
                let state = app.state::<AppState>();
                let mut full = state.inner.write();
                full.files.mount_mac = !full.files.mount_mac;
                let snap = full.clone();
                drop(full);
                let _ = app.emit("state-changed", snap);
            }
            "check_updates" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let _ = crate::commands::check_updates(state, app_handle.clone()).await;
                });
            }
            "open_logs" => {
                if let Ok(dir) = app.path().app_log_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    #[cfg(target_os = "linux")]
                    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
