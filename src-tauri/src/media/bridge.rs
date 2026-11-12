// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Native OS Bridge Commands (Virtual Display, Audio Capture, Gamepad Emulation)

use crate::media::{AudioEngine, ScreenEngine};
use crate::network::{NetworkManager, WireMessage};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct NativeBridgeManager {
    pub audio_engine: AudioEngine,
    pub screen_engine: ScreenEngine,
    audio_rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Vec<u8>>>>>,
    screen_rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Vec<u8>>>>>,
    broadcasting_started: AtomicBool,
}

impl NativeBridgeManager {
    pub fn new() -> Self {
        let (audio_engine, audio_rx) = AudioEngine::new();
        let (screen_engine, screen_rx) = ScreenEngine::new();

        Self {
            audio_engine,
            screen_engine,
            audio_rx: Arc::new(tokio::sync::Mutex::new(Some(audio_rx))),
            screen_rx: Arc::new(tokio::sync::Mutex::new(Some(screen_rx))),
            broadcasting_started: AtomicBool::new(false),
        }
    }

    /// Starts the one task that drains the audio and screen channels this computer
    /// produces and sends them over the link. Called once at startup and again on
    /// every command that might begin sharing, so a late start cannot leave the
    /// channels filling up unread.
    pub fn start_broadcasting(&self, app: AppHandle) {
        if self.broadcasting_started.swap(true, Ordering::SeqCst) {
            return;
        }
        log::info!("[Media] Broadcasting task starting");

        let audio_rx = self.audio_rx.clone();
        let app_audio = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut rx_opt = audio_rx.lock().await;
            if let Some(mut rx) = rx_opt.take() {
                drop(rx_opt);
                log::info!("[Media] Audio forwarder running");
                while let Some(bytes) = rx.recv().await {
                    let msg = WireMessage {
                        t: "media.audio".to_string(),
                        audio_bytes: Some(bytes),
                        ..Default::default()
                    };
                    let _ = app_audio.emit("media-audio-stream", &msg);
                    if let Some(net) = app_audio.try_state::<NetworkManager>() {
                        net.send_packet(msg).await;
                    }
                }
            }
        });

        let screen_rx = self.screen_rx.clone();
        let app_screen = app.clone();
        tauri::async_runtime::spawn(async move {
            use base64::Engine;
            let mut rx_opt = screen_rx.lock().await;
            if let Some(mut rx) = rx_opt.take() {
                drop(rx_opt);
                log::info!("[Media] Screen forwarder running");
                let mut sent = 0u64;
                while let Some(frame) = rx.recv().await {
                    sent += 1;
                    if sent == 1 || sent % 60 == 0 {
                        log::info!("[ScreenShare] sending frame #{} ({} bytes)", sent, frame.len());
                    }
                    // The capture thread hands back one JPEG. Ship it as a data URL:
                    // the other window can put it straight in `img.src`, and it takes
                    // far less room on the wire than a byte array.
                    let size = frame.len();
                    let msg = WireMessage {
                        t: "media.screen".to_string(),
                        frame_b64: Some(format!(
                            "data:image/jpeg;base64,{}",
                            base64::prelude::BASE64_STANDARD.encode(&frame)
                        )),
                        bytes: Some(size),
                        fps: Some(12),
                        ..Default::default()
                    };
                    if let Some(net) = app_screen.try_state::<NetworkManager>() {
                        net.send_packet(msg).await;
                    }
                }
            }
        });
    }
}

#[tauri::command]
pub fn start_audio_capture(bridge: State<'_, NativeBridgeManager>, app: AppHandle) -> Result<(), String> {
    bridge.start_broadcasting(app);
    bridge.audio_engine.start_capture()
}

#[tauri::command]
pub fn stop_audio_capture(bridge: State<'_, NativeBridgeManager>) {
    bridge.audio_engine.stop_capture();
}

#[tauri::command]
pub fn start_screen_streaming(fps: u32, bridge: State<'_, NativeBridgeManager>, app: AppHandle) -> Result<(), String> {
    bridge.start_broadcasting(app);
    bridge.screen_engine.start_streaming(fps)
}

#[tauri::command]
pub fn stop_screen_streaming(bridge: State<'_, NativeBridgeManager>) {
    bridge.screen_engine.stop_streaming();
}

#[tauri::command]
pub fn emulate_gamepad_event(
    button: String,
    pressed: bool,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("Pure Rust gamepad event emulation: button={}, pressed={}", button, pressed);
    let pkt = WireMessage {
        t: "gamepad.event".to_string(),
        k: Some(button),
        b: Some(if pressed { 1 } else { 0 }),
        ..Default::default()
    };
    if let Some(net) = app.try_state::<NetworkManager>() {
        net.send_packet_sync(pkt);
    }
    Ok(())
}

#[tauri::command]
pub fn create_virtual_display(width: u32, height: u32) -> Result<(), String> {
    log::info!("Pure Rust virtual display creation: {}x{}", width, height);
    #[cfg(target_os = "macos")]
    {
        let has_vdd = std::path::Path::new("/Library/CoreMediaIO/Plug-Ins/DAL").exists();
        if !has_vdd {
            return Err("Virtual display driver not installed on host macOS system. Please install OBS Virtual Camera or CoreMediaIO DAL plugin.".to_string());
        }
        // Native virtual display creation requires elevated DisplayServices authorization
        Err("Creating native virtual display requires elevated DisplayServices authorization or BetterDisplay/IddCx driver.".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let driver_path = std::path::Path::new("C:\\Windows\\System32\\drivers\\IddCx.sys");
        if !driver_path.exists() {
            return Err("Virtual display driver (IddCx) not installed on host Windows system. Run PowerShell as Administrator to install IddCx driver.".to_string());
        }
        Err("Virtual display activation requires Windows Display Switcher elevation.".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        Err("Virtual display creation requires xrandr virtual monitor setup. Run 'xrandr --setmonitor' with display manager permissions.".to_string())
    }
}
