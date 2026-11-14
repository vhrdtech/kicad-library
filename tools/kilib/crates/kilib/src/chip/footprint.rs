//! The footprints of the chip tables: one per size and height, pointing at the part's generated model.
//!
//! A chip part's footprint is its size's base footprint (`C_0805_2012Metric`, KiCad's stock IPC-7351 nominal land)
//! under the new name, with the description naming the height and the model at
//! `${VHRD_3DMODEL_DIR}/generated/...`; pads and drawing stay the base's. The base is the checkout's copy where it
//! has one, else a fresh [`stock::import_into`] of KiCad's stock file. A power inductor's footprint is drawn from its
//! table row (the maker's land, the body on the fab layer), then given the library rules' fixes and the designator
//! and Value by the same import. Every one draws its table body, seen from the top, on Body.F (a rectangle; a power
//! inductor's chamfered or round corners), in place of any body the base drew. Only new files are made; one that differs from what would be made is reported,
//! never overwritten.

use std::path::{Path, PathBuf};

use crate::chip::{Body, Part, led, mm, outline, xray};
use crate::migrate::{Edit, apply_edits};
use crate::sexpr::{self, Node, quote};
use crate::stock;

/// One footprint of the tables.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Not in the checkout yet; the text to write.
    New(String),
    /// The checkout has it, as it would be made.
    Same,
    /// The checkout has it, different from what would be made (a hand edit, or the base or the table changed).
    Differs,
    Error(String),
}

/// Every footprint of `parts` in the checkout `library`; `stock` is KiCad's stock footprint folder, for bases the
/// checkout lacks.
pub fn plan(library: &Path, stock: Option<&Path>, parts: &[Part]) -> Vec<Item> {
    let footprints = library.join("footprints");
    parts
        .iter()
        .map(|part| {
            let path = footprints.join(format!("{}.pretty", part.library())).join(format!("{}.kicad_mod", part.name));
            let outcome = match make(&footprints, stock, part) {
                Err(e) => Outcome::Error(e),
                Ok(text) => match std::fs::read_to_string(&path) {
                    Err(_) => Outcome::New(text),
                    Ok(old) if old == text => Outcome::Same,
                    Ok(_) => Outcome::Differs,
                },
            };
            Item { name: part.name.clone(), path, outcome }
        })
        .collect()
}

/// Writes the new footprints; refuses when any item has an error.
pub fn apply(items: &[Item]) -> Result<usize, String> {
    if let Some((name, e)) = items.iter().find_map(|i| match &i.outcome {
        Outcome::Error(e) => Some((&i.name, e)),
        _ => None,
    }) {
        return Err(format!("{name}: {e}; nothing written"));
    }
    let mut n = 0;
    for i in items {
        if let Outcome::New(text) = &i.outcome {
            std::fs::create_dir_all(i.path.parent().expect("a parent")).map_err(|e| e.to_string())?;
            std::fs::write(&i.path, text).map_err(|e| format!("{}: {e}", i.path.display()))?;
            n += 1;
        }
    }
    Ok(n)
}

/// The footprint text of one part.
pub fn make(footprints: &Path, stock: Option<&Path>, part: &Part) -> Result<String, String> {
    with_body(&drawn(footprints, stock, part)?, part)
}

/// The part's body outline seen from the top, in footprint coordinates (y down).
fn body_outline(part: &Part) -> Vec<[f64; 2]> {
    let rect = |l: f64, w: f64| {
        let (x, y) = (l / 2.0, w / 2.0);
        vec![[-x, -y], [x, -y], [x, y], [-x, y]]
    };
    match &part.body {
        Body::Chip(c) => rect(c.length, c.width),
        Body::Power(p) => outline(p),
        Body::Shunt(s) => rect(s.model.length.nominal, s.model.width.nominal),
        Body::Led(l, _) => rect(l.length, l.width),
    }
}

/// The text with the part's body on Body.F (0.05 mm lines, as the fab layer's) and its x-ray view on Xray.F in
/// place of what it drew there: the body a rectangle as `fp_rect`, any other outline as `fp_poly`, the x-ray view
/// every edge of the model seen from above ([`xray::lines`]) as `fp_line`s, before the first pad.
fn with_body(text: &str, part: &Part) -> Result<String, String> {
    let (body, xray) = (crate::layers::BODY[0], crate::layers::XRAY[0]);
    let fp = sexpr::parse(text).map_err(|e| e.to_string())?;
    let mut edits: Vec<Edit> = fp
        .items()
        .iter()
        .filter(|n| {
            n.head().is_some_and(|h| h.starts_with("fp_"))
                && matches!(n.value("layer"), Some(l) if l == body || l == xray)
        })
        .map(|n| Edit { range: led::whole_line(text, n.span()), text: String::new() })
        .collect();
    let pad = fp.child("pad").ok_or("no pad")?;
    let at = crate::footprint::line_start(text, pad.span().start);
    let pts: Vec<(f64, f64)> = body_outline(part).into_iter().map(|[x, y]| (x, y)).collect();
    let (hx, hy) = pts.iter().fold((0f64, 0f64), |(x, y), p| (x.max(p.0.abs()), y.max(p.1.abs())));
    let square = pts.len() == 4 && pts.iter().all(|p| p.0.abs() == hx && p.1.abs() == hy);
    let mut items = if square {
        crate::footprint::render_graphic("fp_rect", &[(-hx, -hy), (hx, hy)], 0.05, false, body)
    } else {
        crate::footprint::render_graphic("fp_poly", &pts, 0.05, false, body)
    };
    for [a, b] in xray::lines(&crate::chip::model(part)) {
        items += &crate::footprint::render_graphic("fp_line", &[a, b], 0.05, false, xray);
    }
    edits.push(Edit { range: at..at, text: items });
    edits.sort_by_key(|e| e.range.start);
    Ok(apply_edits(text, &edits))
}

/// The part's footprint before its body is drawn.
fn drawn(footprints: &Path, stock: Option<&Path>, part: &Part) -> Result<String, String> {
    match &part.body {
        Body::Chip(c) => {
            let base = format!("{:?}_{}_{}Metric", c.kind, c.size, c.metric);
            let text = base_text(footprints, stock, part.library(), &base)?;
            let descr = format!(
                "{}, body {} x {} x {} mm (height {} mm), model by kilib model chip",
                c.kind_name(),
                mm(c.length),
                mm(c.width),
                mm(part.height),
                mm(part.height)
            );
            derive(&text, part, &descr)
        }
        Body::Power(p) => {
            let mut descr = format!(
                "{:?} power inductor, body {} x {} x {} mm",
                p.family,
                mm(p.length),
                mm(p.width),
                mm(part.height)
            );
            if !p.land_source.is_empty() {
                descr.push_str(&format!(", land from {}", p.land_source));
            }
            if !p.fits.is_empty() {
                descr.push_str(&format!(", fits {}", p.fits.join(", ")));
            }
            descr.push_str(", model by kilib model chip");
            let tags = format!("inductor power {}", format!("{:?}", p.family).to_lowercase());
            let land = Land { x: p.pad_x, length: p.pad_length, width: p.pad_width };
            two_pads(part, &outline(p), land, &descr, &tags)
        }
        Body::Shunt(s) => {
            let (x, y) = (s.model.length.nominal / 2.0, s.model.width.nominal / 2.0);
            let body = [[x, -y], [x, y], [-x, y], [-x, -y]];
            let land = Land { x: s.land.pad_x(), length: s.land.pad_length(), width: s.land.width };
            two_pads(part, &body, land, &s.description(part.height), "resistor shunt current sense")
        }
        Body::Led(l, c) => {
            let text = led::footprint_base(&base_text(footprints, stock, part.library(), &l.base())?, l)?;
            let descr = format!(
                "LED SMD {} ({} Metric), {}, IPC-7351 nominal land, body {} x {} x {} mm, model by kilib model chip",
                l.size,
                l.metric,
                c.name().to_lowercase(),
                mm(l.length),
                mm(l.width),
                mm(l.height)
            );
            // The new silkscreen and the name go through the library rules' fixes again.
            stock::import_into(&derive(&text, part, &descr)?, part.library(), &crate::lint::Context::default())
        }
    }
}

/// The base footprint `base` of library `lib`: the checkout's copy, else a fresh import of the stock one.
fn base_text(footprints: &Path, stock: Option<&Path>, lib: &str, base: &str) -> Result<String, String> {
    let ours = footprints.join(format!("{lib}.pretty")).join(format!("{base}.kicad_mod"));
    if let Ok(t) = std::fs::read_to_string(&ours) {
        return Ok(t);
    }
    let stock = stock.ok_or(format!("{lib}:{base} is not in the checkout and no stock folder given"))?;
    let stock_lib = lib.strip_prefix(stock::PREFIX).unwrap_or(lib);
    let from = stock.join(format!("{stock_lib}.pretty")).join(format!("{base}.kicad_mod"));
    let t = std::fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
    stock::import_into(&t, lib, &crate::lint::Context::default())
}

impl crate::chip::Chip {
    fn kind_name(&self) -> String {
        let what = match self.kind {
            crate::chip::Kind::R => "Resistor",
            crate::chip::Kind::C => "Capacitor",
            crate::chip::Kind::L => "Inductor",
        };
        format!("{what} SMD {} ({} Metric), IPC-7351 nominal land", self.size, self.metric)
    }
}

fn atom_edit(text: &str, node: &Node, index: usize, value: &str) -> Result<Edit, String> {
    let atom = node.items().get(index).ok_or("item missing")?;
    if atom.as_atom().is_none() {
        return Err(format!("not a word at {:?}", &text[atom.span()]));
    }
    Ok(Edit { range: atom.span(), text: quote(value) })
}

/// The base footprint's text under the part's name: footprint name, Value, description and model changed.
fn derive(text: &str, part: &Part, descr: &str) -> Result<String, String> {
    let fp = sexpr::parse(text).map_err(|e| e.to_string())?;
    let mut edits = vec![atom_edit(text, &fp, 1, &part.name)?];
    let value = fp.children("property").find(|p| p.arg(0) == Some("Value")).ok_or("no Value property")?;
    edits.push(atom_edit(text, value, 2, &part.name)?);
    let d = fp.child("descr").ok_or("no descr")?;
    edits.push(atom_edit(text, d, 1, descr)?);
    let mut models = fp.children("model");
    let m = models.next().ok_or("no model")?;
    if models.next().is_some() {
        return Err("more than one model".into());
    }
    edits.push(atom_edit(text, m, 1, &part.model_ref())?);
    edits.sort_by_key(|e| e.range.start);
    Ok(apply_edits(text, &edits))
}

/// Two pads at -x and +x, each `length` along x and `width` across.
struct Land {
    x: f64,
    length: f64,
    width: f64,
}

/// A footprint drawn from a table row: two pads, the body outline on the fab layer and silkscreen along the long
/// sides, finished by the import (lint fixes, designator, Value).
fn two_pads(part: &Part, body: &[[f64; 2]], land: Land, descr: &str, tags: &str) -> Result<String, String> {
    let lib = part.library();
    let mut fab = String::new();
    for c in body {
        fab.push_str(&format!(" (xy {} {})", mm6(c[0]), mm6(c[1])));
    }
    // Silkscreen along the long sides, 0.1 mm outside the body; the lint fix cuts it round the pads.
    let (lx, ly) = body.iter().fold((0f64, 0f64), |(x, y), c| (x.max(c[0].abs()), y.max(c[1].abs())));
    let (sx, sy) = (lx + 0.1, ly + 0.1);
    let silk = |y: f64| {
        format!(
            "\t(fp_line\n\t\t(start {} {})\n\t\t(end {} {})\n\t\t(stroke\n\t\t\t(width 0.12)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.SilkS\")\n\t)\n",
            mm6(-sx),
            mm6(y),
            mm6(sx),
            mm6(y)
        )
    };
    let pad = |n: u32, x: f64| {
        format!(
            "\t(pad \"{n}\" smd roundrect\n\t\t(at {} 0)\n\t\t(size {} {})\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t\t(roundrect_rratio 0.1)\n\t)\n",
            mm6(x),
            mm6(land.length),
            mm6(land.width)
        )
    };
    let text = format!(
        "(footprint {name}\n\t(version 20260206)\n\t(generator \"kilib\")\n\t(layer \"F.Cu\")\n\t(descr {descr})\n\t(tags {tags})\n\
         \t(property \"Reference\" \"REF**\"\n\t\t(at 0 0 0)\n\t\t(layer \"F.SilkS\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n\
         \t(property \"Value\" {name}\n\t\t(at 0 0 0)\n\t\t(layer \"F.Fab\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n\
         \t(attr smd)\n\t(duplicate_pad_numbers_are_jumpers no)\n{top}{bottom}\
         \t(fp_poly\n\t\t(pts{fab})\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"F.Fab\")\n\t)\n\
         {pad1}{pad2}\t(embedded_fonts no)\n\
         \t(model {model}\n\t\t(offset\n\t\t\t(xyz 0 0 0)\n\t\t)\n\t\t(scale\n\t\t\t(xyz 1 1 1)\n\t\t)\n\t\t(rotate\n\t\t\t(xyz 0 0 0)\n\t\t)\n\t)\n)\n",
        name = quote(&part.name),
        descr = quote(descr),
        tags = quote(tags),
        top = silk(-sy),
        bottom = silk(sy),
        pad1 = pad(1, -land.x),
        pad2 = pad(2, land.x),
        model = quote(&part.model_ref()),
    );
    stock::import_into(&text, lib, &crate::lint::Context::default())
}

/// A length as KiCad writes it: up to six decimals, no trailing zeros.
fn mm6(v: f64) -> String {
    let v = if v.abs() < 5e-7 { 0.0 } else { v };
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chip::Tables;
    use crate::testdir::TestDir;

    const TABLE: &str = r#"
[[chip]]
kind = "C"
size = "0805"
metric = "2012"
style = "mlcc"
length = 2.0
width = 1.25
band = 0.5
heights = [0.85, 1.25]

[[power]]
family = "Shielded"
length = 4.0
width = 4.0
heights = [2.0]
corners = "chamfer"
corner = 0.6
term_length = 1.0
term_width = 3.2
term_x = 1.5
pad_x = 1.55
pad_length = 1.5
pad_width = 3.6
land_source = "Sunlord SWPA4020S"
fits = ["Sunlord SWPA4020S"]

[[shunt]]
series = ["Yageo PE1206"]
size = "1206"
metric = "3216"
range = "5–8 mΩ"
length = [3.2, 0.25]
width = [1.6, 0.25]
term = [1.15, 0.25]
heights = [0.6]
maker_land = { pad_length = 2.8, pad_width = 1.84, gap = 0.6, source = "PE V.2 Table 7" }
"#;

    /// A stock-like base: what KiCad's generator writes, cut down.
    const BASE: &str = "(footprint \"C_0805_2012Metric\"\n\t(version 20260206)\n\t(generator \"kicad-footprint-generator\")\n\t(layer \"F.Cu\")\n\t(descr \"Capacitor SMD 0805 (2012 Metric)\")\n\t(tags \"capacitor\")\n\t(property \"Reference\" \"REF**\"\n\t\t(at 0 0 0)\n\t\t(layer \"User.23\")\n\t)\n\t(property \"Value\" \"C_0805_2012Metric\"\n\t\t(at 0 0 0)\n\t\t(layer \"User.17\")\n\t)\n\t(attr smd)\n\t(pad \"1\" smd roundrect\n\t\t(at -0.95 0)\n\t\t(size 1 1.45)\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t\t(roundrect_rratio 0.25)\n\t)\n\t(pad \"2\" smd roundrect\n\t\t(at 0.95 0)\n\t\t(size 1 1.45)\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t\t(roundrect_rratio 0.25)\n\t)\n\t(model \"${VHRD_3DMODEL_DIR}/vendor/VHRD_Capacitor_SMD.3dshapes/C_0805_2012Metric.step\"\n\t\t(offset\n\t\t\t(xyz 0 0 0)\n\t\t)\n\t)\n)\n";

    fn setup() -> (TestDir, Vec<Part>) {
        let dir = TestDir::new("chip-footprints");
        dir.write("footprints/VHRD_Capacitor_SMD.pretty/C_0805_2012Metric.kicad_mod", BASE);
        let parts = toml::from_str::<Tables>(TABLE).unwrap().parts().unwrap();
        (dir, parts)
    }

    #[test]
    fn chip_footprints_copy_the_base() {
        let (dir, parts) = setup();
        let items = plan(dir.path(), None, &parts[..2]);
        let Outcome::New(text) = &items[1].outcome else { panic!("{:?}", items[1].outcome) };
        assert!(text.starts_with("(footprint \"C_0805_2012Metric_H1.25\"\n"));
        assert!(text.contains("(property \"Value\" \"C_0805_2012Metric_H1.25\""));
        assert!(text.contains(
            "(descr \"Capacitor SMD 0805 (2012 Metric), IPC-7351 nominal land, body 2.0 x 1.25 x 1.25 mm (height 1.25 mm), model by kilib model chip\")"
        ));
        assert!(text.contains(
            "(model \"${VHRD_3DMODEL_DIR}/generated/VHRD_Capacitor_SMD.3dshapes/C_0805_2012Metric_H1.25.step\""
        ));
        // Pads as in the base.
        let pads = |t: &str| t[t.find("(pad").unwrap()..t.find("(model").unwrap()].to_string();
        assert_eq!(pads(text), pads(BASE));
        assert!(
            text.contains("\t(fp_rect\n\t\t(start -1 -0.625)\n\t\t(end 1 0.625)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"User.5\")\n\t)\n\t(fp_line"),
            "the table body on Body.F: {text}"
        );
        // X-ray: the outline and the inner edges of the two terminations.
        let fp = crate::footprint::read(text).unwrap();
        let xray: Vec<_> = fp.on("User.7").collect();
        assert_eq!(xray.len(), 6, "{text}");
        assert!(text.contains("\t\t(start -0.5 -0.625)\n\t\t(end -0.5 0.625)\n"), "a band line: {text}");
        assert_eq!(apply(&items).unwrap(), 2);
        let again = plan(dir.path(), None, &parts[..2]);
        assert!(again.iter().all(|i| i.outcome == Outcome::Same));
        dir.write("footprints/VHRD_Capacitor_SMD.pretty/C_0805_2012Metric_H0.85.kicad_mod", "(footprint \"x\")\n");
        assert_eq!(plan(dir.path(), None, &parts[..1])[0].outcome, Outcome::Differs);
    }

    #[test]
    fn a_missing_base_without_stock_is_an_error() {
        let (dir, mut parts) = setup();
        if let Body::Chip(c) = &mut parts[0].body {
            c.size = "1206".into();
            c.metric = "3216".into();
        }
        let items = plan(dir.path(), None, &parts[..1]);
        assert!(matches!(&items[0].outcome, Outcome::Error(e) if e.contains("C_1206_3216Metric")));
        assert!(apply(&items).unwrap_err().contains("nothing written"));
    }

    #[test]
    fn power_footprints_are_drawn_and_linted() {
        let (dir, parts) = setup();
        let items = plan(dir.path(), None, &parts[2..3]);
        let Outcome::New(text) = &items[0].outcome else { panic!("{:?}", items[0].outcome) };
        let fp = crate::footprint::read(text).unwrap();
        assert_eq!(fp.name, "L_Shielded_4.0x4.0mm_H2.0");
        let pads: Vec<_> = fp.copper_pads().collect();
        assert_eq!(pads.len(), 2);
        assert!(text.contains("(at -1.55 0)") && text.contains("(size 1.5 3.6)"));
        assert!(text.contains("F.CrtYd"), "the lint fix draws the courtyard");
        assert!(text.contains("\"User.23\""), "the designator on Ref.Des.F");
        let body: Vec<_> = fp.on("User.5").collect();
        assert_eq!((body.len(), body[0].kind.as_str(), body[0].pts.len()), (1, "fp_poly", 8), "chamfered body");
        // X-ray: the chamfered outline and the two bottom electrodes; their outer edges lie on the body's sides.
        assert_eq!(fp.on("User.7").count(), 8 + 2 * 3, "{text}");
        // Nothing left for the lint fixes.
        let (_, new, _) = crate::lint::lint_text(
            text,
            "VHRD_Inductor_SMD",
            &crate::lint::Rule::ALL,
            &crate::lint::Context::default(),
        )
        .unwrap();
        assert!(new.is_none(), "lint would still change it");
    }

    #[test]
    fn shunt_footprints_have_the_ipc_land() {
        let (dir, parts) = setup();
        let items = plan(dir.path(), None, &parts[3..]);
        let Outcome::New(text) = &items[0].outcome else { panic!("{:?}", items[0].outcome) };
        let fp = crate::footprint::read(text).unwrap();
        assert_eq!(fp.name, "R_Shunt_1206_3216Metric_H0.6_Pad1.85x1.85mm_Gap0.45mm");
        // IPC-7351B nominal from 3.2 ±0.25 long, terminations 1.15 ±0.25: span 4.15, gap 0.45, pads 1.85 long
        // centred at 1.15; the maker's 6.20 mm land only in the text.
        assert!(text.contains("(at -1.15 0)") && text.contains("(at 1.15 0)") && text.contains("(size 1.85 1.85)"));
        assert!(text.contains(
            "pads 1.85 x 1.85 mm, gap 0.45 mm; fits Yageo PE1206 5–8 mΩ (maker's land 2.80 x 1.84 mm, gap 0.60 mm, PE V.2 Table 7)"
        ));
        assert!(text.contains(
            "generated/VHRD_Resistor_SMD.3dshapes/R_Shunt_1206_3216Metric_H0.6_Pad1.85x1.85mm_Gap0.45mm.step"
        ));
        let (_, new, _) = crate::lint::lint_text(
            text,
            "VHRD_Resistor_SMD",
            &crate::lint::Rule::ALL,
            &crate::lint::Context::default(),
        )
        .unwrap();
        assert!(new.is_none(), "lint would still change it");
    }
}
