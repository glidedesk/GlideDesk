// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Pure Rust Native OS Bridge Commands (Virtual Display, Audio Capture, Gamepad Emulation)

use crate::media::{AudioEngine, AudioPlayer, ScreenEngine};
use crate::network::{NetworkManager, WireMessage};
use crate::state::AppState;
use base64::Engine as _;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};

/// One wire packet carries about 50 ms of sound: the capture backends hand over
/// 10 ms at a time, and one packet per callback would be mostly overhead.
const AUDIO_PACKET_BYTES: usize = (48000 * 2 * 4) / 20;

pub struct NativeBridgeManager {
    pub audio_engine: AudioEngine,
    pub player: AudioPlayer,
    pub screen_engine: ScreenEngine,
    audio_rx: std::sync::Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Vec<u8>>>>>,
    screen_rx: std::sync::Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Vec<u8>>>>>,
    broadcasting_started: AtomicBool,
}

impl NativeBridgeManager {
    pub fn new() -> Self {
        let (audio_engine, audio_rx) = AudioEngine::new();
        let (screen_engine, screen_rx) = ScreenEngine::new();

        Self {
            audio_engine,
            player: AudioPlayer::new(),
            screen_engine,
            audio_rx: std::sync::Arc::new(tokio::sync::Mutex::new(Some(audio_rx))),
            screen_rx: std::sync::Arc::new(tokio::sync::Mutex::new(Some(screen_rx))),
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
                let mut pending: Vec<u8> = Vec::with_capacity(AUDIO_PACKET_BYTES * 2);
                while let Some(bytes) = rx.recv().await {
                    pending.extend_from_slice(&bytes);
                    if pending.len() < AUDIO_PACKET_BYTES {
                        continue;
                    }
                    let chunk = std::mem::replace(&mut pending, Vec::with_capacity(AUDIO_PACKET_BYTES * 2));
                    let msg = WireMessage {
                        t: "media.audio".to_string(),
                        audio_b64: Some(
                            base64::prelude::BASE64_STANDARD.encode(&chunk),
                        ),
                        audio_rate: Some(crate::media::audio::SAMPLE_RATE),
                        audio_chs: Some(crate::media::audio::CHANNELS),
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
    bridge.start_broadcasting(app.clone());
    bridge.audio_engine.start_capture()?;
    if let Some(state) = app.try_state::<AppState>() {
        crate::commands::push_state(&app, &state);
    }
    Ok(())
}

#[tauri::command]
pub fn stop_audio_capture(bridge: State<'_, NativeBridgeManager>, app: AppHandle) {
    bridge.audio_engine.stop_capture();
    bridge.player.stop();
    if let Some(state) = app.try_state::<AppState>() {
        crate::commands::push_state(&app, &state);
    }
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

// ---------------------------------------------------------------------------
// Sound both ways, driven by the Audio tab and the other computer's policy
// ---------------------------------------------------------------------------

/// Start or stop capture and playback to match the tab and what the other
/// computer accepts. Called on every settings change and whenever the link
/// opens or closes.
pub fn apply_audio(app: &AppHandle, state: &AppState) {
    let Some(bridge) = app.try_state::<NativeBridgeManager>() else {
        return;
    };
    let (enabled, send, receive) = {
        let s = state.inner.read();
        (s.audio.enabled, s.audio.send, s.audio.receive)
    };
    let peer_accepts = app
        .try_state::<NetworkManager>()
        .map(|n| n.audio_accept())
        .unwrap_or(false);
    let connected = state.inner.read().link.state == "connected";
    bridge.player.set_volume(playback_volume(state));

    let want_send = enabled && send && peer_accepts && connected;
    if want_send && !bridge.audio_engine.is_capturing() {
        bridge.start_broadcasting(app.clone());
        if let Err(e) = bridge.audio_engine.start_capture() {
            log::warn!("[Audio] Could not send this computer's sound: {}", e);
            let mut s = state.inner.write();
            s.audio.error = e;
        }
    } else if !want_send && bridge.audio_engine.is_capturing() {
        bridge.audio_engine.stop_capture();
    }

    if !(enabled && receive) && bridge.player.is_playing() {
        bridge.player.stop();
    }
}

/// A `media.audio` packet from the other computer: play it here.
pub fn audio_incoming(app: &AppHandle, state: &AppState, pkt: &WireMessage) {
    let (enabled, receive) = {
        let s = state.inner.read();
        (s.audio.enabled, s.audio.receive)
    };
    if !enabled || !receive {
        return;
    }
    let Some(bridge) = app.try_state::<NativeBridgeManager>() else {
        return;
    };
    let bytes = match (&pkt.audio_b64, &pkt.audio_bytes) {
        (Some(b64), _) => base64::prelude::BASE64_STANDARD
            .decode(b64.as_bytes())
            .unwrap_or_default(),
        (None, Some(raw)) => raw.clone(),
        _ => return,
    };
    if bytes.is_empty() {
        return;
    }
    bridge.player.set_volume(playback_volume(state));
    bridge.player.start();
    bridge.player.push(&bytes);
    let _ = app.emit("media-audio-received", &bytes);
}

/// The player's volume: the tab's slider, unless the person asked not to hear
/// the other computer here while this one is sending its own sound (that is what
/// stops sound played here from being captured and sent back).
fn playback_volume(state: &AppState) -> f64 {
    let s = state.inner.read();
    let quiet =
        s.audio.mute_local && s.audio.send && s.audio.enabled && s.link.state == "connected";
    if quiet {
        0.0
    } else {
        s.audio.volume
    }
}

/// The other computer's `audio.policy`: whether it plays our sound.
pub fn set_peer_policy(app: &AppHandle, state: &AppState, accept: bool) {
    if let Some(net) = app.try_state::<NetworkManager>() {
        net.set_audio_accept(accept);
    }
    {
        let mut s = state.inner.write();
        s.audio.mac_accepts = accept;
    }
    log::info!(
        "[Audio] The other computer {} our sound",
        if accept { "plays" } else { "does not play" }
    );
    apply_audio(app, state);
    crate::commands::push_state(app, state);
}

/// Tell the other computer whether this one plays its sound.
pub fn send_policy(network: &NetworkManager, state: &AppState) {
    let accept = {
        let s = state.inner.read();
        s.audio.enabled && s.audio.receive
    };
    network.send_packet_sync(WireMessage {
        t: "audio.policy".to_string(),
        v: Some(1),
        accept: Some(accept),
        ..Default::default()
    });
}

/// Keep `sending` and `receiving` in step with what the devices are really
/// doing, so the tab says "Playing X" only while sound is flowing.
pub fn start_status(app: AppHandle, state: AppState) {
    let thread = std::thread::Builder::new()
        .name("glidedesk-audio-status".into())
        .spawn(move || {
            let mut last_log = std::time::Instant::now();
            let mut last_packets = 0u64;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                let Some(bridge) = app.try_state::<NativeBridgeManager>() else {
                    continue;
                };
                let sending = bridge.audio_engine.is_capturing();
                let receiving = bridge.player.is_playing();
                let (packets, peak) = bridge.player.traffic();
                let mut error = bridge.audio_engine.error();
                if error.is_empty() {
                    error = bridge.player.error();
                }
                if packets > last_packets && last_log.elapsed() > std::time::Duration::from_secs(10) {
                    log::info!(
                        "[Audio] Played {} more sound packets, peak {:.2}",
                        packets - last_packets,
                        peak
                    );
                    last_packets = packets;
                    last_log = std::time::Instant::now();
                    bridge.player.reset_peak();
                }
                let mut changed = false;
                {
                    let mut s = state.inner.write();
                    if s.audio.sending != sending {
                        s.audio.sending = sending;
                        changed = true;
                    }
                    if s.audio.receiving != receiving {
                        s.audio.receiving = receiving;
                        changed = true;
                    }
                    // The devices own this line: a failure shows while it lasts and
                    // clears once sound flows again.
                    if s.audio.error != error {
                        s.audio.error = error;
                        changed = true;
                    }
                }
                if changed {
                    crate::commands::push_state(&app, &state);
                }
            }
        });
    if let Err(e) = thread {
        log::warn!("[Audio] Could not start the status thread: {}", e);
    }
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
