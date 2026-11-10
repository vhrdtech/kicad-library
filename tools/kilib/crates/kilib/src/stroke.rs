//! Where KiCad's stroke font (newstroke, its default font) puts ink: the box a text covers, the strokes counted, so a
//! designator can be fitted to a footprint the way pcbnew draws it.
//!
//! Each glyph's advance and ink come from KiCad's `common/newstroke_font.cpp` (KiCad 10). A glyph is drawn in units
//! of 1/21 of the text size: its advance is its declared width, its strokes start right of the cursor and run from
//! the cap line (-20 above the baseline, -21 for a few taller glyphs such as `4`) down to 1 below it. A line of text
//! is laid out as `common/font/font.cpp` and `common/font/stroke_font.cpp` do: the glyphs side by side, each
//! advancing the cursor by its width (the inter-character room is part of the width); centred on the anchor by the
//! sum of the advances; the baseline at 0.415 of the height below the anchor less 0.052 of the stroke (pcbnew's
//! 1.17 line-height and stroke fudges). Round stroke ends add half the stroke on every side. The tests pin the
//! result against `kicad-cli fp export svg`.

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

    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// The middle of the box, from the anchor.
    pub fn centre(&self) -> (f64, f64) {
        ((self.x0 + self.x1) / 2.0, (self.y0 + self.y1) / 2.0)
    }

    /// Mirrored left to right, as KiCad draws text on a bottom layer.
    pub fn mirrored(&self) -> Ink {
        Ink { x0: -self.x1, x1: -self.x0, ..*self }
    }

    /// Turned by `angle`, a multiple of 90°, counter-clockwise on screen (y down) as KiCad turns text.
    pub fn turned(&self, angle: f64) -> Ink {
        let Ink { x0, x1, y0, y1 } = *self;
        match (angle.rem_euclid(360.0) / 90.0).round() as i64 % 4 {
            1 => Ink { x0: y0, x1: y1, y0: -x1, y1: -x0 },
            2 => Ink { x0: -x1, x1: -x0, y0: -y1, y1: -y0 },
            3 => Ink { x0: -y1, x1: -y0, y0: x0, y1: x1 },
            _ => *self,
        }
    }

    fn union(&self, o: &Ink) -> Ink {
        Ink { x0: self.x0.min(o.x0), x1: self.x1.max(o.x1), y0: self.y0.min(o.y0), y1: self.y1.max(o.y1) }
    }
}

/// One glyph in font units (1/21 of the size): its advance, and its strokes' extent right of the cursor and from
/// the baseline (y down).
#[derive(Debug, Clone, Copy)]
struct Glyph {
    /// Checked against its place in the table by the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    ch: char,
    advance: i8,
    x0: i8,
    x1: i8,
    y0: i8,
    y1: i8,
}

const fn g(ch: char, advance: i8, x0: i8, x1: i8, y0: i8, y1: i8) -> Glyph {
    Glyph { ch, advance, x0, x1, y0, y1 }
}

const UNIT: f64 = 1.0 / 21.0;

/// Printable ASCII, from the space (which draws nothing) to `~`; anything else is drawn as `?`, as KiCad does.
const GLYPHS: [Glyph; 95] = [
    g(' ', 16, 0, 0, 0, 0),
    g('!', 10, 4, 6, -20, 1),
    g('"', 16, 4, 12, -20, -16),
    g('#', 21, 2, 19, -22, 5),
    g('$', 20, 4, 16, -23, 4),
    g('%', 24, 4, 20, -20, 1),
    g('&', 26, 5, 22, -20, 1),
    g('\'', 10, 4, 6, -20, -16),
    g('(', 14, 5, 11, -23, 9),
    g(')', 14, 3, 9, -23, 9),
    g('*', 16, 3, 13, -20, -11),
    g('+', 26, 5, 21, -15, 1),
    g(',', 10, 4, 6, 0, 4),
    g('-', 26, 5, 21, -7, -7),
    g('.', 10, 4, 6, -1, 1),
    g('/', 22, 2, 20, -21, 6),
    g('0', 20, 4, 16, -20, 1),
    g('1', 20, 4, 16, -20, 1),
    g('2', 20, 3, 16, -20, 1),
    g('3', 20, 3, 16, -20, 1),
    g('4', 20, 4, 17, -21, 1),
    g('5', 20, 4, 16, -20, 1),
    g('6', 20, 4, 16, -20, 1),
    g('7', 20, 3, 17, -20, 1),
    g('8', 20, 4, 16, -20, 1),
    g('9', 20, 4, 16, -20, 1),
    g(':', 10, 4, 6, -12, 1),
    g(';', 10, 4, 6, -12, 4),
    g('<', 26, 5, 21, -13, -1),
    g('=', 26, 5, 21, -10, -4),
    g('>', 26, 5, 21, -13, -1),
    g('?', 18, 4, 14, -20, 1),
    g('@', 27, 4, 23, -16, 4),
    g('A', 18, 2, 16, -20, 1),
    g('B', 21, 5, 17, -20, 1),
    g('C', 21, 4, 17, -20, 1),
    g('D', 21, 5, 17, -20, 1),
    g('E', 19, 5, 15, -20, 1),
    g('F', 18, 5, 15, -20, 1),
    g('G', 21, 4, 17, -20, 1),
    g('H', 22, 5, 17, -20, 1),
    g('I', 10, 5, 5, -20, 1),
    g('J', 16, 3, 11, -20, 1),
    g('K', 21, 5, 17, -20, 1),
    g('L', 17, 5, 15, -20, 1),
    g('M', 24, 5, 19, -20, 1),
    g('N', 22, 5, 17, -20, 1),
    g('O', 22, 4, 18, -20, 1),
    g('P', 21, 5, 17, -20, 1),
    g('Q', 22, 4, 19, -20, 3),
    g('R', 21, 5, 17, -20, 1),
    g('S', 20, 4, 16, -20, 1),
    g('T', 16, 2, 14, -20, 1),
    g('U', 22, 5, 17, -20, 1),
    g('V', 18, 2, 16, -20, 1),
    g('W', 24, 3, 21, -20, 1),
    g('X', 20, 3, 17, -20, 1),
    g('Y', 18, 2, 16, -20, 1),
    g('Z', 20, 3, 17, -20, 1),
    g('[', 14, 6, 11, -22, 8),
    g('\\', 14, -2, 16, -22, 5),
    g(']', 14, 3, 8, -22, 8),
    g('^', 12, 2, 10, -21, -18),
    g('_', 16, 0, 16, 3, 3),
    g('`', 8, 2, 5, -21, -18),
    g('a', 19, 4, 14, -13, 1),
    g('b', 19, 5, 15, -20, 1),
    g('c', 18, 4, 14, -13, 1),
    g('d', 19, 4, 14, -20, 1),
    g('e', 18, 4, 14, -13, 1),
    g('f', 12, 2, 10, -20, 1),
    g('g', 19, 4, 14, -13, 8),
    g('h', 19, 5, 14, -20, 1),
    g('i', 10, 4, 6, -20, 1),
    g('j', 10, 1, 6, -20, 8),
    g('k', 17, 5, 13, -20, 1),
    g('l', 11, 5, 8, -20, 1),
    g('m', 28, 5, 23, -13, 1),
    g('n', 19, 5, 14, -13, 1),
    g('o', 19, 4, 15, -13, 1),
    g('p', 19, 5, 15, -13, 8),
    g('q', 19, 4, 14, -13, 8),
    g('r', 13, 5, 11, -13, 1),
    g('s', 17, 4, 13, -13, 1),
    g('t', 12, 2, 10, -20, 1),
    g('u', 19, 5, 14, -13, 1),
    g('v', 16, 3, 13, -13, 1),
    g('w', 22, 3, 19, -13, 1),
    g('x', 17, 3, 14, -13, 1),
    g('y', 16, 3, 13, -13, 8),
    g('z', 17, 3, 14, -13, 1),
    g('{', 14, 4, 11, -23, 9),
    g('|', 20, 10, 10, -22, 8),
    g('}', 14, 3, 10, -23, 9),
    g('~', 15, 2, 12, -9, -7),
];

fn glyph(c: char) -> &'static Glyph {
    let i = (c as u32).wrapping_sub(' ' as u32) as usize;
    GLYPHS.get(i).unwrap_or(&GLYPHS[('?' as u32 - ' ' as u32) as usize])
}

/// The sum of the advances of `text` at text width `w`: what KiCad centres a line by.
pub fn advance(text: &str, w: f64) -> f64 {
    text.chars().map(|c| glyph(c).advance as f64 * UNIT * w).sum()
}

/// The ink box of one line of `text` at size `w` x `h` and stroke `thickness`, centred on its anchor both ways
/// (KiCad's default justification), unturned and unmirrored; None for text without ink (empty, or spaces only).
pub fn ink(text: &str, w: f64, h: f64, thickness: f64) -> Option<Ink> {
    let baseline = 0.415 * h - 0.052 * thickness;
    let mut cursor = -advance(text, w) / 2.0;
    let mut out: Option<Ink> = None;
    for c in text.chars() {
        let g = glyph(c);
        if c != ' ' {
            let r = Ink {
                x0: cursor + g.x0 as f64 * UNIT * w,
                x1: cursor + g.x1 as f64 * UNIT * w,
                y0: baseline + g.y0 as f64 * UNIT * h,
                y1: baseline + g.y1 as f64 * UNIT * h,
            };
            out = Some(out.map_or(r, |o| o.union(&r)));
        }
        cursor += g.advance as f64 * UNIT * w;
    }
    let t = thickness / 2.0;
    out.map(|i| Ink { x0: i.x0 - t, x1: i.x1 + t, y0: i.y0 - t, y1: i.y1 + t })
}

/// The box that holds the ink of every one of `texts` at the same anchor.
pub fn ink_of_all<'a>(texts: impl IntoIterator<Item = &'a str>, w: f64, h: f64, thickness: f64) -> Option<Ink> {
    texts.into_iter().filter_map(|t| ink(t, w, h, thickness)).reduce(|a, b| a.union(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Ink, b: Ink) -> bool {
        [(a.x0, b.x0), (a.x1, b.x1), (a.y0, b.y0), (a.y1, b.y1)].iter().all(|(p, q)| (p - q).abs() < 5e-4)
    }

    #[test]
    fn the_table_runs_through_printable_ascii() {
        for (i, g) in GLYPHS.iter().enumerate() {
            assert_eq!(g.ch as u32, ' ' as u32 + i as u32);
        }
        assert_eq!(glyph('é').ch, '?');
    }

    /// The ink boxes pcbnew plots (`kicad-cli fp export svg --layers F.Fab` of a footprint whose Reference holds
    /// the text, KiCad 10.0.8): the stroke centre lines measured from the anchor, half the stroke added.
    #[test]
    fn matches_what_kicad_draws() {
        let measured = |x0: f64, x1: f64, y0: f64, y1: f64, t: f64| Ink {
            x0: x0 - t / 2.0,
            x1: x1 + t / 2.0,
            y0: y0 - t / 2.0,
            y1: y1 + t / 2.0,
        };
        assert!(close(ink("H", 1.0, 1.0, 0.15).unwrap(), measured(-0.2857, 0.2857, -0.5452, 0.4548, 0.15)));
        assert!(close(ink("R806", 1.0, 1.0, 0.15).unwrap(), measured(-1.6905, 1.7381, -0.5452, 0.4548, 0.15)));
        assert!(close(ink("U1004", 1.0, 1.0, 0.15).unwrap(), measured(-2.1905, 2.2857, -0.5928, 0.4548, 0.15)));
        assert!(close(ink("REF**", 1.0, 1.0, 0.15).unwrap(), measured(-1.9048, 2.0, -0.5452, 0.4548, 0.15)));
        assert!(close(ink("I1", 1.0, 1.0, 0.15).unwrap(), measured(-0.4762, 0.5238, -0.5452, 0.4548, 0.15)));
        assert!(close(ink("WM88", 1.0, 1.0, 0.15).unwrap(), measured(-1.9524, 1.9048, -0.5452, 0.4548, 0.15)));
        assert!(close(ink("R806", 0.2, 0.2, 0.03).unwrap(), measured(-0.3381, 0.3476, -0.1090, 0.0910, 0.03)));
    }

    #[test]
    fn widths_of_designators() {
        // R806 at 1 mm, 0.15 stroke: 3.579 mm of ink; U1004: 4.626 mm.
        assert!((ink("R806", 1.0, 1.0, 0.15).unwrap().width() - 3.5786).abs() < 1e-3);
        assert!((ink("U1004", 1.0, 1.0, 0.15).unwrap().width() - 4.6262).abs() < 1e-3);
        assert!(ink(" ", 1.0, 1.0, 0.15).is_none());
        let all = ink_of_all(["R1", "U1004"], 1.0, 1.0, 0.15).unwrap();
        assert!(close(all, ink("U1004", 1.0, 1.0, 0.15).unwrap()));
        let i = ink("R806", 1.0, 1.0, 0.15).unwrap();
        assert_eq!(i.turned(90.0), Ink { x0: i.y0, x1: i.y1, y0: -i.x1, y1: -i.x0 });
        assert_eq!(i.mirrored().mirrored(), i);
    }
}
