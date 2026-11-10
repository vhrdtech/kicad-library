//! Footprints whose designator lives only on the fab layer: the `Reference` property on F.Fab (B.Fab for a
//! bottom-side footprint), visible and unlocked, no `${REFERENCE}` text on the silkscreen and no second one on the
//! fab layer. The text is sized and placed so that its box never touches a pad: the largest size up to
//! [`MAX_SIZE`] that fits inside the body's outline (inside the stroke of its lines) clear of every pad, at 0° or
//! 90°. When not even [`MIN_SIZE`] fits there, or the body lies within the pads' box (small passives, QFNs: text
//! between or over the pads is hidden by them), it goes outside the body and the pads. Every other byte of the
//! footprint stays as it was.
//!
//! [`check`] judges a footprint by these rules, so a reference placed by hand that follows them is left alone;
//! [`plan`] rewrites only the footprints that break one, and what it writes passes [`check`].

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
/// KiCad's convention for fab text: 1 mm, smaller to fit the part, not below 0.25 mm.
pub const MAX_SIZE: f64 = 1.0;
pub const MIN_SIZE: f64 = 0.25;
/// Room kept between the text box and every pad.
pub const MARGIN: f64 = 0.1;
/// A text outside the body is at most this far from it.
pub const NEAR: f64 = 1.0;
/// Grid of the positions tried, from the body's centre.
const STEP: f64 = 0.05;
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

    /// The gap between two boxes, 0 when they overlap or touch.
    pub fn distance(&self, o: &Rect) -> f64 {
        let dx = (o.x0 - self.x1).max(self.x0 - o.x1).max(0.0);
        let dy = (o.y0 - self.y1).max(self.y0 - o.y1).max(0.0);
        dx.hypot(dy)
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
    /// The body's box, the stroke of its lines included.
    pub body: Rect,
    pub body_from: BodyFrom,
    /// The widest stroke of the lines the body was measured by (0 for a body from the pads).
    pub stroke: f64,
    pub pads: Vec<Rect>,
}

impl Shape {
    /// Inside the stroke of the body's outline: where text inside the body may go.
    pub fn inner(&self) -> Rect {
        self.body.expand(-self.stroke)
    }

    pub fn pads_box(&self) -> Option<Rect> {
        Rect::of_points(self.pads.iter().flat_map(|p| [(p.x0, p.y0), (p.x1, p.y1)]))
    }

    /// The body lies within the pads' box (a chip resistor, a QFN): text inside it would sit between or over
    /// the pads, so it goes outside them.
    pub fn within_pads(&self) -> bool {
        self.pads_box().is_some_and(|p| p.contains(&self.body))
    }

    /// The body and the pads' box together: what text next to the body stays out of.
    pub fn outline(&self) -> Rect {
        match self.pads_box() {
            Some(p) => Rect {
                x0: self.body.x0.min(p.x0),
                y0: self.body.y0.min(p.y0),
                x1: self.body.x1.max(p.x1),
                y1: self.body.y1.max(p.y1),
            },
            None => self.body,
        }
    }
}

/// A designator's size, centre and angle (0 or 90).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub size: f64,
    pub inside: bool,
}

impl Placement {
    pub fn rect(&self) -> Rect {
        text_rect(self.x, self.y, self.angle, self.size, self.size)
    }
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

/// The ink box of a designator of `width` x `height` font size anchored at (x, y).
fn text_rect(x: f64, y: f64, angle: f64, width: f64, height: f64) -> Rect {
    let i = INK.scaled(width, height).turned(angle);
    Rect { x0: x + i.x0, y0: y + i.y0, x1: x + i.x1, y1: y + i.y1 }
}

/// KiCad's stroke width for fab text: 15 % of the size.
pub fn thickness(size: f64) -> f64 {
    (size * 0.15 * 100.0).round() / 100.0
}

/// The grid points of `area`, [`STEP`] apart from `centre`, rounded the way they are written.
fn grid(area: Rect, centre: (f64, f64)) -> impl Iterator<Item = (f64, f64)> {
    let nx_lo = ((centre.0 - area.x0) / STEP + EPS).floor() as i64;
    let nx_hi = ((area.x1 - centre.0) / STEP + EPS).floor() as i64;
    let ny_lo = ((centre.1 - area.y0) / STEP + EPS).floor() as i64;
    let ny_hi = ((area.y1 - centre.1) / STEP + EPS).floor() as i64;
    (-ny_lo..=ny_hi).flat_map(move |j| {
        (-nx_lo..=nx_hi).map(move |i| (round3(centre.0 + i as f64 * STEP), round3(centre.1 + j as f64 * STEP)))
    })
}

/// The largest size a designator anchored at `c` may have without leaving `body` or coming within [`MARGIN`]
/// of a pad; `ink` is its box at size 1.
fn room(c: (f64, f64), ink: &Ink, body: &Rect, pads: &[Rect]) -> f64 {
    let mut s = ((c.0 - body.x0) / -ink.x0)
        .min((body.x1 - c.0) / ink.x1)
        .min((c.1 - body.y0) / -ink.y0)
        .min((body.y1 - c.1) / ink.y1);
    // The largest scale that keeps [c + s a0, c + s a1] off [lo, hi] widened by the margin.
    let apart = |c: f64, lo: f64, hi: f64, a0: f64, a1: f64| {
        let (lo, hi) = (lo - MARGIN, hi + MARGIN);
        if c <= lo {
            (lo - c) / a1
        } else if c >= hi {
            (c - hi) / -a0
        } else {
            0.0
        }
    };
    for p in pads {
        s = s.min(apart(c.0, p.x0, p.x1, ink.x0, ink.x1).max(apart(c.1, p.y0, p.y1, ink.y0, ink.y1)));
        if s <= 0.0 {
            return 0.0;
        }
    }
    s.max(0.0)
}

fn floor2(v: f64) -> f64 {
    ((v + EPS) * 100.0).floor() / 100.0
}

fn round3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The best place inside the body's outline: the largest size (up to [`MAX_SIZE`]) clear of the pads, at the
/// angle that allows it (on a tie, 90° for a body taller than wide), as near the body's centre as that size
/// allows. None for a body within the pads' box.
pub fn inside(shape: &Shape) -> Option<Placement> {
    if shape.within_pads() {
        return None;
    }
    let body = &shape.inner();
    if body.w() <= 0.0 || body.h() <= 0.0 {
        return None;
    }
    let centre = body.center();
    let angles = if body.h() > body.w() + EPS { [90.0, 0.0] } else { [0.0, 90.0] };
    let mut best: Option<(f64, f64)> = None; // (size, angle)
    for angle in angles {
        let ink = INK.turned(angle);
        let mut top = 0.0f64;
        for c in grid(*body, centre) {
            top = top.max(room(c, &ink, body, &shape.pads));
            if top >= MAX_SIZE {
                break;
            }
        }
        let size = floor2(top.min(MAX_SIZE));
        if best.is_none_or(|(s, _)| size > s) {
            best = Some((size, angle));
        }
    }
    let (size, angle) = best?;
    if size < MIN_SIZE {
        return None;
    }
    let ink = INK.turned(angle);
    let key = |c: &(f64, f64)| {
        let d = (c.0 - centre.0).hypot(c.1 - centre.1);
        ((d * 1000.0).round() as i64, ((c.1 - centre.1).abs() * 1000.0).round() as i64)
    };
    let (x, y) =
        grid(*body, centre).filter(|c| room(*c, &ink, body, &shape.pads) >= size - EPS).min_by_key(|c| key(c))?;
    Some(Placement { x, y, angle, size, inside: true })
}

/// Next to the body: outside the body and the pads' box ([`Shape::outline`]), clear of the pads, a size from the
/// body's length (smaller, down to [`MIN_SIZE`], where that does not fit), as close as possible (in 0.5 mm bands);
/// in a band level text, then above, then centred, then the closest.
pub fn outside(shape: &Shape) -> Option<Placement> {
    let body = &shape.body;
    let around = shape.outline();
    let mut size = floor2((body.w().max(body.h()) / INK.width()).clamp(MIN_SIZE, MAX_SIZE));
    loop {
        if let Some(p) = outside_at(shape, &around, size) {
            return Some(p);
        }
        if size <= MIN_SIZE + EPS {
            return None;
        }
        size = floor2((size - 0.05).max(MIN_SIZE));
    }
}

fn outside_at(shape: &Shape, around: &Rect, size: f64) -> Option<Placement> {
    let centre = around.center();
    // (distance band, angle rank, below, off-centre, distance), smallest first.
    type Key = (i64, u8, u8, i64, i64);
    let mut best: Option<(Key, Placement)> = None;
    for (rank, angle) in [(0u8, 0.0), (1, 90.0)] {
        let probe = text_rect(0.0, 0.0, angle, size, size);
        let area = around.expand(NEAR + probe.w().max(probe.h()));
        for (x, y) in grid(area, centre) {
            let r = text_rect(x, y, angle, size, size);
            if r.overlaps(around) || shape.pads.iter().any(|p| r.overlaps(&p.expand(MARGIN))) {
                continue;
            }
            let d = r.distance(around);
            if d > NEAR + EPS {
                continue;
            }
            // Distances in one 0.5 mm band count as one, so level text centred above the body wins over a corner
            // a grid step closer.
            let key = (
                (d * 2.0 + EPS).floor() as i64,
                rank,
                u8::from(y >= centre.1),
                ((x - centre.0).abs() * 1000.0).round() as i64,
                (d * 1000.0).round() as i64,
            );
            if best.as_ref().is_none_or(|(k, _)| key < *k) {
                best = Some((key, Placement { x, y, angle, size, inside: false }));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Where the designator goes: inside the body when [`MIN_SIZE`] fits there (and the body is not within the pads'
/// box), else next to the body and the pads.
pub fn layout(shape: &Shape) -> Option<Placement> {
    inside(shape).or_else(|| outside(shape))
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

/// A graphic item's line width.
fn stroke_width(g: &Node) -> f64 {
    g.child("stroke")
        .and_then(|s| num(s.child("width").and_then(|w| w.items().get(1))))
        .or_else(|| num(g.child("width").and_then(|w| w.items().get(1))))
        .unwrap_or(0.0)
}

/// The points a graphic item covers, its stroke included (start, end, mid, centre, pts).
fn graphic_points(g: &Node) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let half = stroke_width(g) / 2.0;
    let head = g.head().unwrap_or("");
    if head.ends_with("circle") {
        if let (Some(c), Some(e)) = (g.child("center").and_then(xy), g.child("end").and_then(xy)) {
            let r = (e.0 - c.0).hypot(e.1 - c.1) + half;
            pts.extend([(c.0 - r, c.1 - r), (c.0 + r, c.1 + r)]);
        }
        return pts;
    }
    for name in ["start", "mid", "end"] {
        if let Some(p) = g.child(name).and_then(xy) {
            pts.push(p);
        }
    }
    if let Some(list) = g.child("pts") {
        pts.extend(list.children("xy").filter_map(xy));
    }
    pts.into_iter().flat_map(|(x, y)| [(x - half, y - half), (x + half, y + half)]).collect()
}

const GRAPHICS: [&str; 6] = ["fp_line", "fp_rect", "fp_circle", "fp_arc", "fp_poly", "fp_curve"];

fn layer_graphics<'a>(fp: &'a Node, layer: &'a str) -> impl Iterator<Item = &'a Node> {
    fp.items()
        .iter()
        .filter(move |g| g.head().is_some_and(|h| GRAPHICS.contains(&h)) && g.value("layer") == Some(layer))
}

/// The box of a layer's graphics, their strokes included, and the widest stroke.
fn layer_box(fp: &Node, layer: &str) -> Option<(Rect, f64)> {
    let body = Rect::of_points(layer_graphics(fp, layer).flat_map(graphic_points))?;
    Some((body, layer_graphics(fp, layer).map(stroke_width).fold(0.0, f64::max)))
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
            local.extend(graphic_points(g));
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

/// Reads what the rules look at from a `.kicad_mod` text.
pub fn read(text: &str) -> Result<Footprint, String> {
    let fp = sexpr::parse(text).map_err(|e| e.to_string())?;
    if fp.head() != Some("footprint") {
        return Err("not a (footprint ...) file".into());
    }
    let back = fp.value("layer") == Some("B.Cu");
    let mut reference = None;
    let mut ref_texts = Vec::new();
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
                    hidden: yes(n, "hide") || effects.is_some_and(|e| yes(e, "hide")),
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
            Some("fp_text") if n.arg(0) == Some("user") && n.arg(1).is_some_and(|t| t.contains("${REFERENCE}")) => {
                ref_texts.push(RefText { layer: n.value("layer").unwrap_or("").to_string(), span: n.span() });
            }
            _ => {}
        }
    }
    let fab = if back { "B.Fab" } else { "F.Fab" };
    let crtyd = if back { "B.CrtYd" } else { "F.CrtYd" };
    let pads: Vec<Rect> = fp.children("pad").filter_map(pad_rect).collect();
    // A fab drawing that leaves out the middle of the pads (only a pin-1 dot, say) is not the body.
    let pads_box = Rect::of_points(pads.iter().flat_map(|p| [(p.x0, p.y0), (p.x1, p.y1)]));
    let covers_pads =
        |b: &Rect| pads_box.is_none_or(|p| b.contains(&Rect::around(p.center().0, p.center().1, 0.0, 0.0)));
    let shape = if let Some((body, stroke)) = layer_box(&fp, fab).filter(|(b, _)| covers_pads(b)) {
        Some(Shape { body, body_from: BodyFrom::Fab, stroke, pads })
    } else if let Some((body, stroke)) = layer_box(&fp, crtyd) {
        Some(Shape { body, body_from: BodyFrom::Courtyard, stroke, pads })
    } else {
        pads_box.map(|body| Shape { body, body_from: BodyFrom::Pads, stroke: 0.0, pads })
    };
    Ok(Footprint { back, reference, ref_texts, shape })
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
    if (r.angle.rem_euclid(90.0)).abs() > EPS && (r.angle.rem_euclid(90.0) - 90.0).abs() > EPS {
        out.push(format!("Reference at {}°", fmt(r.angle)));
    }
    let size = r.width.max(r.height);
    if size > MAX_SIZE + EPS || r.width.min(r.height) < MIN_SIZE - EPS {
        out.push(format!("Reference size {} x {}", fmt(r.width), fmt(r.height)));
    }
    if r.thickness.is_none_or(|t| (t - thickness(r.height)).abs() > 0.02) {
        out.push("Reference thickness not 15 % of its size".into());
    }
    if let Some(shape) = &fp.shape {
        let rect = text_rect(r.x, r.y, r.angle, r.width, r.height);
        let hits = shape.pads.iter().filter(|p| rect.overlaps(&p.expand(MARGIN))).count();
        if hits > 0 {
            out.push(format!("Reference within {MARGIN} mm of {hits} pads"));
        }
        if inside(shape).is_some() {
            if !shape.inner().contains(&rect) {
                out.push("Reference not inside the body's outline, where it fits".into());
            }
        } else {
            let around = shape.outline();
            if rect.overlaps(&around) {
                out.push("Reference over the body or the pads".into());
            } else if rect.distance(&around) > NEAR + EPS {
                out.push("Reference not next to the body".into());
            }
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

/// What happens to one footprint.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Follows every rule; nothing to change.
    Fine,
    /// Broke `why`; rewritten with the designator at `placement`.
    Change { why: Vec<String>, placement: Placement, body_from: BodyFrom, new: String },
    /// Left alone.
    Skipped(String),
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
    let Some(placement) = layout(shape) else {
        return Outcome::Skipped(format!("no room clear of the pads within {NEAR} mm of the body"));
    };
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
    edits.sort_by_key(|e| e.range.start);
    Outcome::Change { why, placement, body_from: shape.body_from, new: apply_edits(text, &edits) }
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

    /// A footprint with a fab body `w` x `h`, the given pads (x, y, w, h), the Reference on `ref_layer` and
    /// `${REFERENCE}` user texts on `user_layers`.
    pub(crate) fn footprint(
        w: f64,
        h: f64,
        pads: &[(f64, f64, f64, f64)],
        ref_layer: &str,
        user_layers: &[&str],
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
        for (n, (x, y, pw, ph)) in pads.iter().enumerate() {
            s += &format!(
                "\t(pad \"{}\" smd rect\n\t\t(at {x} {y})\n\t\t(size {pw} {ph})\n\t\t(layers \"F.Cu\")\n\t)\n",
                n + 1
            );
        }
        s + ")\n"
    }

    fn changed(text: &str) -> (Placement, String) {
        match fix(text) {
            Outcome::Change { placement, new, .. } => (placement, new),
            o => panic!("expected a change, got {o:?}"),
        }
    }

    fn assert_clear(p: &Placement, shape: &Shape) {
        let r = p.rect();
        for pad in &shape.pads {
            assert!(!r.overlaps(&pad.expand(MARGIN)), "{p:?} touches {pad:?}");
        }
    }

    #[test]
    fn soic_like_gets_1mm_inside_with_silk_texts_gone() {
        // A 3.9 x 4.9 body, pads outside it left and right.
        let pads: Vec<_> = (0..4)
            .flat_map(|i| [(-2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6), (2.7, -1.905 + 1.27 * i as f64, 1.5, 0.6)])
            .collect();
        let text = footprint(3.9, 4.9, &pads, "F.SilkS", &["F.SilkS", "F.Fab"]);
        let (p, new) = changed(&text);
        assert!(p.inside);
        assert_eq!(p.size, 1.0);
        assert_eq!((p.x, p.y), (0.0, 0.0));
        assert!(!new.contains("${REFERENCE}"), "{new}");
        assert!(new.contains("(layer \"F.Fab\")\n\t\t(uuid \"u-ref\")"), "{new}");
        assert!(new.contains("(property \"Value\" \"T\"\n\t\t(at 0 2 0)"), "the Value stays:\n{new}");
        let fp = read(&new).unwrap();
        assert_eq!(check(&fp), Vec::<String>::new());
        assert_eq!(fix(&new), Outcome::Fine, "a second run changes nothing");
    }

    #[test]
    fn tall_body_turns_the_text() {
        // SOT-23-like: 1.3 x 2.9 body, pads beside it.
        let pads = [(-1.1, -0.95, 0.9, 0.6), (-1.1, 0.95, 0.9, 0.6), (1.1, 0.0, 0.9, 0.6)];
        let (p, _) = changed(&footprint(1.3, 2.9, &pads, "F.SilkS", &[]));
        assert_eq!(p.angle, 90.0);
        assert!(p.inside && p.size >= 0.6, "{p:?}");
    }

    #[test]
    fn text_inside_stays_off_the_outline_stroke() {
        // A wide outline line: the text keeps inside its inner edge.
        let pads = [(-3.0, 0.0, 1.0, 1.0), (3.0, 0.0, 1.0, 1.0)];
        let text = footprint(4.0, 1.4, &pads, "F.SilkS", &[]).replace("(width 0.1)", "(width 0.3)");
        let (p, new) = changed(&text);
        let fp = read(&new).unwrap();
        let shape = fp.shape.as_ref().unwrap();
        assert!((shape.stroke - 0.3).abs() < 1e-9, "{shape:?}");
        assert!(p.inside && shape.inner().contains(&p.rect()), "{p:?}");
        assert!(Rect::around(0.0, 0.0, 1.85, 0.55).contains(&p.rect()), "off the line drawn at ±2 x ±0.7: {p:?}");
        assert_eq!(check(&fp), Vec::<String>::new());
    }

    #[test]
    fn a_body_within_the_pads_puts_the_text_outside_the_pads() {
        // 1206: the body sits within the pads' box. There is room between the pads, but the text goes outside
        // them, never between or over them.
        let pads = [(-1.475, 0.0, 1.05, 1.75), (1.475, 0.0, 1.05, 1.75)];
        let (p, new) = changed(&footprint(3.2, 1.6, &pads, "F.SilkS", &["F.Fab"]));
        let fp = read(&new).unwrap();
        let shape = fp.shape.as_ref().unwrap();
        assert!(shape.within_pads());
        assert!(!p.inside, "{p:?}");
        assert!(!p.rect().overlaps(&shape.outline()), "{p:?} over {:?}", shape.outline());
        assert!(p.rect().y1 <= -0.875 + 1e-9, "above the pads: {p:?}");
        assert_eq!(check(&fp), Vec::<String>::new());
        // A designator placed between the pads by hand breaks the rule.
        let between = new.replace(&format!("(at {} {} {})", fmt(p.x), fmt(p.y), fmt(p.angle)), "(at 0 0 90)");
        assert!(check(&read(&between).unwrap()).iter().any(|w| w.contains("over the body")));
    }

    #[test]
    fn pads_over_the_body_shrink_or_move_it_out() {
        // 0402: pads cover both ends, the gap between them is too narrow for MIN_SIZE: next to the body.
        let pads = [(-0.48, 0.0, 0.56, 0.62), (0.48, 0.0, 0.56, 0.62)];
        let text = footprint(1.0, 0.5, &pads, "F.SilkS", &["F.Fab"]);
        let (p, new) = changed(&text);
        assert!(!p.inside, "{p:?}");
        let fp = read(&new).unwrap();
        let shape = fp.shape.as_ref().unwrap();
        assert_clear(&p, shape);
        assert!(!p.rect().overlaps(&shape.body));
        assert!(p.y < 0.0 && p.angle == 0.0, "above the body: {p:?}");
        assert_eq!(check(&fp), Vec::<String>::new());

        // QFN-like: pins round the edge and an exposed pad in the middle leave only a thin ring.
        let mut pads = vec![(0.0, 0.0, 3.4, 3.4)];
        for i in 0..8 {
            let o = -1.75 + 0.5 * i as f64;
            pads.extend([(-2.4, o, 0.8, 0.25), (2.4, o, 0.8, 0.25), (o, -2.4, 0.25, 0.8), (o, 2.4, 0.25, 0.8)]);
        }
        let (p, new) = changed(&footprint(5.0, 5.0, &pads, "F.SilkS", &[]));
        let fp = read(&new).unwrap();
        assert_clear(&p, fp.shape.as_ref().unwrap());
        assert_eq!(check(&fp), Vec::<String>::new());
    }

    #[test]
    fn a_hand_placed_reference_that_follows_the_rules_stays() {
        let pads = [(-3.0, 0.0, 1.2, 1.2), (3.0, 0.0, 1.2, 1.2)];
        let text = footprint(4.0, 2.0, &pads, "F.Fab", &[])
            .replace("(at 0 -2 0)", "(at 0.2 0.3 0)\n\t\t(unlocked yes)")
            .replace("(size 1 1)\n\t\t\t\t(thickness 0.15)", "(size 0.5 0.5)\n\t\t\t\t(thickness 0.08)");
        assert_eq!(fix(&text), Outcome::Fine);
        // The same text over a pad is not.
        let text = text.replace("(at 0.2 0.3 0)", "(at 2.5 0 0)");
        let Outcome::Change { why, .. } = fix(&text) else { panic!() };
        assert!(why.iter().any(|w| w.contains("pads")), "{why:?}");
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
        assert_eq!(thickness(0.26), 0.04);
    }
}
