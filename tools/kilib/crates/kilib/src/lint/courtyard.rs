//! Courtyards: every part and board feature with pads has one on its side's courtyard layer (F.CrtYd, B.CrtYd for a
//! bottom-side footprint), unless its attributes allow it to have none; it is closed, and it keeps at least
//! [`CLEARANCE`] from every pad and from the body (the drawing on the fab layer), [`SMALL_CLEARANCE`] for 0201,
//! 0402 and 0603 passives (IPC-7351 nominal density). Distances are to the courtyard line's centre, as KiCad and
//! IPC measure them.
//!
//! The fix draws a missing courtyard as a rectangle round the pads and the body at the clearance, on a
//! [`GRID`] grid, [`WIDTH`] wide; a courtyard that is one rectangle and too tight grows to that rectangle (it never
//! shrinks). A courtyard of another shape that is too tight or open is reported: its shape was drawn on purpose.

use super::{Class, Input, Outcome, delete, insert, round_out};
use crate::footprint::{EPS, Footprint, Graphic, Pt, Rect, dist, render_graphic};

pub const CLEARANCE: f64 = 0.25;
pub const SMALL_CLEARANCE: f64 = 0.1;
pub const GRID: f64 = 0.01;
pub const WIDTH: f64 = 0.05;
/// Distances that fall short by less than this pass: half the grid a courtyard is drawn on.
const SLACK: f64 = 0.005;

/// A 0201, 0402 or 0603 (or metric 0603, 1005, 1608) resistor, capacitor, inductor, ferrite, fuse, diode or LED.
pub fn small_passive(lib: &str, name: &str) -> bool {
    let lib = lib.strip_prefix("VHRD_").unwrap_or(lib);
    let passive = ["Resistor", "Capacitor", "Inductor", "Fuse", "Diode", "LED"].iter().any(|k| lib.starts_with(k))
        || ["R_", "C_", "L_", "D_", "LED_", "Fuse_", "FB_"].iter().any(|p| name.starts_with(p));
    let size =
        name.split(['_', '-']).any(|w| ["0201", "0402", "0603", "0603Metric", "1005Metric", "1608Metric"].contains(&w));
    passive && size
}

pub fn clearance(i: &Input) -> f64 {
    if small_passive(i.lib, &i.fp.name) { SMALL_CLEARANCE } else { CLEARANCE }
}

/// Where the courtyard's lines do not meet another line's end: an open courtyard.
fn open_ends(items: &[&Graphic]) -> Vec<Pt> {
    let mut ends: Vec<(Pt, usize)> = Vec::new();
    for g in items.iter().filter(|g| !g.closed) {
        for p in [g.pts[0], *g.pts.last().expect("a graphic has points")] {
            match ends.iter_mut().find(|(q, _)| dist(p, *q) < 0.005) {
                Some((_, n)) => *n += 1,
                None => ends.push((p, 1)),
            }
        }
    }
    ends.into_iter().filter(|(_, n)| n % 2 == 1).map(|(p, _)| p).collect()
}

/// The courtyard as one rectangle, when it is drawn as one: a `fp_rect`, a four-cornered square polygon, or four
/// lines round a box.
pub(crate) fn as_rect(items: &[&Graphic]) -> Option<Rect> {
    let segs: Vec<(Pt, Pt)> = items.iter().flat_map(|g| g.segments()).collect();
    if !items
        .iter()
        .all(|g| g.axis_aligned() && g.circle.is_none() && matches!(g.kind.as_str(), "fp_rect" | "fp_poly" | "fp_line"))
    {
        return None;
    }
    let r = Rect::of_points(items.iter().flat_map(|g| g.pts.iter().copied()))?;
    let on_edge = |p: Pt| {
        ((p.0 - r.x0).abs() < EPS || (p.0 - r.x1).abs() < EPS) || ((p.1 - r.y0).abs() < EPS || (p.1 - r.y1).abs() < EPS)
    };
    let edges_covered = (r.w() + r.h()) * 2.0;
    let drawn: f64 = segs.iter().map(|(a, b)| dist(*a, *b)).sum();
    let all_on_edge =
        segs.iter().all(|(a, b)| on_edge(*a) && on_edge(*b) && on_edge(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)));
    (all_on_edge && (drawn - edges_covered).abs() < 0.01 && open_ends(items).is_empty()).then_some(r)
}

/// The box everything must stay inside, before the clearance: the pads and the body.
fn content(fp: &Footprint) -> Option<Rect> {
    let pads = fp.pads_box();
    match (pads, body_box(fp)) {
        (Some(p), Some(b)) => Some(p.union(&b)),
        (p, b) => p.or(b),
    }
}

/// The body's box: the drawing on the fab layer (without its strokes).
fn body_box(fp: &Footprint) -> Option<Rect> {
    Rect::of_points(fp.on(&fp.layer("Fab")).flat_map(|g| g.pts.iter().copied()))
}

/// What breaks the clearance: each pad or fab line that comes too close or sits outside.
fn violations(fp: &Footprint, segs: &[(Pt, Pt)], c: f64) -> Vec<String> {
    let mut out = Vec::new();
    let poly_inside = |p: Pt| {
        // Even-odd over every courtyard line: works for one loop or several.
        let mut inside = false;
        for (a, b) in segs {
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                inside = !inside;
            }
        }
        inside
    };
    let mut worst_pad: Option<(f64, String)> = None;
    for pad in &fp.pads {
        let d = segs.iter().map(|(a, b)| pad.outline.segment_distance(*a, *b)).fold(f64::MAX, f64::min);
        let d = if poly_inside(pad.at) { d } else { -1.0 };
        if d < c - SLACK && worst_pad.as_ref().is_none_or(|(w, _)| d < *w) {
            worst_pad = Some((d, pad.number.clone()));
        }
    }
    if let Some((d, n)) = worst_pad {
        let name = if n.is_empty() { "a hole".to_string() } else { format!("pad {n}") };
        out.push(if d < 0.0 {
            format!("{name} outside the courtyard")
        } else {
            format!("{name} {} mm from the courtyard", crate::footprint::fmt(d))
        });
    }
    let fab = fp.layer("Fab");
    let mut worst_body: Option<f64> = None;
    for g in fp.on(&fab) {
        for (a, b) in g.segments() {
            let d =
                segs.iter().map(|(p, q)| crate::footprint::segments_distance(a, b, *p, *q)).fold(f64::MAX, f64::min);
            let d = if poly_inside(a) && poly_inside(b) { d } else { -1.0 };
            if d < c - SLACK {
                worst_body = Some(worst_body.map_or(d, |w| w.min(d)));
            }
        }
    }
    if let Some(d) = worst_body {
        out.push(if d < 0.0 {
            "the body (fab drawing) reaches outside the courtyard".to_string()
        } else {
            format!("the body (fab drawing) {} mm from the courtyard", crate::footprint::fmt(d))
        });
    }
    out
}

/// The rectangle a courtyard should at least be.
fn wanted(fp: &Footprint, c: f64) -> Option<Rect> {
    let r = content(fp)?.expand(c);
    Some(Rect {
        x0: round_out(r.x0, GRID, false),
        y0: round_out(r.y0, GRID, false),
        x1: round_out(r.x1, GRID, true),
        y1: round_out(r.y1, GRID, true),
    })
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let fp = i.fp;
    if matches!(i.class, Class::Graphic) || fp.pads.is_empty() {
        return out;
    }
    // Panel features and hardware: a courtyard there would clash with the parts beside it on purpose.
    let draw = i.class != Class::Mechanical;
    let layer = fp.layer("CrtYd");
    let other = if fp.back { "F.CrtYd" } else { "B.CrtYd" };
    let items: Vec<&Graphic> = fp.on(&layer).collect();
    let c = clearance(i);
    let cmm = crate::footprint::fmt(c);
    if items.is_empty() {
        if fp.attr.has("allow_missing_courtyard") {
            return out;
        }
        if fp.on(other).next().is_some() {
            out.left(format!("courtyard only on {other}"));
            return out;
        }
        let Some(r) = wanted(fp, c) else { return out };
        if !draw {
            out.left("no courtyard (mechanical: none drawn)");
            return out;
        }
        out.fixed(format!("no courtyard: drawn round the pads and body at {cmm} mm"));
        out.edits.push(insert(fp, render_graphic("fp_rect", &[(r.x0, r.y0), (r.x1, r.y1)], WIDTH, false, &layer)));
        return out;
    }
    let open = open_ends(&items);
    if !open.is_empty() {
        let at: Vec<String> = open
            .iter()
            .take(4)
            .map(|p| format!("({}, {})", crate::footprint::fmt(p.0), crate::footprint::fmt(p.1)))
            .collect();
        out.left(format!("courtyard not closed: open at {}", at.join(", ")));
        return out;
    }
    let segs: Vec<(Pt, Pt)> = items.iter().flat_map(|g| g.segments()).collect();
    let bad = violations(fp, &segs, c);
    if bad.is_empty() {
        return out;
    }
    let why = bad.join("; ");
    match (as_rect(&items), wanted(fp, c)) {
        (Some(have), Some(want)) => {
            let r = have.union(&want);
            out.fixed(format!("courtyard grown to {cmm} mm clearance ({why})"));
            for g in &items {
                out.edits.push(delete(i.text, &g.span));
            }
            out.edits.push(insert(fp, render_graphic("fp_rect", &[(r.x0, r.y0), (r.x1, r.y1)], WIDTH, false, &layer)));
        }
        _ => out.left(format!("courtyard under {cmm} mm clearance, not a rectangle to grow: {why}")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};
    use super::*;

    const PADS: &str = r#"(pad "1" smd rect (at -0.5 0) (size 0.5 0.6) (layers "F.Cu" "F.Mask"))
(pad "2" smd rect (at 0.5 0) (size 0.5 0.6) (layers "F.Cu" "F.Mask"))
(fp_rect (start -0.5 -0.25) (end 0.5 0.25) (stroke (width 0.05) (type solid)) (fill no) (layer "F.Fab"))"#;

    #[test]
    fn small_passives() {
        assert!(small_passive("VHRD_Resistor_SMD", "R_0402_1005Metric"));
        assert!(small_passive("VHRD_Capacitor_SMD", "C_0603_1608Metric_Pad1.08x0.95mm_HandSolder"));
        assert!(!small_passive("VHRD_Capacitor_SMD", "C_1206_3216Metric"));
        assert!(!small_passive("VHRD_Package_DFN_QFN", "QFN-16_0402"));
    }

    #[test]
    fn a_missing_courtyard_is_drawn() {
        let t = fp("X", PADS);
        let (new, f) = run("VHRD_X", &t, Rule::Courtyard);
        let new = new.unwrap();
        // Pads reach ±0.75 and ±0.3: at 0.25 mm, ±1 and ±0.55.
        assert!(new.contains("(start -1 -0.55)\n\t\t(end 1 0.55)"), "{new}");
        assert!(new.contains("(layer \"F.CrtYd\")"));
        assert!(f[0].fixed);
        settled("VHRD_X", &t, Rule::Courtyard);
        // A small passive gets 0.1 mm.
        let new = run("VHRD_Resistor_SMD", &fp("R_0402_1005Metric", PADS), Rule::Courtyard).0.unwrap();
        assert!(new.contains("(start -0.85 -0.4)\n\t\t(end 0.85 0.4)"), "{new}");
    }

    #[test]
    fn a_tight_rectangle_grows_and_other_shapes_are_reported() {
        let lines = r#"(fp_line (start -0.9 -0.5) (end 0.9 -0.5) (stroke (width 0.05) (type solid)) (layer "F.CrtYd"))
(fp_line (start 0.9 -0.5) (end 0.9 0.5) (stroke (width 0.05) (type solid)) (layer "F.CrtYd"))
(fp_line (start 0.9 0.5) (end -0.9 0.5) (stroke (width 0.05) (type solid)) (layer "F.CrtYd"))
(fp_line (start -0.9 0.5) (end -0.9 -0.5) (stroke (width 0.05) (type solid)) (layer "F.CrtYd"))"#;
        let t = fp("X", &format!("{lines}\n{PADS}"));
        let (new, f) = run("VHRD_X", &t, Rule::Courtyard);
        let new = new.unwrap();
        assert_eq!(new.matches("F.CrtYd").count(), 1, "{new}");
        assert!(new.contains("(start -1 -0.55)\n\t\t(end 1 0.55)"), "{new}");
        assert!(f[0].message.contains("pad 1 0.15 mm from the courtyard"), "{f:?}");
        settled("VHRD_X", &t, Rule::Courtyard);
        let poly = r#"(fp_poly (pts (xy -0.9 -0.5) (xy 0.9 -0.5) (xy 0.9 0.5) (xy 0 0.6) (xy -0.9 0.5)) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))"#;
        let (new, f) = run("VHRD_X", &fp("X", &format!("{poly}\n{PADS}")), Rule::Courtyard);
        assert!(new.is_none());
        assert!(f[0].message.starts_with("courtyard under 0.25 mm clearance, not a rectangle"), "{f:?}");
    }

    #[test]
    fn open_courtyards_are_reported() {
        let lines = r#"(fp_line (start -1 -0.6) (end 1 -0.6) (stroke (width 0.05) (type solid)) (layer "F.CrtYd"))
(fp_line (start 1 -0.6) (end 1 0.6) (stroke (width 0.05) (type solid)) (layer "F.CrtYd"))"#;
        let (new, f) = run("VHRD_X", &fp("X", &format!("{lines}\n{PADS}")), Rule::Courtyard);
        assert!(new.is_none());
        assert_eq!(f[0].message, "courtyard not closed: open at (-1, -0.6), (1, 0.6)");
    }

    #[test]
    fn a_wide_enough_courtyard_passes() {
        let rect = r#"(fp_rect (start -1.2 -0.8) (end 1.2 0.8) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))"#;
        let (new, f) = run("VHRD_X", &fp("X", &format!("{rect}\n{PADS}")), Rule::Courtyard);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
    }
}
