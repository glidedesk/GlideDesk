// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Phone ADB Mirroring, scrcpy bridge, and device control handlers

#[tauri::command]
pub async fn connect_phone(id: String) -> Result<(), String> {
    log::info!("[PhoneADB] Connecting to phone device id: {}", id);
    Ok(())
}

#[tauri::command]
pub async fn disconnect_phone() -> Result<(), String> {
    log::info!("[PhoneADB] Disconnecting active phone session");
    Ok(())
}

#[tauri::command]
pub fn phone_screen(show: bool) -> Result<(), String> {
    log::info!("[PhoneADB] Setting phone screen mirroring visibility: {}", show);
    Ok(())
}

#[tauri::command]
pub fn phone_retry() -> Result<(), String> {
    log::info!("[PhoneADB] Retrying phone bridge connection");
    Ok(())
}

#[tauri::command]
pub fn phone_pair(opts: serde_json::Value) -> Result<(), String> {
    log::info!("[PhoneADB] Pairing phone via wireless debugging options: {:?}", opts);
    Ok(())
}

#[tauri::command]
pub fn phone_assets() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "apps": [], "wallpaper": "" }))
}

#[tauri::command]
pub fn set_phone(partial: serde_json::Value) -> Result<(), String> {
    log::info!("[PhoneADB] Setting phone state: {:?}", partial);
    Ok(())
}

#[tauri::command]
pub fn phone_open_app(pkg: String) -> Result<(), String> {
    log::info!("[PhoneADB] Opening phone application: {}", pkg);
    Ok(())
}

#[tauri::command]
pub fn phone_app_menu() -> Result<(), String> {
    log::info!("[PhoneADB] Triggering phone app menu");
    Ok(())
}

#[tauri::command]
pub fn phone_close_view() -> Result<(), String> {
    log::info!("[PhoneADB] Closing phone view");
    Ok(())
}

#[tauri::command]
pub fn phone_install_full() -> Result<(), String> {
    log::info!("[PhoneADB] Installing full phone companion APK via ADB");
    Ok(())
}

#[tauri::command]
pub fn phone_media(action: String) -> Result<(), String> {
    log::info!("[PhoneADB] Phone media action: {}", action);
    Ok(())
}

#[tauri::command]
pub fn phone_bt_pick() -> Result<(), String> {
    log::info!("[PhoneADB] Picking Bluetooth phone connection");
    Ok(())
}

#[tauri::command]
pub fn hub_message(msg: serde_json::Value) -> Result<(), String> {
    log::info!("[Hub] Hub message received: {:?}", msg);
    Ok(())
}
