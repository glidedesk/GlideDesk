// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// GlideDesk: Lightweight, Zero-Latency Multi-Device KVM (Tauri v2 + Rust)
// - Ultra-low RAM (~25-45 MB vs 600 MB-1.2 GB in Electron)
// - Zero Garbage Collection latency pauses
// - Async Tokio networking with UDP LAN discovery & WebSocket mesh
// - Native KVM input injection, edge-crossing hooks, and clipboard sync

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod applog;
mod commands;
mod kvm;
mod keylayout;
mod inputmethod;
#[allow(dead_code)] // Public API; callers arrive with the layout UI wiring.
mod kvm_geom;
mod pairsync;
mod fastfile;
mod fileclip;
mod media;
mod network;
mod notifications;
mod pairlimit;
mod pairstore;
mod state;
mod tray;
mod phone;
mod camera;
mod parity;
mod streaming;
mod sharefolder;
mod terminal;
mod device_control;
pub mod transport;
pub mod window_manager;
pub mod chat;
pub mod rezume;
pub mod media_playback;
pub mod wol;
pub mod platform;

use media::NativeBridgeManager;

use network::NetworkManager;
use state::AppState;
use window_manager::ParkedWindowRegistry;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    // Rotating file log + crash log; also keeps stderr when one is attached.
    applog::init();

    // Enter Tauri's Tokio runtime context on the main thread so ambient tokio calls succeed
    let rt_handle = match tauri::async_runtime::handle() {
        tauri::async_runtime::RuntimeHandle::Tokio(h) => h,
    };
    let _tokio_guard = rt_handle.enter();

    let state = AppState::new();
    let setup_state = state.clone();
    let window_reg = ParkedWindowRegistry::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("[SingleInstance] Second launch detected; focusing existing window");
            crate::platform::set_window_visible_policy(app, true);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state.clone())
        .manage(window_reg.clone())
        .manage(NativeBridgeManager::new())
        .manage(phone::PhoneManager::new())
        .manage(camera::CameraManager::new())
        .manage(transport::TransferManager::new())
        .setup(move |app| {
            let handle = app.handle().clone();
            let sampler_state = setup_state.clone();
            let network = NetworkManager::new(handle.clone(), setup_state);

            // 1. Start Async Networking Engine (Phase 2: UDP discovery & WS mesh server)
            network.start();

            // 1b. A paired computer comes back on its own after a restart.
            {
                let net = network.clone();
                tauri::async_runtime::spawn(async move {
                    // Let discovery and the listener settle before dialing.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    net.connect_last_paired().await;
                });
            }

            // 2. Start Clipboard Sync Daemon (Phase 3)
            let clip_watcher = Arc::new(kvm::ClipboardWatcher::new(
                network.clone(),
                state.clone(),
                handle.clone(),
            ));
            clip_watcher.start();

            // 3. Start Native KVM Hook Engine (Phase 3: low-level mouse/keyboard & edge detection)
            kvm::hook_engine::start_hooks(handle.clone(), state.clone(), network.clone());

            // A key left held by the last run (or by a link that went away in the
            // middle of a press) is still down as far as this system knows. Nothing
            // types after that and the system only beeps — the "keyboard is dead
            // until a restart" report. Freeing every modifier here makes restarting
            // the app enough.
            kvm::NativeInputInjector::release_all_keys();

            // 4. Register NetworkManager in Tauri state
            app.manage(network.clone());

            // 4a. Mirror this computer's notifications on the paired one.
            notifications::start(handle.clone(), state.clone(), network.clone());

            // 4a. Keep the Audio tab in step with what the sound devices are doing.
            media::bridge::start_status(handle.clone(), state.clone());

            // 4a. Keep the Terminal tab honest: offer this computer's key and learn
            // whether the other side will let a shell in.
            terminal::start(handle.clone(), network.clone());

            // 4b. Serve the folder the Files page offers, and follow the other
            // computer's shared folder.
            sharefolder::start(handle.clone(), network.clone());

            // 4b. Drain the audio and screen channels from the start, so turning on
            // sharing later cannot find them already full.
            if let Some(bridge) = app.try_state::<NativeBridgeManager>() {
                bridge.start_broadcasting(handle.clone());
            }

            // 5. Link statistics into the log (one line every 30 s while connected)
            let stats_net = network.clone();
            applog::start_link_stats(move || {
                let full = sampler_state.inner.read();
                if full.link.state != "connected" {
                    return None;
                }
                Some(applog::LinkSample {
                    rtt: full.link.rtt,
                    queued: stats_net.queued_bytes(),
                })
            });

            // 6. System tray setup
            if let Err(e) = tray::setup_tray(app.handle()) {
                log::warn!("Could not initialize system tray: {}", e);
            }

            // 7. Window presentation & macOS close/dock behavior
            let is_daemon = std::env::args().any(|a| a == "--daemon" || a == "--headless" || a == "--silent");
            if let Some(window) = app.get_webview_window("main") {
                let win = window.clone();
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                        platform::set_window_visible_policy(&handle, false);
                    }
                });

                if is_daemon {
                    let _ = window.hide();
                    platform::set_window_visible_policy(app.handle(), false);
                } else {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    platform::set_window_visible_policy(app.handle(), true);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_kvm,
            commands::set_layout,
            commands::set_keys,
            commands::set_net,
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
            wol::wake_peer,
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
            commands::set_projection_fullscreen,
            media::bridge::start_audio_capture,
            media::bridge::stop_audio_capture,
            media::bridge::start_screen_streaming,
            media::bridge::stop_screen_streaming,
            media::bridge::emulate_gamepad_event,
            media::bridge::create_virtual_display,
            phone::connect_phone,
            phone::disconnect_phone,
            phone::phone_screen,
            phone::phone_retry,
            phone::phone_pair,
            phone::phone_assets,
            phone::set_phone,
            phone::phone_open_app,
            phone::phone_app_menu,
            phone::phone_close_view,
            phone::phone_install_full,
            phone::phone_media,
            phone::phone_bt_pick,
            phone::hub_message,
            phone::qr_pair,
            phone::adb_qr,
            phone::phone_mount,
            phone::phone_unmount,
            phone::phone_list_cameras,
            camera::camera,
            camera::phone_camera,
            camera::install_vdd,
            camera::remove_vdd,
            camera::cam_send,
            camera::vcam_hello,
            camera::vcam_preview,
            camera::vcam_frame,
            parity::cancel_drop,
            parity::check_firewall,
            parity::choose_received_folder,
            parity::choose_share_folder,
            parity::clear_notif_history,
            parity::launch_app,
            parity::list_apps,
            parity::mute_notif_app,
            parity::open_bluetooth_settings,
            parity::open_mac_drive,
            parity::open_monitor,
            parity::open_notified_app,
            parity::open_received_folder,
            sharefolder::list_peer_folder,
            sharefolder::download_peer_file,
            terminal::open_terminal,
            parity::pin_app,
            parity::remote_screen,
            parity::reset_received_folder,
            parity::set_mac_apps,
            parity::set_mount,
            parity::set_pc_screen,
            parity::set_phone_remote,
            parity::set_startup,
            wol::set_wake,
            terminal::setup_sshd,
            parity::fix_firewall,
            parity::net_wifi,
            parity::net_location,
            parity::net_join_hotspot,
            parity::net_cancel,
            parity::set_remote,
            parity::allow_permission,
            parity::allow_full_disk,
            parity::open_settings,
            parity::relaunch,
            streaming::set_sharing,
            streaming::set_sharing_opts,
            streaming::set_stream_input,
            streaming::select_display,
            streaming::set_display_mode,
            streaming::set_virtual,
            streaming::set_stream_max_height,
            streaming::set_bitrate,
            streaming::set_screen_sound,
            streaming::open_sharing_settings,
            device_control::set_overview,
            device_control::device_do,
            device_control::show_hud,
            device_control::hide_hud,
            transport::transfer::cancel_transfer_task,
            transport::transfer::get_transfer_status,
            transport::route::get_transfer_route,
            chat::set_chats,
            chat::notif_reply,
            rezume::set_rezume,
            rezume::rezume_do,
            media_playback::get_now_playing,
            media_playback::control_media,
        ])
        .build(tauri::generate_context!())
        .expect("error while building GlideDesk application")
        .run(|app_handle, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                platform::set_window_visible_policy(app_handle, true);
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
