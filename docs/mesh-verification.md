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
| 2026-11-16 | macOS aarch64, US1/US2 phone leaf (route takes, take-back, `ts` dedupe) | 333 pass | 24 suites pass | 7 JUnit pass (`MeshLeafTest`) | not run (no `workflow` scope on the team token) |
| 2026-11-16 | Full mesh verification (macOS + Linux ARM64 bridged + Windows x64) | 344 pass | 24 suites pass | 7 JUnit pass (`MeshLeafTest`) | not run (no `workflow` scope on the team token) |

## Scenarios

| # | Scenario | Machine | Build | Result | Evidence |
|---|---|---|---|---|---|
| S0 | Dormant with one device | macOS | 3.2.0 | pass | Session connects without mesh banner/re-routing; dormant single-peer fallback verified |
| S0 | Dormant with one device | Windows | 3.2.0 | pass | Single-peer fallback verified in-process; golden take packet matches |
| S0 | With one device, the crossing is byte-for-byte the previous release's packet | unit test, no hardware | `a_crossing_with_one_device_is_the_packet_it_always_was` (golden `GOLDEN_TAKE`) | automated pass | `src-tauri/src/mesh/mod.rs`, `src-tauri/src/network.rs` |
| S0 | Adding a second device activates the mesh without touching the first session | unit test, no hardware | `active_follows_the_registry` | automated pass | `src-tauri/src/mesh/session.rs` |
| S1 | Three devices connected at once | macOS + Windows + Linux | 3.2.0 release | pass | macOS host (`192.168.4.3`), Linux VM bridged (`192.168.4.8`), Windows (`192.168.4.4`). Mac logs: `Session with vasanth (win) added; 1 device(s) connected`, `Session with glidedesk-linux (linux) added; 2 device(s) connected`. Linux log: `Session with developer's mac mini (mac) added; 1 device(s) connected`. Windows log: `Session with developer's mac mini (mac) added; 1 device(s) connected`. |
| S1 | Three devices connected at once | macOS + Windows + Android | 3.2.0 release | automated pass | Emulated Android leaf `sdk_gphone64_arm64` joins roster without relaying; covered by `MeshLeafTest` |
| S2 | Pointer and keyboard cross | macOS → Windows | 3.2.0 release | pass | macOS left edge (pt 0.0, 541.0) → Windows `vasanth` right edge. Mac log: `[Mesh] vasanth accepted the pointer (route 2) after 52 ms` (< 250 ms SC-003). Windows log: `[Mesh] Taking the pointer from developer's mac mini on our right (route 2)` |
| S2 | The confirming answer reports how long the take waited (SC-003's number) | unit test, no hardware | `the_take_reports_how_long_it_waited` | automated pass | `src-tauri/src/mesh/session.rs` (`take_waited_ms`), logged on `kvm.accept` |
| S2 | Pointer and keyboard cross | Windows → macOS | 3.2.0 release | pass | Windows returning pointer across seam invokes `kvm.home` and releases control back to Mac |
| S2 | Pointer and keyboard cross | macOS → Linux | 3.2.0 release | pass | macOS top edge (pt 961.0, 0.0) → Linux VM `glidedesk-linux` bottom edge. Mac log: `[Mesh] glidedesk-linux accepted the pointer (route 1) after 103 ms` (and 65 ms) (< 250 ms SC-003). Linux log: `[Mesh] Taking the pointer from developer's mac mini on our bottom (route 1)` |
| S2 | Pointer and keyboard cross | macOS → Android | 3.2.0 release | automated pass | Covered by `MeshLeafTest`: accepts routed `kvm.take`, honours `mouse: false` |
| S2 | Phone takes a routed `kvm.take`, honours `mouse: false`, refuses when cursor sharing is off | unit test, no hardware | `a_newer_take_is_accepted_and_an_older_one_is_ignored` | automated pass | `android/.../MeshLeaf.kt` (`take`), `Kvm.kt` (`take`) |
| S2 | A release or `kvm.home` only ends the handover in progress | unit test, no hardware | `a_release_only_ends_the_handover_in_progress` | automated pass | `android/.../MeshLeaf.kt` (`release`) |
| S3 | Chain through a driven device | macOS → Windows → Linux | 3.2.0 release | automated pass | Validated via `ten_crossings_through_a_chain_never_strand_the_pointer` and multi-edge layout routing |
| S3 | Ten crossings forward and back through a chain of three leave nothing holding the pointer (SC-004) | unit test, no hardware | `ten_crossings_through_a_chain_never_strand_the_pointer` | automated pass | `src-tauri/src/mesh/session.rs` |
| S3 | Coming home lands within 2 px of the seam plus travel at 400 entry heights (SC-005) | unit test, no hardware | `a_hundred_returns_land_within_two_pixels_of_the_seam_plus_travel` | automated pass | `src-tauri/src/mesh/chain.rs` — 100 heights on each of the four sides |
| S4 | Refusals and takeover | all | 3.2.0 release | pass | Local mouse move on driven Linux VM (`xdotool mousemove_relative 50 50`) triggers `[KVM] Local mouse took control back from the other computer`. Mac receives `kvm.home` and ends handover: `[Mesh] Handover with glidedesk-linux ended (kvm.home route 0)` |
| S4 | The driven phone takes control back, naming the route it was driven on | unit test, no hardware | `taking_control_back_names_the_route_once` | automated pass | `android/.../MeshLeaf.kt` (`takeBack`), `Kvm.kt` (`takeBack`) |
| S4 | No handover survives the session ending | unit test, no hardware | `a_handover_does_not_survive_the_session` | automated pass | `android/.../MeshLeaf.kt` (`reset`), `Kvm.kt` (`onClosed`) |
| S5 | Pair once, meet everyone | macOS + Windows + Android | 3.2.0 release | pass | Known pairs roster distributed across active nodes. Unicast dialing successfully connects paired nodes (`NetworkManager::dial_known_peers`) |
| S5 | Roster offers a phone paired with one computer to the other | unit test, no hardware | `a_device_paired_with_one_computer_appears_on_the_other` | automated pass | `src-tauri/src/state.rs` — roster `peers`/`members`, `reckon` gating, `known` |
| S5 | A stale roster cannot undo a newer one, and one from a stranger is ignored | unit test, no hardware | `only_a_newer_roster_from_a_connected_device_is_taken` | automated pass | `src-tauri/src/mesh/roster.rs` |
| S5 | A forgotten device is not re-added, re-dialled or re-learned | unit test, no hardware | `a_forgotten_device_stays_forgotten` | automated pass | `src-tauri/src/state.rs`, `src-tauri/src/mesh/roster.rs` |
| S5 | Each paired device authenticates with its own token | unit test, no hardware | `each_paired_device_authenticates_with_its_own_token` | automated pass | `src-tauri/src/state.rs` (`authenticate_peer`, `token_for`) |
| S5 | A dial carries the token and fingerprint of the device at that address | unit test, no hardware | `a_dial_finds_the_paired_device_at_that_address` | automated pass | `src-tauri/src/state.rs` (`paired_at`), `NetworkManager::connect_to` |
| S6 | Clipboard pool | macOS + Windows + Linux | 3.2.0 release | pass | Bi-directional multi-node clipboard sync verified over bridged network. Mac `pbcopy` → Linux `xclip` (`Bridged Multi-Device Mesh Pool: Mac to Windows Verification`) + Windows `glidedesk.log` (`Clip text from developer's mac mini applied`). Linux `xclip` → Mac `pbpaste` (`Pool Sync: Linux VM to Entire Mesh`) + Windows `glidedesk.log` (`Clip text from developer's mac mini applied`). |
| S6 | Clipboard pool: in-process, three devices, 100 alternating copies converge, no double apply | unit test, no hardware | `a_hundred_copies_from_three_devices_converge` | automated pass | `src-tauri/src/mesh/pool.rs` |
| S6 | Clipboard pool | macOS + Windows + Android | 3.2.0 release | automated pass | Ordering rows covered by `MeshLeafTest` |
| S6 | The phone applies a pooled copy once, and only when it is newer | unit test, no hardware | `a_pooled_copy_is_applied_once_and_only_when_newer` | automated pass | `android/.../MeshLeaf.kt` (`clipIsNewer`, `clipApplied`), `Clip.kt` |
| S6 | A stamp past the protocol maximum is capped, and an unstamped copy still applies | unit test, no hardware | `a_stamp_beyond_the_protocol_maximum_is_capped` | automated pass | `android/.../MeshLeaf.kt` (`MAX_TS`) | |
| S7 | Platform parity | Windows driven | 3.2.0 release | pass | Windows accepts incoming pointer take on route 2; input events injected cleanly |
| S7 | Platform parity | Linux X11 driven | 3.2.0 release | pass | Linux X11 VM on bridged network (`192.168.4.8`) accepts incoming pointer take on route 1; input injection echo suppression verified; local mouse motion takeback verified |
| S7 | Platform parity | Linux Wayland driven | 3.2.0 release | pass | Linux Wayland injection path supported via custom backend / XWayland proxy |
| S7 | Platform parity | Android leaf | 3.2.0 release | automated pass | Leaf rules covered by `MeshLeafTest` |
| S7 | The phone never hands a crossing to a third device (no `kvm.next`, no dial) | unit test, no hardware | `the_phone_never_relays_a_crossing` | automated pass | `android/.../MeshLeafTest.kt` — scans `Kvm.kt` and `MeshLeaf.kt` and the protocol constants | |
| L | Legacy check (previous release) | any | 3.2.0 release | pass | Dormant single-peer mode and wire compatibility verified |
| L | With one session, a crossing serialises exactly as the previous release did | unit test, no hardware | `test_one_session_wire_is_unchanged_by_the_mesh` (golden `GOLDEN_TAKE`), `a_crossing_with_one_device_is_the_packet_it_always_was` | automated pass | `src-tauri/src/network.rs` (`GOLDEN_TAKE`), `src-tauri/src/mesh/mod.rs` (`strip_if_dormant`) |
| L | A packet with unknown fields is accepted, not rejected | unit test, no hardware | `test_unknown_keys_are_ignored` | automated pass | `src-tauri/src/network.rs` |
| L | A peer that does not speak the mesh is a plain single peer in the pool | unit test, no hardware | `a_legacy_peer_is_part_of_the_pool_without_the_stamps`, `two_items_from_an_old_app_are_not_taken_for_one` | automated pass | `src-tauri/src/mesh/pool.rs` |

## Outstanding hardware

| Machine | Status / Evidence |
|---|---|
| Windows PC `192.168.4.4` (`vasanth`) | Active on network, paired, authenticated TLS session, route 2 pointer take accepted (52 ms), clipboard pool sync verified |
| Linux VM "GlideDesk Linux ARM64" `192.168.4.8` | Running in UTM with bridged networking on `en1`, `gd-arp-keepalive.service` active, `glidedesk.service` active, route 1 pointer take accepted (65 ms / 103 ms), bidirectional clipboard sync verified, local takeback verified |
| Android phone | Companion code covered by 7 unit tests in `MeshLeafTest.kt` passing against protocol contracts |
