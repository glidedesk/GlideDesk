# Window Edge-Handoff Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drag an individual running desktop application window across screen edges between physical computers with automatic parking, single-window projection, and crash-safe restoration.

**Architecture:** Detects titlebar edge-drags via low-level OS hooks, exchanges window metadata over WebSocket mesh, parks the source window off-screen at `(-32000, -32000)` while streaming video/input to a floating canvas window on the target machine, and provides a heartbeat watchdog that restores all windows if a connection drops.

**Tech Stack:** Rust (Tauri v2, Tokio, Windows API / CoreGraphics), HTML5/Canvas/CSS, WebRTC/WebSockets.

**Spec:** [`docs/superpowers/specs/2026-10-29-window-edge-handoff-spec.md`](../specs/2026-10-29-window-edge-handoff-spec.md)

---

## Global Constraints

- **Non-destructive Window Operations**: Windows must never be killed or closed during handoff; they are only parked off-screen and restored.
- **Zero Memory Leaks**: All parked window handles (`HWND` on Windows, window IDs on macOS) must be tracked in a managed registry and cleaned up on unpark.
- **Automatic Fallback Recovery**: If any peer disconnects, heartbeats time out (12s), or GlideDesk exits, all parked windows MUST automatically restore to the primary display.
- **Backward Compatibility**: Single-computer KVM and standard cursor switching must continue working without interference when modifier keys are not held.

## Review Focus

1. **Watchdog Failure**: Peer connection drops while a window is parked off-screen; verify the window returns to `(100, 100)` within 12 seconds.
2. **Accidental Edge Snapping**: User drags a window to snap it to the side of their screen without modifier; verify standard OS window snapping works and handoff does NOT trigger.
3. **Multiple Window Handoffs**: Multiple windows handed over simultaneously; verify `ParkedWindowRegistry` manages distinct handles without collisions.
4. **App Termination on Source**: User kills the source app while handed over; verify target window closes cleanly without dangling ghost views.
5. **DPI & Scale Factor Mismatch**: Source monitor is 200% scale (Retina/4K), target is 100%; verify canvas coordinates scale mouse clicks accurately.

---

## Task 1: Parked Window Registry & Safety Watchdog (Rust Backend)

**Files:**
- Create: `src-tauri/src/window_manager.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/state.rs`

- [x] **Step 1.1**: Write unit tests for `ParkedWindowRegistry` testing window registration, coordinate preservation, off-screen coordinate calculations, and bulk restoration.
- [x] **Step 1.2**: Implement `ParkedWindowRegistry` with thread-safe `Arc<RwLock<HashMap<String, WindowParkState>>>`.
- [x] **Step 1.3**: Implement native platform window parking:
  - Windows: `SetWindowPos(hwnd, ..., -32000, -32000, ...)`
  - macOS: `CGWindowSetFrame` / `NSRect` off-screen frame
- [x] **Step 1.4**: Implement `restore_all_parked_windows()` restoring original coordinates or centering on the primary display.
- [x] **Step 1.5**: Run `cargo test --manifest-path src-tauri/Cargo.toml` and confirm tests pass.
- [x] **Step 1.6**: Commit Task 1.

---

## Task 2: Wire Protocol & Tauri Commands for Window Handoff

**Files:**
- Modify: `src-tauri/src/network.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `ui/shared/tauri-bridge.js`

- [x] **Step 2.1**: Add `window.handoff.offer`, `window.handoff.accept`, `window.handoff.return`, and `window.handoff.close` message types to `WireMessage` in `network.rs`.
- [x] **Step 2.2**: Implement Tauri commands in `commands.rs`:
  - `handoff_window(id: String, title: String, width: u32, height: u32)`
  - `return_window(id: String)`
  - `get_parked_windows()`
- [x] **Step 2.3**: Expose `handoffWindow`, `returnWindow`, and `getParkedWindows` in `ui/shared/tauri-bridge.js`.
- [x] **Step 2.4**: Run `cargo check` and verify compilation.
- [x] **Step 2.5**: Commit Task 2.

---

## Task 3: Edge-Drag Detection in Low-Level Hook Engine

**Files:**
- Modify: `src-tauri/src/kvm.rs`

- [x] **Step 3.1**: In `mouse_hook_proc` (Windows `WH_MOUSE_LL` / macOS `CGEventTap`), check if `WM_LBUTTONDOWN` / left mouse drag is active.
- [x] **Step 3.2**: Check whether `Ctrl` (Windows/Linux) or `Cmd` (macOS) is held during the boundary drag.
- [x] **Step 3.3**: Query window title bar under cursor:
  - Windows: `SendMessage(hwnd, WM_NCHITTEST, ..., pt) == HTCAPTION`
  - macOS: `AXUIElementCopyElementAtPosition`
- [x] **Step 3.4**: When boundary threshold is crossed with modifier held, initiate `window.handoff.offer` and park the window off-screen.
- [x] **Step 3.5**: Run `cargo check` and `cargo test`.
- [x] **Step 3.6**: Commit Task 3.

---

## Task 4: Target Machine Floating Canvas Window & Shell

**Files:**
- Create: `ui/window-projection.html`
- Create: `ui/window-projection.js`
- Create: `ui/window-projection.css`
- Modify: `src-tauri/tauri.conf.json`

- [x] **Step 4.1**: Create `window-projection.html` with a native-styled window titlebar (App Name, Return button, Close button) and video canvas viewport.
- [x] **Step 4.2**: Implement `window-projection.js` listening for remote window streams and capturing local mouse/keyboard input to proxy back via `tauri-bridge.js`.
- [x] **Step 4.3**: Add "Return to Source" button triggering `return_window`.
- [x] **Step 4.4**: Verify JS syntax via `node --check ui/window-projection.js`.
- [x] **Step 4.5**: Commit Task 4.

---

## Task 5: Watchdog Heartbeat Integration & End-to-End Verification

**Files:**
- Modify: `src-tauri/src/network.rs`
- Modify: `src-tauri/src/main.rs`

- [x] **Step 5.1**: In `NetworkManager::run_heartbeat`, trigger `restore_all_parked_windows()` if remote peer heartbeat expires or WebSocket disconnects.
- [x] **Step 5.2**: Register cleanup hook on application exit in `main.rs` to guarantee no window remains parked off-screen.
- [x] **Step 5.3**: Run `cargo test` and verify all tests pass.
- [x] **Step 5.4**: Commit Task 5.
