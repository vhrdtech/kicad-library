//! A pin-1 dot on Pin1.F (User.29; Pin1.B, User.30, on the back), shown together with the body (Body.F/B), which
//! has no pin-1 mark of its own. The parts are those of [`super::pin1`]: three or more pins, and polar two-pin parts
//! (the dot is by the cathode or plus pad). Test points, net ties, mechanical and graphic footprints, and non-polar
//! two-pin parts get none.
//!
//! The dot is filled, [`DIAMETER_MIN`] to [`DIAMETER_MAX`] across (a tenth of the body's shorter side, and under the
//! distance from pin 1 to its nearest neighbour). It sits as near pin 1 as it can inside the body outline, [`GAP`]
//! clear of its edge, off the pads where it can be; where it does not fit there, just outside the body, [`GAP`] away. It is never ambiguous: every turn of the
//! part that fits its pads (and does not tell itself apart by the body) takes pin 1 to a pad the dot is farther
//! from, and no other pad is as near. A part with no such place is left for a person. The layer belongs to this
//! rule: whatever else is there is replaced.

use super::pin1::{body_tells, moves, needs_mark, pin1};
use super::{Class, Input, Outcome, delete, insert};
use crate::footprint::{Footprint, Pad, Pt, Rect, dist, fmt, render_graphic};
use crate::layers::{BODY, PIN1, side_of};

pub const DIAMETER_MIN: f64 = 0.3;
pub const DIAMETER_MAX: f64 = 0.8;
/// The dot's distance from the body's edge, inside or out.
pub const GAP: f64 = 0.05;
/// Where a dot may be put: this far round pin 1.
const REACH: f64 = 4.0;
const GRID: f64 = 0.05;
/// A dot clear of the pads is preferred: it keeps this far from every pad.
const CLEAR: f64 = 0.05;
/// A dot must be this much nearer pin 1 than any other pad.
const TELLS: f64 = 0.01;

/// The body outline: what is drawn on the body layer, else the fab outline, else the pads.
fn body(fp: &Footprint) -> Option<Rect> {
    let layer = side_of(BODY, fp.back);
    let drawn = fp.on(layer).filter_map(|g| g.bbox().map(|b| b.expand(g.width / 2.0))).reduce(|a, b| a.union(&b));
    drawn.or_else(|| fp.fab_box()).or_else(|| fp.pads_box())
}

fn floor_to(v: f64, step: f64) -> f64 {
    ((v / step) + 1e-9).floor() * step
}

/// The dot's diameter for a part of `body`, with pin 1 `neighbour` mm from its nearest other pad.
fn diameter(body: Rect, neighbour: Option<f64>) -> f64 {
    let d = floor_to((0.1 * body.w().min(body.h())).clamp(DIAMETER_MIN, DIAMETER_MAX), GRID);
    neighbour.map_or(d, |n| d.min(floor_to(0.9 * n, GRID)).max(0.2))
}

/// Whether a dot of radius `r` at `c` is inside the body, [`GAP`] from its edge.
fn inside(body: Rect, c: Pt, r: f64) -> bool {
    c.0 - r >= body.x0 + GAP && c.0 + r <= body.x1 - GAP && c.1 - r >= body.y0 + GAP && c.1 + r <= body.y1 - GAP
}

/// Whether a dot of radius `r` at `c` is outside the body, at least [`GAP`] from it.
fn outside(body: Rect, c: Pt, r: f64) -> bool {
    !body.expand(r + GAP - 1e-9).contains(c)
}

/// The nearest place to pin 1, inside the body first, else outside it, where the dot tells pin 1 from every pad a
/// turn of the part takes it to and from every other pad.
fn place(fp: &Footprint, p1: &Pad, body: Rect, d: f64) -> Option<Pt> {
    let r = d / 2.0;
    let (c, turns) = moves(fp, p1);
    let targets: Vec<Pt> = turns.into_iter().filter(|&(m, _)| !body_tells(body, m, c)).map(|(_, to)| to).collect();
    let others: Vec<&Pad> = fp.copper_pads().filter(|p| p.number != p1.number).collect();
    let mine: Vec<&Pad> = fp.copper_pads().filter(|p| p.number == p1.number).collect();
    let tells = |m: Pt| {
        let near = mine.iter().map(|p| p.outline.distance(m)).fold(f64::MAX, f64::min);
        targets.iter().all(|&to| dist(m, p1.at) < dist(m, to) - TELLS)
            && others.iter().all(|p| p.outline.distance(m) > near + TELLS)
    };
    let steps = (REACH / GRID) as i32;
    let at = |k: i32, origin: f64| ((origin + f64::from(k) * GRID) / GRID).round() * GRID;
    let mut found: Vec<(bool, bool, f64, Pt)> = Vec::new();
    for i in -steps..=steps {
        for j in -steps..=steps {
            let m = (at(i, p1.at.0), at(j, p1.at.1));
            let inner = inside(body, m, r);
            if inner || outside(body, m, r) {
                let clear = fp.copper_pads().all(|p| p.outline.distance(m) >= r + CLEAR);
                found.push((!inner, !clear, dist(m, p1.at), m));
            }
        }
    }
    found.sort_by(|a, b| {
        (a.0, a.1)
            .cmp(&(b.0, b.1))
            .then(a.2.total_cmp(&b.2))
            .then(a.3.0.total_cmp(&b.3.0))
            .then(a.3.1.total_cmp(&b.3.1))
    });
    found.into_iter().map(|(_, _, _, m)| m).find(|&m| tells(m))
}

/// Pin 1's distance to the nearest pad of another number.
fn neighbour(fp: &Footprint, p1: &Pad) -> Option<f64> {
    fp.copper_pads().filter(|p| p.number != p1.number).map(|p| dist(p.at, p1.at)).reduce(f64::min)
}

fn is_dot(g: &crate::footprint::Graphic, c: Pt, r: f64) -> bool {
    g.kind == "fp_circle" && g.filled && g.circle.is_some_and(|(gc, gr)| dist(gc, c) < 1e-3 && (gr - r).abs() < 1e-3)
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let fp = i.fp;
    let layer = side_of(PIN1, fp.back);
    let have: Vec<_> = fp.on(layer).collect();
    let texts: Vec<_> = fp.texts.iter().filter(|t| t.layer == layer).collect();
    let stray = have.len() + texts.len();
    let wanted = i.class == Class::Part && needs_mark(i.lib, fp);
    let spot = if wanted {
        let p1 = pin1(fp).expect("needs_mark checks it");
        let Some(b) = body(fp) else {
            out.left(format!("{layer}: no body or pads to place the dot by"));
            return out;
        };
        let d = diameter(b, neighbour(fp, p1));
        match place(fp, p1, b, d) {
            Some(m) => Some((m, d, inside(b, m, d / 2.0))),
            None => {
                out.left(format!("{layer}: no place for a dot that tells pin 1 from the other pads"));
                return out;
            }
        }
    } else {
        None
    };
    let fine = match spot {
        Some((m, d, _)) => have.len() == 1 && texts.is_empty() && is_dot(have[0], m, d / 2.0),
        None => stray == 0,
    };
    if fine {
        return out;
    }
    let had = if stray == 0 { String::new() } else { format!(" (replacing {stray})") };
    match spot {
        Some((m, d, in_body)) => out.fixed(format!(
            "{layer}: pin-1 dot ⌀{} at ({}, {}), {}{had}",
            fmt(d),
            fmt(m.0),
            fmt(m.1),
            if in_body { "in the body" } else { "by the body" }
        )),
        None => out.fixed(format!("{layer}: no dot, no pin 1 to mark{had}")),
    }
    for g in &have {
        out.edits.push(delete(i.text, &g.span));
    }
    for t in &texts {
        out.edits.push(delete(i.text, &t.span));
    }
    if let Some((m, d, _)) = spot {
        out.edits.push(insert(fp, render_graphic("fp_circle", &[m, (m.0 + d / 2.0, m.1)], 0.0, true, layer)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};

    fn rect(w: f64, h: f64, layer: &str) -> String {
        format!(
            "(fp_rect (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (fill no) (layer \"{layer}\"))",
            -w / 2.0,
            -h / 2.0,
            w / 2.0,
            h / 2.0
        )
    }

    fn pad(n: &str, x: f64, y: f64, w: f64, h: f64) -> String {
        format!("(pad \"{n}\" smd rect (at {x} {y}) (size {w} {h}) (layers \"F.Cu\" \"F.Mask\"))")
    }

    fn sot23() -> String {
        let pads = [pad("1", -1.1, -0.95, 1.0, 0.6), pad("2", -1.1, 0.95, 1.0, 0.6), pad("3", 1.1, 0.0, 1.0, 0.6)];
        fp("SOT-23", &format!("{}\n{}", rect(1.3, 2.9, "User.5"), pads.join("\n")))
    }

    #[test]
    fn a_three_pin_part_gets_a_dot_in_the_body_by_pin_1() {
        let t = sot23();
        let (new, f) = run("VHRD_Package_TO_SOT_SMD", &t, Rule::Pin1Body);
        let new = new.unwrap();
        assert!(
            f[0].fixed && f[0].message.starts_with("User.29: pin-1 dot ⌀0.3 at (-0.4, -0.95), in the body"),
            "{f:?}"
        );
        assert!(
            new.contains("(center -0.4 -0.95)\n\t\t(end -0.25 -0.95)")
                && new.contains("(fill yes)\n\t\t(layer \"User.29\")"),
            "{new}"
        );
        settled("VHRD_Package_TO_SOT_SMD", &t, Rule::Pin1Body);
    }

    #[test]
    fn a_polar_two_pin_part_marks_the_cathode_side_and_a_plain_one_nothing() {
        let diode = |name: &str| {
            fp(
                name,
                &format!(
                    "{}\n{}\n{}",
                    rect(2.7, 1.6, "User.5"),
                    pad("1", -1.65, 0.0, 0.9, 1.2),
                    pad("2", 1.65, 0.0, 0.9, 1.2)
                ),
            )
        };
        let (new, _) = run("VHRD_Diode_SMD", &diode("D_SOD-123"), Rule::Pin1Body);
        let new = new.unwrap();
        assert!(new.contains("(center -0.95 0)"), "{new}");
        assert!(run("VHRD_Resistor_SMD", &diode("R_0402"), Rule::Pin1Body).0.is_none());
        assert!(run("VHRD_TestPoint", &diode("TestPoint_Pad"), Rule::Pin1Body).0.is_none());
    }

    #[test]
    fn a_dot_on_a_part_that_wants_none_is_removed_and_a_wrong_one_replaced() {
        let t = fp(
            "R_0402",
            &format!(
                "{}\n{}\n{}\n(fp_circle (center 0 0) (end 0.2 0) (stroke (width 0) (type solid)) (fill yes) (layer \"User.29\"))",
                rect(1.0, 0.5, "User.5"),
                pad("1", -0.5, 0.0, 0.5, 0.5),
                pad("2", 0.5, 0.0, 0.5, 0.5)
            ),
        );
        let (new, f) = run("VHRD_Resistor_SMD", &t, Rule::Pin1Body);
        assert!(!new.unwrap().contains("User.29") && f[0].message.ends_with("(replacing 1)"), "{f:?}");
        let wrong = sot23().replace(
            "(pad \"1\"",
            "(fp_circle (center 0 0) (end 0.2 0) (stroke (width 0) (type solid)) (fill yes) (layer \"User.29\"))\n(pad \"1\"",
        );
        let (new, _) = run("VHRD_Package_TO_SOT_SMD", &wrong, Rule::Pin1Body);
        assert_eq!(new.unwrap().matches("User.29").count(), 1);
    }

    #[test]
    fn a_square_part_has_one_dot_whatever_turn_its_pads_allow() {
        // Four pads of a square part: a quarter turn fits them, so the dot must sit nearer pin 1 than pads 2 to 4.
        let pads = [
            pad("1", -1.5, -0.5, 1.0, 0.5),
            pad("2", 1.5, -0.5, 1.0, 0.5),
            pad("3", 1.5, 0.5, 1.0, 0.5),
            pad("4", -1.5, 0.5, 1.0, 0.5),
        ];
        let t = fp("QFN-4", &format!("{}\n{}", rect(2.0, 2.0, "User.5"), pads.join("\n")));
        let (new, _) = run("VHRD_Package_DFN_QFN", &t, Rule::Pin1Body);
        let new = new.unwrap();
        assert!(new.contains("(center -0.75 -0.5)"), "{new}");
        settled("VHRD_Package_DFN_QFN", &t, Rule::Pin1Body);
    }

    #[test]
    fn a_back_footprint_uses_the_back_layer() {
        let t = sot23().replace("User.5", "User.6").replace("F.Cu", "B.Cu").replace("F.Mask", "B.Mask");
        let (new, _) = run("VHRD_Package_TO_SOT_SMD", &t, Rule::Pin1Body);
        assert!(new.unwrap().contains("(layer \"User.30\")"));
    }
}
