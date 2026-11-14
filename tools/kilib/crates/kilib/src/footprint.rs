//! A `.kicad_mod` footprint read into what the library rules look at: its pads as copper shapes, its drawing per
//! layer as points and segments, its texts, attributes and 3D models, each with the byte span it came from so a fix
//! can rewrite one item and leave the rest of the file as it was. Coordinates are the footprint's own, in mm,
//! y down, as KiCad writes them.

use std::f64::consts::TAU;
use std::ops::Range;

use crate::sexpr::{self, Node};

/// Points closer than this are the same point; lengths within it are equal.
pub const EPS: f64 = 1e-3;
/// Segments an arc or circle is drawn with.
const ARC_SEGMENTS: usize = 32;

pub type Pt = (f64, f64);

pub fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// How far `p` is from the segment `a`-`b`.
pub fn segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-18 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    dist(p, (a.0 + t * dx, a.1 + t * dy))
}

fn segments_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let cross = |o: Pt, p: Pt, q: Pt| (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0);
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// How far apart two segments are, 0 when they cross.
pub fn segments_distance(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    if segments_cross(a, b, c, d) {
        return 0.0;
    }
    segment_distance(a, c, d)
        .min(segment_distance(b, c, d))
        .min(segment_distance(c, a, b))
        .min(segment_distance(d, a, b))
}

pub fn inside_polygon(p: Pt, poly: &[Pt]) -> bool {
    let mut inside = false;
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            inside = !inside;
        }
    }
    inside
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn of_points(pts: impl IntoIterator<Item = Pt>) -> Option<Rect> {
        let mut r: Option<Rect> = None;
        for (x, y) in pts {
            r = Some(match r {
                None => Rect { x0: x, y0: y, x1: x, y1: y },
                Some(r) => Rect { x0: r.x0.min(x), y0: r.y0.min(y), x1: r.x1.max(x), y1: r.y1.max(y) },
            });
        }
        r
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }

    pub fn expand(&self, m: f64) -> Rect {
        Rect { x0: self.x0 - m, y0: self.y0 - m, x1: self.x1 + m, y1: self.y1 + m }
    }

    pub fn w(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn h(&self) -> f64 {
        self.y1 - self.y0
    }

    pub fn center(&self) -> Pt {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }

    pub fn corners(&self) -> [Pt; 4] {
        [(self.x0, self.y0), (self.x1, self.y0), (self.x1, self.y1), (self.x0, self.y1)]
    }

    pub fn contains(&self, p: Pt) -> bool {
        p.0 >= self.x0 - EPS && p.0 <= self.x1 + EPS && p.1 >= self.y0 - EPS && p.1 <= self.y1 + EPS
    }
}

/// A convex piece of copper or of a mask opening.
#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    /// A convex polygon (a rectangle, rounded rectangle, trapezoid, or a custom pad's hull).
    Poly(Vec<Pt>),
    /// Every point within `r` of the segment `a`-`b` (a circle has `a == b`, an oval is a stadium).
    Capsule { a: Pt, b: Pt, r: f64 },
}

impl Outline {
    /// How far `p` is from the shape, 0 inside it.
    pub fn distance(&self, p: Pt) -> f64 {
        match self {
            Outline::Poly(pts) => {
                if inside_polygon(p, pts) {
                    return 0.0;
                }
                (0..pts.len()).map(|i| segment_distance(p, pts[i], pts[(i + 1) % pts.len()])).fold(f64::MAX, f64::min)
            }
            Outline::Capsule { a, b, r } => (segment_distance(p, *a, *b) - r).max(0.0),
        }
    }

    /// How far the segment `a`-`b` is from the shape, 0 when it touches.
    pub fn segment_distance(&self, a: Pt, b: Pt) -> f64 {
        match self {
            Outline::Poly(pts) => {
                if inside_polygon(a, pts) || inside_polygon(b, pts) {
                    return 0.0;
                }
                (0..pts.len())
                    .map(|i| segments_distance(a, b, pts[i], pts[(i + 1) % pts.len()]))
                    .fold(f64::MAX, f64::min)
            }
            Outline::Capsule { a: c, b: d, r } => (segments_distance(a, b, *c, *d) - r).max(0.0),
        }
    }

    pub fn points(&self) -> Vec<Pt> {
        match self {
            Outline::Poly(pts) => pts.clone(),
            Outline::Capsule { a, b, r } => {
                vec![(a.0.min(b.0) - r, a.1.min(b.1) - r), (a.0.max(b.0) + r, a.1.max(b.1) + r)]
            }
        }
    }

    pub fn bbox(&self) -> Rect {
        Rect::of_points(self.points()).expect("an outline has points")
    }

    /// The shape grown by `m` all round (a polygon's corners stay sharp: a little more than the true offset).
    pub fn grown(&self, m: f64) -> Outline {
        match self {
            Outline::Capsule { a, b, r } => Outline::Capsule { a: *a, b: *b, r: r + m },
            Outline::Poly(pts) => {
                let c = centroid(pts);
                Outline::Poly(
                    pts.iter()
                        .map(|&p| {
                            let d = dist(p, c);
                            if d < 1e-12 {
                                p
                            } else {
                                (p.0 + (p.0 - c.0) / d * m * 1.5, p.1 + (p.1 - c.1) / d * m * 1.5)
                            }
                        })
                        .collect(),
                )
            }
        }
    }
}

fn centroid(pts: &[Pt]) -> Pt {
    let n = pts.len().max(1) as f64;
    (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n)
}

/// The convex hull of points (Andrew's monotone chain), counter-clockwise in maths terms.
pub fn hull(mut pts: Vec<Pt>) -> Vec<Pt> {
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup_by(|a, b| dist(*a, *b) < 1e-9);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Pt, a: Pt, b: Pt| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut lower: Vec<Pt> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Pt> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadKind {
    Smd,
    ThruHole,
    NpThruHole,
    Connect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    pub number: String,
    pub kind: PadKind,
    pub at: Pt,
    /// The pad's copper (for a hole without copper, the hole).
    pub outline: Outline,
    /// The pad's own solder mask expansion, when it sets one.
    pub mask_margin: Option<f64>,
    pub layers: Vec<String>,
}

impl Pad {
    /// The pad has copper or a mask opening on the `side` ("F" or "B").
    pub fn on_side(&self, side: &str) -> bool {
        self.layers.iter().any(|l| {
            l == "*.Cu" || l == "*.Mask" || l == "F&B.Cu" || *l == format!("{side}.Cu") || *l == format!("{side}.Mask")
        })
    }

    pub fn has_copper(&self) -> bool {
        self.kind != PadKind::NpThruHole && self.layers.iter().any(|l| l.ends_with(".Cu"))
    }
}

/// One drawn item (a line, rectangle, circle, arc, polygon or curve) on one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Graphic {
    /// `fp_line`, `fp_rect`, `fp_circle`, `fp_arc`, `fp_poly` or `fp_curve`.
    pub kind: String,
    pub layer: String,
    pub width: f64,
    pub filled: bool,
    /// The points it is drawn through: a closed item's first point is not repeated.
    pub pts: Vec<Pt>,
    pub closed: bool,
    /// A circle's centre and radius.
    pub circle: Option<(Pt, f64)>,
    pub span: Range<usize>,
    /// The span of the width's number.
    pub width_span: Option<Range<usize>>,
}

impl Graphic {
    /// The straight pieces it is drawn with.
    pub fn segments(&self) -> Vec<(Pt, Pt)> {
        let n = self.pts.len();
        let count = if self.closed && n > 2 { n } else { n.saturating_sub(1) };
        (0..count).map(|i| (self.pts[i], self.pts[(i + 1) % n])).collect()
    }

    pub fn bbox(&self) -> Option<Rect> {
        Rect::of_points(self.pts.iter().copied())
    }

    /// Every segment is horizontal or vertical.
    pub fn axis_aligned(&self) -> bool {
        self.segments().iter().all(|(a, b)| (a.0 - b.0).abs() < EPS || (a.1 - b.1).abs() < EPS)
    }
}

/// A text on a layer: a `fp_text` or a shown property other than Reference and Value.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    /// `user`, `reference`, `value`, or the property's name.
    pub kind: String,
    pub text: String,
    pub layer: String,
    pub at: Pt,
    /// The `at` angle, as written.
    pub angle: f64,
    pub size: f64,
    pub hidden: bool,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attr {
    /// `smd` or `through_hole`; `None` is KiCad's "unspecified".
    pub kind: Option<String>,
    pub flags: Vec<String>,
    pub span: Option<Range<usize>>,
}

impl Attr {
    pub fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    /// As KiCad writes it: `(attr smd exclude_from_bom)`; `None` when there is nothing to write.
    pub fn render(&self) -> Option<String> {
        let mut words: Vec<&str> = self.kind.iter().map(String::as_str).collect();
        // KiCad's own order.
        for f in ["board_only", "exclude_from_pos_files", "exclude_from_bom", "allow_missing_courtyard", "dnp"] {
            if self.has(f) {
                words.push(f);
            }
        }
        words.extend(self.flags.iter().map(String::as_str).filter(|f| {
            !["board_only", "exclude_from_pos_files", "exclude_from_bom", "allow_missing_courtyard", "dnp"].contains(f)
        }));
        (!words.is_empty()).then(|| format!("(attr {})", words.join(" ")))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub path: String,
    /// The span of the quoted path.
    pub path_span: Range<usize>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub name: String,
    pub descr: String,
    pub tags: String,
    /// Placed on the bottom side.
    pub back: bool,
    pub attr: Attr,
    pub pads: Vec<Pad>,
    pub graphics: Vec<Graphic>,
    pub texts: Vec<Text>,
    pub models: Vec<Model>,
    /// The footprint's own solder mask expansion, when it sets one.
    pub mask_margin: Option<f64>,
    /// Where a new item goes: the start of the line of the first pad (else of the closing parenthesis).
    pub insert_at: usize,
    /// The span of the `(footprint ...)` list.
    pub span: Range<usize>,
}

fn num(n: Option<&Node>) -> Option<f64> {
    n.and_then(Node::as_atom).and_then(|s| s.parse().ok())
}

fn xy(n: &Node) -> Option<Pt> {
    Some((num(n.items().get(1))?, num(n.items().get(2))?))
}

fn yes(n: &Node, name: &str) -> bool {
    n.child(name).is_some_and(|c| c.arg(0).is_none_or(|v| v == "yes"))
}

fn hidden(n: &Node) -> bool {
    yes(n, "hide") || n.child("effects").is_some_and(|e| yes(e, "hide"))
}

fn circle3(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-12 {
        return None;
    }
    let (sa, sb, sc) = (a.0 * a.0 + a.1 * a.1, b.0 * b.0 + b.1 * b.1, c.0 * c.0 + c.1 * c.1);
    let ux = (sa * (b.1 - c.1) + sb * (c.1 - a.1) + sc * (a.1 - b.1)) / d;
    let uy = (sa * (c.0 - b.0) + sb * (a.0 - c.0) + sc * (b.0 - a.0)) / d;
    Some(((ux, uy), dist(a, (ux, uy))))
}

fn arc_points(a: Pt, m: Pt, b: Pt) -> Vec<Pt> {
    let Some((c, r)) = circle3(a, m, b) else { return vec![a, b] };
    let ang = |p: Pt| (p.1 - c.1).atan2(p.0 - c.0);
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

fn circle_points(c: Pt, r: f64) -> Vec<Pt> {
    (0..ARC_SEGMENTS)
        .map(|i| i as f64 * TAU / ARC_SEGMENTS as f64)
        .map(|t| (c.0 + r * t.cos(), c.1 + r * t.sin()))
        .collect()
}

pub const GRAPHICS: [&str; 6] = ["fp_line", "fp_rect", "fp_circle", "fp_arc", "fp_poly", "fp_curve"];

/// A graphic item, `None` for anything else or one without the points it needs.
pub fn graphic(g: &Node) -> Option<Graphic> {
    let kind = g.head().filter(|h| GRAPHICS.contains(h))?.to_string();
    let p = |name: &str| g.child(name).and_then(xy);
    let width_node = g.child("stroke").and_then(|s| s.child("width")).or_else(|| g.child("width"));
    let width = width_node.and_then(|w| num(w.items().get(1))).unwrap_or(0.0);
    let width_span = width_node.and_then(|w| w.items().get(1)).map(Node::span);
    let filled = g
        .child("fill")
        .is_some_and(|f| matches!(f.arg(0), Some("yes" | "solid")) || matches!(f.value("type"), Some("solid" | "yes")));
    let (pts, closed, circle) = match kind.as_str() {
        "fp_line" => (vec![p("start")?, p("end")?], false, None),
        "fp_rect" => {
            let (a, b) = (p("start")?, p("end")?);
            (vec![a, (b.0, a.1), b, (a.0, b.1)], true, None)
        }
        "fp_circle" => {
            let (c, e) = (p("center")?, p("end")?);
            let r = dist(c, e);
            (circle_points(c, r), true, Some((c, r)))
        }
        "fp_arc" => match (p("start"), p("mid"), p("end")) {
            (Some(a), Some(m), Some(b)) => (arc_points(a, m, b), false, None),
            (Some(a), None, Some(b)) => (vec![a, b], false, None),
            _ => return None,
        },
        _ => {
            let pts: Vec<Pt> = g.child("pts")?.children("xy").filter_map(xy).collect();
            let closed = kind == "fp_poly" && pts.len() > 2;
            (pts, closed, None)
        }
    };
    Some(Graphic {
        kind,
        layer: g.value("layer").unwrap_or("").to_string(),
        width,
        filled,
        pts,
        closed,
        circle,
        span: g.span(),
        width_span,
    })
}

/// The turn KiCad applies for an angle in degrees: counter-clockwise on screen, with y down.
pub fn turner(origin: Pt, degrees: f64) -> impl Fn(Pt) -> Pt {
    let (sin, cos) = degrees.to_radians().sin_cos();
    move |(x, y)| (origin.0 + x * cos + y * sin, origin.1 - x * sin + y * cos)
}

fn pad(n: &Node) -> Option<Pad> {
    let number = n.arg(0).unwrap_or("").to_string();
    let kind = match n.arg(1)? {
        "smd" => PadKind::Smd,
        "thru_hole" => PadKind::ThruHole,
        "np_thru_hole" => PadKind::NpThruHole,
        _ => PadKind::Connect,
    };
    let shape = n.arg(2).unwrap_or("rect");
    let at = n.child("at")?;
    let origin = xy(at)?;
    let angle = num(at.items().get(3)).unwrap_or(0.0);
    let turn = turner(origin, angle);
    let (w, h) = n.child("size").and_then(xy).unwrap_or((0.0, 0.0));
    let drill = n.child("drill").and_then(|d| {
        let nums: Vec<f64> = d.items().iter().skip(1).filter_map(|x| num(Some(x))).collect();
        let oval = d.arg(0) == Some("oval");
        match nums.as_slice() {
            [] => None,
            [d] => Some((*d, *d)),
            [dx, dy, ..] if oval => Some((*dx, *dy)),
            [d, ..] => Some((*d, *d)),
        }
    });
    let (w, h) = if kind == PadKind::NpThruHole { drill.unwrap_or((w, h)) } else { (w, h) };
    let capsule = |w: f64, h: f64| {
        let (r, half) = (w.min(h) / 2.0, (w - h).abs() / 2.0);
        let (a, b) = if w >= h { ((-half, 0.0), (half, 0.0)) } else { ((0.0, -half), (0.0, half)) };
        Outline::Capsule { a: turn(a), b: turn(b), r }
    };
    let rect = |w: f64, h: f64| {
        Outline::Poly(
            [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)].map(&turn).to_vec(),
        )
    };
    let outline = match (shape, kind) {
        (_, PadKind::NpThruHole) => capsule(w, h),
        ("circle" | "oval", _) => capsule(w, h),
        ("custom", _) => {
            let mut local = vec![(-w / 2.0, -h / 2.0), (w / 2.0, h / 2.0)];
            if let Some(prims) = n.child("primitives") {
                for g in prims.items().iter().skip(1).filter_map(graphic) {
                    let hw = g.width / 2.0;
                    local.extend(g.pts.iter().flat_map(|p| [(p.0 - hw, p.1 - hw), (p.0 + hw, p.1 + hw)]));
                }
            }
            if w > 0.0 && h > 0.0 && local.len() == 2 {
                rect(w, h)
            } else {
                Outline::Poly(hull(local.into_iter().map(&turn).collect()))
            }
        }
        _ => rect(w, h),
    };
    let layers = n
        .child("layers")
        .map(|l| l.items().iter().skip(1).filter_map(Node::as_atom).map(String::from).collect())
        .unwrap_or_default();
    let mask_margin = num(n.child("solder_mask_margin").and_then(|m| m.items().get(1)));
    Some(Pad { number, kind, at: origin, outline, mask_margin, layers })
}

fn text(n: &Node) -> Option<Text> {
    let (kind, text) = match n.head()? {
        "fp_text" => (n.arg(0)?.to_string(), n.arg(1)?.to_string()),
        "property" => (n.arg(0)?.to_string(), n.arg(1).unwrap_or("").to_string()),
        _ => return None,
    };
    let layer = n.value("layer")?.to_string();
    let at = n.child("at").and_then(xy).unwrap_or((0.0, 0.0));
    let angle = n.child("at").and_then(|a| num(a.items().get(3))).unwrap_or(0.0);
    let size = n
        .child("effects")
        .and_then(|e| e.child("font"))
        .and_then(|f| f.child("size"))
        .and_then(xy)
        .map_or(1.0, |(h, _)| h);
    Some(Text { kind, text, layer, at, angle, size, hidden: hidden(n), span: n.span() })
}

/// Reads a footprint file's text.
pub fn read(text_: &str) -> Result<Footprint, String> {
    let fp = sexpr::parse(text_).map_err(|e| e.to_string())?;
    if fp.head() != Some("footprint") {
        return Err("not a (footprint ...) file".into());
    }
    let mut out = Footprint {
        name: fp.arg(0).unwrap_or("").to_string(),
        descr: fp.value("descr").unwrap_or("").to_string(),
        tags: fp.value("tags").unwrap_or("").to_string(),
        back: fp.value("layer") == Some("B.Cu"),
        attr: Attr::default(),
        pads: Vec::new(),
        graphics: Vec::new(),
        texts: Vec::new(),
        models: Vec::new(),
        mask_margin: num(fp.child("solder_mask_margin").and_then(|m| m.items().get(1))),
        insert_at: line_start(text_, fp.span().end - 1),
        span: fp.span(),
    };
    let mut first_pad = None;
    for n in fp.items() {
        match n.head() {
            Some("attr") => {
                let words: Vec<String> = n.items().iter().skip(1).filter_map(Node::as_atom).map(String::from).collect();
                let kind = words.iter().find(|w| matches!(w.as_str(), "smd" | "through_hole")).cloned();
                let flags = words.into_iter().filter(|w| !matches!(w.as_str(), "smd" | "through_hole")).collect();
                out.attr = Attr { kind, flags, span: Some(n.span()) };
            }
            Some("pad") => {
                first_pad.get_or_insert(n.span().start);
                out.pads.extend(pad(n));
            }
            Some("fp_text" | "property") => out.texts.extend(text(n)),
            Some("model") => {
                if let Some(path) = n.items().get(1) {
                    out.models.push(Model {
                        path: path.as_atom().unwrap_or("").to_string(),
                        path_span: path.span(),
                        hidden: yes(n, "hide"),
                    });
                }
            }
            _ => out.graphics.extend(graphic(n)),
        }
    }
    if let Some(p) = first_pad {
        out.insert_at = line_start(text_, p);
    }
    Ok(out)
}

/// The start of the line `pos` is on.
pub fn line_start(text: &str, pos: usize) -> usize {
    text[..pos].rfind('\n').map_or(0, |i| i + 1)
}

impl Footprint {
    /// "F", or "B" for a bottom-side footprint.
    pub fn side(&self) -> &'static str {
        if self.back { "B" } else { "F" }
    }

    pub fn layer(&self, suffix: &str) -> String {
        format!("{}.{suffix}", self.side())
    }

    pub fn on<'a>(&'a self, layer: &'a str) -> impl Iterator<Item = &'a Graphic> + 'a {
        self.graphics.iter().filter(move |g| g.layer == layer)
    }

    /// The pads with copper on any layer.
    pub fn copper_pads(&self) -> impl Iterator<Item = &Pad> {
        self.pads.iter().filter(|p| p.has_copper())
    }

    /// The box round every pad's copper (and every hole).
    pub fn pads_box(&self) -> Option<Rect> {
        Rect::of_points(self.pads.iter().flat_map(|p| p.outline.points()))
    }

    /// The box round the drawing on the footprint's own fab layer, the strokes included.
    pub fn fab_box(&self) -> Option<Rect> {
        let fab = self.layer("Fab");
        let boxes: Vec<Rect> = self.on(&fab).filter_map(|g| g.bbox().map(|b| b.expand(g.width / 2.0))).collect();
        boxes.iter().copied().reduce(|a, b| a.union(&b))
    }

    /// Pin 1: the pad numbered `1` (else `A1`), the first of them when several share it.
    pub fn pin1(&self) -> Option<&Pad> {
        ["1", "A1"].iter().find_map(|n| self.copper_pads().find(|p| p.number == *n))
    }
}

/// A number as KiCad writes it: at most 4 decimals, no trailing zeros.
pub fn fmt(v: f64) -> String {
    let s = format!("{:.4}", (v * 10000.0).round() / 10000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

/// A new graphic item as KiCad 10 writes it, one tab in.
pub fn render_graphic(kind: &str, pts: &[Pt], width: f64, filled: bool, layer: &str) -> String {
    let stroke = format!("\t\t(stroke\n\t\t\t(width {})\n\t\t\t(type solid)\n\t\t)\n", fmt(width));
    let p = |name: &str, pt: Pt| format!("\t\t({name} {} {})\n", fmt(pt.0), fmt(pt.1));
    let body = match kind {
        "fp_line" => p("start", pts[0]) + &p("end", pts[1]) + &stroke,
        "fp_rect" => {
            p("start", pts[0])
                + &p("end", pts[1])
                + &stroke
                + &format!("\t\t(fill {})\n", if filled { "yes" } else { "no" })
        }
        "fp_circle" => {
            p("center", pts[0])
                + &p("end", pts[1])
                + &stroke
                + &format!("\t\t(fill {})\n", if filled { "yes" } else { "no" })
        }
        _ => {
            let xys: String = pts.iter().map(|&q| format!("\t\t\t(xy {} {})\n", fmt(q.0), fmt(q.1))).collect();
            format!("\t\t(pts\n{xys}\t\t)\n") + &stroke + &format!("\t\t(fill {})\n", if filled { "yes" } else { "no" })
        }
    };
    format!("\t({kind}\n{body}\t\t(layer {})\n\t)\n", sexpr::quote(layer))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP: &str = r#"(footprint "T"
	(version 20260206)
	(layer "F.Cu")
	(attr smd exclude_from_bom)
	(fp_rect (start -1 -0.5) (end 1 0.5) (stroke (width 0.1) (type solid)) (fill no) (layer "F.Fab"))
	(fp_circle (center 0 0) (end 0.2 0) (stroke (width 0.05) (type solid)) (fill yes) (layer "F.Fab"))
	(fp_arc (start -1 0) (mid 0 -1) (end 1 0) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
	(fp_text user "1" (at -1.5 0 0) (layer "F.SilkS") (effects (font (size 0.8 0.8))))
	(pad "1" smd roundrect (at -1 0 90) (size 0.6 0.4) (layers "F.Cu" "F.Mask" "F.Paste") (solder_mask_margin 0.05))
	(pad "2" thru_hole oval (at 1 0) (size 1 2) (drill 0.6) (layers "*.Cu" "*.Mask"))
	(pad "" np_thru_hole circle (at 0 2) (size 1 1) (drill 1) (layers "*.Cu" "*.Mask"))
	(model "${KICAD10_3DMODEL_DIR}/x.step" (hide yes))
)"#;

    #[test]
    fn reads_pads_graphics_texts_attrs_models() {
        let f = read(FP).unwrap();
        assert_eq!(f.attr.kind.as_deref(), Some("smd"));
        assert!(f.attr.has("exclude_from_bom"));
        assert_eq!(f.attr.render().unwrap(), "(attr smd exclude_from_bom)");
        assert_eq!(f.pads.len(), 3);
        let p1 = f.pin1().unwrap();
        // Turned 90°: 0.4 wide, 0.6 high.
        let b = p1.outline.bbox();
        assert!((b.w() - 0.4).abs() < 1e-9 && (b.h() - 0.6).abs() < 1e-9, "{b:?}");
        assert_eq!(p1.mask_margin, Some(0.05));
        assert!(matches!(f.pads[1].outline, Outline::Capsule { r, .. } if (r - 0.5).abs() < 1e-9));
        assert!(!f.pads[2].has_copper());
        assert_eq!(f.on("F.Fab").count(), 2);
        let circle = f.on("F.Fab").find(|g| g.kind == "fp_circle").unwrap();
        assert!(circle.filled && circle.circle.unwrap().1 == 0.2);
        let arc = f.on("F.SilkS").next().unwrap();
        assert!(arc.pts.iter().all(|p| p.1 <= 1e-9), "the arc bulges up through (0, -1)");
        assert_eq!(f.texts.iter().filter(|t| t.layer == "F.SilkS").count(), 1);
        assert_eq!(f.models[0].path, "${KICAD10_3DMODEL_DIR}/x.step");
        assert!(f.models[0].hidden);
        assert_eq!(&FP[f.insert_at..f.insert_at + 5], "\t(pad");
        let fab = f.fab_box().unwrap();
        assert!((fab.x0 + 1.05).abs() < 1e-9);
    }

    #[test]
    fn distances() {
        let sq = Outline::Poly(vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert_eq!(sq.distance((0.5, 0.5)), 0.0);
        assert!((sq.distance((2.0, 0.5)) - 1.0).abs() < 1e-12);
        assert!((sq.segment_distance((2.0, -1.0), (2.0, 2.0)) - 1.0).abs() < 1e-12);
        assert_eq!(sq.segment_distance((-1.0, 0.5), (2.0, 0.5)), 0.0);
        let c = Outline::Capsule { a: (0.0, 0.0), b: (0.0, 0.0), r: 0.5 };
        assert!((c.distance((1.0, 0.0)) - 0.5).abs() < 1e-12);
        assert_eq!(hull(vec![(0.0, 0.0), (1.0, 0.0), (0.5, 0.2), (1.0, 1.0), (0.0, 1.0)]).len(), 4);
    }

    #[test]
    fn renders_like_kicad() {
        let s = render_graphic("fp_line", &[(0.0, -1.25), (1.0, 0.0)], 0.12, false, "F.SilkS");
        assert_eq!(
            s,
            "\t(fp_line\n\t\t(start 0 -1.25)\n\t\t(end 1 0)\n\t\t(stroke\n\t\t\t(width 0.12)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.SilkS\")\n\t)\n"
        );
        assert!(read(&format!("(footprint \"x\"\n{s})")).unwrap().graphics.len() == 1);
        assert_eq!(fmt(-0.00001), "0");
        assert_eq!(fmt(1.23456), "1.2346");
    }
}
