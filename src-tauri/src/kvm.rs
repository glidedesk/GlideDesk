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

/// How long the pointer has to stay pressed against the edge before control moves.
///
/// Crossing on position alone meant a mouse that was simply flicked to the edge —
/// to reach a menu, a scrollbar, the corner of a window — handed the whole session
/// to the other computer. Pushing on for a moment longer is what a person does when
/// they mean to go there, and a brush against the edge never does.
pub const EDGE_DWELL_MS: u64 = 160;

static EDGE_TOUCHED_AT_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
/// Which edge the clock above belongs to (0 = none), so a touch on another edge
/// cannot inherit it.
static EDGE_TOUCHED_SIDE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(0);

fn edge_code(side: &str) -> u8 {
    match side {
        "left" => 1,
        "right" => 2,
        "top" => 3,
        "bottom" => 4,
        _ => 0,
    }
}

/// Forget an edge touch in progress (after a crossing, a return, or a click).
pub fn reset_edge_dwell() {
    EDGE_TOUCHED_AT_MS.store(0, std::sync::atomic::Ordering::Relaxed);
    EDGE_TOUCHED_SIDE.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// True once the pointer has *stayed* on the edge long enough to mean it.
///
/// The first event that reaches the edge only starts the clock; the pointer has to
/// still be there `EDGE_DWELL_MS` later, which needs movement events to keep coming.
/// Moving away clears it, so the next approach starts over.
pub fn edge_push_ready(
    side: &str,
    pt_x: f64,
    pt_y: f64,
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
    now_ms: u64,
) -> bool {
    let touching = match side {
        "right" => pt_x >= (vx + vw) as f64 - 2.0,
        "left" => pt_x <= (vx + 1) as f64,
        "top" => pt_y <= (vy + 1) as f64,
        "bottom" => pt_y >= (vy + vh) as f64 - 2.0,
        _ => false,
    };
    if !touching {
        reset_edge_dwell();
        return false;
    }
    let code = edge_code(side);
    let started = EDGE_TOUCHED_AT_MS.load(std::sync::atomic::Ordering::Relaxed);
    let started_side = EDGE_TOUCHED_SIDE.load(std::sync::atomic::Ordering::Relaxed);
    if started == 0 || started_side != code {
        EDGE_TOUCHED_SIDE.store(code, std::sync::atomic::Ordering::Relaxed);
        EDGE_TOUCHED_AT_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);
        return false;
    }
    if now_ms.saturating_sub(started) < EDGE_DWELL_MS {
        return false;
    }
    reset_edge_dwell();
    true
}

/// Whether what just happened here may hand the pointer to the other computer.
///
/// Only a computer whose pointer is being used at this desk can start a crossing.
/// While the other computer drives this one, the pointer is moved by injection and
/// those positions reach this computer's own edges too; counting one of them as a
/// fresh crossing made each computer take the pointer back from the other, so the
/// pointer bounced between the two seams instead of travelling.
pub fn can_start_crossing(input: &str, has_peer: bool, sharing_enabled: bool) -> bool {
    input == "home" && has_peer && sharing_enabled
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

/// Which key means "run this shortcut" on the computer being driven.
///
/// Windows shortcuts use Ctrl, macOS shortcuts use Command. Returns `None` while the
/// other computer's operating system is not known, in which case keys go unchanged.
pub fn peer_command_modifier_vk(snap: &crate::state::FullState) -> Option<u16> {
    const VK_CONTROL: u16 = 0x11;
    const VK_COMMAND: u16 = 0x5B; // VK_LWIN on Windows; Command on macOS
    let os = snap
        .peer
        .as_ref()
        .or(snap.client.as_ref())
        .map(|p| p.os.to_ascii_lowercase())?;
    if os.starts_with("mac") || os.contains("darwin") {
        Some(VK_COMMAND)
    } else if os.starts_with("win") {
        Some(VK_CONTROL)
    } else {
        None
    }
}

/// Rewrite this computer's shortcut modifier into the driven computer's.
///
/// Without this, Ctrl+C on Windows arrives on a Mac as Ctrl+C, which does nothing
/// there, and Command+C on a Mac arrives on Windows as the Windows key. Typing keys
/// are untouched; only the modifier that starts a shortcut changes.
pub fn translate_modifier_vk(vk: u16, peer_modifier: Option<u16>) -> u16 {
    const VK_CONTROL_GENERIC: u16 = 0x11;
    const VK_LCONTROL: u16 = 0xA2;
    const VK_RCONTROL: u16 = 0xA3;
    const VK_LCOMMAND: u16 = 0x5B; // VK_LWIN
    const VK_RCOMMAND: u16 = 0x5C; // VK_RWIN
    match peer_modifier {
        // Driving a Mac: Ctrl becomes Command.
        Some(VK_LCOMMAND) => match vk {
            VK_CONTROL_GENERIC | VK_LCONTROL | VK_RCONTROL => VK_LCOMMAND,
            other => other,
        },
        // Driving Windows: Command becomes Ctrl.
        Some(VK_CONTROL_GENERIC) => match vk {
            VK_LCOMMAND | VK_RCOMMAND => VK_CONTROL_GENERIC,
            other => other,
        },
        _ => vk,
    }
}

/// Put the side back on a modifier the keyboard reported generically.
///
/// Some keyboards and adapters report a right-hand Shift, Ctrl or Alt as the
/// generic code (0x10, 0x11, 0x12) and mark the right-hand one only in the scan
/// code, so the far computer would see the left-hand key: right Shift + 4 types
/// "4" instead of "$" (#212). The scan code says which side it was.
pub fn side_specific_modifier(vk: u16, scan: u32, extended: bool) -> u16 {
    const VK_SHIFT: u16 = 0x10;
    const VK_CONTROL: u16 = 0x11;
    const VK_MENU: u16 = 0x12;
    const SCAN_RSHIFT: u32 = 0x36;
    match vk {
        // Right Shift is scan 0x36; its left twin is 0x2A.
        VK_SHIFT if scan == SCAN_RSHIFT => 0xA1, // VK_RSHIFT
        // Ctrl and Alt are only distinguished by the E0 prefix.
        VK_CONTROL if extended => 0xA3, // VK_RCONTROL
        VK_MENU if extended => 0xA5,    // VK_RMENU
        other => other,
    }
}

/// How far the hand has to travel back towards this screen before control comes
/// home with it.
///
/// A few pixels is not a deliberate return: pointer acceleration and the very
/// first event after taking control can easily read a little backwards, and with
/// a tight limit that handed control straight back, so the pointer crossed, came
/// home, crossed again — the blink on the edge. A real pull back is much longer
/// than this.
pub const EDGE_RETURN_DISTANCE: f64 = 24.0;

/// Where the pointer waits while this computer drives the other one.
///
/// The middle of the desktop, so the hand has room to move in every direction.
/// Parking it in the seam it crossed seemed tidy — the pointer would already be
/// at the edge when it came home — but the few pixels between the park point and
/// the screen edge were all the movement that edge could report: pushing further
/// left produced no new position at all, so the other computer's pointer stopped
/// a few pixels in. From the middle there is always somewhere to go.
pub fn drive_park_point(vx: i32, vy: i32, vw: i32, vh: i32) -> (i32, i32) {
    (vx + vw / 2, vy + vh / 2)
}

/// Check if cumulative remote virtual deltas indicate cursor has moved back across the seam to home.
#[cfg(any(test, not(windows)))]
pub fn check_edge_returned(side: &str, virtual_remote_x: f64, virtual_remote_y: f64) -> bool {
    match side {
        "right" => virtual_remote_x < -EDGE_RETURN_DISTANCE,
        "left" => virtual_remote_x > EDGE_RETURN_DISTANCE,
        "top" => virtual_remote_y > EDGE_RETURN_DISTANCE,
        "bottom" => virtual_remote_y < -EDGE_RETURN_DISTANCE,
        _ => false,
    }
}

/// This computer's whole desktop, across every monitor, in the units its own
/// pointer API uses (pixels on Windows and Linux, points on macOS).
#[cfg(any(windows, target_os = "macos"))]
pub fn virtual_desktop_size() -> (f64, f64) {
    let (_, _, w, h) = hook_engine::get_virtual_desktop_bounds();
    (w as f64, h as f64)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn virtual_desktop_size() -> (f64, f64) {
    let (w, h) = get_linux_backend()
        .ok()
        .and_then(|b| b.get_viewport_dimensions().ok())
        .unwrap_or((1920, 1080));
    (w as f64, h as f64)
}

/// How much to multiply a pointer delta so the whole of the other computer's
/// desktop is reachable from this one.
///
/// The two desktops are rarely the same size: a 1366×768 PC beside a 1920×1080
/// Mac forwarded its deltas unchanged, so moving from the middle of the PC's
/// screen to its bottom edge moved the Mac's pointer only 768 of its 1080
/// points. The pointer then stopped at the bottom of the PC's screen while the
/// person kept pushing, and the lower third of the Mac could not be reached.
/// Scaling each axis by the ratio of the two sizes means the same fraction of
/// travel on one desktop covers the same fraction of the other.
///
/// Both sizes are in their own pointer units, which is what the two motion
/// APIs work in. The clamp keeps a bogus or half-reported size from turning a
/// normal movement into a jump.
pub fn motion_scale(local_w: f64, local_h: f64, peer_w: f64, peer_h: f64) -> (f64, f64) {
    if local_w < 2.0 || local_h < 2.0 || peer_w < 2.0 || peer_h < 2.0 {
        return (1.0, 1.0);
    }
    (
        (peer_w / local_w).clamp(0.25, 4.0),
        (peer_h / local_h).clamp(0.25, 4.0),
    )
}

/// Say once, in the log, how the two desktops differ and what that does to movement.
///
/// When pointer travel looks wrong, the first thing worth knowing is whether the two
/// computers agreed on how big each other's screens are; without this line that has
/// to be guessed from the behaviour.
pub fn log_peer_screen(peer_w: f64, peer_h: f64) {
    let (lw, lh) = virtual_desktop_size();
    let (sx, sy) = motion_scale(lw, lh, peer_w, peer_h);
    log::info!(
        "[KVM] Other desktop is {:.0}x{:.0}; this one is {:.0}x{:.0}, so movement sent there is scaled by {:.3}x{:.3}",
        peer_w,
        peer_h,
        lw,
        lh,
        sx,
        sy
    );
}

/// Fractional pointer motion waiting to become whole pixels.
#[derive(Debug, Clone, Copy, Default)]
pub struct MotionAccumulator {
    x: f64,
    y: f64,
}

impl MotionAccumulator {
    /// Add a fractional delta and return the whole pixels to move by.
    ///
    /// Scaled deltas are fractional (1.4 px is normal when a small screen is
    /// beside a big one). Rounding each packet on its own loses the fraction
    /// and the pointer falls behind the hand; forcing a whole pixel from any
    /// non-zero remainder overshoots instead. The leftover waits here for the
    /// next packet, so nothing is lost and nothing is invented.
    pub fn add(&mut self, dx: f64, dy: f64) -> (i32, i32) {
        self.x += dx;
        self.y += dy;
        let ix = self.x.trunc();
        let iy = self.y.trunc();
        self.x -= ix;
        self.y -= iy;
        (ix as i32, iy as i32)
    }

    pub fn reset(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
    }
}

static PENDING_MOTION: Mutex<MotionAccumulator> = Mutex::new(MotionAccumulator { x: 0.0, y: 0.0 });

/// Whole-pixel movement for an incoming fractional delta.
pub fn accumulate_motion(dx: f64, dy: f64) -> (i32, i32) {
    PENDING_MOTION.lock().add(dx, dy)
}

/// Forget a leftover fraction when the pointer is placed rather than moved, so
/// an old remainder does not nudge it after a jump.
pub fn reset_pending_motion() {
    PENDING_MOTION.lock().reset();
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

    /// Free-form field on every event. This app writes its own marker in it so the
    /// event tap can tell its injections apart from the person at the desk without
    /// guessing from timing.
    pub const kCGEventSourceUserData: u32 = 42;
    pub const kCGEventTargetUnixProcessID: u32 = 40;
    pub const kCGEventSourceUnixProcessID: u32 = 41;
    pub const kCGEventSourceStateCombinedSessionState: u32 = 0;

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
        pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
        pub fn CGEventSourceCreate(state_id: u32) -> CGEventSourceRef;
        pub fn CGEventSourceSetLocalEventsSuppressionInterval(
            source: CGEventSourceRef,
            seconds: f64,
        );
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
        pub fn CGMainDisplayID() -> u32;
        pub fn CGDisplayHideCursor(display: u32) -> CGError;
        pub fn CGDisplayShowCursor(display: u32) -> CGError;

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

/// Stamped into every event this app posts on macOS ("GLID").
///
/// Timing alone cannot separate the app's own pointer movement from the person's:
/// a slow link delivers injected packets further apart than any window short
/// enough to stay out of the way, and a fast one never leaves a gap at all. The
/// marker travels with the event, so the tap always knows whose movement it is.
#[cfg(target_os = "macos")]
pub const INJECTED_EVENT_MARK: i64 = 0x474C_4944;

#[cfg(target_os = "macos")]
pub fn mark_injected_event(event: macos_ffi::CGEventRef) {
    if event.is_null() {
        return;
    }
    unsafe {
        macos_ffi::CGEventSetIntegerValueField(
            event,
            macos_ffi::kCGEventSourceUserData,
            INJECTED_EVENT_MARK,
        );
    }
}

#[cfg(target_os = "macos")]
pub fn is_injected_event(event: macos_ffi::CGEventRef) -> bool {
    if event.is_null() {
        return false;
    }
    unsafe {
        macos_ffi::CGEventGetIntegerValueField(event, macos_ffi::kCGEventSourceUserData)
            == INJECTED_EVENT_MARK
    }
}

/// Who posted this event, as macOS records it. Zero means the system itself
/// (a real device, or the cursor being put somewhere by a warp).
#[cfg(target_os = "macos")]
pub fn injected_event_debug(event: macos_ffi::CGEventRef) -> String {
    if event.is_null() {
        return "null".to_string();
    }
    unsafe {
        let user_data = macos_ffi::CGEventGetIntegerValueField(
            event,
            macos_ffi::kCGEventSourceUserData,
        );
        let pid = macos_ffi::CGEventGetIntegerValueField(
            event,
            macos_ffi::kCGEventSourceUnixProcessID,
        );
        format!(
            "userData=0x{:X} (ours=0x{:X}) pid={} ours={}",
            user_data,
            INJECTED_EVENT_MARK,
            pid,
            std::process::id()
        )
    }
}

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

/// Keys whose *press* this computer sent to the other one and took away from the
/// app that had them.
///
/// A release is only taken away for those keys. Without that, this order leaves a
/// key down for good: the person holds Command, control moves to the other computer
/// while it is still held, and the release is swallowed and sent away instead of
/// reaching the app that saw the press. macOS then believes Command is still down,
/// so every later letter becomes a shortcut and nothing types — the keyboard looks
/// dead until the machine restarts.
#[cfg(any(test, windows, target_os = "macos"))]
static SWALLOWED_KEY_DOWNS: parking_lot::Mutex<Option<std::collections::HashSet<u16>>> =
    parking_lot::Mutex::new(None);

/// Remember that a press of `keycode` went to the other computer.
#[cfg(any(test, windows, target_os = "macos"))]
fn mark_swallowed_down(keycode: u16) {
    let mut guard = SWALLOWED_KEY_DOWNS.lock();
    guard.get_or_insert_with(std::collections::HashSet::new).insert(keycode);
}

/// True when this key's press was taken away, so its release belongs to the other
/// computer too. Clears the mark: one release per press.
#[cfg(any(test, windows, target_os = "macos"))]
fn take_swallowed_down(keycode: u16) -> bool {
    let mut guard = SWALLOWED_KEY_DOWNS.lock();
    guard
        .as_mut()
        .map(|set| set.remove(&keycode))
        .unwrap_or(false)
}

/// Every key whose press was taken away and never came back up.
#[cfg(any(test, windows, target_os = "macos"))]
fn take_all_swallowed_downs() -> Vec<u16> {
    let mut guard = SWALLOWED_KEY_DOWNS.lock();
    match guard.as_mut() {
        Some(set) => set.drain().collect(),
        None => Vec::new(),
    }
}

#[cfg(any(test, windows, target_os = "macos"))]
fn swallowed_down_count() -> usize {
    SWALLOWED_KEY_DOWNS
        .lock()
        .as_ref()
        .map(|set| set.len())
        .unwrap_or(0)
}

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

        // Keys whose press went to the other computer have no release coming here,
        // so free them here: a press taken away and never given back leaves the app
        // that saw it waiting for a release that will not come.
        #[cfg(any(windows, target_os = "macos"))]
        {
            let pending = swallowed_down_count();
            if pending > 0 {
                log::info!(
                    "[KVM] Releasing {} key(s) whose press went to the other computer",
                    pending
                );
            }
        }
        #[cfg(target_os = "macos")]
        for keycode in take_all_swallowed_downs() {
            let vk = keycodes::macos_keycode_to_windows_vk(keycode) as u16;
            Self::key_event(vk, false, false);
        }
        #[cfg(windows)]
        for vk in take_all_swallowed_downs() {
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

        // A caption-button press is injected as a pair when the release arrives; if the
        // link ends first, there is nothing left to press or release.
        #[cfg(windows)]
        caption_guard::clear();

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
            #[cfg(target_os = "macos")]
            hook_engine::set_cursor_hidden(false);
            return;
        }
        let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
        Self::recenter_cursor(vx, vy, vw, vh);
        #[cfg(target_os = "macos")]
        hook_engine::set_cursor_hidden(false);
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
        hook_engine::note_injected_pointer(center_pt.x, center_pt.y);
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

    /// Move the pointer to an exact pixel as forwarded input.
    ///
    /// Absolute moves skip pointer acceleration, and the `dwExtraInfo` marker keeps the
    /// low-level hook from mistaking our own injection for the local user. `SetCursorPos`
    /// cannot carry that marker, so a pointer placed on the seam to receive the other
    /// computer's input used to look like the user pushing into the edge — and this
    /// computer grabbed control straight back.
    #[cfg(windows)]
    pub fn inject_cursor_absolute(x: i32, y: i32) {
        main_thread_input::run(move || {
            record_injection_timestamp();
            if let Some(backend) = get_custom_backend() {
                let _ = backend.warp(x, y);
                return;
            }
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
                MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
            };
            let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
            if vw <= 1 || vh <= 1 {
                return;
            }
            let nx = ((x - vx) as f64 * 65_535.0 / (vw - 1) as f64).round() as i32;
            let ny = ((y - vy) as f64 * 65_535.0 / (vh - 1) as f64).round() as i32;
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: nx,
                        dy: ny,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_MOVE
                            | MOUSEEVENTF_ABSOLUTE
                            | MOUSEEVENTF_VIRTUALDESK,
                        time: 0,
                        dwExtraInfo: 0x474C4944,
                    },
                },
            };
            unsafe {
                let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                if sent != 1 {
                    log::warn!(
                        "[KVM] SendInput absolute move failed: ({}, {}) sent={} err={}",
                        x,
                        y,
                        sent,
                        std::io::Error::last_os_error()
                    );
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
        hook_engine::note_injected_pointer(x as f64, y as f64);
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
        #[cfg(windows)]
        Self::inject_cursor_absolute(x, y);
        #[cfg(target_os = "macos")]
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
        // Relative SendInput moves go through the user's pointer acceleration
        // ("Enhance pointer precision"), so the distance here did not match the distance
        // the hand moved on the other computer and the two pointers drifted apart. Warp
        // to the exact pixel instead: same distance, no acceleration.
        let Some((x, y)) = Self::cursor_position() else {
            return;
        };
        let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
        if vw <= 1 || vh <= 1 {
            return;
        }
        let nx = (x + dx as f64).clamp(vx as f64, (vx + vw - 1) as f64);
        let ny = (y + dy as f64).clamp(vy as f64, (vy + vh - 1) as f64);
        Self::inject_cursor_absolute(nx.round() as i32, ny.round() as i32);
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
                // Keep the pointer on a screen. Warping past the edge does not
                // stop at it: macOS puts the cursor wherever it is told, so a
                // longer push from the other computer used to carry it off the
                // display, where it could not be seen or clicked.
                let (vx, vy, vw, vh) = hook_engine::get_virtual_desktop_bounds();
                let target = CGPoint {
                    x: (cur.x + dx as f64).clamp(vx as f64, (vx + vw - 1) as f64),
                    y: (cur.y + dy as f64).clamp(vy as f64, (vy + vh - 1) as f64),
                };
                let _ = CGWarpMouseCursorPosition(target);
                hook_engine::note_injected_pointer(target.x, target.y);
                let ev = CGEventCreateMouseEvent(
                    hook_engine::event_source(),
                    kCGEventMouseMoved,
                    target,
                    kCGMouseButtonLeft,
                );
                if !ev.is_null() {
                    crate::kvm::mark_injected_event(ev);
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
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
            MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

        let flags = match (button, is_down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };

        let mouse = |dw_flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: dw_flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        // A press on this app's own caption button is held back until its release:
        // see caption_guard. Windows' modal caption loop runs on the thread that
        // injects here, so injecting the press first would queue the release behind
        // the loop that is waiting for it and the link would time out (#209, #208,
        // #203).
        if button == MouseButton::Left {
            if is_down {
                if unsafe { caption_guard::on_own_caption_button() } {
                    let mut at = POINT::default();
                    let _ = unsafe { GetCursorPos(&mut at) };
                    caption_guard::hold(at.x, at.y);
                    log::info!("[KVM] caption-button press held until its release at ({}, {})", at.x, at.y);
                    return;
                }
            } else if let Some((x, y)) = caption_guard::take() {
                let mut at = POINT::default();
                let _ = unsafe { GetCursorPos(&mut at) };
                let moved = at.x != x || at.y != y;
                let pair = [mouse(MOUSEEVENTF_LEFTDOWN), mouse(MOUSEEVENTF_LEFTUP)];
                unsafe {
                    if moved {
                        let _ = SetCursorPos(x, y);
                    }
                    let sent = SendInput(&pair, std::mem::size_of::<INPUT>() as i32);
                    if sent != 2 {
                        log::warn!(
                            "[KVM] SendInput caption click failed: sent={} err={} {}",
                            sent, std::io::Error::last_os_error(), Self::desktop_probe()
                        );
                    } else {
                        log::info!("[KVM] caption click injected as a press and release pair");
                    }
                    if moved {
                        let _ = SetCursorPos(at.x, at.y);
                    }
                }
                return;
            }
        }

        unsafe {
            let sent = SendInput(&[mouse(flags)], std::mem::size_of::<INPUT>() as i32);
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

            let ev = CGEventCreateMouseEvent(hook_engine::event_source(), ev_type, cur, mouse_btn);
            if !ev.is_null() {
                crate::kvm::mark_injected_event(ev);
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
                hook_engine::event_source(),
                kCGScrollEventUnitLine,
                2,
                dy,
                dx,
                0,
            );
            if !ev.is_null() {
                crate::kvm::mark_injected_event(ev);
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
            hook_engine::note_injected_key();
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
                let ev = CGEventCreateKeyboardEvent(hook_engine::event_source(), mac_code, is_down);
                if !ev.is_null() {
                    crate::kvm::mark_injected_event(ev);
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

/// The clipboard handle used whenever this computer writes text or a picture for the
/// user or on the other computer's behalf.
///
/// On X11 the clipboard belongs to whichever client owns the selection, and the data
/// is gone the moment that client's window is destroyed — unless a clipboard manager
/// takes it over first. A session without one (a bare window manager, a kiosk box)
/// would therefore lose every item the other computer sends the instant the setter
/// returned, so one handle is kept alive for the life of the app instead of a fresh
/// one per write. On macOS and Windows holding the handle is equally harmless.
static SHARED_CLIPBOARD: Mutex<Option<NativeClipboard>> = Mutex::new(None);

fn with_shared_clipboard<R>(f: impl FnOnce(&mut NativeClipboard) -> R) -> Option<R> {
    let mut guard = SHARED_CLIPBOARD.lock();
    if guard.is_none() {
        *guard = Some(NativeClipboard::new());
    }
    guard.as_mut().map(f)
}

/// Put `text` on this clipboard so that it outlives this call.
pub fn set_shared_clipboard_text(text: &str) -> bool {
    with_shared_clipboard(|cb| cb.set_text(text)).unwrap_or(false)
}

/// Put a PNG on this clipboard so that it outlives this call.
pub fn set_shared_clipboard_image(png_bytes: &[u8]) -> bool {
    with_shared_clipboard(|cb| cb.set_image_png(png_bytes)).unwrap_or(false)
}

/// The text and picture this computer last received from the other one.
///
/// Putting an item received from the other computer on this clipboard makes the
/// watcher see a fresh copy, which it would send straight back. These markers let it
/// recognise its own echo and leave the item alone.
static LAST_REMOTE_TEXT: Mutex<Option<String>> = Mutex::new(None);
static LAST_REMOTE_IMAGE: Mutex<Option<usize>> = Mutex::new(None);

/// The other computer just put `text` on this clipboard; do not send it back.
pub fn note_remote_clipboard_text(text: &str) {
    *LAST_REMOTE_TEXT.lock() = Some(text.to_string());
}

/// The other computer just put a `png_len`-byte picture on this clipboard.
pub fn note_remote_clipboard_image(png_len: usize) {
    *LAST_REMOTE_IMAGE.lock() = Some(png_len);
}

/// True once, when `text` is the item that just arrived from the other computer.
fn take_remote_text(text: &str) -> bool {
    let mut guard = LAST_REMOTE_TEXT.lock();
    if guard.as_deref() == Some(text) {
        *guard = None;
        true
    } else {
        false
    }
}

/// True once, when `len` is the picture that just arrived from the other computer.
fn take_remote_image(len: usize) -> bool {
    let mut guard = LAST_REMOTE_IMAGE.lock();
    if *guard == Some(len) {
        *guard = None;
        true
    } else {
        false
    }
}

/// A small data-URL copy of a clipboard picture, for the Recent list.
pub fn clipboard_thumb(png: &[u8], w: u32, h: u32, shot: bool) -> Option<serde_json::Value> {
    use base64::Engine;
    let full = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    let src_w = w.max(1);
    let target_w = 96.min(src_w);
    let target_h = ((h.max(1) as f64) * (target_w as f64) / (src_w as f64)).round().max(1.0) as u32;
    let small = image::imageops::resize(&full, target_w, target_h, image::imageops::FilterType::Triangle);
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(small)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;
    let b64 = base64::prelude::BASE64_STANDARD.encode(cursor.into_inner());
    Some(serde_json::json!({
        "thumb": format!("data:image/png;base64,{}", b64),
        "w": w,
        "h": h,
        "shot": shot,
        "copy": true,
    }))
}

/// Real-time clipboard watcher polling system clipboard and broadcasting changes.
///
/// It is the sending half of clipboard sync: it only sends when the master switch and
/// the "copy here, paste there" direction are both on. Anything that arrives is written
/// by the network side, which also records the echo marker used here.
pub struct ClipboardWatcher {
    network: NetworkManager,
    state: AppState,
    app: tauri::AppHandle,
    last_text: Arc<Mutex<String>>,
    last_image_len: Arc<Mutex<usize>>,
    /// Something was copied while the pointer was still on this computer, and the
    /// "When you switch" timing is selected.
    pending_switch: Arc<Mutex<bool>>,
}

impl ClipboardWatcher {
    pub fn new(network: NetworkManager, state: AppState, app: tauri::AppHandle) -> Self {
        // Start from what is already on the clipboard, so launching the app does not
        // push an old item to the other computer.
        let seen = NativeClipboard::new().get_text().unwrap_or_default();
        Self {
            network,
            state,
            app,
            last_text: Arc::new(Mutex::new(seen)),
            last_image_len: Arc::new(Mutex::new(0)),
            pending_switch: Arc::new(Mutex::new(false)),
        }
    }

    async fn send_text(&self, text: String) {
        let msg = WireMessage {
            t: "clip".to_string(),
            v: Some(1),
            text: Some(text),
            ts: Some(chrono::Utc::now().timestamp_millis() as u64),
            ..Default::default()
        };
        self.network.send_packet(msg).await;
    }

    async fn send_image(&self, png: &[u8], w: u32, h: u32, shot: bool) {
        use base64::Engine;
        let b64 = base64::prelude::BASE64_STANDARD.encode(png);
        let msg = WireMessage {
            t: "clip.image".to_string(),
            v: Some(1),
            id: Some(format!("img_{}", chrono::Utc::now().timestamp_millis())),
            ts: Some(chrono::Utc::now().timestamp_millis() as u64),
            png: Some(b64),
            w: Some(w),
            height: Some(h),
            bytes: Some(png.len()),
            parts: Some(1),
            shot: Some(shot),
            ..Default::default()
        };
        self.network.send_packet(msg).await;
    }

    pub fn start(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(400));
            let mut ticks: u64 = 0;

            loop {
                interval.tick().await;

                let (on, send, images, max_kb, mode, driven) = {
                    let s = self.state.inner.read();
                    (
                        s.clipboard.enabled,
                        s.clipboard.send,
                        s.clipboard.images,
                        s.clipboard.max_kb.max(1),
                        s.clipboard.mode.clone(),
                        s.input == "driven",
                    )
                };

                // Master switch off: nothing is copied, watched or sent.
                if !on {
                    *self.pending_switch.lock() = false;
                    continue;
                }

                ticks += 1;
                let mut cb = NativeClipboard::new();
                let mut saw_text = false;

                if let Some(text) = cb.get_text() {
                    let changed = {
                        let mut last = self.last_text.lock();
                        if *last != text {
                            *last = text.clone();
                            true
                        } else {
                            false
                        }
                    };
                    // An item the other computer just delivered is already on this
                    // clipboard; sending it back would loop for ever.
                    let echo = take_remote_text(&text);
                    if changed && !text.is_empty() {
                        saw_text = true;
                        if !echo {
                            self.handle_local_text(text, send, &mode, driven, max_kb).await;
                        }
                    }
                }

                // If no text changed and every 2 cycles (~800 ms), look for pictures.
                if !saw_text && ticks % 2 == 0 && images {
                    if let Some((png, w, h)) = cb.get_image_png() {
                        let len = png.len();
                        let changed = {
                            let mut last = self.last_image_len.lock();
                            if len > 0 && *last != len {
                                *last = len;
                                true
                            } else {
                                false
                            }
                        };
                        let echo = take_remote_image(len);
                        if changed && !echo && send && (mode != "switch" || driven) && len <= 10 * 1024 * 1024 {
                            self.send_image(&png, w, h, true).await;
                            let thumb = clipboard_thumb(&png, w, h, true);
                            if self.state.record_clipboard_item(
                                format!("Picture · {} × {}", w, h),
                                "out",
                                len as u64,
                                thumb,
                            ) {
                                crate::commands::push_state(&self.app, &self.state);
                            }
                        }
                    }
                }

                // "When you switch": the item copied while the pointer was still here
                // crosses the moment the pointer lands on the other computer.
                if send && mode == "switch" && driven {
                    let pending = {
                        let mut p = self.pending_switch.lock();
                        let was = *p;
                        *p = false;
                        was
                    };
                    if pending {
                        if let Some(text) = cb.get_text().filter(|t| !t.is_empty()) {
                            if text.len() as u32 <= max_kb * 1024 {
                                self.send_text(text.clone()).await;
                                if self.state.record_clipboard_item(
                                    text.clone(),
                                    "out",
                                    text.len() as u64,
                                    None,
                                ) {
                                    crate::commands::push_state(&self.app, &self.state);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// A copy made on this computer: send it, or hold it for the next switch.
    async fn handle_local_text(
        &self,
        text: String,
        send: bool,
        mode: &str,
        driven: bool,
        max_kb: u32,
    ) {
        if !send {
            return;
        }
        let bytes = text.len() as u64;
        if bytes > max_kb as u64 * 1024 {
            if self.state.set_clip_last_skip(&format!("over {} KB", max_kb)) {
                crate::commands::push_state(&self.app, &self.state);
            }
            return;
        }
        self.state.set_clip_last_skip("");
        if mode == "switch" && !driven {
            *self.pending_switch.lock() = true;
            return;
        }
        self.send_text(text.clone()).await;
        if self.state.record_clipboard_item(text.clone(), "out", bytes, None) {
            crate::commands::push_state(&self.app, &self.state);
        }
    }
}

/// Clicks on this app's own Minimize, Maximize, Close and Help buttons.
///
/// Windows runs a synchronous modal loop on the window's thread when a caption
/// button is pressed, waiting for the release. Here that thread is also the one
/// that injects input, so injecting the press first would queue the release behind
/// the loop that is waiting for it: the link would time out and drop (#209, #208,
/// #203). The press is held back and both halves are injected together when the
/// release arrives.
#[cfg(windows)]
mod caption_guard {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetCursorPos, GetWindowThreadProcessId, SendMessageTimeoutW, WindowFromPoint,
        GA_ROOT, SMTO_ABORTIFHUNG, WM_NCHITTEST,
    };

    static HELD: AtomicBool = AtomicBool::new(false);
    static HELD_X: AtomicI32 = AtomicI32::new(0);
    static HELD_Y: AtomicI32 = AtomicI32::new(0);

    /// Remember where the press landed; inject nothing yet.
    pub fn hold(x: i32, y: i32) {
        HELD_X.store(x, Ordering::SeqCst);
        HELD_Y.store(y, Ordering::SeqCst);
        HELD.store(true, Ordering::SeqCst);
    }

    /// Take the held point, if there is one.
    pub fn take() -> Option<(i32, i32)> {
        if !HELD.swap(false, Ordering::SeqCst) {
            return None;
        }
        Some((HELD_X.load(Ordering::SeqCst), HELD_Y.load(Ordering::SeqCst)))
    }

    /// Drop a held press without injecting it (the link ended mid-click).
    pub fn clear() {
        HELD.store(false, Ordering::SeqCst);
    }

    /// Is the pointer on a caption button of one of this app's own windows?
    pub unsafe fn on_own_caption_button() -> bool {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() {
            return false;
        }
        let h: HWND = WindowFromPoint(p);
        if h.0.is_null() {
            return false;
        }
        let root = GetAncestor(h, GA_ROOT);
        let target = if root.0.is_null() { h } else { root };
        let mut pid = 0u32;
        GetWindowThreadProcessId(target, Some(&mut pid));
        if pid != GetCurrentProcessId() {
            return false;
        }
        // Ask the window what that point is, exactly as the mouse would. The hit test
        // must go to the top-level window: a child (the webview) answers HTCLIENT for
        // its whole area, caption included.
        let lparam = LPARAM((((p.y & 0xFFFF) << 16) | (p.x & 0xFFFF)) as isize);
        let mut hit = 0usize;
        let answered = SendMessageTimeoutW(
            target,
            WM_NCHITTEST,
            WPARAM(0),
            lparam,
            SMTO_ABORTIFHUNG,
            100,
            Some(&mut hit),
        );
        if answered.0 == 0 {
            return false;
        }
        matches!(hit as i32, 8 | 9 | 20 | 21) // HTMINBUTTON, HTMAXBUTTON, HTCLOSE, HTHELP
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
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LBUTTON};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, GetSystemMetrics, PostThreadMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK,
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

    /// What the two callbacks below are allowed to know, kept up to date by its own thread.
    ///
    /// Reading the live state inside a low-level hook meant cloning the whole state and
    /// enumerating the local network addresses on every pointer event. When that call took
    /// too long Windows silently unhooked the keyboard, and typing stopped crossing the
    /// link with no error written anywhere. A tenth-of-a-second-old copy of a few fields
    /// is more than enough to decide what to do with one event.
    #[derive(Clone)]
    struct HookView {
        driving: bool,
        driven: bool,
        has_peer: bool,
        keyboard: bool,
        mouse: bool,
        linked: bool,
        drive_other: bool,
        paused: bool,
        side: String,
        peer_modifier: Option<u16>,
    }

    static HOOK_VIEW: std::sync::OnceLock<std::sync::RwLock<std::sync::Arc<HookView>>> =
        std::sync::OnceLock::new();
    /// Keyboard events this hook has seen, and keys this app injected itself.
    static KEY_EVENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static KEY_INJECTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    /// The last key this hook saw, kept as a number so the watchdog can report it.
    static LAST_VK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    pub fn note_injected_key() {
        KEY_INJECTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    /// Thread running the hook message loop, so the watchdog can ask it to reinstall.
    static HOOK_THREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    const WM_REINSTALL_KEYBOARD: u32 = 0x8000 + 1;

    fn hook_view(state: &AppState) -> HookView {
        let s = state.inner.read();
        let os = s
            .peer
            .as_ref()
            .or(s.client.as_ref())
            .map(|p| p.os.to_ascii_lowercase());
        let peer_modifier = match os {
            Some(ref os) if os.starts_with("mac") || os.contains("darwin") => Some(0x5B),
            Some(ref os) if os.starts_with("win") => Some(0x11),
            _ => None,
        };
        HookView {
            driving: s.input == "driving",
            driven: s.input == "driven",
            has_peer: s.client.is_some() || s.peer.is_some() || s.link.state == "connected",
            keyboard: s.kvm.keyboard,
            mouse: s.kvm.mouse,
            linked: s.kvm.linked,
            drive_other: s.kvm.drive_other,
            paused: s.kvm_state.paused,
            side: s.layout.side.clone(),
            peer_modifier,
        }
    }

    fn current_view() -> Option<std::sync::Arc<HookView>> {
        HOOK_VIEW
            .get()
            .and_then(|slot| slot.read().ok().map(|g| std::sync::Arc::clone(&g)))
    }

    /// Re-read the state that decides what the hook does.
    ///
    /// The cached view is refreshed a few times a second, which is fine for a switch
    /// somebody flips but not for the moment the pointer crosses: the very next event
    /// has to know this computer is driving, or it crosses again, takes the pointer a
    /// second time and resets the distance it is counting from.
    fn refresh_view_now() {
        unsafe {
            if let Some(state) = &APP_STATE {
                if let Some(slot) = HOOK_VIEW.get() {
                    let next = std::sync::Arc::new(hook_view(state));
                    if let Ok(mut g) = slot.write() {
                        *g = next;
                    }
                }
            }
        }
    }

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

    /// Move the pointer now, from the hook's own thread.
    ///
    /// The parked position is the origin the next movement is measured from, so
    /// it has to be in place before that event is worked out. A warp posted to
    /// the app's main thread can arrive after the next event and turn a small
    /// hand movement into a jump; `SendInput` runs here and carries the marker
    /// that keeps the hook from treating the move as the user pushing into the
    /// edge. If the session refuses input from this thread, fall back to the
    /// posted warp rather than leave the pointer at the edge.
    fn park_cursor_now(x: i32, y: i32) {
        super::record_injection_timestamp();
        if let Some(backend) = super::get_custom_backend() {
            let _ = backend.warp(x, y);
            return;
        }
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
            MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
        };
        let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
        if vw <= 1 || vh <= 1 {
            return;
        }
        let nx = ((x - vx) as f64 * 65_535.0 / (vw - 1) as f64).round() as i32;
        let ny = ((y - vy) as f64 * 65_535.0 / (vh - 1) as f64).round() as i32;
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: nx,
                    dy: ny,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE
                        | MOUSEEVENTF_ABSOLUTE
                        | MOUSEEVENTF_VIRTUALDESK,
                    time: 0,
                    dwExtraInfo: 0x474C4944,
                },
            },
        };
        unsafe {
            let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if sent != 1 {
                super::NativeInputInjector::warp_cursor(x, y);
            }
        }
    }

    pub fn start_hooks(app: AppHandle, state: AppState, network: NetworkManager) {
        set_kvm_app_state(state.clone());
        super::main_thread_input::register(app.clone());
        {
            let view_state = state.clone();
            let first = hook_view(&view_state);
            let _ = HOOK_VIEW.set(std::sync::RwLock::new(std::sync::Arc::new(first)));
            let refresher = state.clone();
            std::thread::Builder::new()
                .name("kvm-hook-view".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let next = std::sync::Arc::new(hook_view(&refresher));
                    if let Some(slot) = HOOK_VIEW.get() {
                        if let Ok(mut g) = slot.write() {
                            *g = next;
                        }
                    }
                })
                .ok();
            // Windows drops a low-level keyboard hook whose callback misses its deadline,
            // and a dropped hook stops every keystroke crossing without a word. Nothing
            // here is slow any more, but a long silence is suspicious enough to put the
            // hook back, and a key this app injects that never comes back through the
            // hook is proof enough to do it straight away.
            std::thread::Builder::new()
                .name("kvm-hook-watch".into())
                .spawn(move || {
                    use std::sync::atomic::Ordering;
                    let mut events = KEY_EVENTS.load(Ordering::Relaxed);
                    let mut injections = KEY_INJECTIONS.load(Ordering::Relaxed);
                    let mut suspect = 0u32;
                    let mut installed_at = std::time::Instant::now();
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let now_events = KEY_EVENTS.load(Ordering::Relaxed);
                        let now_injections = KEY_INJECTIONS.load(Ordering::Relaxed);
                        let moved = now_events != events;
                        let typed = now_injections != injections;
                        events = now_events;
                        injections = now_injections;
                        if moved {
                            suspect = 0;
                        } else if typed {
                            // Keys this app injected did not come back through the hook,
                            // which only happens when Windows has dropped the hook.
                            suspect += 1;
                        }
                        let silent_too_long = !moved
                            && installed_at.elapsed() > std::time::Duration::from_secs(300);
                        if suspect < 2 && !silent_too_long {
                            continue;
                        }
                        suspect = 0;
                        installed_at = std::time::Instant::now();
                        log::debug!(
                            "[KVM] hook watch: putting the keyboard hook back (events={}, injections={}, last vk=0x{:02X})",
                            now_events,
                            now_injections,
                            LAST_VK.load(Ordering::Relaxed)
                        );
                        let tid = HOOK_THREAD.load(Ordering::Relaxed);
                        if tid != 0 {
                            unsafe {
                                let _ = PostThreadMessageW(
                                    tid,
                                    WM_REINSTALL_KEYBOARD,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        }
                    }
                })
                .ok();
        }

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
                    Ok(h) => {
                        MOUSE_HOOK = Some(h);
                        log::info!("[KVM] Windows mouse hook installed");
                    }
                    Err(e) => log::error!("[KVM] mouse hook install failed: {}", e),
                }

                let key_hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_hook_proc),
                    None,
                    0,
                );
                match key_hook {
                    Ok(h) => {
                        KEYBOARD_HOOK = Some(h);
                        log::info!("[KVM] Windows keyboard hook installed");
                    }
                    Err(e) => log::error!("[KVM] keyboard hook install failed: {}", e),
                }

                HOOK_THREAD.store(GetCurrentThreadId(), std::sync::atomic::Ordering::Relaxed);

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_REINSTALL_KEYBOARD {
                        if let Some(old) = KEYBOARD_HOOK.take() {
                            let _ = UnhookWindowsHookEx(old);
                        }
                        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) {
                            Ok(h) => {
                                KEYBOARD_HOOK = Some(h);
                                log::info!("[KVM] Windows keyboard hook put back after a quiet spell");
                            }
                            Err(e) => log::error!("[KVM] keyboard hook reinstall failed: {}", e),
                        }
                        continue;
                    }
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
                let Some(view) = current_view() else {
                    return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
                };
                let has_peer = view.has_peer;
                let is_driving = view.driving;

                let (vx, vy, vw, vh) = get_virtual_desktop_bounds();
                let max_x = vx + vw;
                let max_y = vy + vh;

                let sharing_enabled =
                    view.mouse && view.linked && view.drive_other && !view.paused;

                // The cached view is refreshed a few times a second, which is fine for a
                // switch but too slow for the moment the pointer changes hands: the entry
                // move arrives while the copy still says "home", and the hook would count
                // it as a crossing back. Read where input belongs right now instead; it is
                // one read lock and a four-byte comparison.
                let (live_home, live_driven) = {
                    let s = state.inner.read();
                    (
                        can_start_crossing(&s.input, has_peer, sharing_enabled),
                        s.input == "driven",
                    )
                };

                // A computer the other one is using gives control back as soon as its
                // own mouse is touched. Injected input carries the marker and was
                // passed on above, so anything left is the person at this desk. Without
                // this both computers can end up each believing the other is driving,
                // and then neither takes input at all.
                if live_driven
                    && matches!(
                        msg,
                        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
                    )
                {
                    log::info!("[KVM] Local mouse took control back from the other computer");
                    state.set_where_input("home");
                    refresh_view_now();
                    crate::kvm::reset_pending_motion();
                    crate::fileclip::reset();
                    NativeInputInjector::release_all_keys();
                    let new_snap = state.snapshot();
                    let _ = app.emit("state-change", &new_snap);
                    let home = WireMessage {
                        t: "kvm.home".to_string(),
                        v: Some(1),
                        ..Default::default()
                    };
                    net.send_packet_sync(home);
                }

                if live_home {
                    // Edge check across the entire virtual multi-monitor desktop
                    let crossed = match view.side.as_str() {
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
                        log::info!("[KVM] Cursor crossed to peer: side={}, pt=({}, {})", view.side, m.pt.x, m.pt.y);
                        let is_left_down = (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0;
                        let is_ctrl_down = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;

                        if is_left_down && is_ctrl_down {
                            // Window Edge-Handoff Trigger: Ctrl held while dragging window to edge
                            let _ = app.emit("window-handoff-trigger", serde_json::json!({
                                "x": m.pt.x,
                                "y": m.pt.y,
                                "side": view.side,
                            }));
                        }

                        state.set_where_input("driving");
                        refresh_view_now();
                        let new_snap = state.snapshot();
                        let _ = app.emit("state-change", &new_snap);

                        let take = WireMessage {
                            t: "kvm.take".to_string(),
                            v: Some(1),
                            edge_at: Some(entry_fraction(
                                &view.side,
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

                        // The crossing event is swallowed, so the pointer never actually
                        // reached the edge; move it to the middle of the desktop and count
                        // the next movement from there. Without this the first delta after
                        // taking was the whole distance from the real pointer, which read as
                        // a huge movement the other way and handed control straight back.
                        let (park_x, park_y) = super::drive_park_point(vx, vy, vw, vh);
                        park_cursor_now(park_x, park_y);
                        LAST_PT = POINT { x: park_x, y: park_y };
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

                            let returned = match view.side.as_str() {
                                "right" => VIRTUAL_REMOTE_X < -EDGE_RETURN_DISTANCE,
                                "left" => VIRTUAL_REMOTE_X > EDGE_RETURN_DISTANCE,
                                "top" => VIRTUAL_REMOTE_Y > EDGE_RETURN_DISTANCE,
                                "bottom" => VIRTUAL_REMOTE_Y < -EDGE_RETURN_DISTANCE,
                                _ => false,
                            };

                            if returned {
                                log::info!("[KVM] Cursor returned back to home screen: side={}, virtual_x={:.1}", view.side, VIRTUAL_REMOTE_X);
                                state.set_where_input("home");
                                refresh_view_now();
                                NativeInputInjector::release_all_keys();
                                let new_snap = state.snapshot();
                                let _ = app.emit("state-change", &new_snap);

                                let home_msg = WireMessage {
                                    t: "kvm.home".to_string(),
                                    v: Some(1),
                                    ..Default::default()
                                };
                                net.send_packet_sync(home_msg);

                                let return_x = match view.side.as_str() {
                                    "right" => max_x - 12,
                                    "left" => vx + 12,
                                    _ => m.pt.x,
                                };
                                let return_y = match view.side.as_str() {
                                    "top" => vy + 12,
                                    "bottom" => max_y - 12,
                                    _ => m.pt.y,
                                };
                                // Put the pointer just inside the seam so the next push
                                // produces a movement event the edge can be judged from
                                // again. This has to happen before the next mouse event is
                                // worked out, which is why it runs here rather than being
                                // posted to the main thread.
                                park_cursor_now(return_x, return_y);
                                LAST_PT = POINT { x: return_x, y: return_y };
                                return LRESULT(1);
                            }

                            if dx.abs() > 0.0 || dy.abs() > 0.0 {
                                net.send_motion(dx, dy);
                            }

                            // Put the pointer back at the park point before the next
                            // event arrives, so every event carries only the movement
                            // the hand made since the last one. This is what lets the
                            // whole of a bigger screen be reached: the pointer here
                            // never runs out of screen, because it is never allowed to
                            // travel — its distance from the park point *is* this
                            // event's movement.
                            park_cursor_now(LAST_PT.x, LAST_PT.y);
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
            KEY_EVENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            LAST_VK.store(k.vkCode, std::sync::atomic::Ordering::Relaxed);

            // Pass on keys this app injected itself, otherwise remote keystrokes would be
            // forwarded back to the computer they came from.
            if k.dwExtraInfo == 0x474C4944 {
                return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
            }

            if let (Some(state), Some(net), Some(app)) = (&APP_STATE, &NETWORK_MGR, &APP_HANDLE) {
                let Some(view) = current_view() else {
                    return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
                };
                let is_driving = view.driving;
                let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;

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
                    // Keyboard sharing off: keys typed here stay here, presses and
                    // releases alike, so a key held when the switch went off is not
                    // left down on either computer.
                    if !view.keyboard && is_down {
                        return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
                    }
                    // A right Shift, Ctrl or Alt reported generically keeps its side,
                    // then Ctrl here is Command on the driven computer, so Ctrl+C there
                    // is a copy.
                    const LLKHF_EXTENDED: u32 = 0x01;
                    let raw_vk = k.vkCode as u16;
                    let vk = side_specific_modifier(
                        raw_vk,
                        k.scanCode,
                        k.flags.0 & LLKHF_EXTENDED != 0,
                    );
                    if vk != raw_vk {
                        log::info!(
                            "[KVM] adapter reported a generic modifier: vk=0x{:02X} scan=0x{:02X} ext={} forwarded as 0x{:02X}",
                            raw_vk,
                            k.scanCode,
                            k.flags.0 & LLKHF_EXTENDED != 0,
                            vk
                        );
                    }
                    let vk = translate_modifier_vk(vk, view.peer_modifier) as u32;
                    let key_msg = WireMessage {
                        t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                        v: Some(1),
                        vk: Some(vk),
                        ..Default::default()
                    };
                    if is_down {
                        // A press the link could not take belongs to this computer
                        // instead of vanishing; the person typed it.
                        if net.send_packet_sync(key_msg) == 0 {
                            return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
                        }
                        mark_swallowed_down(raw_vk);
                        return LRESULT(1);
                    }
                    // Its press stayed here (made before control moved, or with
                    // keyboard sharing off), so this release has to stay here too.
                    if !take_swallowed_down(raw_vk) {
                        log::info!(
                            "[KVM] Key release kept here (vk 0x{:02X}): its press stayed on this computer",
                            raw_vk
                        );
                        return CallNextHookEx(HHOOK::default(), n_code, w_param, l_param);
                    }
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
    static mut CURSOR_HIDDEN: bool = false;
    /// Where this app last put the pointer on behalf of the other computer.
    ///
    /// macOS reports the cursor being put somewhere as an ordinary mouse-moved
    /// event from the system (no marker, pid 0). It looks exactly like the person
    /// touching the mouse, and at a seam it also looks like a new crossing, which
    /// is where the blinking and the two arrows at the edge came from. Keeping the
    /// spot we moved to lets those events be recognised and ignored.
    ///
    /// Stored as bits in atomics because the input thread writes this while the
    /// tap thread reads it. NaN means "nowhere yet", which never matches a point.
    static INJECTED_PT_X: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(f64::NAN.to_bits());
    static INJECTED_PT_Y: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(f64::NAN.to_bits());

    pub fn note_injected_pointer(x: f64, y: f64) {
        INJECTED_PT_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
        INJECTED_PT_Y.store(y.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn injected_pointer() -> (f64, f64) {
        (
            f64::from_bits(INJECTED_PT_X.load(std::sync::atomic::Ordering::Relaxed)),
            f64::from_bits(INJECTED_PT_Y.load(std::sync::atomic::Ordering::Relaxed)),
        )
    }

    /// One shared source for the events this app posts, so the suppression interval can
    /// be shortened once instead of leaking a source per event.
    static mut EVENT_SOURCE: CGEventSourceRef = std::ptr::null_mut();

    /// Hide this computer's pointer while it is driving the other one.
    ///
    /// The pointer is pinned to the seam so movement can be measured from it, and
    /// leaving it visible there means a second arrow sits blinking on the edge while
    /// the real one moves on the other screen. macOS keeps the hide count per
    /// connection, so it comes back by itself if this process ever dies.
    pub fn set_cursor_hidden(hidden: bool) {
        unsafe {
            if hidden == CURSOR_HIDDEN {
                return;
            }
            let display = CGMainDisplayID();
            if hidden {
                if CGDisplayHideCursor(display) == 0 {
                    CURSOR_HIDDEN = true;
                }
            } else if CGDisplayShowCursor(display) == 0 {
                CURSOR_HIDDEN = false;
            }
        }
    }

    /// A short suppression interval keeps the system from swallowing the movement that
    /// follows a cursor warp, which matters now that the pointer is put back on the
    /// seam after every event. The default 0.25 s made the pointer feel glued down.
    unsafe fn configure_event_source() {
        if !EVENT_SOURCE.is_null() {
            return;
        }
        let source = CGEventSourceCreate(kCGEventSourceStateCombinedSessionState);
        if !source.is_null() {
            CGEventSourceSetLocalEventsSuppressionInterval(source, 0.05);
            EVENT_SOURCE = source;
        }
    }

    /// The source every event this app posts is created from.
    pub fn event_source() -> CGEventSourceRef {
        unsafe {
            configure_event_source();
            EVENT_SOURCE
        }
    }

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
            unsafe {
                configure_event_source();
            }
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
            set_cursor_hidden(false);
            if let Some(tap) = EVENT_TAP {
                CGEventTapEnable(tap, true);
            }
            return event;
        }

        if event.is_null() {
            return event;
        }

        // Input aimed at this app's own windows — its page, its menu, the system
        // panels it opens — belongs to whoever is using GlideDesk, not to the other
        // computer. Passing it through untouched keeps every control clickable while
        // a peer is connected; intercepting our own clicks makes them land late or
        // not at all, which looks like a frozen window.
        // Input aimed at this app's own windows belongs to whoever is using the
        // app, not to the other computer. macOS fills this field with the frontmost
        // process for ordinary input, so it is only used to keep clicks, keys and
        // wheel events away from the forwarding code while this computer is at home;
        // pointer movement still goes through, because crossing the screen edge is
        // how control is handed over and the app is often the frontmost window.
        let target_is_self = CGEventGetIntegerValueField(event, kCGEventTargetUnixProcessID)
            == std::process::id() as i64;

        // Movement and clicks this app posted for the other computer are not the
        // person at this desk. The marker travels with the event, so this is exact
        // where timing alone could only guess.
        if crate::kvm::is_injected_event(event) {
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

            // While this computer is driving the other one its pointer is pinned to
            // the seam and means nothing, so it is hidden: otherwise a second arrow
            // sits blinking on the edge while the real one moves on the other screen.
            set_cursor_hidden(is_driving);

            // Putting the pointer down is reported by the system as an ordinary
            // mouse-moved event. It is not the person at this desk, and at a seam it
            // also looks like a fresh crossing, which is what made the pointer blink
            // at the edge and left both computers driving each other.
            let here = CGEventGetLocation(event);
            let (inj_x, inj_y) = injected_pointer();
            let at_injected_point = (here.x - inj_x).abs() < 4.0 && (here.y - inj_y).abs() < 4.0;
            let is_move = r#type == kCGEventMouseMoved
                || r#type == kCGEventLeftMouseDragged
                || r#type == kCGEventRightMouseDragged
                || r#type == kCGEventOtherMouseDragged;

            // While this computer is at home, a click or a keystroke in GlideDesk's
            // own window must reach that window untouched, and must not travel to the
            // other computer. Once this computer is driving the other one, the app's
            // window is behind the pointer and its input belongs to the far side, so
            // the guard is skipped.
            if target_is_self && !is_driving && !is_move {
                return event;
            }

            // A computer the other one is using gives control back as soon as its own
            // mouse is touched: events this app injected were filtered out above, so
            // anything left is the person at this desk. Without this both computers can
            // end up each believing the other is driving, and neither takes input.
            if snap.input == "driven"
                && (r#type == kCGEventMouseMoved
                    || r#type == kCGEventLeftMouseDown
                    || r#type == kCGEventRightMouseDown)
            {
                if r#type == kCGEventMouseMoved && at_injected_point {
                    // The cursor being put where it already is, not a hand moving it.
                    LAST_PT = here;
                    return event;
                }
                log::info!(
                    "[KVM] Local mouse took control back from the other computer at ({:.1}, {:.1}); {}",
                    here.x,
                    here.y,
                    crate::kvm::injected_event_debug(event)
                );
                state.set_where_input("home");
                crate::kvm::reset_pending_motion();
                crate::fileclip::reset();
                NativeInputInjector::release_all_keys();
                let new_snap = state.snapshot();
                let _ = app.emit("state-change", &new_snap);
                let home_msg = WireMessage {
                    t: "kvm.home".to_string(),
                    v: Some(1),
                    ..Default::default()
                };
                net.send_packet_sync(home_msg);
                set_cursor_hidden(false);
                LAST_PT = here;
                // This event brought control back; it is not also a crossing.
                return event;
            }

            if can_start_crossing(&snap.input, has_peer, sharing_enabled) {
                if is_move {
                    let pt = CGEventGetLocation(event);
                    LAST_PT = pt;
                    if at_injected_point {
                        return event;
                    }

                    // Asked on every move, so that leaving the edge clears the
                    // clock and the next approach starts over.
                    let crossed = edge_push_ready(
                            &snap.layout.side,
                            pt.x,
                            pt.y,
                            vx,
                            vy,
                            vw,
                            vh,
                            now_ms,
                        )
                        && check_edge_crossed(&snap.layout.side, pt.x, pt.y, vx, vy, vw, vh);

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

                        let park_pt = {
                            let (park_x, park_y) = super::drive_park_point(vx, vy, vw, vh);
                            CGPoint { x: park_x as f64, y: park_y as f64 }
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

                        // Put the pointer back at the park point before the next
                        // event arrives, so every event carries only the movement
                        // the hand made since the last one. This is what lets the
                        // whole of a bigger screen be reached: the pointer here
                        // never runs out of screen, because it is never allowed to
                        // travel — its distance from the park point *is* this
                        // event's movement.
                        let _ = CGWarpMouseCursorPosition(LAST_PT);
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
                        log::debug!("[KVM] forward wheel dx={} dy={}", dx, dy);
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

                        // Sharing off: presses stay here, so the window under the
                        // pointer still gets them; releases pass too, so a key held
                        // when the switch went off cannot stay stuck down.
                        if !snap.kvm.keyboard && is_down {
                            return event;
                        }

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

                        let vk = translate_modifier_vk(
                            keycodes::macos_keycode_to_windows_vk(keycode) as u16,
                            peer_command_modifier_vk(&snap),
                        );
                        let key_msg = WireMessage {
                            t: if is_down { "k.kd".to_string() } else { "k.ku".to_string() },
                            v: Some(1),
                            vk: Some(vk as u32),
                            ..Default::default()
                        };
                        if is_down {
                            if net.send_packet_sync(key_msg) == 0 {
                                return event;
                            }
                            mark_swallowed_down(keycode);
                            return std::ptr::null_mut();
                        }
                        // The press stayed here (it was made before control moved):
                        // the release has to stay here too, or the modifier stays
                        // down in the app that saw it.
                        if !take_swallowed_down(keycode) {
                            log::info!(
                                "[KVM] Key release kept here (code {}): its press stayed on this computer",
                                keycode
                            );
                            return event;
                        }
                        net.send_packet_sync(key_msg);
                        return std::ptr::null_mut();
                    }
                    kCGEventKeyDown | kCGEventKeyUp => {
                        let keycode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) as u16;
                        let is_down = r#type == kCGEventKeyDown;

                        // Sharing off: presses stay here, so the window under the
                        // pointer still gets them; releases pass too, so a key held
                        // when the switch went off cannot stay stuck down.
                        if !snap.kvm.keyboard && is_down {
                            return event;
                        }

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

                        let vk = translate_modifier_vk(
                            keycodes::macos_keycode_to_windows_vk(keycode) as u16,
                            peer_command_modifier_vk(&snap),
                        ) as u32;
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
                        // A key that never reached the other computer (link full or
                        // gone) is given back to this one instead of vanishing: the
                        // person typed it, and silence on both screens is the one
                        // answer that cannot be right.
                        if is_down {
                            if net.send_packet_sync(key_msg) == 0 {
                                return event;
                            }
                            mark_swallowed_down(keycode);
                            return std::ptr::null_mut();
                        }
                        // Its press stayed on this computer (made before control
                        // moved, or before keyboard sharing came back on), so the
                        // release must be delivered here too; swallowing it alone
                        // leaves that key held down for good.
                        if !take_swallowed_down(keycode) {
                            log::info!(
                                "[KVM] Key release kept here (code {}): its press stayed on this computer",
                                keycode
                            );
                            return event;
                        }
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

        if can_start_crossing(&snap.input, has_peer, sharing_enabled) {
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

                let (park_x, park_y) = drive_park_point(vx, vy, vw, vh);
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

            // Keep cursor parked in the middle of the desktop
            let (park_x, park_y) = drive_park_point(vx, vy, vw, vh);
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

    /// Bounds of the X11 root window, reported in the same shape as the other
    /// platforms so shared pointer handling does not need a special case.
    #[cfg(target_os = "linux")]
    pub fn get_virtual_desktop_bounds() -> (i32, i32, i32, i32) {
        match get_linux_backend() {
            Ok(backend) => match backend.get_viewport_dimensions() {
                Ok((w, h)) if w > 2 && h > 2 => (0, 0, w as i32, h as i32),
                _ => (0, 0, 0, 0),
            },
            Err(_) => (0, 0, 0, 0),
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn get_virtual_desktop_bounds() -> (i32, i32, i32, i32) {
        (0, 0, 0, 0)
    }

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
    fn motion_scale_maps_a_small_screen_onto_a_big_one() {
        // A 1366×768 PC beside a 1920×1080 Mac: without scaling the pointer
        // could only reach two thirds of the Mac before hitting the PC's edge.
        let (sx, sy) = motion_scale(1366.0, 768.0, 1920.0, 1080.0);
        assert!((sx - 1920.0 / 1366.0).abs() < 1e-9);
        assert!((sy - 1080.0 / 768.0).abs() < 1e-9);
        assert!(sx > 1.0 && sy > 1.0);
    }

    #[test]
    fn motion_scale_is_one_for_matching_or_unknown_screens() {
        assert_eq!(motion_scale(1920.0, 1080.0, 1920.0, 1080.0), (1.0, 1.0));
        // A peer that has not said how big it is keeps the old 1:1 behaviour.
        assert_eq!(motion_scale(1920.0, 1080.0, 0.0, 0.0), (1.0, 1.0));
        // A nonsensical size must not turn a normal move into a jump.
        assert_eq!(motion_scale(1920.0, 1080.0, 100.0, 10_000_000.0), (0.25, 4.0));
    }

    #[test]
    fn motion_accumulator_carries_the_fraction_between_packets() {
        let mut a = MotionAccumulator::default();
        // 1.4 px per packet at three packets: 0, 0, then 4 (0.2 left over).
        assert_eq!(a.add(1.4, 0.0), (1, 0));
        assert_eq!(a.add(1.4, 0.0), (1, 0));
        assert_eq!(a.add(1.4, 0.0), (2, 0));
        assert_eq!(a.add(0.2, 0.0), (0, 0));
        // Negative motion is carried the same way.
        a.reset();
        assert_eq!(a.add(-0.6, -0.6), (0, 0));
        assert_eq!(a.add(-0.6, -0.6), (-1, -1));
    }

    #[test]
    fn remote_clipboard_echo_is_swallowed_once() {
        // The other computer put this on our clipboard: the watcher must not send it
        // back, and must not keep suppressing the same text after that one look.
        note_remote_clipboard_text("from the other computer");
        assert!(take_remote_text("from the other computer"));
        assert!(!take_remote_text("from the other computer"));

        // A genuinely new local copy is never mistaken for an echo.
        assert!(!take_remote_text("typed here"));
    }

    #[test]
    fn remote_clipboard_picture_echo_is_swallowed_once() {
        note_remote_clipboard_image(4096);
        assert!(take_remote_image(4096));
        assert!(!take_remote_image(4096));
        assert!(!take_remote_image(1024));
    }

    #[test]
    fn clipboard_thumb_is_a_small_data_url() {
        // A 400 x 200 red picture becomes a 96 px wide preview, still a PNG data URL.
        let img = image::RgbaImage::from_pixel(400, 200, image::Rgba([255, 0, 0, 255]));
        let mut cursor = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        let png = cursor.into_inner();

        let thumb = clipboard_thumb(&png, 400, 200, true).expect("a thumbnail");
        assert_eq!(thumb["w"], 400);
        assert_eq!(thumb["h"], 200);
        assert_eq!(thumb["shot"], true);
        let url = thumb["thumb"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        // Far smaller than the original, so the Recent list stays light.
        assert!(url.len() < png.len() * 2);
    }

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
    fn a_driven_computer_never_starts_a_crossing() {
        // Only the computer being used at this desk hands the pointer over. While
        // the other one drives this computer, injected moves reach the same edges
        // and must never be read as a fresh crossing; that was the loop that made
        // each computer take the pointer back and the cursor crawl.
        assert!(can_start_crossing("home", true, true));
        assert!(!can_start_crossing("driving", true, true));
        assert!(!can_start_crossing("driven", true, true));
        assert!(!can_start_crossing("home", false, true));
        assert!(!can_start_crossing("home", true, false));
    }

    #[test]
    fn the_driving_park_point_leaves_room_on_every_side() {
        // A single 1920x1080 desktop at the origin parks in the middle.
        assert_eq!(drive_park_point(0, 0, 1920, 1080), (960, 540));

        // A monitor to the left moves the whole desktop's origin, and the park
        // point moves with it instead of landing outside the screens.
        assert_eq!(drive_park_point(-1920, 0, 3840, 1080), (0, 540));

        // However the desktop is laid out, there is a deliberate move to be made
        // towards every edge. A pointer parked in the seam could only report the
        // few pixels between itself and the edge, and that is what stopped the
        // other computer's pointer a few pixels in.
        for (vx, vy, vw, vh) in [
            (0, 0, 1920, 1080),
            (0, 0, 1366, 768),
            (-1920, -200, 3840, 1440),
        ] {
            let (px, py) = drive_park_point(vx, vy, vw, vh);
            assert!(px - vx >= 300 && (vx + vw) - px >= 300, "no room left/right on {vw}x{vh}");
            assert!(py - vy >= 200 && (vy + vh) - py >= 200, "no room up/down on {vw}x{vh}");
            // And the park point is not itself on an edge, so it cannot
            // immediately read as another crossing.
            assert!(!check_edge_crossed("left", px as f64, py as f64, vx, vy, vw, vh));
            assert!(!check_edge_crossed("right", px as f64, py as f64, vx, vy, vw, vh));
            assert!(!check_edge_crossed("top", px as f64, py as f64, vx, vy, vw, vh));
            assert!(!check_edge_crossed("bottom", px as f64, py as f64, vx, vy, vw, vh));
        }
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
        assert!(check_edge_returned("right", -24.5, 0.0));
        assert!(!check_edge_returned("right", -24.0, 0.0));
        assert!(!check_edge_returned("right", -5.0, 0.0));

        // Left side: peer is to the left. Returning means moving right past seam (virtual_x > 8.0)
        assert!(check_edge_returned("left", 24.5, 0.0));
        assert!(!check_edge_returned("left", 24.0, 0.0));
        assert!(!check_edge_returned("left", 5.0, 0.0));

        // Top side: peer is above. Returning means moving down past seam (virtual_y > 8.0)
        assert!(check_edge_returned("top", 0.0, 24.5));
        assert!(!check_edge_returned("top", 0.0, 24.0));

        // Bottom side: peer is below. Returning means moving up past seam (virtual_y < -8.0)
        assert!(check_edge_returned("bottom", 0.0, -24.5));
        assert!(!check_edge_returned("bottom", 0.0, -24.0));

        // Unknown side never returns
        assert!(!check_edge_returned("unknown", -100.0, -100.0));
    }

    #[test]
    fn test_edge_crossing_needs_a_deliberate_push() {
        // Both halves in one test: the dwell clock is shared state, and two tests
        // running at once would each see the other's touch.
        reset_edge_dwell();

        // A pointer that is nowhere near the edge neither crosses nor starts a clock.
        assert!(!edge_push_ready("left", 400.0, 500.0, 0, 0, 1920, 1080, 1_000));

        // Reaching the edge is not crossing yet: it only starts the clock.
        assert!(!edge_push_ready("left", 0.0, 500.0, 0, 0, 1920, 1080, 1_000));
        assert!(!edge_push_ready("left", 0.0, 500.0, 0, 0, 1920, 1080, 1_100));

        // Still pressing there after the dwell: this is meant.
        assert!(edge_push_ready("left", 0.0, 500.0, 0, 0, 1920, 1080, 1_160));

        // The crossing cleared the clock, so the next event starts over.
        assert!(!edge_push_ready("left", 0.0, 500.0, 0, 0, 1920, 1080, 1_180));
        assert!(!edge_push_ready("left", 0.0, 500.0, 0, 0, 1920, 1080, 1_250));

        // Moving away before the dwell means the brush never counts.
        assert!(!edge_push_ready("bottom", 500.0, 1080.0, 0, 0, 1920, 1080, 2_000));
        assert!(!edge_push_ready("bottom", 500.0, 900.0, 0, 0, 1920, 1080, 2_050));
        assert!(!edge_push_ready("bottom", 500.0, 1080.0, 0, 0, 1920, 1080, 2_100));
        assert!(edge_push_ready("bottom", 500.0, 1080.0, 0, 0, 1920, 1080, 2_270));

        // A clock left over from one edge does not carry to the opposite one.
        assert!(!edge_push_ready("right", 1920.0, 500.0, 0, 0, 1920, 1080, 3_000));
        assert!(!edge_push_ready("right", 1920.0, 500.0, 0, 0, 1920, 1080, 3_100));
        assert!(edge_push_ready("right", 1920.0, 500.0, 0, 0, 1920, 1080, 3_170));
        reset_edge_dwell();
    }

    #[test]
    fn test_release_belongs_where_its_press_went() {
        // A press taken away for the other computer takes its release with it...
        mark_swallowed_down(55);
        assert!(take_swallowed_down(55));
        // ...once: a second release for the same key is the app's business.
        assert!(!take_swallowed_down(55));

        // A press that stayed here keeps its release here. This is the case that
        // used to leave Command held down: pressed before the pointer moved, released
        // after, with the release swallowed and sent away.
        assert!(!take_swallowed_down(57));

        // Coming back home frees anything that never came up.
        mark_swallowed_down(56);
        mark_swallowed_down(55);
        assert_eq!(swallowed_down_count(), 2);
        let freed = take_all_swallowed_downs();
        assert_eq!(freed.len(), 2);
        assert_eq!(swallowed_down_count(), 0);
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
    fn shortcut_modifier_follows_the_driven_computer() {
        const CTRL: u16 = 0x11;
        const LCTRL: u16 = 0xA2;
        const COMMAND: u16 = 0x5B;
        const RCOMMAND: u16 = 0x5C;

        // Driving a Mac: Ctrl becomes Command, so Ctrl+C is a copy there.
        assert_eq!(translate_modifier_vk(CTRL, Some(COMMAND)), COMMAND);
        assert_eq!(translate_modifier_vk(LCTRL, Some(COMMAND)), COMMAND);

        // Driving Windows: Command becomes Ctrl, so Command+C is a copy there.
        assert_eq!(translate_modifier_vk(COMMAND, Some(CTRL)), CTRL);
        assert_eq!(translate_modifier_vk(RCOMMAND, Some(CTRL)), CTRL);

        // Typing keys never change, and an unknown peer leaves keys alone.
        assert_eq!(translate_modifier_vk(0x43, Some(COMMAND)), 0x43);
        assert_eq!(translate_modifier_vk(0x43, Some(CTRL)), 0x43);
        assert_eq!(translate_modifier_vk(CTRL, Some(CTRL)), CTRL);
        assert_eq!(translate_modifier_vk(CTRL, None), CTRL);
    }

    #[test]
    fn right_shift_is_identified_even_when_the_adapter_flags_it_extended() {
        const SHIFT: u16 = 0x10;
        const CONTROL: u16 = 0x11;
        const MENU: u16 = 0x12;

        // Adapters that report the generic code: the scan code still says "right".
        assert_eq!(side_specific_modifier(SHIFT, 0x36, false), 0xA1); // VK_RSHIFT
        assert_eq!(side_specific_modifier(SHIFT, 0x2A, false), SHIFT); // left Shift
        assert_eq!(side_specific_modifier(CONTROL, 0x1D, true), 0xA3); // VK_RCONTROL
        assert_eq!(side_specific_modifier(CONTROL, 0x1D, false), CONTROL);
        assert_eq!(side_specific_modifier(MENU, 0x38, true), 0xA5); // VK_RMENU
        assert_eq!(side_specific_modifier(MENU, 0x38, false), MENU);

        // Already side-specific or an ordinary key: leave it alone.
        assert_eq!(side_specific_modifier(0xA1, 0x36, false), 0xA1);
        assert_eq!(side_specific_modifier(0x34, 0x36, false), 0x34); // the Shift key's neighbour
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
