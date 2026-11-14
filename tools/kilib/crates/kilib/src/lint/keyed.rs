//! Keyed connectors, switches and mechanical parts: the body fits the board only one way, and the pinout needs the
//! datasheet anyway, so a pin-1 mark only confuses (a USB-C receptacle, an RJ45 jack, a D-sub). They have none,
//! unless pin 1 helps counting along a row of pads ([`counting_row`]: an FFC connector, a pin header).
//!
//! A footprint is keyed when no quarter, half or three-quarter turn, about the origin or the pads' centre, lands
//! every pad (mounting pads and holes without copper included) on a pad of the same kind, shape, size and side and
//! leaves the body where it was: a turn that moves a body sitting to one side of the pads (an edge-mount D-sub's)
//! does not fit either.

use std::collections::BTreeMap;

use super::pin1::{Move, SAME, body_tells};
use crate::footprint::{Footprint, Outline, Pad, Pt, Rect, dist};

/// Pad sizes within this are the same.
const SAME_SIZE: f64 = 0.02;

/// The fewest pads of a row that pin 1 helps to count.
pub const ROW_MIN: usize = 4;

/// Library `lib` holds connectors, switches or mechanical parts (`VHRD_Connector_USB`, `VHRD_Button_Switch_SMD`).
pub fn connector_like(lib: &str) -> bool {
    let lib = lib.strip_prefix("VHRD_").unwrap_or(lib);
    lib.starts_with("Connector") || lib.starts_with("Button_Switch") || lib == "Mechanical"
}

/// The pad's size, turned a quarter round when `quarter`.
fn size(p: &Pad, quarter: bool) -> Pt {
    let b = p.outline.bbox();
    if quarter { (b.h(), b.w()) } else { (b.w(), b.h()) }
}

fn same_size(a: Pt, b: Pt) -> bool {
    (a.0 - b.0).abs() < SAME_SIZE && (a.1 - b.1).abs() < SAME_SIZE
}

fn round(p: &Pad) -> bool {
    matches!(p.outline, Outline::Capsule { .. })
}

/// No quarter, half or three-quarter turn about the origin or the pads' centre lands every pad on a pad of the
/// same kind, shape, size and side and keeps the body in place.
pub fn keyed(fp: &Footprint) -> bool {
    let pads = &fp.pads;
    let Some(centre) = Rect::of_points(pads.iter().map(|p| p.at)).map(|r| r.center()) else { return false };
    let body = fp.fab_box();
    let side = |p: &Pad| (p.on_side("F"), p.on_side("B"));
    let fits = |m: Move, c: Pt| {
        let quarter = m != Move::Turn180;
        body.is_none_or(|b| !body_tells(b, m, c))
            && pads.iter().all(|p| {
                let to = m.apply(p.at, c);
                pads.iter().any(|q| {
                    dist(q.at, to) < SAME
                        && q.kind == p.kind
                        && round(q) == round(p)
                        && side(q) == side(p)
                        && same_size(size(q, false), size(p, quarter))
                })
            })
    };
    !Move::ALL.into_iter().any(|m| [(0.0, 0.0), centre].into_iter().any(|c| fits(m, c)))
}

/// One row of pads: positions along it and the pad numbers, sorted along it.
fn rows(pads: &[(u32, Pt)], along_x: bool) -> Vec<Vec<(f64, u32)>> {
    let mut out: Vec<(f64, Vec<(f64, u32)>)> = Vec::new();
    for &(n, p) in pads {
        let (a, across) = if along_x { (p.0, p.1) } else { (p.1, p.0) };
        match out.iter_mut().find(|(y, _)| (y - across).abs() < SAME) {
            Some((_, r)) => r.push((a, n)),
            None => out.push((across, vec![(a, n)])),
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter()
        .map(|(_, mut r)| {
            r.sort_by(|a, b| a.0.total_cmp(&b.0));
            r
        })
        .collect()
}

/// The pads of `row` sit at one pitch.
fn even(row: &[(f64, u32)]) -> bool {
    let pitch = |k: usize| row[k + 1].0 - row[k].0;
    row.len() >= 2 && pitch(0) > SAME && (1..row.len() - 1).all(|k| (pitch(k) - pitch(0)).abs() < SAME)
}

/// The numbers of `row` step by `step` one way along it.
fn steps(row: &[(f64, u32)], step: i64) -> bool {
    let d: Vec<i64> = row.windows(2).map(|w| i64::from(w[1].1) - i64::from(w[0].1)).collect();
    d.iter().all(|&x| x == step) || d.iter().all(|&x| x == -step)
}

/// Pin 1 helps counting: the pads numbered 1 to N (N at least [`ROW_MIN`]), all of one size but pin 1's shape, sit
/// evenly pitched in one row numbered in order along it, or in a straight double row (each pad across from one in
/// the other row) numbered in order along each row, odd and even across (a 2xN header) or one row after the other.
/// Mounting pads and holes (other numbers, other sizes) do not count. A staggered double row (an RJ45 jack, a D-sub)
/// and named pins (USB-C's A1 to B12) are not counting rows.
pub fn counting_row(fp: &Footprint) -> bool {
    let numbered: Vec<(u32, &Pad)> =
        fp.copper_pads().filter_map(|p| Some((p.number.parse::<u32>().ok().filter(|&n| n > 0)?, p))).collect();
    let Some(two) = numbered.iter().find(|(n, _)| *n == 2).map(|(_, p)| size(p, false)) else { return false };
    let mut by_number: BTreeMap<u32, Pt> = BTreeMap::new();
    for (n, p) in &numbered {
        if (*n == 1 || same_size(size(p, false), two)) && by_number.insert(*n, p.at).is_some() {
            return false;
        }
    }
    let n = by_number.len();
    if n < ROW_MIN || by_number.keys().copied().ne(1..=n as u32) {
        return false;
    }
    let pads: Vec<(u32, Pt)> = by_number.into_iter().collect();
    [true, false].into_iter().any(|along_x| match rows(&pads, along_x).as_slice() {
        [r] => even(r) && steps(r, 1),
        [a, b] => {
            let straight = a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (p.0 - q.0).abs() < SAME);
            let odd_even = steps(a, 2) && steps(b, 2) && a.iter().zip(b).all(|(p, q)| p.1.abs_diff(q.1) == 1);
            let one_then_other = steps(a, 1) && steps(b, 1);
            straight && even(a) && (odd_even || one_then_other)
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint;
    use crate::lint::tests::fp;

    fn pad(n: &str, kind: &str, shape: &str, at: Pt, size: Pt) -> String {
        format!(
            "(pad \"{n}\" {kind} {shape} (at {} {}) (size {} {}) (layers \"F.Cu\" \"F.Mask\"))",
            at.0, at.1, size.0, size.1
        )
    }

    fn read(pads: &[String]) -> Footprint {
        footprint::read(&fp("X", &pads.join("\n"))).unwrap()
    }

    #[test]
    fn a_row_with_mounting_pads_on_one_side_is_keyed() {
        let mut pads: Vec<String> = (0..4)
            .map(|k| pad(&(k + 1).to_string(), "smd", "rect", (-2.25 + 1.5 * f64::from(k), 0.0), (0.8, 1.5)))
            .collect();
        assert!(!keyed(&read(&pads)), "a plain row turns half round onto itself");
        pads.push(pad("MP", "smd", "rect", (-4.0, 3.0), (1.0, 2.0)));
        pads.push(pad("MP", "smd", "rect", (4.0, 3.0), (1.0, 2.0)));
        let f = read(&pads);
        assert!(keyed(&f));
        assert!(counting_row(&f));
    }

    #[test]
    fn a_square_pad_1_keys_a_header() {
        let mut pads = vec![pad("1", "thru_hole", "rect", (0.0, 0.0), (1.7, 1.7))];
        pads.extend(
            (1..4).map(|k| pad(&(k + 1).to_string(), "thru_hole", "oval", (0.0, 2.54 * f64::from(k)), (1.7, 1.7))),
        );
        let f = read(&pads);
        assert!(keyed(&f));
        assert!(counting_row(&f));
        // Three pads: too few to count.
        assert!(!counting_row(&read(&pads[..3])));
    }

    #[test]
    fn a_square_part_a_quarter_turn_fits_is_not_keyed() {
        let pads: Vec<String> = [(-1.0, 0.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0)]
            .iter()
            .enumerate()
            .map(|(k, &at)| {
                pad(&(k + 1).to_string(), "smd", "rect", at, if at.0 == 0.0 { (0.4, 0.8) } else { (0.8, 0.4) })
            })
            .collect();
        assert!(!keyed(&read(&pads)));
    }

    #[test]
    fn double_rows_straight_count_staggered_do_not() {
        // A 2x3 header: 1, 3, 5 in one row, 2, 4, 6 across.
        let header: Vec<String> = (0..6)
            .map(|k| {
                pad(
                    &(k + 1).to_string(),
                    "thru_hole",
                    "oval",
                    (2.54 * f64::from(k % 2), 2.54 * f64::from(k / 2)),
                    (1.7, 1.7),
                )
            })
            .collect();
        assert!(counting_row(&read(&header)));
        // An RJ45: the rows a half pitch apart.
        let rj: Vec<String> = (0..8)
            .map(|k| {
                pad(
                    &(k + 1).to_string(),
                    "thru_hole",
                    "circle",
                    (1.02 * f64::from(k), if k % 2 == 0 { 0.0 } else { 1.78 }),
                    (1.5, 1.5),
                )
            })
            .collect();
        assert!(!counting_row(&read(&rj)));
        // Named pins.
        let usb: Vec<String> = ["A1", "A4", "A5", "B8", "A8", "B5"]
            .iter()
            .enumerate()
            .map(|(k, n)| pad(n, "smd", "rect", (0.5 * k as f64, 0.0), (0.3, 1.0)))
            .collect();
        assert!(!counting_row(&read(&usb)));
    }
}
