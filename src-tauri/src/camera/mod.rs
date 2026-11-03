// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Camera & Virtual Camera Driver Handlers

#[tauri::command]
pub fn camera(on: bool) -> Result<(), String> {
    log::info!("[Camera] Toggle local camera preview: {}", on);
    Ok(())
}

#[tauri::command]
pub fn phone_camera(on: bool, opts: serde_json::Value) -> Result<(), String> {
    log::info!("[Camera] Toggle remote phone camera: on={}, opts={:?}", on, opts);
    Ok(())
}

#[tauri::command]
pub fn install_vdd() -> Result<(), String> {
    log::info!("[VirtualDisplay] Installing virtual display driver");
    Ok(())
}

#[tauri::command]
pub fn remove_vdd() -> Result<(), String> {
    log::info!("[VirtualDisplay] Removing virtual display driver");
    Ok(())
}
