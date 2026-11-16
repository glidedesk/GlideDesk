# Quickstart: verifying multi-device mesh

**Status**: Phase 1 design. Written to be run by a person at the machines, in this order.

## Machines used

| Role | Machine | Notes |
|---|---|---|
| A | Mac (this desk) | 1920×1080 logical, primary development machine |
| B | Windows PC | 1920×1080 (or 1366×768), `192.168.4.11` |
| C | Linux VM (Ubuntu ARM64, UTM) | X11 session first, then a Wayland session |
| D | Android phone | leaf device |

All on the same LAN. Same build version on every device.

## Automated checks (must pass before any manual scenario)

```bash
# 1. Rust: unit + the mesh integration test
cd src-tauri && cargo test

# 2. Protocol, mesh and state-machine suites (includes the vendored JS mesh core)
cd .. && npm test

# 3. Android companion
npm test -- --android    # or the Gradle test task, per scripts/test-all.js
```

Expected: `cargo test` green including the new `mesh::*` tests and the three-session integration
test; `npm test` green with the existing 22 suites unchanged.

## S0 — Dormant with one device

1. Start the app with only device B paired and connected.
2. Confirm: no Multi-device page in navigation, no cursor/sharing changes, clipboard behaves as
   before.
3. In the log, confirm no mesh fields are emitted on a crossing (`grep 'kvm.take' glidedesk.log`
   shows the same shape as the previous release).

**Pass**: identical behaviour and wire to the current build (contracts/wire.md rule 2).

## S1 — Three devices connected at once

1. Pair A↔B. Then connect C using its own pairing with A (one pairing only; see S5).
2. Connect D to A.
3. **Pass**: A lists B, C and D simultaneously; connecting D does not push B or C away; the
   Multi-device page appears on A and B without a restart.
4. Sleep C for 30 s and wake it: A and B stay connected; C returns on its own.

## S2 — Pointer and keyboard cross to the mapped device

1. On A, Multi-device page: assign `right` → B, `top` → C, `left` → D.
2. Push the pointer into A's right edge and rest it there.
3. **Pass**: B takes the pointer after the configured hold (~160 ms + resistance), the pointer appears
   on B's facing edge at the same height, keys typed on A's keyboard land on B, and A's own keystrokes
   do not appear locally.
4. Move the hand back past the entry point.
5. **Pass**: control returns to A landing at the seam spot it left by (±2 px plus travel), not the
   middle of the edge; an immediate second push does not cross until the hold is met again.
6. Repeat at the top quarter and the bottom quarter of the edge (SC-005).

## S3 — Chain through a driven device

1. With A driving B, push further through B's right edge, which is assigned to C.
2. **Pass**: B hands the pointer to C (`kvm.next` in the log), B stops being driven, C is now driven
   by A, and A's pointer record shows a new route.
3. Come back: the chain unwinds C → B → A, in that order, one hold per hop.

## S4 — Refusals and takeover

| Case | Expected |
|---|---|
| Edge mapped to a disconnected device | nothing taken; page shows "not connected" |
| Edge mapped to a device with cursor sharing off | `kvm.refuse not_allowed`; page shows the reason; no retry loop in the log |
| B's own mouse moved while A drives it | B takes control back immediately; A stops sending input and shows home |
| A and B push into each other's edge at the same instant | one keeps control, the other goes home; neither is left without a pointer |

## S5 — Pair once, meet everyone

1. Forget C everywhere, then pair C with A only.
2. Connect B (already paired with A).
3. **Pass**: C appears on B without a pairing code on B.
4. Forget C on B, reconnect: C does not come back through A's roster.

## S6 — Clipboard pool

1. Copy text on A; paste on B, C and D. **Pass**: arrives within 1 s on each.
2. Copy on C, then immediately on B. **Pass**: every device pastes B's item (newest wins).
3. Copy an image on A with images enabled; paste on B. Then disable images and repeat: not sent.
4. Copy a password-manager entry marked concealed on A. **Pass**: not published.
5. Copy 100 alternating items across A, B and C (`for` loop with `pbcopy`/`clip.exe`/`xclip`).
   **Pass**: every device ends with the same newest item; no log line shows the same `(origin, seq)`
   applied twice; no item is re-published by a receiver.
6. Restart C mid-sequence and continue copying. **Pass**: no duplicate deliveries after the restart.

## S7 — Platform parity

Repeat S2 and S4 on each platform as the *driven* machine:

- **Windows**: hold is honoured (no instant crossing), drag has no stutter, return lands at the
  crossing height.
- **Linux X11**: hold measured per poll, pointer takes control back on hand movement.
- **Linux Wayland**: same, with the helper's capabilities reported honestly; if the session cannot
  inject, the page must say so rather than pretend.
- **Android**: the phone can be driven, can take control back, and is never asked to relay (`kvm.next`
  never sent by the phone, asserted in its tests).

## Legacy check

1. Keep one device on the previous release, connected to A.
2. **Pass**: no errors on either side; the old device behaves as a single peer; A does not send it
   pooled items or mesh fields.

## Evidence to keep

- `~/Library/Logs/GlideDesk/glidedesk.log` (A), `%APPDATA%\GlideDesk\logs\glidedesk.log` (B),
  the VM's log (C) for each scenario — excerpts with the `[KVM]` and `[Mesh]` lines.
- The Multi-device page screenshot with three sessions and their roles.
- `cargo test` and `npm test` summaries.

## Failure triage

| Symptom | First place to look |
|---|---|
| Pointer crosses instantly | dwell not applied on that platform (`edge_push_ready` / `edge_hold_ready`) |
| Pointer comes home mid-edge | return used the parked position instead of `entryFraction` + travel |
| Two devices both think they drive | `route` comparison on receive, or a second `take` without a `release` |
| Clipboard ping-pong | item re-published by a receiver, or `ts` not compared |
| Device dropped when another connects | session registry replaced instead of inserted |
| Old app errors | a non-optional field was added, or a pooled message sent to a `pool = false` peer |
