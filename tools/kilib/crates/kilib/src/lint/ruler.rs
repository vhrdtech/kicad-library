//! A pin ruler on Ruler.F (User.31; Ruler.B, User.32, on the back): ticks and numbers beside the pads of a
//! fine-pitch part, so pin N is found by counting on the board.
//!
//! - The parts ([`family`]): QFP, QFN/DFN/SON, SOIC/TSSOP/SSOP and connectors with a counted pad row
//!   ([`keyed::counting_row`]: FFC/FPC, board-to-board) at [`Settings::connector_pitch_max_mm`] or finer, each with
//!   [`Settings::pins_min`] pins or more. Never a BGA, LGA or WLCSP.
//! - The pins: the pads numbered 1, 2, 3..., of the size most of them share (pin 1 whatever its shape); an exposed
//!   pad, its vias and mounting pads are no pins. Each pin points away from the pads' middle along its long side
//!   (a single row: away from the body); pins pointing the same way are one side.
//! - A tick on a pin's centre line, outside the side's toe line (the outer end of its pads' mask openings),
//!   [`Settings::gap_mm`] clear of every mask opening: never between pads. Pin 1, every 10th pin and the first and
//!   last pin of each side get a large tick and their number beyond it; pins 5, 15, 25... a small tick. Counting
//!   runs on around a corner, and the number by each corner pin means it never relies on turning one.
//! - A number is level, mirrored on the back, and keeps the gap from every mask opening and
//!   [`Settings::text_gap_mm`] from every other tick and number; one that has no room is left out (pin 1 and the
//!   side ends first, then the tens). A tick without room is left out with its number.
//!
//! The layer belongs to this rule: whatever else is there is replaced. The library never draws the ruler on the
//! silkscreen (KiCad has no per-part switch for it); [`board`] copies it onto a board's silkscreen and takes the
//! copy off again, the numbers at [`Settings::silk_text_height_mm`].

use serde::Deserialize;

use super::pins::r3;
use super::{Class, Input, Outcome, delete, insert, keyed};
use crate::footprint::{self, Footprint, Outline, Pt, Rect, fmt, render_graphic, render_text, segments_distance};
use crate::layers::{RULER, side_of};
use crate::migrate::{Edit, apply_edits};
use crate::sexpr::{self, Node};
use crate::stroke::ink;

/// The settings, from the checkout's `generate/ruler.toml`, built in.
const SETTINGS: &str = include_str!("../../../../../../generate/ruler.toml");

/// Words in a footprint's name or library that keep the ruler off: area-array packages have no pad rows.
pub const EXCLUDED: [&str; 4] = ["BGA", "LGA", "WLCSP", "CSP"];

/// Pads within this of each other's size are the same size.
const SAME_SIZE: f64 = 0.02;
/// A pad from a third to this many times the common pad's area is a pin's.
const PIN_AREA_MAX: f64 = 3.0;
/// Rounding of the anchors and tick ends to 0.001 mm may take this off a clearance.
const ROUNDING: f64 = 2e-3;

/// Sizes and gaps of the ruler (`generate/ruler.toml`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Ticks and numbers keep this from every solder mask opening.
    pub gap_mm: f64,
    pub tick_width_mm: f64,
    /// Pins 5, 15, 25...
    pub tick_small_mm: f64,
    /// Pin 1, every 10th pin, each side's end pins.
    pub tick_large_mm: f64,
    pub text_height_mm: f64,
    pub text_stroke_mm: f64,
    /// The numbers of the silkscreen copy on a board.
    pub silk_text_height_mm: f64,
    pub silk_text_stroke_mm: f64,
    /// From a tick's end to its number, and between a number and any other tick or number.
    pub text_gap_mm: f64,
    /// Parts with fewer pins get no ruler.
    pub pins_min: usize,
    /// The coarsest pitch of a connector's pad row that gets one.
    pub connector_pitch_max_mm: f64,
}

impl Settings {
    /// The library's settings, `generate/ruler.toml`.
    pub fn library() -> Settings {
        toml::from_str(SETTINGS).expect("generate/ruler.toml parses: tests::the_settings_parse")
    }
}

/// The kinds of part that get a ruler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    /// FFC/FPC and board-to-board connectors: a counted pad row.
    Connector,
    Qfp,
    /// QFN, DFN and SON.
    Qfn,
    /// SOIC, SSOP, TSSOP, MSOP.
    So,
}

impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Family::Connector => "connector",
            Family::Qfp => "QFP",
            Family::Qfn => "QFN/DFN",
            Family::So => "SO",
        }
    }
}

/// The family of footprint `fp` in library `lib`, by library and name; `None` for a part without a ruler.
pub fn family(lib: &str, fp: &Footprint) -> Option<Family> {
    let lib = lib.strip_prefix("VHRD_").unwrap_or(lib);
    let name = fp.name.to_ascii_uppercase();
    let lib_upper = lib.to_ascii_uppercase();
    if EXCLUDED.iter().any(|k| name.contains(k) || lib_upper.contains(k)) {
        return None;
    }
    if keyed::connector_like(lib) {
        return keyed::counting_row(fp).then_some(Family::Connector);
    }
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if lib == "Package_QFP" || has(&["QFP"]) {
        Some(Family::Qfp)
    } else if matches!(lib, "Package_DFN_QFN" | "Package_SON") || has(&["QFN", "DFN", "SON"]) {
        Some(Family::Qfn)
    } else if lib == "Package_SO" || has(&["SOIC", "SOP", "SO-"]) {
        Some(Family::So)
    } else {
        None
    }
}

/// Outward directions: right, left, down, up.
const DIRS: [Pt; 4] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];

/// A pin: its number, copper box, mask margin and the direction it points (an index into [`DIRS`]).
#[derive(Debug, Clone, Copy)]
struct Pin {
    n: u32,
    rect: Rect,
    margin: f64,
    dir: usize,
}

/// A mask opening: a pad's copper (or hole) and how far the opening reaches past it.
pub(crate) type Opening = (Outline, f64);

fn margin(fp: &Footprint, p: &footprint::Pad) -> f64 {
    p.mask_margin.or(fp.mask_margin).unwrap_or(0.0).max(0.0)
}

/// Every mask opening on the footprint's own side.
fn openings(fp: &Footprint) -> Vec<Opening> {
    fp.pads
        .iter()
        .filter(|p| p.on_side(fp.side()) && p.layers.iter().any(|l| l.ends_with(".Mask")))
        .map(|p| (p.outline.clone(), margin(fp, p)))
        .collect()
}

fn size_key(r: &Rect) -> Pt {
    (r.w().min(r.h()), r.w().max(r.h()))
}

fn same_size(a: Pt, b: Pt) -> bool {
    (a.0 - b.0).abs() < SAME_SIZE && (a.1 - b.1).abs() < SAME_SIZE
}

/// The direction a pad at `r` points: along its long side (the farther way from the middle for a square pad),
/// away from `middle`; for a pad in line with the middle (a single row), away from `body`; else up.
fn outward(r: &Rect, middle: Pt, body: Pt, span: &Rect) -> usize {
    let (cx, cy) = r.center();
    let along_x = if r.w() > r.h() * 1.2 {
        true
    } else if r.h() > r.w() * 1.2 {
        false
    } else {
        (cx - middle.0).abs() / span.w().max(1e-6) >= (cy - middle.1).abs() / span.h().max(1e-6)
    };
    let (d, len, from_body) =
        if along_x { (cx - middle.0, r.w(), cx - body.0) } else { (cy - middle.1, r.h(), cy - body.1) };
    let s = if d.abs() >= len * 0.25 {
        d
    } else if from_body.abs() >= 1e-3 {
        from_body
    } else {
        -1.0
    };
    match (along_x, s > 0.0) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// The pins, by number: the numbered copper pads of the size most share, pin 1 whatever its size, the first pad
/// of each number.
fn pins(fp: &Footprint) -> Vec<Pin> {
    let numbered: Vec<(u32, &footprint::Pad)> =
        fp.copper_pads().filter_map(|p| Some((p.number.parse::<u32>().ok().filter(|&n| n > 0)?, p))).collect();
    let mut sizes: Vec<(Pt, usize)> = Vec::new();
    for (_, p) in &numbered {
        let k = size_key(&p.outline.bbox());
        match sizes.iter_mut().find(|(s, _)| same_size(*s, k)) {
            Some((_, c)) => *c += 1,
            None => sizes.push((k, 1)),
        }
    }
    let Some(&(common, _)) = sizes.iter().max_by_key(|(_, c)| *c) else { return Vec::new() };
    // A pin's pads are about the common pad's size (a corner pad may differ); an exposed pad is far larger, and its
    // thermal vias sit inside it.
    let area = common.0 * common.1;
    let pin_like = |r: &Rect| r.w() * r.h() <= area * PIN_AREA_MAX && r.w() * r.h() >= area / PIN_AREA_MAX;
    let large: Vec<Rect> =
        numbered.iter().map(|(_, p)| p.outline.bbox()).filter(|r| r.w() * r.h() > area * PIN_AREA_MAX).collect();
    let mut chosen: Vec<(u32, Rect, f64)> = Vec::new();
    for (n, p) in &numbered {
        let r = p.outline.bbox();
        if !(*n == 1 || pin_like(&r)) || large.iter().any(|l| l.contains(r.center())) {
            continue;
        }
        // A pin drawn as several pads (a footprint for two packages) takes all of them.
        match chosen.iter_mut().find(|(k, _, _)| k == n) {
            Some((_, rect, m)) => {
                *rect = rect.union(&r);
                *m = m.max(margin(fp, p));
            }
            None => chosen.push((*n, r, margin(fp, p))),
        }
    }
    chosen.sort_by_key(|(n, _, _)| *n);
    let Some(span) = chosen.iter().map(|(_, r, _)| *r).reduce(|a, b| a.union(&b)) else { return Vec::new() };
    let middle = span.center();
    let body = fp.fab_box().or_else(|| fp.pads_box()).map_or(middle, |b| b.center());
    chosen
        .into_iter()
        .map(|(n, rect, margin)| Pin { n, rect, margin, dir: outward(&rect, middle, body, &span) })
        .collect()
}

/// A tick: the pin, its ends (the pad end first), and whether it is a large one, with its number wanted.
#[derive(Debug, Clone, PartialEq)]
struct Tick {
    n: u32,
    a: Pt,
    b: Pt,
    numbered: bool,
}

/// A number as it is written: text, anchor, size, stroke, and the ink's box.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Number {
    pub text: String,
    pub at: Pt,
    pub size: f64,
    pub stroke: f64,
    pub rect: Rect,
}

/// What [`plan`] makes of a footprint.
#[derive(Debug, Clone, PartialEq)]
struct Plan {
    family: Family,
    ticks: Vec<(Pt, Pt)>,
    numbers: Vec<Number>,
    /// Pins whose tick had no room.
    ticks_out: Vec<u32>,
    /// Numbers that had no room.
    numbers_out: Vec<String>,
}

fn along(p: Pt, d: Pt) -> f64 {
    p.0 * d.0 + p.1 * d.1
}

/// The point `s` out along `d` and `across` on the other axis.
fn point(d: Pt, s: f64, across: f64) -> Pt {
    if d.0 != 0.0 { (r3(s * d.0), r3(across)) } else { (r3(across), r3(s * d.1)) }
}

fn rect_corners(r: &Rect) -> [Pt; 4] {
    [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)]
}

/// How far a segment is from the box `r`, 0 when it touches or runs inside.
fn segment_to_rect(a: Pt, b: Pt, r: &Rect) -> f64 {
    if r.contains(a) || r.contains(b) {
        return 0.0;
    }
    let c = rect_corners(r);
    (0..4).map(|k| segments_distance(a, b, c[k], c[(k + 1) % 4])).fold(f64::MAX, f64::min)
}

/// How far a mask opening is from the box `r`, 0 when they touch.
fn opening_to_rect(o: &Opening, r: &Rect) -> f64 {
    let (outline, m) = o;
    if r.contains(outline.bbox().center()) {
        return 0.0;
    }
    let c = rect_corners(r);
    let d = (0..4).map(|k| outline.segment_distance(c[k], c[(k + 1) % 4])).fold(f64::MAX, f64::min);
    (d - m).max(0.0)
}

/// The tick of each pin that gets one, with the pins whose tick has no room.
fn ticks(pins: &[Pin], open: &[Opening], s: &Settings) -> (Vec<Tick>, Vec<u32>) {
    let half = s.tick_width_mm / 2.0;
    let mut out = Vec::new();
    let mut left_out = Vec::new();
    for (k, &d) in DIRS.iter().enumerate() {
        let side: Vec<&Pin> = pins.iter().filter(|p| p.dir == k).collect();
        let (Some(first), Some(last)) = (side.iter().map(|p| p.n).min(), side.iter().map(|p| p.n).max()) else {
            continue;
        };
        // The toe line: the outer end of the side's mask openings.
        let toe = side
            .iter()
            .flat_map(|p| rect_corners(&p.rect.expand(p.margin)).map(|c| along(c, d)))
            .fold(f64::MIN, f64::max);
        let start = toe + s.gap_mm + half;
        for p in side {
            let numbered = p.n == 1 || p.n == first || p.n == last || p.n % 10 == 0;
            if !numbered && p.n % 5 != 0 {
                continue;
            }
            let len = if numbered { s.tick_large_mm } else { s.tick_small_mm };
            let (cx, cy) = p.rect.center();
            let across = if d.0 != 0.0 { cy } else { cx };
            let (a, b) = (point(d, start, across), point(d, start + len, across));
            let clear = open.iter().all(|(o, m)| o.segment_distance(a, b) - m >= s.gap_mm + half - ROUNDING);
            if clear {
                out.push(Tick { n: p.n, a, b, numbered });
            } else {
                left_out.push(p.n);
            }
        }
    }
    out.sort_by_key(|t| t.n);
    left_out.sort_unstable();
    (out, left_out)
}

/// The number `text` beyond a tick ending at `end` that points along `d`, at `size`.
fn number_at(text: &str, end: Pt, d: Pt, size: f64, stroke: f64, gap: f64, back: bool) -> Option<Number> {
    let i = ink(text, size, size, stroke)?;
    let i = if back { i.mirrored() } else { i };
    let (cx, cy) = i.centre();
    let at = match DIRS.iter().position(|&x| x == d)? {
        0 => (end.0 + gap - i.x0, end.1 - cy),
        1 => (end.0 - gap - i.x1, end.1 - cy),
        2 => (end.0 - cx, end.1 + gap - i.y0),
        _ => (end.0 - cx, end.1 - gap - i.y1),
    };
    let at = (r3(at.0), r3(at.1));
    let rect = Rect { x0: at.0 + i.x0, y0: at.1 + i.y0, x1: at.0 + i.x1, y1: at.1 + i.y1 };
    Some(Number { text: text.to_owned(), at, size, stroke, rect })
}

/// The unit direction from `a` to `b`, snapped to the nearest of [`DIRS`].
fn direction(a: Pt, b: Pt) -> Pt {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 { DIRS[0] } else { DIRS[1] }
    } else if dy >= 0.0 {
        DIRS[2]
    } else {
        DIRS[3]
    }
}

/// The numbers that fit, in the order of `wanted` (pad end, outer end, text of each tick), and those that do not:
/// each keeps the gap from every mask opening and the text gap from every tick and placed number.
pub(crate) fn numbers(
    wanted: &[(Pt, Pt, String)],
    all_ticks: &[(Pt, Pt)],
    open: &[Opening],
    size: f64,
    stroke: f64,
    s: &Settings,
    back: bool,
) -> (Vec<Number>, Vec<String>) {
    let half = s.tick_width_mm / 2.0;
    let mut placed: Vec<Number> = Vec::new();
    let mut left_out = Vec::new();
    for (a, b, text) in wanted {
        let d = direction(*a, *b);
        let fits = |n: &Number| {
            open.iter().all(|o| opening_to_rect(o, &n.rect) >= s.gap_mm - ROUNDING)
                && all_ticks.iter().all(|(p, q)| segment_to_rect(*p, *q, &n.rect) >= s.text_gap_mm + half - ROUNDING)
                && placed.iter().all(|m| {
                    let g = n.rect.expand(s.text_gap_mm - ROUNDING);
                    !(g.x0 < m.rect.x1 && m.rect.x0 < g.x1 && g.y0 < m.rect.y1 && m.rect.y0 < g.y1)
                })
        };
        match number_at(text, *b, d, size, stroke, half + s.text_gap_mm, back) {
            Some(n) if fits(&n) => placed.push(n),
            _ => left_out.push(text.clone()),
        }
    }
    (placed, left_out)
}

/// The ruler of a footprint, or `None` for a part without one.
fn plan(lib: &str, fp: &Footprint, s: &Settings) -> Option<Plan> {
    let family = family(lib, fp)?;
    let pins = pins(fp);
    if pins.len() < s.pins_min {
        return None;
    }
    if family == Family::Connector {
        let pitch = pins
            .iter()
            .enumerate()
            .flat_map(|(k, p)| pins[k + 1..].iter().map(move |q| footprint::dist(p.rect.center(), q.rect.center())))
            .fold(f64::MAX, f64::min);
        if pitch > s.connector_pitch_max_mm + 1e-3 {
            return None;
        }
    }
    let open = openings(fp);
    let (ticks, ticks_out) = ticks(&pins, &open, s);
    let lines: Vec<(Pt, Pt)> = ticks.iter().map(|t| (t.a, t.b)).collect();
    // Pin 1 and the side ends first, then the tens.
    let mut wanted: Vec<&Tick> = ticks.iter().filter(|t| t.numbered).collect();
    wanted.sort_by_key(|t| (t.n != 1, t.n % 10 == 0, t.n));
    let wanted: Vec<(Pt, Pt, String)> = wanted.iter().map(|t| (t.a, t.b, t.n.to_string())).collect();
    let (mut numbers, numbers_out) = numbers(&wanted, &lines, &open, s.text_height_mm, s.text_stroke_mm, s, fp.back);
    numbers.sort_by_key(|n| n.text.parse::<u32>().unwrap_or(0));
    Some(Plan { family, ticks: lines, numbers, ticks_out, numbers_out })
}

fn near(a: Pt, b: Pt) -> bool {
    (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let s = Settings::library();
    let layer = side_of(RULER, i.fp.back);
    let plan = if i.class == Class::Part { plan(i.lib, i.fp, &s) } else { None };
    let lines: Vec<&footprint::Graphic> = i.fp.on(layer).collect();
    let texts: Vec<&footprint::Text> = i.fp.texts.iter().filter(|t| t.layer == layer).collect();
    let same = match &plan {
        None => lines.is_empty() && texts.is_empty(),
        Some(p) => {
            lines.len() == p.ticks.len()
                && texts.len() == p.numbers.len()
                && p.ticks.iter().all(|(a, b)| {
                    lines.iter().any(|g| {
                        g.kind == "fp_line"
                            && g.pts.len() == 2
                            && near(g.pts[0], *a)
                            && near(g.pts[1], *b)
                            && (g.width - s.tick_width_mm).abs() < 1e-6
                    })
                })
                && p.numbers.iter().all(|n| {
                    texts.iter().any(|t| {
                        t.text == n.text && near(t.at, n.at) && t.angle == 0.0 && (t.size - n.size).abs() < 1e-6
                    })
                })
        }
    };
    if same {
        return out;
    }
    let have = lines.len() + texts.len();
    let had = if have == 0 { String::new() } else { format!(" (replacing {have})") };
    match &plan {
        None => out.fixed(format!("{layer}: no ruler for this footprint{had}")),
        Some(p) => {
            let mut missing = Vec::new();
            if !p.ticks_out.is_empty() {
                let pins: Vec<String> = p.ticks_out.iter().map(u32::to_string).collect();
                missing.push(format!("no room for the ticks of {}", pins.join(", ")));
            }
            if !p.numbers_out.is_empty() {
                missing.push(format!("no room for the numbers {}", p.numbers_out.join(", ")));
            }
            let missing = if missing.is_empty() { String::new() } else { format!("; {}", missing.join("; ")) };
            out.fixed(format!(
                "{layer}: ruler ({}), {} ticks, {} numbers{missing}{had}",
                p.family.label(),
                p.ticks.len(),
                p.numbers.len()
            ));
        }
    }
    for g in &lines {
        out.edits.push(delete(i.text, &g.span));
    }
    for t in &texts {
        out.edits.push(delete(i.text, &t.span));
    }
    if let Some(p) = &plan {
        let mut add: String =
            p.ticks.iter().map(|(a, b)| render_graphic("fp_line", &[*a, *b], s.tick_width_mm, false, layer)).collect();
        for n in &p.numbers {
            add += &render_text(&n.text, n.at, 0.0, n.size, n.stroke, layer, i.fp.back);
        }
        out.edits.push(insert(i.fp, add));
    }
    out
}

/// What [`board`] does to one footprint on a board.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardSilk {
    /// The designator.
    pub reference: String,
    /// Silkscreen copies taken off.
    pub removed: usize,
    /// Ticks and numbers copied onto the silkscreen.
    pub ticks: usize,
    pub numbers: usize,
    /// Numbers of the ruler that have no room on the silkscreen at its text size.
    pub left_out: Vec<String>,
}

/// One drawn item of a board's footprint: its node's span, and what it is.
enum Item {
    Line { span: std::ops::Range<usize>, a: Pt, b: Pt, width: f64 },
    Text { span: std::ops::Range<usize>, text: String, at: Pt, angle: f64, size: f64 },
}

fn items_on(fp: &Node, layer: &str) -> Vec<Item> {
    let num =
        |n: Option<&Node>, k: usize| n.and_then(|n| n.items().get(k)).and_then(Node::as_atom)?.parse::<f64>().ok();
    let mut out = Vec::new();
    for n in fp.items().iter().filter(|n| n.value("layer") == Some(layer)) {
        match n.head() {
            Some("fp_line") => {
                let (start, end) = (n.child("start"), n.child("end"));
                let width = num(n.child("stroke").and_then(|s| s.child("width")), 1).unwrap_or(0.0);
                if let (Some(x0), Some(y0), Some(x1), Some(y1)) =
                    (num(start, 1), num(start, 2), num(end, 1), num(end, 2))
                {
                    out.push(Item::Line { span: n.span(), a: (x0, y0), b: (x1, y1), width });
                }
            }
            Some("fp_text") if n.arg(0) == Some("user") => {
                let at = n.child("at");
                let size = num(n.child("effects").and_then(|e| e.child("font")).and_then(|f| f.child("size")), 1);
                if let (Some(text), Some(x), Some(y), Some(size)) = (n.arg(1), num(at, 1), num(at, 2), size) {
                    let angle = num(at, 3).unwrap_or(0.0);
                    out.push(Item::Text { span: n.span(), text: text.to_owned(), at: (x, y), angle, size });
                }
            }
            _ => {}
        }
    }
    out
}

/// Shifts every line of `block` one tab further in: a footprint's items sit two tabs in on a board.
fn indent(block: &str) -> String {
    block.lines().map(|l| format!("\t{l}\n")).collect()
}

/// The silkscreen copy one board footprint wants: the ruler's ticks, and its numbers at the silkscreen's text size
/// that fit, placed beyond their ticks as in the library.
fn silk_copy(fp_node: &Node, text: &str, back: bool, angle: f64, s: &Settings) -> Result<(String, BoardSilk), String> {
    let fp = footprint::read_placed(&text[fp_node.span()], angle)?;
    let ruler = items_on(fp_node, side_of(RULER, back));
    let silk = if back { "B.SilkS" } else { "F.SilkS" };
    let lines: Vec<(Pt, Pt, f64)> = ruler
        .iter()
        .filter_map(|i| match i {
            Item::Line { a, b, width, .. } => Some((*a, *b, *width)),
            Item::Text { .. } => None,
        })
        .collect();
    let segs: Vec<(Pt, Pt)> = lines.iter().map(|(a, b, _)| (*a, *b)).collect();
    // Each number's tick: the one whose outer end is nearest it.
    let mut wanted = Vec::new();
    let mut angles = Vec::new();
    for i in &ruler {
        let Item::Text { text, at, angle, .. } = i else { continue };
        let tick = segs.iter().min_by(|x, y| {
            let d = |(a, b): &(Pt, Pt)| footprint::dist(*a, *at).min(footprint::dist(*b, *at));
            d(x).total_cmp(&d(y))
        });
        let Some(&(a, b)) = tick else { continue };
        let (a, b) = if footprint::dist(a, *at) > footprint::dist(b, *at) { (a, b) } else { (b, a) };
        wanted.push((a, b, text.clone()));
        angles.push((text.clone(), *angle));
    }
    let (placed, left_out) =
        numbers(&wanted, &segs, &openings(&fp), s.silk_text_height_mm, s.silk_text_stroke_mm, s, back);
    let mut block: String =
        lines.iter().map(|(a, b, w)| render_graphic("fp_line", &[*a, *b], *w, false, silk)).collect();
    for n in &placed {
        let angle = angles.iter().find(|(t, _)| *t == n.text).map_or(0.0, |(_, a)| *a);
        block += &render_text(&n.text, n.at, angle, n.size, n.stroke, silk, back);
    }
    let report = BoardSilk { ticks: lines.len(), numbers: placed.len(), left_out, ..BoardSilk::default() };
    Ok((indent(&block), report))
}

/// The silkscreen items of a board footprint that copy its ruler: lines on the ruler's ticks, numbers of the
/// ruler's texts at the silkscreen size within a tick length of their ruler text.
fn copies(fp_node: &Node, back: bool, s: &Settings) -> Vec<Item> {
    let ruler = items_on(fp_node, side_of(RULER, back));
    let silk = if back { "B.SilkS" } else { "F.SilkS" };
    let reach = s.tick_large_mm + s.silk_text_height_mm * 2.0;
    items_on(fp_node, silk)
        .into_iter()
        .filter(|i| match i {
            Item::Line { a, b, .. } => {
                ruler.iter().any(|r| matches!(r, Item::Line { a: p, b: q, .. } if near(*a, *p) && near(*b, *q)))
            }
            Item::Text { text, at, size, .. } => (size - s.silk_text_height_mm).abs() < 1e-6
                && ruler.iter().any(
                    |r| matches!(r, Item::Text { text: t, at: p, .. } if t == text && footprint::dist(*at, *p) < reach),
                ),
        })
        .collect()
}

/// The same items, by what they draw (not where they sit in the file).
fn same_items(a: &[Item], b: &[Item]) -> bool {
    let key = |i: &Item| match i {
        Item::Line { a, b, width, .. } => {
            format!("L {} {} {} {} {}", fmt(a.0), fmt(a.1), fmt(b.0), fmt(b.1), fmt(*width))
        }
        Item::Text { text, at, angle, size, .. } => {
            format!("T {text} {} {} {} {}", fmt(at.0), fmt(at.1), fmt(*angle), fmt(*size))
        }
    };
    let mut x: Vec<String> = a.iter().map(key).collect();
    let mut y: Vec<String> = b.iter().map(key).collect();
    x.sort();
    y.sort();
    x == y
}

/// A board with the silkscreen copy of each footprint's ruler made anew (`silk`) or taken off, and what happened
/// to each footprint with a ruler; only those footprints' silkscreen items change. Running it again changes
/// nothing, also after KiCad's "Update Footprints from Library" dropped the copies.
pub fn board(text: &str, silk: bool) -> Result<(String, Vec<BoardSilk>), String> {
    let s = Settings::library();
    let pcb = sexpr::parse(text).map_err(|e| e.to_string())?;
    if pcb.head() != Some("kicad_pcb") {
        return Err("not a (kicad_pcb ...) file".into());
    }
    let mut edits = Vec::new();
    let mut out = Vec::new();
    for fp in pcb.children("footprint") {
        let back = fp.value("layer") == Some("B.Cu");
        if items_on(fp, side_of(RULER, back)).is_empty() && copies(fp, back, &s).is_empty() {
            continue;
        }
        let angle = fp
            .child("at")
            .and_then(|a| a.items().get(3))
            .and_then(Node::as_atom)
            .and_then(|a| a.parse::<f64>().ok())
            .unwrap_or(0.0);
        let reference = fp
            .children("property")
            .find(|p| p.arg(0) == Some("Reference"))
            .and_then(|p| p.arg(1))
            .unwrap_or("?")
            .to_owned();
        let have = copies(fp, back, &s);
        let (block, mut report) = if silk {
            let (block, report) = silk_copy(fp, text, back, angle, &s).map_err(|e| format!("{reference}: {e}"))?;
            (block, report)
        } else {
            (String::new(), BoardSilk::default())
        };
        report.reference = reference;
        // Unchanged when the copies already are what would be written.
        let new_items = if block.is_empty() {
            Vec::new()
        } else {
            let wrapped = format!("(footprint \"x\"\n{block})");
            let node = sexpr::parse(&wrapped).map_err(|e| e.to_string())?;
            items_on(&node, if back { "B.SilkS" } else { "F.SilkS" })
        };
        if same_items(&have, &new_items) {
            continue;
        }
        report.removed = have.len();
        for i in &have {
            let span = match i {
                Item::Line { span, .. } | Item::Text { span, .. } => span,
            };
            edits.push(delete(text, span));
        }
        if !block.is_empty() {
            let at = footprint::line_start(text, fp.span().end - 1);
            edits.push(Edit { range: at..at, text: block });
        }
        out.push(report);
    }
    edits.sort_by_key(|e| (e.range.start, e.range.end));
    Ok((apply_edits(text, &edits), out))
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};
    use super::*;

    /// A pad row part: `n` pins at `pitch`, counted down the left side and up the right one (a SOIC), pads 1.5
    /// long with `extra` added to each pad.
    fn so(n: usize, pitch: f64, extra: &str) -> String {
        (0..n)
            .map(|k| {
                let (x, y) = if k < n / 2 { (-2.5, k as f64 * pitch) } else { (2.5, (n - 1 - k) as f64 * pitch) };
                format!(
                    "(pad \"{}\" smd rect (at {x} {y}) (size 1.5 {}) (layers \"F.Cu\" \"F.Mask\"){extra})\n",
                    k + 1,
                    pitch * 0.6
                )
            })
            .collect()
    }

    /// A QFP: `per` pins on each side at `pitch`, counted round anticlockwise from the top of the left side, and an
    /// exposed pad numbered after the last pin.
    fn qfp(per: usize, pitch: f64) -> String {
        let half = (per - 1) as f64 * pitch / 2.0;
        let edge = half + 1.5;
        let mut out = String::new();
        for k in 0..per * 4 {
            let (side, i) = (k / per, (k % per) as f64 * pitch - half);
            let (x, y, w, h) = match side {
                0 => (-edge, i, 1.2, pitch * 0.55),
                1 => (i, edge, pitch * 0.55, 1.2),
                2 => (edge, -i, 1.2, pitch * 0.55),
                _ => (-i, -edge, pitch * 0.55, 1.2),
            };
            out += &format!("(pad \"{}\" smd rect (at {x} {y}) (size {w} {h}) (layers \"F.Cu\" \"F.Mask\"))\n", k + 1);
        }
        out + &format!("(pad \"{}\" smd rect (at 0 0) (size 3 3) (layers \"F.Cu\" \"F.Mask\"))\n", per * 4 + 1)
    }

    type Drawn = (Vec<(Pt, Pt)>, Vec<(String, Pt)>);

    fn ruler(text: &str, layer: &str) -> Drawn {
        let f = footprint::read(text).unwrap();
        let lines = f.on(layer).map(|g| (g.pts[0], g.pts[1])).collect();
        let texts = f.texts.iter().filter(|t| t.layer == layer).map(|t| (t.text.clone(), t.at)).collect();
        (lines, texts)
    }

    fn texts(text: &str) -> Vec<String> {
        ruler(text, "User.31").1.into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn the_settings_parse() {
        let s = Settings::library();
        assert_eq!((s.gap_mm, s.text_height_mm, s.tick_width_mm, s.silk_text_height_mm), (0.4, 0.5, 0.1, 0.8));
    }

    #[test]
    fn ticks_sit_on_the_pad_centres_beyond_the_toe_line_pointing_away() {
        let t = fp("SOIC-16", &so(16, 1.27, ""));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Ruler);
        let new = new.unwrap();
        assert_eq!(f[0].message, "User.31: ruler (SO), 7 ticks, 5 numbers", "{f:?}");
        let s = Settings::library();
        let (lines, _) = ruler(&new, "User.31");
        // Left pads end at x -3.25: ticks start 0.4 + 0.05 out and run left; right ones mirror them.
        let left: Vec<&(Pt, Pt)> = lines.iter().filter(|(a, _)| a.0 < 0.0).collect();
        assert_eq!(left.len(), 3, "pins 1, 5 and 8: {lines:?}");
        for (a, b) in &lines {
            assert!((a.1 - b.1).abs() < 1e-9, "level: {a:?} {b:?}");
            assert!((a.0.abs() - (3.25 + s.gap_mm + 0.05)).abs() < 1e-6, "{a:?}");
            assert!(b.0.abs() > a.0.abs(), "points away from the pads: {a:?} {b:?}");
            // On a pad's centre line, never between pads.
            assert!(((a.1 / 1.27) - (a.1 / 1.27).round()).abs() < 1e-6, "{a:?}");
        }
        let len = |y: f64, left: bool| {
            let (a, b) = lines.iter().find(|(a, _)| (a.1 - y).abs() < 1e-6 && (a.0 < 0.0) == left).unwrap();
            ((b.0 - a.0).abs() * 1000.0).round() / 1000.0
        };
        // Pin 1 large, pin 5 small, pin 8 (the left side's end) large; on the right pin 9 (its start) and 10 large,
        // 15 small.
        assert_eq!((len(0.0, true), len(4.0 * 1.27, true), len(7.0 * 1.27, true)), (0.8, 0.4, 0.8));
        assert_eq!((len(7.0 * 1.27, false), len(6.0 * 1.27, false), len(1.27, false)), (0.8, 0.8, 0.4));
        assert_eq!(texts(&new), ["1", "8", "9", "10", "16"].map(String::from));
        settled("VHRD_Package_SO", &t, Rule::Ruler);
    }

    #[test]
    fn counting_runs_on_round_the_corners_with_a_number_at_each_corner_pin() {
        let t = fp("LQFP-48_7x7mm_P0.5mm", &qfp(12, 0.5));
        let new = run("VHRD_Package_QFP", &t, Rule::Ruler).0.unwrap();
        let mut got: Vec<u32> = texts(&new).iter().map(|t| t.parse().unwrap()).collect();
        got.sort_unstable();
        // Each side's ends (1, 12, 13, 24, 25, 36, 37, 48) and the tens; the exposed pad (49) is no pin.
        assert_eq!(got, [1, 10, 12, 13, 20, 24, 25, 30, 36, 37, 40, 48]);
        let (lines, _) = ruler(&new, "User.31");
        assert_eq!(lines.len(), 16, "12 numbered and 5, 15, 25, 35, 45 less those at a side's end: {lines:?}");
        // Pin 13 starts the bottom side: its tick points down.
        let (_, at) = ruler(&new, "User.31").1.into_iter().find(|(t, _)| t == "13").unwrap();
        assert!(at.1 > 3.0 + 1.2, "{at:?}");
    }

    #[test]
    fn ticks_and_numbers_keep_the_gap_from_every_mask_opening() {
        // A 0.1 mm mask margin on every pad, and a hole beside the row.
        let extra = " (solder_mask_margin 0.1)";
        let hole =
            "(pad \"\" np_thru_hole circle (at -4.6 2.54) (size 0.8 0.8) (drill 0.8) (layers \"*.Cu\" \"*.Mask\"))\n";
        let t = fp("SOIC-16", &format!("{}{hole}", so(16, 1.27, extra)));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Ruler);
        let new = new.unwrap();
        let s = Settings::library();
        let read = footprint::read(&new).unwrap();
        let open = openings(&read);
        for g in read.on("User.31") {
            for (o, m) in &open {
                let d = o.segment_distance(g.pts[0], g.pts[1]) - m - s.tick_width_mm / 2.0;
                assert!(d >= s.gap_mm - ROUNDING, "tick {:?} {d} from a mask opening", g.pts);
            }
        }
        for t in read.texts.iter().filter(|t| t.layer == "User.31") {
            let i = ink(&t.text, t.size, t.size, s.text_stroke_mm).unwrap();
            let rect = Rect { x0: t.at.0 + i.x0, x1: t.at.0 + i.x1, y0: t.at.1 + i.y0, y1: t.at.1 + i.y1 };
            for o in &open {
                assert!(opening_to_rect(o, &rect) >= s.gap_mm - ROUNDING, "{} too near a mask opening", t.text);
            }
        }
        assert!(f[0].message.starts_with("User.31: ruler (SO)"), "{f:?}");
    }

    #[test]
    fn a_tick_without_room_goes_with_its_number_and_a_number_without_room_alone() {
        // A big hole right beyond pin 1's toe.
        let hole =
            "(pad \"\" np_thru_hole circle (at -4.3 0) (size 0.6 0.6) (drill 0.6) (layers \"*.Cu\" \"*.Mask\"))\n";
        let t = fp("SOIC-16", &format!("{}{hole}", so(16, 1.27, "")));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Ruler);
        assert!(f[0].message.contains("no room for the ticks of 1"), "{f:?}");
        assert!(!texts(&new.unwrap()).contains(&"1".to_owned()));
        // Pins at 0.3 mm: the numbers of neighbouring pins would touch; the side ends win over the tens.
        let t = fp("SSOP-24", &so(24, 0.3, ""));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Ruler);
        let got = texts(&new.unwrap());
        assert!(got.contains(&"12".to_owned()) && got.contains(&"13".to_owned()), "{got:?}");
        assert!(f[0].message.contains("no room for the numbers 10"), "{f:?}");
    }

    #[test]
    fn area_arrays_small_parts_and_coarse_connectors_get_none() {
        let pads = qfp(12, 0.5);
        for (lib, name) in [
            ("VHRD_Package_BGA", "BGA-48"),
            ("VHRD_Package_QFP", "LGA-48"),
            ("VHRD_Package_DFN_QFN", "WLCSP-48"),
            ("VHRD_Package_TO_SOT_SMD", "SOT-23-6"),
        ] {
            assert!(run(lib, &fp(name, &pads), Rule::Ruler).0.is_none(), "{lib}:{name}");
        }
        // Fewer pins than pins_min.
        assert!(run("VHRD_Package_SO", &fp("SOIC-6", &so(6, 1.27, "")), Rule::Ruler).0.is_none());
        // A 2.54 mm header is a counted row but too coarse; a 0.5 mm FFC row gets one, pointing away from the body.
        let row = |pitch: f64| -> String {
            (0..10)
                .map(|k| {
                    format!(
                        "(pad \"{}\" smd rect (at {} 0) (size {} 1.2) (layers \"F.Cu\" \"F.Mask\"))\n",
                        k + 1,
                        k as f64 * pitch,
                        pitch * 0.6
                    )
                })
                .collect::<String>()
                + "(fp_rect (start -1 0.5) (end 6 3) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.Fab\"))\n"
        };
        assert!(run("VHRD_Connector_PinHeader_2.54mm", &fp("PinHeader_1x10", &row(2.54)), Rule::Ruler).0.is_none());
        let new = run("VHRD_Connector_FFC-FPC", &fp("FFC_10", &row(0.5)), Rule::Ruler).0.unwrap();
        let (lines, _) = ruler(&new, "User.31");
        assert!(lines.iter().all(|(a, b)| b.1 < a.1 && a.1 < -0.6), "up, away from the body: {lines:?}");
        assert_eq!(texts(&new), ["1", "10"]);
    }

    #[test]
    fn stray_items_go_and_the_back_is_mirrored_on_ruler_b() {
        let stray = "(fp_line (start 0 0) (end 1 0) (stroke (width 0.1) (type solid)) (layer \"User.31\"))\n";
        let t = fp("R_0402", &format!("{stray}(pad \"1\" smd rect (at -0.5 0) (size 0.5 0.5) (layers \"F.Cu\"))"));
        let (new, f) = run("VHRD_Resistor_SMD", &t, Rule::Ruler);
        assert!(!new.unwrap().contains("User.31"));
        assert_eq!(f[0].message, "User.31: no ruler for this footprint (replacing 1)");
        let back = fp("SOIC-16", &so(16, 1.27, "").replace("\"F.Cu\" \"F.Mask\"", "\"B.Cu\" \"B.Mask\""))
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")");
        let new = run("VHRD_Package_SO", &back, Rule::Ruler).0.unwrap();
        assert!(!new.contains("User.31") && new.contains("(layer \"User.32\")") && new.contains("(justify mirror)"));
        settled("VHRD_Package_SO", &back, Rule::Ruler);
    }

    /// A board with the SOIC-16 and its library ruler, placed at `angle`, its pads' angles turned with it as a
    /// board keeps them.
    fn board_with(angle: f64) -> String {
        let lib = run("VHRD_Package_SO", &fp("SOIC-16", &so(16, 1.27, "")), Rule::Ruler).0.unwrap();
        let fp_text = lib.replacen("(footprint \"SOIC-16\"", "(footprint \"VHRD_Package_SO:SOIC-16\"\n\t(at 50 40 ANGLE)\n\t(property \"Reference\" \"U3\" (at 0 0 0) (layer \"F.SilkS\"))", 1);
        let fp_text = fp_text.replace("(at 50 40 ANGLE)", &format!("(at 50 40 {})", fmt(angle)));
        let fp_text = fp_text.replace(") (size 1.5", &format!(" {}) (size 1.5", fmt(angle)));
        let indented: String = fp_text.lines().map(|l| format!("\t{l}\n")).collect();
        format!("(kicad_pcb\n\t(version 20260206)\n{indented})\n")
    }

    fn silk(board: &str) -> (usize, Vec<(String, f64)>) {
        let root = sexpr::parse(board).unwrap();
        let fp = root.children("footprint").next().unwrap();
        let items = items_on(fp, "F.SilkS");
        let lines = items.iter().filter(|i| matches!(i, Item::Line { .. })).count();
        let texts = items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, size, .. } => Some((text.clone(), *size)),
                Item::Line { .. } => None,
            })
            .collect();
        (lines, texts)
    }

    #[test]
    fn a_board_gets_the_silk_copy_at_the_silk_text_size_and_loses_it_again() {
        for angle in [0.0, 90.0] {
            let b = board_with(angle);
            let (on, report) = board(&b, true).unwrap();
            assert_eq!(report.len(), 1);
            assert_eq!((report[0].reference.as_str(), report[0].ticks, report[0].removed), ("U3", 7, 0));
            let (lines, texts) = silk(&on);
            assert_eq!(lines, 7, "every tick copied");
            assert!(texts.iter().all(|(_, size)| *size == 0.8), "{texts:?}");
            // At 0.8 mm pins 9 and 10 (1.27 mm apart) still fit; the turn does not change what fits.
            assert_eq!((texts.len(), report[0].left_out.len()), (5, 0), "{texts:?} {:?}", report[0]);
            // Again: nothing changes.
            let (again, report) = board(&on, true).unwrap();
            assert_eq!(again, on);
            assert!(report.is_empty());
            // Off: back to the board as it was.
            let (off, report) = board(&on, false).unwrap();
            assert_eq!(off, b);
            assert_eq!(report[0].removed, lines + texts.len());
            assert_eq!(board(&off, false).unwrap().0, off);
        }
    }

    #[test]
    fn a_silk_number_without_room_at_the_silk_size_is_left_out_and_its_tick_stays() {
        let lib = run("VHRD_Package_SO", &fp("SSOP-24", &so(24, 0.5, "")), Rule::Ruler).0.unwrap();
        let fp_text = lib.replacen("(footprint \"SSOP-24\"", "(footprint \"VHRD_Package_SO:SSOP-24\"\n\t(at 0 0)", 1);
        let indented: String = fp_text.lines().map(|l| format!("\t{l}\n")).collect();
        let b = format!("(kicad_pcb\n{indented})\n");
        let ruler_texts = texts(&lib).len();
        let (on, report) = board(&b, true).unwrap();
        let (lines, texts) = silk(&on);
        assert_eq!(lines, report[0].ticks);
        assert!(!report[0].left_out.is_empty() && texts.len() + report[0].left_out.len() == ruler_texts, "{report:?}");
    }
}
