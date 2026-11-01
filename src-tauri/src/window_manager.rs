// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Window Edge-Handoff: Single-window cross-computer parking, projection, and recovery engine.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowParkState {
    pub id: String,
    pub title: String,
    pub original_x: i32,
    pub original_y: i32,
    pub width: u32,
    pub height: u32,
    pub peer_id: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub parked_at: Instant,
}

#[derive(Clone)]
pub struct ParkedWindowRegistry {
    parked: Arc<RwLock<HashMap<String, WindowParkState>>>,
}

impl Default for ParkedWindowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ParkedWindowRegistry {
    pub fn new() -> Self {
        Self {
            parked: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn park(&self, id: String, title: String, x: i32, y: i32, w: u32, h: u32, peer_id: Option<String>) -> bool {
        let state = WindowParkState {
            id: id.clone(),
            title,
            original_x: x,
            original_y: y,
            width: w,
            height: h,
            peer_id,
            parked_at: Instant::now(),
        };

        // Native OS window reposition to off-screen coordinates
        Self::apply_native_park(&id);

        let mut guard = self.parked.write();
        guard.insert(id, state);
        true
    }

    pub fn unpark(&self, id: &str) -> Option<WindowParkState> {
        let state = {
            let mut guard = self.parked.write();
            guard.remove(id)
        };

        if let Some(ref st) = state {
            Self::apply_native_restore(&st.id, st.original_x, st.original_y, st.width, st.height);
        }
        state
    }

    pub fn restore_all(&self) -> Vec<WindowParkState> {
        let states: Vec<WindowParkState> = {
            let mut guard = self.parked.write();
            guard.drain().map(|(_, v)| v).collect()
        };

        for st in &states {
            Self::apply_native_restore(&st.id, st.original_x, st.original_y, st.width, st.height);
        }
        states
    }

    pub fn count(&self) -> usize {
        self.parked.read().len()
    }

    pub fn list(&self) -> Vec<WindowParkState> {
        self.parked.read().values().cloned().collect()
    }

    #[cfg(windows)]
    fn apply_native_park(id_str: &str) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOSIZE};

        if let Ok(hwnd_val) = usize::from_str_radix(id_str.trim_start_matches("0x"), 16) {
            let hwnd = HWND(hwnd_val as *mut _);
            unsafe {
                let _ = SetWindowPos(hwnd, HWND_TOP, -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    #[cfg(not(windows))]
    fn apply_native_park(_id_str: &str) {
        // macOS / Linux native off-screen reposition stub
    }

    #[cfg(windows)]
    fn apply_native_restore(id_str: &str, x: i32, y: i32, w: u32, h: u32) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOP, SWP_SHOWWINDOW};

        if let Ok(hwnd_val) = usize::from_str_radix(id_str.trim_start_matches("0x"), 16) {
            let hwnd = HWND(hwnd_val as *mut _);
            unsafe {
                let _ = SetWindowPos(hwnd, HWND_TOP, x, y, w as i32, h as i32, SWP_SHOWWINDOW);
            }
        }
    }

    #[cfg(not(windows))]
    fn apply_native_restore(_id_str: &str, _x: i32, _y: i32, _w: u32, _h: u32) {
        // macOS / Linux native restore stub
    }
}

#[cfg(windows)]
pub fn find_window_at_point(x: f64, y: f64) -> Option<(String, String, i32, i32, u32, u32)> {
    #[allow(unused_imports)]
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        WindowFromPoint, GetAncestor, GA_ROOT, GetWindowTextW, GetWindowRect
    };

    unsafe {
        let pt = POINT { x: x as i32, y: y as i32 };
        let mut hwnd = WindowFromPoint(pt);
        if hwnd.0.is_null() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        if !root.0.is_null() {
            hwnd = root;
        }

        let mut title_buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..len as usize]);

        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let width = (rect.right - rect.left).max(100) as u32;
        let height = (rect.bottom - rect.top).max(100) as u32;

        let id = format!("{:p}", hwnd.0);
        Some((id, title, rect.left, rect.top, width, height))
    }
}

#[cfg(not(windows))]
pub fn find_window_at_point(_x: f64, _y: f64) -> Option<(String, String, i32, i32, u32, u32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parked_window_registry_lifecycle() {
        let registry = ParkedWindowRegistry::new();
        assert_eq!(registry.count(), 0);

        // 1. Park a window
        let ok = registry.park(
            "0x1234".to_string(),
            "Visual Studio Code".to_string(),
            200,
            150,
            1024,
            768,
            Some("peer-mac".to_string()),
        );
        assert!(ok);
        assert_eq!(registry.count(), 1);

        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Visual Studio Code");
        assert_eq!(list[0].original_x, 200);
        assert_eq!(list[0].original_y, 150);

        // 2. Unpark the window
        let unparked = registry.unpark("0x1234");
        assert!(unparked.is_some());
        assert_eq!(unparked.unwrap().id, "0x1234");
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_restore_all_watchdog() {
        let registry = ParkedWindowRegistry::new();
        registry.park("0x1".to_string(), "Terminal".to_string(), 50, 50, 800, 600, None);
        registry.park("0x2".to_string(), "Browser".to_string(), 100, 100, 1200, 800, None);
        assert_eq!(registry.count(), 2);

        let restored = registry.restore_all();
        assert_eq!(restored.len(), 2);
        assert_eq!(registry.count(), 0);
    }
}
