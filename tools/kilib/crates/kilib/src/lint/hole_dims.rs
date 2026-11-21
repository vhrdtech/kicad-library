//! A mounting hole's size on Dimensions (Dwgs.User): a circle as large as the drill, [`WIDTH`] wide, and a radial
//! dimension on it that reads the diameter (`⌀3.2`), its point on the circle up and right of the centre, so a print
//! of the board outline, the bodies and Dimensions shows each hole with its size and no copper layer has to be on.
//! Only mounting holes get it; the largest round drill of the footprint is the hole. The layer belongs to this
//! rule: whatever else is there is replaced.

use super::{Class, Input, Outcome, delete, insert};
use crate::footprint::{Pt, fmt, render_graphic};
use crate::sexpr::{self, Node};

pub const LAYER: &str = "Dwgs.User";
pub const WIDTH: f64 = 0.05;
/// How far the dimension's leader runs out from the circle.
pub const LEADER: f64 = 1.0;
pub const TEXT_MIN: f64 = 0.4;
pub const TEXT_MAX: f64 = 0.8;

/// The hole: the centre and diameter of the footprint's largest round drill.
fn hole(root: &Node) -> Option<(Pt, f64)> {
    let mut best: Option<(Pt, f64)> = None;
    for pad in root.children("pad") {
        let Some(drill) = pad.child("drill") else { continue };
        if drill.arg(0) == Some("oval") {
            continue;
        }
        let Some(d) = drill.arg(0).and_then(|v| v.parse::<f64>().ok()) else { continue };
        let Some(at) = pad.child("at") else { continue };
        let (Some(x), Some(y)) = (at.arg(0).and_then(|v| v.parse().ok()), at.arg(1).and_then(|v| v.parse().ok()))
        else {
            continue;
        };
        if best.is_none_or(|(_, b)| d > b) {
            best = Some(((x, y), d));
        }
    }
    best
}

/// The dimension's point on the circle: up and right of the centre.
fn on_circle(c: Pt, d: f64) -> Pt {
    let k = d / 2.0 * std::f64::consts::FRAC_1_SQRT_2;
    (c.0 + k, c.1 - k)
}

fn label(d: f64) -> String {
    format!("⌀{}", fmt(d))
}

fn text_size(d: f64) -> f64 {
    ((d / 4.0).clamp(TEXT_MIN, TEXT_MAX) * 20.0).round() / 20.0
}

/// The radial dimension as KiCad 10 writes it, one tab in; KiCad places the text itself on load.
fn render_dimension(c: Pt, d: f64) -> String {
    let e = on_circle(c, d);
    let k = LEADER * std::f64::consts::FRAC_1_SQRT_2;
    let s = text_size(d);
    let at = (e.0 + k + s * label(d).chars().count() as f64 * 0.4, e.1 - k);
    let layer = sexpr::quote(LAYER);
    let text = sexpr::quote(&label(d));
    format!(
        "\t(dimension\n\t\t(type radial)\n\t\t(layer {layer})\n\t\t(pts\n\t\t\t(xy {} {}) (xy {} {})\n\t\t)\n\
         \t\t(leader_length {})\n\t\t(format\n\t\t\t(prefix \"\")\n\t\t\t(suffix \"\")\n\t\t\t(units 3)\n\
         \t\t\t(units_format 0)\n\t\t\t(precision 4)\n\t\t\t(override_value {text})\n\t\t)\n\t\t(style\n\
         \t\t\t(thickness {})\n\t\t\t(arrow_length {})\n\t\t\t(text_position_mode 0)\n\t\t\t(extension_offset 0)\n\
         \t\t\t(keep_text_aligned yes)\n\t\t)\n\t\t(gr_text {text}\n\t\t\t(at {} {} 0)\n\t\t\t(layer {layer})\n\
         \t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size {} {})\n\t\t\t\t\t(thickness {})\n\t\t\t\t)\n\t\t\t)\n\t\t)\n\t)\n",
        fmt(c.0),
        fmt(c.1),
        fmt(e.0),
        fmt(e.1),
        fmt(LEADER),
        fmt(WIDTH),
        fmt(s),
        fmt(at.0),
        fmt(at.1),
        fmt(s),
        fmt(s),
        fmt(s / 8.0),
    )
}

/// The dimension node already reads `d` at `c`: radial, the same two points and the same text.
fn is_dimension(n: &Node, c: Pt, d: f64) -> bool {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
    let pts: Vec<Pt> = n.child("pts").map_or_else(Vec::new, |p| {
        p.children("xy").filter_map(|xy| Some((xy.arg(0)?.parse().ok()?, xy.arg(1)?.parse().ok()?))).collect()
    });
    let e = on_circle(c, d);
    n.value("type") == Some("radial")
        && pts.len() == 2
        && near(pts[0].0, c.0)
        && near(pts[0].1, c.1)
        && near(pts[1].0, e.0)
        && near(pts[1].1, e.1)
        && n.child("format").and_then(|f| f.value("override_value")) == Some(label(d).as_str())
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let Ok(root) = sexpr::parse(i.text) else { return out };
    let want = if i.class == Class::MountingHole { hole(&root) } else { None };
    let circles: Vec<_> = i.fp.on(LAYER).collect();
    let dims: Vec<&Node> = root.children("dimension").filter(|n| n.value("layer") == Some(LAYER)).collect();
    let fine = match want {
        Some((c, d)) => {
            let circle_ok = circles.len() == 1
                && circles[0].kind == "fp_circle"
                && (circles[0].width - WIDTH).abs() < 1e-9
                && circles[0].circle.is_some_and(|(cc, r)| {
                    (cc.0 - c.0).abs() < 1e-4 && (cc.1 - c.1).abs() < 1e-4 && (r - d / 2.0).abs() < 1e-4
                });
            circle_ok && dims.len() == 1 && is_dimension(dims[0], c, d)
        }
        None => circles.is_empty() && dims.is_empty(),
    };
    if fine {
        return out;
    }
    let had = circles.len() + dims.len();
    let had = if had == 0 { String::new() } else { format!(" (replacing {had})") };
    match want {
        Some((_, d)) => out.fixed(format!("{LAYER}: the hole's circle and {}{had}", label(d))),
        None => out.fixed(format!("{LAYER}: nothing, not a mounting hole{had}")),
    }
    for g in &circles {
        out.edits.push(delete(i.text, &g.span));
    }
    for n in &dims {
        out.edits.push(delete(i.text, &n.span()));
    }
    if let Some((c, d)) = want {
        let circle = render_graphic("fp_circle", &[c, (c.0 + d / 2.0, c.1)], WIDTH, false, LAYER);
        out.edits.push(insert(i.fp, circle + &render_dimension(c, d)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};

    const HOLE: &str = r#"(pad "" np_thru_hole circle (at 0 0) (size 3.2 3.2) (drill 3.2) (layers "*.Cu" "*.Mask"))"#;

    #[test]
    fn a_mounting_hole_gets_its_circle_and_diameter() {
        let t = fp("MountingHole_3.2mm_M3", HOLE);
        let (new, f) = run("VHRD_MountingHole", &t, Rule::HoleDims);
        let new = new.unwrap();
        assert!(new.contains("(center 0 0)\n\t\t(end 1.6 0)\n\t\t(stroke\n\t\t\t(width 0.05)"), "{new}");
        assert!(new.contains("(type radial)") && new.contains("(xy 0 0) (xy 1.1314 -1.1314)"), "{new}");
        assert!(new.contains("(override_value \"⌀3.2\")") && new.contains("(size 0.8 0.8)"), "{new}");
        assert_eq!(f[0].message, "Dwgs.User: the hole's circle and ⌀3.2");
        settled("VHRD_MountingHole", &t, Rule::HoleDims);
        // A dimension with the wrong text is replaced, with the circle.
        let off = new.replace("⌀3.2", "⌀3");
        let (again, f) = run("VHRD_MountingHole", &off, Rule::HoleDims);
        assert_eq!(again.unwrap(), new);
        assert!(f[0].message.ends_with("(replacing 2)"), "{f:?}");
    }

    #[test]
    fn the_largest_round_drill_is_the_hole() {
        let pads = r#"(pad "1" thru_hole circle (at 0 0) (size 5.6 5.6) (drill 3.2) (layers "*.Cu" "*.Mask"))
            (pad "1" thru_hole circle (at 2.4 0) (size 0.7 0.7) (drill 0.4) (layers "*.Cu" "*.Mask"))"#;
        let new = run("VHRD_MountingHole", &fp("MountingHole_3.2mm_M3_Pad_Via", pads), Rule::HoleDims).0.unwrap();
        assert!(new.contains("(end 1.6 0)") && new.contains("⌀3.2") && !new.contains("⌀0.4"), "{new}");
    }

    #[test]
    fn small_holes_get_smaller_text_and_other_footprints_nothing() {
        let small = HOLE.replace("3.2", "1.8");
        let new = run("VHRD_MountingHole", &fp("MountingHole_1.8mm_M1.6", &small), Rule::HoleDims).0.unwrap();
        assert!(new.contains("⌀1.8") && new.contains("(size 0.45 0.45)"), "{new}");
        let pad = r#"(pad "1" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))"#;
        assert!(run("VHRD_Connector", &fp("Conn_01x01", pad), Rule::HoleDims).0.is_none());
    }
}
