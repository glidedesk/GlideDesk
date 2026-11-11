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
use tauri::{AppHandle, Emitter, Manager};
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

/// Check if cursor coordinates cross the outer edge of the virtual desktop layout.
#[cfg(any(test, not(windows)))]
pub fn check_edge_crossed(side: &str, pt_x: f64, pt_y: f64, vx: i32, vy: i32, vw: i32, vh: i32) -> bool {
    let max_x = (vx + vw) as f64;
    let max_y = (vy + vh) as f64;
    match side {
        "right" => pt_x >= max_x - 2.0,
        "left" => pt_x <= (vx + 1) as f64,
        "top" => pt_y <= (vy + 1) as f64,
        "bottom" => pt_y >= max_y - 2.0,
        _ => false,
    }
}

/// Where along the crossed edge the pointer left, as 0..1 (top/left = 0).
///
/// Sent with `kvm.take` so the other computer can put its pointer on the same spot of
/// the edge it is entered through.
pub fn entry_fraction(side: &str, x: f64, y: f64, vx: i32, vy: i32, vw: i32, vh: i32) -> f64 {
    let (base, span, pos) = match side {
        "top" | "bottom" => (vx, vw.max(2), x),
        _ => (vy, vh.max(2), y),
    };
    ((pos - base as f64) / span as f64).clamp(0.0, 1.0)
}

/// Notches from a Windows wheel message.
///
/// `WM_MOUSEWHEEL`/`WM_MOUSEHWHEEL` carry a signed delta in the high word: +120 is
/// one notch up, -120 one notch down. Windows and macOS both read a positive
/// vertical notch as "scroll up", so the value is forwarded unchanged.
pub fn wheel_notches_from_windows(vertical: bool, mouse_data: u32) -> (f64, f64) {
    let notches = ((mouse_data >> 16) as i16) as f64 / 120.0;
    if vertical {
        (0.0, notches)
    } else {
        (notches, 0.0)
    }
}

/// Check if cumulative remote virtual deltas indicate cursor has moved back across the seam to home.
#[cfg(any(test, not(windows)))]
pub fn check_edge_returned(side: &str, virtual_remote_x: f64, virtual_remote_y: f64) -> bool {
    match side {
        "right" => virtual_remote_x < -8.0,
        "left" => virtual_remote_x > 8.0,
        "top" => virtual_remote_y > 8.0,
        "bottom" => virtual_remote_y < -8.0,
        _ => false,
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
    pub const kCGScrollWheelEventPointDeltaAxis1: u32 = 96;
    pub const kCGScrollWheelEventPointDeltaAxis2: u32 = 97;

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
        pub fn CGEventKeyboardGetUnicodeString(
            event: CGEventRef,
            max_string_length: u64,
            actual_string_length: *mut u64,
            unicode_string: *mut u16,
        );
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

pub trait InputBackend: Send + Sync {
    fn key(&self, vk: u16, is_down: bool, extended: bool) -> Result<(), String>;
    fn button(&self, button: MouseButton, is_down: bool) -> Result<(), String>;
    fn wheel(&self, dx: i32, dy: i32) -> Result<(), String>;
    fn move_relative(&self, dx: i32, dy: i32) -> Result<(), String>;
    fn warp(&self, x: i32, y: i32) -> Result<(), String>;
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum RecordedInputEvent {
    Key { vk: u16, is_down: bool, extended: bool, timestamp_ms: u64 },
    Button { button: MouseButton, is_down: bool, timestamp_ms: u64 },
    Wheel { dx: i32, dy: i32, timestamp_ms: u64 },
    MoveRelative { dx: i32, dy: i32, timestamp_ms: u64 },
    Warp { x: i32, y: i32, timestamp_ms: u64 },
}

#[cfg(test)]
pub struct RecordingBackend {
    events: std::sync::Mutex<Vec<RecordedInputEvent>>,
}

#[cfg(test)]
impl RecordingBackend {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<RecordedInputEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

#[cfg(test)]
impl Default for RecordingBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl InputBackend for RecordingBackend {
    fn key(&self, vk: u16, is_down: bool, extended: bool) -> Result<(), String> {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.events.lock().unwrap().push(RecordedInputEvent::Key {
            vk,
            is_down,
            extended,
            timestamp_ms,
        });
        Ok(())
    }

    fn button(&self, button: MouseButton, is_down: bool) -> Result<(), String> {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.events.lock().unwrap().push(RecordedInputEvent::Button {
            button,
            is_down,
            timestamp_ms,
        });
        Ok(())
    }

    fn wheel(&self, dx: i32, dy: i32) -> Result<(), String> {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.events.lock().unwrap().push(RecordedInputEvent::Wheel {
            dx,
            dy,
            timestamp_ms,
        });
        Ok(())
    }

    fn move_relative(&self, dx: i32, dy: i32) -> Result<(), String> {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.events.lock().unwrap().push(RecordedInputEvent::MoveRelative {
            dx,
            dy,
            timestamp_ms,
        });
        Ok(())
    }

    fn warp(&self, x: i32, y: i32) -> Result<(), String> {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.events.lock().unwrap().push(RecordedInputEvent::Warp {
            x,
            y,
            timestamp_ms,
        });
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectionErrorKind {
    Key,
    Button,
    Wheel,
    MoveRelative,
    Warp,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitAction {
    Log,
    Suppress,
}

#[allow(dead_code)]
#[derive(Default)]
pub struct InjectionErrorRateLimiter {
    failed: std::collections::HashSet<InjectionErrorKind>,
}

#[allow(dead_code)]
impl InjectionErrorRateLimiter {
    pub fn new() -> Self {
        Self {
            failed: std::collections::HashSet::new(),
        }
    }

    pub fn record_failure(&mut self, kind: InjectionErrorKind) -> RateLimitAction {
        if self.failed.insert(kind) {
            RateLimitAction::Log
        } else {
            RateLimitAction::Suppress
        }
    }

    pub fn record_success(&mut self, kind: InjectionErrorKind) {
        self.failed.remove(&kind);
    }
}

pub static INJECTION_RATE_LIMITER: parking_lot::Mutex<Option<InjectionErrorRateLimiter>> =
    parking_lot::Mutex::new(None);

#[allow(dead_code)]
pub fn handle_injection_result(kind: InjectionErrorKind, result: Result<(), String>) {
    let mut guard = INJECTION_RATE_LIMITER.lock();
    let limiter = guard.get_or_insert_with(InjectionErrorRateLimiter::new);
    match result {
        Ok(()) => {
            limiter.record_success(kind);
        }
        Err(err) => {
            let action = limiter.record_failure(kind);
            if action == RateLimitAction::Log {
                log::warn!("[KVM] Linux input injection error ({:?}): {}", kind, err);
                update_kvm_runtime_state(
                    "x11rb",
                    &format!("Linux input injection error: {}", err),
                );
            }
        }
    }
}

#[cfg(any(test, target_os = "linux", target_os = "macos"))]
pub const SELF_ECHO_SUPPRESSION_WINDOW_MS: u64 = 25;

pub static LAST_INJECTION_TIMESTAMP_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub fn record_injection_timestamp() {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    LAST_INJECTION_TIMESTAMP_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(test, target_os = "linux", target_os = "macos"))]
pub fn should_forward_captured_event(now_ms: u64, last_injection_ms: u64) -> bool {
    should_forward_captured_event_with_window(now_ms, last_injection_ms, SELF_ECHO_SUPPRESSION_WINDOW_MS)
}

#[cfg(any(test, target_os = "linux", target_os = "macos"))]
pub fn should_forward_captured_event_with_window(
    now_ms: u64,
    last_injection_ms: u64,
    window_ms: u64,
) -> bool {
    if last_injection_ms == 0 {
        return true;
    }
    if now_ms < last_injection_ms {
        return true;
    }
    now_ms.saturating_sub(last_injection_ms) >= window_ms
}

#[cfg(any(test, target_os = "linux"))]
pub fn probe_display_available(display: Option<&str>) -> Result<(), String> {
    match display {
        Some(d) if !d.trim().is_empty() => Ok(()),
        _ => Err("input injection unavailable: DISPLAY not set".to_string()),
    }
}

#[cfg(any(test, target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureHealth {
    Healthy,
    Lost,
}

#[cfg(any(test, target_os = "linux"))]
pub fn classify_capture_health(consecutive_failures: u32, threshold: u32) -> CaptureHealth {
    if consecutive_failures >= threshold {
        CaptureHealth::Lost
    } else {
        CaptureHealth::Healthy
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn evaluate_linux_session_capability(
    display: Option<&str>,
    session_type: Option<&str>,
    capture_running: bool,
) -> (&'static str, &'static str) {
    match display {
        None => (
            "x11rb",
            "Linux input capture: headless session (DISPLAY not set); remote input injection and local capture unavailable",
        ),
        Some(d) if d.trim().is_empty() => (
            "x11rb",
            "Linux input capture: headless session (DISPLAY not set); remote input injection and local capture unavailable",
        ),
        Some(_) => match session_type {
            Some("wayland") => (
                "wayland",
                "Linux input capture: Wayland session detected; global raw input capture unavailable (requires X11 session or root uinput privileges)",
            ),
            _ => {
                if capture_running {
                    (
                        "x11",
                        "Linux input capture: active X11 session (capture engine running; x11rb ready)",
                    )
                } else {
                    (
                        "x11rb",
                        "Linux input capture: active X11 session detected but capture engine not running; local capture unavailable (injection ready)",
                    )
                }
            }
        },
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn x_keycode_to_vk(keycode: u8) -> Option<u16> {
    match keycode {
        9 => Some(0x1B),  // Escape
        10 => Some(0x31), // 1
        11 => Some(0x32), // 2
        12 => Some(0x33), // 3
        13 => Some(0x34), // 4
        14 => Some(0x35), // 5
        15 => Some(0x36), // 6
        16 => Some(0x37), // 7
        17 => Some(0x38), // 8
        18 => Some(0x39), // 9
        19 => Some(0x30), // 0
        20 => Some(0xBD), // -
        21 => Some(0xBB), // =
        22 => Some(0x08), // BackSpace
        23 => Some(0x09), // Tab
        24 => Some(0x51), // Q
        25 => Some(0x57), // W
        26 => Some(0x45), // E
        27 => Some(0x52), // R
        28 => Some(0x54), // T
        29 => Some(0x59), // Y
        30 => Some(0x55), // U
        31 => Some(0x49), // I
        32 => Some(0x4F), // O
        33 => Some(0x50), // P
        34 => Some(0xDB), // [
        35 => Some(0xDD), // ]
        36 => Some(0x0D), // Return
        37 => Some(0x11), // Control_L
        38 => Some(0x41), // A
        39 => Some(0x53), // S
        40 => Some(0x44), // D
        41 => Some(0x46), // F
        42 => Some(0x47), // G
        43 => Some(0x48), // H
        44 => Some(0x4A), // J
        45 => Some(0x4B), // K
        46 => Some(0x4C), // L
        47 => Some(0xBA), // ;
        48 => Some(0xDE), // '
        49 => Some(0xC0), // `
        50 => Some(0x10), // Shift_L
        51 => Some(0xDC), // \
        52 => Some(0x5A), // Z
        53 => Some(0x58), // X
        54 => Some(0x43), // C
        55 => Some(0x56), // V
        56 => Some(0x42), // B
        57 => Some(0x4E), // N
        58 => Some(0x4D), // M
        59 => Some(0xBC), // ,
        60 => Some(0xBE), // .
        61 => Some(0xBF), // /
        62 => Some(0x10), // Shift_R
        64 => Some(0x12), // Alt_L
        65 => Some(0x20), // Space
        66 => Some(0x14), // Caps_Lock
        67 => Some(0x70), // F1
        68 => Some(0x71), // F2
        69 => Some(0x72), // F3
        70 => Some(0x73), // F4
        71 => Some(0x74), // F5
        72 => Some(0x75), // F6
        73 => Some(0x76), // F7
        74 => Some(0x77), // F8
        75 => Some(0x78), // F9
        76 => Some(0x79), // F10
        95 => Some(0x7A), // F11
        96 => Some(0x7B), // F12
        105 => Some(0x11), // Control_R
        108 => Some(0x12), // Alt_R
        110 => Some(0x24), // Home
        111 => Some(0x26), // Up
        112 => Some(0x21), // Prior / Page_Up
        113 => Some(0x25), // Left
        114 => Some(0x27), // Right
        115 => Some(0x23), // End
        116 => Some(0x28), // Down
        117 => Some(0x22), // Next / Page_Down
        118 => Some(0x2D), // Insert
        119 => Some(0x2E), // Delete
        133 => Some(0x5B), // Super_L
        134 => Some(0x5C), // Super_R
        _ => None,
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn vk_to_x_keysym(vk: u16) -> Option<u32> {
    match vk {
        0x08 => Some(0xFF08), // BackSpace
        0x09 => Some(0xFF09), // Tab
        0x0D => Some(0xFF0D), // Return
        0x10 => Some(0xFFE1), // Shift_L
        0x11 => Some(0xFFE3), // Control_L
        0x12 => Some(0xFFE9), // Alt_L
        0x14 => Some(0xFFE5), // Caps_Lock
        0x1B => Some(0xFF1B), // Escape
        0x20 => Some(0x0020), // space
        0x21 => Some(0xFF55), // Page_Up
        0x22 => Some(0xFF56), // Page_Down
        0x23 => Some(0xFF57), // End
        0x24 => Some(0xFF50), // Home
        0x25 => Some(0xFF51), // Left
        0x26 => Some(0xFF52), // Up
        0x27 => Some(0xFF53), // Right
        0x28 => Some(0xFF54), // Down
        0x2D => Some(0xFF63), // Insert
        0x2E => Some(0xFFFF), // Delete
        // Digits 0-9
        0x30..=0x39 => Some(vk as u32),
        // Letters A-Z -> lowercase 'a'-'z' (0x61..=0x7A)
        0x41..=0x5A => Some((vk - 0x41 + 0x61) as u32),
        // Windows keys / Super
        0x5B => Some(0xFFEB), // Super_L
        0x5C => Some(0xFFEC), // Super_R
        // Function keys F1-F12
        0x70..=0x7B => Some(0xFFBE + (vk - 0x70) as u32),
        // Modifiers specific
        0xA0 => Some(0xFFE1), // Shift_L
        0xA1 => Some(0xFFE2), // Shift_R
        0xA2 => Some(0xFFE3), // Control_L
        0xA3 => Some(0xFFE4), // Control_R
        0xA4 => Some(0xFFE9), // Alt_L
        0xA5 => Some(0xFFEA), // Alt_R
        // OEM Punctuation
        0xBA => Some(0x003B), // semicolon
        0xBB => Some(0x003D), // equal
        0xBC => Some(0x002C), // comma
        0xBD => Some(0x002D), // minus
        0xBE => Some(0x002E), // period
        0xBF => Some(0x002F), // slash
        0xC0 => Some(0x0060), // grave
        0xDB => Some(0x005B), // bracketleft
        0xDC => Some(0x005C), // backslash
        0xDD => Some(0x005D), // bracketright
        0xDE => Some(0x0027), // apostrophe
        _ => None,
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn mouse_button_to_x11_button(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn mouse_wheel_to_x11_pulses(dx: i32, dy: i32) -> (Option<(u8, usize)>, Option<(u8, usize)>) {
    let y_pulse = if dy != 0 {
        let btn = if dy > 0 { 4 } else { 5 };
        let count = dy.abs().min(10) as usize;
        Some((btn, count))
    } else {
        None
    };
    let x_pulse = if dx != 0 {
        let btn = if dx > 0 { 7 } else { 6 };
        let count = dx.abs().min(10) as usize;
        Some((btn, count))
    } else {
        None
    };
    (x_pulse, y_pulse)
}

static CUSTOM_BACKEND: parking_lot::RwLock<Option<std::sync::Arc<dyn InputBackend>>> =
    parking_lot::RwLock::new(None);

#[cfg(test)]
pub fn set_input_backend(backend: Option<std::sync::Arc<dyn InputBackend>>) {
    *CUSTOM_BACKEND.write() = backend;
}

pub fn get_custom_backend() -> Option<std::sync::Arc<dyn InputBackend>> {
    CUSTOM_BACKEND.read().clone()
}

static KVM_APP_STATE: parking_lot::RwLock<Option<AppState>> = parking_lot::RwLock::new(None);

pub fn set_kvm_app_state(state: AppState) {
    *KVM_APP_STATE.write() = Some(state);
}

pub fn update_kvm_runtime_state(helper: &str, message: &str) {
    if let Some(state) = KVM_APP_STATE.read().as_ref() {
        let mut inner = state.inner.write();
        inner.kvm_state.helper = helper.to_string();
        inner.kvm_state.message = message.to_string();
    }
}

#[cfg(target_os = "linux")]
pub struct X11RbBackend {
    conn: x11rb::rust_connection::RustConnection,
    root: x11rb::protocol::xproto::Window,
    width: u32,
    height: u32,
    keysym_to_keycode: std::collections::HashMap<u32, u8>,
}

#[cfg(target_os = "linux")]
impl X11RbBackend {
    pub fn new() -> Result<Self, String> {
        let display = std::env::var("DISPLAY").ok();
        probe_display_available(display.as_deref())?;

        let (conn, screen_num) = x11rb::connect(display.as_deref())
            .map_err(|e| {
                let err = format!("input injection unavailable: failed to open X display: {}", e);
                update_kvm_runtime_state("x11rb", &err);
                err
            })?;

        use x11rb::connection::Connection;
        use x11rb::protocol::xproto;

        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let width = screen.width_in_pixels as u32;
        let height = screen.height_in_pixels as u32;

        let setup = conn.setup();
        let min_keycode = setup.min_keycode;
        let max_keycode = setup.max_keycode;
        let count = max_keycode.saturating_sub(min_keycode).saturating_add(1);
        let reply = xproto::get_keyboard_mapping(&conn, min_keycode, count)
            .map_err(|e| format!("failed to request keyboard mapping: {}", e))?
            .reply()
            .map_err(|e| format!("failed to get keyboard mapping reply: {}", e))?;

        let mut keysym_to_keycode = std::collections::HashMap::new();
        let per_keycode = reply.keysyms_per_keycode as usize;
        if per_keycode > 0 {
            for (idx, chunk) in reply.keysyms.chunks(per_keycode).enumerate() {
                let keycode = (min_keycode as usize + idx) as u8;
                for &sym in chunk {
                    if sym != 0 {
                        keysym_to_keycode.entry(sym).or_insert(keycode);
                    }
                }
            }
        }

        Ok(Self {
            conn,
            root,
            width,
            height,
            keysym_to_keycode,
        })
    }

    pub fn get_viewport_dimensions(&self) -> Result<(u32, u32), String> {
        Ok((self.width, self.height))
    }
}

#[cfg(target_os = "linux")]
impl InputBackend for X11RbBackend {
    fn key(&self, vk: u16, is_down: bool, _extended: bool) -> Result<(), String> {
        use x11rb::connection::Connection;
        use x11rb::protocol::{xproto, xtest};

        let sym = vk_to_x_keysym(vk)
            .ok_or_else(|| format!("unmapped virtual key: 0x{:02X}", vk))?;
        let keycode = self.keysym_to_keycode.get(&sym).copied()
            .ok_or_else(|| format!("unmapped keysym: 0x{:04X}", sym))?;

        xtest::fake_input(
            &self.conn,
            if is_down { xproto::KEY_PRESS_EVENT } else { xproto::KEY_RELEASE_EVENT },
            keycode,
            0,
            x11rb::NONE,
            0,
            0,
            0,
        ).map_err(|e| format!("xtest fake_input key failed: {}", e))?;
        self.conn.flush().map_err(|e| format!("x11 flush failed: {}", e))?;
        Ok(())
    }

    fn button(&self, button: MouseButton, is_down: bool) -> Result<(), String> {
        use x11rb::connection::Connection;
        use x11rb::protocol::{xproto, xtest};

        let btn = mouse_button_to_x11_button(button);
        xtest::fake_input(
            &self.conn,
            if is_down { xproto::BUTTON_PRESS_EVENT } else { xproto::BUTTON_RELEASE_EVENT },
            btn,
            0,
            x11rb::NONE,
            0,
            0,
            0,
        ).map_err(|e| format!("xtest fake_input button failed: {}", e))?;
        self.conn.flush().map_err(|e| format!("x11 flush failed: {}", e))?;
        Ok(())
    }

    fn wheel(&self, dx: i32, dy: i32) -> Result<(), String> {
        use x11rb::connection::Connection;
        use x11rb::protocol::{xproto, xtest};

        let (x_pulse, y_pulse) = mouse_wheel_to_x11_pulses(dx, dy);

        if let Some((btn, count)) = y_pulse {
            for _ in 0..count {
                xtest::fake_input(&self.conn, xproto::BUTTON_PRESS_EVENT, btn, 0, x11rb::NONE, 0, 0, 0)
                    .map_err(|e| format!("x11 wheel scroll {} press failed: {}", if dy > 0 { "up" } else { "down" }, e))?;
                xtest::fake_input(&self.conn, xproto::BUTTON_RELEASE_EVENT, btn, 0, x11rb::NONE, 0, 0, 0)
                    .map_err(|e| format!("x11 wheel scroll {} release failed: {}", if dy > 0 { "up" } else { "down" }, e))?;
            }
        }

        if let Some((btn, count)) = x_pulse {
            for _ in 0..count {
                xtest::fake_input(&self.conn, xproto::BUTTON_PRESS_EVENT, btn, 0, x11rb::NONE, 0, 0, 0)
                    .map_err(|e| format!("x11 wheel scroll {} press failed: {}", if dx > 0 { "right" } else { "left" }, e))?;
                xtest::fake_input(&self.conn, xproto::BUTTON_RELEASE_EVENT, btn, 0, x11rb::NONE, 0, 0, 0)
                    .map_err(|e| format!("x11 wheel scroll {} release failed: {}", if dx > 0 { "right" } else { "left" }, e))?;
            }
        }

        self.conn.flush().map_err(|e| format!("x11 flush failed: {}", e))?;
        Ok(())
    }

    fn move_relative(&self, dx: i32, dy: i32) -> Result<(), String> {
        use x11rb::protocol::xproto;

        let reply = xproto::query_pointer(&self.conn, self.root)
            .map_err(|e| format!("x11 query_pointer failed: {}", e))?
            .reply()
            .map_err(|e| format!("x11 query_pointer reply failed: {}", e))?;
        let new_x = reply.root_x as i32 + dx;
        let new_y = reply.root_y as i32 + dy;
        self.warp(new_x, new_y)
    }

    fn warp(&self, x: i32, y: i32) -> Result<(), String> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto;

        xproto::warp_pointer(
            &self.conn,
            x11rb::NONE,
            self.root,
            0,
            0,
            0,
            0,
            x as i16,
            y as i16,
        ).map_err(|e| format!("x11 warp_pointer failed: {}", e))?;
        self.conn.flush().map_err(|e| format!("x11 flush failed: {}", e))?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
static LINUX_BACKEND: parking_lot::RwLock<Option<Result<std::sync::Arc<X11RbBackend>, String>>> =
    parking_lot::RwLock::new(None);

#[cfg(target_os = "linux")]
pub fn get_linux_backend() -> Result<std::sync::Arc<X11RbBackend>, String> {
    {
        let guard = LINUX_BACKEND.read();
        if let Some(ref res) = *guard {
            return res.clone();
        }
    }
    let mut guard = LINUX_BACKEND.write();
    if let Some(ref res) = *guard {
        return res.clone();
    }
    let res = match X11RbBackend::new() {
        Ok(b) => Ok(std::sync::Arc::new(b)),
        Err(e) => {
            update_kvm_runtime_state("x11rb", &e);
            Err(e)
        }
    };
    *guard = Some(res.clone());
    res
}

static HELD_KEYS: parking_lot::Mutex<Option<std::collections::HashSet<u16>>> = parking_lot::Mutex::new(None);
static HELD_BUTTONS: parking_lot::Mutex<Option<std::collections::HashSet<u8>>> = parking_lot::Mutex::new(None);

fn track_key(vk: u16, is_down: bool) {
    let mut guard = HELD_KEYS.lock();
    let set = guard.get_or_insert_with(std::collections::HashSet::new);
    if is_down {
        set.insert(vk);
    } else {
        set.remove(&vk);
    }
}

fn track_button(button: MouseButton, is_down: bool) {
    let id = match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
    };
    let mut guard = HELD_BUTTONS.lock();
    let set = guard.get_or_insert_with(std::collections::HashSet::new);
    if is_down {
        set.insert(id);
    } else {
        set.remove(&id);
    }
}

/// Native platform input injection implementation with pre-login and lock screen desktop synchronization.
pub struct NativeInputInjector;

impl NativeInputInjector {
    pub fn key_event(vk: u16, is_down: bool, extended: bool) {
        record_injection_timestamp();
        track_key(vk, is_down);
        Self::platform_key_event(vk, is_down, extended);
    }

    /// True while Ctrl or Cmd (Meta) is held on this computer.
    pub fn ctrl_or_meta_held() -> bool {
        HELD_KEYS
            .lock()
            .as_ref()
            .map(|set| {
                set.iter()
                    .any(|vk| matches!(*vk, 0x11 | 0x5B | 0x5C | 0xA2 | 0xA3))
            })
            .unwrap_or(false)
    }

    pub fn mouse_button(button: MouseButton, is_down: bool) {
        record_injection_timestamp();
        track_button(button, is_down);
        Self::platform_mouse_button(button, is_down);
    }

    pub fn release_all_keys() {
        crate::keylayout::reset();
        crate::inputmethod::reset();
        let keys_to_release: Vec<u16> = {
            let mut guard = HELD_KEYS.lock();
            if let Some(ref mut set) = *guard {
                set.drain().collect()
            } else {
                Vec::new()
            }
        };
        for vk in keys_to_release {
            Self::key_event(vk, false, false);
        }

        // Proactively release common modifiers to prevent stuck modifiers
        // 0x10 VK_SHIFT, 0x11 VK_CONTROL, 0x12 VK_MENU (Alt), 0x5B VK_LWIN, 0x5C VK_RWIN
        // 0xA0 VK_LSHIFT, 0xA1 VK_RSHIFT, 0xA2 VK_LCONTROL, 0xA3 VK_RCONTROL, 0xA4 VK_LMENU, 0xA5 VK_RMENU
        const COMMON_MODIFIERS: &[u16] = &[
            0x10, 0x11, 0x12, 0x5B, 0x5C, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5,
        ];
        for &vk in COMMON_MODIFIERS {
            Self::key_event(vk, false, false);
        }

        let btns_to_release: Vec<u8> = {
            let mut guard = HELD_BUTTONS.lock();
            if let Some(ref mut set) = *guard {
                set.drain().collect()
            } else {
                Vec::new()
            }
        };
        for b in btns_to_release {
            let btn = match b {
                1 => MouseButton::Right,
                2 => MouseButton::Middle,
                _ => MouseButton::Left,
            };
            Self::mouse_button(btn, false);
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub fn recenter_local_cursor() {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let _ = backend.warp(960, 540);
            return;
        }
        let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
        Self::recenter_cursor(vx, vy, vw, vh);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn recenter_local_cursor() {
        let (w, h) = {
            #[cfg(target_os = "linux")]
            {
                get_linux_backend()
                    .ok()
                    .and_then(|b| b.get_viewport_dimensions().ok())
                    .unwrap_or((1920, 1080))
            }
            #[cfg(not(target_os = "linux"))]
            {
                (1920, 1080)
            }
        };
        Self::recenter_cursor(0, 0, w as i32, h as i32);
    }

    #[cfg(windows)]
    pub fn recenter_cursor(vx: i32, vy: i32, vw: i32, vh: i32) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            Self::recenter_cursor_now(vx, vy, vw, vh)
        });
    }

#[cfg(windows)]
    fn recenter_cursor_now(vx: i32, vy: i32, vw: i32, vh: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let _ = backend.warp(vx + vw / 2, vy + vh / 2);
            return;
        }
        use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
        unsafe {
            if let Err(e) = SetCursorPos(vx + vw / 2, vy + vh / 2) {
                log::warn!("[KVM] SetCursorPos failed: {} {}", e, Self::desktop_probe());
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn recenter_cursor(vx: i32, vy: i32, vw: i32, vh: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let _ = backend.warp(vx + vw / 2, vy + vh / 2);
            return;
        }
        use macos_ffi::*;
        let center_pt = CGPoint {
            x: (vx + vw / 2) as f64,
            y: (vy + vh / 2) as f64,
        };
        unsafe {
            let _ = CGWarpMouseCursorPosition(center_pt);
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn recenter_cursor(vx: i32, vy: i32, vw: i32, vh: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let res = backend.warp(vx + vw / 2, vy + vh / 2);
            handle_injection_result(InjectionErrorKind::Warp, res);
            return;
        }
        #[cfg(target_os = "linux")]
        {
            match get_linux_backend() {
                Ok(backend) => {
                    let res = backend.warp(vx + vw / 2, vy + vh / 2);
                    handle_injection_result(InjectionErrorKind::Warp, res);
                }
                Err(e) => {
                    handle_injection_result(InjectionErrorKind::Warp, Err(e));
                }
            }
        }
    }
    /// Absolute pointer warp. Runs on the thread that can actually move the pointer.
    #[cfg(windows)]
    pub fn warp_cursor(x: i32, y: i32) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            if let Some(backend) = get_custom_backend() {
                let _ = backend.warp(x, y);
                return;
            }
            use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
            unsafe {
                if let Err(e) = SetCursorPos(x, y) {
                    log::warn!("[KVM] SetCursorPos({}, {}) failed: {}", x, y, e);
                }
            }
        });
    }

    #[cfg(target_os = "macos")]
    pub fn warp_cursor(x: i32, y: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let _ = backend.warp(x, y);
            return;
        }
        use macos_ffi::*;
        unsafe {
            let _ = CGWarpMouseCursorPosition(CGPoint { x: x as f64, y: y as f64 });
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn warp_cursor(x: i32, y: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let res = backend.warp(x, y);
            handle_injection_result(InjectionErrorKind::Warp, res);
            return;
        }
        #[cfg(target_os = "linux")]
        {
            match get_linux_backend() {
                Ok(backend) => {
                    let res = backend.warp(x, y);
                    handle_injection_result(InjectionErrorKind::Warp, res);
                }
                Err(e) => handle_injection_result(InjectionErrorKind::Warp, Err(e)),
            }
        }
    }

    /// Put this computer's pointer where the other computer's pointer left its screen:
    /// on the edge facing it, `frac` of the way along that edge.
    ///
    /// The driving computer starts counting movement from the point it crossed, and
    /// returns control once the movement comes back to that point. Both computers have
    /// to agree on where the pointer started, otherwise the pointer appears in the
    /// middle of the screen and going back needs an arbitrary amount of travel.
    #[cfg(any(windows, target_os = "macos"))]
    pub fn place_cursor_at_entry_edge(side: &str, frac: f64) {
        let frac = frac.clamp(0.0, 1.0);
        let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
        if vw <= 1 || vh <= 1 {
            return;
        }
        let along_y = vy + (frac * (vh - 1) as f64).round() as i32;
        let along_x = vx + (frac * (vw - 1) as f64).round() as i32;
        let (x, y) = match side {
            "left" => (vx + 1, along_y),
            "right" => (vx + vw - 2, along_y),
            "top" => (along_x, vy + 1),
            "bottom" => (along_x, vy + vh - 2),
            _ => return,
        };
        Self::warp_cursor(x, y);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn place_cursor_at_entry_edge(side: &str, frac: f64) {
        let frac = frac.clamp(0.0, 1.0);
        #[cfg(target_os = "linux")]
        let (vw, vh) = get_linux_backend()
            .ok()
            .and_then(|b| b.get_viewport_dimensions().ok())
            .unwrap_or((1920, 1080));
        #[cfg(not(target_os = "linux"))]
        let (vw, vh) = (1920u32, 1080u32);
        let (vw, vh) = (vw as i32, vh as i32);
        let along_y = (frac * (vh - 1) as f64).round() as i32;
        let along_x = (frac * (vw - 1) as f64).round() as i32;
        let (x, y) = match side {
            "left" => (1, along_y),
            "right" => (vw - 2, along_y),
            "top" => (along_x, 1),
            "bottom" => (along_x, vh - 2),
            _ => return,
        };
        Self::warp_cursor(x, y);
    }

    /// Pull the pointer back onto a display if it was pushed just past an edge.
    ///
    /// The driven computer follows the other computer's pointer, and the last few pixels
    /// of movement can land outside the screen. When control comes back here the pointer
    /// has to be somewhere the user can see it.
    pub fn keep_cursor_visible() {
        let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
        if vw <= 2 || vh <= 2 {
            return;
        }
        let Some((x, y)) = Self::cursor_position() else {
            return;
        };
        let min_x = vx as f64 + 1.0;
        let max_x = (vx + vw - 2) as f64;
        let min_y = vy as f64 + 1.0;
        let max_y = (vy + vh - 2) as f64;
        if x < min_x || x > max_x || y < min_y || y > max_y {
            let cx = x.clamp(min_x, max_x).round() as i32;
            let cy = y.clamp(min_y, max_y).round() as i32;
            log::info!("[KVM] Pulling pointer back on screen: ({:.0}, {:.0}) -> ({}, {})", x, y, cx, cy);
            Self::warp_cursor(cx, cy);
        }
    }

    #[cfg(windows)]
    fn cursor_position() -> Option<(f64, f64)> {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() {
                Some((p.x as f64, p.y as f64))
            } else {
                None
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn cursor_position() -> Option<(f64, f64)> {
        use macos_ffi::*;
        unsafe {
            let event = CGEventCreate(std::ptr::null_mut());
            if event.is_null() {
                return None;
            }
            let loc = CGEventGetLocation(event);
            CFRelease(event as _);
            Some((loc.x, loc.y))
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn cursor_position() -> Option<(f64, f64)> {
        None
    }

    #[cfg(windows)]
    pub fn sync_input_desktop() {
        // Synthetic input is marshalled onto the main thread (see `main_thread_input`),
        // which is already attached to the interactive desktop. Re-assigning the desktop
        // there is neither possible nor necessary: SetThreadDesktop fails for a thread
        // that owns windows, and the handle comparison below would never match because
        // every call returns a fresh handle.
    }

    #[allow(dead_code)]
    #[cfg(not(windows))]
    pub fn sync_input_desktop() {}

    #[cfg(windows)]
    pub fn move_cursor_relative(dx: i32, dy: i32) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            Self::move_cursor_relative_now(dx, dy)
        });
    }

#[cfg(windows)]
    fn move_cursor_relative_now(dx: i32, dy: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let _ = backend.move_relative(dx, dy);
            return;
        }
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
                    dwExtraInfo: 0x474C4944,
                },
            },
        };

        unsafe {
            let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if sent != 1 {
                log::warn!(
                    "[KVM] SendInput mouse move failed: dx={} dy={} sent={} err={} {}",
                    dx, dy, sent, std::io::Error::last_os_error(), Self::desktop_probe()
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn move_cursor_relative(dx: i32, dy: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let _ = backend.move_relative(dx, dy);
            return;
        }
        hook_engine::prompt_for_accessibility_once();
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
    pub fn move_cursor_relative(dx: i32, dy: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let res = backend.move_relative(dx, dy);
            handle_injection_result(InjectionErrorKind::MoveRelative, res);
            return;
        }
        #[cfg(target_os = "linux")]
        {
            match get_linux_backend() {
                Ok(backend) => {
                    let res = backend.move_relative(dx, dy);
                    handle_injection_result(InjectionErrorKind::MoveRelative, res);
                }
                Err(e) => {
                    handle_injection_result(InjectionErrorKind::MoveRelative, Err(e));
                }
            }
        }
    }

    #[cfg(windows)]
    pub fn platform_mouse_button(button: MouseButton, is_down: bool) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            Self::platform_mouse_button_now(button, is_down)
        });
    }

#[cfg(windows)]
    fn platform_mouse_button_now(button: MouseButton, is_down: bool) {
        if let Some(backend) = get_custom_backend() {
            let res = backend.button(button, is_down);
            handle_injection_result(InjectionErrorKind::Button, res);
            return;
        }
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
            let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if sent != 1 {
                log::warn!(
                    "[KVM] SendInput mouse button failed: {:?} down={} sent={} err={} {}",
                    button, is_down, sent, std::io::Error::last_os_error(), Self::desktop_probe()
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn platform_mouse_button(button: MouseButton, is_down: bool) {
        if let Some(backend) = get_custom_backend() {
            let res = backend.button(button, is_down);
            handle_injection_result(InjectionErrorKind::Button, res);
            return;
        }
        hook_engine::prompt_for_accessibility_once();
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
    pub fn platform_mouse_button(button: MouseButton, is_down: bool) {
        if let Some(backend) = get_custom_backend() {
            let res = backend.button(button, is_down);
            handle_injection_result(InjectionErrorKind::Button, res);
            return;
        }
        #[cfg(target_os = "linux")]
        {
            match get_linux_backend() {
                Ok(backend) => {
                    let res = backend.button(button, is_down);
                    handle_injection_result(InjectionErrorKind::Button, res);
                }
                Err(e) => {
                    handle_injection_result(InjectionErrorKind::Button, Err(e));
                }
            }
        }
    }

    #[cfg(windows)]
    pub fn mouse_wheel(dx: i32, dy: i32) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            Self::mouse_wheel_now(dx, dy)
        });
    }

#[cfg(windows)]
    fn mouse_wheel_now(dx: i32, dy: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let res = backend.wheel(dx, dy);
            handle_injection_result(InjectionErrorKind::Wheel, res);
            return;
        }
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
            let expected = inputs.len() as u32;
            unsafe {
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent != expected {
                    log::warn!(
                        "[KVM] SendInput wheel failed: dx={} dy={} sent={}/{} err={} {}",
                        dx, dy, sent, expected, std::io::Error::last_os_error(),
                        Self::desktop_probe()
                    );
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn mouse_wheel(dx: i32, dy: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let res = backend.wheel(dx, dy);
            handle_injection_result(InjectionErrorKind::Wheel, res);
            return;
        }
        hook_engine::prompt_for_accessibility_once();
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
    pub fn mouse_wheel(dx: i32, dy: i32) {
        record_injection_timestamp();
        if let Some(backend) = get_custom_backend() {
            let res = backend.wheel(dx, dy);
            handle_injection_result(InjectionErrorKind::Wheel, res);
            return;
        }
        #[cfg(target_os = "linux")]
        {
            match get_linux_backend() {
                Ok(backend) => {
                    let res = backend.wheel(dx, dy);
                    handle_injection_result(InjectionErrorKind::Wheel, res);
                }
                Err(e) => {
                    handle_injection_result(InjectionErrorKind::Wheel, Err(e));
                }
            }
        }
    }

    /// Which desktop this thread is attached to versus the input desktop. Only used in
    /// diagnostics: SendInput fails with "access is denied" when they differ.
    #[cfg(windows)]
    fn desktop_probe() -> String {
        use windows::Win32::System::StationsAndDesktops::{
            CloseDesktop, GetThreadDesktop, OpenInputDesktop, DESKTOP_ACCESS_FLAGS,
            DESKTOP_CONTROL_FLAGS,
        };
        use windows::Win32::System::Threading::GetCurrentThreadId;
        unsafe {
            let thread = match GetThreadDesktop(GetCurrentThreadId()) {
                Ok(h) => format!("{:?}", h),
                Err(e) => format!("err({:?})", e),
            };
            let (input, same) = match OpenInputDesktop(
                DESKTOP_CONTROL_FLAGS(0),
                false,
                DESKTOP_ACCESS_FLAGS(0x0001 | 0x0080 | 0x0100),
            ) {
                Ok(h) => {
                    let same = match GetThreadDesktop(GetCurrentThreadId()) {
                        Ok(t) => t == h,
                        Err(_) => false,
                    };
                    let _ = CloseDesktop(h);
                    (format!("{:?}", h), same)
                }
                Err(e) => (format!("err({:?})", e), false),
            };
            format!("thread={} input={} same={}", thread, input, same)
        }
    }

    #[cfg(windows)]
    pub fn platform_key_event(vk: u16, is_down: bool, extended: bool) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            Self::platform_key_event_now(vk, is_down, extended)
        });
    }

#[cfg(windows)]
    fn platform_key_event_now(vk: u16, is_down: bool, extended: bool) {
        if let Some(backend) = get_custom_backend() {
            let res = backend.key(vk, is_down, extended);
            handle_injection_result(InjectionErrorKind::Key, res);
            return;
        }
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
                    // Marked as ours so this app's own hook passes it through instead of
                    // forwarding it back to the other computer.
                    dwExtraInfo: 0x474C4944,
                },
            },
        };

        unsafe {
            let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if sent != 1 {
                log::warn!(
                    "[KVM] SendInput keyboard failed: vk={} down={} sent={} err={} {}",
                    vk, is_down, sent, std::io::Error::last_os_error(), Self::desktop_probe()
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn platform_key_event(vk: u16, is_down: bool, extended: bool) {
        if let Some(backend) = get_custom_backend() {
            let res = backend.key(vk, is_down, extended);
            handle_injection_result(InjectionErrorKind::Key, res);
            return;
        }
        hook_engine::prompt_for_accessibility_once();
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
    pub fn platform_key_event(vk: u16, is_down: bool, extended: bool) {
        if let Some(backend) = get_custom_backend() {
            let res = backend.key(vk, is_down, extended);
            handle_injection_result(InjectionErrorKind::Key, res);
            return;
        }
        #[cfg(target_os = "linux")]
        {
            match get_linux_backend() {
                Ok(backend) => {
                    let res = backend.key(vk, is_down, extended);
                    handle_injection_result(InjectionErrorKind::Key, res);
                }
                Err(e) => {
                    handle_injection_result(InjectionErrorKind::Key, Err(e));
                }
            }
        }
    }
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
    state: AppState,
    last_text: Arc<Mutex<String>>,
    last_image_len: Arc<Mutex<usize>>,
}

impl ClipboardWatcher {
    pub fn new(network: NetworkManager, state: AppState) -> Self {
        Self {
            network,
            state,
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

                // Respect clipboard master switch: stop polling and broadcasting when disabled
                if !self.state.inner.read().clipboard.enabled || !self.state.inner.read().clipboard.share {
                    continue;
                }

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
                            text: Some(text),
                            ts: Some(chrono::Utc::now().timestamp_millis() as u64),
                            ..Default::default()
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
                                ts: Some(chrono::Utc::now().timestamp_millis() as u64),
                                png: Some(b64),
                                w: Some(w),
                                height: Some(h),
                                bytes: Some(len),
                                parts: Some(1),
                                shot: Some(true),
                                ..Default::default()
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
/// Windows only: synthetic input is marshalled onto the Tauri main thread.
///
/// `SendInput` (and `SetCursorPos`) fail with `ERROR_ACCESS_DENIED` on threads that
/// are not attached to the interactive desktop. Tauri's async worker threads and the
/// low-level hook thread are not, so every injection started there is posted to the
/// main thread first, which is known to work.
#[cfg(windows)]
pub mod main_thread_input {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use tauri::AppHandle;

    static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

    /// Called once during startup, from the main thread.
    pub fn register(app: AppHandle) {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        let id = unsafe { GetCurrentThreadId() };
        MAIN_THREAD_ID.store(id, Ordering::SeqCst);
        *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
        log::debug!("[KVM] input injection pinned to main thread {}", id);
    }

    pub fn is_main_thread() -> bool {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        let main = MAIN_THREAD_ID.load(Ordering::SeqCst);
        main == 0 || unsafe { GetCurrentThreadId() } == main
    }

    /// Run `f` on the main thread, inline when already there.
    pub fn run<F: FnOnce() + Send + 'static>(f: F) {
        if is_main_thread() {
            f();
            return;
        }
        let app = APP
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match app {
            Some(app) => {
                if let Err(e) = app.run_on_main_thread(f) {
                    log::warn!("[KVM] could not post input to the main thread: {}", e);
                }
            }
            None => f(),
        }
    }
}

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
    static mut LAST_PT: POINT = POINT { x: 0, y: 0 };
    static mut VIRTUAL_REMOTE_X: f64 = 0.0;
    static mut VIRTUAL_REMOTE_Y: f64 = 0.0;

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
        set_kvm_app_state(state.clone());
        super::main_thread_input::register(app.clone());
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
                match mouse_hook {
                    Ok(h) => MOUSE_HOOK = Some(h),
                    Err(e) => log::error!("[KVM] mouse hook install failed: {}", e),
                }

                let key_hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_hook_proc),
                    None,
                    0,
                );
                match key_hook {
                    Ok(h) => KEYBOARD_HOOK = Some(h),
                    Err(e) => log::error!("[KVM] keyboard hook install failed: {}", e),
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

            // Bypass hook for inputs injected by GlideDesk itself
            if m.dwExtraInfo == 0x474C4944 {
                return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
            }

            if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
                let snap = state.snapshot();
                let has_peer = snap.client.is_some() || snap.peer.is_some() || snap.link.state == "connected";
                let is_driving = snap.input == "driving";

                let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
                let max_x = vx + vw;
                let max_y = vy + vh;

                let sharing_enabled = snap.kvm.mouse && snap.kvm.linked && snap.kvm.drive_other && !snap.kvm_state.paused;
                if !is_driving && has_peer && sharing_enabled {
                    // Edge check across the entire virtual multi-monitor desktop
                    let crossed = match snap.layout.side.as_str() {
                        "right" => m.pt.x >= max_x - 2,
                        "left" => m.pt.x <= vx + 1,
                        "top" => m.pt.y <= vy + 1,
                        "bottom" => m.pt.y >= max_y - 2,
                        _ => false,
                    };

                    if crossed {
                        LAST_PT = m.pt;
                        VIRTUAL_REMOTE_X = 0.0;
                        VIRTUAL_REMOTE_Y = 0.0;
                        log::info!("[KVM] Cursor crossed to peer: side={}, pt=({}, {})", snap.layout.side, m.pt.x, m.pt.y);
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
                            edge_at: Some(entry_fraction(
                                &snap.layout.side,
                                m.pt.x as f64,
                                m.pt.y as f64,
                                vx,
                                vy,
                                vw,
                                vh,
                            )),
                            ..Default::default()
                        };
                        net.send_packet_sync(take);
                        return LRESULT(1);
                    }
                } else if is_driving {
                    // Swallow and stream to peer
                    match msg {
                        WM_MOUSEMOVE => {
                            let dx = (m.pt.x - LAST_PT.x) as f64;
                            let dy = (m.pt.y - LAST_PT.y) as f64;

                            VIRTUAL_REMOTE_X += dx;
                            VIRTUAL_REMOTE_Y += dy;

                            let returned = match snap.layout.side.as_str() {
                                "right" => VIRTUAL_REMOTE_X < -8.0,
                                "left" => VIRTUAL_REMOTE_X > 8.0,
                                "top" => VIRTUAL_REMOTE_Y > 8.0,
                                "bottom" => VIRTUAL_REMOTE_Y < -8.0,
                                _ => false,
                            };

                            if returned {
                                log::info!("[KVM] Cursor returned back to home screen: side={}, virtual_x={:.1}", snap.layout.side, VIRTUAL_REMOTE_X);
                                state.set_where_input("home");
                                NativeInputInjector::release_all_keys();
                                let new_snap = state.snapshot();
                                let _ = app.emit("state-change", &new_snap);

                                let home_msg = WireMessage {
                                    t: "kvm.home".to_string(),
                                    v: Some(1),
                                    ..Default::default()
                                };
                                net.send_packet_sync(home_msg);

                                let return_x = match snap.layout.side.as_str() {
                                    "right" => max_x - 12,
                                    "left" => vx + 12,
                                    _ => m.pt.x,
                                };
                                let return_y = match snap.layout.side.as_str() {
                                    "top" => vy + 12,
                                    "bottom" => max_y - 12,
                                    _ => m.pt.y,
                                };
                                // SetCursorPos from this hook thread fails (the thread is not
                                // attached to the interactive desktop), which used to leave the
                                // pointer sitting on the edge so the next push produced no
                                // movement event and the edge never re-armed.
                                NativeInputInjector::warp_cursor(return_x, return_y);
                                LAST_PT = POINT { x: return_x, y: return_y };
                                return LRESULT(1);
                            }

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
                            NativeInputInjector::warp_cursor(park_x, park_y);
                            LAST_PT = POINT { x: park_x, y: park_y };
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
                            let (dx, dy) =
                                wheel_notches_from_windows(msg == WM_MOUSEWHEEL, m.mouseData);
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

            // Pass on keys this app injected itself, otherwise remote keystrokes would be
            // forwarded back to the computer they came from.
            if k.dwExtraInfo == 0x474C4944 {
                return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
            }

            if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
                let snap = state.snapshot();
                let is_driving = snap.input == "driving";

                // Hotkey to release: Double-tap VK_RMENU (Right Alt, 165) or VK_RCONTROL (163)
                if (k.vkCode == 165 || k.vkCode == 163) && (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) {
                    if is_driving {
                        static mut LAST_WIN_EMERGENCY_TAP: Option<std::time::Instant> = None;
                        let is_double_tap = unsafe {
                            if let Some(prev) = LAST_WIN_EMERGENCY_TAP {
                                if prev.elapsed() < std::time::Duration::from_millis(400) {
                                    LAST_WIN_EMERGENCY_TAP = None;
                                    true
                                } else {
                                    LAST_WIN_EMERGENCY_TAP = Some(std::time::Instant::now());
                                    false
                                }
                            } else {
                                LAST_WIN_EMERGENCY_TAP = Some(std::time::Instant::now());
                                false
                            }
                        };

                        if is_double_tap {
                            state.set_where_input("home");
                            NativeInputInjector::release_all_keys();
                            let new_snap = state.snapshot();
                            let _ = app.emit("state-change", &new_snap);

                            let home_msg = WireMessage {
                                t: "kvm.home".to_string(),
                                v: Some(1),
                                ..Default::default()
                            };
                            net.send_packet_sync(home_msg);

                            // Center cursor on local virtual desktop
                            NativeInputInjector::recenter_local_cursor();
                            return LRESULT(1);
                        }
                    }
                }

                if is_driving {
                    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                    log::debug!("[KVM] forward key vk={} down={}", k.vkCode, is_down);
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
    static mut VIRTUAL_REMOTE_X: f64 = 0.0;
    static mut VIRTUAL_REMOTE_Y: f64 = 0.0;

    /// Is this app allowed to control the computer? Never shows anything to the user.
    pub fn has_accessibility_permission() -> bool {
        unsafe { AXIsProcessTrustedWithOptions(std::ptr::null_mut()) != 0 }
    }

    /// Asks the system to show its "would like to control this computer" prompt.
    ///
    /// That prompt is modal, has no close button, and appears once per launch, so we only
    /// call this when the user actually tries to forward input — at launch it would just be
    /// in the way, with nothing to act on yet.
    pub fn ensure_accessibility_permission() -> bool {
        unsafe {
            // First check if already trusted WITHOUT prompting
            if AXIsProcessTrustedWithOptions(std::ptr::null_mut()) != 0 {
                return true;
            }

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

    /// Asks for Accessibility at most once per launch, and only when input has to be
    /// forwarded. The system prompt is modal with no close button, so opening it at launch,
    /// before the user has asked for anything, only gets in the way.
    pub fn prompt_for_accessibility_once() {
        use std::sync::Once;
        static ASKED: Once = Once::new();
        ASKED.call_once(|| {
            if !has_accessibility_permission() {
                log::warn!("GlideDesk: forwarding input needs macOS Accessibility; asking for it now.");
                let _ = ensure_accessibility_permission();
            }
        });
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
        // The hook thread cannot ask for the input method itself (macOS 27 traps a process
        // that queries text-input services off the main thread), so the main thread reads it
        // and the hook only copies the cached value. Twice a second is enough to follow
        // someone switching between input methods.
        {
            let cache_app = app.clone();
            std::thread::spawn(move || loop {
                let _ = cache_app.run_on_main_thread(crate::inputmethod::refresh_cached);
                std::thread::sleep(std::time::Duration::from_millis(500));
            });
        }

        set_kvm_app_state(state.clone());
        unsafe {
            APP_HANDLE = Some(app);
            APP_STATE = Some(state);
            NETWORK_MGR = Some(network);
        }

        std::thread::spawn(|| {
            if !has_accessibility_permission() {
                log::warn!(
                    "GlideDesk: macOS Accessibility permission is off, so keyboard and mouse \
                     forwarding is paused. Turn GlideDesk on in System Settings > Privacy & \
                     Security > Accessibility; forwarding starts by itself once it is on, no \
                     restart needed."
                );
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
                    // Fast, silent check: wait until process is trusted before attempting to create event tap
                    if AXIsProcessTrustedWithOptions(std::ptr::null_mut()) == 0 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }

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

                        // If running interactively, bring the main window forward now that permissions are active
                        let is_daemon = std::env::args().any(|a| a == "--daemon" || a == "--headless" || a == "--silent");
                        if !is_daemon {
                            if let Some(app) = &APP_HANDLE {
                                crate::platform::set_window_visible_policy(app, true);
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.unminimize();
                                    let _ = window.set_focus();
                                }
                            }
                        }

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

    /// The character this key event typed, per the OS layout (macOS).
    #[cfg(target_os = "macos")]
    fn mac_event_char(event: macos_ffi::CGEventRef, vk: u16) -> Option<char> {
        let mut buf = [0u16; crate::keylayout::MAX_CHARS];
        let mut len: u64 = 0;
        unsafe {
            macos_ffi::CGEventKeyboardGetUnicodeString(
                event,
                buf.len() as u64,
                &mut len,
                buf.as_mut_ptr(),
            );
        }
        let n = (len as usize).min(buf.len());
        if n > 0 {
            let text = String::from_utf16_lossy(&buf[..n]);
            if let Some(c) = crate::keylayout::clean_text(&text)
                .as_deref()
                .and_then(crate::keylayout::single_char)
            {
                return Some(c);
            }
        }
        // No unicode string (some synthetic/system events): fall back to the US table.
        let shifted = unsafe { macos_ffi::CGEventGetFlags(event) } & macos_ffi::kCGEventFlagMaskShift != 0;
        if shifted {
            crate::keylayout::us_shift_char(vk)
        } else {
            crate::keylayout::us_char(vk)
        }
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

        // Input this app injected (input forwarded from the other computer) is not the
        // local user. Without this the pointer, parked on the edge by the other computer,
        // counts as a fresh edge crossing and both computers start driving each other.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_injection_ms = LAST_INJECTION_TIMESTAMP_MS.load(std::sync::atomic::Ordering::Relaxed);
        if !should_forward_captured_event(now_ms, last_injection_ms) {
            return event;
        }

        if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
            let snap = state.snapshot();
            let has_peer = snap.client.is_some() || snap.peer.is_some() || snap.link.state == "connected";
            let is_driving = snap.input == "driving";

            let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
            let max_x = vx + vw;
            let max_y = vy + vh;

            let sharing_enabled = snap.kvm.mouse && snap.kvm.linked && snap.kvm.drive_other && !snap.kvm_state.paused;
            if !is_driving && has_peer && sharing_enabled {
                if r#type == kCGEventMouseMoved
                    || r#type == kCGEventLeftMouseDragged
                    || r#type == kCGEventRightMouseDragged
                {
                    let pt = CGEventGetLocation(event);
                    LAST_PT = pt;

                    let crossed = check_edge_crossed(&snap.layout.side, pt.x, pt.y, vx, vy, vw, vh);

                    if crossed {
                        VIRTUAL_REMOTE_X = 0.0;
                        VIRTUAL_REMOTE_Y = 0.0;
                        log::info!("[KVM] Cursor crossed to peer: side={}, pt=({:.1}, {:.1})", snap.layout.side, pt.x, pt.y);
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
                            edge_at: Some(entry_fraction(
                                &snap.layout.side,
                                pt.x,
                                pt.y,
                                vx,
                                vy,
                                vw,
                                vh,
                            )),
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

                        VIRTUAL_REMOTE_X += dx;
                        VIRTUAL_REMOTE_Y += dy;

                        let returned = check_edge_returned(&snap.layout.side, VIRTUAL_REMOTE_X, VIRTUAL_REMOTE_Y);

                        if returned {
                            log::info!("[KVM] Cursor returned back to home screen: side={}, virtual_x={:.1}", snap.layout.side, VIRTUAL_REMOTE_X);
                            state.set_where_input("home");
                            NativeInputInjector::release_all_keys();
                            let new_snap = state.snapshot();
                            let _ = app.emit("state-change", &new_snap);

                            let home_msg = WireMessage {
                                t: "kvm.home".to_string(),
                                v: Some(1),
                                ..Default::default()
                            };
                            net.send_packet_sync(home_msg);

                            let return_pt = match snap.layout.side.as_str() {
                                "right" => CGPoint { x: (max_x - 12) as f64, y: pt.y },
                                "left" => CGPoint { x: (vx + 12) as f64, y: pt.y },
                                "top" => CGPoint { x: pt.x, y: (vy + 12) as f64 },
                                "bottom" => CGPoint { x: pt.x, y: (max_y - 12) as f64 },
                                _ => pt,
                            };
                            let _ = CGWarpMouseCursorPosition(return_pt);
                            LAST_PT = return_pt;
                            return std::ptr::null_mut();
                        }

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
                        let mut dy = CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis1) as f64;
                        let mut dx = CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis2) as f64;
                        if dy == 0.0 && dx == 0.0 {
                            // Trackpads and smooth-scrolling mice only report pixel
                            // deltas; the far side works in line notches.
                            let lines = |px: f64| {
                                if px == 0.0 {
                                    0.0
                                } else {
                                    (px / 10.0).round().max(1.0) * px.signum()
                                }
                            };
                            dy = lines(CGEventGetIntegerValueField(
                                event,
                                kCGScrollWheelEventPointDeltaAxis1,
                            ) as f64);
                            dx = lines(CGEventGetIntegerValueField(
                                event,
                                kCGScrollWheelEventPointDeltaAxis2,
                            ) as f64);
                        }
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

                        // Release hotkeys: Double-tap Right Option (61), Right Command (54), Right Control (62)
                        if (keycode == 61 || keycode == 54 || keycode == 62) && is_down {
                            static mut LAST_EMERGENCY_TAP: Option<std::time::Instant> = None;
                            let is_double_tap = unsafe {
                                if let Some(prev) = LAST_EMERGENCY_TAP {
                                    if prev.elapsed() < std::time::Duration::from_millis(400) {
                                        LAST_EMERGENCY_TAP = None;
                                        true
                                    } else {
                                        LAST_EMERGENCY_TAP = Some(std::time::Instant::now());
                                        false
                                    }
                                } else {
                                    LAST_EMERGENCY_TAP = Some(std::time::Instant::now());
                                    false
                                }
                            };

                            if is_double_tap {
                                state.set_where_input("home");
                                NativeInputInjector::release_all_keys();
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
                        log::debug!("[KVM] forward key vk={} down={}", vk, is_down);
                        // `ch`: what this key types on our layout, so shortcuts land on
                        // the right key on the other computer. `im`: the input method we
                        // are typing with, so the other side composes with its own.
                        let ch = if is_down { mac_event_char(event, vk as u16) } else { None };
                        let im = if is_down && ch.is_some() {
                            crate::inputmethod::current_cached()
                        } else {
                            None
                        };
                        let key_msg = WireMessage {
                            t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                            v: Some(1),
                            vk: Some(vk),
                            ch: ch.map(|c| c.to_string()),
                            im,
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


#[cfg(target_os = "linux")]
fn run_linux_x11_capture_loop(
    app: AppHandle,
    state: AppState,
    network: NetworkManager,
    display: Option<String>,
    session_type: Option<String>,
) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;

    let (conn, screen_num) = match x11rb::connect(display.as_deref()) {
        Ok(res) => res,
        Err(e) => {
            log::warn!("[KVM] Linux X11 input capture unavailable: {}", e);
            return;
        }
    };

    let setup = conn.setup();
    let screen = match setup.roots.get(screen_num) {
        Some(s) => s,
        None => {
            log::warn!("[KVM] Linux X11 input capture unavailable: invalid screen index");
            return;
        }
    };
    let root = screen.root;
    let width = screen.width_in_pixels as i32;
    let height = screen.height_in_pixels as i32;

    hook_engine::LINUX_CAPTURE_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    let (helper, message) = evaluate_linux_session_capability(
        display.as_deref(),
        session_type.as_deref(),
        true,
    );
    update_kvm_runtime_state(helper, message);
    log::info!("[KVM] {}", message);

    let (vx, vy, vw, vh) = (0, 0, width, height);
    let max_x = vx + vw;
    let max_y = vy + vh;

    let mut last_x = width / 2;
    let mut last_y = height / 2;
    let mut virtual_remote_x: f64 = 0.0;
    let mut virtual_remote_y: f64 = 0.0;
    let mut prev_mask: u16 = 0;
    let mut prev_keys: [u8; 32] = [0; 32];

    const BUTTON1_MASK: u16 = 1 << 8;
    const BUTTON2_MASK: u16 = 1 << 9;
    const BUTTON3_MASK: u16 = 1 << 10;
    const BUTTON4_MASK: u16 = 1 << 11;
    const BUTTON5_MASK: u16 = 1 << 12;
    const CONTROL_MASK: u16 = 1 << 2;
    const MAX_CONSECUTIVE_CAPTURE_FAILURES: u32 = 10;

    let mut consecutive_failures: u32 = 0;

    while hook_engine::LINUX_CAPTURE_RUNNING.load(std::sync::atomic::Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(8)); // ~120 Hz polling loop (XQueryPointer / XQueryKeymap)

        let query_reply = match conn.query_pointer(root) {
            Ok(cookie) => match cookie.reply() {
                Ok(r) => r,
                Err(e) => {
                    consecutive_failures += 1;
                    if classify_capture_health(consecutive_failures, MAX_CONSECUTIVE_CAPTURE_FAILURES) == CaptureHealth::Lost {
                        log::error!("[KVM] Linux input capture stopped: X server connection lost ({})", e);
                        update_kvm_runtime_state(
                            "x11rb",
                            "Linux input capture stopped: X server connection lost",
                        );
                        hook_engine::LINUX_CAPTURE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                        NativeInputInjector::release_all_keys();
                        break;
                    }
                    continue;
                }
            },
            Err(e) => {
                consecutive_failures += 1;
                if classify_capture_health(consecutive_failures, MAX_CONSECUTIVE_CAPTURE_FAILURES) == CaptureHealth::Lost {
                    log::error!("[KVM] Linux input capture stopped: X server connection lost ({})", e);
                    update_kvm_runtime_state(
                        "x11rb",
                        "Linux input capture stopped: X server connection lost",
                    );
                    hook_engine::LINUX_CAPTURE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                    NativeInputInjector::release_all_keys();
                    break;
                }
                continue;
            }
        };

        let root_x = query_reply.root_x as i32;
        let root_y = query_reply.root_y as i32;
        let mask = u16::from(query_reply.mask);

        let keymap_reply = match conn.query_keymap() {
            Ok(cookie) => match cookie.reply() {
                Ok(r) => r,
                Err(e) => {
                    consecutive_failures += 1;
                    if classify_capture_health(consecutive_failures, MAX_CONSECUTIVE_CAPTURE_FAILURES) == CaptureHealth::Lost {
                        log::error!("[KVM] Linux input capture stopped: X server connection lost ({})", e);
                        update_kvm_runtime_state(
                            "x11rb",
                            "Linux input capture stopped: X server connection lost",
                        );
                        hook_engine::LINUX_CAPTURE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                        NativeInputInjector::release_all_keys();
                        break;
                    }
                    continue;
                }
            },
            Err(e) => {
                consecutive_failures += 1;
                if classify_capture_health(consecutive_failures, MAX_CONSECUTIVE_CAPTURE_FAILURES) == CaptureHealth::Lost {
                    log::error!("[KVM] Linux input capture stopped: X server connection lost ({})", e);
                    update_kvm_runtime_state(
                        "x11rb",
                        "Linux input capture stopped: X server connection lost",
                    );
                    hook_engine::LINUX_CAPTURE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                    NativeInputInjector::release_all_keys();
                    break;
                }
                continue;
            }
        };
        let keys = keymap_reply.keys;

        // Reset failure counter on successful queries
        consecutive_failures = 0;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last_inj_ms = LAST_INJECTION_TIMESTAMP_MS.load(std::sync::atomic::Ordering::Relaxed);

        if !should_forward_captured_event(now_ms, last_inj_ms) {
            // Self-echo suppression: drop inputs injected by GlideDesk itself
            last_x = root_x;
            last_y = root_y;
            prev_mask = mask;
            prev_keys = keys;
            continue;
        }

        let snap = state.snapshot();
        let has_peer = snap.client.is_some() || snap.peer.is_some() || snap.link.state == "connected";
        let is_driving = snap.input == "driving";

        let sharing_enabled = snap.kvm.mouse && snap.kvm.linked && snap.kvm.drive_other && !snap.kvm_state.paused;

        if !is_driving && has_peer && sharing_enabled {
            let crossed = check_edge_crossed(&snap.layout.side, root_x as f64, root_y as f64, vx, vy, vw, vh);
            if crossed {
                virtual_remote_x = 0.0;
                virtual_remote_y = 0.0;
                log::info!("[KVM] Cursor crossed to peer: side={}, pt=({}, {})", snap.layout.side, root_x, root_y);

                let is_left_down = (mask & BUTTON1_MASK) != 0;
                let is_ctrl_down = (mask & CONTROL_MASK) != 0;

                if is_left_down && is_ctrl_down {
                    let _ = app.emit("window-handoff-trigger", serde_json::json!({
                        "x": root_x,
                        "y": root_y,
                        "side": snap.layout.side,
                    }));
                }

                state.set_where_input("driving");
                let new_snap = state.snapshot();
                let _ = app.emit("state-change", &new_snap);

                let take = WireMessage {
                    t: "kvm.take".to_string(),
                    v: Some(1),
                    edge_at: Some(entry_fraction(
                        &snap.layout.side,
                        root_x as f64,
                        root_y as f64,
                        vx,
                        vy,
                        vw,
                        vh,
                    )),
                    ..Default::default()
                };
                network.send_packet_sync(take);

                let park_x = match snap.layout.side.as_str() {
                    "right" => max_x - 5,
                    "left" => vx + 5,
                    _ => root_x,
                };
                let park_y = match snap.layout.side.as_str() {
                    "top" => vy + 5,
                    "bottom" => max_y - 5,
                    _ => root_y,
                };
                let _ = conn.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, park_x as i16, park_y as i16);
                let _ = conn.flush();
                last_x = park_x;
                last_y = park_y;
                prev_mask = mask;
                prev_keys = keys;
                continue;
            }
        } else if is_driving {
            if !has_peer {
                log::info!("[KVM] Peer disconnected while driving; restoring home cursor");
                state.set_where_input("home");
                NativeInputInjector::release_all_keys();
                let new_snap = state.snapshot();
                let _ = app.emit("state-change", &new_snap);
                continue;
            }

            let dx = (root_x - last_x) as f64;
            let dy = (root_y - last_y) as f64;
            virtual_remote_x += dx;
            virtual_remote_y += dy;

            let returned = check_edge_returned(&snap.layout.side, virtual_remote_x, virtual_remote_y);
            if returned {
                log::info!("[KVM] Cursor returned back to home screen: side={}, virtual_x={:.1}", snap.layout.side, virtual_remote_x);
                state.set_where_input("home");
                NativeInputInjector::release_all_keys();
                let new_snap = state.snapshot();
                let _ = app.emit("state-change", &new_snap);

                let home_msg = WireMessage {
                    t: "kvm.home".to_string(),
                    v: Some(1),
                    ..Default::default()
                };
                network.send_packet_sync(home_msg);

                let return_x = match snap.layout.side.as_str() {
                    "right" => max_x - 12,
                    "left" => vx + 12,
                    _ => root_x,
                };
                let return_y = match snap.layout.side.as_str() {
                    "top" => vy + 12,
                    "bottom" => max_y - 12,
                    _ => root_y,
                };
                let _ = conn.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, return_x as i16, return_y as i16);
                let _ = conn.flush();
                last_x = return_x;
                last_y = return_y;
                prev_mask = mask;
                prev_keys = keys;
                continue;
            }

            if dx.abs() > 0.0 || dy.abs() > 0.0 {
                network.send_motion(dx, dy);
            }

            // Keep cursor parked near edge
            let park_x = match snap.layout.side.as_str() {
                "right" => max_x - 5,
                "left" => vx + 5,
                _ => root_x.clamp(vx, max_x - 1),
            };
            let park_y = match snap.layout.side.as_str() {
                "top" => vy + 5,
                "bottom" => max_y - 5,
                _ => root_y.clamp(vy, max_y - 1),
            };
            let _ = conn.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, park_x as i16, park_y as i16);
            let _ = conn.flush();
            last_x = park_x;
            last_y = park_y;

            // Handle mouse button transitions
            let changed_mask = mask ^ prev_mask;
            if changed_mask != 0 {
                // Button 1 (Left)
                if (changed_mask & BUTTON1_MASK) != 0 {
                    let is_down = (mask & BUTTON1_MASK) != 0;
                    network.send_packet_sync(WireMessage {
                        t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                        v: Some(1),
                        b: Some(0),
                        ..Default::default()
                    });
                }
                // Button 3 (Right)
                if (changed_mask & BUTTON3_MASK) != 0 {
                    let is_down = (mask & BUTTON3_MASK) != 0;
                    network.send_packet_sync(WireMessage {
                        t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                        v: Some(1),
                        b: Some(1),
                        ..Default::default()
                    });
                }
                // Button 2 (Middle)
                if (changed_mask & BUTTON2_MASK) != 0 {
                    let is_down = (mask & BUTTON2_MASK) != 0;
                    network.send_packet_sync(WireMessage {
                        t: if is_down { "k.md".to_string() } else { "k.mu".to_string() },
                        v: Some(1),
                        b: Some(2),
                        ..Default::default()
                    });
                }
                // Button 4 (Wheel Up)
                if (changed_mask & BUTTON4_MASK) != 0 && (mask & BUTTON4_MASK) != 0 {
                    network.send_packet_sync(WireMessage {
                        t: "k.wh".to_string(),
                        v: Some(1),
                        dx: Some(0.0),
                        dy: Some(1.0),
                        ..Default::default()
                    });
                }
                // Button 5 (Wheel Down)
                if (changed_mask & BUTTON5_MASK) != 0 && (mask & BUTTON5_MASK) != 0 {
                    network.send_packet_sync(WireMessage {
                        t: "k.wh".to_string(),
                        v: Some(1),
                        dx: Some(0.0),
                        dy: Some(-1.0),
                        ..Default::default()
                    });
                }
            }

            // Handle keyboard transitions
            for byte_idx in 0..32 {
                let diff = keys[byte_idx] ^ prev_keys[byte_idx];
                if diff != 0 {
                    for bit in 0..8 {
                        if (diff & (1 << bit)) != 0 {
                            let keycode = (byte_idx * 8 + bit) as u8;
                            let is_down = (keys[byte_idx] & (1 << bit)) != 0;
                            if let Some(vk) = x_keycode_to_vk(keycode) {
                                network.send_packet_sync(WireMessage {
                                    t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                                    v: Some(1),
                                    vk: Some(vk as u32),
                                    k: Some(vk.to_string()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
        }

        if !is_driving {
            last_x = root_x;
            last_y = root_y;
        }
        prev_mask = mask;
        prev_keys = keys;
    }

    hook_engine::LINUX_CAPTURE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    NativeInputInjector::release_all_keys();
}

#[cfg(not(any(windows, target_os = "macos")))]
pub mod hook_engine {
    use super::*;

    pub static LINUX_CAPTURE_RUNNING: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    pub fn start_hooks(app: AppHandle, state: AppState, network: NetworkManager) {
        set_kvm_app_state(state.clone());
        let display = std::env::var("DISPLAY").ok();
        let session_type = std::env::var("XDG_SESSION_TYPE").ok();
        let (helper, message) = evaluate_linux_session_capability(
            display.as_deref(),
            session_type.as_deref(),
            false,
        );
        {
            let mut inner = state.inner.write();
            inner.kvm_state.helper = helper.to_string();
            inner.kvm_state.message = message.to_string();
        }
        log::info!("[KVM] {}", message);
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = get_linux_backend() {
                log::warn!("[KVM] Linux input injection capability check: {}", e);
            }

            let app_clone = app.clone();
            let state_clone = state.clone();
            let net_clone = network.clone();
            let dpy_str = display.clone();
            let sess_str = session_type.clone();

            std::thread::spawn(move || {
                run_linux_x11_capture_loop(app_clone, state_clone, net_clone, dpy_str, sess_str);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_MUTEX: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

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
        NativeInputInjector::mouse_button(MouseButton::Middle, false);
        NativeInputInjector::mouse_wheel(0, 0);
        NativeInputInjector::key_event(0x41, false, false);
    }

    #[test]
    fn test_edge_crossing_detection_math() {
        let (vx, vy, vw, vh) = (0, 0, 1920, 1080);

        // Right side: crosses when pt_x >= max_x - 2.0 (1918.0)
        assert!(check_edge_crossed("right", 1918.0, 500.0, vx, vy, vw, vh));
        assert!(check_edge_crossed("right", 1920.0, 500.0, vx, vy, vw, vh));
        assert!(!check_edge_crossed("right", 1917.9, 500.0, vx, vy, vw, vh));

        // Left side: crosses when pt_x <= vx + 1.0 (1.0)
        assert!(check_edge_crossed("left", 0.0, 500.0, vx, vy, vw, vh));
        assert!(check_edge_crossed("left", 1.0, 500.0, vx, vy, vw, vh));
        assert!(!check_edge_crossed("left", 1.1, 500.0, vx, vy, vw, vh));

        // Top side: crosses when pt_y <= vy + 1.0 (1.0)
        assert!(check_edge_crossed("top", 500.0, 0.0, vx, vy, vw, vh));
        assert!(check_edge_crossed("top", 500.0, 1.0, vx, vy, vw, vh));
        assert!(!check_edge_crossed("top", 500.0, 1.1, vx, vy, vw, vh));

        // Bottom side: crosses when pt_y >= max_y - 2.0 (1078.0)
        assert!(check_edge_crossed("bottom", 500.0, 1078.0, vx, vy, vw, vh));
        assert!(check_edge_crossed("bottom", 500.0, 1080.0, vx, vy, vw, vh));
        assert!(!check_edge_crossed("bottom", 500.0, 1077.9, vx, vy, vw, vh));

        // Unknown side never crosses
        assert!(!check_edge_crossed("unknown", 0.0, 0.0, vx, vy, vw, vh));
    }

    #[test]
    fn entry_fraction_maps_the_crossing_point_onto_the_edge() {
        // 1920x1080 screen at the origin: crossing half way down lands at 0.5.
        assert!((entry_fraction("left", 0.0, 540.0, 0, 0, 1920, 1080) - 0.5).abs() < 1e-6);
        assert!((entry_fraction("right", 1920.0, 0.0, 0, 0, 1920, 1080) - 0.0).abs() < 1e-6);
        assert!(entry_fraction("right", 1920.0, 1079.0, 0, 0, 1920, 1080) > 0.999);
        // Top/bottom edges use the horizontal position instead.
        assert!((entry_fraction("top", 960.0, 0.0, 0, 0, 1920, 1080) - 0.5).abs() < 1e-6);
        // Off-screen input is clamped instead of producing a wild position.
        assert_eq!(entry_fraction("left", 0.0, -40.0, 0, 0, 1920, 1080), 0.0);
        assert_eq!(entry_fraction("left", 0.0, 5000.0, 0, 0, 1920, 1080), 1.0);
    }

    #[test]
    fn wheel_notches_keep_the_scroll_direction() {
        // +120 is one notch up, -120 one notch down; the high word carries the sign.
        assert_eq!(wheel_notches_from_windows(true, 120 << 16), (0.0, 1.0));
        assert_eq!(
            wheel_notches_from_windows(true, ((-120i32) as u32) << 16),
            (0.0, -1.0)
        );

        // Six notches down stay negative instead of being flipped on the way out.
        let raw = ((6i32 * -120i32) as u32) << 16;
        assert_eq!(wheel_notches_from_windows(true, raw), (0.0, -6.0));

        // Horizontal wheels land on the other axis.
        assert_eq!(wheel_notches_from_windows(false, 240 << 16), (2.0, 0.0));
    }

    #[test]
    fn test_return_crossing_detection_math() {
        // Right side: peer is to the right. Returning means moving left past seam (virtual_x < -8.0)
        assert!(check_edge_returned("right", -8.5, 0.0));
        assert!(!check_edge_returned("right", -8.0, 0.0));
        assert!(!check_edge_returned("right", -5.0, 0.0));

        // Left side: peer is to the left. Returning means moving right past seam (virtual_x > 8.0)
        assert!(check_edge_returned("left", 8.5, 0.0));
        assert!(!check_edge_returned("left", 8.0, 0.0));
        assert!(!check_edge_returned("left", 5.0, 0.0));

        // Top side: peer is above. Returning means moving down past seam (virtual_y > 8.0)
        assert!(check_edge_returned("top", 0.0, 8.5));
        assert!(!check_edge_returned("top", 0.0, 8.0));

        // Bottom side: peer is below. Returning means moving up past seam (virtual_y < -8.0)
        assert!(check_edge_returned("bottom", 0.0, -8.5));
        assert!(!check_edge_returned("bottom", 0.0, -8.0));

        // Unknown side never returns
        assert!(!check_edge_returned("unknown", -100.0, -100.0));
    }

    #[test]
    fn test_native_input_injector_release_all_keys() {
        let _lock = TEST_MUTEX.lock();
        let test_key = 0x50; // VK_P
        NativeInputInjector::key_event(test_key, true, false);
        NativeInputInjector::mouse_button(MouseButton::Left, true);
        NativeInputInjector::mouse_button(MouseButton::Right, true);

        {
            let k_guard = HELD_KEYS.lock();
            assert!(k_guard.as_ref().unwrap().contains(&test_key));
            let b_guard = HELD_BUTTONS.lock();
            assert!(b_guard.as_ref().unwrap().contains(&0));
            assert!(b_guard.as_ref().unwrap().contains(&1));
        }

        // Release all keys and verify tracking sets are completely drained
        NativeInputInjector::release_all_keys();

        {
            let k_guard = HELD_KEYS.lock();
            assert!(!k_guard.as_ref().unwrap().contains(&test_key));
            let b_guard = HELD_BUTTONS.lock();
            assert!(b_guard.as_ref().unwrap().is_empty());
        }
    }

    #[test]
    fn test_vk_to_x_keysym_mapping() {
        // Letters A-Z -> lowercase 'a'-'z' (0x61..=0x7A)
        assert_eq!(vk_to_x_keysym(0x41), Some(0x0061)); // 'a'
        assert_eq!(vk_to_x_keysym(0x4D), Some(0x006D)); // 'm'
        assert_eq!(vk_to_x_keysym(0x5A), Some(0x007A)); // 'z'

        // Digits 0-9
        assert_eq!(vk_to_x_keysym(0x30), Some(0x0030)); // '0'
        assert_eq!(vk_to_x_keysym(0x35), Some(0x0035)); // '5'
        assert_eq!(vk_to_x_keysym(0x39), Some(0x0039)); // '9'

        // Arrows
        assert_eq!(vk_to_x_keysym(0x25), Some(0xFF51)); // Left
        assert_eq!(vk_to_x_keysym(0x26), Some(0xFF52)); // Up
        assert_eq!(vk_to_x_keysym(0x27), Some(0xFF53)); // Right
        assert_eq!(vk_to_x_keysym(0x28), Some(0xFF54)); // Down

        // Modifiers
        assert_eq!(vk_to_x_keysym(0x10), Some(0xFFE1)); // Shift_L
        assert_eq!(vk_to_x_keysym(0xA0), Some(0xFFE1)); // Shift_L
        assert_eq!(vk_to_x_keysym(0xA1), Some(0xFFE2)); // Shift_R
        assert_eq!(vk_to_x_keysym(0x11), Some(0xFFE3)); // Control_L
        assert_eq!(vk_to_x_keysym(0xA2), Some(0xFFE3)); // Control_L
        assert_eq!(vk_to_x_keysym(0xA3), Some(0xFFE4)); // Control_R
        assert_eq!(vk_to_x_keysym(0x12), Some(0xFFE9)); // Alt_L
        assert_eq!(vk_to_x_keysym(0xA4), Some(0xFFE9)); // Alt_L
        assert_eq!(vk_to_x_keysym(0xA5), Some(0xFFEA)); // Alt_R
        assert_eq!(vk_to_x_keysym(0x5B), Some(0xFFEB)); // Super_L
        assert_eq!(vk_to_x_keysym(0x5C), Some(0xFFEC)); // Super_R

        // F-keys F1-F12
        assert_eq!(vk_to_x_keysym(0x70), Some(0xFFBE)); // F1
        assert_eq!(vk_to_x_keysym(0x75), Some(0xFFC3)); // F6
        assert_eq!(vk_to_x_keysym(0x7B), Some(0xFFC9)); // F12

        // Control keys
        assert_eq!(vk_to_x_keysym(0x08), Some(0xFF08)); // BackSpace
        assert_eq!(vk_to_x_keysym(0x09), Some(0xFF09)); // Tab
        assert_eq!(vk_to_x_keysym(0x0D), Some(0xFF0D)); // Return
        assert_eq!(vk_to_x_keysym(0x1B), Some(0xFF1B)); // Escape
        assert_eq!(vk_to_x_keysym(0x20), Some(0x0020)); // space
        assert_eq!(vk_to_x_keysym(0x21), Some(0xFF55)); // Page_Up
        assert_eq!(vk_to_x_keysym(0x22), Some(0xFF56)); // Page_Down
        assert_eq!(vk_to_x_keysym(0x23), Some(0xFF57)); // End
        assert_eq!(vk_to_x_keysym(0x24), Some(0xFF50)); // Home
        assert_eq!(vk_to_x_keysym(0x2E), Some(0xFFFF)); // Delete

        // Unknown VK returns None
        assert_eq!(vk_to_x_keysym(0x00), None);
        assert_eq!(vk_to_x_keysym(0xFF), None);
        assert_eq!(vk_to_x_keysym(0x1234), None);
    }

    #[test]
    fn test_recording_backend_events() {
        let _lock = TEST_MUTEX.lock();
        let recording = std::sync::Arc::new(RecordingBackend::new());
        set_input_backend(Some(recording.clone()));

        // Key events
        NativeInputInjector::key_event(0x41, true, false);  // 'A' down
        NativeInputInjector::key_event(0x41, false, false); // 'A' up

        // Mouse button events
        NativeInputInjector::mouse_button(MouseButton::Left, true);
        NativeInputInjector::mouse_button(MouseButton::Left, false);
        NativeInputInjector::mouse_button(MouseButton::Right, true);
        NativeInputInjector::mouse_button(MouseButton::Right, false);
        NativeInputInjector::mouse_button(MouseButton::Middle, true);
        NativeInputInjector::mouse_button(MouseButton::Middle, false);

        // Mouse wheel
        NativeInputInjector::mouse_wheel(1, -2);

        // Move cursor relative
        NativeInputInjector::move_cursor_relative(10, -5);

        // Recenter cursor (warp)
        NativeInputInjector::recenter_cursor(0, 0, 1920, 1080);

        let events = recording.events();
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Key { vk: 0x41, is_down: true, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Key { vk: 0x41, is_down: false, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Button { button: MouseButton::Left, is_down: true, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Button { button: MouseButton::Left, is_down: false, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Button { button: MouseButton::Right, is_down: true, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Button { button: MouseButton::Middle, is_down: true, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Wheel { dx: 1, dy: -2, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::MoveRelative { dx: 10, dy: -5, .. })));
        assert!(events.iter().any(|e| matches!(e, RecordedInputEvent::Warp { x: 960, y: 540, .. })));

        // Clear and test release_all_keys drainage
        recording.clear();
        NativeInputInjector::key_event(0x42, true, false); // 'B' down
        NativeInputInjector::mouse_button(MouseButton::Left, true);

        {
            let k = HELD_KEYS.lock();
            assert!(k.as_ref().unwrap().contains(&0x42));
            let b = HELD_BUTTONS.lock();
            assert!(b.as_ref().unwrap().contains(&0));
        }

        recording.clear();
        NativeInputInjector::release_all_keys();

        {
            let k = HELD_KEYS.lock();
            assert!(!k.as_ref().unwrap().contains(&0x42));
            let b = HELD_BUTTONS.lock();
            assert!(b.as_ref().unwrap().is_empty());
        }

        let release_events = recording.events();
        assert!(release_events.iter().any(|e| matches!(e, RecordedInputEvent::Key { vk: 0x42, is_down: false, .. })));
        assert!(release_events.iter().any(|e| matches!(e, RecordedInputEvent::Button { button: MouseButton::Left, is_down: false, .. })));

        set_input_backend(None);
    }

    #[test]
    fn test_linux_backend_capability_probe() {
        assert_eq!(
            probe_display_available(None),
            Err("input injection unavailable: DISPLAY not set".to_string())
        );
        assert_eq!(
            probe_display_available(Some("")),
            Err("input injection unavailable: DISPLAY not set".to_string())
        );
        assert_eq!(
            probe_display_available(Some("   ")),
            Err("input injection unavailable: DISPLAY not set".to_string())
        );
        assert!(probe_display_available(Some(":0")).is_ok());
        assert!(probe_display_available(Some(":1.0")).is_ok());
    }

    #[test]
    fn test_update_kvm_runtime_state() {
        let _lock = TEST_MUTEX.lock();
        let state = AppState::new();
        set_kvm_app_state(state.clone());
        update_kvm_runtime_state("x11rb", "input injection unavailable: DISPLAY not set");
        let snap = state.snapshot();
        assert_eq!(snap.kvm_state.helper, "x11rb");
        assert_eq!(snap.kvm_state.message, "input injection unavailable: DISPLAY not set");
    }

    #[test]
    fn test_evaluate_linux_session_capability() {
        // Case 1: Headless / DISPLAY unset -> capture unavailable + injection unavailable
        let (helper, msg) = evaluate_linux_session_capability(None, None, false);
        assert_eq!(helper, "x11rb");
        assert!(msg.contains("remote input injection and local capture unavailable"));

        let (helper, msg) = evaluate_linux_session_capability(Some("   "), None, false);
        assert_eq!(helper, "x11rb");
        assert!(msg.contains("remote input injection and local capture unavailable"));

        // Case 2: Wayland -> capture unavailable
        let (helper, msg) = evaluate_linux_session_capability(Some(":0"), Some("wayland"), false);
        assert_eq!(helper, "wayland");
        assert!(msg.contains("global raw input capture unavailable"));

        // Case 3: X11 + capture engine running -> capture active
        let (helper, msg) = evaluate_linux_session_capability(Some(":0"), Some("x11"), true);
        assert_eq!(helper, "x11");
        assert!(msg.contains("capture engine running"));

        // Case 4: X11 + capture engine NOT running -> capture unavailable (regression test for DEFECT 2)
        let (helper, msg) = evaluate_linux_session_capability(Some(":0"), Some("x11"), false);
        assert_eq!(helper, "x11rb");
        assert!(msg.contains("local capture unavailable"));

        let (helper, msg) = evaluate_linux_session_capability(Some(":0"), None, false);
        assert_eq!(helper, "x11rb");
        assert!(msg.contains("local capture unavailable"));
    }

    #[test]
    fn test_classify_capture_health() {
        assert_eq!(classify_capture_health(0, 10), CaptureHealth::Healthy);
        assert_eq!(classify_capture_health(9, 10), CaptureHealth::Healthy);
        assert_eq!(classify_capture_health(10, 10), CaptureHealth::Lost);
        assert_eq!(classify_capture_health(15, 10), CaptureHealth::Lost);
    }

    #[test]
    fn test_evaluate_linux_session_capability_after_capture_lost() {
        let (helper, msg) = evaluate_linux_session_capability(Some(":0"), Some("x11"), false);
        assert_eq!(helper, "x11rb");
        assert!(msg.contains("local capture unavailable"));
    }

    #[test]
    fn test_mouse_button_and_wheel_mapping() {
        assert_eq!(mouse_button_to_x11_button(MouseButton::Left), 1);
        assert_eq!(mouse_button_to_x11_button(MouseButton::Middle), 2);
        assert_eq!(mouse_button_to_x11_button(MouseButton::Right), 3);

        assert_eq!(mouse_wheel_to_x11_pulses(0, 5), (None, Some((4, 5)))); // Up
        assert_eq!(mouse_wheel_to_x11_pulses(0, -3), (None, Some((5, 3)))); // Down
        assert_eq!(mouse_wheel_to_x11_pulses(2, 0), (Some((7, 2)), None)); // Right
        assert_eq!(mouse_wheel_to_x11_pulses(-4, 0), (Some((6, 4)), None)); // Left
        assert_eq!(mouse_wheel_to_x11_pulses(0, 100), (None, Some((4, 10)))); // Clamped
        assert_eq!(mouse_wheel_to_x11_pulses(0, 0), (None, None));
    }

    #[test]
    fn test_should_forward_captured_event() {
        // Initial state: no injections yet (last_injection_ms == 0) -> always forward
        assert!(should_forward_captured_event(10, 0));
        assert!(should_forward_captured_event(0, 0));

        // When injection happened at t=1000 with 25ms window:
        // Inside window -> drop (suppress self-echo)
        assert!(!should_forward_captured_event(1000, 1000));
        assert!(!should_forward_captured_event(1010, 1000));
        assert!(!should_forward_captured_event(1024, 1000));

        // Boundary: exactly at window (25ms) -> forward
        assert!(should_forward_captured_event(1025, 1000));

        // Exceeding window (> 25ms) -> forward
        assert!(should_forward_captured_event(1026, 1000));
        assert!(should_forward_captured_event(2000, 1000));

        // Clock anomaly (now < last_injection) -> safe forward
        assert!(should_forward_captured_event(500, 1000));
    }

    #[test]
    fn test_x_keycode_to_vk_mapping() {
        assert_eq!(x_keycode_to_vk(9), Some(0x1B)); // Escape
        assert_eq!(x_keycode_to_vk(10), Some(0x31)); // '1'
        assert_eq!(x_keycode_to_vk(19), Some(0x30)); // '0'
        assert_eq!(x_keycode_to_vk(22), Some(0x08)); // Backspace
        assert_eq!(x_keycode_to_vk(23), Some(0x09)); // Tab
        assert_eq!(x_keycode_to_vk(24), Some(0x51)); // 'Q'
        assert_eq!(x_keycode_to_vk(36), Some(0x0D)); // Return
        assert_eq!(x_keycode_to_vk(37), Some(0x11)); // Control_L
        assert_eq!(x_keycode_to_vk(38), Some(0x41)); // 'A'
        assert_eq!(x_keycode_to_vk(50), Some(0x10)); // Shift_L
        assert_eq!(x_keycode_to_vk(64), Some(0x12)); // Alt_L
        assert_eq!(x_keycode_to_vk(65), Some(0x20)); // Space
        assert_eq!(x_keycode_to_vk(67), Some(0x70)); // F1
        assert_eq!(x_keycode_to_vk(111), Some(0x26)); // Up Arrow
        assert_eq!(x_keycode_to_vk(113), Some(0x25)); // Left Arrow
        assert_eq!(x_keycode_to_vk(114), Some(0x27)); // Right Arrow
        assert_eq!(x_keycode_to_vk(116), Some(0x28)); // Down Arrow
        assert_eq!(x_keycode_to_vk(133), Some(0x5B)); // Super_L
        assert_eq!(x_keycode_to_vk(255), None); // Unknown
    }

    #[test]
    fn test_injection_error_rate_limiter() {
        let mut limiter = InjectionErrorRateLimiter::new();

        // First failure returns Log
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::Key),
            RateLimitAction::Log
        );

        // Immediate repeat failure is suppressed
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::Key),
            RateLimitAction::Suppress
        );
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::Key),
            RateLimitAction::Suppress
        );

        // Different failure kind logs on first occurrence
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::MoveRelative),
            RateLimitAction::Log
        );
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::MoveRelative),
            RateLimitAction::Suppress
        );

        // A success resets the failure flag for that kind
        limiter.record_success(InjectionErrorKind::Key);

        // Next failure of Key returns Log again
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::Key),
            RateLimitAction::Log
        );
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::Key),
            RateLimitAction::Suppress
        );

        // MoveRelative is still suppressed until its success
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::MoveRelative),
            RateLimitAction::Suppress
        );
        limiter.record_success(InjectionErrorKind::MoveRelative);
        assert_eq!(
            limiter.record_failure(InjectionErrorKind::MoveRelative),
            RateLimitAction::Log
        );
    }
}
