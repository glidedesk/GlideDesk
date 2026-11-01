// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// GlideDesk: Lightweight, Zero-Latency Multi-Device KVM (Tauri v2 + Rust)
// - Ultra-low RAM (~25-45 MB vs 600 MB-1.2 GB in Electron)
// - Zero Garbage Collection latency pauses
// - Async Tokio networking with UDP LAN discovery & WebSocket mesh
// - Native KVM input injection, edge-crossing hooks, and clipboard sync

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod kvm;
mod network;
mod state;
mod tray;
pub mod window_manager;

use network::NetworkManager;
use state::AppState;
use window_manager::ParkedWindowRegistry;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    env_logger::init();

    // Enter Tauri's Tokio runtime context on the main thread so ambient tokio calls succeed
    let rt_handle = match tauri::async_runtime::handle() {
        tauri::async_runtime::RuntimeHandle::Tokio(h) => h,
    };
    let _tokio_guard = rt_handle.enter();

    let state = AppState::new();
    let setup_state = state.clone();
    let window_reg = ParkedWindowRegistry::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state.clone())
        .manage(window_reg.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            let network = NetworkManager::new(handle.clone(), setup_state);

            // 1. Start Async Networking Engine (Phase 2: UDP discovery & WS mesh server)
            network.start();

            // 2. Start Clipboard Sync Daemon (Phase 3)
            let clip_watcher = Arc::new(kvm::ClipboardWatcher::new(network.clone()));
            clip_watcher.start();

            // 3. Start Native KVM Hook Engine (Phase 3: low-level mouse/keyboard & edge detection)
            kvm::hook_engine::start_hooks(handle.clone(), state.clone(), network.clone());

            // 4. Register NetworkManager in Tauri state
            app.manage(network);

            // 5. System tray setup
            if let Err(e) = tray::setup_tray(app.handle()) {
                log::warn!("Could not initialize system tray: {}", e);
            }

            // 6. Window presentation & macOS close/dock behavior
            let is_daemon = std::env::args().any(|a| a == "--daemon" || a == "--headless" || a == "--silent");
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });

                if is_daemon {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_kvm,
            commands::set_layout,
            commands::set_keys,
            commands::set_tab,
            commands::set_clipboard,
            commands::copy_text,
            commands::set_updates,
            commands::skip_update,
            commands::unskip_update,
            commands::check_updates,
            commands::open_update,
            commands::install_update,
            commands::open_link,
            commands::pair,
            commands::connect_address,
            commands::connect_again,
            commands::refresh_found,
            commands::disconnect,
            commands::pair_respond,
            commands::set_app,
            commands::set_monitor,
            commands::copy_history,
            commands::clear_history,
            commands::set_multi,
            commands::skip_setup,
            commands::confirm_code,
            commands::cancel_pair,
            commands::forget,
            commands::wake_peer,
            commands::get_displays,
            commands::send_files,
            commands::choose_send_files,
            commands::send_wire_message,
            commands::handoff_window,
            commands::handoff_window_at_point,
            commands::return_window,
            commands::get_parked_windows,
            commands::show_projection_window,
            commands::hide_projection_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building GlideDesk application")
        .run(|app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                let window_reg = app_handle.state::<crate::window_manager::ParkedWindowRegistry>();
                let restored = window_reg.restore_all();
                if !restored.is_empty() {
                    log::info!("[WindowHandoff] App exit: restored {} parked window(s)", restored.len());
                }
            }
            _ => {}
        });
}
