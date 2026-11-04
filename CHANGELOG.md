# Changelog

All notable changes to GlideDesk are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.0] - 2026-11-04

GlideDesk 3.0 is the pure **Tauri v2 + Rust** release. The desktop app runs with no
native input-helper dependency in its KVM engine, gains first-class Linux input support,
and behaves as a proper background/tray application on macOS, Windows, and Linux.

### Added

- **Native Linux input support (pure Rust).** Keyboard, mouse buttons, wheel, and
  absolute/relative pointer motion are injected through the XTest extension, and local
  pointer/keyboard input is captured directly from the X server, via
  [`x11rb`](https://crates.io/crates/x11rb). The KVM engine no longer dlopens `libxdo`
  or `libX11`.
- **Linux edge-crossing and window hand-off parity.** The Linux engine runs the same
  edge-crossing detection, `kvm.take`/`kvm.home` protocol, cursor parking, and
  Ctrl+Drag window hand-off as Windows and macOS.
- **Self-echo suppression.** Injected events are ignored within a suppression window so
  local capture never re-forwards GlideDesk's own input.
- **macOS menu-bar agent mode.** Hiding the window (close-to-tray or `--daemon`) switches
  to `ActivationPolicy::Accessory` (no Dock icon or Cmd+Tab entry) and back to `Regular`
  when shown. This also removes the macOS "App Running in Background" notice.
- **Tray-only background mode on Windows and Linux.** Hiding the window applies the
  taskbar-exclusion hint (`set_skip_taskbar(true)`) and clears it when restored; on X11
  this is `_NET_WM_STATE_SKIP_TASKBAR` (some Wayland compositors may ignore it).
- **Linux capture health monitoring.** The capture loop stops and downgrades its reported
  status if the X connection is lost, instead of spinning silently.

### Changed

- **No native X helper dependency on Linux.** `libxdo` and `libX11` are no longer loaded
  at runtime; `x11rb` is the only Linux-specific runtime dependency.
- **Injection failure telemetry.** Injection errors are reported once per failure kind
  (rate-limited) and reflected in the runtime state instead of being dropped.
- **Self-maintaining version.** The About panel now reads its version from the crate
  metadata (`CARGO_PKG_VERSION`) instead of a hard-coded string.
- **Dead-code cleanliness.** Linux-only helpers are correctly cfg-gated, so `cargo check`
  is warning-clean on macOS, Windows, and Linux.

### Fixed

- **Linux capability honesty.** Local capture is only reported as running when the
  capture engine is actually alive.
- **Windows screen capture.** Correct device-context handling for
  `CreateCompatibleDC`/`BitBlt`.

### Security

- **Authenticated transport.** Client TLS negotiation with cryptographic signature
  verification and trust-on-first-use certificate pinning.
- **Transfer integrity.** File packets are unicast to a stable peer id, with chunked,
  resumable, checksum-verified transfer and auth required for explicit targets.

[3.0.0]: https://github.com/glidedesk/GlideDesk/releases/tag/v3.0.0
