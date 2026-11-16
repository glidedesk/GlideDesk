// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! What this app knows about each other device it is connected to.
//!
//! Reference: `shared/mesh.js` (`Mesh` — the registry, `addPeer`/`removePeer`/`get`/`peers`) and
//! `mac/mesh/kvm.js` (`partner`, `peerRoute`, `mode`). Scenarios: `shared/test/mesh-registry.js`,
//! `shared/test/mesh-chain.js`.
//!
//! One rule governs the whole file: **at most one device may be driven and at most one may be
//! driving**. A crossing that would break that is refused, never silently accepted, so no device is
//! ever left believing it has control it does not have.

use serde::{Deserialize, Serialize};

/// What one device is to this one, for the pointer and keyboard only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Not involved in a handover either way.
    #[default]
    Idle,
    /// This device is driving it.
    Driving,
    /// It is driving this device.
    Driven,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Idle => "idle",
            Role::Driving => "driving",
            Role::Driven => "driven",
        }
    }
}

/// One connected device. Mirrors the reference's peer entry plus the handover state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshSession {
    /// Stable device id from pairing; the key of the registry. Never empty.
    pub id: String,
    pub name: String,
    /// `mac` | `win` | `linux` | `android`.
    pub os: String,
    /// Reachable host:port, for the device list and for re-dialling.
    #[serde(default)]
    pub address: Option<String>,
    /// The peer speaks multi-device (false = an older app: plain messages, never a relay).
    #[serde(default)]
    pub mesh: bool,
    /// The peer accepts pooled items (clipboard). Off means neither send nor accept.
    #[serde(default = "default_true")]
    pub pool: bool,
    #[serde(default)]
    pub role: Role,
    /// Last handover route agreed with this peer. Never decreases for a given session.
    #[serde(default)]
    pub route: u64,
    /// Milliseconds since the epoch.
    #[serde(default)]
    pub connected_at: u64,
    #[serde(default)]
    pub last_seen: u64,
}

fn default_true() -> bool {
    true
}

impl MeshSession {
    /// A session for a device that has just connected. `mesh` and `pool` are learned from its
    /// `hello`, so they start at the safe value: an old app until it says otherwise, pool on.
    pub fn connected(id: &str, name: &str, os: &str, address: Option<String>, now: u64) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            os: os.to_string(),
            address,
            mesh: false,
            pool: true,
            role: Role::Idle,
            route: 0,
            connected_at: now,
            last_seen: now,
        }
    }

    /// Phones end a chain: this device may be driven by one, or drive one, but a phone is never
    /// asked to relay a crossing on to a third device (`FR-015`).
    pub fn is_phone(&self) -> bool {
        self.os.eq_ignore_ascii_case("android") || self.os.eq_ignore_ascii_case("ios")
    }
}

/// Why a crossing or a copy did not happen, in a form the person can read (`FR-023`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    /// The device the attempt was aimed at.
    pub device: String,
    /// `not_connected` | `not_allowed` | `busy` | `too_large` | `concealed` | `not_ready`.
    pub why: String,
    /// Milliseconds since the epoch.
    pub at: u64,
}

/// The mesh part of the app state, serialised to the UI as `mesh` in `state-change`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshState {
    /// True once two or more devices are connected. The UI shows the Multi-device page only then.
    #[serde(default)]
    pub active: bool,
    /// The device this one drives or is driven by, if any.
    #[serde(default)]
    pub partner: Option<String>,
    /// `home` | `driving` | `driven` — mirrors `FullState.input` so both can be read.
    #[serde(default = "home_mode")]
    pub role: String,
    /// The last handover route this device issued or accepted.
    #[serde(default)]
    pub route: u64,
    #[serde(default)]
    pub last_refusal: Option<Refusal>,
    /// Every connected device, in connection order.
    #[serde(default)]
    pub sessions: Vec<MeshSession>,
    /// Roster revision, bumped whenever this device's direct links change.
    #[serde(default)]
    pub reckon: u64,
    /// Everyone this device knows of through its peers (`mesh.roster` `members`), so a device
    /// that is not linked to them directly can still be listed. Advisory: trust is `pairs.json`.
    #[serde(default)]
    pub known_links: Vec<String>,
    /// The route the device driving this one is using. A take that does not advance it is a
    /// duplicate and is ignored (reference: `peerRoute`).
    #[serde(skip)]
    pub peer_route: u64,
    /// A take this device sent that has not been answered yet. Not serialised: it lasts one second.
    #[serde(skip)]
    pub pending: Option<PendingTake>,
}

/// A `kvm.take` this device sent and is waiting on (`KVM_TAKE_TIMEOUT_MS`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTake {
    pub target: String,
    pub route: u64,
    /// Milliseconds since the epoch, for the timeout.
    pub since: u64,
}

/// What to answer a `kvm.take` that arrived here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeDecision {
    /// A duplicate of the handover that is already in progress: answer nothing.
    Ignore,
    /// Refuse with this reason (`kvm.refuse {route, why}`).
    Refuse(&'static str),
    /// Take the pointer. `first` says whether this device was idle until now, which is when held
    /// keys and buttons must be released before it starts receiving input.
    Accept { first: bool },
}

fn home_mode() -> String {
    "home".to_string()
}

impl Default for MeshState {
    fn default() -> Self {
        Self {
            active: false,
            partner: None,
            role: home_mode(),
            route: 0,
            last_refusal: None,
            sessions: Vec::new(),
            reckon: 0,
            known_links: Vec::new(),
            peer_route: 0,
            pending: None,
        }
    }
}

impl MeshState {
    /// The session with this id, if one is connected.
    pub fn session(&self, id: &str) -> Option<&MeshSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn session_mut(&mut self, id: &str) -> Option<&mut MeshSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// The session that a socket address belongs to.
    ///
    /// A packet says where it came from, not who. The `hello` that established the link is what
    /// gave the device its id, and it is the id every role in here uses, so a handover arriving
    /// from a socket has to be resolved back to it. Matches the full `ip:port` and the bare IP, the
    /// two spellings the connection registry and older state use.
    pub fn session_by_address(&self, address: &str) -> Option<&MeshSession> {
        if address.is_empty() {
            return None;
        }
        self.sessions.iter().find(|s| {
            s.address.as_deref().map(|a| a == address
                || a.split(':').next().map(|host| host == address).unwrap_or(false)
                || address.split(':').next().map(|host| host == a).unwrap_or(false))
                .unwrap_or(false)
        })
    }

    /// Recompute `active` from the registry. Call after any change to `sessions`.
    pub fn refresh_active(&mut self) {
        self.active = super::mesh_active(self.sessions.len());
    }

    /// The device behind a handover right now, as an id.
    pub fn partner_id(&self) -> Option<&str> {
        self.partner.as_deref()
    }

    /// Give `id` the role and remember it as the partner; clears every other session's role so the
    /// "at most one" invariant holds. Idempotent.
    pub fn set_partner(&mut self, id: &str, role: Role) -> bool {
        if self.session(id).is_none() {
            return false;
        }
        for s in self.sessions.iter_mut() {
            s.role = if s.id == id { role } else { Role::Idle };
        }
        self.partner = Some(id.to_string());
        self.role = match role {
            Role::Driving => "driving".to_string(),
            Role::Driven => "driven".to_string(),
            Role::Idle => "home".to_string(),
        };
        true
    }

    /// Nobody drives or is driven any more. Idempotent.
    pub fn clear_partner(&mut self) {
        for s in self.sessions.iter_mut() {
            s.role = Role::Idle;
        }
        self.partner = None;
        self.role = "home".to_string();
    }

    /// Add a device, or update the one already there (a reconnect keeps its session).
    ///
    /// Returns false when the id is empty, is our own device, or the registry is full
    /// (`MAX_DEVICES`). The caller is the connection path, which is what refuses the
    /// connection; this only records it.
    pub fn upsert(&mut self, fresh: MeshSession, self_id: &str) -> bool {
        if fresh.id.is_empty() || fresh.id == self_id {
            return false;
        }
        if let Some(existing) = self.session_mut(&fresh.id) {
            // Keep what the handover knows; take the newly observed connection facts.
            let role = existing.role;
            let route = existing.route;
            *existing = fresh;
            existing.role = role;
            existing.route = route;
            self.refresh_active();
            return true;
        }
        if self.sessions.len() >= super::MAX_DEVICES {
            return false;
        }
        self.sessions.push(fresh);
        self.reckon += 1;
        self.refresh_active();
        true
    }

    /// Forget a connected device, by id or by address (the disconnect paths know either).
    /// Clears the partner if it was this device, so no session is left holding a role.
    pub fn remove(&mut self, id_or_address: &str) -> bool {
        let before = self.sessions.len();
        let mut removed_id = None;
        self.sessions.retain(|s| {
            let hit = s.id == id_or_address
                || s.address.as_deref() == Some(id_or_address)
                || s.address
                    .as_deref()
                    .and_then(|a| a.split(':').next())
                    .map(|host| host == id_or_address)
                    .unwrap_or(false);
            if hit {
                removed_id = Some(s.id.clone());
            }
            !hit
        });
        if self.sessions.len() == before {
            return false;
        }
        if let Some(id) = removed_id {
            if self.partner.as_deref() == Some(id.as_str()) {
                self.clear_partner();
            }
            self.known_links.retain(|x| x != &id);
            self.reckon += 1;
        }
        self.refresh_active();
        true
    }

    /// Note that a device is still alive. Cheap: only the session list is touched.
    pub fn touch(&mut self, id: &str, now: u64) {
        if let Some(s) = self.session_mut(id) {
            s.last_seen = now;
        }
    }

    /// Record why a crossing or a copy did not happen, for the page to show (`FR-023`).
    pub fn refusal(&mut self, device: &str, why: &str) {
        self.last_refusal = Some(Refusal {
            device: device.to_string(),
            why: why.to_string(),
            at: now_ms(),
        });
    }

    /// The session currently driven by another device, if any.
    pub fn driven_by(&self) -> Option<&MeshSession> {
        self.sessions.iter().find(|s| s.role == Role::Driven)
    }

    /// True while a given device may send input here.
    pub fn is_driven_by(&self, id: &str) -> bool {
        self.sessions
            .iter()
            .any(|s| s.id == id && s.role == Role::Driven)
    }

    // ---- The handover, from the driver's side ------------------------------------------------

    /// Record that this device is taking `target`, and wait for it to answer.
    ///
    /// The pointer is with the target from this moment, exactly as the single-peer app has always
    /// done: the person's hand has already left the edge, and holding the input here until the
    /// answer arrives would make the first movement land on the wrong screen. The answer only
    /// decides whether the handover stays or goes home.
    ///
    /// Returns false when the target is not connected: a take towards a device this machine has no
    /// session with can never be answered.
    pub fn begin_take(&mut self, target: &str, route: u64, now: u64) -> bool {
        if !self.set_partner(target, Role::Driving) {
            return false;
        }
        self.route = self.route.max(route);
        self.pending = Some(PendingTake {
            target: target.to_string(),
            route,
            since: now,
        });
        true
    }

    /// The take still waiting for an answer, if any.
    pub fn pending_take(&self) -> Option<&PendingTake> {
        self.pending.as_ref()
    }

    /// `kvm.accept` from the device we took. Only the current take can be confirmed, so a late
    /// answer to an older route cannot keep a handover alive that has already come home.
    pub fn confirm_take(&mut self, from: &str, route: u64) -> bool {
        match &self.pending {
            Some(p) if p.target == from && p.route <= route => {
                self.pending = None;
                self.route = self.route.max(route);
                true
            }
            _ => false,
        }
    }

    /// `kvm.refuse` from the device we took: nobody is driving anything any more.
    ///
    /// Returns false for an answer to a take that is already settled, which is what stops a refusal
    /// that arrives after the person went home by hand from taking the pointer away a second time.
    pub fn refuse_take(&mut self, from: &str, route: u64) -> bool {
        match &self.pending {
            Some(p) if p.target == from && p.route <= route => {
                self.pending = None;
                self.clear_partner();
                true
            }
            None => false,
            Some(_) => false,
        }
    }

    /// True when the take that is waiting has been waiting longer than `timeout_ms`.
    ///
    /// This is the reference's `KVM_TAKE_TIMEOUT_MS`: a device that does not answer (asleep, gone,
    /// wedged) must not leave the pointer on a screen that does not move.
    pub fn take_expired(&self, now: u64, timeout_ms: u64) -> bool {
        match &self.pending {
            Some(p) => now.saturating_sub(p.since) >= timeout_ms,
            None => false,
        }
    }

    /// The answer to give a `kvm.take` that arrived from `from` with `route`.
    ///
    /// The rules are the reference's, in its order, because the order is the reason no device is
    /// ever left believing it has control it does not have:
    ///
    /// 1. A device that is already driving someone must answer "busy": its own pointer is not here.
    /// 2. A device that is already driven refuses a second driver.
    /// 3. A device the person has not allowed to drive it refuses.
    /// 4. A take that does not advance the route already in use is a duplicate and is ignored.
    ///
    /// `allowed` is the person's "let the other computer drive this one", already combined with
    /// whether input sharing is on. A refusal is always sent, never silence, so the other side goes
    /// home at once instead of waiting for the timeout.
    pub fn decide_take(&self, from: &str, route: u64, allowed: bool) -> TakeDecision {
        match self.role.as_str() {
            "driving" => return TakeDecision::Refuse("busy"),
            "driven" => {
                if self.partner.as_deref() != Some(from) {
                    return TakeDecision::Refuse("busy");
                }
            }
            _ => {}
        }
        if !allowed {
            return TakeDecision::Refuse("not_allowed");
        }
        if self.role == "driven" && route <= self.peer_route {
            return TakeDecision::Ignore;
        }
        TakeDecision::Accept {
            first: self.role != "driven",
        }
    }

    /// Take the pointer because `from` asked for it (`decide_take` said `Accept`).
    pub fn accept_take(&mut self, from: &str, route: u64) -> bool {
        if !self.set_partner(from, Role::Driven) {
            return false;
        }
        self.peer_route = self.peer_route.max(route);
        true
    }

    /// `kvm.release`: the device driving this one is done, so this device goes home.
    pub fn release(&mut self, from: &str, route: u64) -> bool {
        if self.role != "driven" || self.partner.as_deref() != Some(from) {
            return false;
        }
        if route < self.peer_route {
            return false;
        }
        self.clear_partner();
        true
    }

    /// `kvm.home` from the device being driven: the person there took control back, so the device
    /// that was driving is not driving any more either.
    ///
    /// Only the device we took can do this, and only for the handover in progress: a late message
    /// from a device that was driven a while ago must not take the pointer away from the device
    /// driving now.
    pub fn home_from_driven(&mut self, from: &str, route: u64) -> bool {
        if self.role != "driving" || self.partner.as_deref() != Some(from) {
            return false;
        }
        if route < self.route {
            return false;
        }
        self.pending = None;
        self.clear_partner();
        true
    }

    /// The person at this device took control back by hand (`FR-011`).
    ///
    /// The device that was driving is told separately (`kvm.home`, which it answers by going home
    /// too); this is the local half: the role, the partner and any take still waiting go, so the
    /// page stops saying "driven" and nothing arriving late is accepted as input the person no
    /// longer wants. The route counter is not reset: it only ever grows, and a stale message must
    /// still be recognisable as stale.
    pub fn local_takeover(&mut self) -> bool {
        if self.role == "home" && self.partner.is_none() && self.pending.is_none() {
            return false;
        }
        self.pending = None;
        self.clear_partner();
        true
    }
}

/// Milliseconds since the epoch, the unit used everywhere in this feature.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> MeshSession {
        MeshSession::connected(id, id, "mac", None, 1_000)
    }

    #[test]
    fn role_names_match_the_wire() {
        assert_eq!(Role::Idle.as_str(), "idle");
        assert_eq!(Role::Driving.as_str(), "driving");
        assert_eq!(Role::Driven.as_str(), "driven");
    }

    #[test]
    fn new_session_is_idle_legacy_and_pooling() {
        let s = session("peer");
        assert_eq!(s.role, Role::Idle);
        assert!(!s.mesh, "an unknown peer is treated as an older app until it says otherwise");
        assert!(s.pool, "the pool is on by default");
        assert_eq!(s.route, 0);
        assert!(!s.is_phone());
    }

    #[test]
    fn phones_are_recognised_by_system() {
        assert!(MeshSession::connected("p", "Phone", "Android", None, 0).is_phone());
        assert!(MeshSession::connected("p", "Phone", "ios", None, 0).is_phone());
        assert!(!MeshSession::connected("m", "Mac", "mac", None, 0).is_phone());
    }

    #[test]
    fn only_one_partner_at_a_time() {
        let mut m = MeshState::default();
        m.sessions.push(session("b"));
        m.sessions.push(session("c"));
        m.refresh_active();
        assert!(m.active);

        assert!(m.set_partner("b", Role::Driving));
        assert_eq!(m.partner.as_deref(), Some("b"));
        assert_eq!(m.role, "driving");
        assert_eq!(m.session("b").unwrap().role, Role::Driving);

        // Taking a different device clears the first one's role.
        assert!(m.set_partner("c", Role::Driving));
        assert_eq!(m.session("b").unwrap().role, Role::Idle);
        assert_eq!(m.session("c").unwrap().role, Role::Driving);

        m.clear_partner();
        assert!(m.partner.is_none());
        assert_eq!(m.role, "home");
        assert!(m.driven_by().is_none());
        assert!(!m.is_driven_by("c"));
    }

    #[test]
    fn a_disconnected_device_cannot_be_the_partner() {
        let mut m = MeshState::default();
        m.sessions.push(session("b"));
        assert!(!m.set_partner("gone", Role::Driving));
        assert!(m.partner.is_none());
        assert_eq!(m.role, "home");
    }

    #[test]
    fn active_follows_the_registry() {
        let mut m = MeshState::default();
        assert!(!m.active);
        m.sessions.push(session("b"));
        m.refresh_active();
        assert!(!m.active, "one device is the two-device app");
        m.sessions.push(session("c"));
        m.refresh_active();
        assert!(m.active);
        m.sessions.clear();
        m.refresh_active();
        assert!(!m.active);
    }

    #[test]
    fn upsert_refuses_our_own_id_and_keeps_the_handover_state() {
        let mut m = MeshState::default();
        assert!(!m.upsert(session("me"), "me"));
        assert!(m.sessions.is_empty());

        assert!(m.upsert(session("b"), "me"));
        m.set_partner("b", Role::Driving);
        m.session_mut("b").unwrap().route = 4;

        // A reconnect of the same device updates the facts but must not forget who is driving.
        let mut again = MeshSession::connected("b", "Renamed", "win", Some("10.0.0.2".into()), 5_000);
        again.mesh = true;
        assert!(m.upsert(again, "me"));
        assert_eq!(m.sessions.len(), 1);
        let b = m.session("b").unwrap();
        assert_eq!(b.name, "Renamed");
        assert_eq!(b.os, "win");
        assert_eq!(b.route, 4, "the handover route is not reset by a reconnect");
        assert_eq!(b.role, Role::Driving, "a reconnect does not stop the pointer");
        assert!(b.mesh);
    }

    #[test]
    fn upsert_stops_at_the_device_limit() {
        let mut m = MeshState::default();
        for i in 0..super::super::MAX_DEVICES {
            assert!(m.upsert(session(&format!("d{}", i)), "me"));
        }
        assert!(!m.upsert(session("eleventh"), "me"));
        assert_eq!(m.sessions.len(), super::super::MAX_DEVICES);
        // A device already in the list can still be updated when the list is full.
        assert!(m.upsert(session("d0"), "me"));
    }

    #[test]
    fn removing_the_partner_returns_home() {
        let mut m = MeshState::default();
        m.upsert(session("b"), "me");
        m.upsert(session("c"), "me");
        m.set_partner("b", Role::Driven);
        assert!(m.is_driven_by("b"));

        assert!(m.remove("b"));
        assert!(!m.is_driven_by("b"));
        assert!(m.partner.is_none());
        assert_eq!(m.role, "home");
        assert_eq!(m.sessions.len(), 1);

        // Removing something that is not there changes nothing.
        assert!(!m.remove("b"));
    }

    #[test]
    fn a_device_can_be_removed_by_its_address() {
        let mut m = MeshState::default();
        let mut b = session("b");
        b.address = Some("192.168.4.11".to_string());
        m.upsert(b, "me");
        assert!(m.remove("192.168.4.11"));
        assert!(m.sessions.is_empty());
    }
    fn mesh_with(ids: &[&str]) -> MeshState {
        let mut m = MeshState::default();
        for id in ids {
            m.upsert(session(id), "me");
        }
        m
    }

    /// US2: a take that does not advance the route is a duplicate, and a take that is not answered
    /// goes home after 1000 ms.
    #[test]
    fn a_take_that_does_not_advance_the_route_is_ignored() {
        let mut m = mesh_with(&["a", "b"]);
        // The first take is accepted whatever its route: there is no handover to compare with.
        assert_eq!(
            m.decide_take("a", 3, true),
            TakeDecision::Accept { first: true }
        );
        assert!(m.accept_take("a", 3));

        // From then on, only a route that moves forward is a new handover. The same one, or an
        // older one arriving late, is a duplicate and gets no answer at all.
        assert_eq!(m.decide_take("a", 3, true), TakeDecision::Ignore);
        assert_eq!(m.decide_take("a", 2, true), TakeDecision::Ignore);
        assert_eq!(
            m.decide_take("a", 4, true),
            TakeDecision::Accept { first: false }
        );
    }

    #[test]
    fn a_take_with_no_answer_returns_home() {
        let mut m = mesh_with(&["a", "b"]);
        assert!(m.begin_take("a", 1, 10_000));
        assert_eq!(m.partner.as_deref(), Some("a"));
        assert_eq!(m.role, "driving");
        assert!(!m.take_expired(10_999, super::super::TAKE_TIMEOUT_MS));
        assert!(m.take_expired(11_000, super::super::TAKE_TIMEOUT_MS));

        // The answered take is not expired, whatever the clock says.
        assert!(m.confirm_take("a", 1));
        assert!(m.pending_take().is_none());
        assert!(!m.take_expired(99_999, super::super::TAKE_TIMEOUT_MS));
        assert_eq!(m.role, "driving", "an accepted take keeps the pointer there");
    }

    #[test]
    fn a_refusal_after_going_home_changes_nothing() {
        let mut m = mesh_with(&["a", "b"]);
        assert!(m.begin_take("a", 1, 0));
        assert!(m.refuse_take("a", 1));
        assert_eq!(m.role, "home");

        // A second refusal, or one for a take that never existed, is not a second handover.
        assert!(!m.refuse_take("a", 1));
        assert!(!m.refuse_take("b", 9));
        assert_eq!(m.role, "home");
    }

    #[test]
    fn two_devices_cannot_take_at_once() {
        let mut m = mesh_with(&["a", "b", "c"]);

        // This device is driving a: another take is refused, because its pointer is not here.
        assert!(m.begin_take("a", 1, 0));
        assert_eq!(m.decide_take("b", 1, true), TakeDecision::Refuse("busy"));

        // Driven by b: a second driver is refused, and b's own duplicate is ignored.
        m.clear_partner();
        assert!(m.accept_take("b", 5));
        assert_eq!(m.role, "driven");
        assert_eq!(m.decide_take("c", 1, true), TakeDecision::Refuse("busy"));
        assert_eq!(m.decide_take("b", 5, true), TakeDecision::Ignore);
        // A refresh from the same driver on a newer route is accepted.
        assert_eq!(
            m.decide_take("b", 6, true),
            TakeDecision::Accept { first: false }
        );
    }

    #[test]
    fn a_device_that_does_not_allow_it_refuses() {
        let mut m = mesh_with(&["a", "b"]);
        assert_eq!(
            m.decide_take("a", 1, false),
            TakeDecision::Refuse("not_allowed")
        );
        assert!(m.accept_take("a", 1));
        // The refusal codes are the ones the wire carries (`contracts/wire.md`, `why`).
        assert_eq!(
            m.decide_take("b", 1, true),
            TakeDecision::Refuse("busy"),
            "a device that is already driven cannot be driven by a second one"
        );
    }

    #[test]
    fn release_returns_home_only_for_the_device_driving_this_one() {
        let mut m = mesh_with(&["a", "b"]);
        assert!(m.accept_take("a", 7));
        assert!(!m.release("b", 7), "only the driver ends its own handover");
        assert_eq!(m.role, "driven");
        assert!(!m.release("a", 6), "an older route cannot end a newer handover");
        assert!(m.release("a", 7));
        assert_eq!(m.role, "home");
        assert!(m.partner.is_none());
    }

    /// `FR-011`: the person at a driven device takes control back by hand. The device that was
    /// driving is told, and here nothing is left believing this device is still driven.
    #[test]
    fn taking_control_back_by_hand_gives_up_the_handover() {
        let mut m = mesh_with(&["a", "b"]);
        assert!(m.accept_take("a", 4));
        assert_eq!(m.role, "driven");
        assert!(m.is_driven_by("a"));

        assert!(m.local_takeover());
        assert_eq!(m.role, "home");
        assert!(m.partner.is_none());
        assert!(!m.is_driven_by("a"));
        // The route counter keeps its place, so a late take from the old handover is still
        // recognisable as the one that already ended.
        assert_eq!(m.peer_route, 4);

        // Nothing to give up is not a change.
        assert!(!m.local_takeover());

        // A take still waiting is given up too: a crossing the person cancelled by hand must not
        // be answered a second later and pull the pointer away.
        assert!(m.begin_take("b", 9, 0));
        assert!(m.pending_take().is_some());
        assert!(m.local_takeover());
        assert!(m.pending_take().is_none());
        assert_eq!(m.role, "home");
    }

    #[test]
    fn a_take_towards_a_device_that_is_not_connected_cannot_start() {
        let mut m = mesh_with(&["a"]);
        assert!(!m.begin_take("gone", 1, 0));
        assert_eq!(m.role, "home");
        assert!(m.pending_take().is_none());
    }

}
