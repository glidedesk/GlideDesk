// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.

/// Pure predicate determining whether the application should run as an accessory (menu-bar agent)
/// or a regular foreground application on macOS.
///
/// true => Accessory (no Dock icon); false => Regular (Dock icon + Cmd+Tab).
/// Runs on all platforms so the truth table can be exercised in cross-platform unit tests.
pub fn macos_accessory_when_hidden(window_visible: bool) -> bool {
    !window_visible
}

#[cfg(target_os = "macos")]
pub fn set_window_visible_policy(app: &tauri::AppHandle, window_visible: bool) {
    let (policy, policy_name) = if macos_accessory_when_hidden(window_visible) {
        (tauri::ActivationPolicy::Accessory, "Accessory")
    } else {
        (tauri::ActivationPolicy::Regular, "Regular")
    };
    if let Err(e) = app.set_activation_policy(policy) {
        log::warn!("[macOS] failed to set activation policy {}: {}", policy_name, e);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_window_visible_policy(_app: &tauri::AppHandle, _window_visible: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_accessory_when_hidden() {
        // Truth table:
        // When window is hidden (window_visible = false) -> Accessory = true (menu-bar agent, no Dock icon)
        assert!(macos_accessory_when_hidden(false));

        // When window is visible (window_visible = true) -> Accessory = false (Regular app, Dock icon + Cmd+Tab)
        assert!(!macos_accessory_when_hidden(true));
    }
}
