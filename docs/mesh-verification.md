# Mesh verification log

Where the evidence for `specs/001-multi-device-mesh/quickstart.md` is recorded. Run the scenarios in
the order the quickstart gives and fill a row per scenario and per machine. A scenario is only
"pass" when the automated checks are green first (`cargo test`, `npm test`, Android companion tests).

Keep log excerpts rather than whole files: the `[KVM]` and `[Mesh]` lines around the event are what
a later reader needs.

## Automated checks

| Date | Build | `cargo test` | `npm test` | Android | CI run |
|---|---|---|---|---|---|
| 2026-11-16 | macOS aarch64, `npm run build:mac` | 263 pass | 23 suites pass | pass | not run (no `workflow` scope on the team token) |
| 2026-11-16 | macOS aarch64, US2 (edges, handover, chain) | 295 pass | 24 suites pass | pass | not run (no `workflow` scope on the team token) |
| 2026-11-16 | macOS aarch64, US3 (pooled clipboard) | 317 pass | 24 suites pass | pass | not run (no `workflow` scope on the team token) |
| 2026-11-16 | macOS aarch64, US4 (roster, pair once, forget) | 330 pass | 24 suites pass | pass | not run (no `workflow` scope on the team token) |
| | | | | | |

## Scenarios

| # | Scenario | Machine | Build | Result | Evidence |
|---|---|---|---|---|---|
| S0 | Dormant with one device | macOS | | | |
| S0 | Dormant with one device | Windows | | | |
| S1 | Three devices connected at once | macOS + Windows + Linux | | | |
| S1 | Three devices connected at once | macOS + Windows + Android | | | |
| S2 | Pointer and keyboard cross | macOS → Windows | | | |
| S2 | Pointer and keyboard cross | Windows → macOS | | | |
| S2 | Pointer and keyboard cross | macOS → Linux | | | |
| S2 | Pointer and keyboard cross | macOS → Android | | | |
| S3 | Chain through a driven device | macOS → Windows → Linux | | | |
| S4 | Refusals and takeover | all | | | |
| S5 | Pair once, meet everyone | macOS + Windows + Android | | open | phone and Windows machine unavailable; automated rows below |
| S5 | Roster offers a phone paired with one computer to the other | unit test, no hardware | `a_device_paired_with_one_computer_appears_on_the_other` | automated pass | `src-tauri/src/state.rs` — roster `peers`/`members`, `reckon` gating, `known` |
| S5 | A stale roster cannot undo a newer one, and one from a stranger is ignored | unit test, no hardware | `only_a_newer_roster_from_a_connected_device_is_taken` | automated pass | `src-tauri/src/mesh/roster.rs` |
| S5 | A forgotten device is not re-added, re-dialled or re-learned | unit test, no hardware | `a_forgotten_device_stays_forgotten` | automated pass | `src-tauri/src/state.rs`, `src-tauri/src/mesh/roster.rs` |
| S5 | Each paired device authenticates with its own token | unit test, no hardware | `each_paired_device_authenticates_with_its_own_token` | automated pass | `src-tauri/src/state.rs` (`authenticate_peer`, `token_for`) |
| S5 | A dial carries the token and fingerprint of the device at that address | unit test, no hardware | `a_dial_finds_the_paired_device_at_that_address` | automated pass | `src-tauri/src/state.rs` (`paired_at`), `NetworkManager::connect_to` |
| S6 | Clipboard pool | macOS + Windows + Linux | | | |
| S6 | Clipboard pool: in-process, three devices, 100 alternating copies converge, no double apply | unit test, no hardware | `a_hundred_copies_from_three_devices_converge` | automated pass | `src-tauri/src/mesh/pool.rs` — the hardware rows stay open |
| S6 | Clipboard pool | macOS + Windows + Android | | | |
| S7 | Platform parity | Windows driven | | | |
| S7 | Platform parity | Linux X11 driven | | | |
| S7 | Platform parity | Linux Wayland driven | | | |
| S7 | Platform parity | Android leaf | | | |
| L | Legacy check (previous release) | any | | | |

## Outstanding hardware

Blocked while the machine or the phone is unavailable; record the blocker, not a pass.

| Machine | Blocker |
|---|---|
| Windows PC `192.168.4.11` | off the network at the time of writing (SSH refused, no discovery replies) |
| Linux VM "GlideDesk Linux ARM64" | stopped in UTM |
| Android phone | pairing hardware not attached; UAC Secure Desktop cannot be automated |
