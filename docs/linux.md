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

1. **Active X11 Session**:
   - In an interactive X11 desktop session (`DISPLAY` set), input events and cursor edges are monitored via XInput2 / XRecord hooks.
   - Status: `helper = "x11"`, `message = "Linux input capture: active X11 session (XInput2/libxdo ready)"`.

2. **Wayland Sessions**:
   - Due to the Wayland security model, unprivileged background applications are strictly prohibited by the compositor from capturing global raw input events outside their own surfaces.
   - Without root privileges or dedicated compositor protocol extensions (e.g., `ext-foreign-toplevel-list` / custom input capture protocols), global edge crossing capture is restricted.
   - Status: `helper = "wayland"`, `message = "Linux input capture: Wayland session detected; global raw input capture requires X11 session or root uinput privileges"`.

3. **Headless / CI Environments**:
   - In automated testing or headless server environments where `DISPLAY` is not defined:
   - Status: `helper = "libxdo"`, `message = "Linux input capture: headless session (DISPLAY not set); remote input injection and local capture unavailable"`.
   - GlideDesk degrades gracefully with honest capability reporting on `KvmRuntimeState` rather than panicking or failing silently.
