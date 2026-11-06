// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Multi-monitor crossing geometry.
//
// Two computers each report their display rectangles (OS coordinates, points). The
// arrangement is `{side, along}`: the other computer is on `side` of this one, and its
// bounding box starts `along` points further along that side than ours. The pointer
// crosses only inside a "stretch" — a segment where a monitor of each computer is edge-
// to-edge — and lands at the same fractional position within the matching stretch on the
// other side.
//
// Pure functions, no I/O.

/// Display rectangle in OS coordinates (points). Matches `MonitorInfo` in state.rs.
#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Arrangement of the other computer relative to ours.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrange {
    pub side: Side,
    pub along: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

pub const SIDES: [Side; 4] = [Side::Left, Side::Right, Side::Top, Side::Bottom];

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            "top" => Some(Side::Top),
            "bottom" => Some(Side::Bottom),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Top => "top",
            Side::Bottom => "bottom",
        }
    }
}

/// Points — less than this along a side doesn't count as touching.
const MIN_TOUCH: f64 = 40.0;
/// Points — aligning edges or centres pulls the arrangement from this far.
const SNAP: f64 = 24.0;
/// Points — how finely snap searches for the nearest touching place.
const STEP: f64 = 8.0;

/// Bounding rectangle of a set of displays.
pub fn bounds(displays: &[Display]) -> Option<(f64, f64, f64, f64)> {
    if displays.is_empty() {
        return None;
    }
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for d in displays {
        x0 = x0.min(d.x);
        y0 = y0.min(d.y);
        x1 = x1.max(d.x + d.w);
        y1 = y1.max(d.y + d.h);
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// The extent of displays' bounding box along `side`: `(start, len)`.
fn span(displays: &[Display], side: Side) -> Option<(f64, f64)> {
    let (x, y, w, h) = bounds(displays)?;
    if side.is_vertical() {
        Some((y, h))
    } else {
        Some((x, w))
    }
}

/// A display's extent along/across a side.
fn along_range(d: &Display, side: Side) -> (f64, f64) {
    if side.is_vertical() {
        (d.y, d.y + d.h)
    } else {
        (d.x, d.x + d.w)
    }
}

fn across_range(d: &Display, side: Side) -> (f64, f64) {
    if side.is_vertical() {
        (d.x, d.x + d.w)
    } else {
        (d.y, d.y + d.h)
    }
}

/// `(dx, dy)` to translate the other computer's coordinates into ours.
pub fn placement(mine: &[Display], theirs: &[Display], arrange: &Arrange) -> Option<(f64, f64)> {
    let (mx, my, mw, mh) = bounds(mine)?;
    let (tx, ty, _tw, _th) = bounds(theirs)?;
    let v = arrange.side.is_vertical();
    let shift_along = if v { my } else { mx } + arrange.along - if v { ty } else { tx };
    let out = matches!(arrange.side, Side::Right | Side::Bottom);
    let mut shift_across: Option<f64> = None;
    for m in mine {
        let (ma, mz) = along_range(m, arrange.side);
        let (_mlo, mhi) = across_range(m, arrange.side);
        let (mlo, _) = across_range(m, arrange.side);
        for t in theirs {
            let (ta, tz) = along_range(t, arrange.side);
            let (tlo, _thi) = across_range(t, arrange.side);
            let (_tlo2, thi) = across_range(t, arrange.side);
            if (mz).min(tz + shift_along) - (ma).max(ta + shift_along) <= 0.0 {
                continue;
            }
            let s = if out { mhi - tlo } else { mlo - thi };
            shift_across = Some(match shift_across {
                None => s,
                Some(prev) => if out { prev.max(s) } else { prev.min(s) },
            });
        }
    }
    let shift_across = shift_across.unwrap_or_else(|| {
        let (bx, by, bw, bh) = (mx, my, mw, mh);
        let (tx, ty, tw, th) = bounds(theirs).unwrap();
        let b_range = if v { (bx, bx + bw) } else { (by, by + bh) };
        let t_range = if v { (tx, tx + tw) } else { (ty, ty + th) };
        if out { b_range.1 - t_range.0 } else { b_range.0 - t_range.1 }
    });
    if v {
        Some((shift_across, shift_along))
    } else {
        Some((shift_along, shift_across))
    }
}

/// One touching stretch between two monitors.
#[derive(Debug, Clone, PartialEq)]
pub struct Stretch {
    pub mine_display: String,
    pub mine_from: f64,
    pub mine_to: f64,
    pub theirs_display: String,
    pub theirs_from: f64,
    pub theirs_to: f64,
}

/// All touching stretches between `mine` and `theirs`, sorted along the side.
pub fn contact(mine: &[Display], theirs: &[Display], arrange: &Arrange) -> Vec<Stretch> {
    let Some(p) = placement(mine, theirs, arrange) else {
        return Vec::new();
    };
    let v = arrange.side.is_vertical();
    let out = matches!(arrange.side, Side::Right | Side::Bottom);
    let sa = if v { p.1 } else { p.0 };
    let sx = if v { p.0 } else { p.1 };
    let mut res = Vec::new();
    for m in mine {
        let (ma, mz) = along_range(m, arrange.side);
        let (mlo, mhi) = across_range(m, arrange.side);
        for t in theirs {
            let (ta, tz) = along_range(t, arrange.side);
            let (tlo, thi) = across_range(t, arrange.side);
            let edge_diff = if out { tlo + sx - mhi } else { thi + sx - mlo };
            if edge_diff.abs() >= 1.0 {
                continue;
            }
            let lo = ma.max(ta + sa);
            let hi = mz.min(tz + sa);
            if hi - lo < 1.0 {
                continue;
            }
            res.push(Stretch {
                mine_display: m.id.clone(),
                mine_from: lo,
                mine_to: hi,
                theirs_display: t.id.clone(),
                theirs_from: lo - sa,
                theirs_to: hi - sa,
            });
        }
    }
    res.sort_by(|a, b| a.mine_from.partial_cmp(&b.mine_from).unwrap());
    res
}

/// Longest touching stretch (0 if none).
pub fn touch(mine: &[Display], theirs: &[Display], arrange: &Arrange) -> f64 {
    contact(mine, theirs, arrange)
        .iter()
        .map(|c| c.mine_to - c.mine_from)
        .fold(0.0f64, f64::max)
}

/// 0–1 fractional position along a stretch at coordinate `point_along`.
pub fn exit_point(stretch: &Stretch, point_along: f64) -> f64 {
    let (from, to) = (stretch.mine_from, stretch.mine_to);
    ((point_along - from) / (to - from).max(1.0)).clamp(0.0, 1.0)
}

/// Coordinate on the other side's stretch at fractional position `frac`.
pub fn entry_point(stretch: &Stretch, frac: f64) -> f64 {
    let (from, to) = (stretch.theirs_from, stretch.theirs_to);
    from + frac * (to - from)
}

/// Stretches as zones: fractional start/end on the bounding box, for helpers.
pub fn zones(
    mine: &[Display],
    theirs: &[Display],
    arrange: &Arrange,
) -> Vec<(f64, f64, f64, f64)> {
    let Some(ms) = span(mine, arrange.side) else {
        return Vec::new();
    };
    let Some(ts) = span(theirs, arrange.side) else {
        return Vec::new();
    };
    contact(mine, theirs, arrange)
        .iter()
        .map(|c| {
            let mf = (c.mine_from - ms.0) / ms.1.max(1.0);
            let mt = (c.mine_to - ms.0) / ms.1.max(1.0);
            let tf = (c.theirs_from - ts.0) / ts.1.max(1.0);
            let tt = (c.theirs_to - ts.0) / ts.1.max(1.0);
            (mf, mt, tf, tt)
        })
        .collect()
}

/// Map an offset (0–1 on our bounding box) to an offset on theirs.
pub fn map_offset(
    mine: &[Display],
    theirs: &[Display],
    arrange: &Arrange,
    offset: f64,
) -> f64 {
    let ms = match span(mine, arrange.side) {
        Some(s) => s,
        None => return offset,
    };
    let ts = match span(theirs, arrange.side) {
        Some(s) => s,
        None => return offset,
    };
    let cs = contact(mine, theirs, arrange);
    if cs.is_empty() {
        return offset;
    }
    let a = ms.0 + offset * ms.1;
    let dist = |c: &Stretch| {
        if a < c.mine_from {
            c.mine_from - a
        } else if a > c.mine_to {
            a - c.mine_to
        } else {
            0.0
        }
    };
    let c = cs
        .iter()
        .min_by(|a, b| dist(a).partial_cmp(&dist(b)).unwrap())
        .unwrap();
    let b = entry_point(c, exit_point(c, a));
    ((b - ts.0) / ts.1.max(1.0)).clamp(0.0, 1.0)
}

/// The other computer centred on ours along `side`.
pub fn centred(mine: &[Display], theirs: &[Display], side: Side) -> f64 {
    match (span(mine, side), span(theirs, side)) {
        (Some(m), Some(t)) => ((m.1 - t.1) / 2.0).round(),
        _ => 0.0,
    }
}

/// Snap `value` for `along` to the nearest place that touches by at least MIN_TOUCH and
/// pull onto aligned monitor edges or centres within SNAP points.
pub fn snap(mine: &[Display], theirs: &[Display], side: Side, value: f64) -> f64 {
    let ms = match span(mine, side) {
        Some(s) => s,
        None => return value,
    };
    let ts = match span(theirs, side) {
        Some(s) => s,
        None => return value,
    };

    let shortest = mine
        .iter()
        .chain(theirs.iter())
        .map(|d| {
            let (a, b) = along_range(d, side);
            b - a
        })
        .fold(f64::INFINITY, f64::min);
    let need = MIN_TOUCH.min(shortest) - 1e-6;
    let ok = |a: f64| -> bool {
        touch(
            mine,
            theirs,
            &Arrange { side, along: a },
        ) >= need
    };

    let mut v = value;
    if !ok(v) {
        let mut found: Option<f64> = None;
        let lim = ms.1 + ts.1;
        let mut d = STEP;
        while d <= lim && found.is_none() {
            if ok(value - d) {
                found = Some(value - d);
            } else if ok(value + d) {
                found = Some(value + d);
            }
            d += STEP;
        }
        let Some(f) = found else {
            return value.round();
        };
        let (mut lo, mut hi) = if f > value { (f, f - STEP) } else { (f, f + STEP) };
        for _ in 0..12 {
            let mid = (lo + hi) / 2.0;
            if ok(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        v = lo;
    }

    // Aligned starts, ends and centres.
    let mut marks = vec![0.0, ms.1 - ts.1, (ms.1 - ts.1) / 2.0];
    for a in mine {
        for b in theirs {
            let (af, at) = along_range(a, side);
            let (bf, bt) = along_range(b, side);
            marks.push(af - bf - ms.0 + ts.0);
            marks.push(at - bt - ms.0 + ts.0);
            marks.push((af + at - bf - bt) / 2.0 - ms.0 + ts.0);
        }
    }
    let mut pull: Option<f64> = None;
    for &k in &marks {
        if (k - v).abs() <= SNAP && ok(k) {
            pull = Some(match pull {
                None => k,
                Some(p) => {
                    if (k - v).abs() < (p - v).abs() {
                        k
                    } else {
                        p
                    }
                }
            });
        }
    }
    pull.unwrap_or(v).round()
}

/// Validate and normalize a saved arrangement.
pub fn normalize(
    arrange: Option<&Arrange>,
    mine: &[Display],
    theirs: &[Display],
) -> Arrange {
    let side = arrange
        .map(|a| a.side)
        .unwrap_or(Side::Right);
    let along = arrange.and_then(|a| {
        if a.along.is_finite() {
            Some(a.along)
        } else {
            None
        }
    });
    let known = bounds(mine).is_some() && bounds(theirs).is_some();
    let a = match along {
        None => {
            let c = centred(mine, theirs, side);
            if known { snap(mine, theirs, side, c) } else { c }
        }
        Some(v) => {
            if known { snap(mine, theirs, side, v) } else { v }
        }
    };
    Arrange { side, along: a }
}

/// The arrangement as the other computer sees it.
pub fn mirror(arrange: &Arrange) -> Arrange {
    Arrange {
        side: arrange.side.opposite(),
        along: -arrange.along,
    }
}

/// Display ids in numbered order (left to right, then top to bottom).
pub fn numbered(displays: &[Display]) -> Vec<String> {
    let mut sorted: Vec<_> = displays.iter().collect();
    sorted.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap()
            .then(a.y.partial_cmp(&b.y).unwrap())
    });
    sorted.iter().map(|d| d.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, msg: &str) {
        assert!((a - b).abs() < 1e-6, "{}: {} != {}", msg, a, b);
    }

    fn mac() -> Vec<Display> {
        vec![
            Display { id: "laptop".into(), x: 0.0, y: 0.0, w: 1512.0, h: 982.0 },
            Display { id: "ext".into(), x: 1512.0, y: 0.0, w: 2560.0, h: 1440.0 },
        ]
    }

    fn pc() -> Vec<Display> {
        vec![Display { id: "pc".into(), x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 }]
    }

    #[test]
    fn placement_right() {
        let p = placement(&mac(), &pc(), &Arrange { side: Side::Right, along: 100.0 }).unwrap();
        assert_eq!(p, (4072.0, 100.0));
    }

    #[test]
    fn placement_bottom_laptop() {
        let p = placement(&mac(), &pc(), &Arrange { side: Side::Bottom, along: -420.0 }).unwrap();
        assert_eq!(p, (-420.0, 982.0));
    }

    #[test]
    fn placement_bottom_both() {
        let p = placement(&mac(), &pc(), &Arrange { side: Side::Bottom, along: 1000.0 }).unwrap();
        assert_eq!(p, (1000.0, 1440.0));
    }

    #[test]
    fn placement_empty() {
        assert!(placement(&mac(), &[], &Arrange { side: Side::Right, along: 0.0 }).is_none());
    }

    #[test]
    fn contact_right_single() {
        let cs = contact(&mac(), &pc(), &Arrange { side: Side::Right, along: 100.0 });
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].mine_display, "ext");
        assert_eq!(cs[0].mine_from, 100.0);
        assert_eq!(cs[0].mine_to, 1180.0);
        assert_eq!(cs[0].theirs_display, "pc");
        assert_eq!(cs[0].theirs_from, 0.0);
        assert_eq!(cs[0].theirs_to, 1080.0);
    }

    #[test]
    fn contact_partly_past() {
        let cs = contact(
            &mac(),
            &pc(),
            &Arrange { side: Side::Right, along: 900.0 },
        );
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].mine_from, 900.0);
        assert_eq!(cs[0].mine_to, 1440.0);
        assert_eq!(cs[0].theirs_from, 0.0);
        assert_eq!(cs[0].theirs_to, 540.0);
    }

    #[test]
    fn zones_basic() {
        let z = zones(&mac(), &pc(), &Arrange { side: Side::Right, along: 100.0 });
        assert_eq!(z.len(), 1);
        near(z[0].0, 100.0 / 1440.0, "mf");
        near(z[0].1, 1180.0 / 1440.0, "mt");
        near(z[0].2, 0.0, "tf");
        near(z[0].3, 1.0, "tt");
    }

    #[test]
    fn map_offset_right() {
        let o = map_offset(
            &mac(),
            &pc(),
            &Arrange { side: Side::Right, along: 100.0 },
            400.0 / 1440.0,
        );
        // 400 is inside the stretch (100..1180), mapping (400-100)/(1180-100)=300/1080
        near(o, 300.0 / 1080.0, "mapped offset");
    }

    #[test]
    fn mirror_arrangement() {
        let m = mirror(&Arrange { side: Side::Right, along: 100.0 });
        assert_eq!(m.side, Side::Left);
        near(m.along, -100.0, "mirrored along");
    }

    #[test]
    fn centred_value() {
        let c = centred(&mac(), &pc(), Side::Right);
        near(c, ((1440.0_f64 - 1080.0) / 2.0).round(), "centred");
    }

    #[test]
    fn numbered_order() {
        let n = numbered(&mac());
        assert_eq!(n, vec!["laptop".to_string(), "ext".to_string()]);
    }

    #[test]
    fn snap_centred_falls_back() {
        let s = snap(&mac(), &pc(), Side::Right, centred(&mac(), &pc(), Side::Right));
        // Already touching, should stay close to centred
        near(s, centred(&mac(), &pc(), Side::Right), "snap centred");
    }

    #[test]
    fn normalize_default() {
        let n = normalize(None, &mac(), &pc());
        assert_eq!(n.side, Side::Right);
        // along is centred-and-snapped
    }

    #[test]
    fn bounds_empty() {
        assert!(bounds(&[]).is_none());
    }

    #[test]
    fn bounds_single() {
        let b = bounds(&pc()).unwrap();
        assert_eq!(b, (0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn bounds_multi() {
        let b = bounds(&mac()).unwrap();
        assert_eq!(b, (0.0, 0.0, 4072.0, 1440.0));
    }
}
