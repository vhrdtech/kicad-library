//! Chip passives and power inductors made from data: the tables in the checkout's `generate/` folder give each
//! size's body and the heights it is made in, and every size and height becomes a footprint name and a STEP model
//! ([`model`], written by [`crate::step`]).
//!
//! - `generate/chip-passives.toml`: chip resistors, capacitors and inductors (`[[chip]]`), nominal body length,
//!   width, termination band and the heights, from the makers' tables.
//! - `generate/power-inductors.toml`: shielded and moulded power inductors (`[[power]]`), body, bottom
//!   terminations and the maker's recommended land.
//!
//! A chip part is named after KiCad's stock footprint of its size with the height added
//! (`C_0805_2012Metric_H1.25`), a power inductor by its family and body (`L_Shielded_4.0x4.0mm_H2.0`). Models
//! sit on z = 0 (the board), centred on the footprint origin, the terminations at -x and +x.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::step::{Rgb, Solid};

pub mod footprint;

/// The folder of the tables in a checkout.
pub const TABLE_DIR: &str = "generate";
pub const CHIP_TABLE: &str = "chip-passives.toml";
pub const POWER_TABLE: &str = "power-inductors.toml";
/// Where the models go under `${VHRD_3DMODEL_DIR}`.
pub const MODEL_SUBDIR: &str = "generated";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum Kind {
    R,
    C,
    L,
}

impl Kind {
    pub fn library(self) -> &'static str {
        match self {
            Kind::R => "VHRD_Resistor_SMD",
            Kind::C => "VHRD_Capacitor_SMD",
            Kind::L => "VHRD_Inductor_SMD",
        }
    }
}

/// How a chip body looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// Ceramic capacitor: tan body, terminations on five sides at both ends.
    Mlcc,
    /// Thick-film resistor: white alumina body, black overcoat on top, wrap-around terminations.
    Resistor,
    /// Multilayer chip inductor: built like an MLCC, body in the family's colour.
    LMultilayer,
    /// Wire-wound chip inductor: white ceramic body, L-shaped electrodes on the ends and the bottom.
    LWirewound,
}

/// The body colour of a chip inductor family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyColour {
    Ferrite,
    Ceramic,
}

/// One `[[chip]]` row: a size in one body style and the heights it is made in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chip {
    pub kind: Kind,
    /// EIA inch code, `0805`.
    pub size: String,
    /// Metric code as KiCad names it, `2012`.
    pub metric: String,
    pub style: Style,
    /// Body length (along x), width and termination band, nominal, mm.
    pub length: f64,
    pub width: f64,
    pub band: f64,
    pub heights: Vec<f64>,
    #[serde(default)]
    pub colour: Option<BodyColour>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Family {
    Shielded,
    Molded,
}

/// A power inductor's outline seen from the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corners {
    Square,
    Chamfer,
    Round,
}

/// One `[[power]]` row: a power-inductor body, its bottom terminations and land, and its heights.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Power {
    pub family: Family,
    pub length: f64,
    pub width: f64,
    pub heights: Vec<f64>,
    pub corners: Corners,
    #[serde(default)]
    pub corner: f64,
    pub term_length: f64,
    pub term_width: f64,
    pub term_x: f64,
    pub pad_x: f64,
    pub pad_length: f64,
    pub pad_width: f64,
    #[serde(default)]
    pub land_source: String,
    #[serde(default)]
    pub fits: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tables {
    #[serde(default)]
    pub chip: Vec<Chip>,
    #[serde(default)]
    pub power: Vec<Power>,
}

impl Tables {
    /// Reads the tables of a checkout; a table that is not there is empty.
    pub fn load(library: &Path) -> Result<Tables, String> {
        let dir = library.join(TABLE_DIR);
        let mut all = Tables::default();
        for name in [CHIP_TABLE, POWER_TABLE] {
            let path = dir.join(name);
            if !path.exists() {
                continue;
            }
            let t = Tables::read(&path)?;
            all.chip.extend(t.chip);
            all.power.extend(t.power);
        }
        Ok(all)
    }

    pub fn read(path: &Path) -> Result<Tables, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Every part of the tables, one per size and height, in table order; fails on a name given twice or a body
    /// that cannot be built.
    pub fn parts(&self) -> Result<Vec<Part>, String> {
        let mut parts = Vec::new();
        for c in &self.chip {
            for &h in &c.heights {
                parts.push(Part { name: chip_name(c, h), height: h, body: Body::Chip(c.clone()) });
            }
        }
        for p in &self.power {
            for &h in &p.heights {
                parts.push(Part { name: power_name(p, h), height: h, body: Body::Power(p.clone()) });
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &parts {
            if !seen.insert(&p.name) {
                return Err(format!("{}: given twice", p.name));
            }
            p.check()?;
        }
        Ok(parts)
    }
}

/// A height or size as names write it: at least one decimal, no trailing zeros (`2.0`, `0.35`, `1.25`).
pub fn mm(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') { format!("{s}0") } else { s.to_string() }
}

fn chip_name(c: &Chip, h: f64) -> String {
    format!("{:?}_{}_{}Metric_H{}", c.kind, c.size, c.metric, mm(h))
}

fn power_name(p: &Power, h: f64) -> String {
    format!("L_{:?}_{}x{}mm_H{}", p.family, mm(p.length), mm(p.width), mm(h))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Chip(Chip),
    Power(Power),
}

/// One footprint and model: a body in one height.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub name: String,
    pub height: f64,
    pub body: Body,
}

impl Part {
    pub fn library(&self) -> &'static str {
        match &self.body {
            Body::Chip(c) => c.kind.library(),
            Body::Power(_) => Kind::L.library(),
        }
    }

    /// The footprint's model path, under `${VHRD_3DMODEL_DIR}`.
    pub fn model_ref(&self) -> String {
        format!("${{VHRD_3DMODEL_DIR}}/{MODEL_SUBDIR}/{}.3dshapes/{}.step", self.library(), self.name)
    }

    /// The model file under a models folder (`${VHRD_3DMODEL_DIR}/generated`).
    pub fn model_path(&self, out: &Path) -> PathBuf {
        out.join(format!("{}.3dshapes", self.library())).join(format!("{}.step", self.name))
    }

    fn check(&self) -> Result<(), String> {
        let bad = |what: &str| Err(format!("{}: {what}", self.name));
        let h = self.height;
        match &self.body {
            Body::Chip(c) => {
                if !(c.length > 0.0 && c.width > 0.0 && h > 0.0) {
                    return bad("length, width and height must be above 0");
                }
                if !(c.band > 0.0 && 2.0 * c.band < c.length) {
                    return bad("the band must be above 0 and the two bands shorter than the body");
                }
                if c.style == Style::LMultilayer && c.colour.is_none() {
                    return bad("a multilayer inductor needs its colour (ferrite or ceramic)");
                }
            }
            Body::Power(p) => {
                if !(p.length > 0.0 && p.width > 0.0 && h > 0.0) {
                    return bad("length, width and height must be above 0");
                }
                if p.corners != Corners::Square && !(p.corner > 0.0 && 2.0 * p.corner < p.length.min(p.width)) {
                    return bad("corner must be above 0 and below half the body");
                }
                if !(p.term_length > 0.0 && p.term_width > 0.0 && p.term_width <= p.width + 1e-9) {
                    return bad("terminations must have a size and fit the body's width");
                }
                if !(p.term_x - p.term_length / 2.0 > 0.0 && p.term_x + p.term_length / 2.0 <= p.length / 2.0 + 1e-9) {
                    return bad("the terminations must not meet and must stay under the body");
                }
                if !(p.pad_length > 0.0 && p.pad_width > 0.0 && p.pad_x - p.pad_length / 2.0 > 0.0) {
                    return bad("the pads must have a size and must not meet");
                }
            }
        }
        Ok(())
    }
}

/// Colours of the models.
pub mod colour {
    use crate::step::Rgb;
    /// MLCC ceramic.
    pub const TAN: Rgb = Rgb(0.66, 0.53, 0.37);
    /// Nickel-tin terminations.
    pub const TIN: Rgb = Rgb(0.62, 0.63, 0.66);
    /// A resistor's alumina substrate.
    pub const ALUMINA: Rgb = Rgb(0.95, 0.95, 0.93);
    /// Ceramic inductor bodies.
    pub const IVORY: Rgb = Rgb(0.90, 0.86, 0.76);
    /// A resistor's overcoat.
    pub const OVERCOAT: Rgb = Rgb(0.09, 0.09, 0.09);
    /// Ferrite chip inductors.
    pub const FERRITE: Rgb = Rgb(0.24, 0.24, 0.26);
    /// Power inductor bodies.
    pub const POWER: Rgb = Rgb(0.20, 0.20, 0.22);
}

/// The model of a part: its solids, in mm, on z = 0.
pub fn model(part: &Part) -> Vec<Solid> {
    match &part.body {
        Body::Chip(c) => chip_model(c, part.height),
        Body::Power(p) => power_model(p, part.height),
    }
}

/// A box centred in y, from x0 to x1 and z0 to z1, `width` wide.
fn slab(name: &str, x: (f64, f64), width: f64, z: (f64, f64), colour: Rgb) -> Solid {
    Solid::cuboid(name, [x.0, -width / 2.0, z.0], [x.1, width / 2.0, z.1], colour)
}

fn chip_model(c: &Chip, h: f64) -> Vec<Solid> {
    let (l, w, e) = (c.length / 2.0, c.width, c.band);
    // The ceramic shows a little inside the plated ends, as on the real part.
    let inset = (0.02 * w.min(h)).min(0.02);
    let mut solids = Vec::new();
    match c.style {
        Style::Mlcc | Style::LMultilayer => {
            let body = match (c.style, c.colour) {
                (Style::Mlcc, _) => colour::TAN,
                (_, Some(BodyColour::Ceramic)) => colour::IVORY,
                _ => colour::FERRITE,
            };
            solids.push(slab("body", (-l + e, l - e), w - 2.0 * inset, (inset, h - inset), body));
            solids.push(slab("terminal 1", (-l, -l + e), w, (0.0, h), colour::TIN));
            solids.push(slab("terminal 2", (l - e, l), w, (0.0, h), colour::TIN));
        }
        Style::Resistor => {
            let coat = (0.12 * h).min(0.04);
            solids.push(slab("body", (-l + e, l - e), w, (0.0, h - coat), colour::ALUMINA));
            solids.push(slab("overcoat", (-l + e, l - e), w - 2.0 * (0.05 * w), (h - coat, h), colour::OVERCOAT));
            solids.push(slab("terminal 1", (-l, -l + e), w, (0.0, h), colour::TIN));
            solids.push(slab("terminal 2", (l - e, l), w, (0.0, h), colour::TIN));
        }
        Style::LWirewound => {
            // The electrode: a plate on each end and a strip under the body, `t` thick.
            let t = (0.03 * c.length).min(0.03);
            solids.push(slab("body", (-l + e, l - e), w, (0.0, h), colour::IVORY));
            solids.push(slab("body end 1", (-l + t, -l + e), w, (t, h), colour::IVORY));
            solids.push(slab("body end 2", (l - e, l - t), w, (t, h), colour::IVORY));
            solids.push(slab("terminal 1", (-l, -l + t), w, (0.0, h), colour::TIN));
            solids.push(slab("terminal 1 bottom", (-l + t, -l + e), w, (0.0, t), colour::TIN));
            solids.push(slab("terminal 2", (l - t, l), w, (0.0, h), colour::TIN));
            solids.push(slab("terminal 2 bottom", (l - e, l - t), w, (0.0, t), colour::TIN));
        }
    }
    solids
}

/// The body outline seen from the top, counter-clockwise; a chamfer cuts each corner straight, a round corner is
/// drawn with four straight pieces.
pub(crate) fn outline(p: &Power) -> Vec<[f64; 2]> {
    use std::f64::consts::FRAC_PI_2;
    let (x, y, c) = (p.length / 2.0, p.width / 2.0, p.corner);
    let steps = match p.corners {
        Corners::Square => return vec![[x, -y], [x, y], [-x, y], [-x, -y]],
        Corners::Chamfer => 1,
        Corners::Round => 4,
    };
    let mut out = Vec::new();
    for (i, [cx, cy]) in [[x, -y], [x, y], [-x, y], [-x, -y]].into_iter().enumerate() {
        // The corner's arc, centred `c` in from both sides, from the side before it to the side after it.
        let (ox, oy) = (cx - cx.signum() * c, cy - cy.signum() * c);
        let start = i as f64 * FRAC_PI_2 - FRAC_PI_2;
        for k in 0..=steps {
            let a = start + FRAC_PI_2 * k as f64 / steps as f64;
            out.push([ox + c * a.cos(), oy + c * a.sin()]);
        }
    }
    out
}

fn power_model(p: &Power, h: f64) -> Vec<Solid> {
    // The electrodes stand a little proud of the body's bottom, as on the real part.
    let t = 0.05;
    let mut solids = vec![Solid { name: "body".into(), outline: outline(p), z0: t, z1: h, colour: colour::POWER }];
    for (i, sx) in [(1, -1.0), (2, 1.0)] {
        let x = sx * p.term_x;
        let (x0, x1) = (x - p.term_length / 2.0, x + p.term_length / 2.0);
        solids.push(slab(&format!("terminal {i}"), (x0, x1), p.term_width, (0.0, t), colour::TIN));
    }
    solids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step;

    const TABLE: &str = r#"
[[chip]]
kind = "C"
size = "0805"
metric = "2012"
style = "mlcc"
length = 2.0
width = 1.25
band = 0.5
heights = [0.6, 0.85, 1.25]
sources = ["Murata GRM21"]

[[chip]]
kind = "R"
size = "0402"
metric = "1005"
style = "resistor"
length = 1.0
width = 0.5
band = 0.25
heights = [0.35]

[[chip]]
kind = "L"
size = "0603"
metric = "1608"
style = "l_wirewound"
length = 1.6
width = 0.8
band = 0.3
heights = [0.9]

[[chip]]
kind = "L"
size = "0603"
metric = "1608"
style = "l_multilayer"
colour = "ferrite"
length = 1.6
width = 0.8
band = 0.3
heights = [0.8]

[[power]]
family = "Shielded"
length = 4.0
width = 4.0
heights = [1.8, 2.0]
corners = "chamfer"
corner = 0.6
term_length = 1.0
term_width = 3.2
term_x = 1.5
pad_x = 1.55
pad_length = 1.5
pad_width = 3.6

[[power]]
family = "Molded"
length = 4.3
width = 4.3
heights = [3.0]
corners = "round"
corner = 0.3
term_length = 1.0
term_width = 3.2
term_x = 1.6
pad_x = 1.7
pad_length = 1.4
pad_width = 3.4
"#;

    #[test]
    fn names() {
        let t: Tables = toml::from_str(TABLE).unwrap();
        let names: Vec<String> = t.parts().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "C_0805_2012Metric_H0.6",
                "C_0805_2012Metric_H0.85",
                "C_0805_2012Metric_H1.25",
                "R_0402_1005Metric_H0.35",
                "L_0603_1608Metric_H0.9",
                "L_0603_1608Metric_H0.8",
                "L_Shielded_4.0x4.0mm_H1.8",
                "L_Shielded_4.0x4.0mm_H2.0",
                "L_Molded_4.3x4.3mm_H3.0",
            ]
        );
        assert_eq!(mm(2.0), "2.0");
        assert_eq!(mm(0.35), "0.35");
        assert_eq!(mm(1.25), "1.25");
    }

    #[test]
    fn model_paths() {
        let t: Tables = toml::from_str(TABLE).unwrap();
        let p = &t.parts().unwrap()[2];
        assert_eq!(
            p.model_ref(),
            "${VHRD_3DMODEL_DIR}/generated/VHRD_Capacitor_SMD.3dshapes/C_0805_2012Metric_H1.25.step"
        );
        assert_eq!(
            p.model_path(Path::new("/m/generated")),
            Path::new("/m/generated/VHRD_Capacitor_SMD.3dshapes/C_0805_2012Metric_H1.25.step")
        );
    }

    /// Every model is writable, sits on the board and fills the body's box exactly.
    #[test]
    fn models_fill_the_body() {
        let t: Tables = toml::from_str(TABLE).unwrap();
        for part in t.parts().unwrap() {
            let solids = model(&part);
            step::write(&part.name, &solids).unwrap();
            let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for s in &solids {
                for c in &s.outline {
                    lo[0] = lo[0].min(c[0]);
                    lo[1] = lo[1].min(c[1]);
                    hi[0] = hi[0].max(c[0]);
                    hi[1] = hi[1].max(c[1]);
                }
                lo[2] = lo[2].min(s.z0);
                hi[2] = hi[2].max(s.z1);
            }
            let (l, w) = match &part.body {
                Body::Chip(c) => (c.length, c.width),
                Body::Power(p) => (p.length, p.width),
            };
            let want = ([-l / 2.0, -w / 2.0, 0.0], [l / 2.0, w / 2.0, part.height]);
            for k in 0..3 {
                assert!(
                    (lo[k] - want.0[k]).abs() < 1e-9 && (hi[k] - want.1[k]).abs() < 1e-9,
                    "{}: {lo:?} {hi:?}",
                    part.name
                );
            }
        }
    }

    #[test]
    fn bad_rows_are_refused() {
        let twice = format!(
            "{TABLE}\n[[chip]]\nkind = \"R\"\nsize = \"0402\"\nmetric = \"1005\"\nstyle = \"resistor\"\nlength = 1.0\nwidth = 0.5\nband = 0.25\nheights = [0.35]\n"
        );
        let t: Tables = toml::from_str(&twice).unwrap();
        assert!(t.parts().unwrap_err().contains("R_0402_1005Metric_H0.35: given twice"));
        let wide = TABLE.replace("band = 0.25", "band = 0.5");
        let t: Tables = toml::from_str(&wide).unwrap();
        assert!(t.parts().unwrap_err().contains("band"));
        assert!(toml::from_str::<Tables>(&TABLE.replace("band = 0.5\n", "band = 0.5\nfoo = 1\n")).is_err());
    }
}
