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
| A `clip.request` is answered by re-reading the clipboard and publishing it as a fresh copy (`sendIfChanged('request')`) | The answer is the item as it was last published, with its original `ts`/`o`/`s` | A fresh copy would make an old item beat a newer one made elsewhere while the request was in flight. `contracts/wire.md` specifies the original stamp, and the requester's newest-wins rule then does the right thing. |
| A roster is how trust travels: the introducer asks a host for a credential and forwards the grant (`shared/intro.js`, `mac/mesh/autojoin.js`), so a guest is paired without a code | The roster is advisory: it lists devices and tells this app which paired screens to dial; trust never arrives in a `mesh.roster` | `pairs.json` must stay the only source of pairing trust (`FR-022`), and a forgotten device must stay forgotten. A credential channel would need its own replay protection, rate limiting and tests, and it is not needed for the acceptance case: the phone is already paired with one computer, and the other computer dials it as a paired device. |
| `Roster` has no revision; a later message simply replaces an earlier one | The message carries `reckon`, and an older revision from the same device is ignored (`mesh/roster.rs`) | Two devices on a slow Wi-Fi link can deliver a roster out of order; without a revision the newer link list could be overwritten by the older one. |
| `Mesh.known()` lists registered peers plus the `members` a peer sent | Also lists ids a peer named in `peers` without a `members` entry | A peer that sends only ids still introduces its devices; an id with no name is better than a device the person was never told about. |
| Re-dialling is the introducer's job (the plan()/ASK cycle) | This app dials the devices it is already paired with, once when a roster arrives, with a backoff (5 s doubling, capped at 5 min) and never one on the forgotten list | Same outcome (the desk comes back together) using only what `pairs.json` trusts, and without a new credential exchange. |
| The phone is driven by the computer's own edge watch, which sends `kvm.home` when the pointer comes back over the edge (`mac/mesh/phone-bridge.js`); the phone itself has no takeover | The phone is a leaf that answers `kvm.take` with `kvm.accept`/`kvm.refuse` and has an explicit "Take back control" while it is driven (`android/.../MeshLeaf.kt`, `Kvm.takeBack`) | A bare touch cannot tell a person's finger from the computer's own injected click: an accessibility service sees both as a touch interaction, and a false take-back would end the handover on the click the computer was making. An explicit control cannot misfire, works with either control method (ADB or Accessibility), and is what the phone's tests cover. |
| The reference has no phone-side clipboard ordering (the phone is a plain two-device peer) | The phone applies the pool's `ts` and drops an item that is older than, or the same as, the newest copy it applied (`MeshLeaf.clipIsNewer`) | A phone on a desk of three or more can receive the same copy by two paths, and an older copy after a newer one; FR-017 and FR-018 apply to it as much as to a computer. A copy with no stamp is applied as before. |
| A peer's mesh capability arrives as `mesh: 1` on `hello`/`welcome` | Same field name, `mesh: true`, on the same two messages | Needed for the connection that *makes* the mesh active to say so: gating it on dormancy would leave the second device looking like an older app for ever. Everything else stays additive and unknown fields are ignored. |

## Where the code goes

| Concern | File |
|---|---|
| Wire fields, targeting, inbound routing, receive-side validation | `src-tauri/src/network.rs` |
| Session registry, `mesh` state payload, partner resolution | `src-tauri/src/state.rs` |
| Crossing decision, take/accept/refuse/release/next, injection | `src-tauri/src/kvm.rs` |
| `assign_edge`, `disconnect_device`, `forget_device` | `src-tauri/src/commands.rs` |
| `pairs.json` `peers` and `forgotten` | `src-tauri/src/pairstore.rs` |
| `edges` persistence in `layout.json` | `src-tauri/src/pairstore.rs` |
| The pooled clipboard: fan-out, newest-wins apply, `clip.request` | `src-tauri/src/network.rs` (`MeshedClipboard`), `src-tauri/src/mesh/pool.rs`, `clip.rs` |
| Device list, edge picker, refusals, "also on this desk" | `ui/ui-multi.js`, `ui/shared/edges-ui.js` |
| Roster exchange, per-device pairing tokens, dialling | `src-tauri/src/mesh/roster.rs`, `src-tauri/src/state.rs`, `src-tauri/src/network.rs` |
| What the page shows, as tested pure functions | `ui/shared/mesh-view.js` (`ui/shared/test/ui-multi.js`) |
| The phone as a leaf: handover state, accept/refuse, take-back, pooled-copy ordering | `android/app/src/main/java/com/glidedesk/android/MeshLeaf.kt` (`Kvm.kt`, `Clip.kt`; tests in `android/app/src/test/.../MeshLeafTest.kt`) |

## Reading order for a change

1. `specs/001-multi-device-mesh/spec.md` — what the person must be able to do (FR-001…FR-024).
2. `specs/001-multi-device-mesh/contracts/wire.md` — the only thing that may go on the wire.
3. `docs/mesh.md` — the reference's own design notes.
4. The reference module listed above, then its test, then the Rust.
