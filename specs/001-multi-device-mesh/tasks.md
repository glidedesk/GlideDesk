# Tasks: Multi-device mesh — one pointer, one keyboard, one clipboard

**Input**: Design documents from `/specs/001-multi-device-mesh/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/wire.md, contracts/state-and-ui.md

**Tests**: Required. Constitution principle II makes tests part of every task that changes behaviour;
tasks in this list name the test before the implementation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4 from spec.md; US5 (monitor map) is explicitly out of this slice

## Path Conventions

Rust core in `src-tauri/src/`, webview UI in `ui/`, phone companion in `android/app/src/main/java/com/glidedesk/android/`, design docs in `specs/001-multi-device-mesh/`.

---

## Phase 1: Setup

- [X] T001 Create `src-tauri/src/mesh/mod.rs` with the module skeleton (`pool`, `roster`, `session`, `clip`, `edges`) and register it in `src-tauri/src/main.rs`; no behaviour yet
- [X] T002 [P] Add `src-tauri/src/mesh/README.md` mapping each Rust module to the vendored reference (`ui/shared/mesh.js`, `clipsync.js`, `kvm-edges.js`) and to the JS test file whose scenarios it must match
- [X] T003 [P] Add the verification stubs in `specs/001-multi-device-mesh/quickstart.md` to the repo's manual test log (`docs/`), so evidence has a home

---

## Phase 2: Foundational (blocking — every story needs this)

- [X] T004 [P] Test: serialising a crossing packet with one session produces the pre-feature JSON byte for byte (golden fixture captured from the current build) in `src-tauri/src/network.rs` tests
- [X] T005 [P] Test: a packet carrying unknown keys deserialises and is handled without error in `src-tauri/src/network.rs` tests
- [X] T006 Add optional wire fields `route`, `to`, `mouse`, `keyboard`, `via`, `ts`, `reckon` to `WireMessage` in `src-tauri/src/network.rs` (all `#[serde(default)]`, emitted only when `mesh_active`) — satisfies contracts/wire.md
- [X] T007 Add `Session` and the session registry (`HashMap<String, Session>`) to `src-tauri/src/state.rs`, populated on connect/disconnect, leaving `clients`, `client`, `peer` in place as derived views — satisfies FR-001, FR-002
- [X] T008 Replace `clients.first()` uses with a `partner()` helper (partner session, else the single session, else `None`) in `src-tauri/src/state.rs` and `src-tauri/src/kvm.rs` — satisfies FR-001, keeps one-peer behaviour
- [X] T009 Add `mesh_active()` predicate (`sessions.len() >= 2`) and gate every new wire field on it in `src-tauri/src/network.rs`, `src-tauri/src/kvm.rs` — satisfies FR-003
- [X] T010 Add `send_packet_to(device_id, pkt)` and make fan-out explicit for pooled messages in `src-tauri/src/network.rs` — satisfies FR-003, research D3
- [X] T011 Add receive-side address validation (drop packets not addressed to this device) and a test that a `kvm.take` addressed elsewhere is ignored in `src-tauri/src/network.rs` — closes the current multi-peer hole
- [X] T012 Remove `set_multi` and `MultiState.enabled` from `src-tauri/src/commands.rs`, `src-tauri/src/state.rs`, `src-tauri/src/main.rs` and `ui/shared/tauri-bridge.js`; add a test that no such command is registered — satisfies FR-002 (no dead controls)
- [X] T013 Emit `mesh` in the `state-change` payload per contracts/state-and-ui.md, with `active`, `partner`, `role`, `route`, `sessions` in `src-tauri/src/state.rs`

**Checkpoint**: with one device connected, `cargo test` and `npm test` pass unchanged and the golden JSON test proves nothing new is on the wire.

---

## Phase 3: US1 — Several devices connected at the same time (P1)

- [X] T014 [P] [US1] Test: three sessions connect and none is disconnected by a later one (`src-tauri/src/state.rs` tests)
- [X] T015 [P] [US1] Test: the eleventh connection is refused with a reason while ten keep working (`src-tauri/src/network.rs` tests)
- [X] T016 [US1] Replace the single-session assumption in the accept path (`src-tauri/src/network.rs`, `handle_session`) so a new client is inserted rather than replacing the current peer — satisfies FR-001
- [X] T017 [US1] Enforce `MAX_DEVICES = 10` with a refusal message the UI can show — satisfies FR-005
- [X] T018 [US1] Derive `clients`, `client`, `peer` from the registry in `src-tauri/src/state.rs` so every existing page keeps working — satisfies FR-002
- [X] T019 [US1] Add connection lifecycle tests: sleep/wake of one device leaves the others connected (`src-tauri/src/network.rs` tests) — satisfies SC-002
- [X] T019a [US1] Test: activating on the second connection and returning to one device changes no setting, pairing or preference (`src-tauri/src/state.rs` tests) — satisfies FR-004
- [X] T020 [US1] Multi-device page appears only with two or more sessions and lists them with live role in `ui/ui-multi.js` — satisfies FR-002, contracts/state-and-ui.md
- [X] T021 [US1] Add `disconnect_device` command in `src-tauri/src/commands.rs` and wire the page's per-row action — other sessions must be unaffected
- [X] T022 [P] [US1] JS test: the page renders zero, one, two and ten sessions without a dead control (`ui/shared/test/ui-multi.js`)

**Checkpoint**: three devices stay connected, the page exists only when it should, and one-device behaviour is untouched.

---

## Phase 4: US2 — Pointer and keyboard go where the screens are (P1)

- [X] T023 [P] [US2] Test: edge map normalisation matches `ui/shared/kvm-edges.js` (duplicate device on two sides keeps the first, in `left, right, top, bottom` order) in `src-tauri/src/mesh/edges.rs`
- [X] T024 [P] [US2] Test: a take with a route at or below the session's is ignored; a take with no answer within 1000 ms returns home (`src-tauri/src/mesh/session.rs`)
- [X] T025 [US2] Implement `src-tauri/src/mesh/edges.rs` (parse, normalise, persist helpers) and add `edges` to `layout.json` in `src-tauri/src/pairstore.rs`, preserving `side`/`along` — satisfies FR-006
- [X] T026 [US2] Add `assign_edge` command (`{side, deviceId}`) with duplicate rejection and state push in `src-tauri/src/commands.rs` — satisfies FR-006
- [X] T026a [US2] Test: pushing into an edge mapped to a disconnected or refusing device crosses nothing, shows the reason once, and does not retry (`src-tauri/src/kvm.rs` tests) — satisfies FR-014, FR-023
- [X] T027 [US2] Send `kvm.take {route, side, edge_at, mouse, keyboard}` and record `entryFraction` when a crossing is decided, in `src-tauri/src/kvm.rs` — satisfies FR-007, FR-008
- [X] T028 [US2] Handle `kvm.accept`, `kvm.refuse`, `kvm.release` inbound in `src-tauri/src/network.rs`: accept drives, refuse returns home and surfaces `lastRefusal`, release goes home — satisfies FR-013, FR-014
- [X] T029 [US2] Add the take timeout (1000 ms) and the "two devices take at once" resolution in `src-tauri/src/mesh/session.rs` + `src-tauri/src/kvm.rs` — satisfies edge case "two devices take"
- [X] T030 [US2] Route keyboard input to the partner session only, with the existing key mapping, in `src-tauri/src/kvm.rs` — satisfies FR-009
- [X] T031 [US2] Keep the dwell and resistance rules identical on all platforms for a mapped edge (reuse `edge_push_ready` / `edge_hold_ready`, no new boundary check) in `src-tauri/src/kvm.rs` — satisfies FR-007, FR-024
- [X] T032 [US2] Detect a driven device's pointer leaving a further mapped edge and send `kvm.next {route, to, side, edge_at}` in `src-tauri/src/kvm.rs` — satisfies FR-012
- [X] T033 [US2] Handle `kvm.next` as the driver: validate sender, `to`, and an existing session; `release` the current partner and `take` the next with the opposite side and mapped offset — satisfies FR-012
- [X] T034 [US2] Never send `kvm.next` to, or accept it from, a phone (OS check) in `src-tauri/src/mesh/session.rs` — satisfies FR-015
- [X] T035 [US2] Return home to `entryFraction` + travel (reuse `seam_return_point`) after a chain in `src-tauri/src/kvm.rs` — satisfies FR-010
- [X] T035a [US2] Local takeover while driven: clear `partner`/`role` locally (`MeshState::local_takeover`), send `kvm.home` naming the route the driver used, and keep the per-session detail the single-peer path already has (all three capture loops in `src-tauri/src/kvm.rs`) — satisfies FR-011
- [X] T036 [P] [US2] Integration test with three in-process fake sessions: take → next → release → unwind, asserting roles and routes at each step (`three_devices_hand_the_pointer_along_a_chain` in `src-tauri/src/state.rs`) — satisfies SC-004
- [X] T037 [P] [US2] JS test: the edge picker shows each device on at most one side and persists (`ui/shared/test/edges-ui.js`)

**Checkpoint**: a three-device chain can be crossed forward and back with no lost pointer and no double handover.

---

## Phase 5: US3 — One clipboard across every device (P1)

- [ ] T038 [P] [US3] Test: hybrid logical clock ordering matches `ui/shared/clipsync.js` scenarios (newer wins, restart past remembered numbers) in `src-tauri/src/mesh/clip.rs`
- [ ] T039 [P] [US3] Test: pool fan-out, dedupe by `(origin, seq)`, relay with hop limit 4, and legacy peers receive plain messages — ported from `ui/shared/test/mesh-pool.js` — in `src-tauri/src/mesh/pool.rs`
- [ ] T040 [US3] Implement `src-tauri/src/mesh/pool.rs` (bounded seen set, 512 entries, hop limit, plain message for legacy) — satisfies FR-016, FR-017, FR-018, FR-020
- [ ] T041 [US3] Implement `src-tauri/src/mesh/clip.rs` (clock, `ts` on publish, newest-wins apply, bounded receipts) and use it from the clipboard watcher in `src-tauri/src/kvm.rs`
- [ ] T042 [US3] Publish local copies to the pool instead of the current single-target send in `src-tauri/src/kvm.rs` (watcher at `~2983`), keeping `clipboard.enabled/send/images/max_kb` gates — satisfies FR-019
- [ ] T043 [US3] Apply received items only when newer, mark them applied by value and by `(origin, seq)` so they are not published back in `src-tauri/src/network.rs` `clip`/`clip.image` handlers — satisfies FR-018
- [ ] T044 [US3] Send `clip.request` on welcome and answer it with the current item in `src-tauri/src/network.rs` — satisfies FR-016
- [ ] T045 [P] [US3] Integration test: three sessions, 100 alternating copies converge to the same newest item with no double apply (`src-tauri/src/mesh/tests/clip.rs`) — satisfies SC-006
- [ ] T046 [US3] Show "copies go to N devices" on the clipboard page when the mesh is active in `ui/ui-clipboard.js` — satisfies FR-019

**Checkpoint**: copy anywhere, paste everywhere, newest wins, no loop, with three devices.

---

## Phase 6: US4 — Pair once, meet everyone (P2)

- [ ] T047 [P] [US4] Test: roster exchange offers a newly paired device to a peer that already knows the receiver; a forgotten device is not reintroduced (`src-tauri/src/mesh/roster.rs`)
- [ ] T048 [US4] Implement `src-tauri/src/mesh/roster.rs` (self/peers/known, `reckon` revisions) and send it on welcome — satisfies FR-021
- [ ] T049 [US4] Dial learned peers once with backoff, skipping devices already connected and devices removed from `pairs.json` in `src-tauri/src/network.rs` — satisfies US4
- [ ] T050 [US4] Add `forget_device` command that removes pairing and any edges pointing at it, and wins over roster updates in `src-tauri/src/commands.rs` — satisfies FR-022

**Checkpoint**: pair the phone with one computer; it appears on the second without a new code.

---

## Phase 7: Android companion — leaf device (part of US1/US2 acceptance)

- [ ] T051 [P] [US2] Test (Kotlin): a take with an older route is ignored; the phone never emits `kvm.next` (`android/app/src/main/java/com/glidedesk/android/Kvm.kt` tests)
- [ ] T052 [US2] Handle `route`, `mouse`, `keyboard` in the phone's take/accept/refuse paths, and take control back when the user touches the screen — satisfies FR-015, FR-024
- [ ] T053 [US3] Apply `ts` dedupe to incoming clipboard items on the phone so it never echoes an item back

---

## Phase 8: Polish and four-platform verification

- [ ] T054 [P] Update `docs/` with a multi-device page describing what the mesh does and what it does not (phones are leaves; monitor map not in this slice)
- [ ] T055 [P] Add `[Mesh]` log lines for session add/remove, take/refuse/release with route and device, and pool apply/duplicate — needed by quickstart triage
- [ ] T056 Run quickstart S0–S7 on macOS and record evidence — satisfies SC-001, SC-003…SC-005, SC-007; time the three-device setup in S1/S5 to check SC-009
- [ ] T057 Run quickstart S1–S4 and S6 on Windows (driven and driving) — satisfies SC-010
- [ ] T058 Run quickstart S1–S4 and S6 on Linux X11, then S2/S4 on Wayland — satisfies SC-010
- [ ] T059 Run quickstart S1–S3 and S6 with the phone as a leaf; confirm it is never asked to relay — satisfies SC-010
- [ ] T060 Legacy check: one device on the previous release connected to a mesh device, no errors, single-peer behaviour (contracts/wire.md rules 1–4) — satisfies SC-008
- [ ] T061 Confirm CI green on macOS, Windows (x64 + ARM64), Linux and Android after the final task — satisfies SC-010

---

## Phase 9 — deferred, not in this slice

- User Story 5 (monitor-level map) is specified but not tasked here. The design must not block it:
  T025 stores `edges` separately from any future per-monitor geometry, and T033 validates a `to`
  device rather than assuming a single neighbour.

## Dependencies

```
T001-T003  →  T004-T013  →  T014-T022 (US1)  →  T023-T037 (US2)  →  T038-T046 (US3)
                                     ↘ T047-T050 (US4, needs the registry only)
                                                        ↘ T051-T053 (phone, needs US2/US3 wire)
                                                                       ↘ T054-T061
```

- US2 and US3 both need the registry and targeting (Phase 2); they do not need each other.
- US4 needs Phase 2 only, so it can run beside US2.
- The phone work needs the wire from US2 and US3.

## Parallel examples

- Phase 2: T004, T005 and T012 touch different files and can be written together.
- US2: T023, T024, T036, T037 are separate files and can run in parallel once T025 lands.
- US3: T038 and T039 are pure-module tests and can be written together; T045 needs both modules.
- Verification: T056–T059 are different machines and can be run in parallel by different people.
