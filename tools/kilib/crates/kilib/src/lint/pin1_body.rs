//! A pin-1 mark on Pin1.F (User.29; Pin1.B, User.30, on the back), shown together with the body (Body.F/B), which
//! has no pin-1 mark of its own. The parts are those of [`super::pin1`]: three or more pins, and polar two-pin parts
//! (the dot is by the cathode or plus pad). Test points, net ties, mechanical and graphic footprints, and non-polar
//! two-pin parts get none.
//!
//! A polar two-pin part (a diode, an LED, a polarised capacitor, a battery) gets a bar across the cathode or plus
//! pad's centre, square to the part's axis, as long as the pad is across it, [`BAR_WIDTH_MIN`] to [`BAR_WIDTH_MAX`]
//! thick (by part size).
//!
//! A part of three or more pins gets a filled dot, [`DIAMETER_MIN`] to [`DIAMETER_MAX`] across (a tenth of the
//! body's shorter side, and under the distance from pin 1 to its nearest neighbour), centred on pin 1 — unless the
//! part wants none ([`super::pin1::need`]): the pads alone already tell pin 1 apart (an asymmetric SOT-23, SOT-323,
//! SOT-89, SOT-223...), the part works the same turned half round (a 4-pad crystal, a tact switch), or it is a
//! keyed connector (a USB-C receptacle, an RJ45 jack). A symmetric part (a QFN, a QFP, a SOIC, a SOT-23-6, a SOT-363,
//! a connector with a symmetric pad row...) and a keyed connector with a row that pin 1 helps to count (an FFC
//! connector, a pin header) keep the dot.
//!
//! Either mark sits exactly on pin 1's pad: its anchor (`at`), the same point for every shape of a multi-shape pad.
//! No search for a free spot, no check that it clears another pad. The layer belongs to this rule: whatever else is
//! there is replaced.

use super::pin1::{Need, axis, need, pin_numbers, pin1};
use super::{Input, Outcome, delete, insert};
use crate::footprint::{Footprint, Graphic, Pad, Pt, Rect, dist, fmt, render_graphic};
use crate::layers::{BODY, PIN1, side_of};

pub const DIAMETER_MIN: f64 = 0.3;
pub const DIAMETER_MAX: f64 = 0.8;
pub const BAR_WIDTH_MIN: f64 = 0.15;
pub const BAR_WIDTH_MAX: f64 = 0.25;
const GRID: f64 = 0.05;

/// The mark a part wants on its pin-1 layer.
enum Spot {
    Dot { c: Pt, d: f64 },
    Bar { a: Pt, b: Pt, w: f64 },
}

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

/// The bar's thickness for a part of `body`, by size.
fn bar_width(body: Rect) -> f64 {
    floor_to((0.1 * body.w().min(body.h())).clamp(BAR_WIDTH_MIN, BAR_WIDTH_MAX), GRID)
}

/// Pin 1's distance to the nearest pad of another number.
fn neighbour(fp: &Footprint, p1: &Pad) -> Option<f64> {
    fp.copper_pads().filter(|p| p.number != p1.number).map(|p| dist(p.at, p1.at)).reduce(f64::min)
}

/// The bar across `p1`, square to `ax` (the part's axis), as long as the pad reaches across it.
fn bar(p1: &Pad, ax: Pt, width: f64) -> (Pt, Pt) {
    let perp = (-ax.1, ax.0);
    let proj = |p: Pt| (p.0 - p1.at.0) * perp.0 + (p.1 - p1.at.1) * perp.1;
    let (lo, hi) =
        p1.outline.points().into_iter().map(proj).fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let half = floor_to((hi - lo).max(width), GRID) / 2.0;
    ((p1.at.0 - perp.0 * half, p1.at.1 - perp.1 * half), (p1.at.0 + perp.0 * half, p1.at.1 + perp.1 * half))
}

fn is_dot(g: &Graphic, c: Pt, r: f64) -> bool {
    g.kind == "fp_circle" && g.filled && g.circle.is_some_and(|(gc, gr)| dist(gc, c) < 1e-3 && (gr - r).abs() < 1e-3)
}

fn is_bar(g: &Graphic, a: Pt, b: Pt, w: f64) -> bool {
    g.kind == "fp_line"
        && (g.width - w).abs() < 1e-3
        && g.pts.len() == 2
        && ((dist(g.pts[0], a) < 1e-3 && dist(g.pts[1], b) < 1e-3)
            || (dist(g.pts[0], b) < 1e-3 && dist(g.pts[1], a) < 1e-3))
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let fp = i.fp;
    let layer = side_of(PIN1, fp.back);
    let have: Vec<_> = fp.on(layer).collect();
    let texts: Vec<_> = fp.texts.iter().filter(|t| t.layer == layer).collect();
    let stray = have.len() + texts.len();
    let (spot, none_reason) = match need(i) {
        Need::NoMark(why) | Need::Drawn(why) => (None, why),
        Need::Nothing => (None, "no pin 1 to mark"),
        Need::Mark => {
            let p1 = pin1(fp).expect("need checks it");
            let Some(b) = body(fp) else {
                out.left(format!("{layer}: no body or pads to size the mark by"));
                return out;
            };
            if pin_numbers(fp).len() == 2 {
                let Some(ax) = axis(fp, p1) else {
                    out.left(format!("{layer}: pin 1 and its pad sit on top of each other; no axis for the bar"));
                    return out;
                };
                let w = bar_width(b);
                let (a, c) = bar(p1, ax, w);
                (Some(Spot::Bar { a, b: c, w }), "")
            } else {
                let d = diameter(b, neighbour(fp, p1));
                (Some(Spot::Dot { c: p1.at, d }), "")
            }
        }
    };
    let fine = match &spot {
        Some(Spot::Dot { c, d }) => have.len() == 1 && texts.is_empty() && is_dot(have[0], *c, *d / 2.0),
        Some(Spot::Bar { a, b, w }) => have.len() == 1 && texts.is_empty() && is_bar(have[0], *a, *b, *w),
        None => stray == 0,
    };
    if fine {
        return out;
    }
    let had = if stray == 0 { String::new() } else { format!(" (replacing {stray})") };
    match &spot {
        Some(Spot::Dot { c, d }) => {
            out.fixed(format!("{layer}: pin-1 dot ⌀{} at ({}, {}){had}", fmt(*d), fmt(c.0), fmt(c.1)));
        }
        Some(Spot::Bar { a, b, w }) => out.fixed(format!(
            "{layer}: pin-1 bar {}×{} at ({}, {}){had}",
            fmt(dist(*a, *b)),
            fmt(*w),
            fmt((a.0 + b.0) / 2.0),
            fmt((a.1 + b.1) / 2.0)
        )),
        None => out.fixed(format!("{layer}: no mark, {none_reason}{had}")),
    }
    for g in &have {
        out.edits.push(delete(i.text, &g.span));
    }
    for t in &texts {
        out.edits.push(delete(i.text, &t.span));
    }
    match &spot {
        Some(Spot::Dot { c, d }) => {
            out.edits.push(insert(fp, render_graphic("fp_circle", &[*c, (c.0 + d / 2.0, c.1)], 0.0, true, layer)));
        }
        Some(Spot::Bar { a, b, w }) => {
            out.edits.push(insert(fp, render_graphic("fp_line", &[*a, *b], *w, false, layer)))
        }
        None => {}
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
    fn an_asymmetric_three_pin_part_gets_no_mark() {
        // No turn of the pads lands pin 1 on pad 2 or pad 3: the pads already tell pin 1 apart.
        let t = sot23();
        let (new, f) = run("VHRD_Package_TO_SOT_SMD", &t, Rule::Pin1Body);
        assert!(new.is_none() && f.is_empty(), "{f:?}");
        settled("VHRD_Package_TO_SOT_SMD", &t, Rule::Pin1Body);
    }

    #[test]
    fn a_polar_two_pin_part_gets_a_bar_and_a_plain_one_nothing() {
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
        let (new, f) = run("VHRD_Diode_SMD", &diode("D_SOD-123"), Rule::Pin1Body);
        let new = new.unwrap();
        // Centred on pad 1, square to the x axis: start and end differ only in y.
        assert!(f[0].fixed && f[0].message.starts_with("User.29: pin-1 bar 1.2×0.15 at (-1.65, 0)"), "{f:?}");
        assert!(new.contains("(start -1.65 -0.6)\n\t\t(end -1.65 0.6)") && new.contains("(width 0.15)"), "{new}");
        settled("VHRD_Diode_SMD", &diode("D_SOD-123"), Rule::Pin1Body);
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
        // A stray mark on a part that wants none (the pads already tell pin 1 apart) is removed too.
        let wrong = sot23().replace(
            "(pad \"1\"",
            "(fp_circle (center 0 0) (end 0.2 0) (stroke (width 0) (type solid)) (fill yes) (layer \"User.29\"))\n(pad \"1\"",
        );
        let (new, _) = run("VHRD_Package_TO_SOT_SMD", &wrong, Rule::Pin1Body);
        assert!(!new.unwrap().contains("User.29"));
    }

    #[test]
    fn a_symmetric_part_gets_a_dot_on_pad_ones_own_centre() {
        // Four pads of a square part: a half turn lands pin 1 on pad 3, so the layout alone is ambiguous.
        let pads = [
            pad("1", -1.5, -0.5, 1.0, 0.5),
            pad("2", 1.5, -0.5, 1.0, 0.5),
            pad("3", 1.5, 0.5, 1.0, 0.5),
            pad("4", -1.5, 0.5, 1.0, 0.5),
        ];
        let t = fp("QFN-4", &format!("{}\n{}", rect(4.0, 2.0, "User.5"), pads.join("\n")));
        let (new, f) = run("VHRD_Package_DFN_QFN", &t, Rule::Pin1Body);
        let new = new.unwrap();
        assert!(f[0].fixed && f[0].message.starts_with("User.29: pin-1 dot ⌀0.3 at (-1.5, -0.5)"), "{f:?}");
        assert!(new.contains("(center -1.5 -0.5)"), "{new}");
        settled("VHRD_Package_DFN_QFN", &t, Rule::Pin1Body);
    }

    #[test]
    fn a_connector_with_a_symmetric_row_gets_a_dot_too() {
        // A 4-pin header: a half turn swaps pin 1 with pin 4, pin 2 with pin 3 — ambiguous, so pin 1 gets a dot.
        let pads: Vec<String> =
            (1..=4).map(|k| pad(&k.to_string(), -1.905 + 1.27 * f64::from(k - 1), 0.0, 1.0, 1.0)).collect();
        let t = fp("Header_1x04", &format!("{}\n{}", rect(5.08, 2.5, "User.5"), pads.join("\n")));
        let (new, _) = run("VHRD_Connector_PinHeader", &t, Rule::Pin1Body);
        let new = new.unwrap();
        assert!(new.contains("(center -1.905 0)"), "{new}");
        settled("VHRD_Connector_PinHeader", &t, Rule::Pin1Body);
    }

    #[test]
    fn a_back_footprint_uses_the_back_layer() {
        let t = sot23().replace("User.5", "User.6").replace("F.Cu", "B.Cu").replace("F.Mask", "B.Mask");
        assert!(run("VHRD_Package_TO_SOT_SMD", &t, Rule::Pin1Body).0.is_none());
        let diode = fp(
            "D_SOD-123",
            &format!(
                "{}\n{}\n{}",
                rect(2.7, 1.6, "User.5"),
                pad("1", -1.65, 0.0, 0.9, 1.2),
                pad("2", 1.65, 0.0, 0.9, 1.2)
            ),
        )
        .replace("User.5", "User.6")
        .replace("F.Cu", "B.Cu")
        .replace("F.Mask", "B.Mask");
        let (new, _) = run("VHRD_Diode_SMD", &diode, Rule::Pin1Body);
        assert!(new.unwrap().contains("(layer \"User.30\")"));
    }
}
