// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! The pooled clipboard: the latest copy wins, on every device, whichever device made it.
//!
//! Reference: `shared/clipsync.js` (`Clock`, `compare`, `Latest`). Scenarios:
//! `shared/test/clipsync.js`, `shared/test/clipimage.js`.
//!
//! Every copy gets a time `{ts, o}`: `ts` from a hybrid logical clock (milliseconds that never go
//! backwards and always move past any time seen from another device), `o` the device that copied.
//! A device applies an incoming item only when it is newer than the newest copy it knows of, so a
//! late or repeated push of an older copy cannot replace a newer one — plain wall clocks disagree
//! by seconds between machines, which is exactly the bug this avoids.
//!
//! An item from an older app that sends no time counts as copied on arrival, as it does today.
//! The clock is clamped so a device whose clock is far in the future cannot win for ever
//! (`CLIP_SKEW_MS`).
//!
//! Filled by tasks T038–T045 (`specs/001-multi-device-mesh/tasks.md`).
