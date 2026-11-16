// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Multi-device mesh: several devices on one desk, one pointer, one keyboard, one clipboard.
//!
//! The design is in `docs/mesh.md`; `src-tauri/src/mesh/README.md` maps every module here to the
//! JavaScript reference it was ported from. Read that before changing anything.
//!
//! The mesh is always loaded and **dormant** while fewer than two devices are connected: with one
//! device nothing here changes the wire, the navigation or the behaviour, and the app is the
//! two-device app it was before. It activates on the second connection (`mesh_active`).
//!
//! Vocabulary, kept the same as the reference so the two can be read side by side:
//!
//! * **Session** — one other device this app is connected to (`session.rs`).
//! * **Edge** — which device is behind each side of this screen (`edges.rs`).
//! * **Pool** — fan-out, dedupe and relay of items meant for everyone (`pool.rs`).
//! * **Clipboard** — the pooled clipboard: newest copy wins (`clip.rs`).
//! * **Roster** — who is connected to whom, so a crossing can chain (`roster.rs`).

pub mod clip;
pub mod edges;
pub mod pool;
pub mod roster;
pub mod session;

pub use session::{MeshSession, MeshState, Role};

/// Most devices that can be connected at once, this one included in the count of the others.
/// The reference promises ten (`protocol.js`: `MAX_DEVICES`), and `docs/mesh.md` documents it.
pub const MAX_DEVICES: usize = 10;

/// How many `(origin, seq)` pairs each pool remembers, per origin, for dedupe.
/// The reference uses 200 (`protocol.js`: `POOL_SEEN_LIMIT`); a mesh relays, so the same item can
/// legitimately arrive by several paths and must only be applied once.
pub const POOL_SEEN_LIMIT: usize = 200;

/// Longest relay chain. One less than `MAX_DEVICES`, enough for a chain of ten: duplicates are
/// dropped, so a loop cannot storm (`protocol.js`: `POOL_MAX_HOPS`).
pub const POOL_MAX_HOPS: u32 = 9;

/// How long a crossing waits for `kvm.accept` before the pointer goes home (`KVM_TAKE_TIMEOUT_MS`).
pub const TAKE_TIMEOUT_MS: u64 = 1000;

/// The four sides of a screen, in the order a duplicate device keeps its first side.
pub const SIDES: [&str; 4] = ["left", "right", "top", "bottom"];

/// True when enough devices are connected for the mesh to mean anything.
///
/// This is the single dormancy predicate: one device (or none) is the two-device app exactly as it
/// was, and no mesh field is put on the wire.
pub fn mesh_active(sessions: usize) -> bool {
    sessions >= 2
}

/// True when `side` names a side of a screen.
pub fn is_side(side: &str) -> bool {
    SIDES.contains(&side)
}

/// The side facing `side`, the one a crossing arrives on. `None` for anything that is not a side.
pub fn opposite(side: &str) -> Option<&'static str> {
    match side {
        "left" => Some("right"),
        "right" => Some("left"),
        "top" => Some("bottom"),
        "bottom" => Some("top"),
        _ => None,
    }
}

/// Remove the fields this feature added, so a packet bound for a dormant (single-device) link is
/// exactly the packet the previous release sent.
///
/// The producers already set these fields only while the mesh is active; this is the last line of
/// defence, so a missed gate cannot change the wire for the two-device app (`contracts/wire.md`,
/// rule 2). It is a no-op while the mesh is active.
pub fn strip_if_dormant(pkt: &mut crate::network::WireMessage, active: bool) {
    if active {
        return;
    }
    pkt.route = None;
    pkt.to = None;
    pkt.mouse = None;
    pkt.keyboard = None;
    pkt.via = None;
    pkt.reckon = None;
    pkt.why = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_no_dead_multi_device_switch() {
        // The flag this feature replaced (`set_multi` → `MultiState.enabled`) was written by the UI
        // and read by nothing. Removing it is part of the work, and this keeps it removed: a switch
        // that changes no behaviour is worse than no switch.
        assert!(
            !include_str!("../main.rs").contains("set_multi"),
            "set_multi must not be registered again: nothing read its flag"
        );
        assert!(
            !include_str!("../commands.rs").contains("fn set_multi"),
            "set_multi must not come back"
        );
        assert!(
            !include_str!("../../../ui/shared/tauri-bridge.js").contains("setMulti"),
            "the bridge must not expose a command that does nothing"
        );
    }

    #[test]
    fn dormancy_needs_two_devices() {
        assert!(!mesh_active(0));
        assert!(!mesh_active(1));
        assert!(mesh_active(2));
        assert!(mesh_active(MAX_DEVICES - 1));
    }

    #[test]
    fn opposite_is_its_own_inverse() {
        for side in SIDES {
            let other = opposite(side).expect("every side has an opposite");
            assert_eq!(opposite(other), Some(side));
            assert_ne!(other, side);
        }
        assert_eq!(opposite("middle"), None);
        assert!(!is_side(""));
    }

    #[test]
    fn stripping_removes_only_the_new_fields() {
        let mut pkt = crate::network::WireMessage {
            t: "kvm.take".to_string(),
            v: Some(1),
            side: Some("right".to_string()),
            edge_at: Some(0.42),
            route: Some(7),
            to: Some("laptop".to_string()),
            mouse: Some(true),
            keyboard: Some(true),
            via: Some("phone".to_string()),
            reckon: Some(3),
            why: Some("busy".to_string()),
            ..Default::default()
        };
        strip_if_dormant(&mut pkt, false);
        assert_eq!(pkt.route, None);
        assert_eq!(pkt.to, None);
        assert_eq!(pkt.mouse, None);
        assert_eq!(pkt.keyboard, None);
        assert_eq!(pkt.via, None);
        assert_eq!(pkt.reckon, None);
        assert_eq!(pkt.why, None);
        // Everything the previous release sent is still there.
        assert_eq!(pkt.t, "kvm.take");
        assert_eq!(pkt.side.as_deref(), Some("right"));
        assert_eq!(pkt.edge_at, Some(0.42));

        // Active links keep the fields.
        let mut live = pkt.clone();
        live.route = Some(9);
        strip_if_dormant(&mut live, true);
        assert_eq!(live.route, Some(9));
    }
}
