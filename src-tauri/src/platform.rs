// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.

use tauri::Manager;

/// Pure predicate determining whether the application runs in background mode
/// (Accessory menu-bar agent on macOS, taskbar-excluded tray-only on Windows/Linux).
///
/// true => background mode (hidden window, no Dock or Taskbar entry);
/// false => regular foreground mode (visible window, standard Dock/Taskbar entry).
/// Runs on all platforms so the truth table can be exercised in cross-platform unit tests.
pub fn background_when_hidden(window_visible: bool) -> bool {
    !window_visible
}

/// Backward-compatible alias for the macOS accessory mapping.
pub fn macos_accessory_when_hidden(window_visible: bool) -> bool {
    background_when_hidden(window_visible)
}

/// Applies background / tray presentation policy across platforms:
/// - macOS: dynamically sets ActivationPolicy::Accessory when hidden (menu-bar agent),
///   and ActivationPolicy::Regular when visible (Dock icon + Cmd+Tab).
/// - Windows & Linux: hints the window manager to skip the taskbar when hidden (`set_skip_taskbar(true)`),
///   and re-enables taskbar display when restored (`set_skip_taskbar(false)`).
/// - Fallback: logs any window hint failure without crashing.
pub fn set_window_visible_policy(app: &tauri::AppHandle, window_visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let (policy, policy_name) = if background_when_hidden(window_visible) {
            (tauri::ActivationPolicy::Accessory, "Accessory")
        } else {
            (tauri::ActivationPolicy::Regular, "Regular")
        };
        if let Err(e) = app.set_activation_policy(policy) {
            log::warn!("[macOS] failed to set activation policy {}: {}", policy_name, e);
        }
    }

    // Windows, Linux, and desktop window managers: exclude from taskbar / Alt+Tab while backgrounded
    if let Some(win) = app.get_webview_window("main") {
        let skip = background_when_hidden(window_visible);
        if let Err(e) = win.set_skip_taskbar(skip) {
            log::warn!("[Window] failed to set skip_taskbar({}): {}", skip, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_when_hidden() {
        // Truth table:
        // When window is hidden (window_visible = false) -> background mode = true (Accessory / skip taskbar)
        assert!(background_when_hidden(false));
        assert!(macos_accessory_when_hidden(false));

        // When window is visible (window_visible = true) -> background mode = false (Regular app / show taskbar)
        assert!(!background_when_hidden(true));
        assert!(!macos_accessory_when_hidden(true));
    }
}
