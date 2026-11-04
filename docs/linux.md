# GlideDesk Linux KVM Architecture & Platform Capabilities

GlideDesk provides native Linux support for keyboard, video, and mouse (KVM) multi-device sharing and file transfer across the mesh network.

## Input Injection Architecture

Input injection on Linux is implemented via `libxdo` (`XdoBackend`), interacting directly with the X11 display server:
- **Mouse Relative Motion**: `xdo_move_mouse_relative(dx, dy)`
- **Mouse Absolute Warp**: `xdo_move_mouse(x, y, screen)`
- **Mouse Buttons**: `xdo_mouse_down` / `xdo_mouse_up` (Left=1, Middle=2, Right=3)
- **Mouse Wheel**: Emulated via X11 button pulses (Scroll Up=4, Scroll Down=5, Scroll Right=7, Scroll Left=6)
- **Keyboard Events**: Pure virtual key mapping table `vk_to_x_keysym_name` converting Windows Virtual-Key (VK) codes into standard X11 keysym names (`Return`, `BackSpace`, `Tab`, `Escape`, `Shift_L`, `Control_L`, `Alt_L`, `Super_L`, etc.) sent via `xdo_send_keysequence_window_down` / `xdo_send_keysequence_window_up`.
- **Pre-Release Cleanups**: Platform-agnostic `release_all_keys` automatically drains all held keys and buttons to prevent stuck keys across boundary crossings.

## Input Capture & Session Limitations

Input capture on Linux requires listening to global input events across display boundaries:

1. **Active X11 Session (Capture Engine Running)**:
   - In an interactive X11 desktop session (`DISPLAY` set) where the capture loop is active:
   - **Pointer & Key Tracking**: Real-time cursor monitoring via `XQueryPointer` and global key scanning via `XQueryKeymap` running in a dedicated ~120 Hz polling thread.
   - **Self-Echo Suppression**: Injected events are timestamped on generation (`LAST_INJECTION_TIMESTAMP_MS`) and filtered through `should_forward_captured_event` (25 ms suppression window) to prevent feedback loops when remote inputs are injected locally.
   - **Edge Crossing & Window Handoff**: Monitored against virtual desktop boundaries (`check_edge_crossed` / `check_edge_returned`). Pushing against the configured edge emits `window-handoff-trigger` if Ctrl+drag is active, transitions input mode to `driving`, transmits `kvm.take`, and parks the cursor via `XWarpPointer`.
   - **Driving Mode**: Streaming motion (`k.mm`), mouse buttons (`k.md`/`k.mu`), wheel events (`k.wh`), and keystrokes mapped to virtual keycodes via `x_keycode_to_vk` (`k.kd`/`k.ku`). Returning past the edge boundary restores the pointer locally, triggers `release_all_keys()`, and transmits `kvm.home`.
   - Status: `helper = "x11"`, `message = "Linux input capture: active X11 session (capture engine running; XQueryPointer/libxdo ready)"`.

2. **Active X11 Session (Capture Engine Not Running)**:
   - When an X11 display is detected but the capture engine has not started or failed to initialize:
   - Status: `helper = "libxdo"`, `message = "Linux input capture: active X11 session detected but capture engine not running; local capture unavailable (injection ready)"`.

3. **Wayland Sessions**:
   - Due to the Wayland security model, unprivileged background applications are strictly prohibited by the compositor from capturing global raw input events outside their own surfaces.
   - Without root privileges or dedicated compositor protocol extensions (e.g., `ext-foreign-toplevel-list` / custom input capture protocols), global edge crossing capture is restricted.
   - Status: `helper = "wayland"`, `message = "Linux input capture: Wayland session detected; global raw input capture unavailable (requires X11 session or root uinput privileges)"`.

4. **Headless / CI Environments**:
   - In automated testing or headless server environments where `DISPLAY` is not defined:
   - Status: `helper = "libxdo"`, `message = "Linux input capture: headless session (DISPLAY not set); remote input injection and local capture unavailable"`.
   - GlideDesk degrades gracefully with honest capability reporting on `KvmRuntimeState` rather than panicking or failing silently.
