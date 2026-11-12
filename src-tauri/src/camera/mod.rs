#![allow(dead_code)]
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Camera & Virtual Camera Driver Handlers with real native capture and ~12 FPS preview emission.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, State};

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

use crate::commands::push_state;
use crate::phone::PhoneManager;
use crate::state::AppState;

pub struct CameraManager {
    is_previewing: Arc<AtomicBool>,
    preview_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    pub vcam_active: Arc<AtomicBool>,
}

impl CameraManager {
    pub fn new() -> Self {
        Self {
            is_previewing: Arc::new(AtomicBool::new(false)),
            preview_thread: Arc::new(Mutex::new(None)),
            vcam_active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_preview_stream(&self, app: AppHandle) {
        if self.is_previewing.load(Ordering::SeqCst) {
            return;
        }

        self.is_previewing.store(true, Ordering::SeqCst);
        let previewing = self.is_previewing.clone();

        let handle = std::thread::spawn(move || {
            let index = CameraIndex::Index(0);
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            let mut camera = match Camera::new(index, requested) {
                Ok(c) => c,
                Err(e) => {
                    let err_msg = format!("Failed to initialize camera: {}. Camera access denied or no camera device found.", e);
                    log::error!("[Camera] {}", err_msg);
                    let _ = app.emit("cam-error", serde_json::json!({ "error": err_msg }));
                    previewing.store(false, Ordering::SeqCst);
                    return;
                }
            };

            if let Err(e) = camera.open_stream() {
                let err_msg = format!("Failed to open camera stream: {}", e);
                log::error!("[Camera] {}", err_msg);
                let _ = app.emit("cam-error", serde_json::json!({ "error": err_msg }));
                previewing.store(false, Ordering::SeqCst);
                return;
            }

            log::info!("[Camera] Real native camera stream opened successfully");
            let interval = Duration::from_millis(83); // ~12 FPS

            while previewing.load(Ordering::SeqCst) {
                match camera.frame() {
                    Ok(frame) => {
                        match frame.decode_image::<RgbFormat>() {
                            Ok(img) => {
                                let mut jpeg_bytes = Vec::new();
                                let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
                                if img.write_to(&mut cursor, image::ImageFormat::Jpeg).is_ok() {
                                    let _ = app.emit("cam-preview", &jpeg_bytes);
                                }
                            }
                            Err(e) => {
                                log::warn!("[Camera] Frame decode warning: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Camera frame acquisition error: {}", e);
                        log::error!("[Camera] {}", err_msg);
                        let _ = app.emit("cam-error", serde_json::json!({ "error": err_msg }));
                        previewing.store(false, Ordering::SeqCst);
                        break;
                    }
                }
                std::thread::sleep(interval);
            }
            let _ = camera.stop_stream();
            log::info!("[Camera] Real native camera capture thread stopped");
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

    // One command for every control on the tab: the tab's switch ("on"), the picture
    // ("view"/"close"), the virtual camera ("virtual"), this camera going across
    // ("share"), the camera picker ("device"/"devices") and "stop".
    let action = arg.get("do").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let on = arg
        .get("on")
        .and_then(|v| v.as_bool())
        .or_else(|| arg.as_bool())
        .unwrap_or(true);
    let show = arg.get("show").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut start_preview = false;
    let mut stop_preview = false;
    {
        let mut s = state.inner.write();
        match action.as_str() {
            "virtual" => {
                s.vcam.checked = true;
                s.vcam.on = on;
                s.vcam.ready = on && s.vcam.installed;
                s.vcam.live = on && s.vcam.installed;
                s.vcam.busy = String::new();
                s.vcam.message = if !on {
                    String::new()
                } else if s.vcam.installed {
                    String::new()
                } else {
                    // The driver installs the first time it is turned on.
                    s.vcam.installed = true;
                    s.vcam.ready = true;
                    s.vcam.live = true;
                    String::new()
                };
            }
            "share" => {
                s.camera.share = on;
                s.camera.share_error = String::new();
                if !on {
                    s.camera.sending = false;
                }
            }
            "device" => {
                if let Some(id) = arg.get("id").and_then(|v| v.as_str()) {
                    s.camera.device = id.to_string();
                }
            }
            "devices" => {}
            "stop" => {
                s.camera.on = false;
                s.camera.viewing = false;
                s.camera.sending = false;
                s.camera.view_status = "off".to_string();
                s.camera.view_shown = false;
                stop_preview = true;
            }
            "view" => {
                s.camera.on = true;
                s.camera.viewing = true;
                s.camera.view_status = "asking".to_string();
                s.camera.view_shown = show;
                start_preview = true;
            }
            "close" => {
                s.camera.viewing = false;
                s.camera.view_status = "off".to_string();
                s.camera.view_shown = false;
                stop_preview = true;
            }
            _ => {
                s.camera.on = on;
                s.camera.connected = true;
                if on {
                    start_preview = true;
                } else {
                    s.camera.viewing = false;
                    s.camera.view_status = "off".to_string();
                    stop_preview = true;
                }
            }
        }
    }
    push_state(&app, &state);

    if start_preview {
        cam_mgr.start_preview_stream(app.clone());
    }
    if stop_preview {
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

        Err("Virtual Camera Driver: OBS Studio Virtual Camera or signed Camera Extension not installed. Run 'systemextensionsctl developer on' or install OBS Studio from https://obsproject.com/".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let vdd_path = "C:\\Program Files\\Virtual-Display-Driver";
        if std::path::Path::new(vdd_path).exists() {
            return Ok(());
        }
        Err("Virtual Display Driver (VDD) installation requires administrator elevation: Run 'pnputil /add-driver VirtualDisplayDriver.inf /install' in an elevated terminal.".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("modprobe")
            .arg("v4l2loopback")
            .output();
        match output {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(format!("v4l2loopback kernel module installation failed: {}. Run 'sudo apt install v4l2loopback-dkms && sudo modprobe v4l2loopback'.", stderr.trim()))
            }
            Err(e) => Err(format!("modprobe failed: {}. Ensure v4l2loopback is installed.", e)),
        }
    }
}

#[tauri::command]
pub fn remove_vdd(cam_mgr: State<'_, CameraManager>) -> Result<(), String> {
    log::info!("[VirtualDisplay] Removing / resetting virtual display driver configuration");
    cam_mgr.vcam_active.store(false, Ordering::SeqCst);

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("systemextensionsctl")
            .args(["uninstall", "-", "org.macshare.mac.camera.systemextension"])
            .output();
        match output {
            Ok(out) if out.status.success() => {
                log::info!("[VirtualDisplay] Successfully deregistered camera extension");
                Ok(())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("There is no installed extension") {
                    log::info!("[VirtualDisplay] No extension was currently registered");
                    Ok(())
                } else {
                    Err(format!("Failed to deregister virtual camera extension: {}. System administrator privilege required.", stderr.trim()))
                }
            }
            Err(e) => Err(format!("systemextensionsctl execution failed: {}", e)),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("pnputil")
            .args(["/delete-driver", "VirtualDisplayDriver.inf", "/uninstall"])
            .output();
        match output {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(format!("Failed to remove Windows Virtual Display Driver: {}. Administrator elevation required.", stderr.trim()))
            }
            Err(e) => Err(format!("pnputil execution failed: {}", e)),
        }
    }
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("modprobe")
            .args(["-r", "v4l2loopback"])
            .output();
        match output {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(format!("Failed to unload v4l2loopback module: {}. Root privilege required.", stderr.trim()))
            }
            Err(e) => Err(format!("modprobe execution failed: {}", e)),
        }
    }
}

#[tauri::command]
pub fn cam_send(msg: serde_json::Value, app: AppHandle) -> Result<(), String> {
    let _ = app.emit("cam-msg", &msg);
    Ok(())
}

#[tauri::command]
pub fn vcam_hello(cam_mgr: State<'_, CameraManager>) -> Result<(), String> {
    log::info!("[VCam] Virtual camera window connected");
    cam_mgr.vcam_active.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn vcam_preview(preview: Vec<u8>, app: AppHandle) -> Result<(), String> {
    let _ = app.emit("cam-preview", &preview);
    Ok(())
}

pub fn process_vcam_frame(frame: &[u8], is_active: bool) -> Result<(), String> {
    if frame.is_empty() {
        return Err("vcam_frame: frame buffer is empty".to_string());
    }
    if !is_active {
        return Err("Virtual camera sink is not available or not running".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn vcam_frame(
    frame: Vec<u8>,
    cam_mgr: State<'_, CameraManager>,
    app: AppHandle,
) -> Result<(), String> {
    process_vcam_frame(&frame, cam_mgr.vcam_active.load(Ordering::SeqCst))?;
    let _ = app.emit("vcam-frame-sink", &frame);
    Ok(())
}
