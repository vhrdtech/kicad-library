//! A drawing on the reference layers, so the layer shows the part and not just its designator:
//!
//! - a test point (Ref.Test, User.9 front, User.10 back): a circle on its pad's outline, [`MARGIN`] out, or for a
//!   pad that is not round, a rectangle round it;
//! - a net tie, spark gap or solder jumper (Ref.Tie, User.11 / User.12): a rectangle round all its pads, [`MARGIN`]
//!   out;
//! - a mechanical footprint (Ref.Mech): nothing; its designator stays hidden.
//!
//! Every line is [`WIDTH`]. The layer belongs to the designator and this drawing alone: the fix replaces whatever
//! else is drawn there. [`crate::fab`] then fits the designator inside the drawing, or beside it where it is too
//! small.

use super::{Input, Outcome, delete, insert};
use crate::footprint::{Outline, Pt, Rect, dist, fmt, render_graphic};
use crate::layers::{Class, class_of};

pub const MARGIN: f64 = 0.05;
pub const WIDTH: f64 = 0.05;

/// What should be drawn: a circle (centre, a point on it) or a rectangle (two corners).
#[derive(Debug, Clone, PartialEq)]
enum Wanted {
    Circle(Pt, f64),
    Rect(Rect),
}

fn r4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn wanted(i: &Input, class: Class) -> Option<Wanted> {
    let pads: Vec<_> = i.fp.copper_pads().collect();
    let bbox = Rect::of_points(pads.iter().flat_map(|p| p.outline.points()))?;
    match class {
        Class::Test => match pads.as_slice() {
            [p] => match &p.outline {
                Outline::Capsule { a, b, r } if dist(*a, *b) < 1e-9 => Some(Wanted::Circle(*a, r + MARGIN)),
                _ => Some(Wanted::Rect(bbox.expand(MARGIN))),
            },
            _ => Some(Wanted::Rect(bbox.expand(MARGIN))),
        },
        Class::Tie => Some(Wanted::Rect(bbox.expand(MARGIN))),
        Class::Part | Class::Mech => None,
    }
}

fn render(w: &Wanted, layer: &str) -> String {
    match w {
        Wanted::Circle(c, r) => render_graphic("fp_circle", &[*c, (c.0 + r, c.1)], WIDTH, false, layer),
        Wanted::Rect(r) => render_graphic("fp_rect", &[(r.x0, r.y0), (r.x1, r.y1)], WIDTH, false, layer),
    }
}

fn matches(g: &crate::footprint::Graphic, w: &Wanted) -> bool {
    if (g.width - WIDTH).abs() > 1e-9 || g.filled {
        return false;
    }
    match w {
        Wanted::Circle(c, r) => g.circle.is_some_and(|(gc, gr)| dist(gc, *c) < 1e-4 && (gr - r4(*r)).abs() < 1e-4),
        Wanted::Rect(r) => {
            g.kind == "fp_rect"
                && g.bbox().is_some_and(|b| {
                    (b.x0 - r4(r.x0)).abs() < 1e-4
                        && (b.y0 - r4(r.y0)).abs() < 1e-4
                        && (b.x1 - r4(r.x1)).abs() < 1e-4
                        && (b.y1 - r4(r.y1)).abs() < 1e-4
                })
        }
    }
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let reference = i.fp.texts.iter().find(|t| t.kind == "Reference").map_or("", |t| t.text.as_str());
    let class = class_of(&format!("{}:{}", i.lib, i.fp.name), reference);
    let Some(want) = wanted(i, class) else { return out };
    let layer = class.layer(i.fp.back);
    let have: Vec<_> = i.fp.on(layer).collect();
    if have.len() == 1 && matches(have[0], &want) {
        return out;
    }
    let what = match &want {
        Wanted::Circle(_, r) => format!("a circle r {} mm", fmt(*r)),
        Wanted::Rect(r) => format!("a {} x {} mm rectangle", fmt(r.w()), fmt(r.h())),
    };
    let had = if have.is_empty() { String::new() } else { format!(" (replacing {} items)", have.len()) };
    out.fixed(format!("{layer}: {what} round the pads{had}"));
    for g in have {
        out.edits.push(delete(i.text, &g.span));
    }
    out.edits.push(insert(i.fp, render(&want, layer)));
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};

    #[test]
    fn a_round_test_point_gets_a_circle() {
        let t = fp(
            "TestPoint_Pad_D1.0mm",
            r#"(property "Reference" "REF**" (at 0 0 0) (layer "User.9"))
(pad "1" smd circle (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask"))"#,
        );
        let (new, f) = run("VHRD_TestPoint", &t, Rule::RefOutline);
        let new = new.unwrap();
        assert!(new.contains("(fp_circle\n\t\t(center 0 0)\n\t\t(end 0.55 0)"), "{new}");
        assert!(new.contains("(layer \"User.9\")"), "{new}");
        assert_eq!(f[0].message, "User.9: a circle r 0.55 mm round the pads");
        settled("VHRD_TestPoint", &t, Rule::RefOutline);
    }

    #[test]
    fn square_test_points_and_net_ties_get_a_rectangle() {
        let t = fp("TestPoint_Pad_0.8x0.8mm", r#"(pad "1" smd rect (at 0 0) (size 0.8 0.8) (layers "F.Cu" "F.Mask"))"#);
        let new = run("VHRD_TestPoint", &t, Rule::RefOutline).0.unwrap();
        assert!(new.contains("(start -0.45 -0.45)\n\t\t(end 0.45 0.45)") && new.contains("\"User.9\""), "{new}");
        let t = fp(
            "NetTie-2",
            r#"(fp_line (start 0 0) (end 1 0) (stroke (width 0.1) (type solid)) (layer "User.11"))
(pad "1" smd rect (at -0.5 0) (size 0.4 0.3) (layers "F.Cu" "F.Mask"))
(pad "2" smd rect (at 0.5 0) (size 0.4 0.3) (layers "F.Cu" "F.Mask"))"#,
        );
        let (new, f) = run("VHRD_NetTie", &t, Rule::RefOutline);
        let new = new.unwrap();
        assert!(new.contains("(start -0.75 -0.2)\n\t\t(end 0.75 0.2)") && new.contains("\"User.11\""), "{new}");
        assert_eq!(new.matches("User.11").count(), 1, "the old line goes:\n{new}");
        assert!(f[0].message.ends_with("(replacing 1 items)"), "{f:?}");
        settled("VHRD_NetTie", &t, Rule::RefOutline);
    }

    #[test]
    fn parts_and_mechanical_get_nothing() {
        let pad = r#"(pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask"))"#;
        assert!(run("VHRD_Resistor_SMD", &fp("R_0402", pad), Rule::RefOutline).1.is_empty());
        assert!(run("VHRD_MountingHole", &fp("MountingHole_2.2mm", pad), Rule::RefOutline).1.is_empty());
    }
}
