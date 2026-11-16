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

pub mod chain;
pub mod clip;
pub mod edges;
pub mod pool;
pub mod roster;
pub mod session;

pub use edges::Edges;
pub use session::{MeshSession, MeshState, Role, TakeDecision};

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

/// How far ahead of this device's clock a copy time from another device may be before it is
/// clamped (`protocol.js`: `CLIP_SKEW_MS`). A device whose clock is hours fast must not win every
/// later copy for ever; ten minutes is the most a real clock is allowed to drift.
pub const CLIP_SKEW_MS: u64 = 10 * 60 * 1000;

/// Longest device id a clipboard stamp may name (`clipsync.js`: `MAX_ID`).
pub const CLIP_MAX_ID: usize = 128;

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

/// Who the pointer should cross to when it leaves this screen on a side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrossingTarget {
    /// This device, by the id the mesh knows it by.
    Device(String),
    /// This side leads nowhere. With one device connected that is the quiet answer; with several
    /// it means the side was never assigned, and the pointer simply stays.
    Nobody,
    /// This side is assigned to a device that is not connected any more. The person is told once,
    /// by name, instead of the edge doing nothing at all (`FR-014`).
    Missing(String),
}

/// Whether a `kvm.next` may hand the pointer on from one device to another.
///
/// A phone ends a chain rather than extending it (it has no edges of its own), a device cannot hand
/// the pointer back to itself or to the machine holding it, and the side it says it left on has to
/// be a side of a screen. Violations end the chain where it stands: nothing is released, nothing is
/// retried, so a malformed or malicious hop cannot move the pointer onto a screen the person did
/// not reach (`FR-014`).
pub fn chain_allowed(
    from: &str,
    to: &str,
    local_id: &str,
    side: Option<&str>,
    from_is_phone: bool,
    target_connected: bool,
) -> bool {
    !from_is_phone
        && !to.is_empty()
        && to != from
        && !to.eq_ignore_ascii_case(local_id)
        && side.map(is_side).unwrap_or(false)
        && target_connected
}

/// The `kvm.take` this device sends when the pointer crosses a side.
///
/// One place builds it, so the three platforms ask for the same handover in the same words.
/// `mouse: false` is a keyboard-only handover: the receiver accepts it with its keyboard sharing
/// on, places no pointer, and the pointer here never left.
pub fn take_packet(
    route: u64,
    side: &str,
    edge_at: f64,
    mouse: bool,
    keyboard: bool,
) -> crate::network::WireMessage {
    crate::network::WireMessage {
        t: "kvm.take".to_string(),
        v: Some(1),
        route: Some(route),
        // The side this pointer left on. The device entered sees it on the opposite side, and
        // places its pointer there, at the same fraction along it.
        side: Some(side.to_string()),
        edge_at: Some(edge_at),
        mouse: Some(mouse),
        keyboard: Some(keyboard),
        ..Default::default()
    }
}

/// Why a device was not added or a handover was not accepted, in words the person can act on
/// (`FR-005`, `FR-023`).
///
/// The code stays on the wire (`why`, `lastRefusal.why`) so the UI can translate it; this is the
/// sentence the app writes in its own log and in `link.message`.
pub fn refusal_text(why: &str, name: &str) -> String {
    match why {
        "limit" => format!(
            "{} was not added: this desk already has {} devices connected. Disconnect one to make room.",
            name,
            number_word(MAX_DEVICES)
        ),
        "not_allowed" => format!("{} is not accepting the pointer right now.", name),
        "not_connected" => format!("{} is not connected.", name),
        "busy" => format!("{} is already driving or being driven.", name),
        "not_ready" => format!("{} is not ready for the pointer yet.", name),
        "too_large" => "That copy is larger than the clipboard limit.".to_string(),
        "concealed" => "That copy is concealed, so it was not shared.".to_string(),
        _ => format!("{} did not answer.", name),
    }
}

/// Small counts as words, because they read better in a sentence shown to a person.
fn number_word(n: usize) -> String {
    const WORDS: [&str; 13] = [
        "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    WORDS
        .get(n)
        .map(|w| w.to_string())
        .unwrap_or_else(|| n.to_string())
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
    pkt.peers = None;
    pkt.members = None;
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

    /// The crossing a build with one device sends must stay the packet it always was, even though
    /// the same helper now builds the mesh handover too (`contracts/wire.md`, rule 2).
    #[test]
    fn a_crossing_with_one_device_is_the_packet_it_always_was() {
        let mut pkt = take_packet(4, "right", 0.4213, true, true);
        // Active: the handover names its route, the side it left on, and what travels with it.
        let live = serde_json::to_value(&pkt).expect("serialize take");
        assert_eq!(live["route"], 4);
        assert_eq!(live["side"], "right");
        assert_eq!(live["edgeAt"], 0.4213);
        assert_eq!(live["mouse"], true);
        assert_eq!(live["keyboard"], true);

        // Dormant: byte for byte the packet the previous release sent.
        strip_if_dormant(&mut pkt, mesh_active(1));
        let legacy = crate::network::WireMessage {
            t: "kvm.take".to_string(),
            v: Some(1),
            side: Some("right".to_string()),
            edge_at: Some(0.4213),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&pkt).expect("serialize take"),
            serde_json::to_string(&legacy).expect("serialize legacy take"),
            "one device must not see a single new field"
        );
    }

    #[test]
    fn a_handover_names_the_side_it_left_on_so_the_other_side_can_place_the_pointer() {
        let pkt = take_packet(1, "bottom", 0.25, true, false);
        assert_eq!(pkt.mouse, Some(true));
        assert_eq!(pkt.keyboard, Some(false), "the person's keyboard setting travels with it");
        // The receiver places at the opposite of the side this pointer left on.
        assert_eq!(opposite(pkt.side.as_deref().unwrap()), Some("top"));
    }

    /// `FR-014` and the phone rule: a chain ends at a phone, cannot point at itself or at the
    /// machine holding the pointer, and cannot name a device that is not connected.
    #[test]
    fn a_chain_only_goes_to_a_real_device_that_is_not_a_phone() {
        assert!(chain_allowed("laptop", "phone", "me", Some("right"), false, true));
        assert!(!chain_allowed("phone", "laptop", "me", Some("right"), true, true),
            "a phone has no edges of its own, so it ends a chain");
        assert!(!chain_allowed("laptop", "laptop", "me", Some("right"), false, true),
            "a device cannot hand the pointer to itself");
        assert!(!chain_allowed("laptop", "ME", "me", Some("right"), false, true),
            "the machine holding the pointer is not a hop");
        assert!(!chain_allowed("laptop", "tablet", "me", None, false, true),
            "a hop has to say which side it left on");
        assert!(!chain_allowed("laptop", "tablet", "me", Some("diagonal"), false, true));
        assert!(!chain_allowed("laptop", "tablet", "me", Some("right"), false, false),
            "a device that is not connected is not somewhere the pointer can go");
        assert!(!chain_allowed("laptop", "", "me", Some("right"), false, true));
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
