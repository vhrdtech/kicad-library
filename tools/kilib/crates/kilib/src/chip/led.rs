//! Chip LEDs made from data: `generate/chip-leds.toml` gives each size's body (`[[led]]`) and the colours it is
//! made in, and every size and colour becomes a footprint and a STEP model in the LED's colour.
//!
//! A chip LED is a thin printed board (white or black) with plated ends, a die in the middle and a moulded epoxy
//! lens over the whole top. The lens takes the emission colour: water-clear parts get a deep tint, diffused ones a
//! paler, milky one, and a white LED the yellow of its phosphor. The footprint is the size's stock IPC-7351 land
//! (`LED_0603_1608Metric`) with the `_Arrow` silkscreen of the library: a triangle outside the courtyard pointing
//! at pin 1 (the cathode), and one between the pads pointing the same way.

use serde::Deserialize;

use crate::chip::mm;
use crate::step::{Rgb, Solid};

pub const TABLE: &str = "chip-leds.toml";
pub const LIBRARY: &str = "VHRD_LED_SMD";

/// What the LED emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum Emission {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    White,
}

impl Emission {
    pub fn name(self) -> &'static str {
        match self {
            Emission::Red => "Red",
            Emission::Orange => "Orange",
            Emission::Yellow => "Yellow",
            Emission::Green => "Green",
            Emission::Blue => "Blue",
            Emission::White => "White",
        }
    }

    /// The colour the die glows in, and a white LED's phosphor.
    fn light(self) -> Rgb {
        match self {
            Emission::Red => Rgb(0.86, 0.06, 0.05),
            Emission::Orange => Rgb(1.0, 0.42, 0.0),
            Emission::Yellow => Rgb(1.0, 0.80, 0.0),
            Emission::Green => Rgb(0.10, 0.80, 0.18),
            Emission::Blue => Rgb(0.08, 0.28, 0.95),
            Emission::White => Rgb(0.98, 0.86, 0.32),
        }
    }
}

/// The board under the lens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Board {
    White,
    Black,
}

/// The look of the epoxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lens {
    /// Water-clear: the deep tint of the emission colour.
    Clear,
    /// Diffused (milky): a paler tint.
    Diffused,
}

/// One `[[led]]` row: a chip LED size in one height, and the colours it is made in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Led {
    /// EIA inch code, `0603`.
    pub size: String,
    /// Metric code as KiCad names it, `1608`.
    pub metric: String,
    /// Body length (along x), width and height, nominal, mm.
    pub length: f64,
    pub width: f64,
    pub height: f64,
    /// Thickness of the board under the lens.
    pub board_thickness: f64,
    /// Length of the lens along x, centred; at most the body's length (the plated ends show beyond it).
    pub lens_length: f64,
    /// Length of each plated end.
    pub band: f64,
    pub board: Board,
    pub lens: Lens,
    pub colours: Vec<Emission>,
    #[serde(default)]
    pub sources: Vec<String>,
}

impl Led {
    /// The stock footprint of the size, `LED_0603_1608Metric`.
    pub fn base(&self) -> String {
        format!("LED_{}_{}Metric", self.size, self.metric)
    }

    /// The footprint name of one colour, `LED_0603_1608Metric_H0.8_Red`.
    pub fn name(&self, colour: Emission) -> String {
        format!("{}_H{}_{}", self.base(), mm(self.height), colour.name())
    }

    /// The generic symbol of a colour (`VHRD_Device:LED_Red`) and the per-package one
    /// (`VHRD_Device:LED_Red_0603_H0.8`).
    pub fn symbols(&self, colour: Emission) -> (String, String) {
        let c = colour.name();
        (format!("VHRD_Device:LED_{c}"), format!("VHRD_Device:LED_{c}_{}_H{}", self.size, mm(self.height)))
    }

    pub fn check(&self, name: &str) -> Result<(), String> {
        let bad = |what: &str| Err(format!("{name}: {what}"));
        if !(self.length > 0.0 && self.width > 0.0 && self.height > 0.0) {
            return bad("length, width and height must be above 0");
        }
        if !(self.board_thickness > 0.0 && self.board_thickness < self.height) {
            return bad("the board must be thinner than the part");
        }
        if !(self.band > 0.0 && 2.0 * self.band < self.length) {
            return bad("the band must be above 0 and the two bands shorter than the body");
        }
        if !(self.lens_length > 0.0 && self.lens_length <= self.length + 1e-9) {
            return bad("the lens must have a length and fit on the body");
        }
        Ok(())
    }
}

/// Colours of the parts that do not glow.
pub mod colour {
    use crate::step::Rgb;
    /// A white LED board.
    pub const BOARD_WHITE: Rgb = Rgb(0.93, 0.93, 0.90);
    /// A black LED board.
    pub const BOARD_BLACK: Rgb = Rgb(0.08, 0.08, 0.09);
    /// The gold-plated ends.
    pub const GOLD: Rgb = Rgb(0.83, 0.67, 0.29);
}

/// A tint of `c`: mixed with white by `white` (0..1).
fn tint(c: Rgb, white: f64) -> Rgb {
    let m = |v: f64| v + (1.0 - v) * white;
    Rgb(m(c.0), m(c.1), m(c.2))
}

/// How see-through a water-clear lens is drawn.
pub const CLEAR: f64 = 0.45;

/// The lens of a part: its colour and how see-through it is. A water-clear lens is a deep, see-through tint with
/// the die glowing inside; a diffused one an opaque, paler tint; a white LED's lens the opaque yellow of its
/// phosphor, clear or diffused.
pub fn lens_look(led: &Led, colour: Emission) -> (Rgb, f64) {
    let light = colour.light();
    match (colour, led.lens) {
        (Emission::White, _) => (light, 0.0),
        (_, Lens::Clear) => (tint(light, 0.1), CLEAR),
        (_, Lens::Diffused) => (tint(light, 0.45), 0.0),
    }
}

/// The model of one size and height in one colour: its solids, in mm, on z = 0, pin 1 (the cathode) at -x.
pub fn model(led: &Led, colour: Emission) -> Vec<Solid> {
    let (l, w, e, t) = (led.length / 2.0, led.width, led.band, led.board_thickness);
    let slab = |name: &str, x: (f64, f64), y: f64, z: (f64, f64), c: Rgb| {
        Solid::cuboid(name, [x.0, -y / 2.0, z.0], [x.1, y / 2.0, z.1], c)
    };
    let board = match led.board {
        Board::White => colour::BOARD_WHITE,
        Board::Black => colour::BOARD_BLACK,
    };
    let (lens, see_through) = lens_look(led, colour);
    let ll = led.lens_length / 2.0;
    let mut solids = vec![
        slab("board", (-l + e, l - e), w, (0.0, t), board),
        slab("cathode", (-l, -l + e), w, (0.0, t), colour::GOLD),
        slab("anode", (l - e, l), w, (0.0, t), colour::GOLD),
    ];
    if see_through > 0.0 {
        // The die: a small square on the board, under the middle of the lens.
        let d = (0.3 * w).min(0.25);
        let dz = (0.4 * (led.height - t)).min(0.1);
        solids.push(slab("die", (-d / 2.0, d / 2.0), d, (t, t + dz), colour.light()));
    }
    solids.push(slab("lens", (-ll, ll), w, (t, led.height), lens).transparent(see_through));
    solids
}

/// The two `_Arrow` silkscreen triangles of a footprint, as `fp_poly` items: one just outside the courtyard
/// pointing at pin 1 from `-x`, one between the pads pointing at pin 1. `courtyard_x` is the courtyard's left
/// edge (negative), `gap` the half-distance between the pads' inner edges, `body` the body's area: the outer
/// triangle is the 0603's, made smaller where it would cover more than a fifth of the body (the pin-1 rule takes a
/// filled shape up to a quarter of it as a mark).
pub fn arrows(courtyard_x: f64, gap: f64, body: f64) -> String {
    // Clearance from the pads: the silkscreen's half width and the lint's 0.05 mm, rounded up.
    let half = (gap - 0.1).clamp(0.08, 0.3);
    let k = (0.2 * body / (0.33 * 0.48)).sqrt().min(1.0);
    let (ol, oh) = (0.33 * k, 0.24 * k);
    let outer = [[courtyard_x - 0.005, 0.0], [courtyard_x - 0.005 - ol, oh], [courtyard_x - 0.005 - ol, -oh]];
    let inner = [[-half, 0.0], [half, half * 1.1], [half, -half * 1.1]];
    let poly = |pts: [[f64; 2]; 3], width: f64| {
        let pts: Vec<String> = pts.iter().map(|p| format!("(xy {} {})", mm6(p[0]), mm6(p[1]))).collect();
        format!(
            "\t(fp_poly\n\t\t(pts\n\t\t\t{}\n\t\t)\n\t\t(stroke\n\t\t\t(width {width})\n\t\t\t(type solid)\n\t\t)\n\t\t(fill yes)\n\t\t(layer \"F.SilkS\")\n\t)\n",
            pts.join(" ")
        )
    };
    poly(outer, 0.12) + &poly(inner, 0.05)
}

/// A length as KiCad writes it: up to six decimals, no trailing zeros.
fn mm6(v: f64) -> String {
    let v = if v.abs() < 5e-7 { 0.0 } else { v };
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The footprint of one size in one colour, from the size's base footprint (`base`, the checkout's copy or a fresh
/// stock import): the base's silkscreen replaced by the two arrows, then named, described and pointed at the
/// model by `derive`.
pub fn footprint_base(base: &str, led: &Led) -> Result<String, String> {
    let fp = crate::footprint::read(base)?;
    let courtyard_x = fp
        .graphics
        .iter()
        .filter(|g| g.layer == "F.CrtYd")
        .flat_map(|g| g.pts.iter().map(|p| p.0))
        .fold(f64::MAX, f64::min);
    if courtyard_x == f64::MAX {
        return Err("the base has no courtyard".into());
    }
    let gap = fp
        .copper_pads()
        .map(|p| {
            let b = p.outline.bbox();
            if b.x1 <= 0.0 { -b.x1 } else { b.x0 }
        })
        .fold(f64::MAX, f64::min);
    if !(gap > 0.0 && gap < f64::MAX) {
        return Err("the base's pads meet in the middle".into());
    }
    let mut edits: Vec<crate::migrate::Edit> = fp
        .graphics
        .iter()
        .filter(|g| g.layer == "F.SilkS")
        .map(|g| crate::migrate::Edit { range: whole_line(base, g.span.clone()), text: String::new() })
        .collect();
    let first_pad = base.find("\t(pad ").ok_or("the base has no pads")?;
    edits.push(crate::migrate::Edit {
        range: first_pad..first_pad,
        text: arrows(courtyard_x, gap, led.length * led.width),
    });
    edits.sort_by_key(|e| e.range.start);
    Ok(crate::migrate::apply_edits(base, &edits))
}

/// The span widened to the whole lines it stands on, the line break included.
fn whole_line(text: &str, span: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let start = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[span.end..].find('\n').map_or(text.len(), |i| span.end + i + 1);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chip::{Body, Tables};

    const TABLE: &str = r#"
[[led]]
size = "0603"
metric = "1608"
length = 1.6
width = 0.8
height = 0.8
board_thickness = 0.3
lens_length = 1.1
band = 0.3
board = "white"
lens = "clear"
colours = ["Red", "White"]
"#;

    fn parts() -> Vec<crate::chip::Part> {
        toml::from_str::<Tables>(TABLE).unwrap().parts().unwrap()
    }

    #[test]
    fn names_and_symbols() {
        let parts = parts();
        let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["LED_0603_1608Metric_H0.8_Red", "LED_0603_1608Metric_H0.8_White"]);
        assert_eq!(parts[0].footprint_id(), "VHRD_LED_SMD:LED_0603_1608Metric_H0.8_Red");
        assert_eq!(
            parts[0].symbols(),
            ("VHRD_Device:LED_Red".to_string(), "VHRD_Device:LED_Red_0603_H0.8".to_string())
        );
        assert_eq!(
            parts[1].model_ref(),
            "${VHRD_3DMODEL_DIR}/generated/VHRD_LED_SMD.3dshapes/LED_0603_1608Metric_H0.8_White.step"
        );
    }

    /// A clear lens is see-through with the die inside; a white LED's phosphor is opaque and hides it.
    #[test]
    fn clear_lenses_show_the_die() {
        let parts = parts();
        let Body::Led(led, _) = &parts[0].body else { panic!() };
        let red = model(led, Emission::Red);
        let lens = red.iter().find(|s| s.name == "lens").unwrap();
        assert_eq!(lens.transparency, CLEAR);
        assert!((lens.outline[1][0] - 0.55).abs() < 1e-9, "the lens is 1.1 long");
        assert!(red.iter().any(|s| s.name == "die"));
        let white = model(led, Emission::White);
        assert!(white.iter().all(|s| s.transparency == 0.0 && s.name != "die"));
        let text = crate::step::write("x", &red).unwrap();
        assert!(text.contains("SURFACE_STYLE_TRANSPARENT"));
    }

    #[test]
    fn bad_rows_are_refused() {
        let long = TABLE.replace("lens_length = 1.1", "lens_length = 1.7");
        assert!(toml::from_str::<Tables>(&long).unwrap().parts().unwrap_err().contains("lens"));
        let thick = TABLE.replace("board_thickness = 0.3", "board_thickness = 0.8");
        assert!(toml::from_str::<Tables>(&thick).unwrap().parts().unwrap_err().contains("board"));
    }

    /// A stock-like 0603 LED: pads, the stock silkscreen and a courtyard.
    const BASE: &str = "(footprint \"LED_0603_1608Metric\"\n\t(layer \"F.Cu\")\n\t(descr \"LED SMD 0603\")\n\t(property \"Value\" \"LED_0603_1608Metric\"\n\t\t(at 0 0 0)\n\t\t(layer \"User.17\")\n\t)\n\t(fp_line\n\t\t(start 0.8 -0.735)\n\t\t(end -1.485 -0.735)\n\t\t(stroke\n\t\t\t(width 0.12)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.SilkS\")\n\t)\n\t(fp_rect\n\t\t(start -1.48 -0.73)\n\t\t(end 1.48 0.73)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"F.CrtYd\")\n\t)\n\t(pad \"1\" smd roundrect\n\t\t(at -0.7875 0)\n\t\t(size 0.875 0.95)\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t\t(roundrect_rratio 0.25)\n\t)\n\t(pad \"2\" smd roundrect\n\t\t(at 0.7875 0)\n\t\t(size 0.875 0.95)\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t\t(roundrect_rratio 0.25)\n\t)\n)\n";

    /// The stock silkscreen gives way to the library's two arrows, as on `LED_0603_1608Metric_Arrow`.
    #[test]
    fn the_base_gets_the_arrows() {
        let parts = parts();
        let Body::Led(led, _) = &parts[0].body else { panic!() };
        let text = footprint_base(BASE, led).unwrap();
        assert!(!text.contains("(start 0.8 -0.735)"), "the stock silkscreen is gone");
        assert!(text.contains("(xy -1.485 0) (xy -1.815 0.24) (xy -1.815 -0.24)"));
        assert!(text.contains("(xy -0.25 0) (xy 0.25 0.275) (xy 0.25 -0.275)"));
        assert_eq!(text.matches("(fill yes)\n\t\t(layer \"F.SilkS\")").count(), 2);
        // Untouched: pads and courtyard.
        assert!(text.contains("(at -0.7875 0)") && text.contains("(layer \"F.CrtYd\")"));
    }
}
