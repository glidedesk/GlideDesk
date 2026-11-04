# GlideDesk Linux KVM Architecture & Platform Capabilities

GlideDesk provides native Linux support for keyboard, video, and mouse (KVM) multi-device sharing and file transfer across the mesh network via pure-Rust X11 protocol integration (`x11rb`). No runtime C helpers (such as `libxdo` or `libX11`) are required or dynamically loaded.

## Input Injection Architecture

Input injection on Linux is implemented in pure Rust via `x11rb` (`X11RbBackend`) talking directly to the X server:
- **XTest Extension**: Employs the standard X11 XTEST extension (`xtest::fake_input`) for synthetic hardware-level event generation without external C dynamic libraries.
- **Mouse Relative Motion**: Current root coordinates are read via core protocol `query_pointer`, deltas applied, and pointer positioned via `warp_pointer`.
- **Mouse Absolute Warp**: Pointer positioned to screen coordinates via `xproto::warp_pointer`.
- **Mouse Buttons**: Injected using `xtest::fake_input` with `BUTTON_PRESS_EVENT` and `BUTTON_RELEASE_EVENT` (Left=1, Middle=2, Right=3).
- **Mouse Wheel**: Emulated via X11 button pulses (Scroll Up=4, Scroll Down=5, Scroll Right=7, Scroll Left=6) clamped to 10 notches, returning explicit errors if the connection or server rejects an event.
- **Keyboard Events**: Pure virtual key mapping table `vk_to_x_keysym` converts Windows Virtual-Key (VK) codes into standard numeric X11 keysym values (`XK_Return`, `XK_BackSpace`, `XK_Tab`, `XK_Escape`, `XK_Shift_L`, `XK_Control_L`, `XK_Alt_L`, `XK_Super_L`, etc.). At connect time, `xproto::get_keyboard_mapping` resolves active server keycodes for each keysym, and events are dispatched via `xtest::fake_input(KEY_PRESS_EVENT / KEY_RELEASE_EVENT, keycode)`.
- **Pre-Release Cleanups**: Platform-agnostic `release_all_keys` automatically drains all held keys and buttons to prevent stuck keys across boundary crossings.

## Input Capture & Session Limitations

Input capture on Linux requires listening to global input events across display boundaries:

1. **Active X11 Session (Capture Engine Running)**:
   - In an interactive X11 desktop session (`DISPLAY` set) where the capture loop is active:
   - **Pointer & Key Tracking**: Real-time cursor monitoring via pure-Rust `xproto::query_pointer` and global key scanning via `xproto::query_keymap` running in a dedicated ~120 Hz polling thread.
   - **Self-Echo Suppression**: Injected events are timestamped on generation (`LAST_INJECTION_TIMESTAMP_MS`) and filtered through `should_forward_captured_event` (25 ms suppression window) to prevent feedback loops when remote inputs are injected locally.
   - **Edge Crossing & Window Handoff**: Monitored against virtual desktop boundaries (`check_edge_crossed` / `check_edge_returned`). Pushing against the configured edge emits `window-handoff-trigger` if Ctrl+drag is active, transitions input mode to `driving`, transmits `kvm.take`, and parks the cursor via `xproto::warp_pointer`.
   - **Driving Mode**: Streaming motion (`k.mm`), mouse buttons (`k.md`/`k.mu`), wheel events (`k.wh`), and keystrokes mapped to virtual keycodes via `x_keycode_to_vk` (`k.kd`/`k.ku`). Returning past the edge boundary restores the pointer locally, triggers `release_all_keys()`, and transmits `kvm.home`.
   - Status: `helper = "x11"`, `message = "Linux input capture: active X11 session (capture engine running; x11rb ready)"`.

2. **Active X11 Session (Capture Engine Not Running)**:
   - When an X11 display is detected but the capture engine has not started, stopped, or failed to initialize:
   - Status: `helper = "x11rb"`, `message = "Linux input capture: active X11 session detected but capture engine not running; local capture unavailable (injection ready)"`.

3. **Wayland Sessions**:
   - Due to the Wayland security model, unprivileged background applications are strictly prohibited by the compositor from capturing global raw input events outside their own surfaces.
   - Without root privileges or dedicated compositor protocol extensions (e.g., `ext-foreign-toplevel-list` / custom input capture protocols), global edge crossing capture is restricted.
   - Status: `helper = "wayland"`, `message = "Linux input capture: Wayland session detected; global raw input capture unavailable (requires X11 session or root uinput privileges)"`.

4. **Headless / CI Environments**:
   - In automated testing or headless server environments where `DISPLAY` is not defined:
   - Status: `helper = "x11rb"`, `message = "Linux input capture: headless session (DISPLAY not set); remote input injection and local capture unavailable"`.
   - GlideDesk degrades gracefully with honest capability reporting on `KvmRuntimeState` rather than panicking or failing silently.

## Background Presentation & Tray Execution

GlideDesk runs as a persistent background daemon when the primary window is closed or when launched with `--daemon`.

- **X11 Taskbar Exclusion**:
  On X11 environments, hiding the main window sets `set_skip_taskbar(true)`, which translates to setting the `_NET_WM_STATE_SKIP_TASKBAR` property on the client window. Standard window managers (GNOME Shell, KDE KWin, XFCE xfwm4, etc.) omit the window from panel taskbars and pager switchers while the tray icon remains active. When restored from the system tray, `set_skip_taskbar(false)` clears the hint and re-registers the window on the taskbar.
- **Wayland Compositor Caveat**:
  Under native Wayland sessions (such as GNOME Wayland or Sway), window taskbar presentation is governed entirely by the compositor rather than standard X11 window hints. While Tauri/GTK sets the equivalent surface hints, some Wayland compositors may ignore client-side requests to skip dock/panel listing. Full exclusion from taskbar panels is guaranteed on X11 and XWayland-managed windows.

## Native Screen Frame Capture Architecture

GlideDesk streams primary display frames natively across platforms. On Linux:

- **X11 Protocol Screen Capture via `x11rb`**:
  Captures display frames directly using standard X11 `xproto::get_image` (`Z_PIXMAP`) against the root window (`screen.root`). The raw pixel buffer is decoded from X11 TrueColor (BGR0 / BGRA 32-bit depth) into standard RGBA for PNG frame encoding in `ScreenEngine`.
- **Connection Loss & Auto-Shutdown**:
  If the X server connection fails or drops during active streaming, `ScreenEngine` detects consecutive capture failures (threshold: 5 errors) and halts streaming cleanly without spinning in a failure loop.
- **Wayland Limitation**:
  Native Wayland compositors block unprivileged background applications from capturing arbitrary desktop buffers outside their own surface hierarchy. Screen capture is supported on standard X11 sessions and XWayland-hosted environments; native Wayland environments require desktop portal/PipeWire capture. Headless environments without `DISPLAY` report an honest unavailable status.
