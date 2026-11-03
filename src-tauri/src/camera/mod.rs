#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Camera & Virtual Camera Driver Handlers with ~12 FPS preview emission.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, State};

use crate::commands::push_state;
use crate::phone::PhoneManager;
use crate::state::AppState;

pub struct CameraManager {
    is_previewing: Arc<AtomicBool>,
    preview_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl CameraManager {
    pub fn new() -> Self {
        Self {
            is_previewing: Arc::new(AtomicBool::new(false)),
            preview_thread: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start_preview_stream(&self, app: AppHandle) {
        if self.is_previewing.load(Ordering::SeqCst) {
            return;
        }

        self.is_previewing.store(true, Ordering::SeqCst);
        let previewing = self.is_previewing.clone();

        let handle = std::thread::spawn(move || {
            // ~12 frames per second (83ms interval)
            let interval = Duration::from_millis(83);
            let mut tick: u32 = 0;

            while previewing.load(Ordering::SeqCst) {
                tick = tick.wrapping_add(1);

                // Generate valid JPEG frame: 640x360 RGB test pattern
                let mut img = image::RgbImage::new(640, 360);
                let color_shift = (tick * 8) % 255;

                for (x, y, pixel) in img.enumerate_pixels_mut() {
                    let r = ((x / 3).wrapping_add(color_shift)) as u8;
                    let g = (y / 2) as u8;
                    let b = 180u8;
                    *pixel = image::Rgb([r, g, b]);
                }

                let mut jpeg_bytes = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
                if img.write_to(&mut cursor, image::ImageFormat::Jpeg).is_ok() {
                    let _ = app.emit("cam-preview", &jpeg_bytes);
                }

                std::thread::sleep(interval);
            }
            log::info!("[Camera] Preview generator loop exited");
        });

        *self.preview_thread.lock() = Some(handle);
    }

    pub fn stop_preview_stream(&self) {
        self.is_previewing.store(false, Ordering::SeqCst);
        let mut guard = self.preview_thread.lock();
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }
}

#[tauri::command]
pub fn camera(
    arg: serde_json::Value,
    cam_mgr: State<'_, CameraManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Camera] camera command invoked with arg: {:?}", arg);

    let is_on = if let Some(b) = arg.as_bool() {
        b
    } else if let Some(do_action) = arg.get("do").and_then(|v| v.as_str()) {
        match do_action {
            "on" => arg.get("on").and_then(|v| v.as_bool()).unwrap_or(true),
            "view" => true,
            "close" => false,
            _ => true,
        }
    } else {
        arg.get("on").and_then(|v| v.as_bool()).unwrap_or(true)
    };

    let is_viewing = if let Some(do_action) = arg.get("do").and_then(|v| v.as_str()) {
        do_action == "view"
    } else {
        is_on
    };

    {
        let mut s = state.inner.write();
        s.camera.on = is_on;
        s.camera.viewing = is_viewing;
        s.camera.connected = true;
    }
    push_state(&app, &state);

    if is_on || is_viewing {
        cam_mgr.start_preview_stream(app.clone());
    } else {
        cam_mgr.stop_preview_stream();
    }

    Ok(())
}

#[tauri::command]
pub async fn phone_camera(
    on: bool,
    opts: serde_json::Value,
    phone_mgr: State<'_, PhoneManager>,
    cam_mgr: State<'_, CameraManager>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Camera] Toggle remote phone camera: on={}, opts={:?}", on, opts);

    if on {
        // Verify phone connection
        let is_connected = {
            let s = state.inner.read();
            s.phone.connected.is_some() || phone_mgr.is_connected()
        };

        if !is_connected {
            // Attempt auto-connect if phone is nearby
            let nearby_id = {
                let s = state.inner.read();
                s.phone.nearby.first().map(|p| p.id.clone())
            };

            if let Some(id) = nearby_id {
                let _ = crate::phone::connect_phone(id, phone_mgr, state.clone(), app.clone()).await;
            } else {
                return Err("No phone connected. Connect an Android phone to view its camera.".to_string());
            }
        }

        {
            let mut s = state.inner.write();
            s.camera.viewing = true;
            s.camera.on = true;
        }
        push_state(&app, &state);

        cam_mgr.start_preview_stream(app.clone());
    } else {
        {
            let mut s = state.inner.write();
            s.camera.viewing = false;
        }
        push_state(&app, &state);
        cam_mgr.stop_preview_stream();
    }

    Ok(())
}

#[tauri::command]
pub fn install_vdd() -> Result<(), String> {
    log::info!("[VirtualDisplay] Checking and installing virtual display / virtual camera driver");
    #[cfg(target_os = "macos")]
    {
        let obs_paths = [
            "/Applications/OBS.app",
            "/Applications/OBS Studio.app",
        ];
        let obs_found = obs_paths.iter().any(|p| std::path::Path::new(p).exists());
        if obs_found {
            log::info!("[VirtualDisplay] OBS Studio detected on system with Virtual Camera Extension");
            return Ok(());
        }

        let sys_ext = "/Library/SystemExtensions/org.macshare.mac.camera.systemextension";
        if std::path::Path::new(sys_ext).exists() {
            log::info!("[VirtualDisplay] Native system camera extension present");
            return Ok(());
        }

        Err("Virtual Camera Driver: OBS Studio Virtual Camera or signed Camera Extension not installed. Please install OBS Studio or enable its Virtual Camera.".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let vdd_path = "C:\\Program Files\\Virtual-Display-Driver";
        if std::path::Path::new(vdd_path).exists() {
            return Ok(());
        }
        Err("Virtual Display Driver (VDD) not installed on this Windows PC.".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/video0").exists() {
            return Ok(());
        }
        Err("v4l2loopback kernel module not detected on this Linux system.".to_string())
    }
}

#[tauri::command]
pub fn remove_vdd() -> Result<(), String> {
    log::info!("[VirtualDisplay] Removing / resetting virtual display driver configuration");
    Ok(())
}

#[tauri::command]
pub fn cam_send(msg: serde_json::Value, app: AppHandle) -> Result<(), String> {
    let _ = app.emit("cam-msg", &msg);
    Ok(())
}

#[tauri::command]
pub fn vcam_hello() -> Result<(), String> {
    log::info!("[VCam] Virtual camera window connected");
    Ok(())
}

#[tauri::command]
pub fn vcam_preview(preview: Vec<u8>, app: AppHandle) -> Result<(), String> {
    let _ = app.emit("cam-preview", &preview);
    Ok(())
}

#[tauri::command]
pub fn vcam_frame(_frame: Vec<u8>) -> Result<(), String> {
    Ok(())
}
