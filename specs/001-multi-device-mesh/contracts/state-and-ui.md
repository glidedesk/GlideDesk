# Contract: state, commands and UI

**Status**: Phase 1 design.

## State (`state.rs`, serialised in `state-change`)

Added to `FullState`:

```
"mesh": {
  "active": false,            // true once two or more sessions exist
  "partner": null,            // device id this machine drives or is driven by
  "role": "home",             // "home" | "driving" | "driven"  (mirrors kvm mode)
  "route": 0,                 // last handover route on this machine
  "lastRefusal": null,        // { device, why, at }
  "reckon": 12,               // roster revision; bumped whenever this device's links change
  "knownLinks": [],           // devices learned through peers (mesh.roster `members`)
  "known": [                  // everyone this device knows of, direct or not
    { "id":"phone-1", "name":"Phone", "os":"android", "direct":false, "via":["win-1"] }
  ],
  "sessions": [               // one entry per connected device
    { "id":"win-1", "name":"Desktop", "os":"win", "address":"192.168.4.11:47800",
      "mesh":true, "pool":true, "role":"idle", "route":0,
      "connectedAt":1737027000000, "lastSeen":1737027012000 }
  ]
}
```

Compatibility: `client`, `peer` and `clients` stay exactly as they are today, because the existing UI
reads them. They become *derived* views:

- `client` / `peer` = the partner session if one exists, else the single connected session, else
  `null` (today: `clients.first()`).
- `clients` = all connected sessions, unchanged shape.

`input` stays the machine-level mode (`home` / `driving` / `driven`) so every existing consumer keeps
working; `mesh.partner` says **which** device that mode is with.

## Dormancy contract

| Condition | Wire | Navigation | Behaviour |
|---|---|---|---|
| 0 sessions | unchanged | no Multi-device page | unchanged |
| 1 session | no new fields emitted | no Multi-device page | identical to today |
| 2+ sessions | mesh fields allowed; pooled messages fan out | page visible | pointer, keyboard and clipboard span devices |

Activation and deactivation are ordinary state transitions: emit `state-change`, no restart, no
reconnect, and the existing session is not disturbed.

## Commands (Tauri)

| Command | Payload | Effect | Notes |
|---|---|---|---|
| `assign_edge` | `{side, deviceId}` (`deviceId: null` clears) | writes `layout.json.edges` | refuses an id already on another side with a clear error; pushes state |
| `sessions_list` | – | returns `mesh.sessions` | for the page and tests |
| `disconnect_device` | `{deviceId}` | closes that session only | other sessions unaffected |
| `forget_device` | `{deviceId}` | removes pairing and edges pointing at it, and puts the id on the forgotten list | wins over roster; also refuses a reconnect |
| `clip_request` | `{deviceId}` | asks a peer for its current clipboard item | **not built in this slice**: the request goes out on connect, and there is no per-device "paste from" control yet. Adding the command without the control would be a dead switch |

Removed: `set_multi` and `MultiState.enabled` (`commands.rs:1239`, `state.rs:1140`). The dead flag
must not survive this work; nothing reads it, and leaving it invites a switch that does nothing.

## UI contract

**Multi-device page (`ui/ui-multi.js`)**

- Hidden from navigation while `mesh.sessions.length < 2` (clarified behaviour); appears on the
  second connection without a reload, because the page reacts to `state-change`.
- Shows each session with name, OS, address and live role (`idle` / `driving` / `driven`), and a
  disconnect action per row.
- Edge assignment: four sides, each a picker over connected devices plus "None". A device chosen on
  one side disappears from the others' lists. Changing an assignment takes effect immediately.
- Shows `mesh.lastRefusal` when a push into an edge was refused, with the reason in words a person
  understands ("Desktop is not connected", "Cursor sharing is off there").
- "Also on this desk": the devices in `mesh.known` that have no session (a phone paired with one
  computer and reported by its roster, `FR-021`), each with the computer that reported it and a
  **Forget** button wired to `forget_device`. A device with a session belongs to "Connected now"
  and is not repeated.
- No dead controls: every switch on this page must be wired to a command that changes behaviour
  (this is the rule that `set_multi` broke).

**Existing pages**

- Cursor page (`ui/ui-cursor.js`): the "other computer" wording becomes the partner's name when a
  partner exists; with one device the wording is unchanged.
- Clipboard page: unchanged controls; gains a line naming how many devices receive copies when the
  mesh is active.

## Tests this contract implies

1. Serialising a crossing with one session yields the same JSON as before this feature (golden test).
2. A packet carrying unknown keys deserialises and is handled.
3. `assign_edge` rejects a duplicate device on two sides and persists the rest.
4. Removing a session clears `mesh.role`/`mesh.partner` and returns `input` to `home`.
5. The Multi-device page is absent with one session and present with two, driven only by state.
