//! Pin numbers on Pins.F (User.19; Pins.B, User.20, on the back), so a fab or assembly print can show which pad is
//! which on demand:
//!
//! - a connector: every pin;
//! - another part with three or more pins: pin 1 and every 5th (5, 10, 15...), every 10th past [`DENSE`] pins;
//!   pins not numbered with digits (a BGA's A1) only pin 1;
//! - two-pin parts, test points, net ties, logos, holes: none;
//! - a pad on the copper of a pad already numbered (USB-C's B12 on A1): none of its own.
//!
//! Each number is a stroke text beside its pad, [`GAP`] clear of every pad and hole and of the other numbers, level: on a
//! side with no other pad in line within [`OPEN`] (never squeezed between two pads), at the pad's outer end where
//! the pad is long, else facing away from the middle of the pads; where none of those fits, on any side that does. The footprint gets one size
//! for all its numbers: the largest of [`SIZES`] at which every one fits; a number that fits at none is left out
//! and reported. The layer belongs to this rule: whatever else is there is replaced.

use super::{Class, Input, Outcome, delete, insert};
use crate::fab::thickness;
use crate::footprint::{Pt, Rect, fmt};
use crate::layers::{PINS, side_of};
use crate::sexpr;
use crate::stroke::{Ink, ink};

/// The sizes tried, largest first.
pub const SIZES: [f64; 6] = [0.5, 0.4, 0.35, 0.3, 0.25, 0.2];
/// Room kept between a number's ink and any pad or other number.
pub const GAP: f64 = 0.1;
/// A side of a pad with another pad in line closer than this is not open.
pub const OPEN: f64 = 2.0;
/// Past this many pins a part gets every 10th number instead of every 5th.
pub const DENSE: usize = 40;

/// One number as it should be written: the text, its anchor and size.
#[derive(Debug, Clone, PartialEq)]
struct Wanted {
    text: String,
    at: Pt,
    size: f64,
}

/// A connector: from its library or its name.
fn connector(lib: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    lib.contains("Connector")
        || ["conn", "pinheader", "pinsocket", "header", "socket", "usb", "rj45", "terminal"]
            .iter()
            .any(|k| n.contains(k))
}

/// The pad numbers that get a label, each with the first pad of that number, in the order of the file.
fn labelled<'a>(i: &'a Input) -> Vec<(&'a str, Rect)> {
    let mut seen: Vec<&str> = Vec::new();
    let mut pads = Vec::new();
    for p in i.fp.copper_pads() {
        if p.number.is_empty() || seen.contains(&p.number.as_str()) {
            continue;
        }
        seen.push(&p.number);
        // A pad on the copper of one already numbered (USB-C's B12 on A1) shares its number's place.
        let b = p.outline.bbox();
        if pads.iter().any(|(_, q): &(&str, Rect)| overlaps(q, &b)) {
            continue;
        }
        pads.push((p.number.as_str(), b));
    }
    if pads.len() < 3 {
        return Vec::new();
    }
    if connector(i.lib, &i.fp.name) {
        return pads;
    }
    let every = if pads.len() > DENSE { 10 } else { 5 };
    let numeric = pads.iter().all(|(n, _)| n.parse::<u32>().is_ok());
    pads.into_iter()
        .filter(|(n, _)| match n.parse::<u32>() {
            Ok(k) if numeric => k == 1 || k % every == 0,
            _ => *n == "1" || *n == "A1",
        })
        .collect()
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

fn ink_rect(i: &Ink, at: Pt) -> Rect {
    Rect { x0: at.0 + i.x0, y0: at.1 + i.y0, x1: at.0 + i.x1, y1: at.1 + i.y1 }
}

/// Every label placed at `size`, or the numbers that do not fit.
fn place(
    labels: &[(&str, Rect)],
    pads: &[Rect],
    middle: Pt,
    size: f64,
    back: bool,
) -> Result<Vec<Wanted>, Vec<String>> {
    let mut placed: Vec<Rect> = Vec::new();
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for (text, pad) in labels {
        let Some(ink) = ink(text, size, size, thickness(size)) else { continue };
        let ink = if back { ink.mirrored() } else { ink };
        let (w, h) = (ink.width(), ink.height());
        let (cx, cy) = pad.center();
        // Right, left, below, above: each the middle of the ink beside the pad.
        let sides = [
            ((1.0, 0.0), (pad.x1 + GAP + w / 2.0, cy)),
            ((-1.0, 0.0), (pad.x0 - GAP - w / 2.0, cy)),
            ((0.0, 1.0), (cx, pad.y1 + GAP + h / 2.0)),
            ((0.0, -1.0), (cx, pad.y0 - GAP - h / 2.0)),
        ];
        let (dx, dy) = (cx - middle.0, cy - middle.1);
        // A side is open when no other pad lies in line with this one that way within [`OPEN`]: a number between
        // two pads would read as either's.
        let open = |k: usize| {
            let (ux, _) = sides[k].0;
            !pads.iter().filter(|p| *p != pad).any(|p| {
                let across = if ux != 0.0 { p.y0 < pad.y1 && pad.y0 < p.y1 } else { p.x0 < pad.x1 && pad.x0 < p.x1 };
                let gap = match k {
                    0 => p.x0 - pad.x1,
                    1 => pad.x0 - p.x1,
                    2 => p.y0 - pad.y1,
                    _ => pad.y0 - p.y1,
                };
                across && gap > -1e-9 && gap < OPEN
            })
        };
        // A pad's outer end (along its long side, away from the middle) suits best.
        let end = |k: usize| {
            let (ux, uy) = sides[k].0;
            let long = if pad.w() > pad.h() * 1.2 {
                ux != 0.0
            } else if pad.h() > pad.w() * 1.2 {
                uy != 0.0
            } else {
                false
            };
            long && ux * dx + uy * dy > 0.0
        };
        let mut order: Vec<usize> = (0..4).collect();
        // Open sides first, the pad's outer end first among them, then away from the middle; on a tie, right before
        // left, below before above.
        order.sort_by(|&a, &b| {
            let s = |k: usize| sides[k].0.0 * dx + sides[k].0.1 * dy;
            (open(b), end(b))
                .cmp(&(open(a), end(a)))
                .then(s(b).partial_cmp(&s(a)).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.cmp(&b))
        });
        let found = order.into_iter().find_map(|k| {
            let c = sides[k].1;
            let (ax, ay) = ink.centre();
            let at = (r3(c.0 - ax), r3(c.1 - ay));
            let r = ink_rect(&ink, at);
            // The anchor is rounded to 0.001 mm: the gap may come out that much short.
            let grown = r.expand(GAP - 1e-3);
            let clear = !pads.iter().any(|p| overlaps(&grown, p)) && !placed.iter().any(|q| overlaps(&grown, q));
            clear.then_some((at, r))
        });
        match found {
            Some((at, r)) => {
                placed.push(r);
                out.push(Wanted { text: text.to_string(), at, size });
            }
            None => missing.push(text.to_string()),
        }
    }
    if missing.is_empty() { Ok(out) } else { Err(missing) }
}

fn r3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The numbers to write, and those that fit nowhere even at the smallest size.
fn wanted(i: &Input) -> (Vec<Wanted>, Vec<String>) {
    let labels = labelled(i);
    if labels.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // Holes without copper are in the way too.
    let pads: Vec<Rect> = i.fp.pads.iter().map(|p| p.outline.bbox()).collect();
    let middle = i.fp.pads_box().map_or((0.0, 0.0), |b| b.center());
    for &size in &SIZES {
        if let Ok(w) = place(&labels, &pads, middle, size, i.fp.back) {
            return (w, Vec::new());
        }
    }
    // At the smallest size: those that fit, in order, the rest reported.
    let size = SIZES[SIZES.len() - 1];
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for l in &labels {
        let mut with =
            labels.iter().filter(|x| out.iter().any(|w: &Wanted| w.text == x.0)).copied().collect::<Vec<_>>();
        with.push(*l);
        match place(&with, &pads, middle, size, i.fp.back) {
            Ok(w) => out = w,
            Err(_) => missing.push(l.0.to_string()),
        }
    }
    (out, missing)
}

fn render(w: &Wanted, layer: &str, back: bool) -> String {
    let mirror = if back { "\t\t\t(justify mirror)\n" } else { "" };
    format!(
        "\t(fp_text user {}\n\t\t(at {} {} 0)\n\t\t(unlocked yes)\n\t\t(layer {})\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size {s} {s})\n\t\t\t\t(thickness {})\n\t\t\t)\n{mirror}\t\t)\n\t)\n",
        sexpr::quote(&w.text),
        fmt(w.at.0),
        fmt(w.at.1),
        sexpr::quote(layer),
        fmt(thickness(w.size)),
        s = fmt(w.size),
    )
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let layer = side_of(PINS, i.fp.back);
    let (want, missing) = if i.class == Class::Part { wanted(i) } else { (Vec::new(), Vec::new()) };
    let have: Vec<_> = i.fp.texts.iter().filter(|t| t.layer == layer).collect();
    let same = have.len() == want.len()
        && want.iter().all(|w| {
            have.iter().any(|t| {
                t.text == w.text
                    && (t.at.0 - w.at.0).abs() < 1e-4
                    && (t.at.1 - w.at.1).abs() < 1e-4
                    && (t.size - w.size).abs() < 1e-6
            })
        });
    if !missing.is_empty() {
        out.left(format!("{layer}: no room beside the pads for {}", missing.join(", ")));
    }
    if same {
        return out;
    }
    let had = if have.is_empty() { String::new() } else { format!(" (replacing {})", have.len()) };
    if want.is_empty() {
        out.fixed(format!("{layer}: no pin numbers for this footprint{had}"));
    } else {
        out.fixed(format!("{layer}: {} pin numbers at {} mm{had}", want.len(), fmt(want[0].size)));
    }
    for t in have {
        out.edits.push(delete(i.text, &t.span));
    }
    if !want.is_empty() {
        out.edits.push(insert(i.fp, want.iter().map(|w| render(w, layer, i.fp.back)).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};
    use crate::footprint;

    fn pads(n: usize, pitch: f64) -> String {
        (0..n)
            .map(|k| {
                let (x, y) = if k < n / 2 { (-2.5, k as f64 * pitch) } else { (2.5, (n - 1 - k) as f64 * pitch) };
                format!(
                    "(pad \"{}\" smd rect (at {x} {y}) (size 1.5 {}) (layers \"F.Cu\" \"F.Mask\"))\n",
                    k + 1,
                    pitch * 0.6
                )
            })
            .collect()
    }

    fn numbers(text: &str) -> Vec<String> {
        let f = footprint::read(text).unwrap();
        f.texts.iter().filter(|t| t.layer == "User.19").map(|t| t.text.clone()).collect()
    }

    #[test]
    fn an_ic_gets_pin_1_and_every_5th_outside_its_pads() {
        let t = fp("SOIC-16", &pads(16, 1.27));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Pins);
        let new = new.unwrap();
        assert_eq!(numbers(&new), ["1", "5", "10", "15"]);
        assert_eq!(f[0].message, "User.19: 4 pin numbers at 0.5 mm");
        // Pin 1: its number at the pad's outer end, left of it, clear of it.
        let f = footprint::read(&new).unwrap();
        let one = f.texts.iter().find(|t| t.text == "1" && t.layer == "User.19").unwrap();
        assert!(one.at.0 < -3.25 - super::GAP && one.at.1.abs() < 0.05, "{:?}", one.at);
        settled("VHRD_Package_SO", &t, Rule::Pins);
    }

    #[test]
    fn a_dense_part_gets_every_10th_and_a_connector_every_pin() {
        let t = fp("LQFP-48", &pads(48, 0.5));
        assert_eq!(numbers(&run("VHRD_Package_QFP", &t, Rule::Pins).0.unwrap()), ["1", "10", "20", "30", "40"]);
        let t = fp(
            "PinHeader_1x04",
            &(0..4)
                .map(|k| {
                    format!(
                        "(pad \"{}\" thru_hole oval (at 0 {}) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\"))\n",
                        k + 1,
                        k as f64 * 2.54
                    )
                })
                .collect::<String>(),
        );
        let new = run("VHRD_Connector_PinHeader_2.54mm", &t, Rule::Pins).0.unwrap();
        assert_eq!(numbers(&new), ["1", "2", "3", "4"]);
        // A single row: the inner numbers beside the pads, not between them; the end ones beyond the ends.
        let f = footprint::read(&new).unwrap();
        let at = |n: &str| f.texts.iter().find(|t| t.layer == "User.19" && t.text == n).unwrap().at;
        assert!(at("2").0.abs() > 0.85 && at("3").0.abs() > 0.85, "{new}");
        assert!(at("1").1 < -0.85 && at("4").1 > 3.0 * 2.54 + 0.85, "{new}");
        settled("VHRD_Connector_PinHeader_2.54mm", &t, Rule::Pins);
    }

    #[test]
    fn two_pin_parts_and_test_points_get_none_and_stray_numbers_go() {
        let two = "(pad \"1\" smd rect (at -1 0) (size 1 1) (layers \"F.Cu\"))\n(pad \"2\" smd rect (at 1 0) (size 1 1) (layers \"F.Cu\"))";
        assert!(run("VHRD_Resistor_SMD", &fp("R_0603", two), Rule::Pins).0.is_none());
        let stray =
            format!("{two}\n(fp_text user \"1\" (at 0 0 0) (layer \"User.19\") (effects (font (size 0.5 0.5))))");
        let (new, f) = run("VHRD_Resistor_SMD", &fp("R_0603", &stray), Rule::Pins);
        assert!(!new.unwrap().contains("User.19"));
        assert!(f[0].message.ends_with("(replacing 1)"), "{f:?}");
        let t = fp("TestPoint_3", &pads(4, 1.27));
        assert!(run("VHRD_TestPoint", &t, Rule::Pins).0.is_none());
    }

    #[test]
    fn on_the_back_numbers_are_mirrored_on_pins_b() {
        let t = fp("SOIC-8", &pads(8, 1.27).replace("\"F.Cu\"", "\"B.Cu\""))
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")");
        let new = run("VHRD_Package_SO", &t, Rule::Pins).0.unwrap();
        assert!(new.contains("(layer \"User.20\")") && new.contains("(justify mirror)"), "{new}");
    }

    #[test]
    fn a_pad_on_numbered_copper_and_holes() {
        // B12 on A1's copper gets no number of its own; a hole beside the pads is kept clear.
        let t = fp(
            "USB_C_X",
            r#"(pad "A1" smd rect (at 0 0) (size 0.6 1.2) (layers "F.Cu"))
(pad "B12" smd rect (at 0 0) (size 0.6 1.2) (layers "F.Cu"))
(pad "A4" smd rect (at 2 0) (size 0.6 1.2) (layers "F.Cu"))
(pad "A5" smd rect (at 4 0) (size 0.6 1.2) (layers "F.Cu"))
(pad "" np_thru_hole circle (at 0 -1.4) (size 0.65 0.65) (drill 0.65) (layers "*.Cu" "*.Mask"))"#,
        );
        let new = run("VHRD_Connector_USB", &t, Rule::Pins).0.unwrap();
        assert_eq!(numbers(&new), ["A1", "A4", "A5"]);
        let f = footprint::read(&new).unwrap();
        let a1 = f.texts.iter().find(|t| t.text == "A1" && t.layer == "User.19").unwrap();
        assert!(a1.at.1 > -0.6 || a1.at.1 < -1.4 - 0.33 - super::GAP, "{:?}", a1.at);
    }
}
