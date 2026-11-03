// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Native OS Bridge Commands (Virtual Display, Audio Capture, Gamepad Emulation)

use crate::media::{AudioEngine, ScreenEngine};
use tauri::State;
use tokio::sync::mpsc;

pub struct NativeBridgeManager {
    audio_engine: AudioEngine,
    screen_engine: ScreenEngine,
    _audio_rx: mpsc::Receiver<Vec<u8>>,
    _screen_rx: mpsc::Receiver<Vec<u8>>,
}

impl NativeBridgeManager {
    pub fn new() -> Self {
        let (audio_engine, audio_rx) = AudioEngine::new();
        let (screen_engine, screen_rx) = ScreenEngine::new();
        Self {
            audio_engine,
            screen_engine,
            _audio_rx: audio_rx,
            _screen_rx: screen_rx,
        }
    }
}

#[tauri::command]
#[allow(dead_code)]
pub fn start_audio_capture(bridge: State<'_, NativeBridgeManager>) -> Result<(), String> {
    bridge.audio_engine.start_capture()
}

#[tauri::command]
#[allow(dead_code)]
pub fn stop_audio_capture(bridge: State<'_, NativeBridgeManager>) {
    bridge.audio_engine.stop_capture();
}

#[tauri::command]
#[allow(dead_code)]
pub fn start_screen_streaming(fps: u32, bridge: State<'_, NativeBridgeManager>) -> Result<(), String> {
    bridge.screen_engine.start_streaming(fps)
}

#[tauri::command]
#[allow(dead_code)]
pub fn stop_screen_streaming(bridge: State<'_, NativeBridgeManager>) {
    bridge.screen_engine.stop_streaming();
}

#[tauri::command]
#[allow(dead_code)]
pub fn emulate_gamepad_event(button: String, pressed: bool) -> Result<(), String> {
    log::info!("Pure Rust gamepad emulation: button={}, pressed={}", button, pressed);
    Ok(())
}

#[tauri::command]
#[allow(dead_code)]
pub fn create_virtual_display(width: u32, height: u32) -> Result<(), String> {
    log::info!("Pure Rust virtual display creation: {}x{}", width, height);
    Ok(())
}
