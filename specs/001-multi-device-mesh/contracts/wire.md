# Contract: wire

**Status**: Phase 1 design. Everything here is additive. A packet without these fields must behave
exactly as it does today, and a device that does not know a field must ignore it.

## Fields added to `WireMessage`

All are `#[serde(default)]` unless stated, and (except `mesh`, which is app identity) are only
emitted when the mesh is active (two or more sessions). Names follow the reference implementation so
the two can be compared field by field.

| Field | Type | Used by | Meaning | Default when absent |
|---|---|---|---|---|
| `route` | u64 | `kvm.take` `kvm.accept` `kvm.refuse` `kvm.release` `kvm.next` | this handover's number | 0 — treated as the oldest possible, so a route-less take behaves as today |
| `to` | string | `kvm.next`, `roster` | the device the crossing or roster entry is about | absent |
| `mouse` | bool | `kvm.take` | the taker wants the pointer, not only the keyboard | `true` |
| `keyboard` | bool | `kvm.take` | the taker wants keys too | `true` |
| `via` | string | `kvm.take` | the taker asks the receiver to drive a third device on its behalf | absent |
| `ts` | u64 | `clip` `clip.image` | hybrid logical clock value at the origin | absent — item counts as copied on arrival |
| `reckon` | u64 | `welcome`, `mesh.roster` | the peer's roster revision, so stale updates are ignored | 0 |
| `why` | string | `kvm.refuse` | `not_allowed` \| `busy` \| `not_connected` \| `not_ready` | absent |
| `mesh` | bool | `hello`, `welcome` | the sender speaks the mesh: it understands pooled stamps and may be asked to relay | absent — treated as an older app: plain messages, never a relay |

`side` and a fractional along-edge position already exist as `side: Option<String>` and
`edge_at: Option<f64>` (used by `kvm.take` today); they are kept as-is. The reference carries this
position as `offset` in `kvm.take`/`kvm.next`, but `WireMessage.offset` here is the file-transfer byte
offset (`Option<u64>`), so **`edge_at` is used instead**. Reusing `offset` would corrupt file transfer;
this is a recorded divergence (`src-tauri/src/mesh/README.md`).

`o` (origin), `s` (sequence), `d` (direct links at publish) and `h` (hops) already exist
(`network.rs:379-389`) and become the pool stamps. `s` is the origin's own counter; it must survive a
restart as instructed by the reference (start past the wall clock in ms), otherwise a restarted app
publishes duplicates.

## Destination sets

Explicit, because `send_packet_sync` currently addresses every connected client (`network.rs:4160`).

| Message | Destination | On receive, drop unless |
|---|---|---|
| `clip`, `clip.image` | every session with `pool = true`, then relayed by hops | **not** address-filtered: `d` is the origin's link list for dedupe and relay, and a relayed item deliberately names a set that does not include the relay's recipient |
| `mesh.roster` | every session | **not** address-filtered (it is pooled metadata) |
| `kvm.take` `kvm.accept` `kvm.refuse` `kvm.release` `kvm.next` | the one session involved | addressed to this device and `route` is newer than the session's |
| `k.mm` `k.md` `k.mu` `k.wh` `k.kd` `k.ku` | the driven session only | addressed to this device (`d`) **and** this device is `driven` by the sender |
| files, terminal, apps, media, hotkeys, wake | the selected session only | addressed to this device (unchanged behaviour, now enforced) |

Rule of thumb: a message is either **pooled** (fan-out, dedupe, relay) or **addressed** (one session,
validated). There is no third kind.

## Flows

### Crossing to a mapped neighbour

```json
// driver -> laptop
{"t":"kvm.take","v":1,"route":7,"side":"right","edge_at":0.42,"mouse":true,"keyboard":true}
// laptop -> driver
{"t":"kvm.accept","v":1,"route":7}
// or
{"t":"kvm.refuse","v":1,"route":7,"why":"not_allowed"}
```

`accept` is final. `refuse` (or 1000 ms without either) returns the driver home without changing its
pointer's travel record, and the refusal reason is shown (`not_allowed`, `busy`, `not_connected`).

### Chaining through a driven device

```json
// laptop (driven) sees its pointer leave the side mapped to the phone
{"t":"kvm.next","v":1,"route":7,"to":"phone-1","side":"right","edge_at":0.5}
// driver, on receiving it, releases the laptop and takes the phone
{"t":"kvm.release","v":1,"route":8}
{"t":"kvm.take","v":1,"route":9,"side":"left","edge_at":0.5}
```

A `kvm.next` is only honoured from the session currently driven, only for a `to` the driver has a
session with, and never when the sender is a phone.

### Clipboard

```json
{"t":"clip","v":1,"text":"hello","ts":1737027020123,"o":"mac-1","s":41,"d":["win-1","linux-1"],"h":0}
// relaying device forwards the same item, one hop further, to peers not in `d`
{"t":"clip","v":1,"text":"hello","ts":1737027020123,"o":"mac-1","s":41,"d":["mac-1","linux-1","phone-1"],"h":1}
```

- Apply only if `ts` is newer than the newest applied; never re-publish an item that arrived.
- A new device asks for the current item on welcome: `{"t":"clip.request","v":1}`; the answer is an
  ordinary `clip` with its original `ts`/`o`/`s`.

### Roster

The reference's shape, kept exactly: the message type is `mesh.roster`, `peers` is this device's
direct links, and `members` names them so a device that is not linked to them can still list them.

```json
{"t":"mesh.roster","v":1,"reckon":12,"o":"mac-1",
 "peers":["win-1","linux-1"],
 "members":[{"id":"win-1","name":"Desktop","os":"win"},{"id":"linux-1","name":"Box","os":"linux"}]}
```

Advisory only; a crossing still requires a session with the device, or a `kvm.next` from a partner
that can reach it. `pairs.json` decides trust: a device the person forgot is never reintroduced.

## Compatibility rules (must hold)

1. A packet with none of these fields is valid and means today's behaviour.
2. With one session connected, this app emits no new field: the serialised JSON for a crossing is
   identical to the current build's (asserted by a test). The one exception is `mesh` on
   `hello`/`welcome`, which is app identity like `os` or `fx`, not mesh state: the connection that
   makes the mesh active is the one that has to announce it, so it cannot be gated on dormancy. An
   older app ignores it and stays `mesh: false` here.
3. Unknown fields are ignored, never rejected (`serde` default behaviour, plus a test that feeds a
   packet with extra keys).
4. A legacy peer is never sent pooled items it did not advertise (`pool = false`) and is never asked
   to relay.
5. Removing a device from `pairs.json` wins over any roster update: the device is not reconnected by
   another peer's recommendation.
6. The `d` stamp now carries **device ids** rather than the sender's session addresses
   (`<ip>:<port>`). The receiver compares them with its own id. The old spelling was already
   documented as a bug for files (`network.rs`, `resolve_file_dest` tests) and is still *accepted* on
   receive, so a device running an older build keeps working (`SC-008`).
