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
| 2026-11-16 | macOS aarch64, US1/US2 phone leaf (route takes, take-back, `ts` dedupe) | 330 pass | 24 suites pass | 7 JUnit pass (`MeshLeafTest`) | not run (no `workflow` scope on the team token) |
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
| S2 | Pointer and keyboard cross | macOS → Android | | open | real phone not attached; leaf rules covered by the automated rows below |
| S2 | Phone takes a routed `kvm.take`, honours `mouse: false`, refuses when cursor sharing is off | unit test, no hardware | `a_newer_take_is_accepted_and_an_older_one_is_ignored` | automated pass | `android/.../MeshLeaf.kt` (`take`), `Kvm.kt` (`take`) |
| S2 | A release or `kvm.home` only ends the handover in progress | unit test, no hardware | `a_release_only_ends_the_handover_in_progress` | automated pass | `android/.../MeshLeaf.kt` (`release`) | |
| S3 | Chain through a driven device | macOS → Windows → Linux | | | |
| S4 | Refusals and takeover | all | | | |
| S4 | The driven phone takes control back, naming the route it was driven on | unit test, no hardware | `taking_control_back_names_the_route_once` | automated pass | `android/.../MeshLeaf.kt` (`takeBack`), `Kvm.kt` (`takeBack`) |
| S4 | No handover survives the session ending | unit test, no hardware | `a_handover_does_not_survive_the_session` | automated pass | `android/.../MeshLeaf.kt` (`reset`), `Kvm.kt` (`onClosed`) |
| S5 | Pair once, meet everyone | macOS + Windows + Android | | open | phone and Windows machine unavailable; automated rows below |
| S5 | Roster offers a phone paired with one computer to the other | unit test, no hardware | `a_device_paired_with_one_computer_appears_on_the_other` | automated pass | `src-tauri/src/state.rs` — roster `peers`/`members`, `reckon` gating, `known` |
| S5 | A stale roster cannot undo a newer one, and one from a stranger is ignored | unit test, no hardware | `only_a_newer_roster_from_a_connected_device_is_taken` | automated pass | `src-tauri/src/mesh/roster.rs` |
| S5 | A forgotten device is not re-added, re-dialled or re-learned | unit test, no hardware | `a_forgotten_device_stays_forgotten` | automated pass | `src-tauri/src/state.rs`, `src-tauri/src/mesh/roster.rs` |
| S5 | Each paired device authenticates with its own token | unit test, no hardware | `each_paired_device_authenticates_with_its_own_token` | automated pass | `src-tauri/src/state.rs` (`authenticate_peer`, `token_for`) |
| S5 | A dial carries the token and fingerprint of the device at that address | unit test, no hardware | `a_dial_finds_the_paired_device_at_that_address` | automated pass | `src-tauri/src/state.rs` (`paired_at`), `NetworkManager::connect_to` |
| S6 | Clipboard pool | macOS + Windows + Linux | | | |
| S6 | Clipboard pool: in-process, three devices, 100 alternating copies converge, no double apply | unit test, no hardware | `a_hundred_copies_from_three_devices_converge` | automated pass | `src-tauri/src/mesh/pool.rs` — the hardware rows stay open |
| S6 | Clipboard pool | macOS + Windows + Android | | open | real phone not attached; ordering rows below |
| S6 | The phone applies a pooled copy once, and only when it is newer | unit test, no hardware | `a_pooled_copy_is_applied_once_and_only_when_newer` | automated pass | `android/.../MeshLeaf.kt` (`clipIsNewer`, `clipApplied`), `Clip.kt` |
| S6 | A stamp past the protocol maximum is capped, and an unstamped copy still applies | unit test, no hardware | `a_stamp_beyond_the_protocol_maximum_is_capped` | automated pass | `android/.../MeshLeaf.kt` (`MAX_TS`) | |
| S7 | Platform parity | Windows driven | | | |
| S7 | Platform parity | Linux X11 driven | | | |
| S7 | Platform parity | Linux Wayland driven | | | |
| S7 | Platform parity | Android leaf | | open | real phone not attached; leaf rules below |
| S7 | The phone never hands a crossing to a third device (no `kvm.next`, no dial) | unit test, no hardware | `the_phone_never_relays_a_crossing` | automated pass | `android/.../MeshLeafTest.kt` — scans `Kvm.kt` and `MeshLeaf.kt` and the protocol constants | |
| L | Legacy check (previous release) | any | | | |

## Outstanding hardware

Blocked while the machine or the phone is unavailable; record the blocker, not a pass.

| Machine | Blocker |
|---|---|
| Windows PC `192.168.4.11` | off the network at the time of writing (SSH refused, no discovery replies) |
| Linux VM "GlideDesk Linux ARM64" | stopped in UTM |
| Android phone | pairing hardware not attached; UAC Secure Desktop cannot be automated |
