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
//! Pointer and keyboard traffic never goes to a device just because the roster mentions it: a
//! session is what makes a device reachable (`contracts/wire.md`). The roster is how that session
//! comes to exist in the first place, because it is how the address of a paired device is learned.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::network::WireMessage;

/// A device as a peer described it (`mesh.roster` `members` entry).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownMember {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub os: String,
}

/// Everyone this device knows of, for the page and for re-dialling.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDevice {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub os: String,
    /// True when this device has its own session with it.
    #[serde(default)]
    pub direct: bool,
    /// The devices that reported it, for an indirect one.
    #[serde(default)]
    pub via: Vec<String>,
}

/// The links of this device and of every peer that has told it, plus the names they gave.
///
/// One entry per device id. `links` is that device's direct links as last heard; `members` is the
/// device list the peer sent with them, so a name and an OS survive without a session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Roster {
    self_id: String,
    links: BTreeMap<String, Vec<String>>,
    members: BTreeMap<String, Vec<KnownMember>>,
    /// The last `reckon` accepted from each peer, so a stale message cannot undo a newer one.
    revisions: BTreeMap<String, u64>,
}

impl Roster {
    pub fn new(self_id: &str) -> Self {
        let mut links = BTreeMap::new();
        links.insert(self_id.to_string(), Vec::new());
        Self {
            self_id: self_id.to_string(),
            links,
            members: BTreeMap::new(),
            revisions: BTreeMap::new(),
        }
    }

    /// Learn this device's own id once the app knows it (the host name, lowercased).
    pub fn set_self_id(&mut self, self_id: &str) {
        if self_id.is_empty() || self_id == self.self_id {
            return;
        }
        let old = std::mem::take(&mut self.self_id);
        let mut links = std::mem::take(&mut self.links);
        if !old.is_empty() {
            links.remove(&old);
        }
        links.insert(self_id.to_string(), Vec::new());
        self.self_id = self_id.to_string();
        self.links = links;
    }

    pub fn self_id(&self) -> &str {
        &self.self_id
    }

    /// This device's direct links (the connected sessions).
    pub fn set_self(&mut self, ids: &[String]) {
        let self_id = self.self_id.clone();
        self.set_peer(&self_id, ids);
    }

    /// Replace what `id` reports as its direct links. Duplicates are dropped, and the input is
    /// copied, so a caller cannot change the roster afterwards by keeping the list.
    pub fn set_peer(&mut self, id: &str, ids: &[String]) {
        if id.is_empty() {
            return;
        }
        let mut unique: Vec<String> = Vec::with_capacity(ids.len());
        for x in ids {
            if x.is_empty() || unique.iter().any(|u| u == x) {
                continue;
            }
            unique.push(x.clone());
        }
        self.links.insert(id.to_string(), unique);
    }

    /// Replace the device list a peer reported, so names survive for devices without a session.
    pub fn set_members(&mut self, from: &str, members: &[KnownMember]) {
        if from.is_empty() {
            return;
        }
        let mut clean: Vec<KnownMember> = Vec::with_capacity(members.len());
        for m in members {
            if m.id.is_empty() {
                continue;
            }
            let mut member = m.clone();
            member.name = member.name.chars().take(64).collect();
            member.os = member.os.chars().take(16).collect();
            clean.push(member);
        }
        self.members.insert(from.to_string(), clean);
    }

    /// Forget a device, from every list as well as from the roster itself.
    ///
    /// Forgetting has to win over whatever a peer still believes: a device removed here is removed
    /// from this device's own links and from every link list a peer reported, so no later update
    /// can quietly walk it back in.
    pub fn remove(&mut self, id: &str) {
        self.links.remove(id);
        self.members.remove(id);
        self.revisions.remove(id);
        for list in self.links.values_mut() {
            list.retain(|x| x != id);
        }
        for list in self.members.values_mut() {
            list.retain(|m| m.id != id);
        }
    }

    /// The direct links of `id` (a copy), or `None` when nothing is known about it.
    pub fn direct(&self, id: &str) -> Option<Vec<String>> {
        self.links.get(id).cloned()
    }

    /// True when `a` lists `b` or `b` lists `a`. Symmetric by construction.
    pub fn linked(&self, a: &str, b: &str) -> bool {
        let has = |id: &str, other: &str| {
            self.links
                .get(id)
                .map(|list| list.iter().any(|x| x == other))
                .unwrap_or(false)
        };
        has(a, b) || has(b, a)
    }

    /// A copy of every link list, for tests and for a state snapshot.
    pub fn snapshot(&self) -> BTreeMap<String, Vec<String>> {
        self.links.clone()
    }

    /// Everyone this device knows of: its own sessions, and the devices its peers reported.
    ///
    /// A direct device is listed once even when several peers also mention it. An indirect one
    /// names the peers that reported it, so the page can say who it came through.
    pub fn known(&self) -> Vec<KnownDevice> {
        let own = self.links.get(&self.self_id).cloned().unwrap_or_default();
        let mut out: BTreeMap<String, KnownDevice> = BTreeMap::new();
        // Everyone with a session here, then everyone who has reported their own links to us.
        let mut seen: Vec<String> = own.clone();
        seen.extend(self.links.keys().cloned());
        for id in seen {
            if id == self.self_id || id.is_empty() {
                continue;
            }
            let member = self.member_of(&id);
            out.insert(
                id.clone(),
                KnownDevice {
                    id: id.clone(),
                    name: member
                        .and_then(|m| (!m.name.is_empty()).then(|| m.name.clone()))
                        .unwrap_or_default(),
                    os: member
                        .and_then(|m| (!m.os.is_empty()).then(|| m.os.clone()))
                        .unwrap_or_default(),
                    direct: own.iter().any(|x| x == &id),
                    via: Vec::new(),
                },
            );
        }
        // And the devices a peer listed as its own links. A peer that sends only `peers` (no
        // `members`) still introduces them: an id with no name is better than a device the person
        // was never told about.
        for (reporter, list) in &self.links {
            if reporter == &self.self_id {
                continue;
            }
            for id in list {
                if id == &self.self_id || id.is_empty() {
                    continue;
                }
                match out.get_mut(id) {
                    Some(cur) => {
                        if !cur.direct && !cur.via.iter().any(|x| x == reporter) {
                            cur.via.push(reporter.clone());
                        }
                    }
                    None => {
                        let member = self.member_of(id);
                        out.insert(
                            id.clone(),
                            KnownDevice {
                                id: id.clone(),
                                name: member
                                    .and_then(|m| (!m.name.is_empty()).then(|| m.name.clone()))
                                    .unwrap_or_default(),
                                os: member
                                    .and_then(|m| (!m.os.is_empty()).then(|| m.os.clone()))
                                    .unwrap_or_default(),
                                direct: false,
                                via: vec![reporter.clone()],
                            },
                        );
                    }
                }
            }
        }
        for (via, list) in &self.members {
            if via == &self.self_id || !self.links.contains_key(via) {
                continue;
            }
            for m in list {
                if m.id == self.self_id {
                    continue;
                }
                match out.get_mut(&m.id) {
                    Some(cur) => {
                        if !cur.direct {
                            if !cur.via.iter().any(|x| x == via) {
                                cur.via.push(via.clone());
                            }
                            if cur.name.is_empty() && !m.name.is_empty() {
                                cur.name = m.name.clone();
                            }
                            if cur.os.is_empty() && !m.os.is_empty() {
                                cur.os = m.os.clone();
                            }
                        }
                    }
                    None => {
                        out.insert(
                            m.id.clone(),
                            KnownDevice {
                                id: m.id.clone(),
                                name: m.name.clone(),
                                os: m.os.clone(),
                                direct: false,
                                via: vec![via.clone()],
                            },
                        );
                    }
                }
            }
        }
        out.into_values().collect()
    }

    fn member_of(&self, id: &str) -> Option<&KnownMember> {
        self.members
            .values()
            .flat_map(|list| list.iter())
            .find(|m| m.id == id)
    }

    /// The message this device sends to its peers: its own direct links, their names, and the
    /// revision a receiver uses to ignore an older one that arrives late.
    pub fn roster_message(&self, reckon: u64) -> WireMessage {
        let peers: Vec<String> = self
            .links
            .get(&self.self_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|id| id != &self.self_id)
            .collect();
        let members = self
            .members
            .get(&self.self_id)
            .cloned()
            .unwrap_or_else(|| {
                peers
                    .iter()
                    .map(|id| KnownMember {
                        id: id.clone(),
                        name: String::new(),
                        os: String::new(),
                    })
                    .collect()
            });
        WireMessage {
            t: "mesh.roster".to_string(),
            v: Some(1),
            o: Some(self.self_id.clone()),
            reckon: Some(reckon),
            peers: Some(peers),
            members: if members.is_empty() {
                None
            } else {
                Some(
                    members
                        .into_iter()
                        .map(|m| crate::network::WireRosterMember {
                            id: m.id,
                            name: Some(m.name).filter(|s| !s.is_empty()),
                            os: Some(m.os).filter(|s| !s.is_empty()),
                        })
                        .collect(),
                )
            },
            ..Default::default()
        }
    }

    /// Take a `mesh.roster` from `from`: its link list and its device list.
    ///
    /// Returns false when the message is stale (`reckon` at or below the last one accepted from the
    /// same device), which is what keeps a slow update from undoing a newer one. `forgotten` names
    /// the devices the person removed here: they are dropped from what is learned, so a peer's
    /// belief cannot bring one back.
    pub fn accept(
        &mut self,
        from: &str,
        reckon: Option<u64>,
        peers: &[String],
        members: &[KnownMember],
        forgotten: &[String],
    ) -> bool {
        if from.is_empty() || from == self.self_id {
            return false;
        }
        if let Some(r) = reckon {
            if let Some(seen) = self.revisions.get(from) {
                if r <= *seen {
                    return false;
                }
            }
            self.revisions.insert(from.to_string(), r);
        }
        let keep = |id: &str| !forgotten.iter().any(|f| f == id);
        let peers: Vec<String> = peers.iter().filter(|id| keep(id)).cloned().collect();
        let members: Vec<KnownMember> = members
            .iter()
            .filter(|m| keep(&m.id))
            .cloned()
            .collect();
        self.set_peer(from, &peers);
        self.set_members(from, &members);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// `ui/shared/test/mesh-roster.js`, first block: direct links, symmetry, copies, dedupe.
    #[test]
    fn the_roster_matches_the_reference_rules() {
        let mut r = Roster::new("A");
        assert_eq!(r.direct("A"), Some(vec![]), "self starts with no links");
        assert_eq!(r.direct("B"), None, "unknown device: null");

        r.set_self(&ids(&["B", "C"]));
        r.set_peer("B", &ids(&["A", "D"]));
        r.set_peer("C", &ids(&["A"]));

        assert_eq!(r.direct("A"), Some(ids(&["B", "C"])));
        assert_eq!(r.direct("B"), Some(ids(&["A", "D"])));
        assert_eq!(r.direct("C"), Some(ids(&["A"])));
        assert!(r.linked("A", "B"));
        assert!(r.linked("B", "A"), "linked is symmetric");
        assert!(
            r.linked("B", "D"),
            "B lists D: linked, even though D has no entry"
        );
        assert!(!r.linked("C", "D"), "no link known between C and D");
        assert!(!r.linked("X", "A"), "unknown device is not linked");

        // `direct()` and `snapshot()` return copies: changing them does not change the roster.
        assert!(r.direct("A").is_some());
        r.snapshot()
            .get("A")
            .expect("self is in the snapshot")
            .clone()
            .push("Z".to_string());
        assert_eq!(r.direct("A"), Some(ids(&["B", "C"])));

        // `set_peer` copies its input and drops duplicates.
        let input = ids(&["E", "E", "F"]);
        r.set_peer("E", &input);
        assert_eq!(
            r.direct("E"),
            Some(ids(&["E", "F"])),
            "duplicates dropped, later changes to the input ignored"
        );
    }

    /// `mesh-roster.js`, second block: removing a device cleans every list.
    #[test]
    fn removing_a_device_removes_it_everywhere() {
        let mut r = Roster::new("A");
        r.set_self(&ids(&["B", "C", "D"]));
        r.set_peer("B", &ids(&["A", "C"]));
        r.set_peer("C", &ids(&["A", "B", "D"]));
        r.set_peer("D", &ids(&["A", "C"]));

        r.remove("C");
        assert_eq!(r.direct("C"), None, "removed device is unknown");
        assert_eq!(r.direct("A"), Some(ids(&["B", "D"])), "self no longer lists C");
        assert_eq!(r.direct("B"), Some(ids(&["A"])), "B no longer lists C");
        assert_eq!(r.direct("D"), Some(ids(&["A"])), "D no longer lists C");
        assert!(!r.linked("B", "C"));

        for (id, list) in r.snapshot() {
            assert!(
                !list.iter().any(|x| x == "C"),
                "{id} still lists C"
            );
        }

        r.remove("Q");
        assert_eq!(r.direct("A"), Some(ids(&["B", "D"])));
    }

    /// `mesh-roster.js`, third block: a peer's newer list replaces its older one.
    #[test]
    fn a_peer_replaces_its_earlier_list() {
        let mut r = Roster::new("A");
        r.set_peer("B", &ids(&["A", "C"]));
        r.set_peer("B", &ids(&["A"]));
        assert_eq!(r.direct("B"), Some(ids(&["A"])));
        assert!(
            !r.linked("B", "C"),
            "a link that B no longer reports is gone"
        );
    }

    /// `Mesh.known()`: direct devices, and the ones learned through a peer, each named once.
    #[test]
    fn known_lists_direct_and_indirect_devices_once() {
        let mut r = Roster::new("A");
        r.set_self(&ids(&["B"]));
        r.set_peer("B", &ids(&["A", "C"]));
        r.set_members(
            "B",
            &[
                KnownMember {
                    id: "A".into(),
                    name: "alpha".into(),
                    os: "mac".into(),
                },
                KnownMember {
                    id: "C".into(),
                    name: "phone".into(),
                    os: "android".into(),
                },
                // A peer must not be able to name us as a device of its own roster.
                KnownMember {
                    id: "A".into(),
                    name: "impostor".into(),
                    os: "linux".into(),
                },
            ],
        );

        let known = r.known();
        assert_eq!(known.len(), 2, "self is never listed: {known:?}");
        let b = known.iter().find(|d| d.id == "B").expect("B");
        assert!(b.direct, "B has a session with us");
        let c = known.iter().find(|d| d.id == "C").expect("C");
        assert!(!c.direct);
        assert_eq!(c.via, vec!["B".to_string()]);
        assert_eq!(c.name, "phone");
        assert_eq!(c.os, "android");
    }

    /// A stale roster must not undo a newer one from the same device.
    #[test]
    fn a_stale_reckon_is_ignored() {
        let mut r = Roster::new("A");
        let first = ids(&["A", "C"]);
        assert!(r.accept("B", Some(4), &first, &[], &[]));
        let later = ids(&["A"]);
        assert!(
            !r.accept("B", Some(3), &later, &[], &[]),
            "an older revision arriving late changes nothing"
        );
        assert_eq!(r.direct("B"), Some(first));
        assert!(
            r.accept("B", Some(5), &later, &[], &[]),
            "a newer revision is taken"
        );
        assert_eq!(r.direct("B"), Some(later));
    }

    /// A device the person forgot is not reintroduced by any peer's roster.
    #[test]
    fn a_forgotten_device_is_never_learned_back() {
        let mut r = Roster::new("A");
        r.set_self(&ids(&["B", "C"]));
        r.remove("C");
        let forgotten = ids(&["C"]);
        assert!(r.accept(
            "B",
            Some(1),
            &ids(&["A", "C"]),
            &[KnownMember {
                id: "C".into(),
                name: "phone".into(),
                os: "android".into(),
            }],
            &forgotten
        ));
        assert_eq!(
            r.direct("B"),
            Some(ids(&["A"])),
            "the forgotten device is dropped from the peer's link list"
        );
        assert!(
            r.known().iter().all(|d| d.id != "C"),
            "and it is not listed as known"
        );
    }

    /// The wire shape the contract documents.
    #[test]
    fn the_roster_message_names_direct_links_and_their_devices() {
        let mut r = Roster::new("mac-1");
        r.set_self(&ids(&["win-1", "linux-1"]));
        r.set_members(
            "mac-1",
            &[
                KnownMember {
                    id: "win-1".into(),
                    name: "Desktop".into(),
                    os: "win".into(),
                },
                KnownMember {
                    id: "linux-1".into(),
                    name: "Box".into(),
                    os: "linux".into(),
                },
            ],
        );
        let msg = r.roster_message(12);
        assert_eq!(msg.t, "mesh.roster");
        assert_eq!(msg.reckon, Some(12));
        assert_eq!(msg.o.as_deref(), Some("mac-1"));
        assert_eq!(
            msg.peers,
            Some(ids(&["win-1", "linux-1"])),
            "peers are this device's direct links"
        );
        let members = msg.members.clone().expect("members");
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].id, "win-1");
        assert_eq!(members[0].name.as_deref(), Some("Desktop"));
        assert_eq!(members[0].os.as_deref(), Some("win"));
        // Serialised with the documented key names and nothing else.
        let json = serde_json::to_string(&msg).expect("serialize roster");
        assert!(json.contains("\"t\":\"mesh.roster\""), "{json}");
        assert!(json.contains("\"peers\":[\"win-1\",\"linux-1\"]"), "{json}");
        assert!(json.contains("\"reckon\":12"), "{json}");
    }

    /// An empty roster still says "nothing here" rather than omitting the field, so a peer can
    /// learn that a link ended (`FR-021`).
    #[test]
    fn an_empty_roster_says_so() {
        let mut r = Roster::new("mac-1");
        r.set_self(&[]);
        let msg = r.roster_message(1);
        assert_eq!(msg.peers, Some(vec![]));
        assert!(msg.members.is_none(), "no devices to name");
    }
}
