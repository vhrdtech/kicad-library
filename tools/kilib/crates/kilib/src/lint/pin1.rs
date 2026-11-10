//! Pin-1 and polarity marks. A part with three or more pins, and a polar two-pin part (diodes, LEDs, batteries,
//! polarised capacitors; pin 1 is the cathode of a diode or LED, the plus of a capacitor or battery), shows which
//! pad is pin 1 on its fab layer, and on its silkscreen where it draws one.
//!
//! How a mark is found: the pads alone often fit the part turned (a SOIC turned half round, a diode end to end, a
//! square QFN a quarter round), each such turn taking pin 1 to another pad; a part is never mirrored on its side,
//! so mirror images do not count (a SOIC's notch marks pin 1 although pin 8 shares its end). For every such move the drawing has to tell
//! the two apart: a mark that the move does not map onto another mark (a dot, a filled shape, a small closed
//! shape, a chamfered corner, a diode's cathode bar, a `1` or `+`) and that lies nearer pin 1 than the pad the move
//! takes it to. Without such marks, the outline itself counts when the part of it the move does not map onto itself
//! lies mostly by pin 1 (a silkscreen line that runs on past pin 1, say). A move the drawing does not tell apart is a
//! missing mark; marks by both pads, or only by the other, are ambiguous or point at the wrong pad.
//!
//! The fix, where the rule is clear: a missing fab mark becomes a chamfered pin-1 corner of the fab outline, when
//! that outline is one rectangle (the chamfer [`CHAMFER`], at most a quarter of the shorter side, KiCad's library
//! convention); a missing silkscreen mark becomes a filled triangle (the largest of [`TRIANGLES`] that fits)
//! pointing at pin 1, outside the body, near the courtyard and [`TRIANGLE_GAP`] clear of every pad. Everything else is reported.

use super::courtyard::as_rect;
use super::silk::MARKS;
use super::{Class, Input, Outcome, delete, insert};
use crate::footprint::{EPS, Footprint, Graphic, Pad, Pt, Rect, dist, fmt, render_graphic, segment_distance};

pub const CHAMFER: f64 = 1.0;
/// The silkscreen triangle's sizes, the largest that fits.
pub const TRIANGLES: [f64; 3] = [0.6, 0.5, 0.4];
pub const TRIANGLE_GAP: f64 = 0.15;
/// How far past the courtyard a silkscreen triangle may reach.
pub const PAST_COURTYARD: f64 = 0.5;
/// Pads this close are in the same place.
const SAME: f64 = 0.02;
/// Marks this close are the same mark.
const SAME_MARK: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Turn180,
    Turn90,
    Turn270,
}

impl Move {
    const ALL: [Move; 3] = [Move::Turn180, Move::Turn90, Move::Turn270];

    fn apply(self, p: Pt, c: Pt) -> Pt {
        let (dx, dy) = (p.0 - c.0, p.1 - c.1);
        let (x, y) = match self {
            Move::Turn180 => (-dx, -dy),
            Move::Turn90 => (-dy, dx),
            Move::Turn270 => (dy, -dx),
        };
        (c.0 + x, c.1 + y)
    }
}

/// Pin 1: the pad `1`, `A1`, `K` (a cathode) or `+`.
pub fn pin1(fp: &Footprint) -> Option<&Pad> {
    ["1", "A1", "K", "+"].iter().find_map(|n| fp.copper_pads().find(|p| p.number == *n))
}

/// The part has a pin 1 to mark.
pub fn needs_mark(lib: &str, fp: &Footprint) -> bool {
    if pin1(fp).is_none() {
        return false;
    }
    let mut numbers: Vec<&str> = fp.copper_pads().map(|p| p.number.as_str()).filter(|n| !n.is_empty()).collect();
    numbers.sort();
    numbers.dedup();
    match numbers.len() {
        0 | 1 => false,
        2 => {
            let lib = lib.strip_prefix("VHRD_").unwrap_or(lib);
            let name = fp.name.to_ascii_lowercase();
            let bidir = name.contains("bidir") || name.contains("bi-dir");
            let polar_lib = ["Diode", "LED", "Battery"].iter().any(|k| lib.starts_with(k));
            let polar_name = ["d_", "led_", "cp_", "tvs_"].iter().any(|p| name.starts_with(p))
                || ["tantal", "elec", "polar"].iter().any(|k| name.contains(k));
            let polar_pads = numbers.iter().any(|n| matches!(*n, "K" | "+"));
            !bidir && (polar_lib || polar_name || polar_pads)
        }
        _ => true,
    }
}

/// The moves that fit the pads onto themselves and take pin 1 elsewhere, with where they take it, about the pads'
/// centre; when none does, a half turn (the part still has to show which way round it goes).
fn moves(fp: &Footprint, p1: &Pad) -> (Pt, Vec<(Move, Pt)>) {
    let pads: Vec<Pt> = fp.copper_pads().map(|p| p.at).collect();
    let c = Rect::of_points(pads.iter().copied()).map_or(p1.at, |r| r.center());
    let fits = |m: Move| pads.iter().all(|&p| pads.iter().any(|&q| dist(m.apply(p, c), q) < SAME));
    let mut out: Vec<(Move, Pt)> = Move::ALL
        .into_iter()
        .filter(|&m| fits(m))
        .map(|m| (m, m.apply(p1.at, c)))
        .filter(|(_, to)| dist(*to, p1.at) > SAME)
        .collect();
    if out.is_empty() && !fits(Move::Turn180) {
        let to = Move::Turn180.apply(p1.at, c);
        if dist(to, p1.at) > SAME {
            out.push((Move::Turn180, to));
        }
    }
    (c, out)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Feature {
    at: Pt,
    kind: &'static str,
}

fn angle_kind(a: Pt, b: Pt) -> (bool, bool) {
    let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
    let deg = dy.atan2(dx).to_degrees();
    (deg < 2.0 || deg > 88.0, (20.0..=70.0).contains(&deg))
}

fn mid(a: Pt, b: Pt) -> Pt {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

/// The marks drawn on `layer`, and the segments that are not part of one.
fn features(fp: &Footprint, layer: &str, body: Rect, axis: Option<Pt>) -> (Vec<Feature>, Vec<(Pt, Pt)>) {
    let small = body.w().min(body.h()).max(0.5);
    let mut out = Vec::new();
    let mut rest: Vec<(Pt, Pt)> = Vec::new();
    // Arcs and curves are drawn with many short slanted pieces: only straight items count as a chamfer or a bar.
    let straight_kind = |g: &Graphic| matches!(g.kind.as_str(), "fp_line" | "fp_rect" | "fp_poly");
    let mut segs: Vec<(Pt, Pt)> = Vec::new();
    let mut straight: Vec<bool> = Vec::new();
    for g in fp.on(layer).filter(|g| !g.filled && g.circle.is_none()) {
        for s in g.segments() {
            segs.push(s);
            straight.push(straight_kind(g));
        }
    }
    let square_at = |p: Pt, skip: usize| {
        segs.iter().enumerate().any(|(j, (a, b))| {
            j != skip && straight[j] && angle_kind(*a, *b).0 && (dist(p, *a) < 0.01 || dist(p, *b) < 0.01)
        })
    };
    let mut used = vec![false; segs.len()];
    for g in fp.on(layer) {
        let Some(bb) = g.bbox() else { continue };
        let size = bb.w().max(bb.h());
        // A filled shape smaller than a quarter of the body: a dot, a triangle, a cathode band.
        if g.filled && bb.w() * bb.h() <= 0.25 * body.w() * body.h() {
            out.push(Feature { at: bb.center(), kind: "filled" });
        } else if let Some((c, r)) = g.circle {
            if r <= 0.3 * small.max(1.5) {
                out.push(Feature { at: c, kind: "dot" });
            }
        } else if g.closed && size <= 0.35 * small {
            out.push(Feature { at: bb.center(), kind: "shape" });
        }
    }
    for t in fp.texts.iter().filter(|t| t.layer == layer && !t.hidden && MARKS.contains(&t.text.trim())) {
        out.push(Feature { at: t.at, kind: "text" });
    }
    let diag: Vec<usize> = (0..segs.len()).filter(|&i| straight[i] && angle_kind(segs[i].0, segs[i].1).1).collect();
    // A diode's symbol (two-pin parts): slanted lines meeting at a point on a cross bar, the cathode.
    if let Some(ax) = axis {
        let mut apexes: Vec<Pt> = Vec::new();
        for &i in &diag {
            for p in [segs[i].0, segs[i].1] {
                let meeting = diag
                    .iter()
                    .filter(|&&j| j != i && (dist(segs[j].0, p) < 0.02 || dist(segs[j].1, p) < 0.02))
                    .count();
                if meeting >= 1 && !apexes.iter().any(|q| dist(*q, p) < 0.02) {
                    apexes.push(p);
                }
            }
        }
        for p in apexes {
            let bars: Vec<usize> = (0..segs.len())
                .filter(|&k| {
                    let (a, b) = segs[k];
                    let d = (b.0 - a.0, b.1 - a.1);
                    let len = d.0.hypot(d.1);
                    straight[k]
                        && len > EPS
                        && (d.0 * ax.0 + d.1 * ax.1).abs() / len < 0.05
                        && segment_distance(p, a, b) < 0.02
                })
                .collect();
            if bars.is_empty() {
                continue;
            }
            out.push(Feature { at: p, kind: "cathode bar" });
            for k in bars {
                used[k] = true;
            }
            for &j in diag.iter().filter(|&&j| dist(segs[j].0, p) < 0.02 || dist(segs[j].1, p) < 0.02) {
                used[j] = true;
                // The triangle's back: the segment through the slanted lines' other ends.
                let far = if dist(segs[j].0, p) < 0.02 { segs[j].1 } else { segs[j].0 };
                for (k, s) in segs.iter().enumerate() {
                    if !diag.contains(&k) && segment_distance(far, s.0, s.1) < 0.01 {
                        used[k] = true;
                    }
                }
            }
        }
    }
    // A chamfered corner: a slanted line between two square ones.
    for &i in &diag {
        let (a, b) = segs[i];
        if used[i] || dist(a, b) > 0.75 * small {
            continue;
        }
        if square_at(a, i) && square_at(b, i) {
            out.push(Feature { at: mid(a, b), kind: "chamfer" });
            used[i] = true;
        }
    }
    // A cathode bar across a two-pin part: a line square to the pins' axis, inside the body, not on its edge.
    if let Some(ax) = axis {
        for (i, &(a, b)) in segs.iter().enumerate() {
            if used[i] || !straight[i] {
                continue;
            }
            let d = (b.0 - a.0, b.1 - a.1);
            let len = d.0.hypot(d.1);
            let square = len > EPS && (d.0 * ax.0 + d.1 * ax.1).abs() / len < 0.05;
            let m = mid(a, b);
            let on_edge = (m.0 - body.x0).abs() < 0.06
                || (m.0 - body.x1).abs() < 0.06
                || (m.1 - body.y0).abs() < 0.06
                || (m.1 - body.y1).abs() < 0.06;
            let inside = m.0 > body.x0 && m.0 < body.x1 && m.1 > body.y0 && m.1 < body.y1;
            if square && inside && !on_edge {
                out.push(Feature { at: m, kind: "bar" });
                used[i] = true;
            }
        }
    }
    for (i, s) in segs.iter().enumerate() {
        if !used[i] {
            rest.push(*s);
        }
    }
    (out, rest)
}

/// `dot, chamfer ×4`.
fn kinds(marks: &[&Feature]) -> String {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for f in marks {
        match seen.iter_mut().find(|(k, _)| *k == f.kind) {
            Some((_, n)) => *n += 1,
            None => seen.push((f.kind, 1)),
        }
    }
    let parts: Vec<String> =
        seen.iter().map(|(k, n)| if *n == 1 { k.to_string() } else { format!("{k} ×{n}") }).collect();
    parts.join(", ")
}

#[derive(Debug, Clone, PartialEq)]
enum Verdict {
    Ok,
    Missing(String),
    Ambiguous(String),
    Wrong(String),
}

fn pad_at(fp: &Footprint, p: Pt) -> String {
    fp.copper_pads()
        .find(|q| dist(q.at, p) < SAME)
        .map_or_else(|| "another pad".to_string(), |q| format!("pad {}", q.number))
}

/// Judges the marks on `layer`.
fn judge(fp: &Footprint, layer: &str, p1: &Pad, axis: Option<Pt>) -> Verdict {
    let Some(body) = fp.fab_box().or_else(|| fp.pads_box()) else { return Verdict::Ok };
    let (c, moves) = moves(fp, p1);
    let (feats, rest) = features(fp, layer, body, axis);
    let mut worst = Verdict::Ok;
    let rank = |v: &Verdict| match v {
        Verdict::Ok => 0,
        Verdict::Ambiguous(_) => 1,
        Verdict::Wrong(_) => 2,
        Verdict::Missing(_) => 3,
    };
    for (m, to) in moves {
        let other = pad_at(fp, to);
        // A body that sits to one side of the pads (a right-angle header's) tells the move apart by itself.
        let moved = Rect::of_points(body.corners().map(|p| m.apply(p, c))).expect("four corners");
        let overlap = (moved.x1.min(body.x1) - moved.x0.max(body.x0)).max(0.0)
            * (moved.y1.min(body.y1) - moved.y0.max(body.y0)).max(0.0);
        if overlap < 0.8 * body.w() * body.h() {
            continue;
        }
        let marks: Vec<&Feature> = feats
            .iter()
            .filter(|f| !feats.iter().any(|g| g.kind == f.kind && dist(m.apply(f.at, c), g.at) < SAME_MARK))
            .collect();
        let near1 = marks.iter().filter(|f| dist(f.at, p1.at) < dist(f.at, to) - 0.01).count();
        let near2 = marks.iter().filter(|f| dist(f.at, to) < dist(f.at, p1.at) - 0.01).count();
        let v = if near1 > 0 && near2 == 0 {
            Verdict::Ok
        } else if near1 > 0 {
            Verdict::Ambiguous(format!("marks ({}) by pin 1 and by {other}", kinds(&marks)))
        } else if near2 > 0 {
            let by = fp
                .copper_pads()
                .min_by(|a, b| dist(a.at, marks[0].at).total_cmp(&dist(b.at, marks[0].at)))
                .map_or(other.clone(), |q| format!("pad {}", q.number));
            Verdict::Wrong(format!("the mark ({}) is by {by}, not pin 1", marks[0].kind))
        } else {
            // The outline: what of it the move does not map onto the drawing, and which pad it lies by.
            let on = |p: Pt| rest.iter().any(|(a, b)| segment_distance(p, *a, *b) < 0.03);
            let (mut by1, mut by2) = (0.0, 0.0);
            for (a, b) in &rest {
                let n = (dist(*a, *b) / 0.05).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let t = k as f64 / n as f64;
                    let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    if !on(m.apply(p, c)) {
                        let (d1, d2) = (dist(p, p1.at), dist(p, to));
                        if d1 < d2 - 0.01 {
                            by1 += 1.0;
                        } else if d2 < d1 - 0.01 {
                            by2 += 1.0;
                        }
                    }
                }
            }
            if by1 + by2 == 0.0 {
                Verdict::Missing(format!("nothing tells pin 1 from {other}"))
            } else if by1 >= 2.0 * by2 {
                Verdict::Ok
            } else if by2 >= 2.0 * by1 {
                Verdict::Wrong(format!("the outline's odd part is by {other}, not pin 1"))
            } else {
                Verdict::Ambiguous(format!("the outline is uneven by pin 1 and by {other}"))
            }
        };
        if rank(&v) > rank(&worst) {
            worst = v;
        }
    }
    worst
}

/// The pins' axis of a two-pin part: from pin 1 to the other pin, as a unit vector.
fn axis(fp: &Footprint, p1: &Pad) -> Option<Pt> {
    let mut numbers: Vec<&str> = fp.copper_pads().map(|p| p.number.as_str()).collect();
    numbers.sort();
    numbers.dedup();
    if numbers.len() != 2 {
        return None;
    }
    let other = fp.copper_pads().find(|p| p.number != p1.number)?;
    let (dx, dy) = (other.at.0 - p1.at.0, other.at.1 - p1.at.1);
    let len = dx.hypot(dy);
    (len > EPS).then(|| (dx / len, dy / len))
}

/// The fab outline drawn as one rectangle, with the items it is drawn with: of the straight drawings joined end to
/// end that make a rectangle, the largest round the pads' centre.
fn fab_rect<'a>(fp: &'a Footprint, layer: &'a str) -> Option<(Rect, Vec<&'a Graphic>)> {
    let items: Vec<&Graphic> = fp
        .on(layer)
        .filter(|g| !g.filled && g.circle.is_none() && matches!(g.kind.as_str(), "fp_line" | "fp_rect" | "fp_poly"))
        .collect();
    // Join items that share an end into groups.
    let mut group: Vec<usize> = (0..items.len()).collect();
    fn root(g: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while g[r] != r {
            r = g[r];
        }
        g[i] = r;
        r
    }
    let ends = |g: &Graphic| if g.closed { vec![] } else { vec![g.pts[0], *g.pts.last().expect("points")] };
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            if ends(items[i]).iter().any(|p| ends(items[j]).iter().any(|q| dist(*p, *q) < 0.005)) {
                let (a, b) = (root(&mut group, i), root(&mut group, j));
                group[a] = b;
            }
        }
    }
    let centre = fp.pads_box()?.center();
    let mut best: Option<(Rect, Vec<&Graphic>)> = None;
    for r0 in 0..items.len() {
        let members: Vec<&Graphic> =
            (0..items.len()).filter(|&i| root(&mut group, i) == r0).map(|i| items[i]).collect();
        if members.is_empty() {
            continue;
        }
        if let Some(r) = as_rect(&members).filter(|r| r.contains(centre))
            && best.as_ref().is_none_or(|(b, _)| r.w() * r.h() > b.w() * b.h())
        {
            best = Some((r, members));
        }
    }
    best
}

/// The outline with the corner nearest pin 1 cut off.
fn chamfered(r: Rect, p1: Pt) -> Vec<Pt> {
    let c = CHAMFER.min(0.25 * r.w().min(r.h()));
    let c = (c * 100.0).floor() / 100.0;
    let corners = r.corners();
    let k = (0..4)
        .min_by(|&a, &b| {
            dist(corners[a], p1).total_cmp(&dist(corners[b], p1)).then(corners[a].1.total_cmp(&corners[b].1))
        })
        .expect("four corners");
    let p = corners[k];
    let toward = |q: Pt| {
        let d = dist(p, q);
        (p.0 + (q.0 - p.0) / d * c, p.1 + (q.1 - p.1) / d * c)
    };
    // From the cut's end along the next side, round the other three corners, back to the cut's start.
    vec![
        toward(corners[(k + 1) % 4]),
        corners[(k + 1) % 4],
        corners[(k + 2) % 4],
        corners[(k + 3) % 4],
        toward(corners[(k + 3) % 4]),
    ]
}

/// A filled triangle pointing at pin 1, outside the body and clear of every pad, nearest pin 1.
fn triangle(fp: &Footprint, p1: &Pad, side: &str) -> Option<Vec<Pt>> {
    TRIANGLES.iter().find_map(|&size| triangle_of(fp, p1, side, size))
}

fn triangle_of(fp: &Footprint, p1: &Pad, side: &str, size: f64) -> Option<Vec<Pt>> {
    let body = fp.fab_box();
    // Near the courtyard, when there is one: at most [`PAST_COURTYARD`] past it (KiCad's own SOT-23 puts its
    // triangle a little outside).
    let court = Rect::of_points(fp.on(&fp.layer("CrtYd")).flat_map(|g| g.pts.iter().copied()))
        .map(|r| r.expand(PAST_COURTYARD));
    let half = size / 2.0;
    let stroke = super::silk::WIDTH / 2.0;
    let pads: Vec<&Pad> = fp.pads.iter().filter(|p| p.on_side(side)).collect();
    let margin = |p: &Pad| p.mask_margin.or(fp.mask_margin).unwrap_or(0.0).max(0.0);
    let pb = p1.outline.bbox();
    let start = (pb.w().hypot(pb.h())) / 2.0;
    let (c, moves) = moves(fp, p1);
    let mut best: Option<(f64, Vec<Pt>)> = None;
    for k in 0..32 {
        let th = k as f64 * std::f64::consts::TAU / 32.0;
        let u = (th.cos(), th.sin());
        let mut d = start;
        while d < start + 3.0 {
            let m = (p1.at.0 + u.0 * d, p1.at.1 + u.1 * d);
            // Apex towards the pad, base away.
            let apex = (m.0 - u.0 * half, m.1 - u.1 * half);
            let back = (m.0 + u.0 * half, m.1 + u.1 * half);
            let v = (-u.1 * half, u.0 * half);
            let tri = vec![apex, (back.0 + v.0, back.1 + v.1), (back.0 - v.0, back.1 - v.1)];
            let edges = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])];
            let clear = pads.iter().all(|p| {
                let need = margin(p) + stroke + TRIANGLE_GAP;
                !crate::footprint::inside_polygon(p.at, &tri)
                    && edges.iter().all(|(a, b)| p.outline.segment_distance(*a, *b) >= need)
            });
            let outside = body.is_none_or(|b| {
                let tb = Rect::of_points(tri.iter().copied()).expect("three points").expand(stroke);
                tb.x1 <= b.x0 || tb.x0 >= b.x1 || tb.y1 <= b.y0 || tb.y0 >= b.y1
            });
            let inside = court.is_none_or(|cy| {
                let tb = Rect::of_points(tri.iter().copied()).expect("three points").expand(stroke);
                tb.x0 >= cy.x0 && tb.x1 <= cy.x1 && tb.y0 >= cy.y0 && tb.y1 <= cy.y1
            });
            if clear && outside && !inside {
                break;
            }
            if clear && outside {
                let nearest = fp
                    .copper_pads()
                    .min_by(|a, b| a.outline.distance(m).total_cmp(&b.outline.distance(m)))
                    .is_some_and(|q| q.number == p1.number);
                let tells = moves.iter().all(|(_, to)| dist(m, p1.at) < dist(m, *to) - 0.01) && dist(c, m) > EPS;
                if nearest && tells && best.as_ref().is_none_or(|(bd, _)| d < *bd - 1e-9) {
                    let round = |p: Pt| ((p.0 * 1000.0).round() / 1000.0, (p.1 * 1000.0).round() / 1000.0);
                    best = Some((d, tri.into_iter().map(round).collect()));
                }
                break;
            }
            d += 0.02;
        }
    }
    best.map(|(_, t)| t)
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let fp = i.fp;
    if i.class != Class::Part || !needs_mark(i.lib, fp) {
        return out;
    }
    let p1 = pin1(fp).expect("needs_mark checks it");
    let ax = axis(fp, p1);
    let fab = fp.layer("Fab");
    let silk = fp.layer("SilkS");
    match judge(fp, &fab, p1, ax) {
        Verdict::Ok => {}
        Verdict::Missing(why) => match fab_rect(fp, &fab) {
            Some((r, items)) => {
                let width = items[0].width;
                out.fixed(format!("no pin-1 mark on {fab} ({why}): pin-1 corner chamfered"));
                for g in &items {
                    out.edits.push(delete(i.text, &g.span));
                }
                out.edits.push(insert(fp, render_graphic("fp_poly", &chamfered(r, p1.at), width, false, &fab)));
            }
            None => out.left(format!("no pin-1 mark on {fab} ({why}); the outline is not a rectangle to chamfer")),
        },
        Verdict::Ambiguous(why) => out.left(format!("pin-1 mark on {fab} ambiguous: {why}")),
        Verdict::Wrong(why) => out.left(format!("pin-1 mark on {fab} wrong: {why}")),
    }
    if fp.on(&silk).next().is_none() {
        return out;
    }
    match judge(fp, &silk, p1, ax) {
        Verdict::Ok => {}
        Verdict::Missing(why) => match triangle(fp, p1, fp.side()) {
            Some(tri) => {
                out.fixed(format!(
                    "no pin-1 mark on {silk} ({why}): triangle added at ({}, {})",
                    fmt((tri[0].0 + tri[1].0 + tri[2].0) / 3.0),
                    fmt((tri[0].1 + tri[1].1 + tri[2].1) / 3.0)
                ));
                out.edits.push(insert(fp, render_graphic("fp_poly", &tri, super::silk::WIDTH, true, &silk)));
            }
            None => out.left(format!("no pin-1 mark on {silk} ({why}); no clear spot for a triangle")),
        },
        Verdict::Ambiguous(why) => out.left(format!("pin-1 mark on {silk} ambiguous: {why}")),
        Verdict::Wrong(why) => out.left(format!("pin-1 mark on {silk} wrong: {why}")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};
    use super::*;

    fn line(a: Pt, b: Pt, layer: &str) -> String {
        format!(
            "(fp_line (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (layer \"{layer}\"))",
            a.0, a.1, b.0, b.1
        )
    }

    /// A SOIC-8-like part: pins 1-4 down the left, 5-8 up the right.
    fn soic(extra: &str) -> String {
        let mut pads = String::new();
        for k in 0..4 {
            let y = -1.905 + 1.27 * k as f64;
            pads +=
                &format!("(pad \"{}\" smd rect (at -2.5 {y}) (size 1.5 0.6) (layers \"F.Cu\" \"F.Mask\"))\n", k + 1);
            pads +=
                &format!("(pad \"{}\" smd rect (at 2.5 {}) (size 1.5 0.6) (layers \"F.Cu\" \"F.Mask\"))\n", 8 - k, y);
        }
        fp("SOIC-8", &format!("{extra}\n{pads}"))
    }

    const RECT: &str =
        r#"(fp_rect (start -1.95 -2.45) (end 1.95 2.45) (stroke (width 0.05) (type solid)) (fill no) (layer "F.Fab"))"#;

    #[test]
    fn a_plain_rectangle_gets_a_chamfer() {
        let t = soic(RECT);
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Pin1);
        let new = new.unwrap();
        assert!(
            f[0].fixed && f[0].message.starts_with("no pin-1 mark on F.Fab (nothing tells pin 1 from pad "),
            "{f:?}"
        );
        assert!(!new.contains("fp_rect"), "{new}");
        // The corner by pin 1 (top left) cut by 0.97 (a quarter of 3.9, under 1 mm).
        assert!(new.contains("(xy -0.98 -2.45)") && new.contains("(xy -1.95 -1.48)"), "{new}");
        settled("VHRD_Package_SO", &t, Rule::Pin1);
    }

    #[test]
    fn chamfer_dot_and_wrong_side() {
        let chamfer = [
            line((-0.95, -2.45), (1.95, -2.45), "F.Fab"),
            line((1.95, -2.45), (1.95, 2.45), "F.Fab"),
            line((1.95, 2.45), (-1.95, 2.45), "F.Fab"),
            line((-1.95, 2.45), (-1.95, -1.45), "F.Fab"),
            line((-1.95, -1.45), (-0.95, -2.45), "F.Fab"),
        ]
        .join("\n");
        let (new, f) = run("VHRD_Package_SO", &soic(&chamfer), Rule::Pin1);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
        let dot = format!(
            "{RECT}\n(fp_circle (center -1.2 -1.7) (end -1 -1.7) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.Fab\"))"
        );
        let (new, f) = run("VHRD_Package_SO", &soic(&dot), Rule::Pin1);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
        let wrong = format!(
            "{RECT}\n(fp_circle (center 1.2 1.7) (end 1.4 1.7) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.Fab\"))"
        );
        let (_, f) = run("VHRD_Package_SO", &soic(&wrong), Rule::Pin1);
        assert_eq!(f[0].message, "pin-1 mark on F.Fab wrong: the mark (dot) is by pad 5, not pin 1", "{f:?}");
    }

    #[test]
    fn silk_without_a_mark_gets_a_triangle() {
        let silk = [line((-1.3, -2.6), (1.3, -2.6), "F.SilkS"), line((-1.3, 2.6), (1.3, 2.6), "F.SilkS")].join("\n");
        let dot = "(fp_circle (center -1.2 -1.7) (end -1 -1.7) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.Fab\"))";
        let t = soic(&format!("{RECT}\n{dot}\n{silk}"));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Pin1);
        let new = new.unwrap();
        assert!(f[0].fixed && f[0].message.contains("triangle added"), "{f:?}");
        assert!(new.contains("(fill yes)\n\t\t(layer \"F.SilkS\")"), "{new}");
        settled("VHRD_Package_SO", &t, Rule::Pin1);
        // A line running on past pin 1 is a mark.
        let silk = [line((-3.45, -2.6), (1.3, -2.6), "F.SilkS"), line((-1.3, 2.6), (1.3, 2.6), "F.SilkS")].join("\n");
        let (new, f) = run("VHRD_Package_SO", &soic(&format!("{RECT}\n{dot}\n{silk}")), Rule::Pin1);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
    }

    fn diode(fab: &str) -> String {
        let pads = r#"(pad "1" smd rect (at -0.7 0) (size 0.5 0.5) (layers "F.Cu" "F.Mask"))
(pad "2" smd rect (at 0.7 0) (size 0.5 0.5) (layers "F.Cu" "F.Mask"))"#;
        fp("D_SOD-523", &format!("{fab}\n{pads}"))
    }

    #[test]
    fn diodes_by_their_symbol_or_bar() {
        let outline = r#"(fp_rect (start -0.65 -0.45) (end 0.65 0.45) (stroke (width 0.05) (type solid)) (fill no) (layer "F.Fab"))"#;
        let symbol = [
            outline.to_string(),
            line((-0.2, 0.0), (-0.35, 0.0), "F.Fab"),
            line((-0.2, 0.0), (0.1, 0.2), "F.Fab"),
            line((-0.2, 0.2), (-0.2, -0.2), "F.Fab"),
            line((0.1, -0.2), (-0.2, 0.0), "F.Fab"),
            line((0.1, 0.0), (0.25, 0.0), "F.Fab"),
            line((0.1, 0.2), (0.1, -0.2), "F.Fab"),
        ]
        .join("\n");
        let (new, f) = run("VHRD_Diode_SMD", &diode(&symbol), Rule::Pin1);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
        let bar = format!("{outline}\n{}", line((-0.4, -0.45), (-0.4, 0.45), "F.Fab"));
        let (new, f) = run("VHRD_Diode_SMD", &diode(&bar), Rule::Pin1);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
        let (new, f) = run("VHRD_Diode_SMD", &diode(outline), Rule::Pin1);
        assert!(new.is_some() && f[0].message.contains("chamfered"), "{f:?}");
        // Not polar: no mark wanted.
        let bidir = diode(outline).replace("D_SOD-523", "D_SOD-523_BIDIR");
        assert!(run("VHRD_Diode_SMD", &bidir, Rule::Pin1).1.is_empty());
        assert!(run("VHRD_Resistor_SMD", &diode(outline).replace("D_SOD-523", "R_0402"), Rule::Pin1).1.is_empty());
    }
}
