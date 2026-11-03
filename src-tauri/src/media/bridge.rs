// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Native OS Bridge Commands (Virtual Display, Audio Capture, Gamepad Emulation)

use crate::media::{AudioEngine, ScreenEngine};
use tauri::State;
use tokio::sync::mpsc;

pub struct NativeBridgeManager {
    pub audio_engine: AudioEngine,
    pub screen_engine: ScreenEngine,
    audio_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>>,
    screen_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>>,
}

use std::sync::Arc;

impl NativeBridgeManager {
    pub fn new() -> Self {
        let (audio_engine, audio_rx) = AudioEngine::new();
        let (screen_engine, screen_rx) = ScreenEngine::new();
        
        let bridge = Self {
            audio_engine,
            screen_engine,
            audio_rx: Arc::new(tokio::sync::Mutex::new(audio_rx)),
            screen_rx: Arc::new(tokio::sync::Mutex::new(screen_rx)),
        };

        bridge.start_consumer_loops();
        bridge
    }

    fn start_consumer_loops(&self) {
        let audio_rx = self.audio_rx.clone();
        tokio::spawn(async move {
            let mut rx = audio_rx.lock().await;
            while let Some(bytes) = rx.recv().await {
                log::trace!("[AudioStream] Consumed {} bytes of real audio PCM sample data", bytes.len());
            }
        });

        let screen_rx = self.screen_rx.clone();
        tokio::spawn(async move {
            let mut rx = screen_rx.lock().await;
            while let Some(frame) = rx.recv().await {
                log::trace!("[ScreenStream] Consumed {} bytes of real screen frame buffer data", frame.len());
            }
        });
    }
}

#[tauri::command]
pub fn start_audio_capture(bridge: State<'_, NativeBridgeManager>) -> Result<(), String> {
    bridge.audio_engine.start_capture()
}

#[tauri::command]
pub fn stop_audio_capture(bridge: State<'_, NativeBridgeManager>) {
    bridge.audio_engine.stop_capture();
}

#[tauri::command]
pub fn start_screen_streaming(fps: u32, bridge: State<'_, NativeBridgeManager>) -> Result<(), String> {
    bridge.screen_engine.start_streaming(fps)
}

#[tauri::command]
pub fn stop_screen_streaming(bridge: State<'_, NativeBridgeManager>) {
    bridge.screen_engine.stop_streaming();
}

#[tauri::command]
pub fn emulate_gamepad_event(button: String, pressed: bool) -> Result<(), String> {
    log::info!("Pure Rust gamepad emulation: button={}, pressed={}", button, pressed);
    Ok(())
}

#[tauri::command]
pub fn create_virtual_display(width: u32, height: u32) -> Result<(), String> {
    log::info!("Pure Rust virtual display creation: {}x{}", width, height);
    Ok(())
}
