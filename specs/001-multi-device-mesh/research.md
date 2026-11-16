# Research: Multi-device mesh

**Status**: Complete — no `NEEDS CLARIFICATION` remains in the plan.

The reference implementation (TwinDesk v3.2.1) and this repository's current state were read before
each decision. Evidence below uses `path:line` from this repository unless it names TwinDesk.

## D1. Where the mesh core runs: native, not the webview

**Decision**: implement `Pool`, `Roster`, sessions and pooled clipboard in Rust under
`src-tauri/src/mesh/`; keep the vendored JavaScript core (`ui/shared/mesh.js`, `clipsync.js`,
`kvm-edges.js`, `layout.js`) as the executable reference, unchanged and still tested.

**Rationale**: sockets, state, capture and injection are already Rust (`network.rs`, `kvm.rs`,
`state.rs`). The webview only renders; it cannot see a session, and routing input through it would
add latency to every pointer event. The JS core is small and pure, which makes it a good oracle: its
22 suites (`ui/shared/test/`) already encode the pool/roster/chain rules and keep passing in
`npm test`.

**Rejected**: a Node sidecar (new runtime, packaging and signing cost, violates zero bloat); moving
networking into the webview (breaks the existing security boundary and the dormant single-peer path).

**Consequence**: the same rules are written twice, so the Rust port must be verified against the JS
suites' scenarios (tasks carry a 1:1 mapping) and any divergence is a bug, not a design choice.

## D2. Dormancy instead of a mode switch

**Decision**: the mesh is always loaded. It is *dormant* while fewer than two sessions are
connected: no additional fields on outgoing packets, no `kvm.next`/`kvm.release`, no Multi-device
navigation, unchanged behaviour. It activates on the second session.

**Rationale**: clarified by the person; also the only shape that avoids a restart. The existing
transport already supports several sessions internally (`active_clients` is a
`HashMap<String, ActiveClientSession>`, `network.rs:1125`) — what is missing is state and routing,
not sockets.

**Consequence**: `Partner` resolution must be explicit everywhere `clients.first()` is used today
(`state.rs:2083`), and every new field is gated: `Some(...)` only when `sessions.len() >= 2`.

## D3. Targeting is explicit; broadcast is opt-in

**Decision**: add `send_packet_to(device_id, pkt)`; make fan-out explicit for pooled messages
(`clip`, `clip.image`, roster/presence). Addressed messages (`kvm.*`, files, terminal, apps, media)
go to one session. The receiver validates that the packet is addressed to it and drops it otherwise.

**Rationale**: `send_packet_sync` currently fills `d` with *every* connected client
(`network.rs:4160-4171`) and the receiver never checks it. With one peer that is invisible; with
three, a `kvm.take` would be accepted by every device with "be driven" on. This is a real defect the
moment a third device connects, independent of the mesh work.

**Consequence**: a compatibility risk only for packets that today rely on implicit fan-out; each
such message type is listed in `contracts/wire.md` with its intended destination set, and the
single-peer path is unchanged because the only session is also the target.

## D4. Handover routes

**Decision**: each handover carries a monotonic `route: u64` chosen by the driving device. `take`,
`accept`, `refuse` and `release` echo it; a driven device ignores a take with a route at or below the
one it already holds; a take with no answer within 1000 ms returns home.

**Rationale**: the reference uses exactly this and it removes the class of bug where two devices each
believe they drive (or a late packet re-drives a device that already went home). Without a route
number, a delayed `take` after a `release` is indistinguishable from a fresh crossing.

**Consequence**: `state.input` alone is no longer enough; each session carries its role and route.

## D5. Edge map storage

**Decision**: `layout.json` gains `edges: {left, right, top, bottom}` (device ids, `null` allowed).
The existing `side` + `along` stay and are used when exactly one device is mapped, so an older build
reading the file still finds what it expects.

**Rationale**: `layout.json` is already the placement file (`pairstore.rs:105`, `state.rs:46`), and
`side`/`along` remain meaningful in the two-device case. A separate `multi.json` would split one
concept (where screens are) across two files.

**Rejected**: writing only `edges` and dropping `side`/`along` (breaks older builds and the current
one-peer path).

## D6. "Newest copy wins" ordering

**Decision**: hybrid logical clock in Rust, mirroring `ui/shared/clipsync.js`. Each published item
carries `ts` (HLC value) and `o` (origin id). A device applies an incoming item only when its `ts` is
greater than the newest it has applied; an item without `ts` (an older peer) counts as arrival time,
as today.

**Rationale**: wall clocks disagree by seconds between machines, so plain timestamps let an older
copy overwrite a newer one. HLC keeps the person's ordering (a copy made after receiving one always
sorts later) without assuming synchronised clocks.

**Consequence**: the clipboard watcher's echo suppression must key on `(o, seq)` and on the applied
value, not only on the last received string (today `LAST_REMOTE_TEXT`, `kvm.rs:2854`).

## D7. Dedupe memory stays bounded

**Decision**: pool receipts are kept as a bounded set of `(origin, seq)` (capacity 512 per pool,
oldest evicted) plus the per-origin highest sequence seen; the hop limit is 4.

**Rationale**: a mesh relays, so a device can legitimately see the same item by several paths; an
unbounded set is a slow leak on a long-running desktop app.

## D8. Pairing: meet the rest without a second code

**Decision**: on welcome, exchange the roster of paired devices; a device dials every peer it learns
about that is reachable and not connected, once, with backoff, and never reintroduces a device the
person forgot.

**Rationale**: required by US4 and in scope for this slice; the pairing store already distinguishes
"paired" from "connected", so the exchange can reuse it.

**Consequence**: connection count and idle keep-alive now scale with peers; ping cadence must differ
for active (driving/driven) versus idle sessions, as the reference does.

## D9. Phones are leaves

**Decision**: the Android companion handles `take`/`accept`/`refuse` with `route`, applies `ts`
dedupe, and never emits `kvm.next`, never relays pooled items, and never appears in a chain.

**Rationale**: clarified by the person; the phone app talks to one computer at a time today, so leaf
semantics are already true — they must be enforced rather than assumed, because a mesh device can now
ask it to do more.

## D10. Verification

**Decision**: three layers — (a) Rust unit tests ported from the JS suites' scenarios; (b) an
in-process integration test with three fake sessions over a channel, exercising take → next →
release and clipboard convergence; (c) the real matrix in `quickstart.md` on macOS, Windows, Linux
(X11 and Wayland) and the phone.

**Rationale**: the defect class here (two devices both believing they drive) is a state-machine bug
that unit tests can pin exactly, while platform parity (hold, resistance, injection) needs real
hardware — the six platform differences just fixed in `kvm.rs` are the proof.

## Open items carried into tasks

- Whether idle sessions need a distinct ping cadence in this slice or can keep today's single
  cadence (measure first; change only if it costs battery or traffic).
- Where the Multi-device page lives in navigation when dormant (hidden entirely, per clarification).
