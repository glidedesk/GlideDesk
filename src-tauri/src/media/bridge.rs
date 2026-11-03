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

    pub fn start_broadcasting(&self, app: AppHandle) {
        if self.broadcasting_started.load(Ordering::SeqCst) {
            return;
        }
        self.broadcasting_started.store(true, Ordering::SeqCst);

        let audio_rx = self.audio_rx.clone();
        let app_audio = app.clone();
        tokio::spawn(async move {
            let mut rx_opt = audio_rx.lock().await;
            if let Some(mut rx) = rx_opt.take() {
                drop(rx_opt);
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
        tokio::spawn(async move {
            let mut rx_opt = screen_rx.lock().await;
            if let Some(mut rx) = rx_opt.take() {
                drop(rx_opt);
                while let Some(frame) = rx.recv().await {
                    let msg = WireMessage {
                        t: "media.screen".to_string(),
                        frame_png: Some(frame),
                        fps: Some(30),
                        ..Default::default()
                    };
                    let _ = app_screen.emit("media-screen-stream", &msg);
                    let _ = app_screen.emit("media-screen-received", &msg);
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
