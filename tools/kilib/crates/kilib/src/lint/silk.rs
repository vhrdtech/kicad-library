//! The silkscreen holds the outline and the pin-1 mark, nothing else, drawn one way:
//!
//! - every line is [`WIDTH`] wide (KiCad's library convention, and the width most of this library already used);
//!   filled marks keep their stroke, it is part of their size;
//! - no line comes onto a pad's solder mask opening: the line's edge keeps the mask expansion (the pad's, else
//!   the footprint's, else the board's, 0) from the copper, and from a hole without copper;
//! - no text but pin-1 and polarity marks (`1`, `A1`, `+`, `-`, `A`, `K`); the designator lives on the fab layer
//!   ([`crate::fab`] moves it there).
//!
//! The fix sets the widths and cuts lines (and the sides of rectangles and polygons, split into lines) where they
//! cross onto a pad, dropping pieces shorter than [`MIN_PIECE`]. Arcs, circles and filled shapes over a pad, and
//! texts, are reported. Artwork (logos and markings) is left alone.

use super::{Class, Input, Outcome, delete, insert};
use crate::footprint::{Graphic, Outline, Pt, fmt, render_graphic};
use crate::migrate::Edit;

pub const WIDTH: f64 = 0.12;
/// A piece of a cut line shorter than this goes.
pub const MIN_PIECE: f64 = 0.1;
/// Kept on top of the clearance, so the cut ends rounded to 0.001 mm still keep it.
const MARGIN: f64 = 0.002;
/// Texts that mark pin 1 or the polarity.
pub const MARKS: [&str; 7] = ["1", "A1", "+", "-", "A", "K", "●"];

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// The part of the segment `a`-`b` (as `t` from 0 to 1) that comes within `r` of the convex `o`.
fn too_close(o: &Outline, a: Pt, b: Pt, r: f64) -> Option<(f64, f64)> {
    let f = |t: f64| o.distance(lerp(a, b, t)) - r;
    // The distance to a convex shape along a line is convex: one minimum, and the part below r is one stretch.
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..80 {
        let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if f(m1) <= f(m2) { hi = m2 } else { lo = m1 }
    }
    let tm = (lo + hi) / 2.0;
    let tm = [0.0, tm, 1.0].into_iter().min_by(|x, y| f(*x).total_cmp(&f(*y))).expect("three");
    if f(tm) >= 0.0 {
        return None;
    }
    let edge = |mut inside: f64, mut outside: f64| {
        if f(outside) < 0.0 {
            return outside;
        }
        for _ in 0..60 {
            let m = (inside + outside) / 2.0;
            if f(m) < 0.0 { inside = m } else { outside = m }
        }
        outside
    };
    Some((edge(tm, 0.0), edge(tm, 1.0)))
}

/// The pieces of `a`-`b` that stay `r(pad)` clear of every pad.
fn cut(a: Pt, b: Pt, pads: &[(Outline, f64)]) -> Option<Vec<(Pt, Pt)>> {
    let mut gone: Vec<(f64, f64)> = pads.iter().filter_map(|(o, r)| too_close(o, a, b, *r)).collect();
    if gone.is_empty() {
        return None;
    }
    gone.sort_by(|x, y| x.0.total_cmp(&y.0));
    let len = crate::footprint::dist(a, b);
    let mut pieces = Vec::new();
    let mut from = 0.0;
    for (s, e) in gone {
        if s > from {
            pieces.push((from, s));
        }
        from = from.max(e);
    }
    if from < 1.0 {
        pieces.push((from, 1.0));
    }
    let round = |p: Pt| (((p.0 * 1000.0).round()) / 1000.0, ((p.1 * 1000.0).round()) / 1000.0);
    Some(
        pieces
            .into_iter()
            .filter(|(s, e)| (e - s) * len >= MIN_PIECE)
            .map(|(s, e)| (round(lerp(a, b, s)), round(lerp(a, b, e))))
            .collect(),
    )
}

/// Sets a line's width to [`WIDTH`]; a filled mark keeps its stroke.
fn set_width(g: &Graphic, edits: &mut Vec<Edit>, count: &mut usize) {
    if !g.filled
        && (g.width - WIDTH).abs() > 1e-9
        && let Some(span) = &g.width_span
    {
        edits.push(Edit { range: span.clone(), text: fmt(WIDTH) });
        *count += 1;
    }
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let fp = i.fp;
    if i.class == Class::Graphic {
        return out;
    }
    for side in ["F", "B"] {
        let layer = format!("{side}.SilkS");
        let margin = |m: Option<f64>| m.or(fp.mask_margin).unwrap_or(0.0).max(0.0);
        let pads: Vec<(&crate::footprint::Pad, Outline, f64)> = fp
            .pads
            .iter()
            .filter(|p| p.on_side(side) && p.layers.iter().any(|l| l.ends_with(".Mask")))
            .map(|p| (p, p.outline.clone(), margin(p.mask_margin)))
            .collect();
        let mut widths = 0;
        let mut uncut: Vec<(String, String, usize)> = Vec::new();
        for g in fp.on(&layer) {
            let filled_mark = g.filled;
            let width = if filled_mark { g.width } else { WIDTH };
            let need: Vec<(Outline, f64)> =
                pads.iter().map(|(_, o, m)| (o.clone(), m + width / 2.0 + MARGIN)).collect();
            let over = |g: &Graphic| {
                pads.iter()
                    .zip(&need)
                    .find(|(_, (o, r))| g.segments().iter().any(|(a, b)| o.segment_distance(*a, *b) < r - MARGIN))
            };
            let Some(((pad, _, _), _)) = over(g) else {
                set_width(g, &mut out.edits, &mut widths);
                continue;
            };
            let what = if pad.number.is_empty() { "a hole".to_string() } else { format!("pad {}", pad.number) };
            if filled_mark || g.circle.is_some() || matches!(g.kind.as_str(), "fp_arc" | "fp_curve") {
                set_width(g, &mut out.edits, &mut widths);
                let kind = if filled_mark { format!("filled {}", g.kind) } else { g.kind.clone() };
                match uncut.iter_mut().find(|(k, w, _)| *k == kind && *w == what) {
                    Some((_, _, n)) => *n += 1,
                    None => uncut.push((kind, what, 1)),
                }
                continue;
            }
            let mut lines = String::new();
            let mut cuts = 0;
            for (a, b) in g.segments() {
                match cut(a, b, &need) {
                    None => lines.push_str(&render_graphic("fp_line", &[a, b], WIDTH, false, &layer)),
                    Some(pieces) => {
                        cuts += 1;
                        for (p, q) in pieces {
                            lines.push_str(&render_graphic("fp_line", &[p, q], WIDTH, false, &layer));
                        }
                    }
                }
            }
            out.fixed(format!(
                "silk {} cut clear of {what}{}",
                g.kind,
                if cuts > 1 { format!(" ({cuts} sides)") } else { String::new() }
            ));
            let mut e = delete(i.text, &g.span);
            let indent_ok = e.range.start < g.span.start;
            if indent_ok {
                e.text = lines;
                out.edits.push(e);
            } else {
                out.edits.push(e);
                out.edits.push(insert(fp, lines));
            }
        }
        for (kind, what, n) in uncut {
            let n = if n > 1 { format!("{n} ") } else { String::new() };
            out.left(format!("silk {n}{kind} over {what} on {layer}, not cut"));
        }
        if widths > 0 {
            out.fixed(format!("{widths} silk lines set to {} mm", fmt(WIDTH)));
        }
        for t in fp.texts.iter().filter(|t| t.layer == layer && !t.hidden) {
            if t.kind == "Reference" || t.text.contains("${REFERENCE}") {
                out.left(format!("designator on {layer} (kilib fab-designators moves it)"));
            } else if t.kind == "Value" {
                out.left(format!("Value shown on {layer}"));
            } else if !MARKS.contains(&t.text.trim()) {
                let label = t.text.trim().trim_end_matches('.');
                if fp.pads.iter().any(|p| !p.number.is_empty() && p.number == label) {
                    out.left(format!("pin number {:?} on {layer} (only pin 1 is marked)", t.text));
                } else {
                    out.left(format!("text {:?} on {layer}", t.text));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};
    use super::*;

    const PADS: &str = r#"(pad "1" smd rect (at -1 0) (size 0.6 0.6) (layers "F.Cu" "F.Mask"))
(pad "2" smd rect (at 1 0) (size 0.6 0.6) (layers "F.Cu" "F.Mask") (solder_mask_margin 0.05))"#;

    #[test]
    fn cuts_a_line_round_pads() {
        let o = Outline::Poly(vec![(-0.3, -0.3), (0.3, -0.3), (0.3, 0.3), (-0.3, 0.3)]);
        let (s, e) = too_close(&o, (-2.0, 0.0), (2.0, 0.0), 0.1).unwrap();
        assert!((s * 4.0 - 2.0 + 0.4).abs() < 1e-6 && (e * 4.0 - 2.0 - 0.4).abs() < 1e-6, "{s} {e}");
        assert!(too_close(&o, (-2.0, 1.0), (2.0, 1.0), 0.1).is_none());
    }

    #[test]
    fn widths_and_lines_over_pads() {
        let silk = r#"(fp_line (start -2 0) (end 2 0) (stroke (width 0.2) (type solid)) (layer "F.SilkS"))
(fp_line (start -2 1) (end 2 1) (stroke (width 0.15) (type solid)) (layer "F.SilkS"))
(fp_poly (pts (xy -1.6 -0.6) (xy -1.4 -0.8) (xy -1.8 -0.8)) (stroke (width 0.1) (type solid)) (fill yes) (layer "F.SilkS"))"#;
        let t = fp("X", &format!("{silk}\n{PADS}"));
        let (new, f) = run("VHRD_X", &t, Rule::Silk);
        let new = new.unwrap();
        let msgs: Vec<&str> = f.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(
            msgs,
            ["silk fp_line cut clear of pad 1", "1 silk lines set to 0.12 mm"],
            "the cut line is drawn anew at 0.12:\n{new}"
        );
        // Pad 1 reaches -0.7 to -1.3, plus 0.06: cut at -1.362 and -0.638; pad 2 adds its 0.05 mask margin.
        assert!(new.contains("(start -2 0)\n\t\t(end -1.362 0)"), "{new}");
        assert!(new.contains("(start -0.638 0)\n\t\t(end 0.588 0)"), "{new}");
        assert!(new.contains("(start 1.412 0)\n\t\t(end 2 0)"), "{new}");
        assert!(
            new.contains("(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill yes)")
                || new.contains("(width 0.1) (type solid)) (fill yes)"),
            "the filled mark keeps its stroke:\n{new}"
        );
        settled("VHRD_X", &t, Rule::Silk);
    }

    #[test]
    fn rectangles_split_and_texts_reported() {
        let silk = r#"(fp_rect (start -1.2 -0.5) (end 1.2 0.5) (stroke (width 0.12) (type solid)) (fill no) (layer "F.SilkS"))
(fp_text user "1" (at -1.6 0 0) (layer "F.SilkS") (effects (font (size 0.8 0.8))))
(fp_text user "PWR" (at 0 1 0) (layer "F.SilkS") (effects (font (size 0.8 0.8))))
(fp_text user "2" (at 1.6 0 0) (layer "F.SilkS") (effects (font (size 0.8 0.8))))
(fp_text user "${REFERENCE}" (at 0 -1 0) (layer "F.SilkS") (effects (font (size 0.8 0.8))))"#;
        let t = fp("X", &format!("{silk}\n{PADS}"));
        let (new, f) = run("VHRD_X", &t, Rule::Silk);
        let new = new.unwrap();
        assert!(!new.contains("fp_rect"), "{new}");
        // The long sides stay; the left side keeps its ends, the right one (pad 2's wider margin) is all gone.
        assert_eq!(new.matches("(fp_line").count(), 4, "{new}");
        let msgs: Vec<&str> = f.iter().map(|f| f.message.as_str()).collect();
        assert!(msgs.contains(&"text \"PWR\" on F.SilkS"), "{msgs:?}");
        assert!(msgs.contains(&"pin number \"2\" on F.SilkS (only pin 1 is marked)"), "{msgs:?}");
        assert!(msgs.contains(&"designator on F.SilkS (kilib fab-designators moves it)"), "{msgs:?}");
        assert!(!msgs.iter().any(|m| m.contains("\"1\"")), "{msgs:?}");
        settled("VHRD_X", &t, Rule::Silk);
    }

    #[test]
    fn artwork_is_left_alone() {
        let silk = r#"(fp_line (start -2 0) (end 2 0) (stroke (width 0.3) (type solid)) (layer "F.SilkS"))"#;
        let (new, f) = run("VHRD_Symbol", &fp("LOGO", &format!("{silk}\n{PADS}")), Rule::Silk);
        assert!(new.is_none() && f.is_empty());
    }
}
