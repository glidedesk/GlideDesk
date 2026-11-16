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
//! Pure: no clipboard, no sockets. `now` is passed in, so every rule here is a function of numbers
//! and can be tested without a desktop session.

use super::{CLIP_MAX_ID, CLIP_SKEW_MS};
use std::cmp::Ordering;

/// The largest time a stamp may carry: JavaScript's `2 ** 53`, so a stamp made here is one the
/// reference can still represent exactly.
pub const MAX_TS: u64 = 1 << 53;

/// When and where a copy was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    /// Hybrid logical clock value, milliseconds.
    pub ts: u64,
    /// The device that made the copy.
    pub o: String,
}

impl Stamp {
    pub fn new(ts: u64, o: impl Into<String>) -> Self {
        Self { ts, o: o.into() }
    }
}

/// What was copied. The kind is remembered so the page can word an item and so a request can be
/// answered with the same sort of thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Files,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "image",
            Kind::Files => "files",
        }
    }
}

/// A hybrid logical clock (`clipsync.js`: `Clock`).
///
/// A copy made here gets the wall clock, but always at least one millisecond past the last time
/// this device used or saw. A copy made right after receiving another therefore orders after it,
/// even when the two machines' clocks disagree by seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clock {
    last: u64,
}

impl Clock {
    /// The time of a copy made here (`now` is the wall clock at the moment of the copy).
    pub fn tick(&mut self, now: u64) -> u64 {
        self.last = now.max(self.last.saturating_add(1));
        self.last
    }

    /// A time seen from another device: later ticks move past it, clamped, so a clock far in the
    /// future cannot drag ours there.
    pub fn observe(&mut self, ts: u64, now: u64) {
        let ceiling = now.saturating_add(CLIP_SKEW_MS);
        self.last = self.last.max(ts.min(ceiling));
    }

    /// The last time this clock handed out or accepted.
    pub fn last(&self) -> u64 {
        self.last
    }
}

/// Newer first by time, then by device id so two devices always agree on a tie (`compare`).
pub fn compare(a: &Stamp, b: &Stamp) -> Ordering {
    match a.ts.cmp(&b.ts) {
        Ordering::Equal => a.o.cmp(&b.o),
        other => other,
    }
}

/// Whether a stamp off the wire can be trusted (`validStamp`).
///
/// A missing, zero, negative or absurdly large time, or an empty or over-long device id, is not a
/// stamp: the item is then treated as copied on arrival, which is what older apps expect.
pub fn valid_stamp(ts: Option<u64>, o: Option<&str>) -> bool {
    let (Some(ts), Some(o)) = (ts, o) else {
        return false;
    };
    ts > 0 && ts < MAX_TS && !o.is_empty() && o.len() <= CLIP_MAX_ID
}

/// The newest copy a device knows of (`clipsync.js`: `Latest`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Latest {
    self_id: String,
    clock: Clock,
    cur: Option<Current>,
}

/// The newest copy: when and where it was made, and what it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    pub stamp: Stamp,
    pub kind: Kind,
    /// The picture's id, when the item is one. Text has none.
    pub id: Option<String>,
}

impl Latest {
    pub fn new(self_id: impl Into<String>) -> Self {
        Self {
            self_id: self_id.into(),
            clock: Clock::default(),
            cur: None,
        }
    }

    /// The newest copy this device knows of, if anything was copied yet.
    pub fn current(&self) -> Option<&Current> {
        self.cur.as_ref()
    }

    /// The clock, for a caller that wants to stamp something else with the same history.
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// A copy happened here: its time, which becomes the newest (`copied`).
    pub fn copied(&mut self, kind: Kind, id: Option<String>, now: u64) -> Stamp {
        let stamp = Stamp::new(self.clock.tick(now), self.self_id.clone());
        self.cur = Some(Current {
            stamp: stamp.clone(),
            kind,
            id,
        });
        stamp
    }

    /// An item from another device. True means apply it, and it becomes the newest copy.
    ///
    /// `from` is the sender, used as the origin of an item from an app that stamps nothing — the
    /// same rule the pool uses for such items.
    pub fn accept(
        &mut self,
        ts: Option<u64>,
        o: Option<&str>,
        from: &str,
        kind: Kind,
        id: Option<String>,
        now: u64,
    ) -> bool {
        let stamp = if valid_stamp(ts, o) {
            let raw = ts.unwrap_or(0);
            self.clock.observe(raw, now);
            // The stamp is stored clamped as well, so a device far in the future cannot hold the
            // newest slot with a time nothing here can ever beat.
            Stamp::new(raw.min(now.saturating_add(CLIP_SKEW_MS)), o.unwrap_or_default())
        } else {
            Stamp::new(
                self.clock.tick(now),
                if from.is_empty() { "~peer" } else { from },
            )
        };
        // Our own copy, coming back after a round trip: never applied (and never re-published).
        if stamp.o == self.self_id {
            return false;
        }
        if let Some(cur) = &self.cur {
            if compare(&stamp, &cur.stamp) != Ordering::Greater {
                return false;
            }
        }
        self.cur = Some(Current { stamp, kind, id });
        true
    }

    /// Another device says it holds `ts`/`o`: true when it is newer than what we know (`isNewer`).
    pub fn is_newer(&mut self, ts: Option<u64>, o: Option<&str>, now: u64) -> bool {
        let (Some(ts), Some(o)) = (ts, o) else {
            return false;
        };
        if !valid_stamp(Some(ts), Some(o)) {
            return false;
        }
        self.clock.observe(ts, now);
        match &self.cur {
            Some(cur) => compare(&Stamp::new(ts, o), &cur.stamp) == Ordering::Greater,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest(id: &str) -> Latest {
        Latest::new(id)
    }

    /// The reported bug: Windows copies A, then the Mac copies B; A's late push must not replace B.
    #[test]
    fn an_older_copy_arriving_late_does_not_replace_a_newer_one() {
        let mut w = latest("W");
        let mut m = latest("M");

        let a = w.copied(Kind::Text, None, 1000);
        let b = m.copied(Kind::Text, None, 1500);
        assert!(b.ts > a.ts);

        assert!(
            !m.accept(Some(a.ts), Some(&a.o), "W", Kind::Text, None, 1600),
            "the older A must not overwrite the Mac's newer B"
        );
        assert!(
            w.accept(Some(b.ts), Some(&b.o), "M", Kind::Text, None, 1700),
            "B is newer than A: Windows takes it"
        );
        assert!(
            !w.accept(Some(a.ts), Some(&a.o), "W", Kind::Text, None, 1800),
            "and an echo of A is ignored afterwards"
        );
    }

    /// A copy made right after receiving another is newer even when the clocks disagree.
    #[test]
    fn a_copy_made_after_receiving_one_orders_after_it() {
        let mut w = latest("W");
        let mut m = latest("M"); // the Mac's clock is four seconds behind

        let a = w.copied(Kind::Text, None, 5000);
        assert!(m.accept(Some(a.ts), Some(&a.o), "W", Kind::Text, None, 1100));
        let b = m.copied(Kind::Text, None, 1100);
        assert!(b.ts > a.ts, "the clock moved past what it saw: B > A");
        assert!(w.accept(Some(b.ts), Some(&b.o), "M", Kind::Text, None, 5200));
    }

    /// A clock far in the future does not win for ever.
    #[test]
    fn a_clock_far_in_the_future_is_clamped() {
        let mut m = latest("M");
        assert!(m.accept(Some(1_000_000_000_000), Some("X"), "X", Kind::Text, None, 1000));
        let mine = m.copied(Kind::Text, None, 1500);
        assert!(
            mine.ts <= 1500 + CLIP_SKEW_MS + 10,
            "our next copy is not dragged into the far future: {}",
            mine.ts
        );
    }

    #[test]
    fn ties_are_broken_the_same_way_everywhere() {
        let a = Stamp::new(5, "a");
        let b = Stamp::new(5, "b");
        assert_eq!(compare(&a, &b), Ordering::Less);
        assert_eq!(compare(&b, &a), Ordering::Greater);
        assert_eq!(compare(&a, &a.clone()), Ordering::Equal);
        assert_eq!(compare(&Stamp::new(6, "a"), &b), Ordering::Greater);
    }

    #[test]
    fn own_echo_is_dropped_and_an_item_without_a_time_counts_as_new() {
        let mut m = latest("M");
        assert!(
            m.accept(None, None, "W", Kind::Text, None, 1000),
            "no ts: copied on arrival"
        );
        assert!(
            !m.accept(Some(900), Some("M"), "M", Kind::Text, None, 1100),
            "our own copy coming back"
        );
        // An item from an app that sends no time still counts as a copy, but only if it beats the
        // newest: the clock ticked past what we knew, so it does.
        assert!(m.accept(None, None, "W", Kind::Text, None, 1200));
    }

    #[test]
    fn a_stamp_off_the_wire_has_to_be_plausible() {
        assert!(valid_stamp(Some(1), Some("a")));
        assert!(!valid_stamp(Some(0), Some("a")));
        assert!(!valid_stamp(Some(MAX_TS), Some("a")), "past 2^53 is not representable");
        assert!(!valid_stamp(Some(1), None));
        assert!(!valid_stamp(Some(1), Some("")));
        assert!(!valid_stamp(None, Some("a")));
        assert!(!valid_stamp(Some(1), Some(&"x".repeat(CLIP_MAX_ID + 1))));
        assert!(valid_stamp(Some(1), Some(&"x".repeat(CLIP_MAX_ID))));
    }

    #[test]
    fn the_clock_only_moves_forward() {
        let mut c = Clock::default();
        assert_eq!(c.tick(100), 100);
        assert_eq!(c.tick(100), 101, "a second copy in the same millisecond still orders after");
        assert_eq!(c.tick(50), 102, "a clock that stepped back does not order back");
        c.observe(500, 1000);
        assert_eq!(c.tick(1000), 1000, "the wall clock is already past what we saw");
        c.observe(10, 2000);
        assert_eq!(c.last(), 1000, "an old time seen late lowers nothing");
    }

    #[test]
    fn a_new_device_asks_whether_the_other_side_holds_something_newer() {
        let mut w = latest("W");
        let mut m = latest("M");
        assert!(
            !m.is_newer(None, None, 1000),
            "nothing said is not news"
        );
        let s = w.copied(Kind::Text, None, 1000);
        assert_eq!(m.current(), None);
        assert!(
            m.is_newer(Some(s.ts), Some(&s.o), 1000),
            "the Mac has nothing: the Windows copy is newer"
        );
        let s2 = m.copied(Kind::Image, Some("img_1".to_string()), 2000);
        assert!(w.is_newer(Some(s2.ts), Some(&s2.o), 2000));
        // The Mac copied later, so the Windows copy is older news now.
        let w_stamp = w.copied(Kind::Text, None, 2100);
        assert_eq!(w_stamp.ts, 2100);
        assert!(m.is_newer(Some(w_stamp.ts), Some(&w_stamp.o), 2200));
        let m_stamp = m.copied(Kind::Text, None, 2300);
        assert!(w.accept(Some(m_stamp.ts), Some(&m_stamp.o), "M", Kind::Text, None, 2400));
        assert!(
            !w.is_newer(Some(m_stamp.ts), Some(&m_stamp.o), 2400),
            "what we already hold is not news"
        );
    }

    #[test]
    fn the_newest_copy_remembers_what_it_is() {
        let mut m = latest("M");
        assert!(m.current().is_none());
        let s = m.copied(Kind::Image, Some("img_7".to_string()), 1000);
        let cur = m.current().expect("a copy");
        assert_eq!(cur.stamp.ts, s.ts);
        assert_eq!(cur.kind, Kind::Image);
        assert_eq!(cur.id.as_deref(), Some("img_7"));
        assert_eq!(Kind::Text.as_str(), "text");
        assert_eq!(Kind::Files.as_str(), "files");
    }
}
