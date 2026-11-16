# `src-tauri/src/mesh` — the multi-device mesh, ported from the reference

The mesh lets up to ten devices stay connected at once: each screen edge leads to a named device,
a crossing chains through a driven device and unwinds in reverse, and one clipboard holds the
newest copy made anywhere.

It is **always loaded and dormant** until a second device connects (`mesh_active`). With one
device, nothing here changes the wire, the navigation or the behaviour.

## Why a Rust port when the reference is JavaScript

Sockets, session state, input capture and injection are already Rust (`network.rs`, `state.rs`,
`kvm.rs`). The webview only renders; it cannot see a session, and sending every pointer event
through it would add latency to the thing the product is named for.

The JavaScript core is still vendored at `ui/shared/mesh.js`, `clipsync.js`, `kvm-edges.js` and
`layout.js`, and its suites still run in `npm test`. **It is the executable specification.** If the
Rust and the JavaScript disagree, the Rust is wrong — unless the difference is recorded here as a
deliberate divergence with a reason.

## Module map

| Rust module | Reference | Suites the port must satisfy |
|---|---|---|
| `mod.rs` | `protocol.js` constants (`MAX_DEVICES`, `POOL_SEEN_LIMIT`, `POOL_MAX_HOPS`, `SIDES`, `KVM_TAKE_TIMEOUT_MS`) | `shared/test/mesh-registry.js` (the documented numbers) |
| `session.rs` | `shared/mesh.js` — `Mesh` registry, `addPeer`/`removePeer`/`peers`; `mac/mesh/kvm.js` — `partner`, `peerRoute`, `mode` | `mesh-registry.js`, `mesh-chain.js` |
| `edges.rs` | `shared/kvm-edges.js` — all of it | `kvm-edges.js` |
| `pool.rs` | `shared/mesh.js` — `Pool` (`publish`, `receive`, seen set, relay) | `mesh-pool.js` |
| `roster.rs` | `shared/mesh.js` — `Roster`, `Mesh.rosterMessage`, `Mesh.known` | `mesh-roster.js` |
| `clip.rs` | `shared/clipsync.js` — `Clock`, `compare`, `Latest` | `clipsync.js`, `clipimage.js` |
| `chain.rs` | `mac/mesh/kvm.js` — `_armReturn`/`_onReturn` (the driven device noticing its pointer left the screen) | `mesh-chain.js` |

`ui/shared/layout.js` (the shared super-grid) is not ported in this slice. Edge assignment uses
`edges.rs` only; the monitor-level map is a later slice, and `edges` is stored separately from any
future monitor geometry so it is not blocked.

## Deliberate divergences from the reference wire

Recorded because a reader comparing the two will find them, and they are choices, not drift.

| Reference | Here | Why |
|---|---|---|
| `kvm.take`/`kvm.next` carry the along-edge position as `offset` (a fraction) | `edge_at` | `WireMessage.offset` is already the file-transfer byte offset. Reusing it would corrupt file transfer. `edge_at` already exists for the same purpose in `kvm.take` today. |
| Roster message is `mesh.roster {peers, members}` | Same names kept | No conflict; the message type is new to this app. |
| `MultiState.enabled` switch in `multi.json` | Removed: dormancy by session count | Clarified with the person: no switch, no restart, activates on the second connection. `set_multi` wrote a flag nothing read. |
| The driven device learns its pointer left an edge from the native helper's edge watch (`_armReturn`) | `chain.rs` tracks the pointer from the motion it injects | `cursor_position()` returns `None` on Linux, so there is no portable way to ask where the pointer is. Arithmetic on the deltas also makes the rule testable without a desktop session. |
| A chain restarts the driven side's travel from the new entry point by the helper's own bookkeeping | `kvm::begin_remote_handover` sets a restart flag each capture loop consumes on its own thread | The Windows and macOS hooks own statics, the Linux loop its locals; writing another thread's travel counters mid-event is the race this avoids. |

## Where the code goes

| Concern | File |
|---|---|
| Wire fields, targeting, inbound routing, receive-side validation | `src-tauri/src/network.rs` |
| Session registry, `mesh` state payload, partner resolution | `src-tauri/src/state.rs` |
| Crossing decision, take/accept/refuse/release/next, injection | `src-tauri/src/kvm.rs` |
| `assign_edge`, `disconnect_device`, `forget_device` | `src-tauri/src/commands.rs` |
| `edges` persistence in `layout.json` | `src-tauri/src/pairstore.rs` |
| Device list, edge picker, refusals | `ui/ui-multi.js`, `ui/shared/edges-ui.js` |
| What the page shows, as tested pure functions | `ui/shared/mesh-view.js` (`ui/shared/test/ui-multi.js`) |

## Reading order for a change

1. `specs/001-multi-device-mesh/spec.md` — what the person must be able to do (FR-001…FR-024).
2. `specs/001-multi-device-mesh/contracts/wire.md` — the only thing that may go on the wire.
3. `docs/mesh.md` — the reference's own design notes.
4. The reference module listed above, then its test, then the Rust.
