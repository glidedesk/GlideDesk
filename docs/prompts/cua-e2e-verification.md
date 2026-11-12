# Prompt: end-to-end GlideDesk check on macOS and Windows through CUA

Copy everything below into an agent session that has shell access to this Mac and to the Windows
machine (`ssh pc`), and CUA (`cua`, `cua-driver`) on both. Every check is done on the real, running
app — no test doubles, no mocked bridges.

---

## Role

You are verifying GlideDesk on two real computers against each other: this Mac (macOS) and the
Windows PC (`ssh pc`). Drive the actual windows with the accessibility trees CUA exposes, confirm
what the app *does* (side effects, logs, state files, the other computer's UI), and report a table.
When a check fails, fix the cause in the repo, rebuild and redeploy both apps, and run that check
again before moving on.

## Ground rules

- One machine at a time: never press on one side and expect an answer on the other without waiting
  for the state you asked for. Poll the UI or the log, do not sleep blindly longer than needed.
- Every screenshot and tree dump you rely on goes into `~/glidedesk-e2e/<run-id>/` on this Mac
  (`<run-id>` = `YYYYMMDD-HHMM`), named `<phase>-<check>-<side>.<png|json|txt>`.
- Read the checkbox/switch state from the accessibility tree where it exists (macOS exposes it);
  on Windows take a fresh screenshot and compare the switch's pixels inside its tree `bounds`
  (green knob = on, grey = off). Never infer a state from a click you just made.
- A press invalidates the accessibility snapshot: fetch a new tree before the next press.
- Test data lives in `~/glidedesk-e2e/` (Mac) and `C:\Users\<user>\glidedesk-e2e\` (Windows).
  Never delete, move or overwrite the user's real files, folders or downloads.
- Capture the starting value of every switch you will touch in Phase 0 and put it back in Phase 14.
- Leave the pair intact at the end (paired, connected, no test toggles on).

## The two CUA setups

macOS (local, `cua-driver 0.38.0`):

```sh
PID=$(pgrep -f "/Applications/GlideDesk.app/Contents/MacOS/glidedesk")
cua-driver call list_windows        "{\"pid\":$PID}"
cua-driver call get_window_state    "{\"pid\":$PID,\"window_id\":$WID,\"include_accessibility_tree\":true,\"session\":\"e2e\"}"
cua-driver call click               "{\"pid\":$PID,\"window_id\":$WID,\"element_token\":\"<snapshot_id>:<node_id>\",\"session\":\"e2e\"}"
cua-driver call bring_to_front      "{\"pid\":$PID}"
```

Windows (over `ssh pc`; `cua 0.24.2` — the host target is the interactive desktop, so the SSH
session itself never touches the UI, only the agent does):

```sh
ssh pc "cua do window ls"
ssh pc "cua do window focus <target-id>"
ssh pc "cua do a11y tree --window <target-id>"          # nodes: id, role, name, value, bounds
ssh pc "cua do a11y act <node-id> press --snapshot <snapshot_id>"
ssh pc "cua do a11y act <node-id> set-value 'text' --snapshot <snapshot_id>"
ssh pc "cua do screenshot --save C:\\Users\\<user>\\shot.png"
ssh pc "cua do move <x> <y>" ; ssh pc "cua do scroll up|down [n]"
```

If `cua do` returns `unauthenticated`, the saved host target lost its token: re-select it with the
token in `C:\Users\<user>\.cua\host\env-token`
(`cua do switch host --token <token>`), then retry. The window list shows two `glidedesk.exe`
windows — use the one titled exactly `GlideDesk`, never `com.glidedesk.desktop-siw`.

Deploy/restart commands (only when a fix needs a rebuild):

```sh
# Mac
cd src-tauri && cargo build --release
osascript -e 'quit app "GlideDesk"' ; sleep 2
cp target/release/glidedesk /Applications/GlideDesk.app/Contents/MacOS/glidedesk
codesign --force --deep --sign - /Applications/GlideDesk.app
open -a GlideDesk
# Windows
cd src-tauri && CFLAGS="-DHAVE_INTRIN_H=1" cargo xwin build --release --target x86_64-pc-windows-msvc
ssh pc "taskkill /F /IM glidedesk.exe"
scp target/x86_64-pc-windows-msvc/release/glidedesk.exe pc:C:/Users/<user>/glidedesk.new.exe
ssh pc "move /Y C:\\Users\\<user>\\glidedesk.new.exe C:\\Users\\<user>\\AppData\\Local\\GlideDesk\\glidedesk.exe && schtasks /Run /TN GlideDesk"
```

## What the code offers to test

The window registers 131 commands (`src-tauri/src/main.rs` `invoke_handler`), and the UI calls 130
of them (`ui/shared/tauri-bridge.js`). Group them like this and walk the groups in this order —
cheap and reversible first, real data and pairing last:

| Group | Commands | Surfaced in | Observable result |
|---|---|---|---|
| Shell / state | `get_state`, `set_tab`, `set_overview`, `relay`… | whole window | state pushes, tab switches |
| Pairing | `pair`, `pair_respond`, `confirm_code`, `cancel_pair`, `forget`, `connect_address`, `connect_again`, `refresh_found` | Overview, pairing banner | code on both sides, `pairs.json`, link state |
| Cursor / KVM | `set_kvm`, `set_layout`, `hide_hud`, `show_hud`, `device_do` | Cursor | pointer crosses, peer pointer moves, HUD |
| Keyboard | `set_keys` | Keyboard | keys typed on the peer, modifier swap |
| Clipboard | `set_clipboard`, `copy_text`, `copy_history`, `clear_history` | Clipboard | text appears on the peer's clipboard, history |
| Screens | `set_sharing`, `set_sharing_opts`, `select_display`, `set_display_mode`, `set_virtual`, `set_stream_input`, `set_stream_max_height`, `set_bitrate`, `set_screen_sound`, `open_monitor`, `remote_screen`, `set_monitor`, `set_pc_screen`, `show_projection_window`, `hide_projection_window`, `get_displays` | Receive screen, Share screen | a screen window on the other computer, modes, sound |
| Files | `send_files`, `choose_send_files`, `set_sharing`(folders), `choose_share_folder`, `reset_received_folder`, `choose_received_folder`, `open_received_folder`, `cancel_drop`, `get_transfer_status`, `get_transfer_route`, `cancel_transfer_task`, `open_mac_drive` | Files | bytes land in the peer's folder, progress, cancel |
| Terminal | `open_terminal`, `setup_sshd`, `fix_firewall`, `check_firewall`, `allow_permission`, `allow_full_disk`, `open_settings` | Terminal | a shell on the other computer |
| Apps | `list_apps`, `launch_app`, `pin_app`, `set_mac_apps` | Apps | the app opens on the other computer |
| Audio | `start_audio_capture`, `stop_audio_capture`, `set_app` | Audio | sound plays on the peer, mute-while-sending |
| Notifications | `clear_notif_history`, `open_notified_app`, `mute_notif_app`, `notif_reply` | Notifications | the peer shows the notification, reply text |
| Camera | `camera`, `vcam_hello`, `vcam_frame`, `vcam_preview`, `install_vdd`, `remove_vdd` | Camera | virtual camera / shared camera |
| Phone | `phone_*`, `set_phone`, `set_phone_remote`, `adb_qr`, `qr_pair`, `connect_phone`, `disconnect_phone` | Phone, Calls, Messages, Photos, Phone files | needs a phone — report Blocked if none |
| Media | `get_now_playing`, `control_media` | General / Phone | the peer's player reacts |
| Rezume | `set_rezume`, `rezume_do` | Multi-device | resume/sleep of the peer |
| Wake / power | `wake_peer`, `set_wake` | peers list | the peer wakes |
| Network | `set_net`, `net_wifi`, `net_location`, `net_join_hotspot`, `net_cancel`, `open_bluetooth_settings` | Overview, Multi-device | Bluetooth/Wi-Fi state |
| Windows / windows | `handoff_window`, `handoff_window_at_point`, `return_window`, `get_parked_windows` | Overview, Cursor | a window moves to the peer and back |
| Settings / update | `set_startup`, `set_updates`, `check_updates`, `install_update`, `skip_update`, `unskip_update`, `skip_setup`, `relaunch`, `open_link` | General, About | settings persist, update flow |

Tabs to visit (ids from `ui/ui-*.js`): `overview`, `receive`/`mac-pc`, `share`/`pc-mac`, `cursor`,
`keyboard`, `clipboard`, `notifications`, `files`, `terminal`, `audio`, `camera`, `apps`, `multi`,
`general`, `about`, plus the phone sections when a phone is paired.

## Procedure

### Phase 0 — preflight (both sides)

1. Both apps running (`pgrep` on the Mac, `tasklist` over `ssh pc`), same version line in both logs.
2. Link `Connected` in both windows; peer name and address correct on both.
3. `pairs.json` on both sides contains the same `BEAM_AUTH_V1:` token; `identity.json` present.
4. Record the current value of every switch you will touch (Bluetooth with phones/computers, cursor
   and keyboard directions, clipboard directions and master, sound, frame generation, notifications,
   camera, HUD, startup, updates, media controls).
5. Create the two scratch folders. Record the log line offset on each side (`Get-Content … | Measure`
   on Windows, `wc -l` on the Mac) so later checks only read new lines.

### Phase 1 — pairing and lifecycle

- Restart Windows only (`taskkill` + `schtasks /Run /TN GlideDesk`): the Mac must show the peer back
  within ~20 s and the Windows log must contain `[Startup] Reconnected` then
  `[Auth] Peer … authenticated successfully`.
- Restart the Mac only: the same, mirrored.
- Disconnect on one side, then connect again: no new code is needed, both re-authenticate.
- Forget: the pair disappears on that side only; re-pairing needs a fresh code; the codes match on
  both; `pairs.json` is rewritten with one token.
- While paired but with the other app down, the sidebar says `<peer> · Offline` and the settings are
  visibly not actionable.

### Phase 2 — the shell

- Every sidebar tab opens, the heading and body render, no blank page, no console error in the log.
- Each feature page shows its one-line introduction bar (`Show me`, `Hide`) **and** its settings;
  `Show me` opens the picture, `Show less` folds it, `Hide` removes the bar and leaves the settings.
- Overview: peer card, found-nearby list, Bluetooth switches, the two-computer diagram and its
  Disconnect.
- General: startup, popup-on-switch, media controls, theme, update row.
- About: version, links, licences.

### Phase 3 — cursor, keyboard, wheel, clipboard

- Move the Mac pointer off the right edge: the Windows pointer takes over at the same height
  (`[KVM] take`), and moving back returns it to the Mac. Repeat left→right and right→left.
- Type on the peer while the pointer is there (letters, capitals, a shortcut such as Ctrl/Cmd+C),
  confirm the text lands in a scratch text field on the peer and no key bounces back.
- Wheel up/down on both directions: the peer scrolls the same way.
- Switch the cursor and keyboard directions off in each tab and confirm input stops crossing.
- Clipboard: copy text on the Mac, paste on Windows; copy on Windows, paste on the Mac; a picture
  copy; switch both directions off and confirm nothing crosses; history lists the last items and
  Clear empties it.
- Lock the cursor, reverse scrolling, trackpad gestures and the HUD (`show_hud`/`hide_hud`) react.

### Phase 4 — screens

- Share screen (Windows→Mac or Mac→Windows, whichever the pair allows): the other side gets a window
  with the picture; Off/Mirror/Extend switch it; Fastest/Smooth and frame generation change the
  stream; sound plays; F11 goes full screen; closing the window turns sharing off.
- Receive screen: Open starts it and the master switch reflects `stream.state`; when the other
  computer forbids it (`remote` false) the switch is disabled with the reason shown.
- The sharing side's own state (`set_sharing` / `set_sharing_opts`) matches what the receiving side
  shows — no control that only changes locally.

### Phase 5 — files

- Send a file Mac→Windows with Choose…: it lands in the Windows received folder, progress finishes,
  the Recent list shows it, and the Windows Files tab shows it as received.
- Send Windows→Mac the same way, and by dragging across the screen edge.
- Copy and paste files, take the other computer's files, open its shared folder.
- Share a folder with the other computer (Choose…): the switch stays on, the peer sees the folder,
  and turning it off removes it — check that `set_sharing` is *not* what the folder switch calls
  (folder sharing only).
- Received folder: change it, reset it, open it.

### Phase 6 — terminal

- Set up the SSH server where the page offers it (`setup_sshd`, `fix_firewall`, permission prompts)
  and confirm the page then offers Open Terminal.
- Open a terminal on the peer from each side: a shell appears, `whoami`/`hostname` answers from the
  other computer, and closing it leaves no stray process behind.

### Phase 7 — apps

- Refresh lists the peer's apps; launch one and confirm it opens **on the peer**; pin and unpin;
  switch the direction ("Open the other computer's apps on This PC" / "Open This PC's apps on the
  other computer") and repeat.
- Launching an app the peer cannot show must fail visibly, not silently.

### Phase 8 — audio

- Play sound from the peer here: the Mac's output carries the Windows system sound, volume and mute
  work.
- Send this computer's sound to the peer: the peer's speakers play it; "Mute while sending" silences
  the local side only; stopping restores local playback.
- `set_app` per-app routing changes only the chosen app.

### Phase 9 — notifications

- Trigger a notification on Windows (e.g. a calendar or a `msg`-style test): the Mac shows it with
  the app icon and text, within a few seconds.
- Trigger one on the Mac: Windows shows it.
- Hide message text, reply from the notification (`notif_reply`), open the app (`open_notified_app`),
  mute one app (`mute_notif_app`) and confirm the next one of that app stays quiet.
- Clear history on both sides.

### Phase 10 — camera

- Virtual camera: turn it on, confirm a virtual device appears where the system lists cameras, and
  that a viewer (QuickTime on the Mac / Camera on Windows) shows the feed.
- Share this camera: pick a device, preview, stop; confirm the peer sees the feed and the device list
  is the local one.
- `install_vdd` / `remove_vdd` only when the OS needs the driver, and only after confirming the user
  is fine with a driver install.

### Phase 11 — settings, updates, network

- General: startup, popup on switching, media controls on the phone page, "Open"/"Change…" rows —
  each changes the stored state and survives a restart.
- Updates: Check for updates reports truthfully and the update row does not claim a version it
  cannot install.
- Network: Bluetooth with phones off by default and switchable off; Bluetooth with computers;
  Wi-Fi/hotspot actions report success or a real error.
- Window handoff (`handoff_window`, `handoff_window_at_point`, `return_window`,
  `get_parked_windows`): move a window to the peer and bring it back.

### Phase 12 — phones (only if one is paired; else mark Blocked)

Phone page, Calls, Messages, Photos, Phone files, Mount: each control reaches the backend
(`phone_*`, `set_phone`, `set_phone_remote`), the phone shows the same state, and unmount leaves no
drive behind.

### Phase 13 — resilience

- Sleep/wake one computer: the pair reconnects without a new code.
- Kill the app on one side while it is streaming and while it is transferring: the other side
  reports it, cleans up, and reconnects when it returns.
- Change `identity.json` on one side on purpose: the pair reports "looks different now", does not
  silently trust the new identity, and Forget + pair again fixes it.

### Phase 14 — restore and report

1. Put every switch back to the Phase 0 value; confirm with a fresh tree/screenshot per side.
2. Leave both apps paired and connected; delete the scratch folders and `glidedesk-e2e` screenshots
   you no longer need (keep the report).
3. Write the report:

| # | Check | macOS | Windows | Evidence | Result |
|---|-------|-------|---------|----------|--------|
| 1 | Restart Windows → auto-reconnect | `[Startup] Reconnected` | window `Connected` | `<run>/phase1-*.png` | PASS |

`Result` is `PASS`, `FAIL`, or `BLOCKED` (with the reason: no phone, no second display, driver
install declined, …). A check counts as PASS only when the effect was observed, not when the click
succeeded.

4. For every FAIL: name the file and the line that causes it, fix it, rebuild/redeploy both apps,
   re-run that check, and record the before/after. Do not batch fixes without re-running the checks
   they touch.

## Definition of done

- All phases walked on both machines, with a screenshot or tree dump for every PASS.
- Every FAIL either fixed and re-verified, or left in the report with a precise cause and the exact
  reproduction.
- Both apps left paired, connected, with the user's settings as they were.
- The report saved to `~/glidedesk-e2e/<run-id>/report.md` and summarised in the chat: counts of
  PASS/FAIL/BLOCKED, the failures with their causes, and anything that still needs a human (driver
  install, macOS permission prompts, a phone).
