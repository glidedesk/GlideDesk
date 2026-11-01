// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Tauri v2 + Rust: Phase 3 Native KVM & Helper Consolidation
// Eliminates external child-process helpers with native OS hooks, motion coalescing, and clipboard sync.

use crate::network::{NetworkManager, WireMessage};
use crate::state::AppState;
use arboard::Clipboard;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
#[allow(unused_imports)]
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum KvmEvent {
    MouseMove { dx: i32, dy: i32 },
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    MouseWheel { delta_x: i32, delta_y: i32 },
    KeyDown { vk: u32 },
    KeyUp { vk: u32 },
}

/// Bounded queue for KVM events with motion coalescing.
#[allow(dead_code)]
pub struct KvmQueue {
    sender: mpsc::Sender<KvmEvent>,
}

#[allow(dead_code)]
impl KvmQueue {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<KvmEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { sender: tx }, rx)
    }

    pub async fn push(&self, event: KvmEvent) -> Result<(), String> {
        self.sender
            .send(event)
            .await
            .map_err(|e| format!("KVM queue closed: {}", e))
    }
}

#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals, non_snake_case)]
pub mod macos_ffi {
    use std::ffi::c_void;

    pub type CGEventRef = *mut c_void;
    pub type CGEventSourceRef = *mut c_void;
    pub type CFMachPortRef = *mut c_void;
    pub type CFRunLoopRef = *mut c_void;
    pub type CFRunLoopSourceRef = *mut c_void;
    pub type CFAllocatorRef = *const c_void;
    pub type CFStringRef = *const c_void;
    pub type CFDictionaryRef = *mut c_void;
    pub type CGKeyCode = u16;
    pub type CGEventTapLocation = u32;
    pub type CGEventTapPlacement = u32;
    pub type CGEventTapOptions = u32;
    pub type CGEventMask = u64;
    pub type CGEventType = u32;
    pub type CGMouseButton = u32;
    pub type CGScrollEventUnit = u32;
    pub type CGError = i32;
    pub type CFIndex = isize;
    pub type Boolean = u8;

    pub const kCGHIDEventTap: CGEventTapLocation = 0;
    pub const kCGSessionEventTap: CGEventTapLocation = 1;

    pub const kCGHeadInsertEventTap: CGEventTapPlacement = 0;

    pub const kCGEventTapOptionDefault: CGEventTapOptions = 0;

    pub const kCGEventLeftMouseDown: CGEventType = 1;
    pub const kCGEventLeftMouseUp: CGEventType = 2;
    pub const kCGEventRightMouseDown: CGEventType = 3;
    pub const kCGEventRightMouseUp: CGEventType = 4;
    pub const kCGEventMouseMoved: CGEventType = 5;
    pub const kCGEventLeftMouseDragged: CGEventType = 6;
    pub const kCGEventRightMouseDragged: CGEventType = 7;
    pub const kCGEventKeyDown: CGEventType = 10;
    pub const kCGEventKeyUp: CGEventType = 11;
    pub const kCGEventFlagsChanged: CGEventType = 12;
    pub const kCGEventScrollWheel: CGEventType = 22;
    pub const kCGEventOtherMouseDown: CGEventType = 25;
    pub const kCGEventOtherMouseUp: CGEventType = 26;
    pub const kCGEventOtherMouseDragged: CGEventType = 27;

    pub const kCGEventTapDisabledByTimeout: CGEventType = 0xFFFFFFFE;
    pub const kCGEventTapDisabledByUserInput: CGEventType = 0xFFFFFFFF;

    pub const kCGMouseButtonLeft: CGMouseButton = 0;
    pub const kCGMouseButtonRight: CGMouseButton = 1;
    pub const kCGMouseButtonCenter: CGMouseButton = 2;

    pub const kCGScrollEventUnitLine: CGScrollEventUnit = 1;

    pub const kCGKeyboardEventKeycode: u32 = 9;
    pub const kCGScrollWheelEventDeltaAxis1: u32 = 11;
    pub const kCGScrollWheelEventDeltaAxis2: u32 = 12;

    pub const kCGEventFlagMaskAlphaShift: u64 = 0x00010000;
    pub const kCGEventFlagMaskShift: u64 = 0x00020000;
    pub const kCGEventFlagMaskControl: u64 = 0x00040000;
    pub const kCGEventFlagMaskAlternate: u64 = 0x00080000;
    pub const kCGEventFlagMaskCommand: u64 = 0x00100000;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    pub type CGEventTapCallBack = unsafe extern "C" fn(
        proxy: *mut c_void,
        r#type: CGEventType,
        event: CGEventRef,
        refcon: *mut c_void,
    ) -> CGEventRef;

    #[link(name = "CoreGraphics", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn CGEventTapCreate(
            tap: CGEventTapLocation,
            place: CGEventTapPlacement,
            options: CGEventTapOptions,
            events_of_interest: CGEventMask,
            callback: CGEventTapCallBack,
            refcon: *mut c_void,
        ) -> CFMachPortRef;

        pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

        pub fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
        pub fn CGEventCreateMouseEvent(
            source: CGEventSourceRef,
            mouse_type: CGEventType,
            mouse_cursor_position: CGPoint,
            mouse_button: CGMouseButton,
        ) -> CGEventRef;
        pub fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        pub fn CGEventCreateScrollWheelEvent2(
            source: CGEventSourceRef,
            units: CGScrollEventUnit,
            wheel_count: u32,
            wheel1: i32,
            wheel2: i32,
            wheel3: i32,
        ) -> CGEventRef;

        pub fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
        pub fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        pub fn CGEventGetFlags(event: CGEventRef) -> u64;
        pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        pub fn CGWarpMouseCursorPosition(new_cursor_position: CGPoint) -> CGError;

        pub fn CGGetActiveDisplayList(
            max_displays: u32,
            active_displays: *mut u32,
            display_count: *mut u32,
        ) -> CGError;
        pub fn CGDisplayBounds(display: u32) -> CGRect;

        pub fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: CFIndex,
        ) -> CFRunLoopSourceRef;
        pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        pub fn CFRunLoopAddSource(
            rl: CFRunLoopRef,
            source: CFRunLoopSourceRef,
            mode: CFStringRef,
        );
        pub fn CFRunLoopRun();
        pub fn CFRelease(cf: *const c_void);

        pub static kCFRunLoopCommonModes: CFStringRef;

        pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
        pub static kAXTrustedCheckOptionPrompt: *const c_void;

        pub fn CFDictionaryCreate(
            allocator: CFAllocatorRef,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
        pub static kCFBooleanTrue: *const c_void;
        pub static kCFTypeDictionaryKeyCallBacks: c_void;
        pub static kCFTypeDictionaryValueCallBacks: c_void;
    }
}

#[cfg(target_os = "macos")]
pub mod keycodes {
    // Bidirectional mapping between macOS CGKeyCode and Windows Virtual-Key (VK) codes
    const KEY_MAP: &[(u16, u32)] = &[
        (0, 0x41),   // A
        (1, 0x53),   // S
        (2, 0x44),   // D
        (3, 0x46),   // F
        (4, 0x48),   // H
        (5, 0x47),   // G
        (6, 0x5A),   // Z
        (7, 0x58),   // X
        (8, 0x43),   // C
        (9, 0x56),   // V
        (11, 0x42),  // B
        (12, 0x51),  // Q
        (13, 0x57),  // W
        (14, 0x45),  // E
        (15, 0x52),  // R
        (16, 0x59),  // Y
        (17, 0x54),  // T
        (18, 0x31),  // 1
        (19, 0x32),  // 2
        (20, 0x33),  // 3
        (21, 0x34),  // 4
        (22, 0x36),  // 6
        (23, 0x35),  // 5
        (24, 0xBB),  // = (VK_OEM_PLUS)
        (25, 0x39),  // 9
        (26, 0x37),  // 7
        (27, 0xBD),  // - (VK_OEM_MINUS)
        (28, 0x38),  // 8
        (29, 0x30),  // 0
        (30, 0xDD),  // ] (VK_OEM_6)
        (31, 0x4F),  // O
        (32, 0x55),  // U
        (33, 0xDB),  // [ (VK_OEM_4)
        (34, 0x49),  // I
        (35, 0x50),  // P
        (36, 0x0D),  // Return (VK_RETURN)
        (37, 0x4C),  // L
        (38, 0x4A),  // J
        (39, 0xDE),  // ' (VK_OEM_7)
        (40, 0x4B),  // K
        (41, 0xBA),  // ; (VK_OEM_1)
        (42, 0xDC),  // \ (VK_OEM_5)
        (43, 0xBC),  // , (VK_OEM_COMMA)
        (44, 0xBF),  // / (VK_OEM_2)
        (45, 0x4E),  // N
        (46, 0x4D),  // M
        (47, 0xBE),  // . (VK_OEM_PERIOD)
        (48, 0x09),  // Tab (VK_TAB)
        (49, 0x20),  // Space (VK_SPACE)
        (50, 0xC0),  // ` (VK_OEM_3)
        (51, 0x08),  // Backspace (VK_BACK)
        (53, 0x1B),  // Esc (VK_ESCAPE)
        (54, 0x5C),  // Right Command (VK_RWIN)
        (55, 0x5B),  // Left Command (VK_LWIN)
        (56, 0xA0),  // Left Shift (VK_LSHIFT)
        (57, 0x14),  // CapsLock (VK_CAPITAL)
        (58, 0xA4),  // Left Alt (VK_LMENU)
        (59, 0xA2),  // Left Ctrl (VK_LCONTROL)
        (60, 0xA1),  // Right Shift (VK_RSHIFT)
        (61, 0xA5),  // Right Alt (VK_RMENU)
        (62, 0xA3),  // Right Ctrl (VK_RCONTROL)
        (115, 0x24), // Home (VK_HOME)
        (116, 0x21), // PageUp (VK_PRIOR)
        (117, 0x2E), // Forward Delete (VK_DELETE)
        (119, 0x23), // End (VK_END)
        (121, 0x22), // PageDown (VK_NEXT)
        (122, 0x70), // F1 (VK_F1)
        (120, 0x71), // F2 (VK_F2)
        (99, 0x72),  // F3 (VK_F3)
        (118, 0x73), // F4 (VK_F4)
        (96, 0x74),  // F5 (VK_F5)
        (97, 0x75),  // F6 (VK_F6)
        (98, 0x76),  // F7 (VK_F7)
        (100, 0x77), // F8 (VK_F8)
        (101, 0x78), // F9 (VK_F9)
        (109, 0x79), // F10 (VK_F10)
        (103, 0x7A), // F11 (VK_F11)
        (111, 0x7B), // F12 (VK_F12)
        (123, 0x25), // Left Arrow (VK_LEFT)
        (124, 0x27), // Right Arrow (VK_RIGHT)
        (125, 0x28), // Down Arrow (VK_DOWN)
        (126, 0x26), // Up Arrow (VK_UP)
    ];

    pub fn windows_vk_to_macos_keycode(vk: u16) -> Option<u16> {
        let v = vk as u32;
        // Generic modifiers mapping
        if v == 0x10 { return Some(56); } // VK_SHIFT -> Left Shift
        if v == 0x11 { return Some(59); } // VK_CONTROL -> Left Ctrl
        if v == 0x12 { return Some(58); } // VK_MENU -> Left Alt

        for &(mac, win) in KEY_MAP {
            if win == v {
                return Some(mac);
            }
        }
        None
    }

    pub fn macos_keycode_to_windows_vk(mac: u16) -> u32 {
        for &(m, win) in KEY_MAP {
            if m == mac {
                return win;
            }
        }
        mac as u32
    }
}

/// Native platform input injection implementation with pre-login and lock screen desktop synchronization.
pub struct NativeInputInjector;

impl NativeInputInjector {
    #[cfg(windows)]
    pub fn sync_input_desktop() {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::System::StationsAndDesktops::{
            CloseDesktop, GetThreadDesktop, OpenInputDesktop, SetThreadDesktop,
            DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS,
        };

        unsafe {
            // DESKTOP_READOBJECTS (0x0001) | DESKTOP_WRITEOBJECTS (0x0080) | DESKTOP_SWITCHDESKTOP (0x0100)
            let access = DESKTOP_ACCESS_FLAGS(0x0001 | 0x0080 | 0x0100);
            if let Ok(input_desktop) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, access) {
                let cur_thread = GetCurrentThreadId();
                if let Ok(current_desktop) = GetThreadDesktop(cur_thread) {
                    if current_desktop != input_desktop {
                        let _ = SetThreadDesktop(input_desktop);
                    }
                }
                let _ = CloseDesktop(input_desktop);
            }
        }
    }

    #[allow(dead_code)]
    #[cfg(not(windows))]
    pub fn sync_input_desktop() {}

    #[cfg(windows)]
    pub fn move_cursor_relative(dx: i32, dy: i32) {
        Self::sync_input_desktop();
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT,
        };

        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        unsafe {
            let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn move_cursor_relative(dx: i32, dy: i32) {
        use macos_ffi::*;
        unsafe {
            let dummy = CGEventCreate(std::ptr::null_mut());
            if !dummy.is_null() {
                let cur = CGEventGetLocation(dummy);
                CFRelease(dummy as _);
                let target = CGPoint {
                    x: cur.x + dx as f64,
                    y: cur.y + dy as f64,
                };
                let _ = CGWarpMouseCursorPosition(target);
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null_mut(),
                    kCGEventMouseMoved,
                    target,
                    kCGMouseButtonLeft,
                );
                if !ev.is_null() {
                    CGEventPost(kCGHIDEventTap, ev);
                    CFRelease(ev as _);
                }
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn move_cursor_relative(_dx: i32, _dy: i32) {
        // Platform specific implementation for Linux (evdev)
    }

    #[cfg(windows)]
    pub fn mouse_button(button: MouseButton, is_down: bool) {
        Self::sync_input_desktop();
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
            MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
        };

        let flags = match (button, is_down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };

        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        unsafe {
            let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn mouse_button(button: MouseButton, is_down: bool) {
        use macos_ffi::*;
        unsafe {
            let dummy = CGEventCreate(std::ptr::null_mut());
            let cur = if !dummy.is_null() {
                let loc = CGEventGetLocation(dummy);
                CFRelease(dummy as _);
                loc
            } else {
                CGPoint { x: 0.0, y: 0.0 }
            };

            let (ev_type, mouse_btn) = match (button, is_down) {
                (MouseButton::Left, true) => (kCGEventLeftMouseDown, kCGMouseButtonLeft),
                (MouseButton::Left, false) => (kCGEventLeftMouseUp, kCGMouseButtonLeft),
                (MouseButton::Right, true) => (kCGEventRightMouseDown, kCGMouseButtonRight),
                (MouseButton::Right, false) => (kCGEventRightMouseUp, kCGMouseButtonRight),
                (MouseButton::Middle, true) => (kCGEventOtherMouseDown, kCGMouseButtonCenter),
                (MouseButton::Middle, false) => (kCGEventOtherMouseUp, kCGMouseButtonCenter),
            };

            let ev = CGEventCreateMouseEvent(std::ptr::null_mut(), ev_type, cur, mouse_btn);
            if !ev.is_null() {
                CGEventPost(kCGHIDEventTap, ev);
                CFRelease(ev as _);
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn mouse_button(_button: MouseButton, _is_down: bool) {}

    #[cfg(windows)]
    pub fn mouse_wheel(dx: i32, dy: i32) {
        Self::sync_input_desktop();
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL,
            MOUSEINPUT,
        };

        let mut inputs = Vec::new();
        if dy != 0 {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dy * 120) as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        if dx != 0 {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dx * 120) as u32,
                        dwFlags: MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }

        if !inputs.is_empty() {
            unsafe {
                let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn mouse_wheel(dx: i32, dy: i32) {
        use macos_ffi::*;
        unsafe {
            // dy for vertical wheel, dx for horizontal wheel
            let ev = CGEventCreateScrollWheelEvent2(
                std::ptr::null_mut(),
                kCGScrollEventUnitLine,
                2,
                dy,
                dx,
                0,
            );
            if !ev.is_null() {
                CGEventPost(kCGHIDEventTap, ev);
                CFRelease(ev as _);
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn mouse_wheel(_dx: i32, _dy: i32) {}

    #[cfg(windows)]
    pub fn key_event(vk: u16, is_down: bool, extended: bool) {
        Self::sync_input_desktop();
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, VIRTUAL_KEY,
        };

        let mut flags = if is_down {
            windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
        } else {
            KEYEVENTF_KEYUP
        };
        if extended {
            flags = flags | KEYEVENTF_EXTENDEDKEY;
        }

        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        unsafe {
            let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn key_event(vk: u16, is_down: bool, _extended: bool) {
        use macos_ffi::*;
        if let Some(mac_code) = keycodes::windows_vk_to_macos_keycode(vk) {
            unsafe {
                let ev = CGEventCreateKeyboardEvent(std::ptr::null_mut(), mac_code, is_down);
                if !ev.is_null() {
                    CGEventPost(kCGHIDEventTap, ev);
                    CFRelease(ev as _);
                }
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn key_event(_vk: u16, _is_down: bool, _extended: bool) {}
}

/// Unified cross-platform clipboard synchronization engine.
pub struct NativeClipboard {
    inner: Option<Clipboard>,
}

impl NativeClipboard {
    pub fn new() -> Self {
        Self {
            inner: Clipboard::new().ok(),
        }
    }

    pub fn get_text(&mut self) -> Option<String> {
        self.inner.as_mut().and_then(|cb| cb.get_text().ok())
    }

    pub fn set_text(&mut self, text: &str) -> bool {
        if let Some(cb) = self.inner.as_mut() {
            cb.set_text(text).is_ok()
        } else {
            false
        }
    }

    pub fn get_image_png(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        if let Some(cb) = self.inner.as_mut() {
            if let Ok(img) = cb.get_image() {
                let w = img.width as u32;
                let h = img.height as u32;
                if w == 0 || h == 0 {
                    return None;
                }
                let rgba = image::RgbaImage::from_raw(w, h, img.bytes.into_owned())?;
                let mut cursor = std::io::Cursor::new(Vec::new());
                if rgba.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                    return Some((cursor.into_inner(), w, h));
                }
            }
        }
        None
    }

    pub fn set_image_png(&mut self, png_bytes: &[u8]) -> bool {
        if let Ok(dynamic_img) = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png) {
            let rgba = dynamic_img.to_rgba8();
            let (width, height) = rgba.dimensions();
            let img_data = arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: std::borrow::Cow::Owned(rgba.into_raw()),
            };
            if let Some(cb) = self.inner.as_mut() {
                return cb.set_image(img_data).is_ok();
            }
        }
        false
    }
}

/// Real-time clipboard watcher polling system clipboard and broadcasting changes.
pub struct ClipboardWatcher {
    network: NetworkManager,
    last_text: Arc<Mutex<String>>,
    last_image_len: Arc<Mutex<usize>>,
}

impl ClipboardWatcher {
    pub fn new(network: NetworkManager) -> Self {
        Self {
            network,
            last_text: Arc::new(Mutex::new(String::new())),
            last_image_len: Arc::new(Mutex::new(0)),
        }
    }

    pub fn start(self: Arc<Self>) {
        use base64::Engine;
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(400));
            let mut img_counter = 0;

            loop {
                interval.tick().await;
                img_counter += 1;

                let mut cb = NativeClipboard::new();
                let text_opt = cb.get_text();

                let mut sent_text = false;
                if let Some(text) = text_opt {
                    let should_send = {
                        let mut last = self.last_text.lock();
                        if !text.is_empty() && *last != text {
                            *last = text.clone();
                            true
                        } else {
                            false
                        }
                    };

                    if should_send {
                        sent_text = true;
                        let msg = WireMessage {
                            t: "clip".to_string(),
                            v: Some(1),
                            id: None, name: None, os: None, port: None, role: None,
                            o: None, s: None, d: None, h: None, target: None,
                            dx: None, dy: None, b: None, k: None, code: None,
                            vk: None, mods: None, smooth: None,
                            text: Some(text),
                            ts: Some(chrono::Utc::now().timestamp_millis() as u64),
                            png: None, w: None, height: None, bytes: None, parts: None, shot: None,
                            files: None,
                        };
                        self.network.send_packet(msg).await;
                    }
                }

                // If no text was sent and every 2 cycles (~800ms), check for clipboard pictures/screenshots
                if !sent_text && img_counter % 2 == 0 {
                    if let Some((png, w, h)) = cb.get_image_png() {
                        let len = png.len();
                        let should_send_img = {
                            let mut last_len = self.last_image_len.lock();
                            if len > 0 && len <= 10 * 1024 * 1024 && *last_len != len {
                                *last_len = len;
                                true
                            } else {
                                false
                            }
                        };

                        if should_send_img {
                            let b64 = base64::prelude::BASE64_STANDARD.encode(&png);
                            let msg = WireMessage {
                                t: "clip.image".to_string(),
                                v: Some(1),
                                id: Some(format!("img_{}", chrono::Utc::now().timestamp_millis())),
                                name: None, os: None, port: None, role: None,
                                o: None, s: None, d: None, h: None, target: None,
                                dx: None, dy: None, b: None, k: None, code: None,
                                vk: None, mods: None, smooth: None,
                                text: None,
                                ts: Some(chrono::Utc::now().timestamp_millis() as u64),
                                png: Some(b64),
                                w: Some(w),
                                height: Some(h),
                                bytes: Some(len),
                                parts: Some(1),
                                shot: Some(true),
                                files: None,
                            };
                            self.network.send_packet(msg).await;
                        }
                    }
                }
            }
        });
    }
}

/// Global low-level hook engine for Windows mouse/keyboard capture and edge-crossing.
#[cfg(windows)]
pub mod hook_engine {
    use super::*;
    use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LBUTTON};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, GetSystemMetrics,
        SetCursorPos, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK,
        KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, MSG, SM_CXSCREEN, SM_CYSCREEN, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WH_KEYBOARD_LL,
        WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
        WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
        WM_SYSKEYDOWN,
    };

    static mut MOUSE_HOOK: Option<HHOOK> = None;
    static mut KEYBOARD_HOOK: Option<HHOOK> = None;
    static mut APP_HANDLE: Option<AppHandle> = None;
    static mut APP_STATE: Option<AppState> = None;
    static mut NETWORK_MGR: Option<NetworkManager> = None;

    pub fn get_virtual_desktop_bounds() -> (i32, i32, i32, i32) {
        unsafe {
            let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if vw <= 0 || vh <= 0 {
                (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
            } else {
                (vx, vy, vw, vh)
            }
        }
    }

    pub fn start_hooks(app: AppHandle, state: AppState, network: NetworkManager) {
        unsafe {
            APP_HANDLE = Some(app);
            APP_STATE = Some(state);
            NETWORK_MGR = Some(network);
        }

        std::thread::spawn(|| {
            unsafe {
                let mouse_hook = SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(mouse_hook_proc),
                    None,
                    0,
                );
                if let Ok(h) = mouse_hook {
                    MOUSE_HOOK = Some(h);
                }

                let key_hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_hook_proc),
                    None,
                    0,
                );
                if let Ok(h) = key_hook {
                    KEYBOARD_HOOK = Some(h);
                }

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                if let Some(h) = MOUSE_HOOK.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
                if let Some(h) = KEYBOARD_HOOK.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
        });
    }

    unsafe extern "system" fn mouse_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
        if n_code >= 0 && l_param.0 != 0 {
            let msg = w_param.0 as u32;
            let m = *(l_param.0 as *const MSLLHOOKSTRUCT);

            if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
                let snap = state.snapshot();
                let has_peer = snap.client.is_some();
                let is_driving = snap.input == "driving";

                let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
                let max_x = vx + vw;
                let max_y = vy + vh;

                if !is_driving && has_peer {
                    // Edge check across the entire virtual multi-monitor desktop
                    let crossed = match snap.layout.side.as_str() {
                        "right" => m.pt.x >= max_x - 2,
                        "left" => m.pt.x <= vx + 1,
                        "top" => m.pt.y <= vy + 1,
                        "bottom" => m.pt.y >= max_y - 2,
                        _ => false,
                    };

                    if crossed {
                        let is_left_down = (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0;
                        let is_ctrl_down = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;

                        if is_left_down && is_ctrl_down {
                            // Window Edge-Handoff Trigger: Ctrl held while dragging window to edge
                            let _ = app.emit("window-handoff-trigger", serde_json::json!({
                                "x": m.pt.x,
                                "y": m.pt.y,
                                "side": snap.layout.side,
                            }));
                        }

                        state.set_where_input("driving");
                        let new_snap = state.snapshot();
                        let _ = app.emit("state-change", &new_snap);

                        let take = WireMessage {
                            t: "kvm.take".to_string(),
                            v: Some(1),
                            ..Default::default()
                        };
                        net.send_packet_sync(take);
                        return LRESULT(1);
                    }
                } else if is_driving {
                    // Swallow and stream to peer
                    match msg {
                        WM_MOUSEMOVE => {
                            static mut LAST_PT: POINT = POINT { x: 0, y: 0 };
                            let dx = (m.pt.x - LAST_PT.x) as f64;
                            let dy = (m.pt.y - LAST_PT.y) as f64;
                            LAST_PT = m.pt;

                            if dx.abs() > 0.0 || dy.abs() > 0.0 {
                                net.send_motion(dx, dy);
                            }

                            // Keep cursor parked near edge of the virtual desktop
                            let park_x = match snap.layout.side.as_str() {
                                "right" => max_x - 5,
                                "left" => vx + 5,
                                _ => m.pt.x,
                            };
                            let park_y = match snap.layout.side.as_str() {
                                "top" => vy + 5,
                                "bottom" => max_y - 5,
                                _ => m.pt.y,
                            };
                            let _ = SetCursorPos(park_x, park_y);
                            return LRESULT(1);
                        }
                        WM_LBUTTONDOWN | WM_LBUTTONUP => {
                            let is_down = msg == WM_LBUTTONDOWN;
                            let btn_msg = WireMessage {
                                t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                                v: Some(1),
                                b: Some(0),
                                ..Default::default()
                            };
                            net.send_packet_sync(btn_msg);
                            return LRESULT(1);
                        }
                        WM_RBUTTONDOWN | WM_RBUTTONUP => {
                            let is_down = msg == WM_RBUTTONDOWN;
                            let btn_msg = WireMessage {
                                t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                                v: Some(1),
                                b: Some(1),
                                ..Default::default()
                            };
                            net.send_packet_sync(btn_msg);
                            return LRESULT(1);
                        }
                        WM_MBUTTONDOWN | WM_MBUTTONUP => {
                            let is_down = msg == WM_MBUTTONDOWN;
                            let btn_msg = WireMessage {
                                t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                                v: Some(1),
                                b: Some(2),
                                ..Default::default()
                            };
                            net.send_packet_sync(btn_msg);
                            return LRESULT(1);
                        }
                        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                            let notches = ((m.mouseData >> 16) as i16) as f64 / 120.0;
                            let (dx, dy) = if msg == WM_MOUSEHWHEEL {
                                (notches, 0.0)
                            } else {
                                (0.0, -notches)
                            };
                            let wh_msg = WireMessage {
                                t: "k.wh".to_string(),
                                v: Some(1),
                                dx: Some(dx), dy: Some(dy),
                                ..Default::default()
                            };
                            net.send_packet_sync(wh_msg);
                            return LRESULT(1);
                        }
                        _ => {}
                    }
                }
            }
        }

        CallNextHookEx(HHOOK::default(), n_code, w_param, l_param)
    }

    unsafe extern "system" fn keyboard_hook_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
        if n_code >= 0 && l_param.0 != 0 {
            let msg = w_param.0 as u32;
            let k = *(l_param.0 as *const KBDLLHOOKSTRUCT);

            if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
                let snap = state.snapshot();
                let is_driving = snap.input == "driving";

                // Hotkey to release: VK_RMENU (Right Alt, 165) or VK_RCONTROL (163)
                if (k.vkCode == 165 || k.vkCode == 163) && (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) {
                    if is_driving {
                        state.set_where_input("home");
                        let new_snap = state.snapshot();
                        let _ = app.emit("state-change", &new_snap);

                        let home_msg = WireMessage {
                            t: "kvm.home".to_string(),
                            v: Some(1),
                            ..Default::default()
                        };
                        net.send_packet_sync(home_msg);

                        // Center cursor on local virtual desktop
                        let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
                        let _ = SetCursorPos(vx + vw / 2, vy + vh / 2);
                        return LRESULT(1);
                    }
                }

                if is_driving {
                    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                    let key_msg = WireMessage {
                        t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                        v: Some(1),
                        vk: Some(k.vkCode),
                        ..Default::default()
                    };
                    net.send_packet_sync(key_msg);
                    return LRESULT(1);
                }
            }
        }

        CallNextHookEx(HHOOK::default(), n_code, w_param, l_param)
    }
}

#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals, non_snake_case, static_mut_refs)]
pub mod hook_engine {
    use super::*;
    use super::macos_ffi::*;
    use std::ffi::c_void;

    static mut APP_HANDLE: Option<AppHandle> = None;
    static mut APP_STATE: Option<AppState> = None;
    static mut NETWORK_MGR: Option<NetworkManager> = None;
    static mut EVENT_TAP: Option<CFMachPortRef> = None;
    static mut LAST_PT: CGPoint = CGPoint { x: 0.0, y: 0.0 };

    pub fn ensure_accessibility_permission() -> bool {
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let dict = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            let trusted = AXIsProcessTrustedWithOptions(dict) != 0;
            if !dict.is_null() {
                CFRelease(dict as _);
            }
            trusted
        }
    }

    pub fn get_virtual_desktop_bounds() -> (i32, i32, i32, i32) {
        unsafe {
            let mut displays = [0u32; 16];
            let mut count = 0u32;
            let err = CGGetActiveDisplayList(16, displays.as_mut_ptr(), &mut count);
            if err != 0 || count == 0 {
                return (0, 0, 1920, 1080);
            }

            let mut min_x = f64::MAX;
            let mut min_y = f64::MAX;
            let mut max_x = f64::MIN;
            let mut max_y = f64::MIN;

            for &d in &displays[..count as usize] {
                let bounds = CGDisplayBounds(d);
                if bounds.origin.x < min_x {
                    min_x = bounds.origin.x;
                }
                if bounds.origin.y < min_y {
                    min_y = bounds.origin.y;
                }
                let right = bounds.origin.x + bounds.size.width;
                let bottom = bounds.origin.y + bounds.size.height;
                if right > max_x {
                    max_x = right;
                }
                if bottom > max_y {
                    max_y = bottom;
                }
            }

            let vx = min_x as i32;
            let vy = min_y as i32;
            let vw = (max_x - min_x) as i32;
            let vh = (max_y - min_y) as i32;

            if vw <= 0 || vh <= 0 {
                (0, 0, 1920, 1080)
            } else {
                (vx, vy, vw, vh)
            }
        }
    }

    pub fn start_hooks(app: AppHandle, state: AppState, network: NetworkManager) {
        unsafe {
            APP_HANDLE = Some(app);
            APP_STATE = Some(state);
            NETWORK_MGR = Some(network);
        }

        std::thread::spawn(|| {
            if !ensure_accessibility_permission() {
                log::warn!("GlideDesk: macOS Accessibility permissions not granted yet. Prompting user in System Settings...");
            }

            let mask: u64 = (1 << kCGEventMouseMoved)
                | (1 << kCGEventLeftMouseDown)
                | (1 << kCGEventLeftMouseUp)
                | (1 << kCGEventRightMouseDown)
                | (1 << kCGEventRightMouseUp)
                | (1 << kCGEventOtherMouseDown)
                | (1 << kCGEventOtherMouseUp)
                | (1 << kCGEventLeftMouseDragged)
                | (1 << kCGEventRightMouseDragged)
                | (1 << kCGEventOtherMouseDragged)
                | (1 << kCGEventScrollWheel)
                | (1 << kCGEventKeyDown)
                | (1 << kCGEventKeyUp)
                | (1 << kCGEventFlagsChanged);

            loop {
                unsafe {
                    let tap = CGEventTapCreate(
                        kCGSessionEventTap,
                        kCGHeadInsertEventTap,
                        kCGEventTapOptionDefault,
                        mask,
                        event_tap_callback,
                        std::ptr::null_mut(),
                    );

                    if !tap.is_null() {
                        log::info!("GlideDesk: macOS CGEventTap successfully initialized!");
                        EVENT_TAP = Some(tap);

                        let dummy = CGEventCreate(std::ptr::null_mut());
                        if !dummy.is_null() {
                            LAST_PT = CGEventGetLocation(dummy);
                            CFRelease(dummy as _);
                        }

                        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                        if !source.is_null() {
                            let rl = CFRunLoopGetCurrent();
                            CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                            CGEventTapEnable(tap, true);
                            CFRunLoopRun();
                            CFRelease(source as _);
                        }
                        CFRelease(tap as _);
                        EVENT_TAP = None;
                        break;
                    } else {
                        log::warn!("GlideDesk: CGEventTapCreate returned NULL (waiting for Accessibility permissions)...");
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                }
            }
        });
    }

    unsafe extern "C" fn event_tap_callback(
        _proxy: *mut c_void,
        r#type: CGEventType,
        event: CGEventRef,
        _refcon: *mut c_void,
    ) -> CGEventRef {
        if r#type == kCGEventTapDisabledByTimeout || r#type == kCGEventTapDisabledByUserInput {
            if let Some(tap) = EVENT_TAP {
                CGEventTapEnable(tap, true);
            }
            return event;
        }

        if event.is_null() {
            return event;
        }

        if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
            let snap = state.snapshot();
            let has_peer = snap.client.is_some();
            let is_driving = snap.input == "driving";

            let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
            let max_x = vx + vw;
            let max_y = vy + vh;

            if !is_driving && has_peer {
                if r#type == kCGEventMouseMoved
                    || r#type == kCGEventLeftMouseDragged
                    || r#type == kCGEventRightMouseDragged
                {
                    let pt = CGEventGetLocation(event);
                    LAST_PT = pt;

                    let crossed = match snap.layout.side.as_str() {
                        "right" => pt.x >= (max_x - 2) as f64,
                        "left" => pt.x <= (vx + 1) as f64,
                        "top" => pt.y <= (vy + 1) as f64,
                        "bottom" => pt.y >= (max_y - 2) as f64,
                        _ => false,
                    };

                    if crossed {
                        let is_left_drag = r#type == kCGEventLeftMouseDragged;
                        let flags = CGEventGetFlags(event);
                        let cmd_held = (flags & kCGEventFlagMaskCommand) != 0;

                        if is_left_drag && cmd_held {
                            // Window Edge-Handoff Trigger: Command held while dragging window to edge
                            let _ = app.emit("window-handoff-trigger", serde_json::json!({
                                "x": pt.x,
                                "y": pt.y,
                                "side": snap.layout.side,
                            }));
                        }

                        state.set_where_input("driving");
                        let new_snap = state.snapshot();
                        let _ = app.emit("state-change", &new_snap);

                        let take = WireMessage {
                            t: "kvm.take".to_string(),
                            v: Some(1),
                            ..Default::default()
                        };
                        net.send_packet_sync(take);

                        let park_pt = match snap.layout.side.as_str() {
                            "right" => CGPoint { x: (max_x - 5) as f64, y: pt.y },
                            "left" => CGPoint { x: (vx + 5) as f64, y: pt.y },
                            "top" => CGPoint { x: pt.x, y: (vy + 5) as f64 },
                            "bottom" => CGPoint { x: pt.x, y: (max_y - 5) as f64 },
                            _ => pt,
                        };
                        let _ = CGWarpMouseCursorPosition(park_pt);
                        LAST_PT = park_pt;
                        return std::ptr::null_mut();
                    }
                }
            } else if is_driving {
                match r#type {
                    kCGEventMouseMoved
                    | kCGEventLeftMouseDragged
                    | kCGEventRightMouseDragged
                    | kCGEventOtherMouseDragged => {
                        let pt = CGEventGetLocation(event);
                        let dx = pt.x - LAST_PT.x;
                        let dy = pt.y - LAST_PT.y;

                        if dx.abs() > 0.0 || dy.abs() > 0.0 {
                            net.send_motion(dx, dy);
                        }

                        let park_pt = match snap.layout.side.as_str() {
                            "right" => CGPoint {
                                x: (max_x - 5) as f64,
                                y: pt.y.clamp(vy as f64, (max_y - 1) as f64),
                            },
                            "left" => CGPoint {
                                x: (vx + 5) as f64,
                                y: pt.y.clamp(vy as f64, (max_y - 1) as f64),
                            },
                            "top" => CGPoint {
                                x: pt.x.clamp(vx as f64, (max_x - 1) as f64),
                                y: (vy + 5) as f64,
                            },
                            "bottom" => CGPoint {
                                x: pt.x.clamp(vx as f64, (max_x - 1) as f64),
                                y: (max_y - 5) as f64,
                            },
                            _ => pt,
                        };
                        let _ = CGWarpMouseCursorPosition(park_pt);
                        LAST_PT = park_pt;
                        return std::ptr::null_mut();
                    }
                    kCGEventLeftMouseDown | kCGEventLeftMouseUp => {
                        let is_down = r#type == kCGEventLeftMouseDown;
                        let btn_msg = WireMessage {
                            t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                            v: Some(1),
                            b: Some(0),
                            ..Default::default()
                        };
                        net.send_packet_sync(btn_msg);
                        return std::ptr::null_mut();
                    }
                    kCGEventRightMouseDown | kCGEventRightMouseUp => {
                        let is_down = r#type == kCGEventRightMouseDown;
                        let btn_msg = WireMessage {
                            t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                            v: Some(1),
                            b: Some(1),
                            ..Default::default()
                        };
                        net.send_packet_sync(btn_msg);
                        return std::ptr::null_mut();
                    }
                    kCGEventOtherMouseDown | kCGEventOtherMouseUp => {
                        let is_down = r#type == kCGEventOtherMouseDown;
                        let btn_msg = WireMessage {
                            t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                            v: Some(1),
                            b: Some(2),
                            ..Default::default()
                        };
                        net.send_packet_sync(btn_msg);
                        return std::ptr::null_mut();
                    }
                    kCGEventScrollWheel => {
                        let dy = CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis1) as f64;
                        let dx = CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis2) as f64;
                        let wh_msg = WireMessage {
                            t: "k.wh".to_string(),
                            v: Some(1),
                            dx: Some(dx), dy: Some(dy),
                            ..Default::default()
                        };
                        net.send_packet_sync(wh_msg);
                        return std::ptr::null_mut();
                    }
                    kCGEventFlagsChanged => {
                        let keycode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) as u16;
                        let flags = CGEventGetFlags(event);

                        let is_down = match keycode {
                            56 | 60 => (flags & kCGEventFlagMaskShift) != 0,
                            59 | 62 => (flags & kCGEventFlagMaskControl) != 0,
                            58 | 61 => (flags & kCGEventFlagMaskAlternate) != 0,
                            55 | 54 => (flags & kCGEventFlagMaskCommand) != 0,
                            57 => (flags & kCGEventFlagMaskAlphaShift) != 0,
                            _ => false,
                        };

                        // Release hotkeys: Right Option (61), Right Command (54), Right Control (62)
                        if (keycode == 61 || keycode == 54 || keycode == 62) && is_down {
                            state.set_where_input("home");
                            let new_snap = state.snapshot();
                            let _ = app.emit("state-change", &new_snap);

                            let home_msg = WireMessage {
                                t: "kvm.home".to_string(),
                                v: Some(1),
                                ..Default::default()
                            };
                            net.send_packet_sync(home_msg);

                            let center_pt = CGPoint {
                                x: (vx + vw / 2) as f64,
                                y: (vy + vh / 2) as f64,
                            };
                            let _ = CGWarpMouseCursorPosition(center_pt);
                            LAST_PT = center_pt;
                            return std::ptr::null_mut();
                        }

                        let vk = keycodes::macos_keycode_to_windows_vk(keycode);
                        let key_msg = WireMessage {
                            t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                            v: Some(1),
                            vk: Some(vk),
                            ..Default::default()
                        };
                        net.send_packet_sync(key_msg);
                        return std::ptr::null_mut();
                    }
                    kCGEventKeyDown | kCGEventKeyUp => {
                        let keycode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) as u16;
                        let is_down = r#type == kCGEventKeyDown;

                        // Release hotkey check
                        if (keycode == 61 || keycode == 62) && is_down {
                            state.set_where_input("home");
                            let new_snap = state.snapshot();
                            let _ = app.emit("state-change", &new_snap);

                            let home_msg = WireMessage {
                                t: "kvm.home".to_string(),
                                v: Some(1),
                                ..Default::default()
                            };
                            net.send_packet_sync(home_msg);

                            let center_pt = CGPoint {
                                x: (vx + vw / 2) as f64,
                                y: (vy + vh / 2) as f64,
                            };
                            let _ = CGWarpMouseCursorPosition(center_pt);
                            LAST_PT = center_pt;
                            return std::ptr::null_mut();
                        }

                        let vk = keycodes::macos_keycode_to_windows_vk(keycode);
                        let key_msg = WireMessage {
                            t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                            v: Some(1),
                            vk: Some(vk),
                            ..Default::default()
                        };
                        net.send_packet_sync(key_msg);
                        return std::ptr::null_mut();
                    }
                    _ => {}
                }
            }
        }

        event
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub mod hook_engine {
    use super::*;
    pub fn start_hooks(_app: AppHandle, _state: AppState, _network: NetworkManager) {
        // Safe stub for Linux (uinput / libinput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_keycodes_bidirectional() {
        use keycodes::*;

        // A key
        assert_eq!(windows_vk_to_macos_keycode(0x41), Some(0));
        assert_eq!(macos_keycode_to_windows_vk(0), 0x41);

        // Return key
        assert_eq!(windows_vk_to_macos_keycode(0x0D), Some(36));
        assert_eq!(macos_keycode_to_windows_vk(36), 0x0D);

        // Space key
        assert_eq!(windows_vk_to_macos_keycode(0x20), Some(49));
        assert_eq!(macos_keycode_to_windows_vk(49), 0x20);

        // Escape key
        assert_eq!(windows_vk_to_macos_keycode(0x1B), Some(53));
        assert_eq!(macos_keycode_to_windows_vk(53), 0x1B);

        // Modifiers: Shift, Control, Alt
        assert_eq!(windows_vk_to_macos_keycode(0x10), Some(56)); // VK_SHIFT
        assert_eq!(windows_vk_to_macos_keycode(0xA0), Some(56)); // VK_LSHIFT
        assert_eq!(windows_vk_to_macos_keycode(0xA1), Some(60)); // VK_RSHIFT
        assert_eq!(windows_vk_to_macos_keycode(0xA5), Some(61)); // VK_RMENU
        assert_eq!(windows_vk_to_macos_keycode(0xA3), Some(62)); // VK_RCONTROL

        assert_eq!(macos_keycode_to_windows_vk(61), 0xA5); // Right Alt
        assert_eq!(macos_keycode_to_windows_vk(62), 0xA3); // Right Control
        assert_eq!(macos_keycode_to_windows_vk(54), 0x5C); // Right Command
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_virtual_desktop_bounds() {
        let (x, y, w, h) = hook_engine::get_virtual_desktop_bounds();
        println!("Virtual desktop bounds: x={}, y={}, w={}, h={}", x, y, w, h);
        assert!(w > 0, "Width should be greater than zero");
        assert!(h > 0, "Height should be greater than zero");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_native_input_injector_macos() {
        // Test that relative move, mouse buttons, scroll, and key injection execute without panic
        NativeInputInjector::move_cursor_relative(0, 0);
        NativeInputInjector::mouse_button(MouseButton::Left, false);
        NativeInputInjector::mouse_wheel(0, 0);
        NativeInputInjector::key_event(0x41, false, false);
    }
}
