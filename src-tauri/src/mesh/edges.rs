// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Which device is behind each side of this screen, and where along it a crossing lands.
//!
//! Reference: `ui/shared/kvm-edges.js` (`SIDES`, `normalizeEdges`, `assignEdge`, `edgeForPeer`,
//! `setRemoteEdge`, `clearPeer`, `migrateEdges`). Scenarios: `ui/shared/test/kvm-edges.js`, which
//! stays the executable specification: every test below is one of its cases.
//!
//! The rules the port must keep, because they are what makes the picker impossible to break:
//!
//! * A side is either empty or names exactly one device.
//! * A device appears on at most one side; a duplicate keeps its first side in `left, right, top,
//!   bottom` order.
//! * `assign` moves a device that was on another side, and refuses to overwrite a side that already
//!   leads to a different device.
//! * Edges live in `layout.json` next to the existing `side`/`along`, which stay written while
//!   exactly one side is set, so an older build still reads the file it expects.
//!
//! While fewer than two devices are connected the mesh is dormant and nothing here is consulted:
//! the two-device app keeps using `layout.side`, exactly as before (`docs/mesh.md`).

use serde::{Deserialize, Serialize};

/// Longest device id accepted. Matches `MAX_ID` in the reference: ids are host names.
pub const MAX_ID: usize = 64;

/// Which device is behind each side of the screen. `None` is a side that leads nowhere.
///
/// Serialised as `{"left":null,"right":"laptop",...}`: a stable shape, so the file and the state
/// payload can be read by an older build without a migration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Edges {
    #[serde(default)]
    pub left: Option<String>,
    #[serde(default)]
    pub right: Option<String>,
    #[serde(default)]
    pub top: Option<String>,
    #[serde(default)]
    pub bottom: Option<String>,
}

/// A device id, or nothing. Empty and over-long ids are not devices, and must never end up on a
/// side: a side that names something that cannot be reached is a dead edge.
fn valid_id(id: &str) -> Option<String> {
    let id = id.trim();
    if id.is_empty() || id.len() > MAX_ID {
        None
    } else {
        Some(id.to_string())
    }
}

impl Edges {
    /// The device behind `side`, if any.
    pub fn get(&self, side: &str) -> Option<&str> {
        match side {
            "left" => self.left.as_deref(),
            "right" => self.right.as_deref(),
            "top" => self.top.as_deref(),
            "bottom" => self.bottom.as_deref(),
            _ => None,
        }
    }

    fn set(&mut self, side: &str, id: Option<String>) {
        match side {
            "left" => self.left = id,
            "right" => self.right = id,
            "top" => self.top = id,
            "bottom" => self.bottom = id,
            _ => {}
        }
    }

    /// Nothing assigned: the two-device app's arrangement is still the one that counts.
    pub fn is_empty(&self) -> bool {
        super::SIDES.iter().all(|side| self.get(side).is_none())
    }

    /// Any value becomes at most one device per side, in `SIDES` order.
    ///
    /// A device named twice keeps its first side, so a hand-edited file or a stale state from
    /// another build cannot make one device answer for two edges — which would send the pointer to
    /// the same screen whichever way it left.
    pub fn normalize(&self) -> Edges {
        let mut out = Edges::default();
        let mut seen: Vec<String> = Vec::new();
        for side in super::SIDES {
            let id = self.get(side).and_then(valid_id);
            if let Some(id) = id {
                if !seen.iter().any(|s| s.eq_ignore_ascii_case(&id)) {
                    seen.push(id.clone());
                    out.set(side, Some(id));
                }
            }
        }
        out
    }

    /// The side that leads to `id`, or `None`. A device is on at most one side, so the first match
    /// is the only match.
    pub fn side_of(&self, id: &str) -> Option<&'static str> {
        if id.is_empty() {
            return None;
        }
        super::SIDES
            .iter()
            .find(|side| self.get(side).map(|d| d.eq_ignore_ascii_case(id)).unwrap_or(false))
            .copied()
    }

    /// The person's choice: put `id` on `side`, or clear the side with `None`.
    ///
    /// A device on another side moves here. `None` means the choice was refused: the side already
    /// leads to a different device, or the side or the id is not one. Refusing rather than
    /// overwriting is what stops a click from silently undoing another edge the person set.
    pub fn assign(&self, side: &str, id: Option<&str>) -> Option<Edges> {
        if !super::is_side(side) {
            return None;
        }
        let device = match id {
            None => None,
            Some(raw) => Some(valid_id(raw)?),
        };
        let mut next = self.normalize();
        match device {
            None => {
                next.set(side, None);
                Some(next)
            }
            Some(device) => {
                if let Some(current) = next.get(side) {
                    if !current.eq_ignore_ascii_case(&device) {
                        return None;
                    }
                }
                for other in super::SIDES {
                    if next
                        .get(other)
                        .map(|d| d.eq_ignore_ascii_case(&device))
                        .unwrap_or(false)
                    {
                        next.set(other, None);
                    }
                }
                next.set(side, Some(device));
                Some(next)
            }
        }
    }

    /// A peer's `kvm.layout {pcSide}`: `pc_side` is the side the **sender** crosses on, so it is on
    /// the opposite side of this screen. Refused when another device already holds that edge.
    pub fn set_remote(&self, pc_side: &str, id: &str) -> Option<Edges> {
        let ours = super::opposite(pc_side)?;
        self.assign(ours, Some(id))
    }

    /// The device is gone from every side (a forget, or an explicit disconnect).
    pub fn clear_peer(&self, id: &str) -> Edges {
        let mut next = self.normalize();
        if id.is_empty() {
            return next;
        }
        for side in super::SIDES {
            if next
                .get(side)
                .map(|d| d.eq_ignore_ascii_case(id))
                .unwrap_or(false)
            {
                next.set(side, None);
            }
        }
        next
    }

    /// Settings from before edges existed: one paired device and a legacy `pcSide`.
    ///
    /// `pc_side` is where this machine is seen from the other computer, so the device is on the
    /// opposite side of this screen. Anything else (edges already set, another number of devices,
    /// no `pc_side`) stays exactly as it is, so this can run on every launch without undoing a
    /// person's arrangement.
    pub fn migrate(&self, pc_side: Option<&str>, device_ids: &[String]) -> Edges {
        let current = self.normalize();
        if !current.is_empty() {
            return current;
        }
        let mut ids: Vec<String> = Vec::new();
        for id in device_ids {
            if let Some(id) = valid_id(id) {
                if !ids.iter().any(|s| s.eq_ignore_ascii_case(&id)) {
                    ids.push(id);
                }
            }
        }
        let pc_side = match pc_side {
            Some(s) if super::is_side(s) => s,
            _ => return current,
        };
        if ids.len() != 1 {
            return current;
        }
        current.assign(super::opposite(pc_side).unwrap_or("left"), Some(&ids[0]))
            .unwrap_or(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Edges {
        Edges::default()
    }

    fn edges(pairs: &[(&str, &str)]) -> Edges {
        let mut e = Edges::default();
        for (side, id) in pairs {
            e.set(side, Some(id.to_string()));
        }
        e
    }

    /// `ui/shared/test/kvm-edges.js`, "normalizeEdges".
    #[test]
    fn normalization_drops_what_is_not_a_device() {
        assert_eq!(none().normalize(), none());
        // A bad id (a number in the file) and an unknown side are dropped; serde drops the key.
        assert_eq!(
            edges(&[("top", "B"), ("bottom", "C")]).normalize(),
            edges(&[("top", "B"), ("bottom", "C")])
        );
        // One device on one side only: the first in SIDES order wins.
        assert_eq!(
            edges(&[("top", "B"), ("left", "B")]).normalize(),
            edges(&[("left", "B")])
        );
        assert_eq!(edges(&[("top", &"x".repeat(65))]).normalize(), none());
        assert_eq!(edges(&[("top", "")]).normalize(), none());
        assert_eq!(edges(&[("top", "   ")]).normalize(), none());
    }

    /// `kvm-edges.js`, "edgeForPeer".
    #[test]
    fn a_device_has_exactly_one_side() {
        let e = edges(&[("top", "B"), ("right", "C")]);
        assert_eq!(e.side_of("B"), Some("top"));
        assert_eq!(e.side_of("C"), Some("right"));
        assert_eq!(e.side_of("Z"), None);
        assert_eq!(e.side_of(""), None);
        assert_eq!(none().side_of("B"), None);
    }

    /// `kvm-edges.js`, "setRemoteEdge".
    #[test]
    fn a_peer_arrives_on_the_side_facing_it() {
        // The sender crosses on its left, so it is on our right.
        assert_eq!(
            none().set_remote("left", "B"),
            Some(edges(&[("right", "B")]))
        );
        assert_eq!(
            none().set_remote("bottom", "C").map(|e| e.top),
            Some(Some("C".to_string()))
        );

        let a = none().set_remote("left", "B");
        assert_eq!(a.clone().unwrap().set_remote("left", "B"), a, "a refresh changes nothing");

        // A device that moves leaves its old edge behind.
        assert_eq!(
            edges(&[("top", "B")]).set_remote("left", "B"),
            Some(edges(&[("right", "B")]))
        );

        // Never overwrite another device's side.
        assert_eq!(edges(&[("right", "C")]).set_remote("left", "B"), None);
        assert_eq!(none().set_remote("bogus", "B"), None);
        assert_eq!(none().set_remote("left", ""), None);
    }

    /// `kvm-edges.js`, "assignEdge".
    #[test]
    fn the_person_can_move_and_clear_an_edge() {
        let one = edges(&[("top", "B")]);
        assert_eq!(Edges::default().assign("top", Some("B")), Some(one.clone()));
        assert_eq!(one.assign("right", Some("B")), Some(edges(&[("right", "B")])));
        assert_eq!(one.assign("top", Some("C")), None, "top is taken by B");
        assert_eq!(one.assign("top", None), Some(none()));
        assert_eq!(one.assign("top", Some("B")), Some(one.clone()));
        assert_eq!(one.assign("diagonal", Some("C")), None);
        assert_eq!(one, edges(&[("top", "B")]), "the input is not changed");
    }

    /// `kvm-edges.js`, "clearPeer".
    #[test]
    fn forgetting_a_device_clears_its_edge() {
        let e = edges(&[("top", "B"), ("bottom", "C")]);
        assert_eq!(e.clear_peer("B"), edges(&[("bottom", "C")]));
        assert_eq!(e.clear_peer("Z"), e, "nothing to clear");
        assert_eq!(e.clear_peer("C"), edges(&[("top", "B")]));
        assert_eq!(e, edges(&[("top", "B"), ("bottom", "C")]));
    }

    /// `kvm-edges.js`, "migrateEdges".
    #[test]
    fn an_old_pc_side_becomes_the_one_devices_edge() {
        let one = vec!["B".to_string()];
        assert_eq!(
            none().migrate(Some("left"), &one),
            edges(&[("right", "B")])
        );
        assert_eq!(
            none().migrate(Some("top"), &one),
            edges(&[("bottom", "B")])
        );
        assert_eq!(
            none().migrate(Some("right"), &one),
            edges(&[("left", "B")])
        );

        // Already migrated: left alone, whatever pcSide says.
        let set = edges(&[("top", "C")]);
        assert_eq!(set.migrate(Some("left"), &one), set);

        // Not exactly one device, or no usable pcSide: nothing to migrate.
        assert_eq!(none().migrate(Some("left"), &[]), none());
        assert_eq!(
            none().migrate(Some("left"), &["B".to_string(), "C".to_string()]),
            none()
        );
        assert_eq!(none().migrate(None, &one), none());
        assert_eq!(none().migrate(Some("nope"), &one), none());
        assert_eq!(none().migrate(None, &[]), none());
    }

    #[test]
    fn edges_survive_a_round_trip_and_older_files_still_load() {
        let e = edges(&[("left", "laptop"), ("bottom", "phone")]);
        let json = serde_json::to_string(&e).expect("serialize edges");
        assert_eq!(json, r#"{"left":"laptop","right":null,"top":null,"bottom":"phone"}"#);
        let back: Edges = serde_json::from_str(&json).expect("deserialize edges");
        assert_eq!(back, e);

        // A layout file written before edges existed: no key at all.
        let old = serde_json::from_str::<Edges>("{}").expect("empty object");
        assert_eq!(old, none());
        assert!(old.is_empty());
    }
}
