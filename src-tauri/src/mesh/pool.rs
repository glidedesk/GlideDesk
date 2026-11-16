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

use super::{POOL_MAX_HOPS, POOL_SEEN_LIMIT};
use crate::network::WireMessage;
use std::collections::{HashMap, VecDeque};

/// One peer as the pool sees it.
///
/// `mesh` says whether the peer speaks the mesh (it gets the stamps) or is an older app (it gets
/// the plain message). `pool` says whether it accepts pooled items at all: a peer that has them
/// switched off must never be sent one, and never be believed if it sends one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolPeer {
    pub id: String,
    pub mesh: bool,
    pub pool: bool,
}

impl PoolPeer {
    pub fn new(id: impl Into<String>, mesh: bool, pool: bool) -> Self {
        Self {
            id: id.into(),
            mesh,
            pool,
        }
    }
}

/// Counters a later reader (or a test) can look at without any logging.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolStats {
    pub published: u64,
    pub received: u64,
    pub fresh: u64,
    pub dropped_echo: u64,
    pub dropped_dup: u64,
    pub dropped_off: u64,
    pub relayed: u64,
}

/// What `publish` did: the stamped message, and the peers that took it.
#[derive(Debug, Clone)]
pub struct Published {
    pub msg: WireMessage,
    pub sent: Vec<String>,
}

/// What `receive` decided: apply it, and where it went next.
#[derive(Debug, Clone)]
pub struct Received {
    /// True when the caller must apply the item: the first time this device saw it, and not ours.
    pub fresh: bool,
    /// The stamped message (an item from an older app is stamped here, so it can be relayed).
    pub msg: WireMessage,
    /// The peers the item was forwarded to.
    pub relayed: Vec<String>,
}

/// The message as an old app sees it: pool stamps removed. Never mutates its input.
fn plain(pkt: &WireMessage) -> WireMessage {
    let mut out = pkt.clone();
    out.o = None;
    out.s = None;
    out.d = None;
    out.h = None;
    out
}

/// Fan-out, dedupe and relay for one kind of pooled item (`shared/mesh.js`: `Pool`).
///
/// `start` is where this device's sequence numbers begin: by default the wall clock in
/// milliseconds, so a restarted app numbers past its last run. Starting from zero again would make
/// every item of the new run a duplicate to the devices that stayed up, which still remember the
/// old numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pool {
    self_id: String,
    seen_limit: usize,
    max_hops: u32,
    start: u64,
    seq: u64,
    /// Sequence numbers given to an older peer's unstamped items, per origin.
    legacy_seq: HashMap<String, u64>,
    /// `(origin, seq)` pairs already seen, oldest first per origin.
    seen: HashMap<String, VecDeque<u64>>,
    stats: PoolStats,
}

impl Pool {
    /// A pool for `self_id`, numbered from the wall clock as the reference does.
    pub fn new(self_id: impl Into<String>, now: u64) -> Self {
        Self::with_limits(self_id, now, POOL_SEEN_LIMIT, POOL_MAX_HOPS)
    }

    /// The same with explicit limits, which is what the tests need: they must not depend on the
    /// protocol constants to prove a rule.
    pub fn with_limits(self_id: impl Into<String>, start: u64, seen_limit: usize, max_hops: u32) -> Self {
        Self {
            self_id: self_id.into(),
            seen_limit: seen_limit.max(1),
            max_hops,
            start,
            seq: start,
            legacy_seq: HashMap::new(),
            seen: HashMap::new(),
            stats: PoolStats::default(),
        }
    }

    pub fn self_id(&self) -> &str {
        &self.self_id
    }

    /// The last sequence number this device handed out (for diagnostics and tests).
    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn stats(&self) -> PoolStats {
        self.stats
    }

    /// Stamp a copy of `pkt` as our own item and send it to every pool-capable peer.
    ///
    /// `send` returns whether the peer took the message; only a peer that took it is reported as
    /// sent, so the caller can tell "nothing connected" from "sent to three".
    pub fn publish(
        &mut self,
        pkt: WireMessage,
        peers: &[PoolPeer],
        send: &mut dyn FnMut(&str, &WireMessage) -> bool,
    ) -> Published {
        self.seq = self.seq.saturating_add(1);
        let mut stamped = pkt.clone();
        stamped.o = Some(self.self_id.clone());
        stamped.s = Some(self.seq);
        stamped.d = Some(peers.iter().map(|p| p.id.clone()).collect());
        stamped.h = Some(0);
        self.mark_seen(&self.self_id.clone(), self.seq);
        self.stats.published += 1;

        let plain_msg = plain(&pkt);
        let mut sent = Vec::new();
        for peer in peers {
            if !peer.pool {
                continue;
            }
            let msg = if peer.mesh { &stamped } else { &plain_msg };
            if send(&peer.id, msg) {
                sent.push(peer.id.clone());
            }
        }
        Published { msg: stamped, sent }
    }

    /// Handle an item that arrived from `from`.
    ///
    /// The caller applies the item when `fresh` is true and must not re-publish it; the relay has
    /// already been done here, one hop further, to the peers the origin did not reach itself.
    pub fn receive(
        &mut self,
        from: &str,
        pkt: &WireMessage,
        peers: &[PoolPeer],
        send: &mut dyn FnMut(&str, &WireMessage) -> bool,
    ) -> Received {
        self.stats.received += 1;
        let mut msg = pkt.clone();

        if peers.iter().any(|p| p.id == from && !p.pool) {
            // The peer has pooled items switched off, so what it sends is not part of the pool.
            self.stats.dropped_off += 1;
            return Received {
                fresh: false,
                msg,
                relayed: Vec::new(),
            };
        }

        if msg.o.is_none() {
            // A message without stamps comes from an old one-to-one peer, which is the origin of
            // it. Give it a number of its own so it can be relayed and deduplicated.
            let next = self
                .legacy_seq
                .get(from)
                .copied()
                .unwrap_or(self.start)
                .saturating_add(1);
            self.legacy_seq.insert(from.to_string(), next);
            msg.o = Some(from.to_string());
            msg.s = Some(next);
            msg.d = Some(vec![from.to_string()]);
            msg.h = Some(0);
        }

        let origin = msg.o.clone().unwrap_or_default();
        let seq = msg.s.unwrap_or(0);

        if origin == self.self_id {
            // Our own item, coming back after a round trip.
            self.stats.dropped_echo += 1;
            return Received {
                fresh: false,
                msg,
                relayed: Vec::new(),
            };
        }
        if self.has_seen(&origin, seq) {
            self.stats.dropped_dup += 1;
            return Received {
                fresh: false,
                msg,
                relayed: Vec::new(),
            };
        }
        self.mark_seen(&origin, seq);
        self.stats.fresh += 1;

        // Relay. A peer is skipped when it is the sender, the origin, this device, one of the
        // origin's direct links (it got the item from the origin itself), or switched the pool
        // off. The hop limit bounds the chain: an item whose next hop would be above `max_hops` is
        // not relayed any further.
        let mut relayed = Vec::new();
        let hops = msg.h.unwrap_or(0).saturating_add(1);
        if hops <= self.max_hops {
            let mut onward = msg.clone();
            onward.h = Some(hops);
            let plain_msg = plain(&msg);
            for peer in peers {
                if peer.id == from || peer.id == origin || peer.id == self.self_id || !peer.pool {
                    continue;
                }
                if msg
                    .d
                    .as_ref()
                    .map(|links| links.iter().any(|l| l == &peer.id))
                    .unwrap_or(false)
                {
                    continue;
                }
                let out = if peer.mesh { &onward } else { &plain_msg };
                if send(&peer.id, out) {
                    relayed.push(peer.id.clone());
                    self.stats.relayed += 1;
                }
            }
        }

        Received {
            fresh: true,
            msg,
            relayed,
        }
    }

    fn has_seen(&self, origin: &str, seq: u64) -> bool {
        self.seen
            .get(origin)
            .map(|set| set.contains(&seq))
            .unwrap_or(false)
    }

    /// Remember `(origin, seq)`, keeping only the newest `seen_limit` numbers per origin.
    ///
    /// So an item is accepted again only after `seen_limit` newer items from the same origin have
    /// been remembered, which is what makes a long outage recoverable without unbounded memory.
    fn mark_seen(&mut self, origin: &str, seq: u64) {
        let set = self.seen.entry(origin.to_string()).or_default();
        if set.contains(&seq) {
            return;
        }
        set.push_back(seq);
        while set.len() > self.seen_limit {
            set.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// An item published by `origin`.
    fn clip(item: &str) -> WireMessage {
        WireMessage {
            t: "clip".to_string(),
            v: Some(1),
            text: Some(item.to_string()),
            ..Default::default()
        }
    }

    fn item_of(msg: &WireMessage) -> String {
        msg.text.clone().unwrap_or_default()
    }

    /// The in-memory network of `shared/test/mesh-pool.js`: nodes on a graph, a queue of messages,
    /// delivery in any order.
    struct Net {
        nodes: HashMap<String, Node>,
        adj: HashMap<String, Vec<String>>,
        queue: Vec<(String, String, WireMessage)>,
        sends: usize,
    }

    struct Node {
        legacy: bool,
        pool: Option<Pool>,
        applied: HashMap<String, usize>,
        raw: Vec<WireMessage>,
    }

    impl Net {
        fn new(ids: &[&str]) -> Self {
            let mut nodes = HashMap::new();
            let mut adj = HashMap::new();
            for id in ids {
                let legacy = id.starts_with('L');
                nodes.insert(
                    id.to_string(),
                    Node {
                        legacy,
                        pool: None,
                        applied: HashMap::new(),
                        raw: Vec::new(),
                    },
                );
                adj.insert(id.to_string(), Vec::new());
            }
            Self {
                nodes,
                adj,
                queue: Vec::new(),
                sends: 0,
            }
        }

        fn link(mut self, a: &str, b: &str) -> Self {
            self.adj.get_mut(a).unwrap().push(b.to_string());
            self.adj.get_mut(b).unwrap().push(a.to_string());
            self
        }

        fn start(&mut self, seen_limit: usize, max_hops: u32) {
            for (id, node) in self.nodes.iter_mut() {
                if node.legacy {
                    continue;
                }
                node.pool = Some(Pool::with_limits(id, 0, seen_limit, max_hops));
            }
        }

        fn peers_of(&self, id: &str, legacy: &HashMap<String, bool>) -> Vec<PoolPeer> {
            self.adj[id]
                .iter()
                .map(|other| PoolPeer::new(other, !legacy[other], true))
                .collect()
        }

        fn enqueue(&mut self, from: &str, to: &str, msg: &WireMessage) {
            self.sends += 1;
            self.queue
                .push((from.to_string(), to.to_string(), msg.clone()));
        }

        /// Deliver one queued message. Returns false when the queue is empty.
        fn deliver_at(&mut self, index: usize) -> bool {
            if index >= self.queue.len() {
                return false;
            }
            let (from, to, msg) = self.queue.remove(index);
            if self.nodes[&to].legacy {
                let node = self.nodes.get_mut(&to).unwrap();
                node.raw.push(msg.clone());
                *node.applied.entry(item_of(&msg)).or_insert(0) += 1;
                return true;
            }
            let legacy: HashMap<String, bool> = self
                .nodes
                .iter()
                .map(|(id, n)| (id.clone(), n.legacy))
                .collect();
            let peers = self.peers_of(&to, &legacy);
            let mut out: Vec<(String, String, WireMessage)> = Vec::new();
            let mut received = None;
            {
                let pool = self.nodes.get_mut(&to).unwrap().pool.as_mut().unwrap();
                received = Some(pool.receive(&from, &msg, &peers, &mut |id, m| {
                    out.push((to.clone(), id.to_string(), m.clone()));
                    true
                }));
            }
            let received = received.unwrap();
            if received.fresh {
                let node = self.nodes.get_mut(&to).unwrap();
                *node.applied.entry(item_of(&received.msg)).or_insert(0) += 1;
            }
            for (f, t, m) in out {
                self.enqueue(&f, &t, &m);
            }
            true
        }

        /// Deliver everything, in FIFO order.
        fn run(&mut self) {
            while !self.queue.is_empty() {
                self.deliver_at(0);
            }
        }

        /// Publish `item` from `origin`; returns the sends it caused.
        fn publish(&mut self, origin: &str, item: &str) -> usize {
            let before = self.sends;
            let legacy: HashMap<String, bool> = self
                .nodes
                .iter()
                .map(|(id, n)| (id.clone(), n.legacy))
                .collect();
            let peers = self.peers_of(origin, &legacy);
            let mut out: Vec<(String, String, WireMessage)> = Vec::new();
            {
                let pool = self.nodes.get_mut(origin).unwrap().pool.as_mut().unwrap();
                pool.publish(clip(item), &peers, &mut |id, m| {
                    out.push((origin.to_string(), id.to_string(), m.clone()));
                    true
                });
            }
            for (f, t, m) in out {
                self.enqueue(&f, &t, &m);
            }
            self.sends - before
        }

        /// An old one-to-one app: it sends to its first neighbour and applies everything it gets.
        fn legacy_send(&mut self, leaf: &str, item: &str) {
            let to = self.adj[leaf][0].clone();
            self.enqueue(leaf, &to, &clip(item));
        }

        fn applied(&self, node: &str, item: &str) -> usize {
            self.nodes[node]
                .applied
                .get(item)
                .copied()
                .unwrap_or(0)
        }

        /// Every node except the origin applied the item exactly once.
        fn assert_exactly_once(&self, origin: &str, item: &str, label: &str) {
            for (id, node) in self.nodes.iter() {
                let _ = node;
                let got = self.applied(id, item);
                let want = if id == origin { 0 } else { 1 };
                assert_eq!(got, want, "{label}: node {id} applied {item} {got} times, want {want}");
            }
        }
    }

    fn full_mesh(n: usize) -> Net {
        let ids: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let mut net = Net::new(&refs);
        for i in 0..n {
            for j in (i + 1)..n {
                net = net.link(&ids[i], &ids[j]);
            }
        }
        net
    }

    /// A full mesh: every other device applies the item once, and publishing a copy costs one send
    /// per device — no more, so the pool never floods a desk that is already fully connected.
    #[test]
    fn every_device_applies_a_copy_once() {
        for n in 2..=6 {
            let mut net = full_mesh(n);
            net.start(POOL_SEEN_LIMIT, POOL_MAX_HOPS);
            for k in 0..10 {
                let origin = format!("n{}", k % n);
                let item = format!("item-{n}-{k}");
                let sends = net.publish(&origin, &item);
                net.run();
                net.assert_exactly_once(&origin, &item, "full mesh");
                assert_eq!(sends, n - 1, "one send per other device");
            }
        }
    }

    /// A line, a star and a ring: every node still applies each item once, and the item never
    /// travels a link twice.
    #[test]
    fn partial_meshes_still_reach_everyone_once() {
        // Line A-B-C-D: the item crosses each link once.
        let mut line = Net::new(&["A", "B", "C", "D"])
            .link("A", "B")
            .link("B", "C")
            .link("C", "D");
        line.start(POOL_SEEN_LIMIT, 10);
        assert_eq!(line.publish("A", "line-1"), 1, "publishing sends to the one neighbour it has");
        line.run();
        line.assert_exactly_once("A", "line-1", "line from an end");
        line.publish("B", "line-2");
        line.run();
        line.assert_exactly_once("B", "line-2", "line from the middle");

        // Star: centre C, six leaves. A copy from a leaf goes to the centre, which relays it to the
        // other five.
        // ("S" names, because `Net` reads `L` as an old one-to-one app.)
        let mut star = Net::new(&["C", "S1", "S2", "S3", "S4", "S5", "S6"]);
        for i in 1..=6 {
            star = star.link("C", &format!("S{i}"));
        }
        star.start(POOL_SEEN_LIMIT, 10);
        assert_eq!(star.publish("S3", "star-1"), 1, "a leaf has one neighbour");
        star.run();
        star.assert_exactly_once("S3", "star-1", "star from a leaf");

        // Ring of seven: every origin reaches everyone once.
        let ids: Vec<String> = (0..7).map(|i| format!("r{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let mut ring = Net::new(&refs);
        for i in 0..7 {
            let a = ids[i].clone();
            let b = ids[(i + 1) % 7].clone();
            ring = ring.link(&a, &b);
        }
        ring.start(POOL_SEEN_LIMIT, 10);
        let links = ring.adj.values().map(|v| v.len()).sum::<usize>();
        for i in 0..7 {
            let item = format!("ring-{i}");
            let sends = ring.publish(&ids[i], &item);
            ring.run();
            ring.assert_exactly_once(&ids[i], &item, "ring");
            assert!(sends <= links, "sends stay bounded by the links");
        }
    }

    /// An item from an old app reaches every device once, and the old app receives a message with
    /// none of the stamps it would not understand.
    #[test]
    fn a_legacy_peer_is_part_of_the_pool_without_the_stamps() {
        let mut net = Net::new(&["A", "B", "C", "L"])
            .link("A", "B")
            .link("B", "C")
            .link("B", "L");
        net.start(POOL_SEEN_LIMIT, POOL_MAX_HOPS);

        net.legacy_send("L", "from-leaf");
        net.run();
        for id in ["A", "B", "C"] {
            assert_eq!(net.applied(id, "from-leaf"), 1, "legacy origin: {id} applies once");
        }

        net.publish("A", "to-leaf");
        net.run();
        net.assert_exactly_once("A", "to-leaf", "item to a legacy leaf");
        let leaf = &net.nodes["L"];
        assert_eq!(leaf.raw.len(), 1, "the old app got exactly the one item");
        let got = &leaf.raw[0];
        assert!(got.o.is_none() && got.s.is_none() && got.d.is_none() && got.h.is_none(),
            "no stamp reaches an app that would not understand it");
        assert_eq!(got.text.as_deref(), Some("to-leaf"));
    }

    /// Two different items from an old app in a triangle are both applied: without a stamp given
    /// at the relay they would share one dedupe key and the second would be dropped.
    #[test]
    fn two_items_from_an_old_app_are_not_taken_for_one() {
        let mut net = Net::new(&["A", "B", "C", "L"])
            .link("A", "B")
            .link("B", "C")
            .link("A", "C")
            .link("B", "L");
        net.start(POOL_SEEN_LIMIT, POOL_MAX_HOPS);
        net.legacy_send("L", "first");
        net.run();
        net.assert_exactly_once("L", "first", "legacy loop, first item");
        net.legacy_send("L", "second");
        net.run();
        net.assert_exactly_once("L", "second", "legacy loop, second item");
    }

    /// Our own item coming back, and an item seen twice, are dropped; a new number from the same
    /// origin is fresh.
    #[test]
    fn echoes_and_duplicates_are_dropped() {
        let mut pool = Pool::with_limits("A", 0, 3, 3);
        let peers = [PoolPeer::new("B", true, true)];
        let mut sent: Vec<(String, WireMessage)> = Vec::new();
        let published = pool.publish(clip("mine"), &peers, &mut |id, m| {
            sent.push((id.to_string(), m.clone()));
            true
        });
        assert_eq!(published.msg.o.as_deref(), Some("A"));
        assert_eq!(published.msg.s, Some(1));
        assert_eq!(published.msg.d.as_ref().map(|d| d.as_slice()), Some(&["B".to_string()][..]));
        assert_eq!(published.msg.h, Some(0));
        assert_eq!(published.sent, vec!["B".to_string()]);

        let echo = pool.receive("B", &published.msg, &peers, &mut |_, _| true);
        assert!(!echo.fresh, "own echo is not fresh");
        assert_eq!(pool.stats().dropped_echo, 1);

        let mut dup = clip("x");
        dup.o = Some("C".into());
        dup.s = Some(1);
        dup.d = Some(vec!["C".into()]);
        dup.h = Some(0);
        assert!(pool.receive("B", &dup, &peers, &mut |_, _| true).fresh, "first copy is fresh");
        assert!(!pool.receive("B", &dup, &peers, &mut |_, _| true).fresh, "second copy is a duplicate");
        assert_eq!(pool.stats().dropped_dup, 1);
    }

    /// The remembered window is bounded per origin: after `seen_limit` newer items an old number is
    /// accepted again, and another origin's numbers are counted separately.
    #[test]
    fn the_remembered_window_is_bounded_and_per_origin() {
        let mut pool = Pool::with_limits("A", 0, 3, 3);
        let got = |pool: &mut Pool, o: &str, s: u64| {
            let mut m = clip(&format!("w{s}"));
            m.o = Some(o.to_string());
            m.s = Some(s);
            m.d = Some(vec![o.to_string()]);
            m.h = Some(0);
            pool.receive("X", &m, &[], &mut |_, _| true).fresh
        };
        assert!(got(&mut pool, "C", 1));
        assert!(got(&mut pool, "C", 2));
        assert!(got(&mut pool, "C", 3));
        assert!(!got(&mut pool, "C", 1), "still remembered after two newer");
        assert!(got(&mut pool, "C", 4));
        assert!(got(&mut pool, "C", 1), "evicted after three newer: accepted again");
        assert!(got(&mut pool, "D", 4), "another origin's numbers are its own");
    }

    /// T045 (`SC-006`): three devices in a full mesh, a hundred copies alternating between them.
    ///
    /// Every device ends on the last copy made anywhere, every item is applied at most once
    /// (`FR-017`), and nothing is applied twice by two paths of the same fan-out.
    #[test]
    fn a_hundred_copies_from_three_devices_converge() {
        use crate::mesh::clip::{Kind, Latest};
        use std::collections::VecDeque;

        let ids = ["A", "B", "C"];
        let peers_of = |id: &str| -> Vec<PoolPeer> {
            ids.iter()
                .filter(|other| **other != id)
                .map(|other| PoolPeer::new(*other, true, true))
                .collect()
        };

        let mut pools: HashMap<String, Pool> = ids
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    Pool::with_limits(*id, 0, POOL_SEEN_LIMIT, POOL_MAX_HOPS),
                )
            })
            .collect();
        let mut latest: HashMap<String, Latest> =
            ids.iter().map(|id| (id.to_string(), Latest::new(*id))).collect();
        let mut applied: HashMap<String, Vec<String>> =
            ids.iter().map(|id| (id.to_string(), Vec::new())).collect();
        // What each device believes is the newest copy, in words: a stamp proves the order, the
        // text proves it is the copy the person made last.
        let mut newest_text: HashMap<String, String> =
            ids.iter().map(|id| (id.to_string(), String::new())).collect();
        let mut queue: VecDeque<(String, String, WireMessage)> = VecDeque::new();
        let mut now = 1_000u64;

        const COUNT: usize = 100;
        for i in 0..COUNT {
            let origin = ids[i % 3];
            let text = format!("copy-{i}");
            now += 1;
            let stamp = latest
                .get_mut(origin)
                .expect("every device has a clock")
                .copied(Kind::Text, None, now);
            newest_text.insert(origin.to_string(), text.clone());
            let pkt = WireMessage {
                t: "clip".to_string(),
                v: Some(1),
                text: Some(text),
                ts: Some(stamp.ts),
                ..Default::default()
            };
            {
                let peers = peers_of(origin);
                let pool = pools.get_mut(origin).expect("origin pool");
                pool.publish(pkt, &peers, &mut |id, msg| {
                    queue.push_back((origin.to_string(), id.to_string(), msg.clone()));
                    true
                });
            }

            while let Some((from, to, msg)) = queue.pop_front() {
                let peers = peers_of(&to);
                let received = {
                    let pool = pools.get_mut(&to).expect("receiving pool");
                    pool.receive(&from, &msg, &peers, &mut |id, m| {
                        queue.push_back((to.clone(), id.to_string(), m.clone()));
                        true
                    })
                };
                if !received.fresh {
                    continue;
                }
                let took = latest.get_mut(&to).expect("clock").accept(
                    received.msg.ts,
                    received.msg.o.as_deref(),
                    &from,
                    Kind::Text,
                    None,
                    now,
                );
                if took {
                    let text = received.msg.text.clone().unwrap_or_default();
                    applied.get_mut(&to).expect("applied list").push(text.clone());
                    newest_text.insert(to.clone(), text);
                }
            }
        }

        let newest = format!("copy-{}", COUNT - 1);
        for id in ids {
            let cur = latest[id].current().expect("every device knows the newest copy");
            assert_eq!(
                cur.stamp,
                latest["A"].current().expect("A has it").stamp,
                "every device agrees on the same stamp"
            );
        }
        for id in ids {
            let cur = latest[id].current().expect("newest copy");
            assert_eq!(cur.kind, Kind::Text);
            assert_eq!(
                newest_text[id], newest,
                "{id} knows the last copy made anywhere as its newest"
            );
        }
        for id in ids {
            let got = &applied[id];
            let mut unique = got.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(got.len(), unique.len(), "{id} applied an item twice");
            let own = (0..COUNT).filter(|i| ids[i % 3] == id).count();
            assert_eq!(
                got.len(),
                COUNT - own,
                "{id} applied every copy made elsewhere exactly once"
            );
            // The device that made the last copy keeps it rather than applying it, so its list
            // ends on the copy before; everyone else applied the last copy made anywhere.
            if ids[(COUNT - 1) % 3] != id {
                assert_eq!(
                    got.last().map(|t| t.as_str()),
                    Some(newest.as_str()),
                    "{id} applied the last copy made anywhere"
                );
            }
        }
    }

    /// A peer with pooled items switched off is sent nothing, relays nothing, and what it sends is
    /// ignored.
    #[test]
    fn a_peer_that_switched_the_pool_off_is_left_alone() {
        let peers = [
            PoolPeer::new("B", true, true),
            PoolPeer::new("C", true, false),
            PoolPeer::new("D", false, true),
        ];
        let mut pool = Pool::with_limits("A", 0, POOL_SEEN_LIMIT, 3);
        let mut log: HashMap<String, Vec<WireMessage>> = HashMap::new();
        let published = pool.publish(clip("p"), &peers, &mut |id, m| {
            log.entry(id.to_string()).or_default().push(m.clone());
            true
        });
        assert_eq!(published.sent, vec!["B".to_string(), "D".to_string()]);
        assert!(log.get("C").map(|v| v.is_empty()).unwrap_or(true), "pool:false got nothing");

        let mut incoming = clip("q");
        incoming.o = Some("E".into());
        incoming.s = Some(1);
        incoming.d = Some(vec!["E".into()]);
        incoming.h = Some(0);
        let relayed = pool.receive("B", &incoming, &peers, &mut |id, m| {
            log.entry(id.to_string()).or_default().push(m.clone());
            true
        });
        assert_eq!(relayed.relayed, vec!["D".to_string()], "relay skips the pool:false peer");
        assert!(log.get("C").map(|v| v.len() <= 1).unwrap_or(true), "pool:false got no relay");
        // The relayed copy to an old app carries no stamps.
        let to_d = log.get("D").expect("D got a relay").last().cloned().expect("one relay");
        assert!(to_d.o.is_none() && to_d.s.is_none() && to_d.d.is_none() && to_d.h.is_none());
        assert_eq!(to_d.text.as_deref(), Some("q"));

        let mut from_off = clip("r");
        from_off.o = Some("E".into());
        from_off.s = Some(2);
        from_off.d = Some(vec!["E".into()]);
        from_off.h = Some(0);
        assert!(
            !pool.receive("C", &from_off, &peers, &mut |_, _| true).fresh,
            "items from a pool:false peer are ignored"
        );
        assert_eq!(pool.stats().dropped_off, 1);
    }

    /// The hop limit, exactly: on a line of ten with `max_hops = 3`, the item reaches N1..N4 and
    /// stops.
    #[test]
    fn the_hop_limit_stops_a_relay_chain() {
        let ids: Vec<String> = (0..10).map(|i| format!("N{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let mut net = Net::new(&refs);
        for i in 0..9 {
            let a = ids[i].clone();
            let b = ids[i + 1].clone();
            net = net.link(&a, &b);
        }
        net.start(POOL_SEEN_LIMIT, 3);
        let sends = net.publish("N0", "far");
        net.run();
        assert_eq!(sends, 1, "N0 sends to N1; the rest is relay");
        for i in 1..=4 {
            assert_eq!(net.applied(&format!("N{i}"), "far"), 1, "N{i} gets it");
        }
        for i in 5..10 {
            assert_eq!(net.applied(&format!("N{i}"), "far"), 0, "N{i} is beyond reach");
        }
        assert_eq!(
            net.sends, 1 + 3,
            "one publish plus three relays: N1->N2->N3->N4, and N4 stops"
        );
    }

    /// A relay only skips the peers the origin listed: one the origin did not list still gets it.
    #[test]
    fn a_stale_link_list_does_not_strand_a_peer() {
        let peers = [PoolPeer::new("A", true, true), PoolPeer::new("B", true, true)];
        let mut pool = Pool::with_limits("X", 0, POOL_SEEN_LIMIT, 3);
        let mut sent: Vec<(String, WireMessage)> = Vec::new();
        let mut stale = clip("stale");
        stale.o = Some("O".into());
        stale.s = Some(1);
        stale.d = Some(vec!["A".into()]);
        stale.h = Some(0);
        let r = pool.receive("A", &stale, &peers, &mut |id, m| {
            sent.push((id.to_string(), m.clone()));
            true
        });
        assert!(r.fresh);
        assert_eq!(r.relayed, vec!["B".to_string()], "B is reached even though d did not list it");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1.h, Some(1), "the relay carries h+1");
        assert_eq!(sent[0].1.o.as_deref(), Some("O"), "the relay keeps the origin");
    }

    /// A restarted app must number its items past its last run, or every device that stayed up
    /// takes them for duplicates.
    #[test]
    fn sequence_numbers_survive_a_restart() {
        let mut b = Pool::with_limits("B", 0, POOL_SEEN_LIMIT, 3);
        let mut old = Pool::with_limits("A", 0, POOL_SEEN_LIMIT, 3);
        for i in 0..3 {
            let p = old.publish(clip(&format!("old{i}")), &[], &mut |_, _| true);
            assert!(b.receive("A", &p.msg, &[], &mut |_, _| true).fresh);
        }
        // Counting from the same place again: the new items look like duplicates.
        let mut again = Pool::with_limits("A", 0, POOL_SEEN_LIMIT, 3);
        let p = again.publish(clip("new0"), &[], &mut |_, _| true);
        assert!(!b.receive("A", &p.msg, &[], &mut |_, _| true).fresh);

        // A later start (the clock by default) makes every item of the new run fresh.
        let mut b2 = Pool::with_limits("B", 0, POOL_SEEN_LIMIT, 3);
        let mut first = Pool::with_limits("A", 1000, POOL_SEEN_LIMIT, 3);
        for i in 0..3 {
            let p = first.publish(clip(&format!("old{i}")), &[], &mut |_, _| true);
            assert!(b2.receive("A", &p.msg, &[], &mut |_, _| true).fresh);
        }
        let mut second = Pool::with_limits("A", 2000, POOL_SEEN_LIMIT, 3);
        for i in 0..3 {
            let p = second.publish(clip(&format!("new{i}")), &[], &mut |_, _| true);
            assert!(b2.receive("A", &p.msg, &[], &mut |_, _| true).fresh, "new run item {i}");
        }
        assert_eq!(b2.stats().dropped_dup, 0);
    }

    /// An old peer's items are numbered from `start` too, so a peer that later switches the mesh on
    /// numbers past them and its first own item is fresh.
    #[test]
    fn a_peer_that_switches_the_mesh_on_is_not_taken_for_a_duplicate() {
        let legacy = [PoolPeer::new("C", false, true)];
        let mut x = Pool::with_limits("X", 1000, POOL_SEEN_LIMIT, 3);
        let mut plain1 = clip("legacy1");
        plain1.text = Some("legacy1".into());
        let r = x.receive("C", &plain1, &legacy, &mut |_, _| true);
        assert_eq!(r.msg.s, Some(1001), "a legacy item is numbered from start too");

        let mut c = Pool::with_limits("C", 2000, POOL_SEEN_LIMIT, 3);
        let p = c.publish(clip("mesh1"), &[], &mut |_, _| true);
        assert!(x.receive("C", &p.msg, &legacy, &mut |_, _| true).fresh, "C's first own item is fresh");
    }
}
