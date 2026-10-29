# Window Edge-Handoff Specification

**Feature:** Single-Window Cross-Computer Drag & Projection  
**Target:** GlideDesk (Tauri v2 + Rust)  
**Reference Issue:** InukaAsith/TwinDesk#18  
**Author:** GlideDesk Engineering Team  
**Date:** 2026-10-29  

---

## 1. Executive Summary & Goals

GlideDesk currently provides low-latency mouse and keyboard sharing (KVM), synchronized clipboard (text and pictures), and file transfer across physical computers and mobile devices. 

This specification introduces **Window Edge-Handoff**: the ability to drag an individual running desktop application window from one physical computer across the screen boundary onto another physical computer, where it appears as a native floating window. The original window is parked off-screen on the source computer, ensuring it occupies space on only one computer at a time.

---

## 2. User Experience & Gestures

1. **Initiating Handoff**:
   - The user clicks and drags the title bar of any window toward the screen edge leading to a connected peer.
   - Holding `Ctrl` (Windows/Linux) or `Cmd` (macOS) while dragging to the boundary signals a **Window Handoff** rather than a standard cursor-only boundary crossing.
2. **Boundary Crossing**:
   - As the dragged window touches the edge, the source computer captures its window geometry, title, and process metadata.
   - A `window.handoff.offer` packet is dispatched to the peer over the existing WebSocket mesh link.
   - The source computer parks the local window off-screen at coordinates `(-32000, -32000)`, removing it from the user's active display without stopping or pausing the underlying process.
3. **Display on Target Computer**:
   - The target computer instantiates a floating, borderless window with the exact aspect ratio, icon, and title of the source application (prefixed with `[GlideDesk: AppName]`).
   - A low-latency hardware-accelerated video stream (60 fps) of the single window is rendered into the canvas.
   - Mouse clicks, scrolls, and keystrokes inside the projection window are forwarded directly back to the source process.
4. **Returning or Closing**:
   - Dragging the projected window back across the edge to the source computer terminates the remote session and restores the local window to `(100, 100)` on the primary screen.
   - Closing the projected window on the target machine closes the remote session and restores the local window on the source machine.
5. **Fail-Safe Recovery**:
   - If the network drops, the peer crashes, or GlideDesk closes, a local watchdog timer immediately unparks and restores all off-screen windows to the primary display.

---

## 3. Wire Protocol Additions

All protocol additions are backward-compatible and additive to `WireMessage`:

### `window.handoff.offer`
Sent by the source machine when a window hits the boundary:
```json
{
  "t": "window.handoff.offer",
  "v": 1,
  "id": "win_0x140a2b",
  "name": "Visual Studio Code",
  "w": 1280,
  "height": 800,
  "os": "win"
}
```

### `window.handoff.accept`
Sent by the target machine acknowledging receipt and window creation:
```json
{
  "t": "window.handoff.accept",
  "v": 1,
  "id": "win_0x140a2b"
}
```

### `window.handoff.return`
Sent when the window is dragged back across the edge or closed:
```json
{
  "t": "window.handoff.return",
  "v": 1,
  "id": "win_0x140a2b"
}
```

---

## 4. Native OS Capture & Window Management

### Windows (10 & 11)
- **Drag Detection**: `WH_MOUSE_LL` hook identifies cursor in `HTCAPTION` (`SendMessage(hwnd, WM_NCHITTEST, ...)`).
- **Parking**: `SetWindowPos(hwnd, HWND_TOP, -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE)`.
- **Capture**: `Windows.Graphics.Capture` (WGC) for hardware-accelerated capture of individual `HWND`.
- **Input Forwarding**: Targeted `PostMessageW` / `SendInput` coordinate mapping.

### macOS (Sonoma & Sequoia)
- **Drag Detection**: `CGEventTap` combined with `AXUIElementCopyElementAtPosition`.
- **Parking**: Move window to off-screen frame coordinates (`NSRect(x: -32000, y: -32000, ...)`).
- **Capture**: `ScreenCaptureKit` (`SCStream`) targeting specific `SCWindow`.
- **Input Forwarding**: `CGEventPostToPid` with local window relative offsets.

---

## 5. Safety & Watchdog Architecture

```rust
pub struct ParkedWindowRegistry {
    parked: HashMap<String, WindowParkState>,
}

pub struct WindowParkState {
    pub id: String,
    pub original_x: i32,
    pub original_y: i32,
    pub width: u32,
    pub height: u32,
    pub peer_id: String,
    pub last_heartbeat: Instant,
}
```
If a peer silent duration exceeds `HEARTBEAT_TIMEOUT` (12s) or `AppState` disconnects, `restore_all_parked_windows()` is automatically invoked.
