# Multi-device

Up to ten computers and phones stay connected at once: each screen edge leads to a named device, one
clipboard holds the newest copy made anywhere, and every device is listed in one place. The mesh is
**always loaded and dormant** until a second device connects — there is no switch and no restart. With
one device connected, nothing changes on the wire, in the navigation or in behaviour.

The specification is [specs/001-multi-device-mesh](../specs/001-multi-device-mesh/); the code is
`src-tauri/src/mesh/` (Rust) plus a small part of the phone app. The design notes are
[mesh.md](mesh.md) and [screen-mesh.md](screen-mesh.md); the evidence log is
[mesh-verification.md](mesh-verification.md).

## What it does

1. **Several devices at once.** Connecting a second computer or phone no longer pushes the first one
   away. Every connected device is listed, each with its own row, and disconnecting one leaves the
   others alone. `MAX_DEVICES` is ten; the eleventh is refused with a sentence that says why.
2. **An edge leads to a device.** The Multi-device page assigns a side (left, right, top, bottom) to a
   device. Moving the pointer past that edge sends it there, with the keyboard, and moving back past
   the entry point returns it to where it left by.
3. **A crossing can chain.** While computer A drives B, pushing further through B's own edge hands the
   pointer to C. Coming back unwinds C → B → A in that order, one hold per hop. The phone is the
   exception: it is a leaf (below).
4. **One clipboard pool.** Copying on any device makes the copy pasteable on all of them. Each item
   carries a stamp, so the newest copy wins, an older copy that arrives late cannot replace a newer
   one, and nothing is applied twice. A copy that leaves the desk along one path is not relayed down
   it again by another.
5. **Pair once, meet everyone.** A device paired with one computer is listed on the others and is
   dialled by them as soon as it is paired there. A `mesh.roster` message carries the sender's direct
   links (`peers`) and their names and systems (`members`). The roster is advisory: it never grants
   input or trust — see "What it does not do".
6. **Forgetting wins.** Forgetting a device removes its pairing, its edges and its sessions, and the
   device is not re-added by a roster, not dialled, and not re-learned. A forgotten device that dials
   in is refused.

## What it does not do

- **The phone is a leaf.** A computer can drive the phone, and the phone can take control back (the
  Cursor page shows "Take back control" while it is driven), but the phone is never asked to relay a
  crossing to a third device, and it never sends `kvm.next`. It also never dials a second device: the
  session it is on is the only one it has.
- **The roster is not trust.** `pairs.json` is the only source of pairing trust. A roster names
  devices so they can be listed and so paired screens can be dialled; it cannot make a device trusted,
  cannot grant input, and cannot reintroduce a device the person forgot.
- **No notification pool in this slice.** A notification still shows on the device it was selected
  for. Pooling notifications is a later slice; the design keeps the pooled path (`pool.rs`) general
  enough to carry them.
- **No monitor-level map in this slice.** Edges are assigned per device, not per monitor of a device.
  User Story 5 is specified and tasked separately; the design keeps `edges` apart from any future
  per-monitor geometry so it can land without a migration.
- **No automatic introduction channel.** A paired-but-disconnected device is dialled directly with
  its own token (5 s doubling to a 5 min cap); there is no third-party introduction. This differs
  from the reference's autojoin, deliberately: one dialling rule with one token per pair is easier to
  reason about and to prove.
- **No takeover by bare touch on the phone.** An accessibility service cannot tell the person's finger
  from the computer's own injected click, so the way back is an explicit control. See
  `src-tauri/src/mesh/README.md`.

## Where things live

| | |
|---|---|
| Mesh core (Rust) | `src-tauri/src/mesh/` — `session.rs` (sessions, handover, take timeout), `layout.rs` (edges), `pool.rs` (clipboard pool), `roster.rs` (who is on the desk), `chain.rs` (position through a chain), `mod.rs` (constants, dormancy, refusal text) |
| Wire and sessions | `src-tauri/src/network.rs` (`WireMessage`, authentication per peer, `mesh.roster`, dialling) |
| State and commands | `src-tauri/src/state.rs` (`pairs.json` peers and forgotten, `crossing_target`, takes), `src-tauri/src/commands.rs` (`forget_device`) |
| Page | `ui/ui-multi.js` with the tested view logic in `ui/shared/mesh-view.js`; bridge in `ui/shared/tauri-bridge.js` |
| Phone | `android/app/src/main/java/com/glidedesk/android/MeshLeaf.kt` (leaf rules), `Kvm.kt` (handover), `Clip.kt` (pool ordering) |
| Reference (executable specification) | `ui/shared/mesh.js`, `clipsync.js`, `kvm-edges.js`, `layout.js` and their suites in `ui/shared/test/` |

`pairs.json` keeps the existing `paired` object and adds `peers` (every paired device, each with its
own token) and `forgotten` (ids the person removed). An older build ignores both keys and reads
`paired` exactly as before.

## Wire compatibility

A packet with none of the mesh fields is valid and means today's behaviour. With one session
connected this app emits no new field: `strip_if_dormant` removes `route`, `to`, `mouse`, `keyboard`,
`via`, `reckon`, `why`, `peers` and `members`, and a test asserts the serialised crossing is
byte-identical to the previous release. The one documented exception is `mesh: true` on `hello` and
`welcome`, which is app identity like `os`: the connection that makes the mesh active is the one that
has to announce it.

The rules are in [contracts/wire.md](../specs/001-multi-device-mesh/contracts/wire.md); the page and
commands in [contracts/state-and-ui.md](../specs/001-multi-device-mesh/contracts/state-and-ui.md).

## Checking it

`npm test` runs the Rust engine tests, the JavaScript protocol and mesh suites, and the phone's JVM
tests. The scenarios to run on real machines — three devices connected, a crossing, a chain,
refusals, pair-once, the clipboard pool, each platform as driven and driving — are in
[quickstart.md](../specs/001-multi-device-mesh/quickstart.md), and where each one stands is the table
in [mesh-verification.md](mesh-verification.md).
