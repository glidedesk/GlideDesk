# Implementation Plan: Multi-device mesh — one pointer, one keyboard, one clipboard

**Branch**: `001-multi-device-mesh` | **Date**: 2026-11-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-multi-device-mesh/spec.md`

## Summary

GlideDesk currently runs one peer at a time: several devices can be paired, but the app keeps a
single `client`/`peer`, single-target KVM, and a clipboard that fans out without dedupe. This plan
adds a mesh layer so up to ten devices stay connected, each screen edge leads to a named device
(crossings chain through a driven device and unwind in reverse), and one clipboard is pooled with
"newest copy wins" ordering.

The capability is always loaded and stays dormant until a second device connects, as clarified.
Behaviour with one peer must stay byte-identical to today, including the wire: every added field is
optional and only emitted once the mesh is active.

The reference implementation (`TwinDesk`, v3.2.1, `docs/mesh.md`, `shared/mesh.js`, `*/mesh/kvm.js`)
is the behavioural authority. Its JavaScript core is already vendored here as `ui/shared/mesh.js`,
`clipsync.js`, `kvm-edges.js`, `layout.js` and is covered by 22 passing suites — those stay as the
executable spec, while the runtime implementation becomes native so the existing networking and
state layer stays authoritative.

## Technical Context

**Language/Version**: Rust 2021 (application core), JavaScript (webview UI), Kotlin (Android companion)

**Primary Dependencies**: Tauri 2, tokio, serde/serde_json, rustls (existing); no new runtime dependency is introduced

**Storage**: JSON settings already in use — `kvm.json`, `net.json`, `layout.json`, `pairs.json` in the
platform config directory; no database, no new file format beyond `layout.json` gaining `edges`

**Testing**: `cargo test` (unit + in-process integration), `scripts/test-all.js` (22 JS protocol/mesh
suites), Android companion tests, plus the real-machine matrix in quickstart.md

**Target Platform**: macOS (Intel + Apple silicon), Windows (x64 + ARM64), Linux (X11 and Wayland
helpers), Android companion app — all four proven for this slice

**Project Type**: desktop application (Tauri shell + webview UI) with a phone companion

**Performance Goals**: crossing within 250 ms of the hold expiring; clipboard propagation within 1 s;
pointer motion keeps its current 8–16 ms cadence with no added per-event allocation

**Constraints**: no relay server, no cloud; up to 10 devices; the single-peer path must not change;
mesh traffic must not exceed current idle cost when dormant

**Scale/Scope**: 10 devices, one mesh per person; ~15 Rust modules touched, 1 new module group
(`src-tauri/src/mesh/`), 5 UI files, Android companion handlers

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Gate | Status |
|---|---|---|
| I. Specification & Intent First | Spec exists, clarified, and every task must trace to an FR | PASS — spec.md + Clarifications section |
| II. Test-Driven Verification | Each slice lands with automated checks before it is called done | PASS — plan defines Rust ports of the vendored JS suites plus a 3-session integration test |
| III. Minimal Disruption & Zero Bloat | No new dependency, daemon or background process; respect existing conventions | PASS — native port, no new crates, no sidecar |
| IV. Documented Architecture & Contracts | Data models, API boundaries and file changes mapped in plan.md | PASS — data-model.md, contracts/wire.md, contracts/state-and-ui.md |

Post-design re-check: unchanged. The two intentional duplications (Rust mesh core vs vendored JS
core, and always-on dormancy vs a separate mode) are recorded in Complexity Tracking with reasons.

## Project Structure

### Documentation (this feature)

```text
specs/001-multi-device-mesh/
├── plan.md              # This file
├── research.md          # Phase 0: decisions and rejected alternatives
├── data-model.md        # Phase 1: entities, state transitions, storage
├── quickstart.md        # Phase 1: per-platform verification runbook
├── contracts/
│   ├── wire.md          # Phase 1: message additions, defaults, compatibility
│   └── state-and-ui.md  # Phase 1: FullState fields, commands, UI contract
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src-tauri/src/
├── mesh/
│   ├── mod.rs           # module wiring, activation/dormancy
│   ├── pool.rs          # fan-out, dedupe by (origin, seq), relay, hop limit
│   ├── roster.rs        # who is connected to whom; routing for chains
│   ├── session.rs       # per-device session registry and state
│   ├── clip.rs          # pooled clipboard: HLC, newest-wins, request-on-welcome
│   └── edges.rs         # edge map (side -> device id) and traversal helpers
├── network.rs           # WireMessage additions, targeting, inbound routing
├── kvm.rs               # take/accept/refuse/release/next, route numbers, chain unwind
├── state.rs             # sessions, mesh state, partner instead of clients.first()
├── commands.rs          # edge assignment, session list, remove dead set_multi
└── pairstore.rs         # layout.json edges persistence

ui/
├── ui-multi.js          # Multi-device page: device list, edge assignment, live state
├── shared/edges-ui.js   # NEW: edge picker control (four sides, one device each)
├── ui-cursor.js         # per-device cursor switch labels (partner, not "the other")
└── index.html           # load the new script

android/app/src/main/java/com/glidedesk/android/
└── Kvm.kt               # leaf semantics: route, accept/refuse, never relay

shared (reference only, unchanged):
ui/shared/mesh.js, clipsync.js, kvm-edges.js, layout.js, protocol.js  + ui/shared/test/*
```

**Structure Decision**: single Tauri project (existing layout). The mesh logic lives in a new
`src-tauri/src/mesh/` module group so the dormant/active switch has one obvious seam; `network.rs`
keeps transport and wire concerns; `kvm.rs` keeps capture/injection and gains routing only.

## Phase Plan

### Phase A — Sessions and targeting (prerequisite for everything)

Wire: add optional `route`, `mouse`, `keyboard`, `via`, `to`, `ts` (contracts/wire.md). Session
registry keyed by device id; `partner` replaces `clients.first()`; `send_packet_to`; receiver drops
packets not addressed to it; pooled messages fan out explicitly. Dormancy predicate
`sessions.len() >= 2`. Acceptance: FR-001…FR-005, SC-001, SC-002.

### Phase B — Edges and crossing chains

`mesh/edges.rs` + `layout.json.edges`; crossing sends `kvm.take{route, side, offset, mouse,
keyboard}`; receiver answers `kvm.accept` / `kvm.refuse{why}`; timeout 1 s returns home; driven device
reports `kvm.next{route, to, side, offset}` for a further mapped edge; `kvm.release` unwinds. Phone
never asked to relay. Acceptance: FR-006…FR-015, FR-024, SC-003…SC-005.

### Phase C — Pooled clipboard

`mesh/pool.rs` + `mesh/clip.rs`: HLC ordering, `(origin, seq)` dedupe with a bounded seen set, relay
with hop limit, newest-wins apply, no echo, `clip.request` on welcome. Existing clipboard settings
keep working unchanged. Acceptance: FR-016…FR-020, SC-006, SC-007.

### Phase D — Pair once, meet everyone

Roster exchange on welcome; a device paired with one computer is offered to the others it can reach;
a forgotten device is not reintroduced. Acceptance: FR-022, US4, SC-009.

### Phase E — UI

Multi-device page appears only with two or more sessions; device list shows live role (idle /
driving / driven); edge picker assigns one device per side; refusals and reasons visible. Remove the
dead `set_multi` command and `multi.enabled` flag. Acceptance: FR-002, FR-023.

### Phase F — Android leaf

Honour `route`, answer accept/refuse, support `ts` dedupe, never emit `kvm.next` and never join the
relay path. Acceptance: FR-015, FR-024, SC-010 (phone half).

### Phase G — Four-platform verification

Run quickstart.md on macOS, Windows, Linux (X11 + Wayland) and the phone; record evidence per
scenario; CI must be green on all four build targets.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Mesh core exists twice (Rust runtime + vendored JS reference and suites) | The Rust side owns sockets, state and injection; the JS core is the executable specification the reference implementation is tested against | Deleting the JS core loses the port's oracle; moving networking into the webview would break the security boundary and the existing single-peer path |
| Always-on code with a dormancy predicate instead of a separate mode | Clarified decision: no switch, no restart; activates on the second connection | A separate mode doubles the code paths and cannot activate without restarting, which the person would experience as the app quitting |
| Route numbers and accept/refuse handshake added to a field already carrying `offset` | `offset` is the file-transfer byte offset; the mesh needs a fractional along-edge position and an explicit handover outcome | Reusing `offset` would corrupt file transfer and make the handover ambiguous |
