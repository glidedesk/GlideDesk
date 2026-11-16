// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Which device is behind each side of this screen, and where along it a crossing lands.
//!
//! Reference: `shared/kvm-edges.js` (`SIDES`, `normalizeEdges`, `assignEdge`, `edgeForPeer`,
//! `setRemoteEdge`, `clearPeer`, `migrateEdges`). Scenarios: `shared/test/kvm-edges.js`.
//!
//! The rules the port must keep, because they are what makes the picker impossible to break:
//!
//! * A side is either empty or names exactly one device.
//! * A device appears on at most one side; a duplicate keeps its first side in `left, right, top,
//!   bottom` order.
//! * `assign_edge` moves a device that was on another side, and refuses to overwrite a side that
//!   already leads to a different device.
//! * Edges live in `layout.json` next to the existing `side`/`along`, which stay written while
//!   exactly one side is set, so an older build still reads the file it expects.
//!
//! Filled by tasks T023–T026 (`specs/001-multi-device-mesh/tasks.md`).
