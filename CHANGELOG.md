# Changelog

All notable changes to GlideDesk are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Pairing stuck on "Pairing…"**: a tab's `badge()` that assumed a runtime block the backend does not send (`terminal.server`) threw on every state push, aborting the render before the pairing banner was drawn. The Allow / Codes match buttons never appeared, so a pending request could never be answered. The Terminal badge now tolerates a missing runtime block, the banner is rendered first and independently of the rest of the page, and one tab's `hidden()`/`badge()` can no longer stop the others. Regression test: `ui/shared/test/tab-badges.js`.
- **Pairing codes never matched**: the guest hashed the host's certificate fingerprint (received in `pair.nonce`) but the host hashed the guest's. Each side therefore showed different six digits, so "Codes match" could never be confirmed honestly. Both sides now hash the host's fingerprint — the identity the guest pins and the one the session token binds to — matching the Android guest, which already used it. Test: `test_pair_code_is_the_same_on_both_computers`.
- **Pairing diagnostics**: `[Pair]` log lines now report the dial target, the stored TLS fingerprint, dial success/failure, and a failed `pair.start` send, so a stalled handshake can be traced from the log alone.
- **Mac crash while forwarding a typed key**: macOS 27 traps a process that asks its text-input services off the main thread, and the keyboard hook asked for the input method from its own thread — forwarding any letter (with or without an input method) killed the app. The hook now reads a cached answer that the main thread refreshes, and keys arriving from the other computer are injected on the main thread so their input-method switch happens first and legally. Test: `cached_input_method_is_readable_off_the_main_thread`.
- **Keys arrived over the network but were never typed on Windows**: `SendInput` returns `ERROR_ACCESS_DENIED` on threads that are not attached to the interactive desktop, and every remote key, click, wheel tick and modifier release ran on an async worker thread (or the low-level hook thread), so forwarding silently did nothing. All Windows injection now runs through a single gate that posts the work to the main thread (the one that owns the window station) and reports a failed `SendInput` instead of dropping it. `sync_input_desktop` no longer re-assigns the thread desktop on every event, which could never work for a thread that owns windows and closed a handle the thread was still using.
- **Remote keys echoed back**: when the receiving computer was also driving, Windows forwarded the keystrokes it had just injected, so typing bounced between the two machines. Injected keys now carry the same self-injection marker as the mouse path and are passed through the hook untouched.
- **Silent hook failures**: a failed low-level mouse or keyboard hook installation was swallowed; both failure paths are now logged.

## [3.2.0] - 2026-11-06

GlideDesk 3.2.0 delivers full core feature parity across Rust/Tauri v2, implementing all six core KVM/mesh migration modules, fast file transfer (`fx2`), two-way settings synchronization (`pairsync`), and modern Nix build support.

### Added

- **Core Ported Modules (Rust / Tauri v2):**
  - **`applog`**: Per-OS log directory (`glidedesk.log` with 5MB × 3 rotation, `crash.log`, panic hook, and env_logger TeeWriter integration). Periodic link statistics (`[link]`) sampling every 30s reporting RTT p50/p95, moves/s, and queued bytes.
  - **`pairlimit`**: Pairing time limits (8s connecting, 15s starting, 60s waiting) across wired/BLE connections, protocol version mismatch guidance, and expiration state machine.
  - **`kvm-moves`**: Pointer-motion coalescing with 64 KiB outbound backlog limits, 20ms maximum wait, and 2ms retry flushing.
  - **`keylayout`**: Shortcut resolution across keyboard layouts using US mapping tables, `ch` character tagging on macOS, and modifier tracking.
  - **`inputmethod`**: Cross-computer input method (IME) language synchronization (Fcitx 5 / IBus on Linux, TIS on macOS, HKL/IME on Windows) so typing keys compose seamlessly with the receiver's IME.
  - **`kvm-geom`**: Multi-monitor crossing geometry calculation, touching edge stretches, placement computation, fractional exit/entry points, mapped offsets (`map_offset`), and boundary snapping.
- **Settings & Transfer Enhancements:**
  - **`pairsync`**: Two-way settings synchronization state machine keeping shared features (clipboard, notifications, audio, terminal, file drops, kvm) in sync across connected peers.
  - **`fastfile` (`fx2`)**: High-performance direct HTTP streaming over the session port (`/glidedesk/fx/`) with parallelism, chunking, and streaming SHA-256 integrity verification.
  - **Nix Build Integration**: Updated `flake.nix` for building the Tauri v2 + Rust app using `cargo-tauri`, WebKitGTK 4.1, GTK3, and soup3.
- **Packaging & Platforms:**
  - **Android companion (Kotlin/Compose)**: First-class Android 10+ companion under `android/` (`com.glidedesk.android`), pairing through the desktop's `glidedesk://pair` QR scheme and the same `/glidedesk/fx` wire paths. Ships as `GlideDesk-3.2.0.apk` (standard) and `GlideDesk-3.2.0-full.apk` (notifications, Messages, and Accessibility for mouse/keyboard without ADB).
  - **Native Windows ARM64**: `windows-11-arm` CI leg producing `GlideDesk_3.2.0_arm64-setup.exe` and `GlideDesk-portable-arm64.exe` for Snapdragon X Elite and other Windows on Arm machines.
  - **Linux RPM**: `deb`, `rpm`, and `appimage` bundles from the Linux leg, covering Fedora, RHEL, openSUSE, and Rocky in addition to Debian/Ubuntu.

### Fixed

- **Single Instance & Startup Control**:
  - Integrated `tauri-plugin-single-instance` to prevent dual app launches and duplicate macOS menu bar icons.
  - Prevented `launchctl load` from immediately launching a duplicate instance on macOS when enabling "Run at startup".
  - Added startup boolean to FullState and AppState with cross-platform detection to persist the toggle state in UI.
  - Configured initial main window visibility to prevent startup splash flash.
- **Nearby Peer Discovery**:
  - Enhanced UDP discovery with socket address and port reuse (`SO_REUSEADDR`/`SO_REUSEPORT`).
  - Active broadcast probes on refresh and startup, plus beacon logging.

## [3.1.0] - 2026-11-05

GlideDesk 3.1.0 introduces a built-in in-app updater across macOS, Windows, and Linux with progress tracking, package integrity verification, automated installation, and graceful fallback.

### Added

- **In-App Updater with Streamed Progress and SHA-256 Verification:**
  - Direct streamed downloads to per-user temporary directories without buffering whole DMG/installer binaries in memory.
  - Real-time download progress updates (%, MB downloaded / total) throttled to 100ms.
  - Package integrity verification comparing actual payload byte sizes and SHA-256 checksums extracted from published release notes.
  - Dedicated "Release page" fallback button in the About tab for immediate manual download if network or install errors occur.
- **Per-OS Automated In-Place Installation:**
  - **macOS**: Pre-flight `/Applications` write verification, DMG mount, `/Applications/GlideDesk.app` replacement, quarantine flag clearing (`xattr -cr`), ad-hoc re-signing, running-instance termination before relaunch.
  - **Windows**: Executes NSIS `-setup.exe` with silent flags (`/SILENT /FORCECLOSEAPPLICATIONS /RESTARTAPPLICATIONS`), passes exit code back, detects non-zero exit codes (such as dismissed UAC), and rejects portable `.exe` packages.
  - **Linux**: Downloads AppImage directly to `$XDG_DATA_HOME` or `~/.local/bin` (not `/tmp`), marks executable (`chmod +x`), spawns in background, and terminates previous instance handling both `glidedesk` and `GlideDesk.AppImage` process naming.

### Fixed

- **Updater Asset Selection & State Reporting:**
  - Resolved issue where Windows auto-update could select zero-install portable executables (`GlideDesk-portable-x64.exe`) which ignore silent installer flags.
  - Fixed UI hanging indefinitely on "Checking...": all update checks and install operations cleanly set `checking = false` and report actionable status messages.
  - Stripped external shell scraping from install scripts: installers directly consume the Rust-resolved asset URL.

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
- **Pure-Rust Linux X11 screen capture.** Screen capture on Linux is implemented via `x11rb`
  using `xproto::get_image` (`ZPixmap`) with automatic color-channel normalization and
  auto-stop / graceful health degradation on lost X connection (5 consecutive errors).
  Wayland limitations are documented in `docs/linux.md`.
- **Native off-screen window parking and hit testing.** Real window parking at
  `(-32000, -32000)`, window restore, and hit testing (`find_window_at_point`) are implemented
  for macOS (`CGWindowListCopyWindowInfo` + `AXUIElement`) and Linux (`x11rb` `query_tree` +
  `get_geometry` + `WM_NAME`), maintaining the 12-second crash-safe recovery watchdog.
- **Multi-peer transfer isolation and resume verification.** Loopback TCP socket integration
  tests verify that concurrent peer transfers remain strictly isolated to designated peer IDs
  with zero cross-talk, valid SHA256 verification, and chunk offset resumption.
- **Live localhost E2E testing suite.** Added automated end-to-end integration test
  (`e2e-kvm-transfer-lifecycle.test.js`) validating the full wire protocol lifecycle
  (`pair.start` through SAS authentication, `kvm.take`, pointer streaming, file batch/chunk transfer,
  and cleanup) over live loopback sockets.
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

- **Android companion de-scoped.** The Android companion app is explicitly de-scoped in v3.0.0
  to focus on hardening and stabilizing the core desktop Tauri v2 + Rust KVM engine.
- **No native X helper dependency on Linux.** `libxdo` and `libX11` are no longer loaded
  at runtime; `x11rb` is the only Linux-specific runtime dependency.
- **Injection failure telemetry.** Injection errors are reported once per failure kind
  (rate-limited) and reflected in the runtime state instead of being dropped.
- **Self-maintaining version.** The About panel now reads its version from the crate
  metadata (`CARGO_PKG_VERSION`) instead of a hard-coded string.
- **Dead-code cleanliness.** Linux-only helpers are correctly cfg-gated, so `cargo check`
  is warning-clean on macOS, Windows, and Linux.

### Fixed

- **Destination contract in unicast file transfer.** Sender and receiver contract aligned on
  stable client ID routing; receiver guard accurately accepts legitimate peer transfer packets.
- **Linux capability honesty.** Local capture is only reported as running when the
  capture engine is actually alive.
- **Windows screen capture.** Correct device-context handling for
  `CreateCompatibleDC`/`BitBlt`.
- **Release installer cleanliness.** Build pipeline cleans stale bundle directories before packaging
  to prevent historical installer versions from leaking into current releases.

### Security

- **Authenticated transport.** Client TLS negotiation with cryptographic signature
  verification and trust-on-first-use certificate pinning.
- **Transfer integrity.** File packets are unicast to a stable peer id, with chunked,
  resumable, checksum-verified transfer and auth required for explicit targets.

[3.1.0]: https://github.com/glidedesk/GlideDesk/releases/tag/v3.1.0
[3.0.0]: https://github.com/glidedesk/GlideDesk/releases/tag/v3.0.0
