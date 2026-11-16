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
| S5 | Pair once, meet everyone | macOS + Windows + Android | | | |
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
