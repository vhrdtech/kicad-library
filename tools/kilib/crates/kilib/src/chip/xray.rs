//! The x-ray top view of a generated model: every edge of its solids seen from above, hidden ones included.
//!
//! The solids are prisms (an outline extruded from `z0` to `z1`), so seen from above each one is its outline: the
//! top and bottom faces fall on the same lines, the side edges on points. The outlines of all solids are laid over
//! each other, and lines on one straight line that touch or overlap become one. Parallel lines closer than
//! [`SAME_LINE_MM_MAX`] are one line, at the one farther from the centre: a ceramic body set a hundredth inside its
//! terminations draws no second line beside the outline.

use crate::footprint::Pt;
use crate::step::Solid;

/// Parallel lines closer than this are one line (mm).
const SAME_LINE_MM_MAX: f64 = 0.0201;
/// Shorter lines are dropped, and gaps this small along a line are closed (mm).
const LENGTH_MM_MIN: f64 = 1e-6;

/// One edge on its line: the unit direction (pointing into the upper half plane), the offset of the line from the
/// origin along its normal, and the span along the direction.
#[derive(Debug, Clone, Copy)]
struct Edge {
    dir: Pt,
    offset: f64,
    t: (f64, f64),
}

impl Edge {
    fn new(a: Pt, b: Pt) -> Option<Self> {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy);
        if len < LENGTH_MM_MIN {
            return None;
        }
        let mut dir = (dx / len, dy / len);
        if dir.1 < -1e-12 || (dir.1.abs() <= 1e-12 && dir.0 < 0.0) {
            dir = (-dir.0, -dir.1);
        }
        // -0.0 would sort apart from 0.0.
        dir = (dir.0 + 0.0, dir.1 + 0.0);
        let along = |p: Pt| p.0 * dir.0 + p.1 * dir.1;
        let (ta, tb) = (along(a), along(b));
        Some(Edge { dir, offset: a.1 * dir.0 - a.0 * dir.1 + 0.0, t: (ta.min(tb), ta.max(tb)) })
    }

    fn point(&self, t: f64) -> Pt {
        (t * self.dir.0 - self.offset * self.dir.1, t * self.dir.1 + self.offset * self.dir.0)
    }
}

/// The lines of the solids' x-ray top view, in footprint coordinates (y down), each a start and an end.
pub(crate) fn lines(solids: &[Solid]) -> Vec<[Pt; 2]> {
    let mut edges: Vec<Edge> = Vec::new();
    for s in solids {
        let n = s.outline.len();
        for i in 0..n {
            let (a, b) = (s.outline[i], s.outline[(i + 1) % n]);
            edges.extend(Edge::new((a[0], -a[1]), (b[0], -b[1])));
        }
    }
    edges.sort_by(|a, b| {
        a.dir.0.total_cmp(&b.dir.0).then(a.dir.1.total_cmp(&b.dir.1)).then(a.offset.total_cmp(&b.offset))
    });
    let mut out = Vec::new();
    let mut i = 0;
    while i < edges.len() {
        // One group: the same direction, offsets within SAME_LINE_MM_MAX of the first.
        let first = edges[i];
        let mut j = i;
        while j < edges.len()
            && (edges[j].dir.0 - first.dir.0).abs() < 1e-9
            && (edges[j].dir.1 - first.dir.1).abs() < 1e-9
            && edges[j].offset - first.offset <= SAME_LINE_MM_MAX
        {
            j += 1;
        }
        let group = &mut edges[i..j];
        group.sort_by(|a, b| a.t.0.total_cmp(&b.t.0));
        let mut run = group[0];
        for e in group.iter().skip(1) {
            if e.t.0 <= run.t.1 + LENGTH_MM_MIN {
                run.t.1 = run.t.1.max(e.t.1);
                if e.offset.abs() > run.offset.abs() {
                    run.offset = e.offset;
                }
            } else {
                out.push([run.point(run.t.0), run.point(run.t.1)]);
                run = *e;
            }
        }
        out.push([run.point(run.t.0), run.point(run.t.1)]);
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::Rgb;

    fn cuboid(x: (f64, f64), y: (f64, f64)) -> Solid {
        Solid::cuboid("s", [x.0, y.0, 0.0], [x.1, y.1, 1.0], Rgb(0.5, 0.5, 0.5))
    }

    fn rounded(lines: &[[Pt; 2]]) -> Vec<[(i64, i64); 2]> {
        let r = |p: Pt| ((p.0 * 1000.0).round() as i64, (p.1 * 1000.0).round() as i64);
        let mut v: Vec<_> = lines.iter().map(|[a, b]| [r(*a), r(*b)]).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn a_chip_is_its_outline_and_two_band_lines() {
        // 0402 MLCC: terminals 0.25 long, the body 0.01 inside them.
        let solids = [
            cuboid((-0.25, 0.25), (-0.24, 0.24)),
            cuboid((-0.5, -0.25), (-0.25, 0.25)),
            cuboid((0.25, 0.5), (-0.25, 0.25)),
        ];
        let got = rounded(&lines(&solids));
        let want = rounded(&[
            [(-0.5, -0.25), (0.5, -0.25)],
            [(-0.5, 0.25), (0.5, 0.25)],
            [(-0.5, -0.25), (-0.5, 0.25)],
            [(0.5, -0.25), (0.5, 0.25)],
            [(-0.25, -0.25), (-0.25, 0.25)],
            [(0.25, -0.25), (0.25, 0.25)],
        ]);
        assert_eq!(got, want);
    }

    #[test]
    fn hidden_electrodes_under_a_body_stay() {
        let solids = [cuboid((-1.0, 1.0), (-1.0, 1.0)), cuboid((-0.9, -0.5), (-0.6, 0.6))];
        let got = lines(&solids);
        assert_eq!(got.len(), 8, "{got:?}");
    }
}
