// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Who is connected to whom, as this device currently believes, so a crossing can chain.
//!
//! Reference: `shared/mesh.js` (`Roster`, `Mesh.rosterMessage`, `Mesh.known`). The message is
//! `mesh.roster` with `peers` (this device's direct links) and `members` (`{id, name, os}`), which
//! is what lets a device that is not linked to them still list them ("pair once, meet everyone").
//!
//! The roster is advisory: a crossing is only attempted towards a device this machine has a session
//! with, or that its partner reports and can reach. A device the person forgot is never
//! reintroduced by a roster update — `pairs.json` decides trust, the roster only decides routing.
//!
//! Filled by tasks T047–T050 (`specs/001-multi-device-mesh/tasks.md`).
