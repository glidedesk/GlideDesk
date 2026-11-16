# Data Model: Multi-device mesh

**Status**: Phase 1 design. Storage is JSON on the device that owns it; nothing is copied to other
devices except what the wire contract allows.

## Session

One other device this app is connected to.

| Field | Type | Notes |
|---|---|---|
| `deviceId` | string | stable, from pairing; key of the registry |
| `name` | string | shown in the device list |
| `os` | string | `mac` \| `win` \| `linux` \| `android` |
| `address` | string | reachable host:port, for display and re-dial |
| `mesh` | bool | the peer speaks multi-device (false = legacy single peer) |
| `pool` | bool | the peer accepts pooled items (clipboard) |
| `role` | enum | `idle` \| `driving` \| `driven` |
| `route` | u64 | last handover route agreed with this peer |
| `connectedAt` | u64 | ms since epoch |
| `lastSeen` | u64 | ms since epoch, for staleness and ping cadence |

**Invariants**

- At most one session may have `role = driven` (this machine is driven by one device).
- At most one session may have `role = driving` (this machine drives one device).
- `route` never decreases for a given session; a take with `route <= session.route` is ignored.
- A session removed from the registry releases any role it held and returns this machine to `home`.

**Lifecycle**: `dialling → welcome → connected → (driving | driven)* → closed`, with `closed →
dialling` allowed by reconnect with backoff. A legacy peer never leaves `connected` + `idle` for the
mesh purposes; it can still be driven one-to-one.

## EdgeMap

Which device is behind each side of this screen.

| Field | Type | Notes |
|---|---|---|
| `left` / `right` / `top` / `bottom` | string \| null | device id, unique across sides |

**Invariants**

- One device appears on at most one side (the first side in `left, right, top, bottom` order wins on
  read, as `ui/shared/kvm-edges.js` does).
- An id that is not a known session is kept (it may simply be offline) but is not traversable; a
  crossing attempt towards it is refused with `not_connected`.
- Persisted in `layout.json` alongside the existing `side`/`along`; see contracts/state-and-ui.md.

## Layout (unchanged, extended)

| Field | Type | Notes |
|---|---|---|
| `side` | string | today's field: the edge the one peer is on |
| `along` | f64 | today's offset |
| `edges` | EdgeMap | new, optional; absent in files written by older builds |

**Read rule**: if `edges` is absent or empty, derive it from `side` (single mapping) so an existing
installation keeps working. If present, `side`/`along` are still written for older builds when exactly
one edge is set.

## Handover (transient, per crossing)

| Field | Type | Notes |
|---|---|---|
| `route` | u64 | this crossing's number, monotonic per machine |
| `side` | string | the side that was left |
| `offset` | f64 | 0..1 along that side, where the hand left |
| `mouse`, `keyboard` | bool | what the taker asks for; `mouse = false` is keyboard-only |
| `via` | string? | a device asked to drive a third one on the taker's behalf (not used for phones) |
| `entryFraction` | f64 | recorded locally, used to come home to the same seam spot |
| `travelX`, `travelY` | f64 | accumulated since the handover, used to carry the return |

**State transitions** on the driving machine: `home --take sent--> taking --accept--> driving`;
`taking --refuse|timeout--> home`; `driving --release|next|hand travel--> home`. On the driven
machine: `home --take accepted--> driven`; `driven --release--> home`; `driven --own input--> home`
(and the driver is told). Every transition is idempotent for a given `route`.

## PoolItem (clipboard)

| Field | Type | Notes |
|---|---|---|
| `kind` | enum | `text` \| `image` |
| `text` | string? | for `kind = text` |
| `png` | bytes? | for `kind = image` |
| `ts` | u64 | hybrid logical clock value at the origin |
| `origin` | string | device id that copied it |
| `seq` | u64 | origin's sequence number, for dedupe |

**Invariants**

- A device applies an item only if `ts > lastAppliedTs` for that pool.
- An item is applied at most once per `(origin, seq)`; receipts are bounded (512, oldest evicted).
- A received item is never published again; the watcher recognises it by value and by `(origin, seq)`.
- Items exceeding the configured size limit, or marked concealed at the origin, are never published.
- An item without `ts` from an older peer is applied on arrival and treated as `ts = now`.

## Roster

Who is connected to whom, as this device currently believes.

| Field | Type | Notes |
|---|---|---|
| `self` | string | this device id |
| `peers` | map<deviceId, {connected: bool, to: [deviceId]}> | per-device view |
| `known` | [deviceId] | paired devices, for join-once and re-dial |

**Invariants**

- The roster is advisory: a crossing is only attempted towards a device this machine has a session
  with, or that its partner reports and can reach.
- A device the person forgot is not reintroduced by a roster update.

## Persistence summary

| File | Owner | Gains |
|---|---|---|
| `layout.json` | this device | `edges` (device id per side) |
| `pairs.json` | this device | unchanged; still the single source of pairing trust |
| `kvm.json` | this device | unchanged (`speed`, `resistanceMs`, `reverseScroll`, `gestures`) |
| `net.json` | this device | unchanged |

No mesh setting lives in another device's file, and no file is written by a peer.
