//! Room round a real part on a board, for a designator its body holds only small ([`Surroundings::roomier`]).
//!
//! On a board the designator need not stay on the body. Where the body holds it only small (a diode's symbol fills
//! its outline, an 0402 is narrow), it takes the part's own courtyard round the body when it grows there by
//! [`COURTYARD_GROWTH`], or free board beside the courtyard when it grows there by [`NEAR_GROWTH`]: a larger text
//! fits only where the board is open, so the designator drifts towards the most room. Beside the courtyard its ink
//! keeps [`NEIGHBOUR_GAP_MIN`] from every other courtyard on its side, [`EDGE_GAP`] from the board's edge and holes,
//! [`LABEL_GAP_MIN`] from the designators already moved out, and grows to [`NEAR_SIZE_MAX`] at most. It reads as its
//! own part's: it sits beside a side of the courtyard (not off a corner), within [`OWN_GAP_MAX`], and every other
//! courtyard is [`NEIGHBOUR_RATIO`] times as far from its middle. The parts whose body holds the smallest text choose
//! first.

use super::{EPS, MAX_SIZE, Placement, Prim, Rect, Shape, inside_polygon, place, place_where, segment_distance, sizes};
use crate::footprint::{Pt, turner};
use crate::sexpr::Node;

/// The growth that takes a designator off the body into the part's own courtyard.
pub(crate) const COURTYARD_GROWTH: f64 = 1.25;
/// The growth that takes a designator out of the part's courtyard, beside it.
pub(crate) const NEAR_GROWTH: f64 = 1.5;
/// How far beside the courtyard the ink may reach.
pub(crate) const NEAR_REACH: f64 = 1.0;
/// The ink beside the courtyard stays this close to it, so it reads as the part's.
pub(crate) const OWN_GAP_MAX: f64 = 0.3;
/// The largest text beside the courtyard: larger, it would outweigh the small part it names.
pub(crate) const NEAR_SIZE_MAX: f64 = 0.6;
/// Room kept between the ink and another part's courtyard.
pub(crate) const NEIGHBOUR_GAP_MIN: f64 = 0.15;
/// Room kept between two designators moved out beside their parts.
pub(crate) const LABEL_GAP_MIN: f64 = 0.5;
/// Every other courtyard is at least this many times as far from the ink's middle as the part's own.
pub(crate) const NEIGHBOUR_RATIO: f64 = 3.0;
/// Room kept between the ink and the board's edge (Edge.Cuts, holes included), beyond its stroke.
pub(crate) const EDGE_GAP: f64 = 0.2;
/// Grid of the positions tried beside the courtyard.
const NEAR_STEP: f64 = 0.1;

/// A footprint's frame on the board: its origin and angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    origin: Pt,
    phi: f64,
}

impl Frame {
    pub(crate) fn of(node: &Node) -> Frame {
        let at = node.child("at");
        let n = |i: usize| at.and_then(|a| super::num(a.items().get(i))).unwrap_or(0.0);
        Frame { origin: (n(1), n(2)), phi: n(3) }
    }

    fn to_board(self, p: Pt) -> Pt {
        turner(self.origin, self.phi)(p)
    }

    /// The inverse of [`Frame::to_board`].
    fn to_local(self, p: Pt) -> Pt {
        let (sin, cos) = self.phi.to_radians().sin_cos();
        let (dx, dy) = (p.0 - self.origin.0, p.1 - self.origin.1);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    }

    /// A box of this frame as the corners of the same box in `other`'s.
    fn rect_into(self, r: &Rect, other: Frame) -> Vec<Pt> {
        r.corners().into_iter().map(|c| other.to_local(self.to_board(c))).collect()
    }
}

/// A part's outline: its courtyard, else its body.
#[derive(Debug, Clone, Copy)]
struct Outline {
    idx: usize,
    frame: Frame,
    back: bool,
    rect: Rect,
}

/// What lies round the parts of a board: their outlines, its edge, and the designators moved out so far.
#[derive(Debug, Clone)]
pub(crate) struct Surroundings {
    outlines: Vec<Outline>,
    /// Edge.Cuts in board coordinates, each line with its stroke.
    edge: Vec<Prim>,
    /// Designators moved out of their courtyard, in board coordinates, with the side they are on.
    placed: Vec<(bool, Vec<Pt>)>,
}

/// A shape's outline in its own frame: the courtyard, else the body.
fn outline_of(shape: &Shape) -> Rect {
    shape.courtyard.unwrap_or(shape.body)
}

/// How far the point is from the polygon `pts`, 0 inside it.
fn point_gap(p: Pt, pts: &[Pt]) -> f64 {
    if inside_polygon(p, pts) {
        return 0.0;
    }
    (0..pts.len()).map(|i| segment_distance(p, pts[i], pts[(i + 1) % pts.len()])).fold(f64::INFINITY, f64::min)
}

/// The ink `r` sits beside a side of `own`, not off a corner: within [`OWN_GAP_MAX`] of it, and alongside it for at
/// least half the shorter of the two spans.
fn beside(own: &Rect, r: &Rect) -> bool {
    let along =
        |lo0: f64, hi0: f64, lo1: f64, hi1: f64| hi0.min(hi1) - lo0.max(lo1) >= 0.5 * (hi0 - lo0).min(hi1 - lo1) - EPS;
    own.gap(r) <= OWN_GAP_MAX + EPS && (along(own.x0, own.x1, r.x0, r.x1) || along(own.y0, own.y1, r.y0, r.y1))
}

fn bounds(pts: &[Pt]) -> Option<Rect> {
    Rect::of_points(pts.iter().copied())
}

impl Surroundings {
    /// Reads the board's Edge.Cuts (its own and its footprints') and takes `parts`: each footprint's index, frame
    /// and shape.
    pub(crate) fn read<'a>(pcb: &Node, parts: impl IntoIterator<Item = (usize, Frame, &'a Shape)>) -> Surroundings {
        let on_edge = |g: &&Node| g.value("layer") == Some("Edge.Cuts");
        let mut edge: Vec<Prim> = pcb
            .items()
            .iter()
            .filter(|g| g.head().is_some_and(|h| h.starts_with("gr_")))
            .filter(on_edge)
            .flat_map(|g| super::graphic_prims(g, super::stroke_width(g)))
            .collect();
        for fp in pcb.children("footprint") {
            let frame = Frame::of(fp);
            for g in fp.items().iter().filter(|g| super::is_graphic(g)).filter(on_edge) {
                edge.extend(
                    super::graphic_prims(g, super::stroke_width(g))
                        .into_iter()
                        .map(|p| map_prim(&p, |q| frame.to_board(q))),
                );
            }
        }
        let outlines = parts
            .into_iter()
            .map(|(idx, frame, s)| Outline { idx, frame, back: s.back, rect: outline_of(s) })
            .collect();
        Surroundings { outlines, edge, placed: Vec::new() }
    }

    /// The point lies inside the board's edge (an odd number of edge lines to its right, outside every hole); any
    /// point does on a board without Edge.Cuts.
    fn on_board(&self, p: Pt) -> bool {
        if self.edge.is_empty() {
            return true;
        }
        let mut inside = false;
        for g in &self.edge {
            match g {
                Prim::Seg { a, b, .. } => {
                    if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                        inside = !inside;
                    }
                }
                Prim::Ring { c, r, .. } => {
                    if (p.0 - c.0).hypot(p.1 - c.1) < *r {
                        inside = !inside;
                    }
                }
                Prim::Area(_) => {}
            }
        }
        inside
    }

    /// Where part `idx` (in `frame`, its designator fitted to its body at `inside`) shows its designator: in its
    /// courtyard or beside it where the text grows by [`COURTYARD_GROWTH`] or [`NEAR_GROWTH`], else at `inside`. A
    /// designator moved beside the courtyard is kept clear of by the parts that choose after it.
    pub(crate) fn roomier(&mut self, idx: usize, frame: Frame, shape: &Shape, inside: Placement) -> Placement {
        let base = if inside.clean { inside.size } else { 0.0 };
        if base >= MAX_SIZE - EPS {
            return inside;
        }
        let at_least = |floor: f64| sizes().into_iter().filter(|&s| s >= floor - EPS).collect::<Vec<_>>();
        let mut best = inside;
        if let Some(c) = shape.courtyard {
            let s = Shape { region: c, ..shape.clone() };
            if let Some(p) = place(&s, &at_least(base * COURTYARD_GROWTH)) {
                best = p;
            }
        }
        let floor = if best.clean { best.size } else { 0.0 };
        let near: Vec<f64> = at_least(floor * NEAR_GROWTH).into_iter().filter(|&s| s <= NEAR_SIZE_MAX + EPS).collect();
        if near.is_empty() {
            return best;
        }
        let own = outline_of(shape);
        let region = own.expand(NEAR_REACH);
        let mut graphics = shape.graphics.clone();
        let mut others: Vec<Vec<Pt>> = Vec::new();
        for o in self.outlines.iter().filter(|o| o.idx != idx && o.back == shape.back) {
            let pts = o.frame.rect_into(&o.rect.expand(NEIGHBOUR_GAP_MIN), frame);
            if bounds(&pts).is_some_and(|b| b.overlaps(&region.expand(NEAR_REACH))) {
                graphics.push(Prim::Area(pts));
                others.push(o.frame.rect_into(&o.rect, frame));
            }
        }
        for (_, pts) in self.placed.iter().filter(|(back, _)| *back == shape.back) {
            let pts: Vec<Pt> = pts.iter().map(|&p| frame.to_local(p)).collect();
            if bounds(&pts).is_some_and(|b| b.overlaps(&region)) {
                graphics.push(Prim::Area(pts));
            }
        }
        graphics.extend(
            self.edge
                .iter()
                .map(|g| map_prim(&widened(g, EDGE_GAP), |p| frame.to_local(p)))
                .filter(|g| bounds(&g.points()).is_some_and(|b| b.overlaps(&region))),
        );
        let s = Shape { region, graphics, ..shape.clone() };
        let ok = |r: &Rect| {
            let c = r.center();
            let mine = own.distance_to(c);
            beside(&own, r)
                && others.iter().all(|o| point_gap(c, o) >= NEIGHBOUR_RATIO * mine - EPS)
                && self.on_board(frame.to_board(c))
        };
        let Some(p) = place_where(&s, &near, NEAR_STEP, ok) else { return best };
        let ink = s.text_rect(p.x, p.y, p.angle, p.size).expand(LABEL_GAP_MIN);
        if !own.contains(&ink) {
            self.placed.push((shape.back, ink.corners().into_iter().map(|c| frame.to_board(c)).collect()));
        }
        p
    }
}

/// A drawing primitive with each point mapped by `f` (a turn, a shift: its strokes and radii stay).
fn map_prim(p: &Prim, f: impl Fn(Pt) -> Pt) -> Prim {
    match p {
        Prim::Seg { a, b, hw } => Prim::Seg { a: f(*a), b: f(*b), hw: *hw },
        Prim::Ring { c, r, hw } => Prim::Ring { c: f(*c), r: *r, hw: *hw },
        Prim::Area(pts) => Prim::Area(pts.iter().map(|&q| f(q)).collect()),
    }
}

/// A line's stroke widened by `by` on each side.
fn widened(p: &Prim, by: f64) -> Prim {
    match p {
        Prim::Seg { a, b, hw } => Prim::Seg { a: *a, b: *b, hw: hw + by },
        Prim::Ring { c, r, hw } => Prim::Ring { c: *c, r: *r, hw: hw + by },
        Prim::Area(pts) => Prim::Area(pts.clone()),
    }
}
