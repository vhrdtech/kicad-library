//! Footprints whose designator lives on a layer of its own, and whose fab layer carries the drawing alone: the
//! `Reference` property on Ref.Des (Ref.Des.B for a bottom-side footprint; Ref.Test, Ref.Tie or Ref.Mech for the
//! other [`Class`]es), visible and unlocked, no `${REFERENCE}` text on the silkscreen, the fab layer or the
//! designator's layer. A real part's Value goes to Value, shown and centred on the body the same way; the Value of a
//! test point, net tie or mechanical footprint stays hidden on the fab layer. The designator's ink sits centred on
//! the body (the fab outline's, else the courtyard's): KiCad draws the selected layers on top of the pads, so text
//! over the pads reads, while text outside the body runs into the neighbouring parts. It never touches the drawing
//! on the fab layer of its side (the outline, polarity and pin-1 marks, diode symbols, inner lines), so the two
//! read together: [`GAP`] stays free between the strokes, the text measured with KiCad's stroke font
//! ([`crate::stroke`]). Ref.Des and Value are both centred, so they are shown one at a time. A library footprint is sized for the longest
//! realistic designator of its kind ([`library_refs`]: `R1004`, `C1001`, `TP526`, `U1004`); on a board
//! ([`plan_board`]) each footprint's own designator is fitted, so a short one gets larger text. To fit, it gets
//! smaller (from [`MAX_SIZE`] down to [`MIN_SIZE`]) or turns to read top to bottom; where neither fits at the centre it moves to the
//! largest free spot inside the body nearest the centre. Only where nothing fits at [`MIN_SIZE`] (0201, 01005) does
//! it go down to [`TINY_SIZE`], reported as tiny. Where nothing fits at all it stays on the centre at [`MIN_SIZE`]
//! and the placement is not clean. Every line on the fab layers of a library footprint gets a stroke of
//! [`FAB_STROKE`]. Every other byte of the footprint stays as it was.
//!
//! [`check`] judges a footprint by these rules; [`plan`] rewrites only the footprints that break one, and what it
//! writes passes [`check`].
//!
//! A layout note ([`layout_note`]) sits apart: Ref.Des and Crit/Imp show together on the Floorplan preset, so a
//! note hugs an edge of the real courtyard instead of the centre, clear of the designator's own ink.

use std::f64::consts::TAU;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::layers::{self, Class, class_of};
use crate::migrate::{Edit, apply_edits};
use crate::reading::reading;
use crate::sexpr::{self, Node};
pub use crate::stroke::Ink;
use crate::stroke::ink_of_all;

/// KiCad's convention for fab text is 1 mm, smaller to fit the part.
pub const MAX_SIZE: f64 = 1.0;
/// Below this a designator no longer reads well on a fab print; 0402 takes about 0.2 at five characters.
pub const MIN_SIZE: f64 = 0.2;
/// The floor for the smallest parts (0201, 01005), where nothing fits at [`MIN_SIZE`]; reported.
pub const TINY_SIZE: f64 = 0.15;
/// The floor for a layout note ([`layout_note`]): a short note (`U101.5 VDD`) reads fine much smaller than a
/// designator, and fitting it on the body matters more than its size.
pub const NOTE_TINY_SIZE: f64 = 0.05;
/// The sizes a designator takes outside a reference-layer drawing too small to hold it.
pub const OUTSIDE_SIZES: [f64; 2] = [0.3, 0.2];
/// How far outside a reference-layer drawing such a designator may go.
pub const OUTSIDE_REACH: f64 = 1.5;
/// Sizes tried from [`MAX_SIZE`] down to [`FINE_BELOW`]; finer steps below it.
pub const SIZE_STEP: f64 = 0.05;
/// Below this size the steps are [`FINE_STEP`].
pub const FINE_BELOW: f64 = 0.3;
pub const FINE_STEP: f64 = 0.01;
/// The stroke of every line on the fab layers.
pub const FAB_STROKE: f64 = 0.05;
/// Room kept between the designator's ink and the other drawing on its layer, beyond the strokes.
pub const GAP: f64 = 0.02;
/// Text strokes are 15 % of the size, never thinner than this.
pub const MIN_THICKNESS: f64 = 0.025;
/// Grid of the positions tried off the centre.
const STEP: f64 = 0.05;
/// At most this many grid steps across the body (a coarser grid on a large body).
const MAX_STEPS: f64 = 200.0;
/// Segments an arc is drawn with.
const ARC_SEGMENTS: usize = 24;
const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn around(cx: f64, cy: f64, hw: f64, hh: f64) -> Rect {
        Rect { x0: cx - hw, y0: cy - hh, x1: cx + hw, y1: cy + hh }
    }

    fn of_points(pts: impl IntoIterator<Item = (f64, f64)>) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        for (x, y) in pts {
            r = Some(match r {
                None => Rect { x0: x, y0: y, x1: x, y1: y },
                Some(r) => Rect { x0: r.x0.min(x), y0: r.y0.min(y), x1: r.x1.max(x), y1: r.y1.max(y) },
            });
        }
        r
    }

    pub fn w(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn h(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center(&self) -> (f64, f64) {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }

    pub fn expand(&self, m: f64) -> Rect {
        Rect { x0: self.x0 - m, y0: self.y0 - m, x1: self.x1 + m, y1: self.y1 + m }
    }

    /// The interiors overlap (touching edges do not).
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 - EPS && o.x0 < self.x1 - EPS && self.y0 < o.y1 - EPS && o.y0 < self.y1 - EPS
    }

    pub fn contains(&self, o: &Rect) -> bool {
        o.x0 >= self.x0 - EPS && o.x1 <= self.x1 + EPS && o.y0 >= self.y0 - EPS && o.y1 <= self.y1 + EPS
    }

    fn corners(&self) -> [(f64, f64); 4] {
        [(self.x0, self.y0), (self.x1, self.y0), (self.x1, self.y1), (self.x0, self.y1)]
    }

    /// How far a point is from the box, 0 inside it.
    fn distance_to(&self, p: (f64, f64)) -> f64 {
        let dx = (self.x0 - p.0).max(p.0 - self.x1).max(0.0);
        let dy = (self.y0 - p.1).max(p.1 - self.y1).max(0.0);
        dx.hypot(dy)
    }

    /// The segment a-b crosses or lies in the box (Liang-Barsky clipping).
    fn meets_segment(&self, a: (f64, f64), b: (f64, f64)) -> bool {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q) in [(-dx, a.0 - self.x0), (dx, self.x1 - a.0), (-dy, a.1 - self.y0), (dy, self.y1 - a.1)] {
            if p.abs() < 1e-12 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
                if t0 > t1 {
                    return false;
                }
            }
        }
        true
    }
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-18 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

fn inside_polygon(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let mut inside = false;
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            inside = !inside;
        }
    }
    inside
}

/// A piece of drawing on the fab layer, the stroke included.
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    /// A line from `a` to `b`, `hw` half its stroke.
    Seg { a: (f64, f64), b: (f64, f64), hw: f64 },
    /// A circle line of radius `r` round `c` (a filled circle: `r` 0, `hw` its radius).
    Ring { c: (f64, f64), r: f64, hw: f64 },
    /// A filled area (a filled rectangle or polygon, a text's box).
    Area(Vec<(f64, f64)>),
}

impl Prim {
    fn points(&self) -> Vec<(f64, f64)> {
        match self {
            Prim::Seg { a, b, hw } => {
                vec![(a.0 - hw, a.1 - hw), (a.0 + hw, a.1 + hw), (b.0 - hw, b.1 - hw), (b.0 + hw, b.1 + hw)]
            }
            Prim::Ring { c, r, hw } => vec![(c.0 - r - hw, c.1 - r - hw), (c.0 + r + hw, c.1 + r + hw)],
            Prim::Area(pts) => pts.clone(),
        }
    }

    /// Text with the ink box `t` comes within [`GAP`] of this.
    pub fn hits(&self, t: &Rect) -> bool {
        match self {
            Prim::Seg { a, b, hw } => {
                let d = if t.meets_segment(*a, *b) {
                    0.0
                } else {
                    let ends = t.distance_to(*a).min(t.distance_to(*b));
                    t.corners().iter().map(|&c| segment_distance(c, *a, *b)).fold(ends, f64::min)
                };
                d < hw + GAP - EPS
            }
            Prim::Ring { c, r, hw } => {
                let near = t.distance_to(*c);
                let far = t.corners().iter().map(|k| (k.0 - c.0).hypot(k.1 - c.1)).fold(0.0, f64::max);
                near < r + hw + GAP - EPS && far > r - hw - GAP + EPS
            }
            Prim::Area(pts) => {
                let t = t.expand(GAP - EPS);
                t.corners().iter().any(|&c| inside_polygon(c, pts))
                    || pts.iter().any(|&p| t.distance_to(p) == 0.0)
                    || pts.iter().enumerate().any(|(i, &a)| t.meets_segment(a, pts[(i + 1) % pts.len()]))
            }
        }
    }
}

/// Where the body box came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFrom {
    Fab,
    /// The drawing on the designator's own reference layer (a test point's circle, a net tie's box).
    RefLayer,
    Courtyard,
    Pads,
}

/// What the layout reads of a footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// The body's box (the fab drawing's, strokes included, else the courtyard's or the pads').
    pub body: Rect,
    pub body_from: BodyFrom,
    /// Where the designator's ink may go: the body's box, inside the courtyard's stroke for a body from it.
    pub region: Rect,
    /// The drawing on the designator's fab layer, each line with its stroke.
    pub graphics: Vec<Prim>,
    /// A bottom-side footprint: its text is mirrored.
    pub back: bool,
    /// The designators the text must hold: the box fitted holds the ink of each.
    pub texts: Vec<String>,
    /// Angle the text is drawn at for each of the two placements (level, and vertical reading top to bottom,
    /// [`crate::reading`]) in the footprint's frame: on a board a rotated part turns its text, which then turns by a
    /// half turn where needed.
    pub drawn: [f64; 2],
    /// The courtyard's inside (F.CrtYd/B.CrtYd, its own stroke already subtracted), where [`layout_note`] hugs an
    /// edge; `None` where the footprint draws no courtyard (or [`body_from`](Shape::body_from) is already
    /// [`BodyFrom::Courtyard`], leaving no room between the body and it), and a note falls back to the centre.
    pub courtyard: Option<Rect>,
}

impl Shape {
    /// The angle a vertical text is written at: reading top to bottom ([`crate::reading::vertical`]).
    pub fn vertical(&self) -> f64 {
        crate::reading::vertical(self.back)
    }

    /// The body's centre, rounded the way it is written.
    pub fn centre(&self) -> (f64, f64) {
        let (x, y) = self.body.center();
        (round3(x), round3(y))
    }

    /// The designator's ink box from its anchor at `size`, placed at `angle` (level, or vertical: [`Shape::vertical`], in the footprint's frame).
    pub fn ink(&self, angle: f64, size: f64) -> Ink {
        let i = ink_of_all(self.texts.iter().map(String::as_str), size, size, thickness(size)).unwrap_or(Ink {
            x0: 0.0,
            x1: 0.0,
            y0: 0.0,
            y1: 0.0,
        });
        let i = if self.back { i.mirrored() } else { i };
        i.turned(self.drawn_angle(angle))
    }

    fn drawn_angle(&self, angle: f64) -> f64 {
        if angle == 0.0 { self.drawn[0] } else { self.drawn[1] }
    }

    /// The anchor that puts the ink's middle on `c`, rounded the way it is written.
    pub fn anchor(&self, c: (f64, f64), angle: f64, size: f64) -> (f64, f64) {
        let (dx, dy) = self.ink(angle, size).centre();
        (round3(c.0 - dx), round3(c.1 - dy))
    }

    /// The designator's ink box with its anchor at (x, y).
    pub fn text_rect(&self, x: f64, y: f64, angle: f64, size: f64) -> Rect {
        rect_at(&self.ink(angle, size), x, y)
    }

    /// The designator fits with its anchor at (x, y): inside the body, clear of the drawing.
    pub fn fits(&self, x: f64, y: f64, angle: f64, size: f64) -> bool {
        self.fits_rect(&self.text_rect(x, y, angle, size))
    }

    fn fits_rect(&self, r: &Rect) -> bool {
        self.region.contains(r) && !self.graphics.iter().any(|g| g.hits(r))
    }

    /// The largest of `sizes` whose ink fits centred on `c` at `angle`, and its anchor.
    fn room(&self, sizes: &[f64], c: (f64, f64), angle: f64) -> Option<(f64, (f64, f64))> {
        sizes.iter().map(|&s| (s, self.anchor(c, angle, s))).find(|&(s, (x, y))| self.fits(x, y, angle, s))
    }
}

fn rect_at(i: &Ink, x: f64, y: f64) -> Rect {
    Rect { x0: x + i.x0, y0: y + i.y0, x1: x + i.x1, y1: y + i.y1 }
}

/// `from` down to `to` in `step`s, rounded to 0.01.
fn steps(from: f64, to: f64, step: f64) -> impl Iterator<Item = f64> {
    let n = ((from - to) / step + EPS).floor() as i64;
    (0..=n).map(move |i| ((from - i as f64 * step) * 100.0).round() / 100.0)
}

/// [`MAX_SIZE`] down to [`MIN_SIZE`]: in [`SIZE_STEP`]s to [`FINE_BELOW`], then in [`FINE_STEP`]s.
fn sizes() -> Vec<f64> {
    let mut v: Vec<f64> = steps(MAX_SIZE, FINE_BELOW, SIZE_STEP).collect();
    v.extend(steps(FINE_BELOW - FINE_STEP, MIN_SIZE, FINE_STEP));
    v
}

/// Below [`MIN_SIZE`] down to `floor` ([`TINY_SIZE`] for a designator or Value, [`NOTE_TINY_SIZE`] for a layout
/// note), for text where nothing larger fits.
fn tiny_sizes(floor: f64) -> Vec<f64> {
    steps(MIN_SIZE - FINE_STEP, floor, FINE_STEP).collect()
}

/// A designator's size, anchor and angle (level, or vertical: [`Shape::vertical`], in the footprint's frame); `clean` when it fits by the rules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub size: f64,
    pub clean: bool,
}

impl Placement {
    /// Fits only below [`MIN_SIZE`].
    pub fn tiny(&self) -> bool {
        self.size < MIN_SIZE - EPS
    }
}

/// KiCad's stroke width for fab text: 15 % of the size, at least [`MIN_THICKNESS`].
pub fn thickness(size: f64) -> f64 {
    ((size * 0.15 * 1000.0).round() / 1000.0).max(MIN_THICKNESS)
}

fn round3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The grid points of `area` round `centre`, nearest first (then the least off level, then left and up first).
fn grid(area: Rect, centre: (f64, f64)) -> Vec<(f64, f64)> {
    let step = STEP * (area.w().max(area.h()) / (STEP * MAX_STEPS)).ceil().max(1.0);
    let span = |lo: f64, c: f64, hi: f64| {
        (-(((c - lo) / step + EPS).floor() as i64))..=(((hi - c) / step + EPS).floor() as i64)
    };
    let mut pts: Vec<(i64, i64, (f64, f64))> = Vec::new();
    for j in span(area.y0, centre.1, area.y1) {
        for i in span(area.x0, centre.0, area.x1) {
            pts.push((i, j, (round3(centre.0 + i as f64 * step), round3(centre.1 + j as f64 * step))));
        }
    }
    pts.sort_by_key(|&(i, j, _)| (i * i + j * j, j.abs(), j, i));
    pts.into_iter().map(|(_, _, p)| p).collect()
}

/// The largest of `sizes` that fits with the ink centred on the body, level or vertical (level on a tie); else the
/// largest that fits anywhere inside the body, the ink's middle nearest the centre.
fn place(shape: &Shape, sizes: &[f64]) -> Option<Placement> {
    let c = shape.centre();
    let at = |angle: f64, (size, (x, y)): (f64, (f64, f64))| Placement { x, y, angle, size, clean: true };
    let v = shape.vertical();
    match (shape.room(sizes, c, 0.0), shape.room(sizes, c, v)) {
        (Some(a), Some(b)) if b.0 > a.0 + EPS => return Some(at(v, b)),
        (Some(a), _) => return Some(at(0.0, a)),
        (None, Some(b)) => return Some(at(v, b)),
        (None, None) => {}
    }
    let pts = grid(shape.region, c);
    for &size in sizes {
        let inks = [shape.ink(0.0, size), shape.ink(v, size)];
        for &(x, y) in &pts {
            for (k, angle) in [0.0, v].into_iter().enumerate() {
                let (dx, dy) = inks[k].centre();
                let (ax, ay) = (round3(x - dx), round3(y - dy));
                if shape.fits_rect(&rect_at(&inks[k], ax, ay)) {
                    return Some(Placement { x: ax, y: ay, angle, size, clean: true });
                }
            }
        }
    }
    None
}

/// The largest size from [`MAX_SIZE`] to [`MIN_SIZE`] that fits ([`place`]); else the same down to `floor`; else on
/// the centre at [`MIN_SIZE`], turned along the body, not clean.
fn layout_with_floor(shape: &Shape, floor: f64) -> Placement {
    if let Some(p) = place(shape, &sizes()).or_else(|| place(shape, &tiny_sizes(floor))) {
        return p;
    }
    // A reference layer's drawing too small for any text: the designator goes outside it, nearest.
    if shape.body_from == BodyFrom::RefLayer {
        let b = shape.body;
        let mut outside = shape.clone();
        outside.region = b.expand(OUTSIDE_REACH);
        outside.graphics.push(Prim::Area(vec![(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)]));
        if let Some(p) = place(&outside, &OUTSIDE_SIZES) {
            return p;
        }
    }
    let angle = if shape.region.h() > shape.region.w() + EPS { shape.vertical() } else { 0.0 };
    let (x, y) = shape.anchor(shape.centre(), angle, MIN_SIZE);
    Placement { x, y, angle, size: MIN_SIZE, clean: false }
}

/// Where the designator or Value goes: [`layout_with_floor`] down to [`TINY_SIZE`].
pub fn layout(shape: &Shape) -> Placement {
    layout_with_floor(shape, TINY_SIZE)
}

/// Which edge of the courtyard a layout note hugs ([`layout_note`]): the body's top or bottom for a courtyard at
/// least as wide as tall, its left or right for one taller than wide (the longer pair usually has more room).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// The edge pair [`layout_note`] prefers, the longer one: top and bottom for a courtyard at least as wide as tall,
/// left and right for one taller than wide. [`crate::lint::layout::FIELDS`]' two notes take one each, so they land
/// on opposite sides of the body whenever both fit there.
pub fn note_edges(courtyard: Rect) -> [Edge; 2] {
    if courtyard.w() >= courtyard.h() { [Edge::Top, Edge::Bottom] } else { [Edge::Left, Edge::Right] }
}

/// `prefer`, its opposite, then the other pair, screen order.
fn edge_order(prefer: Edge) -> [Edge; 4] {
    use Edge::*;
    match prefer {
        Top => [Top, Bottom, Left, Right],
        Bottom => [Bottom, Top, Left, Right],
        Left => [Left, Right, Top, Bottom],
        Right => [Right, Left, Top, Bottom],
    }
}

/// The slice of `courtyard` beyond `body`'s `edge`; `None` where there is no room (the body already reaches that
/// side of the courtyard, or past it).
fn edge_band(courtyard: Rect, body: Rect, edge: Edge) -> Option<Rect> {
    let band = match edge {
        Edge::Top => Rect { x0: courtyard.x0, x1: courtyard.x1, y0: courtyard.y0, y1: body.y0 },
        Edge::Bottom => Rect { x0: courtyard.x0, x1: courtyard.x1, y0: body.y1, y1: courtyard.y1 },
        Edge::Left => Rect { x0: courtyard.x0, x1: body.x0, y0: courtyard.y0, y1: courtyard.y1 },
        Edge::Right => Rect { x0: body.x1, x1: courtyard.x1, y0: courtyard.y0, y1: courtyard.y1 },
    };
    (band.w() > EPS && band.h() > EPS).then_some(band)
}

/// The edge `layout_note` prefers for note number `idx` of [`crate::lint::layout::FIELDS`] and whether to `keep`
/// it: Critical (0) the longer pair's first edge, Important (1) the side opposite the edge Critical took where it
/// found one, else the longer pair's second edge.
pub fn note_prefer(edges: [Edge; 2], idx: usize, critical: Option<Edge>) -> (Edge, bool) {
    match (idx, critical) {
        (0, _) => (edges[0], false),
        (_, Some(e)) => (opposite(e), true),
        (_, None) => (edges[1], false),
    }
}

/// The edge across the body from `edge`.
pub fn opposite(edge: Edge) -> Edge {
    match edge {
        Edge::Top => Edge::Bottom,
        Edge::Bottom => Edge::Top,
        Edge::Left => Edge::Right,
        Edge::Right => Edge::Left,
    }
}

/// Where a layout note's text goes, and the ink box it takes there: inside the real courtyard
/// ([`Shape::courtyard`]), clear of the fab drawing (the body, same as [`layout`]'s rules) and of every rect in
/// `avoid` ([`GAP`] kept free of each, as from any other drawing) — the designator's own ink as [`layout`] fits it,
/// and a note already placed on the same footprint, so [`crate::lint::layout::CRITICAL`] and
/// [`crate::lint::layout::IMPORTANT`] never overlap. All four edges are tried, the text level along a longer edge
/// and turned to read top to bottom at a shorter one wherever that gives larger text: the band
/// along a passive's long edge is thin, the ends beyond its pads have more room. The largest size wins; a tie
/// keeps `prefer`, its opposite, then the other pair's two ([`edge_order`]). With `keep` set `prefer` is taken as
/// soon as the note fits there at all (the opposite side of the other note), and the others only where it does
/// not. Where none of the four has room, the legacy centred placement instead ([`layout_with_floor`] down to
/// [`NOTE_TINY_SIZE`], `avoid` not counted there: a note alone on an edgeless body still just centres). The edge
/// is `None` on that fallback.
pub fn layout_note(
    shape: &Shape,
    values: &[String],
    prefer: Edge,
    keep: bool,
    avoid: &[Rect],
) -> (Placement, Rect, Option<Edge>) {
    if let Some(courtyard) = shape.courtyard {
        let extra: Vec<Prim> = avoid.iter().map(|r| Prim::Area(r.corners().to_vec())).collect();
        let mut best: Option<(Placement, Rect, Edge)> = None;
        for edge in edge_order(prefer) {
            let Some(band) = edge_band(courtyard, shape.body, edge) else { continue };
            let mut s = shape.clone();
            s.body = band;
            s.region = band;
            s.texts = values.to_vec();
            s.graphics.extend(extra.iter().cloned());
            let Some(p) = place(&s, &sizes()).or_else(|| place(&s, &tiny_sizes(NOTE_TINY_SIZE))) else { continue };
            let found = (p, s.text_rect(p.x, p.y, p.angle, p.size), edge);
            if keep && edge == prefer {
                best = Some(found);
                break;
            }
            if best.as_ref().is_none_or(|b| found.0.size > b.0.size + EPS) {
                best = Some(found);
            }
        }
        if let Some((p, ink, edge)) = best {
            return (p, ink, Some(edge));
        }
    }
    let mut s = shape.clone();
    s.texts = values.to_vec();
    let p = layout_with_floor(&s, NOTE_TINY_SIZE);
    let ink = s.text_rect(p.x, p.y, p.angle, p.size);
    (p, ink, None)
}

/// The designators a library footprint is sized for, from its name: the longest realistic one of its kind, five
/// characters.
pub fn library_refs(class: Class, name: &str) -> &'static [&'static str] {
    let name = name.rsplit(':').next().unwrap_or(name);
    match class {
        Class::Test => return &["TP526"],
        Class::Tie => return &["JP104", "NT104"],
        Class::Part | Class::Mech => {}
    }
    let starts = |p: &[&str]| p.iter().any(|p| name.starts_with(p));
    if starts(&["R_", "RN_", "R_Array"]) {
        &["R1004"]
    } else if starts(&["C_", "CP_"]) {
        &["C1001"]
    } else if starts(&["L_"]) {
        &["L1004"]
    } else if starts(&["D_", "LED_"]) {
        &["D1004"]
    } else if starts(&["Fuse"]) {
        &["F1004"]
    } else {
        &["U1004", "Q1004"]
    }
}

/// The values a real part's library footprint fits its Value for, from its kind: the longest typical value, six to
/// eight characters for most kinds (a board fits each footprint's own). Other classes keep their Value hidden.
pub fn library_values(class: Class, name: &str) -> &'static [&'static str] {
    let name = name.rsplit(':').next().unwrap_or(name);
    if class != Class::Part {
        return &[];
    }
    let starts = |p: &[&str]| p.iter().any(|p| name.starts_with(p));
    if starts(&["R_", "RN_", "R_Array"]) {
        &["100K", "4K75"]
    } else if starts(&["C_", "CP_"]) {
        &["100nF", "4.7uF"]
    } else if starts(&["L_"]) {
        &["4.7uH"]
    } else if starts(&["D_", "LED_"]) {
        &["BAS16TW"]
    } else if starts(&["Fuse"]) {
        &["500mA"]
    } else {
        &["TPS54202"]
    }
}

/// Where a real part's Value goes: fitted for `values` exactly like the designator ([`layout`]), centred on the body
/// and clear of the fab drawing. The designator is no obstacle: Ref.Des and Value are shown one at a time.
pub fn layout_value(shape: &Shape, values: &[String]) -> Placement {
    let mut s = shape.clone();
    s.texts = values.to_vec();
    layout(&s)
}

/// The `Reference` (or `Value`) property as the footprint has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub value: String,
    pub layer: String,
    pub hidden: bool,
    pub unlocked: bool,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub width: f64,
    pub height: f64,
    pub thickness: Option<f64>,
    pub justify: Vec<String>,
    pub uuid: Option<String>,
    pub span: Range<usize>,
}

/// A `${REFERENCE}` user text.
#[derive(Debug, Clone, PartialEq)]
pub struct RefText {
    pub layer: String,
    pub span: Range<usize>,
}

/// What [`read`] finds in a footprint file.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub back: bool,
    /// Where its designator lives ([`class_of`]).
    pub class: Class,
    pub reference: Option<Reference>,
    /// The `Value` property.
    pub value: Option<Reference>,
    /// The values a real part's Value is fitted for ([`library_values`]; on a board, its own).
    pub values: Vec<String>,
    pub ref_texts: Vec<RefText>,
    /// The stroke widths on F.Fab and B.Fab that are not [`FAB_STROKE`]: the `(width ...)` nodes.
    pub fab_widths: Vec<Range<usize>>,
    pub shape: Option<Shape>,
}

impl Footprint {
    pub fn fab_layer(&self) -> &'static str {
        if self.back { "B.Fab" } else { "F.Fab" }
    }

    /// The layer the designator belongs on ([`Class::layer`]).
    pub fn ref_layer(&self) -> &'static str {
        self.class.layer(self.back)
    }

    /// The Value is shown: a real part's is, on Value; every other class keeps it hidden on the fab layer.
    pub fn value_shown(&self) -> bool {
        self.class == Class::Part
    }

    /// The layer the Value belongs on: Value.F (Value.B for a bottom-side footprint) for a real part, else the fab
    /// layer, hidden.
    pub fn value_layer(&self) -> &'static str {
        if self.value_shown() { layers::side_of(layers::VALUE, self.back) } else { self.fab_layer() }
    }

    /// Where a real part's Value goes ([`layout_value`]); None for other classes.
    pub fn value_placement(&self) -> Option<Placement> {
        self.shape.as_ref().filter(|_| self.value_shown()).map(|s| layout_value(s, &self.values))
    }
}

fn num(n: Option<&Node>) -> Option<f64> {
    n.and_then(Node::as_atom).and_then(|s| s.parse().ok())
}

fn xy(n: &Node) -> Option<(f64, f64)> {
    Some((num(n.items().get(1))?, num(n.items().get(2))?))
}

/// A graphic item's `(width ...)` node.
fn width_node(g: &Node) -> Option<&Node> {
    g.child("stroke").and_then(|s| s.child("width")).or_else(|| g.child("width"))
}

/// A graphic item's line width.
fn stroke_width(g: &Node) -> f64 {
    width_node(g).and_then(|w| num(w.items().get(1))).unwrap_or(0.0)
}

fn filled(g: &Node) -> bool {
    g.child("fill")
        .is_some_and(|f| matches!(f.arg(0), Some("yes" | "solid")) || matches!(f.value("type"), Some("solid" | "yes")))
}

/// The circle through three points.
fn circle3(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> Option<((f64, f64), f64)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-12 {
        return None;
    }
    let (sa, sb, sc) = (a.0 * a.0 + a.1 * a.1, b.0 * b.0 + b.1 * b.1, c.0 * c.0 + c.1 * c.1);
    let ux = (sa * (b.1 - c.1) + sb * (c.1 - a.1) + sc * (a.1 - b.1)) / d;
    let uy = (sa * (c.0 - b.0) + sb * (a.0 - c.0) + sc * (b.0 - a.0)) / d;
    Some(((ux, uy), (a.0 - ux).hypot(a.1 - uy)))
}

/// The points along an arc from `a` through `m` to `b`.
fn arc_points(a: (f64, f64), m: (f64, f64), b: (f64, f64)) -> Vec<(f64, f64)> {
    let Some((c, r)) = circle3(a, m, b) else { return vec![a, b] };
    let ang = |p: (f64, f64)| (p.1 - c.1).atan2(p.0 - c.0);
    let a0 = ang(a);
    let mut sweep = (ang(b) - a0).rem_euclid(TAU);
    if (ang(m) - a0).rem_euclid(TAU) > sweep {
        sweep -= TAU;
    }
    (0..=ARC_SEGMENTS)
        .map(|i| {
            let t = a0 + sweep * i as f64 / ARC_SEGMENTS as f64;
            (c.0 + r * t.cos(), c.1 + r * t.sin())
        })
        .collect()
}

fn chain(pts: &[(f64, f64)], closed: bool, hw: f64) -> Vec<Prim> {
    let n = if closed { pts.len() } else { pts.len().saturating_sub(1) };
    (0..n).map(|i| Prim::Seg { a: pts[i], b: pts[(i + 1) % pts.len()], hw }).collect()
}

/// A graphic item as drawn with a stroke of `width`.
fn graphic_prims(g: &Node, width: f64) -> Vec<Prim> {
    let hw = width / 2.0;
    let p = |name: &str| g.child(name).and_then(xy);
    let mut out = Vec::new();
    match g.head().unwrap_or("") {
        "fp_line" => {
            if let (Some(a), Some(b)) = (p("start"), p("end")) {
                out.push(Prim::Seg { a, b, hw });
            }
        }
        "fp_rect" => {
            if let (Some(a), Some(b)) = (p("start"), p("end")) {
                let pts = [a, (b.0, a.1), b, (a.0, b.1)];
                out.extend(chain(&pts, true, hw));
                if filled(g) {
                    out.push(Prim::Area(pts.to_vec()));
                }
            }
        }
        "fp_circle" => {
            if let (Some(c), Some(e)) = (p("center"), p("end")) {
                let r = (e.0 - c.0).hypot(e.1 - c.1);
                out.push(if filled(g) { Prim::Ring { c, r: 0.0, hw: r + hw } } else { Prim::Ring { c, r, hw } });
            }
        }
        "fp_arc" => match (p("start"), p("mid"), p("end")) {
            (Some(a), Some(m), Some(b)) => out.extend(chain(&arc_points(a, m, b), false, hw)),
            (Some(a), None, Some(b)) => out.push(Prim::Seg { a, b, hw }),
            _ => {}
        },
        "fp_poly" | "fp_curve" => {
            let pts: Vec<_> = g.child("pts").map(|l| l.children("xy").filter_map(xy).collect()).unwrap_or_default();
            let closed = g.head() == Some("fp_poly");
            out.extend(chain(&pts, closed && pts.len() > 2, hw));
            if closed && pts.len() > 2 && filled(g) {
                out.push(Prim::Area(pts));
            }
        }
        _ => {}
    }
    out
}

const GRAPHICS: [&str; 6] = ["fp_line", "fp_rect", "fp_circle", "fp_arc", "fp_poly", "fp_curve"];

fn is_graphic(g: &Node) -> bool {
    g.head().is_some_and(|h| GRAPHICS.contains(&h))
}

fn layer_graphics<'a>(fp: &'a Node, layer: &'a str) -> impl Iterator<Item = &'a Node> {
    fp.items().iter().filter(move |g| is_graphic(g) && g.value("layer") == Some(layer))
}

fn prims_box(prims: &[Prim]) -> Option<Rect> {
    Rect::of_points(prims.iter().flat_map(Prim::points))
}

/// The box a visible user text on the fab layer takes, measured in KiCad's stroke font (centred, as most are).
fn text_area(t: &Node) -> Option<Prim> {
    let text = t.arg(1)?;
    let at = t.child("at")?;
    let (x, y) = xy(at)?;
    let a = num(at.items().get(3)).unwrap_or(0.0).to_radians();
    let (w, h, th) = font_size(t.child("effects"));
    let i = crate::stroke::ink(text, w, h, th.unwrap_or(0.15 * h))?;
    let (sin, cos) = a.sin_cos();
    let turn = |(u, v): (f64, f64)| (x + u * cos + v * sin, y - u * sin + v * cos);
    let pts = [(i.x0, i.y0), (i.x1, i.y0), (i.x1, i.y1), (i.x0, i.y1)];
    Some(Prim::Area(pts.into_iter().map(turn).collect()))
}

/// A pad's box: its size and drill, and a custom pad's primitives, turned by the pad's angle.
fn pad_rect(pad: &Node) -> Option<Rect> {
    let at = pad.child("at")?;
    let (px, py) = xy(at)?;
    let a = num(at.items().get(3)).unwrap_or(0.0).to_radians();
    let (sin, cos) = a.sin_cos();
    // KiCad turns counter-clockwise on screen, with y down.
    let turn = |(x, y): (f64, f64)| (px + x * cos + y * sin, py - x * sin + y * cos);
    let mut local = Vec::new();
    if let Some(size) = pad.child("size") {
        let (w, h) = xy(size)?;
        local.extend([(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)]);
    }
    if let Some(d) = pad.child("drill").and_then(|d| d.items().iter().skip(1).find_map(|n| num(Some(n)))) {
        local.extend([(-d / 2.0, -d / 2.0), (d / 2.0, d / 2.0)]);
    }
    if let Some(prims) = pad.child("primitives") {
        for g in prims.items().iter().skip(1) {
            local.extend(graphic_prims(g, stroke_width(g)).iter().flat_map(Prim::points));
        }
    }
    Rect::of_points(local.into_iter().map(turn))
}

fn font_size(effects: Option<&Node>) -> (f64, f64, Option<f64>) {
    let font = effects.and_then(|e| e.child("font"));
    let size = font.and_then(|f| f.child("size"));
    let (h, w) = size.and_then(xy).unwrap_or((1.27, 1.27));
    let t = font.and_then(|f| num(f.child("thickness").and_then(|t| t.items().get(1))));
    (w, h, t)
}

fn yes(n: &Node, name: &str) -> bool {
    n.child(name).is_some_and(|c| c.arg(0).is_none_or(|v| v == "yes"))
}

fn hidden(n: &Node) -> bool {
    yes(n, "hide") || n.child("effects").is_some_and(|e| yes(e, "hide"))
}

fn is_ref_text(n: &Node) -> bool {
    n.head() == Some("fp_text") && n.arg(0) == Some("user") && n.arg(1).is_some_and(|t| t.contains("${REFERENCE}"))
}

/// A property (`Reference`, `Value`, a field of the library's) as the footprint has it.
pub fn read_property(n: &Node) -> Reference {
    let at = n.child("at");
    let effects = n.child("effects");
    let (width, height, thickness) = font_size(effects);
    let justify = effects
        .and_then(|e| e.child("justify"))
        .map(|j| j.items().iter().skip(1).filter_map(Node::as_atom).map(String::from).collect())
        .unwrap_or_default();
    Reference {
        value: n.arg(1).unwrap_or("").to_string(),
        layer: n.value("layer").unwrap_or("").to_string(),
        hidden: hidden(n),
        unlocked: yes(n, "unlocked"),
        x: at.and_then(|a| num(a.items().get(1))).unwrap_or(0.0),
        y: at.and_then(|a| num(a.items().get(2))).unwrap_or(0.0),
        angle: at.and_then(|a| num(a.items().get(3))).unwrap_or(0.0),
        width,
        height,
        thickness,
        justify,
        uuid: n.value("uuid").map(String::from),
        span: n.span(),
    }
}

/// Reads what the rules look at from a `.kicad_mod` text.
pub fn read(text: &str) -> Result<Footprint, String> {
    read_as(text, "")
}

/// [`read`] for the footprint `id` (`Lib:Name`, whose library tells its [`Class`]).
pub fn read_as(text: &str, id: &str) -> Result<Footprint, String> {
    let fp = sexpr::parse(text).map_err(|e| e.to_string())?;
    if fp.head() != Some("footprint") {
        return Err("not a (footprint ...) file".into());
    }
    Ok(read_node(&fp, id, false))
}

/// What the rules look at in a footprint node. `board`: a footprint placed on a board, whose lines are measured
/// with their own strokes (a library footprint's with [`FAB_STROKE`], which [`fix`] gives them).
pub fn read_node(fp: &Node, id: &str, board: bool) -> Footprint {
    let name = fp.arg(0).unwrap_or("");
    let back = fp.value("layer") == Some("B.Cu");
    let mut reference = None;
    let mut ref_texts = Vec::new();
    let mut value = None;
    for n in fp.items() {
        match n.head() {
            Some("property") if n.arg(0) == Some("Reference") => reference = Some(read_property(n)),
            Some("property") if n.arg(0) == Some("Value") => value = Some(read_property(n)),
            _ if is_ref_text(n) => {
                ref_texts.push(RefText { layer: n.value("layer").unwrap_or("").to_string(), span: n.span() });
            }
            _ => {}
        }
    }
    let fab_widths = fp
        .items()
        .iter()
        .filter(|g| is_graphic(g) && matches!(g.value("layer"), Some("F.Fab" | "B.Fab")))
        .filter_map(width_node)
        .filter(|w| num(w.items().get(1)).is_none_or(|v| (v - FAB_STROKE).abs() > EPS))
        .map(Node::span)
        .collect();
    let fab = if back { "B.Fab" } else { "F.Fab" };
    let crtyd = if back { "B.CrtYd" } else { "F.CrtYd" };
    let refv = reference.as_ref().map_or("", |r: &Reference| r.value.as_str());
    let class = class_of(if id.is_empty() { name } else { id }, refv);
    let ref_layer = class.layer(back);
    let stroke = |g: &Node| if board { stroke_width(g).max(EPS) } else { FAB_STROKE };
    let lines: Vec<Prim> = layer_graphics(fp, fab).flat_map(|g| graphic_prims(g, stroke(g))).collect();
    // A reference layer with a drawing of its own: that drawing is the body, and the only drawing in the way. A
    // real part's Ref.Des has none: its body and the drawing in the way are the fab layer's.
    let ref_lines: Vec<Prim> = if ref_layer == fab || class == Class::Part {
        Vec::new()
    } else {
        layer_graphics(fp, ref_layer).flat_map(|g| graphic_prims(g, stroke_width(g))).collect()
    };
    let ref_stroke = layer_graphics(fp, ref_layer).map(stroke_width).fold(0.0, f64::max);
    let own = !ref_lines.is_empty();
    let mut graphics = if own { ref_lines.clone() } else { lines.clone() };
    let text_layers = if ref_layer == fab {
        vec![fab]
    } else if own {
        vec![ref_layer]
    } else {
        vec![fab, ref_layer]
    };
    if ref_layer != fab && !own {
        graphics.extend(layer_graphics(fp, ref_layer).flat_map(|g| graphic_prims(g, stroke_width(g))));
    }
    graphics.extend(
        fp.children("fp_text")
            .filter(|t| t.value("layer").is_some_and(|l| text_layers.contains(&l)) && !hidden(t) && !is_ref_text(t))
            .filter_map(text_area),
    );
    let pads: Vec<Rect> = fp.children("pad").filter_map(pad_rect).collect();
    // A fab drawing that leaves out the middle of the pads (only a pin-1 dot, say) is not the body.
    let pads_box = Rect::of_points(pads.iter().flat_map(|p| [(p.x0, p.y0), (p.x1, p.y1)]));
    let covers_pads =
        |b: &Rect| pads_box.is_none_or(|p| b.contains(&Rect::around(p.center().0, p.center().1, 0.0, 0.0)));
    let crty = {
        let g: Vec<&Node> = layer_graphics(fp, crtyd).collect();
        let stroke = g.iter().map(|g| stroke_width(g)).fold(0.0, f64::max);
        prims_box(&g.iter().flat_map(|g| graphic_prims(g, stroke_width(g))).collect::<Vec<_>>())
            .map(|body| (body, body.expand(-stroke)))
    };
    let shape = if let Some(body) = prims_box(&ref_lines) {
        Some((body, BodyFrom::RefLayer, body.expand(-ref_stroke)))
    } else if let Some(body) = prims_box(&lines).filter(covers_pads) {
        Some((body, BodyFrom::Fab, body))
    } else if let Some((body, region)) = crty {
        Some((body, BodyFrom::Courtyard, region))
    } else {
        pads_box.map(|body| (body, BodyFrom::Pads, body))
    };
    let texts = library_refs(class, name).iter().map(|t| t.to_string()).collect::<Vec<_>>();
    // A note hugs the courtyard's inside beyond the body; where the body already is the courtyard (no fab or
    // reference-layer drawing of its own) there is no room between the two, and a note falls back to the centre.
    let shape = shape.map(|(body, body_from, region)| Shape {
        body,
        body_from,
        region,
        graphics,
        back,
        texts: texts.clone(),
        drawn: [0.0, crate::reading::vertical(back)],
        courtyard: crty.map(|(_, inner)| inner).filter(|_| body_from != BodyFrom::Courtyard),
    });
    let values = library_values(class, name).iter().map(|t| t.to_string()).collect();
    Footprint { back, class, reference, value, values, ref_texts, fab_widths, shape }
}

fn is_silk(layer: &str) -> bool {
    layer == "F.SilkS" || layer == "B.SilkS"
}

/// Every rule the footprint breaks, in words; empty when it is fine.
pub fn check(fp: &Footprint) -> Vec<String> {
    let mut out = Vec::new();
    let Some(r) = &fp.reference else {
        return vec!["no Reference property".into()];
    };
    let fab = fp.fab_layer();
    let layer = fp.ref_layer();
    if r.layer != layer {
        out.push(format!("Reference on {}", r.layer));
    }
    if r.hidden && fp.class.shown() {
        out.push("Reference hidden".into());
    }
    if !r.hidden && !fp.class.shown() {
        out.push("Reference shown".into());
    }
    if !r.unlocked {
        out.push("Reference locked".into());
    }
    let silk = fp.ref_texts.iter().filter(|t| is_silk(&t.layer)).count();
    if silk > 0 {
        out.push(format!("{silk} ${{REFERENCE}} on silk"));
    }
    for l in if layer == fab { vec![fab] } else { vec![fab, layer] } {
        let n = fp.ref_texts.iter().filter(|t| t.layer == l).count();
        if n > 0 {
            out.push(format!("{n} ${{REFERENCE}} on {l} besides the Reference"));
        }
    }
    justified("Reference", r, fp.back, &mut out);
    if !fp.fab_widths.is_empty() {
        out.push(format!("{} fab lines not {FAB_STROKE} mm", fp.fab_widths.len()));
    }
    let placed = fp.shape.as_ref().filter(|_| fp.class.shown()).map(layout);
    if let Some(p) = &placed {
        misplaced("Reference", r, p, &mut out);
    }
    // A footprint without a Value has none to fit.
    match &fp.value {
        None => {}
        Some(v) => {
            if v.layer != fp.value_layer() {
                out.push(format!("Value on {}", v.layer));
            }
            if v.hidden && fp.value_shown() {
                out.push("Value hidden".into());
            }
            if !v.hidden && !fp.value_shown() {
                out.push("Value shown".into());
            }
            if !v.unlocked {
                out.push("Value locked".into());
            }
            justified("Value", v, fp.back, &mut out);
            if let Some(vp) = fp.value_placement() {
                misplaced("Value", v, &vp, &mut out);
            }
        }
    }
    out
}

/// A property's justification: none, mirrored on the back.
pub(crate) fn justified(name: &str, r: &Reference, back: bool, out: &mut Vec<String>) {
    let mirror = r.justify.iter().any(|j| j == "mirror");
    if r.justify.iter().any(|j| j != "mirror") || mirror != back {
        out.push(format!("{name} justified ({})", r.justify.join(" ")));
    }
}

/// A property not where, at the angle or the size `p` puts it.
pub(crate) fn misplaced(name: &str, r: &Reference, p: &Placement, out: &mut Vec<String>) {
    if (r.x - p.x).abs() > 5e-4 || (r.y - p.y).abs() > 5e-4 {
        out.push(format!("{name} at ({}, {}), not ({}, {})", fmt(r.x), fmt(r.y), fmt(p.x), fmt(p.y)));
    }
    if (r.angle - p.angle).rem_euclid(360.0).min((p.angle - r.angle).rem_euclid(360.0)) > EPS {
        out.push(format!("{name} at {}°, not {}°", fmt(r.angle), fmt(p.angle)));
    }
    if (r.width - p.size).abs() > EPS || (r.height - p.size).abs() > EPS {
        out.push(format!("{name} size {} x {}, not {}", fmt(r.width), fmt(r.height), fmt(p.size)));
    } else if r.thickness.is_none_or(|t| (t - thickness(p.size)).abs() > EPS) {
        out.push(format!("{name} thickness not {}", fmt(thickness(p.size))));
    }
}

/// KiCad's way of writing a number: no trailing zeros, no `-0`.
pub fn fmt(v: f64) -> String {
    let s = format!("{:.4}", (v * 10000.0).round() / 10000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// The property `name` (`Reference`, `Value`, a field) with `value` and `uuid` at `p`, visible unless `hide`,
/// unlocked, mirrored on the back, indented at `indent`.
#[allow(clippy::too_many_arguments)]
pub fn property_text(
    name: &str,
    value: &str,
    uuid: Option<&str>,
    layer: &str,
    back: bool,
    hide: bool,
    p: &Placement,
    indent: &str,
) -> String {
    let i1 = format!("{indent}\t");
    // KiCad writes a property's angle even when it is 0.
    let at = format!("(at {} {} {})", fmt(p.x), fmt(p.y), fmt(p.angle));
    let mut s = format!(
        "(property \"{name}\" {}\n{i1}{at}\n{i1}(unlocked yes)\n{i1}(layer {})\n",
        sexpr::quote(value),
        sexpr::quote(layer)
    );
    if hide {
        s += &format!("{i1}(hide yes)\n");
    }
    if let Some(u) = uuid {
        s += &format!("{i1}(uuid {})\n", sexpr::quote(u));
    }
    let size = fmt(p.size);
    s += &format!(
        "{i1}(effects\n{i1}\t(font\n{i1}\t\t(size {size} {size})\n{i1}\t\t(thickness {})\n{i1}\t)\n",
        fmt(thickness(p.size))
    );
    if back {
        s += &format!("{i1}\t(justify mirror)\n");
    }
    s += &format!("{i1})\n{indent})");
    s
}

/// The edit that hides the `Value` property `value` of `text` (`(hide yes)` after its layer, where KiCad writes
/// it); None when it is hidden already.
pub fn hide_value_edit(text: &str, value: &Node) -> Result<Option<Edit>, String> {
    if let Some(h) = value.child("hide") {
        if h.arg(0).is_none_or(|v| v == "yes") {
            return Ok(None);
        }
        return Ok(Some(Edit { range: h.span(), text: "(hide yes)".into() }));
    }
    if value.child("effects").is_some_and(|e| yes(e, "hide")) {
        return Ok(None);
    }
    let after = value.child("layer").or_else(|| value.child("at")).ok_or("Value without a layer")?.span();
    let start = text[..value.span().start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[start..value.span().start];
    let sep = if indent.trim().is_empty() { format!("\n{indent}\t") } else { " ".into() };
    Ok(Some(Edit { range: after.end..after.end, text: format!("{sep}(hide yes)") }))
}

/// The whole lines a node stands on, when nothing else shares them; else the node itself.
fn line_span(text: &str, span: &Range<usize>) -> Range<usize> {
    let start = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[span.end..].find('\n').map_or(text.len(), |i| span.end + i + 1);
    if text[start..span.start].trim().is_empty() && text[span.end..end].trim().is_empty() {
        start..end
    } else {
        span.clone()
    }
}

/// The designator as it was before a change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Before {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub size: f64,
}

/// What happens to one footprint.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Follows every rule; nothing to change.
    Fine,
    /// Broke `why`; rewritten with the designator at `placement` and the Value at `value`.
    Change {
        why: Vec<String>,
        before: Before,
        placement: Placement,
        value: Option<Placement>,
        /// What the body was read from; None for a mechanical footprint without one (its text stays where it was).
        body_from: Option<BodyFrom>,
        new: String,
    },
    /// Left alone.
    Skipped(String),
}

impl Outcome {
    /// Moved, resized, turned: what changed about the designator.
    pub fn designator_changes(&self) -> (bool, bool, bool) {
        match self {
            Outcome::Change { before: b, placement: p, .. } => (
                (b.x - p.x).abs() > 5e-4 || (b.y - p.y).abs() > 5e-4,
                (b.size - p.size).abs() > EPS,
                (b.angle - p.angle).rem_euclid(360.0).min((p.angle - b.angle).rem_euclid(360.0)) > EPS,
            ),
            _ => (false, false, false),
        }
    }
}

/// One footprint's [`Outcome`].
pub fn fix(text: &str) -> Outcome {
    fix_as(text, "")
}

/// [`fix`] for the footprint `id` (`Lib:Name`).
pub fn fix_as(text: &str, id: &str) -> Outcome {
    let fp = match read_as(text, id) {
        Ok(fp) => fp,
        Err(e) => return Outcome::Skipped(format!("does not parse: {e}")),
    };
    let why = check(&fp);
    if why.is_empty() {
        return Outcome::Fine;
    }
    let Some(r) = &fp.reference else { return Outcome::Skipped("no Reference property".into()) };
    // A mechanical footprint's designator only moves to its layer, hidden, where it was: it needs no body.
    let placement = match &fp.shape {
        _ if !fp.class.shown() => Placement { x: r.x, y: r.y, angle: r.angle, size: r.height, clean: true },
        Some(shape) => layout(shape),
        None => return Outcome::Skipped("no fab outline, courtyard or pads to place the designator by".into()),
    };
    let fab = fp.fab_layer();
    let layer = fp.ref_layer();
    let indent_of = |span: &Range<usize>| {
        let start = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
        let indent = &text[start..span.start];
        if indent.trim().is_empty() { indent } else { "\t" }
    };
    let mut edits = vec![Edit {
        range: r.span.clone(),
        text: property_text(
            "Reference",
            &r.value,
            r.uuid.as_deref(),
            layer,
            fp.back,
            !fp.class.shown(),
            &placement,
            indent_of(&r.span),
        ),
    }];
    // A real part's Value centred on its own layer; any other's hidden on the fab layer, where it was.
    let value = fp.value.as_ref().map(|v| {
        let vp = if let Some(shape) = fp.shape.as_ref().filter(|_| fp.value_shown()) {
            layout_value(shape, &fp.values)
        } else {
            Placement { x: v.x, y: v.y, angle: v.angle, size: v.height, clean: true }
        };
        let hide = !fp.value_shown();
        edits.push(Edit {
            range: v.span.clone(),
            text: property_text(
                "Value",
                &v.value,
                v.uuid.as_deref(),
                fp.value_layer(),
                fp.back,
                hide,
                &vp,
                indent_of(&v.span),
            ),
        });
        vp
    });
    for t in &fp.ref_texts {
        if is_silk(&t.layer) || t.layer == fab || t.layer == layer {
            edits.push(Edit { range: line_span(text, &t.span), text: String::new() });
        }
    }
    for w in &fp.fab_widths {
        edits.push(Edit { range: w.clone(), text: format!("(width {})", fmt(FAB_STROKE)) });
    }
    edits.sort_by_key(|e| e.range.start);
    let before = Before { x: r.x, y: r.y, angle: r.angle, size: r.height };
    let body_from = fp.shape.as_ref().map(|s| s.body_from);
    Outcome::Change { why, before, placement, value, body_from, new: apply_edits(text, &edits) }
}

/// One footprint file and its [`Outcome`].
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    /// `Library:Name`, from the `.pretty` folder and the file name.
    pub id: String,
    pub old: String,
    pub outcome: Outcome,
}

/// Every `.kicad_mod` under `paths` (files, or folders searched), sorted, each with its outcome.
pub fn plan(paths: &[PathBuf]) -> Vec<Item> {
    let mut files = Vec::new();
    for p in paths {
        collect(p, &mut files);
    }
    files.sort();
    files.dedup();
    files
        .into_iter()
        .map(|path| {
            let id = footprint_id(&path);
            match std::fs::read_to_string(&path) {
                Ok(old) => {
                    let outcome = fix_as(&old, &id);
                    Item { path, id, old, outcome }
                }
                Err(e) => Item { path, id, old: String::new(), outcome: Outcome::Skipped(format!("unreadable: {e}")) },
            }
        })
        .collect()
}

fn footprint_id(path: &Path) -> String {
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    match path.parent().and_then(Path::file_name).map(|d| d.to_string_lossy()) {
        Some(d) if d.ends_with(".pretty") => format!("{}:{name}", d.trim_end_matches(".pretty")),
        _ => name.to_string(),
    }
}

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_file() {
        if p.extension().is_some_and(|e| e == "kicad_mod") {
            out.push(p.to_path_buf());
        }
        return;
    }
    let Ok(rd) = std::fs::read_dir(p) else { return };
    for e in rd.flatten() {
        let path = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        collect(&path, out);
    }
}

/// Writes every changed footprint; refuses when a file changed since [`plan`] read it.
pub fn apply(items: &[Item]) -> Result<usize, String> {
    let changes: Vec<(&Item, &String)> = items
        .iter()
        .filter_map(|i| match &i.outcome {
            Outcome::Change { new, .. } => Some((i, new)),
            _ => None,
        })
        .collect();
    for (i, _) in &changes {
        let now = std::fs::read_to_string(&i.path).map_err(|e| format!("{}: {e}", i.path.display()))?;
        if now != i.old {
            return Err(format!("{} changed since it was read; run again", i.path.display()));
        }
    }
    for (i, new) in &changes {
        std::fs::write(&i.path, new).map_err(|e| format!("{}: {e}", i.path.display()))?;
    }
    Ok(changes.len())
}

/// What happens to one footprint's designator on a board.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardOutcome {
    /// Already where [`layout`] puts it.
    Fine(Placement),
    /// Moved, resized or turned to `placement` (angle in the footprint's frame); `angle` is written.
    Change { before: Before, placement: Placement, angle: f64 },
    /// Left alone.
    Skipped(String),
}

/// One footprint of a board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardItem {
    /// The designator (`R806`).
    pub reference: String,
    /// `Lib:Name`.
    pub id: String,
    pub outcome: BoardOutcome,
    /// The Value's, centred on its own layer (a real part's only).
    pub value: BoardOutcome,
    /// The layout notes that are set (`Layout_Critical`, `Layout_Important`), each fitted to its own text on its
    /// layer ([`crate::lint::layout`]); an empty one is not listed.
    pub notes: Vec<(&'static str, BoardOutcome)>,
    /// The pin numbers on the Pins layer, turned to read left to right or top to bottom ([`crate::lint::pins::board`]).
    pub pins: crate::lint::pins::BoardPins,
}

/// A board's designators fitted, each to its own text.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardPlan {
    pub items: Vec<BoardItem>,
    pub old: String,
    pub new: String,
}

/// Fits each footprint's own designator on a board (`.kicad_pcb` text): a footprint whose Reference already sits on
/// its designator layer (as [`fix`] leaves it in the library) gets the size, anchor and angle [`layout`] finds for
/// its actual text, so a short designator gets larger text; a real part's Value, when shown on the Value layer, gets
/// what [`layout_value`] finds for its actual text, centred the same way; a layout note that is set
/// (`Layout_Critical`, `Layout_Important`) gets what [`layout_note`] finds for its own text, hugging the
/// courtyard's edge, clear of the designator and (Important's) of Critical. Only these properties' `(at ...)`,
/// `(size ...)` and `(thickness ...)` change; the angle is written to read left to right or top to bottom ([`crate::reading`]). Other footprints are
/// skipped with the reason.
pub fn plan_board(text: &str) -> Result<BoardPlan, String> {
    let pcb = sexpr::parse(text).map_err(|e| e.to_string())?;
    if pcb.head() != Some("kicad_pcb") {
        return Err("not a (kicad_pcb ...) file".into());
    }
    let mut items = Vec::new();
    let mut edits = Vec::new();
    for node in pcb.children("footprint") {
        let id = node.arg(0).unwrap_or("").to_string();
        let fp = read_node(node, &id, true);
        let reference = fp.reference.as_ref().map(|r| r.value.clone()).unwrap_or_default();
        let mut run = |r: Result<(BoardOutcome, Vec<Edit>), String>| match r {
            Ok((outcome, e)) => {
                edits.extend(e);
                outcome
            }
            Err(why) => BoardOutcome::Skipped(why),
        };
        let outcome = run(board_edits(text, node, &fp));
        let value = run(board_value_edits(text, node, &fp));
        // The notes hug the courtyard's longer pair of edges, Critical then Important, each clear of the
        // designator's own ink and of the other once it is placed, so the two never overlap.
        let edges = fp.shape.as_ref().and_then(|s| s.courtyard).map(note_edges).unwrap_or([Edge::Top, Edge::Bottom]);
        let mut avoid: Vec<Rect> = board_designator(node, &fp)
            .ok()
            .map(|(shape, p, _)| shape.text_rect(p.x, p.y, p.angle, p.size))
            .into_iter()
            .collect();
        let mut notes = Vec::new();
        let mut critical_edge = None;
        for (idx, (name, pair)) in crate::lint::layout::FIELDS.into_iter().enumerate() {
            let Some(prop) = node.children("property").find(|n| n.arg(0) == Some(name)) else { continue };
            let r = read_property(prop);
            if r.value.trim().is_empty() {
                continue;
            }
            let prefer = note_prefer(edges, idx, critical_edge);
            match board_note_edits(text, node, &fp, (name, pair), &r, prefer, &avoid) {
                Ok((outcome, e, ink, edge)) => {
                    edits.extend(e);
                    avoid.push(ink);
                    if idx == 0 {
                        critical_edge = edge;
                    }
                    notes.push((name, outcome));
                }
                Err(why) => notes.push((name, BoardOutcome::Skipped(why))),
            }
        }
        let phi = node.child("at").and_then(|a| num(a.items().get(3))).unwrap_or(0.0);
        let (pins, e) = crate::lint::pins::board(node, fp.back, phi);
        edits.extend(e);
        items.push(BoardItem { reference, id, outcome, value, notes, pins });
    }
    edits.sort_by_key(|e| e.range.start);
    Ok(BoardPlan { items, old: text.to_string(), new: apply_edits(text, &edits) })
}

fn board_edits(text: &str, node: &Node, fp: &Footprint) -> Result<(BoardOutcome, Vec<Edit>), String> {
    let (_, p, phi) = board_designator(node, fp)?;
    let r = fp.reference.as_ref().expect("board_designator checked it");
    fit_property(text, node, "Reference", r, p, phi, fp.back)
}

/// A real part's Value fitted to its own text: skipped for other classes, and where the Value is not on its layer
/// shown (a footprint not updated from the library).
fn board_value_edits(text: &str, node: &Node, fp: &Footprint) -> Result<(BoardOutcome, Vec<Edit>), String> {
    if !fp.value_shown() {
        return Err("not a real part: its Value stays hidden".into());
    }
    let v = fp.value.as_ref().ok_or("no Value property")?;
    if v.layer != fp.value_layer() {
        return Err(format!("Value on {}, not {} (update the footprint from the library)", v.layer, fp.value_layer()));
    }
    if v.hidden {
        return Err("Value hidden".into());
    }
    if crate::stroke::ink(&v.value, 1.0, 1.0, 0.0).is_none() {
        return Err("empty Value".into());
    }
    let (mut shape, phi) = board_shape(node, fp)?;
    shape.texts = vec![v.value.clone()];
    let p = layout(&shape);
    fit_property(text, node, "Value", v, p, phi, fp.back)
}

/// A layout note fitted to its own text like the Value, hugging `prefer`'s edge of the courtyard, clear of `avoid`
/// (the designator's own ink, and the other note once it is placed). A real part's note that is elsewhere or
/// hidden (Update PCB from Schematic puts a field the footprint lacks hidden on the fab layer, and Update
/// Footprints from Library keeps it there) is moved to its layer, shown: the property is written anew. Other
/// classes' notes are left alone. Returns the note's own ink box and edge too, so the next note can avoid it.
fn board_note_edits(
    text: &str,
    node: &Node,
    fp: &Footprint,
    field: (&'static str, [&'static str; 2]),
    r: &Reference,
    (prefer, keep): (Edge, bool),
    avoid: &[Rect],
) -> Result<(BoardOutcome, Vec<Edit>, Rect, Option<Edge>), String> {
    let (name, pair) = field;
    let layer = layers::side_of(pair, fp.back);
    let (shape, phi) = board_shape(node, fp)?;
    let (p, ink, edge) = layout_note(&shape, std::slice::from_ref(&r.value), prefer, keep, avoid);
    if r.layer == layer && !r.hidden {
        let (outcome, edits) = fit_property(text, node, name, r, p, phi, fp.back)?;
        return Ok((outcome, edits, ink, edge));
    }
    if fp.class != Class::Part {
        return Err(format!("{name} on {}: not a real part, left alone", r.layer));
    }
    let angle = reading(phi + p.angle, fp.back);
    let start = text[..r.span.start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[start..r.span.start];
    let indent = if indent.trim().is_empty() { indent } else { "\t\t" };
    let at = Placement { angle, ..p };
    let new = property_text(name, &r.value, r.uuid.as_deref(), layer, fp.back, false, &at, indent);
    let before = Before { x: r.x, y: r.y, angle: r.angle, size: r.height };
    Ok((
        BoardOutcome::Change { before, placement: p, angle },
        vec![Edit { range: r.span.clone(), text: new }],
        ink,
        edge,
    ))
}

/// The footprint's shape as placed on the board, its text turned to read left to right or top to bottom, and its angle.
fn board_shape(node: &Node, fp: &Footprint) -> Result<(Shape, f64), String> {
    let mut shape = fp.shape.clone().ok_or("no fab outline, courtyard or pads to place the text by")?;
    let phi = node.child("at").and_then(|a| num(a.items().get(3))).unwrap_or(0.0);
    shape.drawn = [reading(phi, shape.back) - phi, reading(phi + 90.0, shape.back) - phi];
    Ok((shape, phi))
}

/// The shape fitted to the footprint's own designator on the board, where [`layout`] puts it, and the footprint's
/// angle.
fn board_designator(node: &Node, fp: &Footprint) -> Result<(Shape, Placement, f64), String> {
    let r = fp.reference.as_ref().ok_or("no Reference property")?;
    if r.layer != fp.ref_layer() {
        return Err(format!(
            "Reference on {}, not {} (update the footprint from the library)",
            r.layer,
            fp.ref_layer()
        ));
    }
    if !fp.class.shown() {
        return Err("mechanical: designator kept hidden".into());
    }
    if r.hidden {
        return Err("Reference hidden".into());
    }
    if crate::stroke::ink(&r.value, 1.0, 1.0, 0.0).is_none() {
        return Err("empty Reference".into());
    }
    let (mut shape, phi) = board_shape(node, fp)?;
    shape.texts = vec![r.value.clone()];
    let p = layout(&shape);
    Ok((shape, p, phi))
}

/// The edits that put property `name` (as read, `r`) at `p` on a footprint turned by `phi`: its `at` (the angle
/// written to read left to right or top to bottom), font size and thickness.
fn fit_property(
    text: &str,
    node: &Node,
    name: &str,
    r: &Reference,
    p: Placement,
    phi: f64,
    back: bool,
) -> Result<(BoardOutcome, Vec<Edit>), String> {
    let angle = reading(phi + p.angle, back);
    let before = Before { x: r.x, y: r.y, angle: r.angle, size: r.height };
    let same_angle = (r.angle - angle).rem_euclid(360.0).min((angle - r.angle).rem_euclid(360.0)) < 1e-3;
    let fine = (r.x - p.x).abs() < 5e-4
        && (r.y - p.y).abs() < 5e-4
        && same_angle
        && (r.width - p.size).abs() < EPS
        && (r.height - p.size).abs() < EPS
        && r.thickness.is_some_and(|t| (t - thickness(p.size)).abs() < EPS);
    if fine {
        return Ok((BoardOutcome::Fine(p), Vec::new()));
    }
    let prop = node.children("property").find(|n| n.arg(0) == Some(name)).expect("read_node found it");
    let at = prop.child("at").ok_or(format!("{name} without (at ...)"))?;
    let font = prop.child("effects").and_then(|e| e.child("font")).ok_or(format!("{name} without a font"))?;
    let size = font.child("size").ok_or(format!("{name} without a font size"))?;
    let mut edits = vec![
        Edit { range: at.span(), text: format!("(at {} {} {})", fmt(p.x), fmt(p.y), fmt(angle)) },
        Edit { range: size.span(), text: format!("(size {s} {s})", s = fmt(p.size)) },
    ];
    let t = format!("(thickness {})", fmt(thickness(p.size)));
    match font.child("thickness") {
        Some(n) => edits.push(Edit { range: n.span(), text: t }),
        None => {
            let start = text[..size.span().start].rfind('\n').map_or(0, |i| i + 1);
            let indent = &text[start..size.span().start];
            let sep = if indent.trim().is_empty() { format!("\n{indent}") } else { " ".into() };
            edits.push(Edit { range: size.span().end..size.span().end, text: format!("{sep}{t}") });
        }
    }
    Ok((BoardOutcome::Change { before, placement: p, angle }, edits))
}

/// Writes the board when [`plan_board`] changed it; refuses when the file changed since it was read.
pub fn apply_board(path: &Path, plan: &BoardPlan) -> Result<bool, String> {
    if plan.new == plan.old {
        return Ok(false);
    }
    let now = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if now != plan.old {
        return Err(format!("{} changed since it was read; run again", path.display()));
    }
    std::fs::write(path, &plan.new).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A footprint with a fab body `w` x `h` (stroke 0.1), the given pads (x, y, w, h), the Reference on
    /// `ref_layer`, `${REFERENCE}` user texts on `user_layers` and `extra` fab drawing.
    pub(crate) fn footprint(
        w: f64,
        h: f64,
        pads: &[(f64, f64, f64, f64)],
        ref_layer: &str,
        user_layers: &[&str],
    ) -> String {
        with_drawing(w, h, pads, ref_layer, user_layers, "")
    }

    fn with_drawing(
        w: f64,
        h: f64,
        pads: &[(f64, f64, f64, f64)],
        ref_layer: &str,
        user_layers: &[&str],
        extra: &str,
    ) -> String {
        let mut s = String::from("(footprint \"T\"\n\t(version 20260206)\n\t(layer \"F.Cu\")\n");
        s += &format!(
            "\t(property \"Reference\" \"REF**\"\n\t\t(at 0 -2 0)\n\t\t(layer \"{ref_layer}\")\n\t\t(uuid \"u-ref\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n"
        );
        s += "\t(property \"Value\" \"T\"\n\t\t(at 0 2 0)\n\t\t(layer \"F.Fab\")\n\t\t(uuid \"u-val\")\n\t)\n";
        for l in user_layers {
            s += &format!(
                "\t(fp_text user \"${{REFERENCE}}\"\n\t\t(at 0 0 0)\n\t\t(layer \"{l}\")\n\t\t(uuid \"u-{l}\")\n\t)\n"
            );
        }
        s += &format!(
            "\t(fp_rect\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n",
            -w / 2.0,
            -h / 2.0,
            w / 2.0,
            h / 2.0
        );
        s += extra;
        for (n, (x, y, pw, ph)) in pads.iter().enumerate() {
            s += &format!(
                "\t(pad \"{}\" smd rect\n\t\t(at {x} {y})\n\t\t(size {pw} {ph})\n\t\t(layers \"F.Cu\")\n\t)\n",
                n + 1
            );
        }
        s + ")\n"
    }

    #[test]
    fn a_reference_layer_drawing_is_the_body() {
        let circle = |r: f64| {
            format!(
                "\t(fp_circle\n\t\t(center 0.5 0)\n\t\t(end {} 0)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"User.9\")\n\t)\n",
                0.5 + r
            )
        };
        // A large test point circle beside the fab outline: the designator sits in the circle, the fab drawing ignored.
        let text = with_drawing(1.0, 1.0, &[(0.5, 0.0, 1.0, 1.0)], "User.9", &[], &circle(2.0));
        let Outcome::Change { placement: p, new, body_from, .. } = fix_as(&text, "VHRD_TestPoint:TestPoint_X") else {
            panic!()
        };
        assert_eq!(body_from, Some(BodyFrom::RefLayer));
        let fp = read_as(&new, "VHRD_TestPoint:TestPoint_X").unwrap();
        let shape = fp.shape.unwrap();
        let (cx, _) = shape.text_rect(p.x, p.y, p.angle, p.size).center();
        assert!(p.clean && (cx - 0.5).abs() < 2e-3 && p.size > 0.5, "{p:?}");
        assert_eq!(fix_as(&new, "VHRD_TestPoint:TestPoint_X"), Outcome::Fine);
        // A small one holds no text: the designator goes beside it, clear of the circle.
        let text = with_drawing(0.5, 0.5, &[(0.5, 0.0, 0.5, 0.5)], "User.9", &[], &circle(0.3));
        let Outcome::Change { placement: p, new, .. } = fix_as(&text, "VHRD_TestPoint:TestPoint_X") else { panic!() };
        let shape = read_as(&new, "VHRD_TestPoint:TestPoint_X").unwrap().shape.unwrap();
        let r = shape.text_rect(p.x, p.y, p.angle, p.size);
        let b = shape.body;
        assert!(p.clean && OUTSIDE_SIZES.contains(&p.size), "{p:?}");
        assert!(
            r.x1 <= b.x0 + 1e-9 || r.x0 >= b.x1 - 1e-9 || r.y1 <= b.y0 + 1e-9 || r.y0 >= b.y1 - 1e-9,
            "{r:?} {b:?}"
        );
        assert_eq!(fix_as(&new, "VHRD_TestPoint:TestPoint_X"), Outcome::Fine);
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> String {
        format!(
            "\t(fp_line\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.12)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n",
            a.0, a.1, b.0, b.1
        )
    }

    /// The ink's middle sits on the body's centre.
    fn centred(new: &str, p: &Placement) {
        let shape = read(new).unwrap().shape.unwrap();
        let (cx, cy) = shape.text_rect(p.x, p.y, p.angle, p.size).center();
        let (bx, by) = shape.centre();
        assert!((cx - bx).abs() < 1.1e-3 && (cy - by).abs() < 1.1e-3, "{p:?}: ink at ({cx}, {cy})");
    }

    fn changed(text: &str) -> (Placement, String) {
        match fix(text) {
            Outcome::Change { placement, new, .. } => (placement, new),
            o => panic!("expected a change, got {o:?}"),
        }
    }

    /// The rewritten footprint follows every rule, a second run changes nothing, and the text is clear of the
    /// drawing when the placement is clean.
    fn settled(new: &str, p: &Placement) -> Footprint {
        let fp = read(new).unwrap();
        assert_eq!(check(&fp), Vec::<String>::new(), "{new}");
        assert_eq!(fix(new), Outcome::Fine, "a second run changes nothing");
        let shape = fp.shape.as_ref().unwrap();
        if p.clean {
            let r = shape.text_rect(p.x, p.y, p.angle, p.size);
            assert!(shape.region.contains(&r), "{p:?} leaves {:?}", shape.region);
            assert!(!shape.graphics.iter().any(|g| g.hits(&r)), "{p:?} touches the drawing");
        }
        fp
    }

    #[test]
    fn soic_like_gets_1mm_turned_on_the_centre_with_silk_texts_gone() {
        // A 3.9 x 4.9 body, pads outside it left and right: 1 mm only fits along the 4.9 mm side.
        let pads: Vec<_> = (0..4)
            .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
            .collect();
        let text = footprint(3.9, 4.9, &pads, "F.SilkS", &["F.SilkS", "F.Fab"]);
        let (p, new) = changed(&text);
        assert_eq!((p.angle, p.size, p.clean), (270.0, 1.0, true), "{p:?}");
        centred(&new, &p);
        assert!(!new.contains("${REFERENCE}"), "{new}");
        assert!(new.contains("(layer \"User.23\")\n\t\t(uuid \"u-ref\")"), "{new}");
        assert!(new.contains("(property \"Value\" \"T\"\n\t\t(at "), "{new}");
        assert!(new.contains("(unlocked yes)\n\t\t(layer \"User.17\")\n\t\t(uuid \"u-val\")"), "{new}");
        assert!(new.contains("(width 0.05)") && !new.contains("(width 0.1)"), "{new}");
        settled(&new, &p);
    }

    #[test]
    fn pads_do_not_push_the_text_off_the_centre() {
        // 1206: the body lies within the pads; the text sits on its centre over them, level, 0.65 mm.
        let pads = [(-1.475, 0.0, 1.05, 1.75), (1.475, 0.0, 1.05, 1.75)];
        let (p, new) = changed(&footprint(3.2, 1.6, &pads, "F.SilkS", &["F.Fab"]));
        assert_eq!((p.angle, p.size, p.clean), (0.0, 0.65, true), "{p:?}");
        centred(&new, &p);
        settled(&new, &p);
    }

    #[test]
    fn a_mark_on_the_centre_moves_the_text_to_the_largest_free_spot() {
        // An 8 x 4 body with a diode bar across its middle: the text goes beside it, as large as it gets.
        let bar = line((0.0, -1.0), (0.0, 1.0));
        let (p, new) = changed(&with_drawing(8.0, 4.0, &[], "F.Fab", &[], &bar));
        assert!(p.clean && p.x != 0.0, "{p:?}");
        assert!(p.size >= 0.75, "{p:?}");
        let fp = settled(&new, &p);
        // Every line on the fab layer is 0.05 mm now; the obstacle was measured that way already.
        assert!(fp.fab_widths.is_empty());
        // Nothing larger fits anywhere, and nothing as large nearer the centre.
        let shape = fp.shape.unwrap();
        let bigger = p.size + SIZE_STEP;
        assert!(
            grid(shape.region, shape.centre())
                .iter()
                .all(|&(x, y)| !shape.fits(x, y, 0.0, bigger) && !shape.fits(x, y, 90.0, bigger))
        );
    }

    #[test]
    fn a_mark_shrinks_or_turns_the_text_before_it_moves() {
        // A band line near the left end of a 6 x 3 body leaves the centre free at a smaller size.
        let mark = line((-2.0, -1.5), (-2.0, 1.5));
        let (p, new) = changed(&with_drawing(6.0, 3.0, &[], "F.Fab", &[], &mark));
        assert_eq!((p.angle, p.size, p.clean), (0.0, 0.8, true), "{p:?}");
        centred(&with_drawing(6.0, 3.0, &[], "F.Fab", &[], &mark), &p);
        settled(&new, &p);
        // A tall body turns it.
        let (p, new) = changed(&footprint(1.4, 4.0, &[], "F.Fab", &[]));
        assert_eq!(p.angle, 270.0, "{p:?}");
        centred(&new, &p);
        settled(&new, &p);
    }

    #[test]
    fn circles_arcs_and_filled_polygons_count() {
        let circle = "\t(fp_circle\n\t\t(center 0 0)\n\t\t(end 0.5 0)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill yes)\n\t\t(layer \"F.Fab\")\n\t)\n";
        let fp = read(&with_drawing(6.0, 6.0, &[], "F.Fab", &[], circle)).unwrap();
        let shape = fp.shape.unwrap();
        assert!(!shape.fits(0.0, 0.0, 0.0, MIN_SIZE), "a filled dot on the centre");
        let p = layout(&shape);
        assert!(p.clean && (p.x, p.y) != (0.0, 0.0), "{p:?}");

        let arc = "\t(fp_arc\n\t\t(start -1 0)\n\t\t(mid 0 -1)\n\t\t(end 1 0)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n";
        let shape = read(&with_drawing(6.0, 6.0, &[], "F.Fab", &[], arc)).unwrap().shape.unwrap();
        // The arc's top at (0, -1): text there is hit, the empty lower half of its circle is free.
        assert!(!shape.fits(0.0, -1.0, 0.0, MIN_SIZE));
        assert!(shape.fits(0.0, 0.5, 0.0, MIN_SIZE));

        let tri = "\t(fp_poly\n\t\t(pts\n\t\t\t(xy -1 -1) (xy 1 -1) (xy 0 1)\n\t\t)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill yes)\n\t\t(layer \"F.Fab\")\n\t)\n";
        let shape = read(&with_drawing(6.0, 6.0, &[], "F.Fab", &[], tri)).unwrap().shape.unwrap();
        assert!(Prim::Area(vec![(-1.0, -1.0), (1.0, -1.0), (0.0, 1.0)]).hits(&Rect::around(0.0, -0.2, 0.05, 0.05)));
        assert!(!shape.fits(0.0, -0.3, 0.0, MIN_SIZE), "inside a filled triangle");
    }

    #[test]
    fn small_bodies_get_small_text_and_too_small_ones_stay_centred_not_clean() {
        // 0402 (1 x 0.5): R1004 fits inside the outline at 0.19, below the usual floor.
        let pads = [(-0.48, 0.0, 0.56, 0.62), (0.48, 0.0, 0.56, 0.62)];
        let r0402 = footprint(1.0, 0.5, &pads, "F.SilkS", &["F.Fab"]).replace("\"T\"", "\"R_0402\"");
        let (p, new) = changed(&r0402);
        assert_eq!((p.angle, p.size, p.clean, p.tiny()), (0.0, 0.19, true, true), "{p:?}");
        centred(&new, &p);
        settled(&new, &p);
        assert!(new.contains("(size 0.19 0.19)\n\t\t\t\t(thickness 0.028)"), "{new}");
        // A 0.6 x 0.3 body (0201): nothing fits even at the tiny floor.
        let (p, new) = changed(&footprint(0.6, 0.3, &[], "F.SilkS", &[]));
        assert_eq!((p.angle, p.size, p.clean), (0.0, MIN_SIZE, false), "{p:?}");
        centred(&new, &p);
        settled(&new, &p);
    }

    #[test]
    fn a_layout_note_fits_much_smaller_than_a_designator_would() {
        // The same 0.6 x 0.3 body, no courtyard drawn: the note falls back to the centre, like a designator.
        let shape = read(&footprint(0.6, 0.3, &[], "F.SilkS", &[])).unwrap().shape.unwrap();
        assert!(shape.courtyard.is_none());
        let designator = layout(&shape);
        assert_eq!((designator.size, designator.clean), (MIN_SIZE, false), "{designator:?}");
        // ...but a short note reads fine well below that floor, so it gets a clean fit instead.
        let (note, _, edge) = layout_note(&shape, &["FB U301".to_string()], Edge::Top, false, &[]);
        assert!(note.clean && note.size < TINY_SIZE && edge.is_none(), "{note:?} {edge:?}");
        assert!(note.size >= NOTE_TINY_SIZE - EPS, "{note:?}");
        // A note too long even for the note floor still ends up not clean, like a designator.
        let (huge, _, _) = layout_note(
            &shape,
            &["a very much longer note than any body this small could ever hold".to_string()],
            Edge::Top,
            false,
            &[],
        );
        assert_eq!((huge.size, huge.clean), (MIN_SIZE, false), "{huge:?}");
    }

    /// A courtyard rect round `body` with `margin` on every side, 0.05 mm stroke, on F.CrtYd (B.CrtYd when `back`).
    fn crtyd_rect(w: f64, h: f64, margin: f64, back: bool) -> String {
        let layer = if back { "B.CrtYd" } else { "F.CrtYd" };
        format!(
            "\t(fp_rect\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"{layer}\")\n\t)\n",
            -w / 2.0 - margin,
            -h / 2.0 - margin,
            w / 2.0 + margin,
            h / 2.0 + margin
        )
    }

    #[test]
    fn a_layout_note_hugs_the_courtyard_clear_of_the_designator() {
        // A 4 x 6 body (taller than wide: left/right are the longer pair) with a 0.25 mm courtyard margin.
        let extra = crtyd_rect(4.0, 6.0, 0.25, false);
        let text = with_drawing(4.0, 6.0, &[], "F.Fab", &[], &extra);
        let shape = read(&text).unwrap().shape.unwrap();
        let courtyard = shape.courtyard.expect("a courtyard was drawn");
        assert_eq!(note_edges(courtyard), [Edge::Left, Edge::Right]);
        let designator = layout(&shape);
        let designator_ink = shape.text_rect(designator.x, designator.y, designator.angle, designator.size);

        let (critical, crit_ink, edge) =
            layout_note(&shape, &["U101.5 VDD".to_string()], Edge::Left, false, &[designator_ink]);
        assert_eq!(edge, Some(Edge::Left), "{critical:?}");
        assert!(critical.clean, "{critical:?}");
        // Left of the body, inside the courtyard, clear of the designator's ink.
        assert!(crit_ink.x1 <= shape.body.x0 + 1e-9, "{crit_ink:?} body {:?}", shape.body);
        assert!(crit_ink.x0 >= courtyard.x0 - 1e-9, "{crit_ink:?} courtyard {courtyard:?}");
        assert!(!crit_ink.overlaps(&designator_ink), "{crit_ink:?} {designator_ink:?}");

        // Important avoids both the designator and Critical's ink: it lands on the opposite (Right) edge.
        let (important, imp_ink, edge) =
            layout_note(&shape, &["FB U301".to_string()], Edge::Right, true, &[designator_ink, crit_ink]);
        assert_eq!(edge, Some(Edge::Right), "{important:?}");
        assert!(important.clean, "{important:?}");
        assert!(imp_ink.x0 >= shape.body.x1 - 1e-9, "{imp_ink:?} body {:?}", shape.body);
        assert!(!imp_ink.overlaps(&crit_ink) && !imp_ink.overlaps(&designator_ink), "{imp_ink:?} {crit_ink:?}");
    }

    #[test]
    fn a_passives_notes_turn_to_the_ends_where_there_is_more_room() {
        // 0402: a 1.0 x 0.5 body, courtyard 0.43 beyond it at the ends and 0.22 along the long edges.
        let extra = "\t(fp_rect\n\t\t(start -0.93 -0.47)\n\t\t(end 0.93 0.47)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"F.CrtYd\")\n\t)\n";
        let text = with_drawing(1.0, 0.5, &[], "F.Fab", &[], extra);
        let shape = read(&text).unwrap().shape.unwrap();
        let note = ["U1001.48".to_string()];
        let (crit, crit_ink, edge) = layout_note(&shape, &note, Edge::Top, false, &[]);
        assert!(matches!(edge, Some(Edge::Left | Edge::Right)), "{crit:?} {edge:?}");
        assert_eq!(crit.angle, 270.0, "{crit:?}");
        // Along the long edge the same note got only 0.08 mm.
        assert!(crit.size > 0.11, "{crit:?}");
        // Important goes to the opposite end and never touches Critical.
        let (prefer, keep) = note_prefer([Edge::Top, Edge::Bottom], 1, edge);
        let (imp, imp_ink, imp_edge) = layout_note(&shape, &note, prefer, keep, &[crit_ink]);
        assert_eq!(imp_edge, edge.map(opposite), "{imp:?}");
        assert!(!imp_ink.overlaps(&crit_ink), "{imp_ink:?} {crit_ink:?}");
        assert!((imp.size - crit.size).abs() < 1e-9, "{imp:?} {crit:?}");
    }

    #[test]
    fn a_courtyard_with_no_room_beside_the_body_falls_back_to_the_centre() {
        // The courtyard clears the body by less than a floor-sized note needs on any side.
        let extra = crtyd_rect(4.0, 4.0, 0.01, false);
        let text = with_drawing(4.0, 4.0, &[], "F.Fab", &[], &extra);
        let shape = read(&text).unwrap().shape.unwrap();
        assert!(shape.courtyard.is_some(), "a courtyard was drawn, just too tight to use");
        let (p, ink, edge) = layout_note(&shape, &["VDDA33".to_string()], Edge::Top, false, &[]);
        assert!(edge.is_none(), "{p:?}");
        let (cx, cy) = ink.center();
        let (bx, by) = shape.centre();
        assert!((cx - bx).abs() < 1.1e-3 && (cy - by).abs() < 1.1e-3, "{p:?}: ink at ({cx}, {cy})");
    }

    #[test]
    fn the_library_text_is_sized_for_the_longest_designator_of_the_kind() {
        assert_eq!(library_refs(Class::Part, "R_0402_1005Metric"), ["R1004"]);
        assert_eq!(library_refs(Class::Part, "VHRD_Capacitor_SMD:C_0603_1608Metric"), ["C1001"]);
        assert_eq!(library_refs(Class::Test, "TestPoint_Pad_D1.0mm"), ["TP526"]);
        assert_eq!(library_refs(Class::Tie, "SPARK_GAP_011"), ["JP104", "NT104"]);
        assert_eq!(library_refs(Class::Part, "SOIC-8_3.9x4.9mm_P1.27mm"), ["U1004", "Q1004"]);
    }

    #[test]
    fn test_points_net_ties_and_mechanical_parts_get_their_own_layers() {
        // A test point: its designator on Ref.Test (User.9), fitted like any other.
        let (p, new) = changed(&footprint(3.0, 3.0, &[], "F.SilkS", &["F.Fab"]).replace("\"T\"", "\"TestPoint_X\""));
        assert!(new.contains("(unlocked yes)\n\t\t(layer \"User.9\")") && !new.contains("${REFERENCE}"), "{new}");
        assert!(p.clean && p.size > 0.5, "{p:?}");
        assert_eq!(fix_as(&new, "VHRD_TestPoint:TestPoint_X"), Outcome::Fine);
        // A net tie of the VHRD_NetTie library, named otherwise: on Ref.Tie (User.11), the fab drawing in the way.
        let text = with_drawing(3.0, 1.0, &[], "F.SilkS", &[], &line((0.0, -0.5), (0.0, 0.5)));
        let Outcome::Change { placement: p, new, .. } = fix_as(&text, "VHRD_NetTie:T") else { panic!() };
        assert!(new.contains("(layer \"User.11\")"), "{new}");
        let fp = read_as(&new, "VHRD_NetTie:T").unwrap();
        assert!(check(&fp).is_empty(), "{:?}", check(&fp));
        let shape = fp.shape.unwrap();
        assert!(!shape.graphics.iter().any(|g| g.hits(&shape.text_rect(p.x, p.y, p.angle, p.size))));
        // A logo: on Ref.Mech (User.13), hidden, where it was.
        let text = footprint(3.0, 3.0, &[], "F.SilkS", &[]);
        let Outcome::Change { new, .. } = fix_as(&text, "VHRD_Symbol:LOGO") else { panic!() };
        assert!(new.contains("(at 0 -2 0)\n\t\t(unlocked yes)\n\t\t(layer \"User.13\")\n\t\t(hide yes)\n"), "{new}");
        assert_eq!(fix_as(&new, "VHRD_Symbol:LOGO"), Outcome::Fine);
        // A logo drawn on the silkscreen alone has no body: its texts still move, hidden, where they were.
        let bare = format!("{})\n", text.split("\t(fp_rect").next().unwrap());
        let Outcome::Change { new, body_from: None, .. } = fix_as(&bare, "VHRD_Symbol:LOGO") else { panic!() };
        assert!(
            new.contains("(layer \"User.13\")\n\t\t(hide yes)") && new.contains("(layer \"F.Fab\")\n\t\t(hide yes)")
        );
        assert_eq!(fix_as(&new, "VHRD_Symbol:LOGO"), Outcome::Fine);
        // On the back, the back layer of the pair.
        let back = footprint(3.0, 3.0, &[], "F.SilkS", &[])
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
            .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
        let o = fix_as(&back, "VHRD_TestPoint:T");
        let Outcome::Change { new, .. } = &o else { panic!("{o:?}") };
        assert!(new.contains("(layer \"User.10\")") && new.contains("(justify mirror)"), "{new}");
    }

    #[test]
    fn a_reference_off_the_centre_is_moved_back() {
        let pads = [(-3.0, 0.0, 1.2, 1.2), (3.0, 0.0, 1.2, 1.2)];
        let (p, new) = changed(&footprint(4.0, 2.0, &pads, "F.Fab", &[]));
        let at = format!("(at {} {} {})", fmt(p.x), fmt(p.y), fmt(p.angle));
        assert!(new.contains(&at), "{new}");
        let moved = new.replace(&at, "(at 0.2 0.3 0)");
        let Outcome::Change { why, before, .. } = fix(&moved) else { panic!("{moved}") };
        assert!(why.iter().any(|w| w.contains(&format!("not ({}, {})", fmt(p.x), fmt(p.y)))), "{why:?}");
        assert_eq!((before.x, before.y), (0.2, 0.3));
        let o = fix(&moved);
        assert_eq!(o.designator_changes(), (true, false, false));
    }

    #[test]
    fn a_bottom_footprint_gets_ref_des_b_and_mirrored_text() {
        let text = footprint(4.0, 2.0, &[], "B.SilkS", &[])
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
            .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
        let (p, new) = changed(&text);
        assert!(new.contains("(layer \"User.24\")") && new.contains("(justify mirror)"), "{new}");
        settled(&new, &p);
    }

    #[test]
    fn a_pin_1_dot_alone_is_not_the_body() {
        let pads = [(-2.0, 0.0, 1.0, 0.6), (2.0, 0.0, 1.0, 0.6)];
        let text = footprint(0.2, 0.2, &pads, "F.SilkS", &[])
            .replace("(start -0.1 -0.1)\n\t\t(end 0.1 0.1)", "(start -2.6 -0.6)\n\t\t(end -2.4 -0.4)");
        let fp = read(&text).unwrap();
        assert_eq!(fp.shape.as_ref().unwrap().body_from, BodyFrom::Pads);
    }

    #[test]
    fn turned_pads_and_formatting() {
        let n = sexpr::parse("(pad \"1\" smd rect (at 1 2 90) (size 2 0.5))").unwrap();
        let r = pad_rect(&n).unwrap();
        assert!((r.w() - 0.5).abs() < 1e-9 && (r.h() - 2.0).abs() < 1e-9, "{r:?}");
        assert_eq!(
            (fmt(0.30000001), fmt(-0.0), fmt(1.0), fmt(-1.25)),
            ("0.3".into(), "0".into(), "1".into(), "-1.25".into())
        );
        assert_eq!((thickness(1.0), thickness(0.3), thickness(0.22), thickness(0.15)), (0.15, 0.045, 0.033, 0.025));
        let s = sizes();
        assert_eq!((s.len(), s[13], s[14], s[15], s.last()), (25, 0.35, 0.3, 0.29, Some(&MIN_SIZE)));
        assert_eq!(tiny_sizes(TINY_SIZE), vec![0.19, 0.18, 0.17, 0.16, 0.15]);
        assert_eq!(tiny_sizes(NOTE_TINY_SIZE).len(), 15);
        assert_eq!(tiny_sizes(NOTE_TINY_SIZE).last(), Some(&NOTE_TINY_SIZE));
    }

    #[test]
    fn a_real_parts_value_is_centred_on_its_own_layer_and_the_fab_layer_keeps_no_text() {
        // A SOIC-like body: the designator on Ref.Des, the Value on Value, each centred, each clear of the drawing.
        let pads: Vec<_> = (0..4)
            .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
            .collect();
        let (p, new) = changed(&footprint(3.9, 4.9, &pads, "F.Fab", &["F.Fab"]));
        let fp = settled(&new, &p);
        assert_eq!(fp.reference.as_ref().unwrap().layer, "User.23");
        let v = fp.value.as_ref().unwrap();
        assert!(v.layer == "User.17" && !v.hidden && v.unlocked, "{v:?}");
        assert!(!new.contains("(layer \"F.Fab\")\n\t\t(uuid") && !new.contains("${REFERENCE}"), "{new}");
        assert_eq!(fp.values, ["TPS54202"]);
        let shape = fp.shape.as_ref().unwrap();
        let vp = fp.value_placement().unwrap();
        let mut vs = shape.clone();
        vs.texts = fp.values.clone();
        let vr = vs.text_rect(vp.x, vp.y, vp.angle, vp.size);
        let ((cx, cy), (bx, by)) = (vr.center(), shape.centre());
        assert!(vp.clean && (cx - bx).abs() < 1.1e-3 && (cy - by).abs() < 1.1e-3, "{vp:?}");
        assert!(shape.region.contains(&vr) && !shape.graphics.iter().any(|g| g.hits(&vr)));
        // A value longer than the designator gets smaller text, at the same angle on this body.
        assert!(vp.size < p.size && vp.angle == p.angle, "{p:?} {vp:?}");
        // On the back: Ref.Des.B and Value.B, mirrored.
        let back = footprint(4.0, 2.0, &[], "F.Fab", &[])
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
            .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
        let (_, new) = changed(&back);
        let fp = read(&new).unwrap();
        let (r, v) = (fp.reference.unwrap(), fp.value.unwrap());
        assert!(r.layer == "User.24" && v.layer == "User.18" && v.justify == ["mirror"], "{v:?}");
        // A logo's and a test point's Value stay hidden on the fab layer, where they were.
        for id in ["VHRD_Symbol:LOGO", "VHRD_TestPoint:TP"] {
            let Outcome::Change { new, .. } = fix_as(&footprint(3.0, 3.0, &[], "F.SilkS", &[]), id) else { panic!() };
            assert!(
                new.contains(
                    "(property \"Value\" \"T\"\n\t\t(at 0 2 0)\n\t\t(unlocked yes)\n\t\t(layer \"F.Fab\")\n\t\t(hide yes)"
                ),
                "{new}"
            );
            assert_eq!(fix_as(&new, id), Outcome::Fine);
        }
    }

    #[test]
    fn a_board_fits_each_value_to_its_own_text() {
        let pads: Vec<_> = (0..4)
            .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
            .collect();
        let (_, lib) = changed(&footprint(3.9, 4.9, &pads, "F.Fab", &[]));
        let placed = lib
            .replacen("(footprint \"T\"", "(footprint \"Lib:T\"\n\t(at 10 10 90)", 1)
            .replace("\"REF**\"", "\"U1\"")
            .replace("(property \"Value\" \"T\"", "(property \"Value\" \"TPS54202DDCR\"");
        let board = format!("(kicad_pcb\n{placed})\n");
        let plan = plan_board(&board).unwrap();
        let item = &plan.items[0];
        let (BoardOutcome::Change { placement: p, .. }, BoardOutcome::Change { placement: vp, angle, .. }) =
            (&item.outcome, &item.value)
        else {
            panic!("{item:?}")
        };
        // Both centred on the body, each as large as its own text allows.
        assert!(p.clean && vp.clean && vp.size < p.size, "{p:?} {vp:?}");
        assert!(*angle == 0.0 || *angle == 270.0, "{angle}");
        let again = plan_board(&plan.new).unwrap();
        assert_eq!(again.new, plan.new, "a second run changes nothing");
        assert!(matches!(again.items[0].value, BoardOutcome::Fine(_)));
        // A footprint not updated from the library: its value on F.Fab, hidden, is left alone.
        let old = board.replace("(layer \"User.17\")", "(layer \"F.Fab\")");
        let plan = plan_board(&old).unwrap();
        assert!(matches!(&plan.items[0].value, BoardOutcome::Skipped(w) if w.contains("update the footprint")));
        // A test point's Value is not fitted.
        let (_, tp) = changed(&footprint(3.0, 3.0, &[], "F.Fab", &[]).replace("\"T\"", "\"TestPoint_X\""));
        let tp = tp.replacen("(footprint \"TestPoint_X\"", "(footprint \"VHRD_TestPoint:TestPoint_X\"", 1);
        let plan = plan_board(&format!("(kicad_pcb\n{})\n", tp.replace("\"REF**\"", "\"TP1\""))).unwrap();
        assert!(matches!(&plan.items[0].outcome, BoardOutcome::Change { .. }), "{:?}", plan.items[0]);
        assert!(matches!(&plan.items[0].value, BoardOutcome::Skipped(w) if w.contains("not a real part")));
    }

    #[test]
    fn a_board_moves_a_set_layout_note_kicad_left_on_the_fab_layer() {
        let pads = [(-1.0, 0.0, 0.6, 0.6), (1.0, 0.0, 0.6, 0.6)];
        let (_, lib) = changed(&footprint(3.0, 1.6, &pads, "F.Fab", &[]));
        // As Update PCB from Schematic adds a field the footprint lacks: hidden on F.Fab at the origin.
        let note = "\t(property \"Layout_Critical\" \"U1.5\"\n\t\t(at 10 10 90)\n\t\t(layer \"F.Fab\")\n\t\t(hide yes)\n\t\t(uuid \"u-crit\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1.27 1.27)\n\t\t\t)\n\t\t)\n\t)\n";
        let placed = lib
            .replacen("(footprint \"T\"", "(footprint \"Lib:T\"\n\t(at 10 10 90)", 1)
            .replace("\"REF**\"", "\"C1\"")
            .replacen("\t(fp_rect", &format!("{note}\t(fp_rect"), 1);
        let board = format!("(kicad_pcb\n{placed})\n");
        let plan = plan_board(&board).unwrap();
        let [(name, BoardOutcome::Change { placement: p, angle, .. })] = plan.items[0].notes.as_slice() else {
            panic!("{:?}", plan.items[0])
        };
        assert_eq!((*name, *angle), ("Layout_Critical", 270.0));
        assert!(
            plan.new.contains(&format!(
                "(property \"Layout_Critical\" \"U1.5\"\n\t\t(at {} {} 270)\n\t\t(unlocked yes)\n\t\t(layer \"User.25\")\n\t\t(uuid \"u-crit\")",
                fmt(p.x),
                fmt(p.y)
            )),
            "{}",
            plan.new
        );
        let again = plan_board(&plan.new).unwrap();
        assert_eq!(again.new, plan.new, "a second run changes nothing");
        // An empty note is not looked at.
        let empty = plan_board(&board.replace("\"U1.5\"", "\"\"")).unwrap();
        assert!(empty.items[0].notes.is_empty());
    }

    #[test]
    fn every_designator_reads_left_to_right_or_top_to_bottom_on_each_side() {
        let (_, lib) = changed(&footprint(3.9, 4.9, &[], "F.Fab", &[]));
        for (side, vertical) in [("F.Cu", 270.0), ("B.Cu", 90.0)] {
            for phi in [0.0, 90.0, 180.0, 270.0, -90.0] {
                let placed = lib
                    .replacen(
                        "(footprint \"T\"",
                        &format!("(footprint \"Lib:T\"\n\t(layer \"{side}\")\n\t(at 10 10 {phi})"),
                        1,
                    )
                    .replace("\"REF**\"", "\"U1\"");
                let placed = if side == "B.Cu" {
                    placed.replace("User.23", "User.24").replace("User.17", "User.18").replace("F.Fab", "B.Fab")
                } else {
                    placed
                };
                let plan = plan_board(&format!("(kicad_pcb\n{placed})\n")).unwrap();
                let (BoardOutcome::Change { angle, .. } | BoardOutcome::Fine(Placement { angle, .. })) =
                    plan.items[0].outcome
                else {
                    panic!("{:?}", plan.items[0])
                };
                assert!(angle == 0.0 || angle == vertical, "{side} part at {phi}: {angle}");
            }
        }
    }
}
