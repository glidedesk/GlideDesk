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

    #[cfg(target_os = "macos")]
    fn apply_native_park(id_str: &str) {
        let (pid_opt, wid) = parse_macos_window_id(id_str);
        if wid == 0 {
            return;
        }
        let pid = pid_opt.or_else(|| lookup_macos_pid_for_window(wid));
        if let Some(pid) = pid {
            reposition_macos_window(pid, wid, -32000.0, -32000.0);
        }
    }

    #[cfg(target_os = "linux")]
    fn apply_native_park(id_str: &str) {
        if let Ok(win) = u32::from_str_radix(id_str.trim_start_matches("0x"), 16) {
            let display = std::env::var("DISPLAY").ok();
            if display.as_deref().map_or(true, |d| d.trim().is_empty()) {
                return;
            }
            if let Ok((conn, _)) = x11rb::connect(display.as_deref()) {
                use x11rb::connection::Connection as _;
                use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};
                let aux = ConfigureWindowAux::new().x(-32000).y(-32000);
                let _ = conn.configure_window(win, &aux);
                let _ = conn.flush();
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    fn apply_native_park(_id_str: &str) {}

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

    #[cfg(target_os = "macos")]
    fn apply_native_restore(id_str: &str, x: i32, y: i32, _w: u32, _h: u32) {
        let (pid_opt, wid) = parse_macos_window_id(id_str);
        if wid == 0 {
            return;
        }
        let pid = pid_opt.or_else(|| lookup_macos_pid_for_window(wid));
        if let Some(pid) = pid {
            reposition_macos_window(pid, wid, x as f64, y as f64);
        }
    }

    #[cfg(target_os = "linux")]
    fn apply_native_restore(id_str: &str, x: i32, y: i32, w: u32, h: u32) {
        if let Ok(win) = u32::from_str_radix(id_str.trim_start_matches("0x"), 16) {
            let display = std::env::var("DISPLAY").ok();
            if display.as_deref().map_or(true, |d| d.trim().is_empty()) {
                return;
            }
            if let Ok((conn, _)) = x11rb::connect(display.as_deref()) {
                use x11rb::connection::Connection as _;
                use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};
                let aux = ConfigureWindowAux::new().x(x).y(y).width(w).height(h);
                let _ = conn.configure_window(win, &aux);
                let _ = conn.map_window(win);
                let _ = conn.flush();
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    fn apply_native_restore(_id_str: &str, _x: i32, _y: i32, _w: u32, _h: u32) {}
}

#[cfg(windows)]
pub fn find_window_at_point(x: f64, y: f64) -> Option<(String, String, i32, i32, u32, u32)> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetWindowRect, GetWindowTextW, WindowFromPoint, GA_ROOT,
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

#[cfg(target_os = "macos")]
mod macos_window {
    use std::ffi::c_void;

    pub type AXUIElementRef = *mut c_void;
    pub type AXValueRef = *mut c_void;
    pub type CFTypeRef = *const c_void;
    pub type CFStringRef = *const c_void;
    pub type CFArrayRef = *const c_void;
    pub type CFIndex = isize;
    pub type CFDictionaryRef = *const c_void;
    pub type CFNumberRef = *const c_void;

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFArrayRef;
        pub fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
        pub fn CFArrayGetValueAtIndex(array: CFArrayRef, index: CFIndex) -> *const c_void;
        pub fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
        pub fn CFStringCreateWithCString(alloc: *const c_void, c_str: *const u8, encoding: u32) -> CFStringRef;
        pub fn CFStringGetCString(cf_str: CFStringRef, buffer: *mut u8, buf_size: isize, encoding: u32) -> bool;
        pub fn CFNumberGetValue(number: CFNumberRef, the_type: CFIndex, value_ptr: *mut c_void) -> bool;
        pub fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect) -> bool;
        pub fn CFRelease(cf: *const c_void);

        pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
        pub fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
        pub fn AXValueCreate(the_type: u32, value_ptr: *const c_void) -> AXValueRef;
        pub fn _AXUIElementGetWindow(element: AXUIElementRef, wid: *mut u32) -> i32;
    }
}

#[cfg(target_os = "macos")]
fn parse_macos_window_id(id_str: &str) -> (Option<i32>, u32) {
    if let Some((pid_str, wid_str)) = id_str.split_once(':') {
        let pid = pid_str.parse::<i32>().ok();
        let wid = u32::from_str_radix(wid_str.trim_start_matches("0x"), 16).unwrap_or(0);
        (pid, wid)
    } else {
        let wid = u32::from_str_radix(id_str.trim_start_matches("0x"), 16).unwrap_or(0);
        (None, wid)
    }
}

#[cfg(target_os = "macos")]
fn lookup_macos_pid_for_window(target_wid: u32) -> Option<i32> {
    use macos_window::*;
    unsafe {
        let list = CGWindowListCopyWindowInfo(1 | 16, 0);
        if list.is_null() {
            return None;
        }
        let count = CFArrayGetCount(list);
        let k_num = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowNumber\0".as_ptr(), 0x08000100);
        let k_pid = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowOwnerPID\0".as_ptr(), 0x08000100);

        let mut matched_pid = None;
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            let mut wid: u32 = 0;
            let wid_ref = CFDictionaryGetValue(dict, k_num as _);
            if !wid_ref.is_null() {
                CFNumberGetValue(wid_ref, 3, &mut wid as *mut _ as *mut _);
            }
            if wid == target_wid {
                let mut pid: i32 = 0;
                let pid_ref = CFDictionaryGetValue(dict, k_pid as _);
                if !pid_ref.is_null() {
                    CFNumberGetValue(pid_ref, 3, &mut pid as *mut _ as *mut _);
                    matched_pid = Some(pid);
                }
                break;
            }
        }
        CFRelease(k_pid as _);
        CFRelease(k_num as _);
        CFRelease(list as _);
        matched_pid
    }
}

#[cfg(target_os = "macos")]
fn reposition_macos_window(pid: i32, target_wid: u32, x: f64, y: f64) {
    use macos_window::*;
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        let mut windows_ref: CFTypeRef = std::ptr::null();
        let k_windows = CFStringCreateWithCString(std::ptr::null(), b"AXWindows\0".as_ptr(), 0x08000100);
        if AXUIElementCopyAttributeValue(app, k_windows, &mut windows_ref) == 0 && !windows_ref.is_null() {
            let win_count = CFArrayGetCount(windows_ref as _);
            for j in 0..win_count {
                let win_el = CFArrayGetValueAtIndex(windows_ref as _, j) as AXUIElementRef;
                let mut cur_wid: u32 = 0;
                if _AXUIElementGetWindow(win_el, &mut cur_wid) == 0 && cur_wid == target_wid {
                    let pt = CGPoint { x, y };
                    let val = AXValueCreate(1, &pt as *const _ as *const _); // 1 = kAXValueCGPointType
                    let k_pos = CFStringCreateWithCString(std::ptr::null(), b"AXPosition\0".as_ptr(), 0x08000100);
                    AXUIElementSetAttributeValue(win_el, k_pos, val as _);
                    CFRelease(k_pos as _);
                    CFRelease(val as _);
                    break;
                }
            }
            CFRelease(windows_ref);
        }
        CFRelease(k_windows as _);
        CFRelease(app as _);
    }
}

#[cfg(target_os = "macos")]
pub fn find_window_at_point(x: f64, y: f64) -> Option<(String, String, i32, i32, u32, u32)> {
    use macos_window::*;
    unsafe {
        let list = CGWindowListCopyWindowInfo(1 | 16, 0); // OptionOnScreenOnly | ExcludeDesktopElements
        if list.is_null() {
            return None;
        }
        let count = CFArrayGetCount(list);
        let k_name = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowName\0".as_ptr(), 0x08000100);
        let k_owner = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowOwnerName\0".as_ptr(), 0x08000100);
        let k_num = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowNumber\0".as_ptr(), 0x08000100);
        let k_pid = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowOwnerPID\0".as_ptr(), 0x08000100);
        let k_layer = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowLayer\0".as_ptr(), 0x08000100);
        let k_bounds = CFStringCreateWithCString(std::ptr::null(), b"kCGWindowBounds\0".as_ptr(), 0x08000100);

        let mut hit = None;
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            let mut layer: i32 = 0;
            let layer_ref = CFDictionaryGetValue(dict, k_layer as _);
            if !layer_ref.is_null() {
                CFNumberGetValue(layer_ref, 3, &mut layer as *mut _ as *mut _);
            }
            if layer != 0 {
                continue;
            }

            let mut rect = CGRect::default();
            let bounds_ref = CFDictionaryGetValue(dict, k_bounds as _);
            if !bounds_ref.is_null() {
                CGRectMakeWithDictionaryRepresentation(bounds_ref as _, &mut rect);
            }

            let min_x = rect.origin.x;
            let min_y = rect.origin.y;
            let max_x = rect.origin.x + rect.size.width;
            let max_y = rect.origin.y + rect.size.height;

            if x >= min_x && x < max_x && y >= min_y && y < max_y {
                let mut wid: u32 = 0;
                let wid_ref = CFDictionaryGetValue(dict, k_num as _);
                if !wid_ref.is_null() {
                    CFNumberGetValue(wid_ref, 3, &mut wid as *mut _ as *mut _);
                }

                let mut pid: i32 = 0;
                let pid_ref = CFDictionaryGetValue(dict, k_pid as _);
                if !pid_ref.is_null() {
                    CFNumberGetValue(pid_ref, 3, &mut pid as *mut _ as *mut _);
                }

                let mut title_buf = [0u8; 256];
                let mut got_title = false;
                let name_ref = CFDictionaryGetValue(dict, k_name as _);
                if !name_ref.is_null() && CFStringGetCString(name_ref as _, title_buf.as_mut_ptr(), 256, 0x08000100) {
                    got_title = true;
                }
                if !got_title || title_buf[0] == 0 {
                    let owner_ref = CFDictionaryGetValue(dict, k_owner as _);
                    if !owner_ref.is_null() {
                        let _ = CFStringGetCString(owner_ref as _, title_buf.as_mut_ptr(), 256, 0x08000100);
                    }
                }

                let title = String::from_utf8_lossy(&title_buf)
                    .trim_matches(char::from(0))
                    .to_string();

                let id = format!("{}:0x{:x}", pid, wid);
                hit = Some((
                    id,
                    title,
                    rect.origin.x as i32,
                    rect.origin.y as i32,
                    rect.size.width as u32,
                    rect.size.height as u32,
                ));
                break;
            }
        }

        CFRelease(k_bounds as _);
        CFRelease(k_layer as _);
        CFRelease(k_pid as _);
        CFRelease(k_num as _);
        CFRelease(k_owner as _);
        CFRelease(k_name as _);
        CFRelease(list as _);

        hit
    }
}

#[cfg(target_os = "linux")]
pub fn find_window_at_point(x: f64, y: f64) -> Option<(String, String, i32, i32, u32, u32)> {
    let display = std::env::var("DISPLAY").ok()?;
    if display.trim().is_empty() {
        return None;
    }
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, MapState};
    let (conn, screen_num) = x11rb::connect(Some(&display)).ok()?;
    let setup = conn.setup();
    let screen = setup.roots.get(screen_num)?;
    let root = screen.root;

    let tree = conn.query_tree(root).ok()?.reply().ok()?;
    for &child in tree.children.iter().rev() {
        if let Ok(attr) = conn.get_window_attributes(child).ok()?.reply() {
            if attr.map_state != MapState::VIEWABLE || attr.override_redirect {
                continue;
            }
            if let Ok(geom) = conn.get_geometry(child).ok()?.reply() {
                let gx = geom.x as i32;
                let gy = geom.y as i32;
                let gw = geom.width as u32;
                let gh = geom.height as u32;
                let px = x as i32;
                let py = y as i32;
                if px >= gx && px < gx + gw as i32 && py >= gy && py < gy + gh as i32 {
                    let title = conn
                        .get_property(false, child, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .and_then(|r| String::from_utf8(r.value).ok())
                        .unwrap_or_else(|| "Window".to_string());

                    return Some((format!("0x{:x}", child), title, gx, gy, gw, gh));
                }
            }
        }
    }
    None
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
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

    #[test]
    fn test_find_window_at_point_does_not_panic() {
        let _ = find_window_at_point(100.0, 100.0);
        let _ = find_window_at_point(-99999.0, -99999.0);
    }
}
