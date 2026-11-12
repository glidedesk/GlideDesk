# Changelog

All notable changes to GlideDesk are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Notifications from one computer appear on the other**: the Notifications tab used to keep state, per-app mutes and history with nothing behind them — no notification ever left or arrived. Each computer now reads its own notification list once a second and sends what is new over the paired, encrypted connection; the other computer shows it as a native notification from GlideDesk, and both directions appear in the tab's history with a count and the apps seen. Each side says whether it wants the other's (`notif.policy`), so turning the tab off or muting an app stops the traffic instead of only hiding rows. Where the lists are read from: `%LOCALAPPDATA%\Microsoft\Windows\Notifications\wpndatabase.db` through `winsqlite3.dll` on Windows (the `UserNotificationListener` API needs package identity), and `~/Library/Group Containers/group.com.apple.usernoted/db2/db` on macOS (needs Full Disk Access; without it the tab says the list cannot be read and receiving still works). A toast's XML and a macOS record's binary plist are parsed for title/subtitle/body, an app identifier becomes a readable name, repeats of the same app and text within 15 seconds are dropped, and a notification received from the other computer is never read back and sent again. Only what arrives from now on is forwarded — the existing list is history. Wire messages `notif` and `notif.policy` carry new optional fields (`title`, `subtitle`, `body`, `at`, `accept`), so an older peer ignores them. `Clear` now really clears the history. Tests: notification guard, own-app filter and display names.
- **Copy files on one computer, paste them on the other with Ctrl+Alt+V**: press the shortcut where the pointer is and the files copied on the other computer arrive in this one's received folder, in either direction. The driven computer recognises the gesture from the key stream it already receives, asks the computer the files were copied on for them, and they travel over the same batch transfer as the Files page; the V is used up so nothing types a stray letter, and the shortcut is also read when Ctrl arrives as Command because this computer drives a Mac. Only files are sent — a folder on the clipboard is skipped. Tests: `fileclip` (modifier tracking, shortcut on the press, single use of the release).
- **A fresh install can open the other computer's folder with no setup**: sharing used to start off and empty, so the other computer saw nothing until someone pressed `Choose…` and picked a folder. Sharing now starts on with a folder whose name says what it is — `GlideDesk Share` in the home directory, created on first use — so a new install announces itself within seconds and `Open` works immediately. Nothing personal goes out by surprise: it is an empty folder of its own, never Desktop or Documents. Turning sharing off withdraws the folder and is remembered, and the Files page names the default folder as soon as it is made.
- **Share a folder with the other computer, and open its folder here**: the Files page's two folder rows now do what they say. `Choose…` picks a folder and this computer serves it read-only — a list, a page any browser can follow, and the files themselves — on a random port behind a random token, both handed over on the paired, encrypted connection; turning the switch off (or losing the folder) withdraws the token and the server answers nothing. The other computer sees `X is sharing N items` at once, `Open` lists the folder one level at a time with folders first, `Save` puts a file in the same received folder as any other arrival and records it in Recent, and `In browser` opens the same listing in the system browser. Paths that try to leave the shared folder (`..`, absolute paths, backslashes, symlinks pointing out) are refused, hidden files stay hidden, a second copy of a name becomes `name (2).ext` instead of overwriting, and the wire carries the new `files.folder` message with `sharedFolder` optional so older peers simply see nothing shared. Tests: `sharefolder::tests` (path escape, listing order, name encoding, no overwrite).
- **Open Terminal opens a shell on the other computer**: the Terminal tab used to open an empty local terminal, and its SSH server box was advice text. GlideDesk now makes one SSH key for itself (kept in its own folder, never shown), offers the public half over the paired connection, and the other computer adds that line to the keys it lets in and answers with its login name and whether its SSH server is running. Open Terminal then opens the terminal app here running `ssh` there, as the other computer's user — no password. The SSH server row shows the real service state, Set up starts it (asking for administrator rights once; on Windows the same step puts the key where an administrator's sshd reads it), and both directions work from the one page. The tab also keeps `ssh glidedesk` true by maintaining a marked block in your SSH config pointing at the paired computer.
- **F11 fills the screen window and Escape leaves it**: the screen window now has a full screen toggle (`set_projection_fullscreen`), so the picture can fill a 1920×1080 display and come back to a window.
- **A feature's page shows its settings straight away**: the picture that introduces a feature (Cursor, Keyboard, Clipboard, Receive screen, Share screen, Files, Terminal, Apps, Audio, Camera, Notifications) used to stand in place of the settings, leaving only Dismiss — so the page looked like an advert with nothing in it. It is now a one line bar above the settings (the feature's name, Show me, Hide); Show me opens the picture, Show less folds it back, and Hide puts it away as before. The settings stay usable in every state.
- **This computer's sound on the other one, and the other one's here**: the Audio tab's two directions had state and a volume slider but no sound behind them. Sending now captures what this computer plays — WASAPI loopback on the default output on Windows, ScreenCaptureKit's system audio on macOS 13+ with this app's own playback excluded so the other computer's sound cannot travel back — and both send 48 kHz stereo float32 in ~50 ms packets. The receiving side plays through its default output with cpal, resampling only when that device does not run at 48 kHz, with the volume slider and the "don't play the other computer's sound here while sending" switch applied by the player. The wire fields are optional (`audioB64`, `audioRate`, `audioChs`), so a peer that only knows `audio_bytes` still works. Each computer sends `audio.policy` at hello and on every change, so a direction switched off on one side stops the traffic instead of arriving and being discarded, and the tab's sending/receiving lines follow the real devices; a device that fails says why on the page. On macOS the capture needs Screen Recording once: the app asks with `CGPreflightScreenCaptureAccess`/`CGRequestScreenCaptureAccess` before using ScreenCaptureKit, and the bundle declares `NSScreenCaptureUsageDescription` (with microphone, audio capture and camera strings) from `src-tauri/Info.plist`. `build.rs` gives the ScreenCaptureKit binding's Swift runtime an rpath, without which the binary did not start. Verified on the real pair both ways with real sounds: Windows reported `Played 167 more sound packets, peak 0.44` while the Mac played a chime, and the Mac reported `Played 4 more sound packets, peak 0.77` while Windows played a notification.

### Fixed

- **The pointer blinked and jumped at the seam, and the bottom of a bigger screen could not be reached**: crossing from a smaller desktop to a larger one, the sending pointer ran out of its own screen a third of the way down the larger one and then sat on the edge, while the receiving pointer was dragged back and forth. Three things were wrong. Movement was forwarded 1:1, so a 1366×768 PC beside a 1920×1080 Mac could only reach 768 of the Mac's 1080 points; each side now sends its whole desktop size in `hello`, `welcome` and `kvm.layout` (optional fields, so an older peer keeps the old behaviour), the sender scales each axis by the ratio of the two sizes, and fractional results are carried into the next packet instead of being rounded away. The sending pointer is now put back on the point where it crossed after every event, so an event carries only the distance the hand moved and the pointer never runs out of screen — this replaces the old "park near any edge" logic that caused the jumping. And macOS reports the cursor being put somewhere as an ordinary mouse movement from the system (no marker, pid 0), which looked exactly like the person touching the mouse and, at a seam, like a new crossing: the app now remembers where it last put the pointer and ignores events there, and an event that gives control back does not also count as a crossing. Also: the receiving pointer is clamped to the visible desktop on macOS (it could be pushed to y=2155 on a 1080 point screen and vanish), the return threshold is 24 px of deliberate travel instead of 8 (acceleration alone used to hand control back the instant it was taken), the macOS pointer is hidden while it drives the other computer so only one arrow is visible, and the hook re-reads its state on a crossover instead of up to 100 ms later. Verified on the real pair with the shipped builds: Windows→Mac and Mac→Windows each cross once, reach 1078 / 767 (both bottoms), return once at 30–55 px, and inject nothing that takes control back.
- **Both computers could end up each saying the other was using it, and then neither took input**: if the pointer crossed in each direction before either side settled, each computer accepted the other's takeover and both sat in the "driven" state, showing `X is using …` and ignoring keyboard and pointer from the link with no way back except Disconnect. A computer that is already driving refuses the other side's takeover and tells it to go home, so the pair cannot deadlock; and touching the mouse on a computer the other one is using takes control straight back, parks the other pointer and sends the other computer home, which is what the person at that desk expects. Verified on the real pair: with the Mac driving, a local mouse move on Windows took control back (Windows `Local mouse took control back`, the Mac went home) and the next crossing worked normally.
- **Typing stopped crossing the link while the pointer still worked**: Windows silently removes a low-level keyboard hook whose callback misses its deadline, and this one cloned the whole application state — including a network address enumeration — on every pointer event, thousands of times a second. When the hook was dropped, keystrokes stopped arriving on the other computer with nothing in the log to say why, and only a restart brought them back. The callbacks now read a small cached view refreshed ten times a second and no longer log per event, so they return immediately; a watchdog also asks the hook thread to install it again when a key this app injected never comes back through the hook, or after a long silence. Verified end to end in both directions, including that recovery needs no restart.
- **A file paste could be delivered twice**: when both computers had dialled each other there were two sessions, and a packet addressed to the peer rather than to one session went out on both, so the same file batch arrived twice. A destination that names the peer now uses only the session both ends agree on; a destination that names one session still uses it. Verified: one `files.batch` per paste in each direction.
- **The right Shift key on some keyboards and 2.4G receivers did nothing on the other computer**: those keyboards report the right Shift as the generic Shift code and mark only the scan code as the right one, so the other computer received the left Shift. The scan code is now read as well, and the right Shift, Ctrl and Alt keep their side across the link (#212).
- **Clicking Minimize, Maximize or Close on the remote window froze the session**: Windows runs a modal loop on the window's thread when a caption button is pressed, waiting for the release — and that is the same thread that injects input and brings the release in, so the press queued the release behind the loop that was waiting for it. The session stalled and dropped, then reconnected a few seconds later. A press on one of GlideDesk's own caption buttons is now held back in the injector and both halves are sent together when the release arrives, so the button works and the link stays up. The same guard covers Help.
- **The Files page forgot its setup on every launch**: the shared folder, whether sharing was on, the received folder and the direction switches lived only in memory, so every restart came back with sharing off and the folder empty. They are now written to `files.json` beside the pairing and restored at launch, saved on every change and on each folder picker, and older files without the newer fields load with the safe defaults.
- **Open on the Files page showed nothing on a small window**: the folder browser was scrolled into view while it was still hidden, and a hidden element cannot be scrolled to — so on a laptop-sized window the list stayed out of sight and the click looked ignored. The card is now shown before the scroll, and turning sharing off says `Turned off · <folder>` instead of leaving the hint blank.
- **The Files page's folder rows were squeezed into the direction cards**: the label wrapped onto three lines and the switch sat on top of the hint, so `Open` was not reliably clickable. Both folder rows now live in a full-width Folders card, with the peer's sharing state as the row's hint, and the folder browser opens underneath.
- **Console windows blinked on screen every few seconds on Windows**: the Terminal tab's background loop started `whoami`, `sc` and `icacls` to read the SSH server state, and on Windows each console program started by a GUI app gets its own console window — a black window appearing and vanishing every four seconds. Those helpers now start with `CREATE_NO_WINDOW`, the administrator check and the service state are remembered instead of re-asked, and the key's permissions are set once per run. The handshake is also only repeated when the two computers have not agreed yet, plus a slow refresh.
- **The other computer's picture never arrived**: the task that drains the screen and audio channels was spawned with `tokio::spawn` from a synchronous command, where no runtime exists, so it never ran and frames filled a 30 slot channel unread. It is now started once at startup with `tauri::async_runtime::spawn` (idempotent), which is what makes sharing work in both directions.
- **The screen window stayed blank even when frames arrived**: the `projection` window was missing from `src-tauri/capabilities/default.json`, so `event.listen` and `invoke` were denied inside it. Both windows are listed.
- **Resolution and Quality on the Share page changed nothing**: the capture engine always sent 1600×900 at one quality. `CaptureSettings::from_choices` now turns the page's choices (1080p, 1440p, Full size; 10–50 Mbps) into a frame size and a JPEG quality that the capture loop reads for every frame. Test: `picture_choices_set_frame_size_and_quality`.
- **Files → Choose… picked files and then did nothing**: the dialog's answer is now awaited and passed to `send_files`, and a send that cannot start says why on the Files page instead of only in the console.
- **Files → Recent always said "Nothing sent" / "Nothing received"**: nothing ever wrote those entries. A finished batch now records what was sent (and where it went) and what arrived (with its saved path), with the timestamp in the unit the page reads.
- **A computer that had dialled the other one could not send files**: when both computers dialled each other, the pair of sessions made the destination lookup answer "multiple peers connected" even though one computer was connected. The lookup now counts computers, not sessions. Regression assertions in `test_resolve_file_dest_exact_rules_and_matching`.
- **Received files went somewhere the General page did not show**: `Open` opened `~/Downloads/GlideDesk` while files were written to `~/Downloads`, and `Change…` and the shared folder's `Choose…` were stubs that returned hardcoded paths (`~/Downloads/GlideDesk`, `~/Public`) without opening a dialog. Both now open the system folder picker, cancelling changes nothing, a chosen folder is where files are actually saved (`effective_received_dir`), `Downloads` resets the choice, `Open` opens the folder in use, and the shown path is refreshed on every state push.
- **A paired computer lost the pairing when the other one went away**: the disconnect paths cleared the paired record (and a live client overwrote its token with `None`), so unplugging or restarting meant a new code. A pairing now survives a disconnect, sleep and restart: it is written to a small `pairs.json` (the token is kept there, never in the state the window sees), loaded at startup, and only Forget clears it. Test: `pairing_survives_disconnect_and_client_updates`.
- **No reconnection after a restart**: GlideDesk never dialled a known pair on its own, so a restart left the window on "Connecting…" until the user paired again. Startup now connects to the paired computer (stored address, else discovery), keeps retrying while it is off or asleep, and stops only on Disconnect; both sides prove their identity in `welcome` so the pairing authenticates in both directions. The TLS identity is now made once and kept (`identity.json`), so the stored certificate pin still matches on the next launch; a changed identity is reported instead of failing silently.
- **Clipboard tab disappeared when one direction was switched off**: `send` was treated as the master switch, so turning off "Copy on this PC, paste on the other computer" hid the whole tab. `enabled`/`share` are the master; `send` and `receive` are directions. Regression assertion in `test_kvm_and_clipboard_master_switch_handling`.
- **Files "Share a folder" did nothing**: the bridge called `set_sharing` (screen sharing) instead of updating the files state; the folder picked with Choose… is now stored and shown, and the switch reaches the backend.
- **Camera tab controls did nothing**: Virtual camera, Share this camera, the camera picker, Stop and the window/view actions had no backend behind them. The `camera` command now handles `virtual`, `share`, `device`, `devices`, `stop`, `view` and `close`, and the state carries the `vcam` and camera fields the tab reads.
- **Remote tab switches did nothing**: "Allow a phone" and "Real gamepad" now reach `set_remote` (`allow`, `pad`).
- **A muted app could not be unmuted**: the notifications switch now sends the wanted state instead of toggling blindly; phone media actions pass the whole request (including seek position) through.
- **The sidebar said "Not paired" while the other computer was merely offline**: it now names the paired computer and says Offline.
- **Pairing stuck on "Pairing…"**: a tab's `badge()` that assumed a runtime block the backend does not send (`terminal.server`) threw on every state push, aborting the render before the pairing banner was drawn. The Allow / Codes match buttons never appeared, so a pending request could never be answered. The Terminal badge now tolerates a missing runtime block, the banner is rendered first and independently of the rest of the page, and one tab's `hidden()`/`badge()` can no longer stop the others. Regression test: `ui/shared/test/tab-badges.js`.
- **Pairing codes never matched**: the guest hashed the host's certificate fingerprint (received in `pair.nonce`) but the host hashed the guest's. Each side therefore showed different six digits, so "Codes match" could never be confirmed honestly. Both sides now hash the host's fingerprint — the identity the guest pins and the one the session token binds to — matching the Android guest, which already used it. Test: `test_pair_code_is_the_same_on_both_computers`.
- **Pairing diagnostics**: `[Pair]` log lines now report the dial target, the stored TLS fingerprint, dial success/failure, and a failed `pair.start` send, so a stalled handshake can be traced from the log alone.
- **Mac crash while forwarding a typed key**: macOS 27 traps a process that asks its text-input services off the main thread, and the keyboard hook asked for the input method from its own thread — forwarding any letter (with or without an input method) killed the app. The hook now reads a cached answer that the main thread refreshes, and keys arriving from the other computer are injected on the main thread so their input-method switch happens first and legally. Test: `cached_input_method_is_readable_off_the_main_thread`.
- **Keys arrived over the network but were never typed on Windows**: `SendInput` returns `ERROR_ACCESS_DENIED` on threads that are not attached to the interactive desktop, and every remote key, click, wheel tick and modifier release ran on an async worker thread (or the low-level hook thread), so forwarding silently did nothing. All Windows injection now runs through a single gate that posts the work to the main thread (the one that owns the window station) and reports a failed `SendInput` instead of dropping it. `sync_input_desktop` no longer re-assigns the thread desktop on every event, which could never work for a thread that owns windows and closed a handle the thread was still using.
- **Remote keys echoed back**: when the receiving computer was also driving, Windows forwarded the keystrokes it had just injected, so typing bounced between the two machines. Injected keys now carry the same self-injection marker as the mouse path and are passed through the hook untouched.
- **Silent hook failures**: a failed low-level mouse or keyboard hook installation was swallowed; both failure paths are now logged.
- **Bluetooth with phones was on by default and could not be switched off**: the toggle had no backend behind it — `setNet` was never defined, so pressing it did nothing and the next state push snapped it back on. There is now a `set_net` command (`ble`, `blePc`, `bleWifiOff`), the switch defaults to off, and the UI reads the stored value as on only when it truly is. Older saved snapshots without `bleWifiOff` load with the safe default. Test: `test_net_defaults_keep_bluetooth_off`.
- **Wheel scrolled the wrong way (Windows → Mac)**: the Windows hook negated the notch count before sending it, so scrolling up on Windows scrolled the peer down. Both sides treat a positive notch as "scroll up"; the sign is forwarded unchanged. macOS also forwards pixel deltas from trackpads and smooth-scrolling mice, which report no line delta at all. Test: `wheel_notches_keep_the_scroll_direction`.
- **Cursor stuck on the edge after returning home on Windows**: the hook moved the pointer with a raw `SetCursorPos` on the hook thread, which silently fails there, so the pointer stayed parked on the edge and no further movement was reported. Returning home, re-parking and double-tap recentring now use the gated injector, and `kvm.home`/`kvm.release` nudge the pointer back into the screen.
- **Cursor landed in the wrong place when entering the peer**: `kvm.take` now carries the point along the crossed edge (`edgeAt`) so the receiving side parks its pointer on the same spot, keeping movement bookkeeping and the reverse-return threshold consistent.
- **Remote input echoed back into the sender on macOS**: the tap forwarded the events GlideDesk had just injected. Events injected by the app are now recognised by their recent injection timestamp and filtered out.
- **Accessibility prompt returned on every launch**: it is now requested only the first time input is forwarded, not at startup.
- **The Screen Recording question came back on every rebuild and the answer never took on macOS**: each build was signed ad hoc, so the code requirement stored with the permission changed on the next build and the grant no longer matched, while the bundle had no `NSScreenCaptureUsageDescription` for the request to explain itself. The app is now signed with a stable identity, the usage strings travel in `src-tauri/Info.plist`, and capture checks the grant before starting, so one approval lasts across updates. Verified on the Mac: `[Audio] Screen Recording permission: true` and `[Audio] Sending this computer's sound (ScreenCaptureKit, 48 kHz stereo)`.
- **Playing the other computer's sound could quit GlideDesk**: the output callback assumed the buffer held as many frames as the audio device asked for, but the first callback after sound starts usually runs with a fraction of a packet. The callback then drained a range past the end of its sample slice, and a panic inside the CoreAudio callback aborts the process (the same code runs the Windows output). It now takes only the frames that are there and restarts the interpolation after a starved callback; a test covers the arithmetic (`playback_callback_never_consumes_more_than_the_buffer_holds`). Reproduced with a burst of notification sounds after a silent pause, which used to leave two crash reports; the fixed build played five bursts and logged peaks of 0.92–0.94 with the process still up. The panic hook also writes a backtrace to `crash.log` now, so a future abort says where it happened.

### Changed

- **The input hooks no longer touch the application state on every event**: the Windows mouse and keyboard hooks read a small structure — driving state, sharing switches, layout side and the peer's shortcut modifier — that a background thread refreshes every 100 ms, instead of cloning the whole state and enumerating local addresses for each pointer movement and keystroke. Per-event debug logging inside the callbacks is gone as well; the hook now keeps counters and the last key code, which are only reported when the watchdog acts. This is what keeps the hook within its deadline under a fast mouse.
- **The Files page no longer offers a switch for dragging files across the screen edge**: it stored `files.edgeDrag` but no drag was ever implemented behind it, so it was a control that did nothing. The row is gone rather than left as decoration; the stored value is still read so an older `files.json` loads, and the code can be used when the native drag handoff is built. Ctrl+Alt+V still moves files copied on one computer to the other, and dropping files onto the Files page sends them.
- **The Files page's shortcut switch and key recorders did nothing**: they stored `files.shortcuts`, `files.copyKeys` and `files.pasteKeys`, but the hook reads the fixed Ctrl+Alt+V gesture and never looked at them, so turning the switch off changed nothing and a recorded key was ignored. The card now states the gesture ("Select files and copy them here, then press Ctrl+Alt+V on …") instead of offering controls that had no effect; the stored fields stay so older `files.json` files load.

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
