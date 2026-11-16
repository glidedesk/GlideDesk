// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Pooled messages: fan-out, dedupe by `(origin, seq)` and relay, for items meant for everyone.
//!
//! Reference: `shared/mesh.js` (`Pool`), constants `POOL_SEEN_LIMIT` / `POOL_MAX_HOPS`. Scenarios:
//! `shared/test/mesh-pool.js`.
//!
//! Two kinds of message exist on the wire and there is no third (`contracts/wire.md`):
//!
//! * **Pooled** — clipboard text and pictures, roster. Sent to every session that accepts pool
//!   items, deduplicated, relayed one hop further with a bounded hop count.
//! * **Addressed** — one session only, validated on receive.
//!
//! A pooled message carries the origin's stamps in the fields that already exist on `WireMessage`:
//! `o` (origin), `s` (origin sequence), `d` (the origin's direct links when it published) and `h`
//! (hops). Those are already on the wire, so an older app sees a message it ignores.
//!
//! A receiver never publishes an item it received, applies it at most once, and never sends it back
//! to the peer it came from, the origin, or one of the origin's direct links.
//!
//! Filled by tasks T039–T040 (`specs/001-multi-device-mesh/tasks.md`).
