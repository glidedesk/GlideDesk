// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Remaining TwinDesk Parity Stubs (Apps, Drive/Mount, Notifications, Terminal, Network, Misc)

#[tauri::command]
pub fn cancel_drop() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn check_firewall() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn choose_received_folder() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn choose_share_folder() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn clear_notif_history() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn launch_app(name: String) -> Result<(), String> {
    log::info!("[Apps] Launching app: {}", name);
    Ok(())
}

#[tauri::command]
pub fn list_apps() -> Result<Vec<String>, String> {
    Ok(vec![])
}

#[tauri::command]
pub fn mute_notif_app(app: String) -> Result<(), String> {
    log::info!("[Notifs] Muting app notifications: {}", app);
    Ok(())
}

#[tauri::command]
pub fn open_bluetooth_settings() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn open_mac_drive() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn open_monitor() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn open_notified_app(id: String) -> Result<(), String> {
    log::info!("[Notifs] Opening notified app: {}", id);
    Ok(())
}

#[tauri::command]
pub fn open_received_folder() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn open_terminal() -> Result<(), String> {
    log::info!("[Terminal] Opening terminal shell");
    Ok(())
}

#[tauri::command]
pub fn pin_app(name: String) -> Result<(), String> {
    log::info!("[Apps] Pinning app: {}", name);
    Ok(())
}

#[tauri::command]
pub fn remote_screen() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn reset_received_folder() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn set_mac_apps(_partial: serde_json::Value) -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn set_mount(_partial: serde_json::Value) -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn set_pc_screen(mode: String) -> Result<(), String> {
    log::info!("[Screen] Setting PC screen mode: {}", mode);
    Ok(())
}

#[tauri::command]
pub fn set_phone_remote(_on: bool) -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn set_startup(_on: bool) -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn set_wake(_on: bool) -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn setup_sshd() -> Result<(), String> {
    log::info!("[Terminal] Setting up SSH daemon");
    Ok(())
}

#[tauri::command]
pub fn phone_mount() -> Result<(), String> { Ok(()) }

#[tauri::command]
pub fn phone_unmount() -> Result<(), String> { Ok(()) }

#[cfg(test)]
mod tests;
