//! Footprints whose designator lives only on the fab layer: the `Reference` property on F.Fab (B.Fab for a
//! bottom-side footprint), visible and unlocked, no `${REFERENCE}` text on the silkscreen and no second one on the
//! fab layer, the Value hidden. The designator sits on the centre of the body (the fab outline's, else the
//! courtyard's): KiCad draws the selected fab layer on top of the pads, so text over the pads reads, while text
//! outside the body runs into the neighbouring parts. It never touches the other drawing on its fab layer (the
//! outline, polarity and pin-1 marks, diode symbols, inner lines), the strokes of the lines and of the text
//! counted. To fit, it gets smaller (from [`MAX_SIZE`] down to [`MIN_SIZE`]) or turns 90°; where neither fits at the
//! centre it moves to the largest free spot inside the body nearest the centre. Where nothing fits at all it stays
//! on the centre at [`MIN_SIZE`] and the placement is not clean. Every line on the fab layers gets a stroke of
//! [`FAB_STROKE`]. Every other byte of the footprint stays as it was.
//!
//! [`check`] judges a footprint by these rules; [`plan`] rewrites only the footprints that break one, and what it
//! writes passes [`check`].

use std::f64::consts::TAU;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::migrate::{Edit, apply_edits};
use crate::sexpr::{self, Node};

/// Ink box of a five-character designator (`R1234`, and the library's `REF**`) in KiCad's stroke font at size 1 mm
/// and thickness 0.15 mm, the stroke included, from the text's anchor, as pcbnew plots it (x -2.242 to 2.337,
/// y -0.668 to 0.530), rounded outwards: the ink sits a little right of and above the anchor. Boards carry
/// designators of two to five characters (a fifth of them five). The thickness is 15 % of the size, so the box
/// scales with it.
pub const INK: Ink = Ink { x0: -2.25, x1: 2.35, y0: -0.67, y1: 0.54 };
/// KiCad's convention for fab text is 1 mm, smaller to fit the part.
pub const MAX_SIZE: f64 = 1.0;
/// Below this a designator no longer reads on a fab print.
pub const MIN_SIZE: f64 = 0.3;
/// Sizes tried, from [`MAX_SIZE`] down.
pub const SIZE_STEP: f64 = 0.05;
/// The stroke of every line on the fab layers.
pub const FAB_STROKE: f64 = 0.05;
/// Room kept between the designator's ink and the other drawing on its layer, beyond the strokes.
pub const GAP: f64 = 0.05;
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
    /// The drawing on the designator's fab layer, as drawn with [`FAB_STROKE`].
    pub graphics: Vec<Prim>,
    /// A bottom-side footprint: its text is mirrored.
    pub back: bool,
}

impl Shape {
    /// The body's centre, rounded the way it is written.
    pub fn centre(&self) -> (f64, f64) {
        let (x, y) = self.body.center();
        (round3(x), round3(y))
    }

    /// The designator's ink box at (x, y).
    pub fn text_rect(&self, x: f64, y: f64, angle: f64, size: f64) -> Rect {
        let ink = if self.back { Ink { x0: -INK.x1, x1: -INK.x0, ..INK } } else { INK };
        let i = ink.scaled(size, size).turned(angle);
        Rect { x0: x + i.x0, y0: y + i.y0, x1: x + i.x1, y1: y + i.y1 }
    }

    /// The designator fits at (x, y): inside the body, clear of the drawing.
    pub fn fits(&self, x: f64, y: f64, angle: f64, size: f64) -> bool {
        let r = self.text_rect(x, y, angle, size);
        self.region.contains(&r) && !self.graphics.iter().any(|g| g.hits(&r))
    }

    /// The largest size that fits at `c` at `angle`.
    fn room(&self, c: (f64, f64), angle: f64) -> Option<f64> {
        sizes().find(|&s| self.fits(c.0, c.1, angle, s))
    }
}

/// [`MAX_SIZE`] down to [`MIN_SIZE`] in [`SIZE_STEP`]s.
fn sizes() -> impl Iterator<Item = f64> {
    let n = ((MAX_SIZE - MIN_SIZE) / SIZE_STEP + EPS).floor() as i64;
    (0..=n).map(|i| ((MAX_SIZE - i as f64 * SIZE_STEP) * 100.0).round() / 100.0)
}

/// A designator's size, anchor and angle (0 or 90); `clean` when it fits by the rules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub size: f64,
    pub clean: bool,
}

/// A text's ink box from its anchor: `x0` and `y0` below 0, `x1` and `y1` above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

impl Ink {
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    fn scaled(&self, w: f64, h: f64) -> Ink {
        Ink { x0: self.x0 * w, x1: self.x1 * w, y0: self.y0 * h, y1: self.y1 * h }
    }

    /// Turned by `angle`, a multiple of 90°, counter-clockwise on screen (y down) as KiCad turns text.
    fn turned(&self, angle: f64) -> Ink {
        let Ink { x0, x1, y0, y1 } = *self;
        match (angle.rem_euclid(360.0) / 90.0).round() as i64 % 4 {
            1 => Ink { x0: y0, x1: y1, y0: -x1, y1: -x0 },
            2 => Ink { x0: -x1, x1: -x0, y0: -y1, y1: -y0 },
            3 => Ink { x0: -y1, x1: -y0, y0: x0, y1: x1 },
            _ => *self,
        }
    }
}

/// KiCad's stroke width for fab text: 15 % of the size.
pub fn thickness(size: f64) -> f64 {
    (size * 0.15 * 100.0).round() / 100.0
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

/// Where the designator goes: on the body's centre at the largest size that fits there, at 0° or 90° (0° on a
/// tie); else the largest size that fits anywhere inside the body, nearest the centre; else on the centre at
/// [`MIN_SIZE`], turned along the body, not clean.
pub fn layout(shape: &Shape) -> Placement {
    let c = shape.centre();
    let level = shape.room(c, 0.0);
    let turned = shape.room(c, 90.0);
    match (level, turned) {
        (Some(a), Some(b)) if b > a + EPS => return Placement { x: c.0, y: c.1, angle: 90.0, size: b, clean: true },
        (Some(a), _) => return Placement { x: c.0, y: c.1, angle: 0.0, size: a, clean: true },
        (None, Some(b)) => return Placement { x: c.0, y: c.1, angle: 90.0, size: b, clean: true },
        (None, None) => {}
    }
    let pts = grid(shape.region, c);
    for size in sizes() {
        for &(x, y) in &pts {
            for angle in [0.0, 90.0] {
                if shape.fits(x, y, angle, size) {
                    return Placement { x, y, angle, size, clean: true };
                }
            }
        }
    }
    let angle = if shape.region.h() > shape.region.w() + EPS { 90.0 } else { 0.0 };
    Placement { x: c.0, y: c.1, angle, size: MIN_SIZE, clean: false }
}

/// The `Reference` property as the footprint has it.
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
    pub reference: Option<Reference>,
    pub ref_texts: Vec<RefText>,
    /// The Value property is visible (false without one).
    pub value_shown: bool,
    /// The stroke widths on F.Fab and B.Fab that are not [`FAB_STROKE`]: the `(width ...)` nodes.
    pub fab_widths: Vec<Range<usize>>,
    pub shape: Option<Shape>,
}

impl Footprint {
    pub fn fab_layer(&self) -> &'static str {
        if self.back { "B.Fab" } else { "F.Fab" }
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

/// The box a visible user text on the fab layer takes (about 0.92 of its size per character).
fn text_area(t: &Node) -> Option<Prim> {
    let text = t.arg(1)?;
    let at = t.child("at")?;
    let (x, y) = xy(at)?;
    let a = num(at.items().get(3)).unwrap_or(0.0).to_radians();
    let (w, h, _) = font_size(t.child("effects"));
    let half = 0.46 * w * text.chars().count() as f64;
    let (sin, cos) = a.sin_cos();
    let turn = |(u, v): (f64, f64)| (x + u * cos + v * sin, y - u * sin + v * cos);
    let pts = [(-half, -0.67 * h), (half, -0.67 * h), (half, 0.54 * h), (-half, 0.54 * h)];
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

/// Reads what the rules look at from a `.kicad_mod` text.
pub fn read(text: &str) -> Result<Footprint, String> {
    let fp = sexpr::parse(text).map_err(|e| e.to_string())?;
    if fp.head() != Some("footprint") {
        return Err("not a (footprint ...) file".into());
    }
    let back = fp.value("layer") == Some("B.Cu");
    let mut reference = None;
    let mut ref_texts = Vec::new();
    let mut value_shown = false;
    for n in fp.items() {
        match n.head() {
            Some("property") if n.arg(0) == Some("Reference") => {
                let at = n.child("at");
                let effects = n.child("effects");
                let (width, height, thickness) = font_size(effects);
                let justify = effects
                    .and_then(|e| e.child("justify"))
                    .map(|j| j.items().iter().skip(1).filter_map(Node::as_atom).map(String::from).collect())
                    .unwrap_or_default();
                reference = Some(Reference {
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
                });
            }
            Some("property") if n.arg(0) == Some("Value") => value_shown = !hidden(n),
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
    let lines: Vec<Prim> = layer_graphics(&fp, fab).flat_map(|g| graphic_prims(g, FAB_STROKE)).collect();
    let mut graphics = lines.clone();
    graphics.extend(
        fp.children("fp_text")
            .filter(|t| t.value("layer") == Some(fab) && !hidden(t) && !is_ref_text(t))
            .filter_map(text_area),
    );
    let pads: Vec<Rect> = fp.children("pad").filter_map(pad_rect).collect();
    // A fab drawing that leaves out the middle of the pads (only a pin-1 dot, say) is not the body.
    let pads_box = Rect::of_points(pads.iter().flat_map(|p| [(p.x0, p.y0), (p.x1, p.y1)]));
    let covers_pads =
        |b: &Rect| pads_box.is_none_or(|p| b.contains(&Rect::around(p.center().0, p.center().1, 0.0, 0.0)));
    let courtyard = || {
        let g: Vec<&Node> = layer_graphics(&fp, crtyd).collect();
        let stroke = g.iter().map(|g| stroke_width(g)).fold(0.0, f64::max);
        let body = prims_box(&g.iter().flat_map(|g| graphic_prims(g, stroke_width(g))).collect::<Vec<_>>())?;
        Some((body, body.expand(-stroke)))
    };
    let shape = if let Some(body) = prims_box(&lines).filter(covers_pads) {
        Some((body, BodyFrom::Fab, body))
    } else if let Some((body, region)) = courtyard() {
        Some((body, BodyFrom::Courtyard, region))
    } else {
        pads_box.map(|body| (body, BodyFrom::Pads, body))
    };
    let shape = shape.map(|(body, body_from, region)| Shape { body, body_from, region, graphics, back });
    Ok(Footprint { back, reference, ref_texts, value_shown, fab_widths, shape })
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
    if r.layer != fab {
        out.push(format!("Reference on {}", r.layer));
    }
    if r.hidden {
        out.push("Reference hidden".into());
    }
    if !r.unlocked {
        out.push("Reference locked".into());
    }
    let silk = fp.ref_texts.iter().filter(|t| is_silk(&t.layer)).count();
    if silk > 0 {
        out.push(format!("{silk} ${{REFERENCE}} on silk"));
    }
    let fab_texts = fp.ref_texts.iter().filter(|t| t.layer == fab).count();
    if fab_texts > 0 {
        out.push(format!("{fab_texts} ${{REFERENCE}} on {fab} besides the Reference"));
    }
    let mirror = r.justify.iter().any(|j| j == "mirror");
    if r.justify.iter().any(|j| j != "mirror") || mirror != fp.back {
        out.push(format!("Reference justified ({})", r.justify.join(" ")));
    }
    if fp.value_shown {
        out.push("Value shown".into());
    }
    if !fp.fab_widths.is_empty() {
        out.push(format!("{} fab lines not {FAB_STROKE} mm", fp.fab_widths.len()));
    }
    if let Some(shape) = &fp.shape {
        let p = layout(shape);
        if (r.x - p.x).abs() > 5e-4 || (r.y - p.y).abs() > 5e-4 {
            out.push(format!("Reference at ({}, {}), not ({}, {})", fmt(r.x), fmt(r.y), fmt(p.x), fmt(p.y)));
        }
        if (r.angle - p.angle).rem_euclid(360.0).min((p.angle - r.angle).rem_euclid(360.0)) > EPS {
            out.push(format!("Reference at {}°, not {}°", fmt(r.angle), fmt(p.angle)));
        }
        if (r.width - p.size).abs() > EPS || (r.height - p.size).abs() > EPS {
            out.push(format!("Reference size {} x {}, not {}", fmt(r.width), fmt(r.height), fmt(p.size)));
        } else if r.thickness.is_none_or(|t| (t - thickness(p.size)).abs() > EPS) {
            out.push(format!("Reference thickness not {}", fmt(thickness(p.size))));
        }
    }
    out
}

/// KiCad's way of writing a number: no trailing zeros, no `-0`.
pub fn fmt(v: f64) -> String {
    let s = format!("{:.4}", (v * 10000.0).round() / 10000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// The `Reference` property at `p`, indented like the original at `indent`.
fn property_text(r: &Reference, fab: &str, back: bool, p: &Placement, indent: &str) -> String {
    let i1 = format!("{indent}\t");
    // KiCad writes a property's angle even when it is 0.
    let at = format!("(at {} {} {})", fmt(p.x), fmt(p.y), fmt(p.angle));
    let mut s = format!(
        "(property \"Reference\" {}\n{i1}{at}\n{i1}(unlocked yes)\n{i1}(layer {})\n",
        sexpr::quote(&r.value),
        sexpr::quote(fab)
    );
    if let Some(u) = &r.uuid {
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
    /// Broke `why`; rewritten with the designator at `placement`.
    Change { why: Vec<String>, before: Before, placement: Placement, body_from: BodyFrom, new: String },
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
                (b.angle - p.angle).rem_euclid(180.0).min((p.angle - b.angle).rem_euclid(180.0)) > EPS,
            ),
            _ => (false, false, false),
        }
    }
}

/// One footprint's [`Outcome`].
pub fn fix(text: &str) -> Outcome {
    let fp = match read(text) {
        Ok(fp) => fp,
        Err(e) => return Outcome::Skipped(format!("does not parse: {e}")),
    };
    let why = check(&fp);
    if why.is_empty() {
        return Outcome::Fine;
    }
    let Some(r) = &fp.reference else { return Outcome::Skipped("no Reference property".into()) };
    let Some(shape) = &fp.shape else {
        return Outcome::Skipped("no fab outline, courtyard or pads to place the designator by".into());
    };
    let placement = layout(shape);
    let fab = fp.fab_layer();
    let start = text[..r.span.start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[start..r.span.start];
    let indent = if indent.trim().is_empty() { indent } else { "\t" };
    let mut edits = vec![Edit { range: r.span.clone(), text: property_text(r, fab, fp.back, &placement, indent) }];
    for t in &fp.ref_texts {
        if is_silk(&t.layer) || t.layer == fab {
            edits.push(Edit { range: line_span(text, &t.span), text: String::new() });
        }
    }
    for w in &fp.fab_widths {
        edits.push(Edit { range: w.clone(), text: format!("(width {})", fmt(FAB_STROKE)) });
    }
    if fp.value_shown {
        let node = sexpr::parse(text).expect("parsed by read");
        let value = node.children("property").find(|p| p.arg(0) == Some("Value")).expect("read found it");
        match hide_value_edit(text, value) {
            Ok(Some(e)) => edits.push(e),
            Ok(None) => {}
            Err(e) => return Outcome::Skipped(e),
        }
    }
    edits.sort_by_key(|e| e.range.start);
    let before = Before { x: r.x, y: r.y, angle: r.angle, size: r.height };
    Outcome::Change { why, before, placement, body_from: shape.body_from, new: apply_edits(text, &edits) }
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
                    let outcome = fix(&old);
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

    fn line(a: (f64, f64), b: (f64, f64)) -> String {
        format!(
            "\t(fp_line\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.12)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n",
            a.0, a.1, b.0, b.1
        )
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
        assert_eq!(p, Placement { x: 0.0, y: 0.0, angle: 90.0, size: 1.0, clean: true });
        assert!(!new.contains("${REFERENCE}"), "{new}");
        assert!(new.contains("(layer \"F.Fab\")\n\t\t(uuid \"u-ref\")"), "{new}");
        assert!(
            new.contains("(property \"Value\" \"T\"\n\t\t(at 0 2 0)\n\t\t(layer \"F.Fab\")\n\t\t(hide yes)\n"),
            "{new}"
        );
        assert!(new.contains("(width 0.05)") && !new.contains("(width 0.1)"), "{new}");
        settled(&new, &p);
    }

    #[test]
    fn pads_do_not_push_the_text_off_the_centre() {
        // 1206: the body lies within the pads; the text sits on its centre over them, level, 0.6 mm.
        let pads = [(-1.475, 0.0, 1.05, 1.75), (1.475, 0.0, 1.05, 1.75)];
        let (p, new) = changed(&footprint(3.2, 1.6, &pads, "F.SilkS", &["F.Fab"]));
        assert_eq!(p, Placement { x: 0.0, y: 0.0, angle: 0.0, size: 0.6, clean: true });
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
        assert_eq!((p.x, p.y, p.angle), (0.0, 0.0, 0.0), "{p:?}");
        assert!(p.size == 0.85 && p.clean, "{p:?}");
        settled(&new, &p);
        // A tall body turns it.
        let (p, new) = changed(&footprint(1.4, 4.0, &[], "F.Fab", &[]));
        assert_eq!((p.x, p.y, p.angle), (0.0, 0.0, 90.0), "{p:?}");
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
    fn a_body_too_small_keeps_the_text_centred_at_the_smallest_size_not_clean() {
        // 0402: 1 x 0.5, nothing fits.
        let pads = [(-0.48, 0.0, 0.56, 0.62), (0.48, 0.0, 0.56, 0.62)];
        let (p, new) = changed(&footprint(1.0, 0.5, &pads, "F.SilkS", &["F.Fab"]));
        assert_eq!(p, Placement { x: 0.0, y: 0.0, angle: 0.0, size: MIN_SIZE, clean: false });
        settled(&new, &p);
        assert!(new.contains("(size 0.3 0.3)\n\t\t\t\t(thickness 0.05)"), "{new}");
    }

    #[test]
    fn a_reference_off_the_centre_is_moved_back() {
        let pads = [(-3.0, 0.0, 1.2, 1.2), (3.0, 0.0, 1.2, 1.2)];
        let (p, new) = changed(&footprint(4.0, 2.0, &pads, "F.Fab", &[]));
        let moved = new.replace(&format!("(at 0 0 {})", fmt(p.angle)), "(at 0.2 0.3 0)");
        let Outcome::Change { why, before, .. } = fix(&moved) else { panic!("{moved}") };
        assert!(why.iter().any(|w| w.contains("not (0, 0)")), "{why:?}");
        assert_eq!((before.x, before.y), (0.2, 0.3));
        let o = fix(&moved);
        assert_eq!(o.designator_changes(), (true, false, false));
    }

    #[test]
    fn a_bottom_footprint_gets_b_fab_and_mirrored_text() {
        let text = footprint(4.0, 2.0, &[], "B.SilkS", &[])
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")")
            .replace("(layer \"F.Fab\")\n\t)", "(layer \"B.Fab\")\n\t)");
        let (p, new) = changed(&text);
        assert!(new.contains("(layer \"B.Fab\")") && new.contains("(justify mirror)"), "{new}");
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
        assert_eq!(thickness(0.3), 0.05);
        assert_eq!(sizes().collect::<Vec<_>>().len(), 15);
        assert_eq!(sizes().last(), Some(MIN_SIZE));
    }
}
