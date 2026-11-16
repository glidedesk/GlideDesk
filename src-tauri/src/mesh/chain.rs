// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
//! Noticing that the pointer this device was handed has walked off its own screen.
//!
//! A device being driven receives relative motion and moves its own pointer with it. When the hand
//! keeps going past an edge of this screen and another device sits on that side, the pointer should
//! carry on into *that* device instead of stopping dead at the edge. Reference: `_armReturn` /
//! `_onReturn` in the JavaScript mesh (`kvm.js`), which gets this from the native helper's edge
//! watch.
//!
//! There is no native cursor query on every platform (`cursor_position()` returns `None` on Linux),
//! so the position is tracked from the motion itself: it starts where the handover placed the
//! pointer and is carried by every delta since. That is also what makes this testable without a
//! desktop session — the whole rule is arithmetic on two numbers.
//!
//! Nothing here decides *who* is on the side; that is the edge map's job. This module only answers
//! "the pointer has left the screen on this side, at this fraction along it".

use parking_lot::Mutex;

/// How far past an edge the hand has to travel before it counts as leaving.
///
/// A pointer parked exactly on the edge, or a hand overshooting by a pixel, is not a crossing. The
/// value is small because the reference's watcher fires as soon as the pointer leaves the screen.
pub const LEAVE_DEPTH: f64 = 4.0;

/// The last known pointer position on this screen, carried by the motion received.
#[derive(Debug, Clone, Copy)]
pub struct Tracker {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    over: f64,
    side: Option<&'static str>,
    /// False once this push has been reported: the hand still leaning on the edge is the same
    /// leaving, not a new one. A pointer that comes back inside the screen arms it again.
    armed: bool,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            over: 0.0,
            side: None,
            armed: true,
        }
    }
}

impl Tracker {
    /// Start tracking at the point along `side` where the handover put the pointer.
    ///
    /// `side` is this screen's own side, the one facing the device that took it (`kvm.take` names
    /// the side the sender left on; the receiver places on the opposite one).
    pub fn place(&mut self, w: f64, h: f64, side: &str, frac: f64) {
        self.w = w.max(1.0);
        self.h = h.max(1.0);
        self.over = 0.0;
        self.side = None;
        self.armed = true;
        let f = frac.clamp(0.0, 1.0);
        let (x, y) = match side {
            "left" => (0.0, f * self.h),
            "right" => (self.w, f * self.h),
            "top" => (f * self.w, 0.0),
            "bottom" => (f * self.w, self.h),
            _ => (self.w / 2.0, self.h / 2.0),
        };
        self.x = x;
        self.y = y;
    }

    /// Carry the pointer by a delta, and report a leaving: `(side, fraction along it)`.
    ///
    /// The position is clamped to the screen, and the distance that was refused is added up: that
    /// is the hand still pushing the way it was going. Leaving requires `LEAVE_DEPTH` of it, and
    /// only once per side, so one long push does not send the pointer on twice.
    pub fn apply(&mut self, dx: f64, dy: f64) -> Option<(&'static str, f64)> {
        self.x += dx;
        self.y += dy;

        // The distance the screen refused: that is the hand still pushing the way it was going.
        let (hit, pushed) = if self.x < 0.0 {
            let p = -self.x;
            self.x = 0.0;
            (Some("left"), p)
        } else if self.x > self.w {
            let p = self.x - self.w;
            self.x = self.w;
            (Some("right"), p)
        } else if self.y < 0.0 {
            let p = -self.y;
            self.y = 0.0;
            (Some("top"), p)
        } else if self.y > self.h {
            let p = self.y - self.h;
            self.y = self.h;
            (Some("bottom"), p)
        } else {
            (None, 0.0)
        };

        let Some(side) = hit else {
            // Back inside the screen: the next push starts from nothing.
            self.over = 0.0;
            self.side = None;
            self.armed = true;
            return None;
        };
        if self.side != Some(side) {
            // A different edge than the one being pushed at: only that push counts.
            self.side = Some(side);
            self.over = 0.0;
        }
        self.over += pushed;
        if !self.armed || self.over < LEAVE_DEPTH {
            return None;
        }

        let frac = match side {
            "left" | "right" => self.y / self.h,
            _ => self.x / self.w,
        };
        self.over = 0.0;
        self.armed = false;
        Some((side, frac.clamp(0.0, 1.0)))
    }

    /// Where the pointer is believed to be, as `(x, y, w, h)`. For diagnostics and tests.
    pub fn position(&self) -> (f64, f64, f64, f64) {
        (self.x, self.y, self.w, self.h)
    }
}

static TRACKER: Mutex<Option<Tracker>> = Mutex::new(None);

/// Begin tracking at the edge a handover placed the pointer on.
pub fn place(screen: (f64, f64), side: &str, frac: f64) {
    let mut t = Tracker::default();
    t.place(screen.0, screen.1, side, frac);
    *TRACKER.lock() = Some(t);
}

/// Stop tracking: the pointer is not on this screen any more, or not being moved by anyone else.
pub fn reset() {
    *TRACKER.lock() = None;
}

/// Carry the tracked pointer by a received delta and report a leaving, if there is one.
///
/// `screen` is only read when tracking has not started (a `kvm.take` from a build that did not name
/// an edge): the pointer is assumed to be in the middle, and the first delta starts from there.
pub fn apply(screen: (f64, f64), dx: f64, dy: f64) -> Option<(&'static str, f64)> {
    let mut slot = TRACKER.lock();
    let tracker = slot.get_or_insert_with(|| {
        let mut t = Tracker::default();
        t.place(screen.0, screen.1, "", 0.5);
        t
    });
    // A screen that changed size (a monitor plugged in) must not leave a stale width: the edges
    // would be somewhere the pointer can never reach.
    if (tracker.w - screen.0).abs() > 1.0 || (tracker.h - screen.1).abs() > 1.0 {
        let (x, y) = (tracker.x, tracker.y);
        tracker.place(screen.0, screen.1, "", 0.5);
        tracker.x = x.clamp(0.0, tracker.w);
        tracker.y = y.clamp(0.0, tracker.h);
    }
    tracker.apply(dx, dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> Tracker {
        let mut t = Tracker::default();
        t.place(1920.0, 1080.0, "left", 0.5);
        t
    }

    #[test]
    fn a_handover_puts_the_pointer_on_the_facing_edge() {
        let mut t = Tracker::default();
        t.place(1920.0, 1080.0, "left", 0.25);
        assert_eq!(t.position(), (0.0, 270.0, 1920.0, 1080.0));
        t.place(1920.0, 1080.0, "right", 0.75);
        assert_eq!(t.position(), (1920.0, 810.0, 1920.0, 1080.0));
        t.place(1920.0, 1080.0, "top", 0.5);
        assert_eq!(t.position(), (960.0, 0.0, 1920.0, 1080.0));
        t.place(1920.0, 1080.0, "bottom", 0.5);
        assert_eq!(t.position(), (960.0, 1080.0, 1920.0, 1080.0));
        // An edge that is not an edge starts in the middle rather than nowhere.
        t.place(1920.0, 1080.0, "sideways", 0.5);
        assert_eq!(t.position(), (960.0, 540.0, 1920.0, 1080.0));
    }

    #[test]
    fn walking_inside_the_screen_never_leaves_it() {
        let mut t = tracker();
        assert_eq!(t.apply(10.0, 5.0), None);
        assert_eq!(t.position().0, 10.0);
        assert_eq!(t.apply(-5.0, -2.0), None);
        assert_eq!(t.position(), (5.0, 543.0, 1920.0, 1080.0));
    }

    #[test]
    fn a_small_overshoot_at_the_edge_is_not_a_crossing() {
        let mut t = tracker();
        // The pointer starts on the left edge; a pixel of overshoot is just the edge.
        assert_eq!(t.apply(-1.0, 0.0), None);
        assert_eq!(t.apply(-1.0, 0.0), None);
        assert_eq!(t.position().0, 0.0);
    }

    #[test]
    fn pushing_past_the_edge_leaves_on_that_side_at_that_point() {
        let mut t = Tracker::default();
        t.place(1920.0, 1080.0, "left", 0.25);
        assert_eq!(t.apply(-5.0, 0.0), Some(("left", 0.25)));
        // It is reported once: the hand still pushing does not leave twice.
        assert_eq!(t.apply(-5.0, 0.0), None);
        // Back inside, then out again: a new leaving, at the new point.
        assert_eq!(t.apply(50.0, 108.0), None);
        assert_eq!(t.apply(-60.0, 0.0), Some(("left", 0.35)));
    }

    #[test]
    fn leaving_through_another_side_names_that_side() {
        let mut t = Tracker::default();
        t.place(1920.0, 1080.0, "top", 0.5);
        assert_eq!(t.apply(0.0, -10.0), Some(("top", 0.5)));
        // A corner push reports the side it actually went past.
        let mut c = Tracker::default();
        c.place(1920.0, 1080.0, "right", 1.0);
        assert_eq!(c.apply(10.0, 0.0), Some(("right", 1.0)));
        let mut b = Tracker::default();
        b.place(1920.0, 1080.0, "bottom", 0.5);
        assert_eq!(b.apply(0.0, 10.0), Some(("bottom", 0.5)));
    }

    /// `SC-005`: coming home must land within 2 px of the seam plus the travel since, at any
    /// height of the edge. The tracker is arithmetic on the seam it was given, so this sweeps 100
    /// entry heights on each side and checks the landing point against the hand's own travel.
    #[test]
    fn a_hundred_returns_land_within_two_pixels_of_the_seam_plus_travel() {
        let (w, h) = (1920.0_f64, 1080.0_f64);
        let mut checked = 0;
        for side in ["left", "right", "top", "bottom"] {
            for step in 0..100 {
                let frac = 0.02 + 0.96 * (step as f64) / 99.0;
                let mut t = Tracker::default();
                t.place(w, h, side, frac);

                // The seam point the hand came home to, before any travel.
                let (sx, sy, _, _) = t.position();
                // The hand keeps moving inside the screen: 7 px along the edge and 31 px in from
                // it, both small enough to stay inside on a screen this size.
                let (dx, dy) = match side {
                    "left" => (31.0, 7.0),
                    "right" => (-31.0, 7.0),
                    "top" => (7.0, 31.0),
                    _ => (7.0, -31.0),
                };
                assert_eq!(t.apply(dx, dy), None, "travel inside the screen never leaves it");
                let (x, y, _, _) = t.position();
                assert!(
                    (x - (sx + dx)).abs() <= 2.0 && (y - (sy + dy)).abs() <= 2.0,
                    "{side} at {frac:.3}: landed at ({x:.1}, {y:.1}), seam was ({sx:.1}, {sy:.1}) + ({dx:.1}, {dy:.1})"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 400, "100 entry heights on each of the four sides");
    }

    #[test]
    fn a_screen_that_changed_size_keeps_the_pointer_inside_it() {
        // A monitor was unplugged while the other device drove this one: the pointer must not be
        // tracked at a position the screen no longer has.
        place((1920.0, 1080.0), "right", 0.5);
        let left = apply((1280.0, 800.0), 0.0, 0.0);
        assert_eq!(left, None);
        reset();
    }
}
