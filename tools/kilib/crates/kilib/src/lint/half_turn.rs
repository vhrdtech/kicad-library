//! Whether a part placed half a turn round works the same: then a pin-1 mark only makes people look for an
//! orientation that does not matter, and the part gets none.
//!
//! The pads first: turned half round, about the footprint's origin or about the pads' centre, every pad has to land
//! on a pad of the same kind and size. That takes each pad number to another (or to itself). Then the circuit,
//! from every library symbol whose `Footprint` field names the footprint: pins of the same name are one node (a
//! crystal's two `G`, a slide switch's `A`, `B` and `C`), and so are the pins [`CONNECTED`] lists, where the part
//! connects pins inside that its symbol names apart (a tact switch's pairs). The turn keeps the circuit when it
//! takes every node to itself, or swaps the only two nodes it moves of a part that has no polarity
//! ([`NON_POLAR`]: a crystal's two terminals, a switch's two sides). A footprint no symbol uses, or one a symbol
//! uses where the turn changes the circuit, keeps its mark.

use std::collections::BTreeMap;
use std::path::Path;

use super::pin1::SAME;
use crate::footprint::{Footprint, Pt, Rect, dist};
use crate::layers::prefix;
use crate::sexpr::{self, Node};

/// The table of pins connected inside a part, beside the checkout's symbols: `generate/connected-pins.toml`.
pub const CONNECTED: &str = "generate/connected-pins.toml";

/// Designator prefixes of parts with two terminals and no polarity: resistors, capacitors, inductors, ferrite
/// beads, crystals, fuses, switches.
pub const NON_POLAR: [&str; 7] = ["R", "C", "L", "FB", "Y", "F", "SW"];

/// Pad sizes within this are the same.
const SAME_SIZE: f64 = 0.02;

/// One symbol's pins, as the half-turn rule sees them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Symbol {
    /// `VHRD_Switch:GSPTS0020`.
    pub id: String,
    /// The designator's letters (`SW`).
    pub prefix: String,
    /// Number and name of every pin.
    pub pins: Vec<(String, String)>,
    /// Pin numbers the part connects inside, each group one node.
    pub connected: Vec<Vec<String>>,
}

/// The symbols that use each footprint, by footprint id (`VHRD_Button_Switch_SMD:PTS815`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Uses(pub BTreeMap<String, Vec<Symbol>>);

impl Uses {
    /// The symbols of the checkout `root` (`symbols/*.kicad_symdir`), with its [`CONNECTED`] table.
    pub fn load(root: &Path) -> Result<Uses, String> {
        let table = root.join(CONNECTED);
        let connected: BTreeMap<String, Vec<Vec<String>>> = match std::fs::read_to_string(&table) {
            Ok(t) => toml::from_str(&t).map_err(|e| format!("{}: {e}", table.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(format!("{}: {e}", table.display())),
        };
        let mut uses = Uses::default();
        let Ok(libs) = std::fs::read_dir(root.join("symbols")) else { return Ok(uses) };
        let mut dirs: Vec<_> =
            libs.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "kicad_symdir")).collect();
        dirs.sort();
        for dir in dirs {
            let lib = dir.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let Ok(files) = std::fs::read_dir(&dir) else { continue };
            let mut files: Vec<_> =
                files.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "kicad_sym")).collect();
            files.sort();
            for file in files {
                let Some((footprint, mut sym)) = read_symbol(&dir, &file)? else { continue };
                sym.id =
                    format!("{lib}:{}", file.file_stem().unwrap_or_default().to_string_lossy().replace("{slash}", "/"));
                sym.connected = connected.get(&sym.id).cloned().unwrap_or_default();
                uses.0.entry(footprint).or_default().push(sym);
            }
        }
        Ok(uses)
    }
}

/// The symbol in `file` with the footprint it names; `None` without one. A derived symbol takes its pins and
/// designator from its parent, in the same folder.
fn read_symbol(dir: &Path, file: &Path) -> Result<Option<(String, Symbol)>, String> {
    let parse = |path: &Path| -> Result<Node, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let root = sexpr::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        root.child("symbol").cloned().ok_or_else(|| format!("{}: no symbol", path.display()))
    };
    let sym = parse(file)?;
    let property = |s: &Node, name: &str| -> Option<String> {
        s.children("property").find(|p| p.arg(0) == Some(name)).and_then(|p| p.arg(1)).map(str::to_owned)
    };
    let Some(footprint) = property(&sym, "Footprint").filter(|f| !f.is_empty()) else { return Ok(None) };
    let base = match sym.value("extends") {
        Some(parent) => parse(&dir.join(format!("{}.kicad_sym", parent.replace('/', "{slash}"))))?,
        None => sym.clone(),
    };
    let reference = property(&sym, "Reference").or_else(|| property(&base, "Reference")).unwrap_or_default();
    let pins = base
        .children("symbol")
        .flat_map(|unit| unit.children("pin"))
        .filter_map(|p| Some((p.value("number")?.to_owned(), p.value("name").unwrap_or_default().to_owned())))
        .collect();
    Ok(Some((footprint, Symbol { prefix: prefix(&reference).to_owned(), pins, ..Symbol::default() })))
}

/// Where half a turn takes each pad number, about the origin or else the pads' centre: `None` when the turned pads
/// do not land on pads of the same kind and size.
pub fn turn(fp: &Footprint) -> Option<BTreeMap<String, String>> {
    let pads: Vec<_> = fp.copper_pads().filter(|p| !p.number.is_empty()).collect();
    let centre = Rect::of_points(pads.iter().map(|p| p.at))?.center();
    [(0.0, 0.0), centre].into_iter().find_map(|c: Pt| {
        let mut map = BTreeMap::new();
        for p in &pads {
            let to = (2.0 * c.0 - p.at.0, 2.0 * c.1 - p.at.1);
            let pb = p.outline.bbox();
            let q = pads.iter().find(|q| {
                let qb = q.outline.bbox();
                dist(q.at, to) < SAME
                    && q.kind == p.kind
                    && (qb.w() - pb.w()).abs() < SAME_SIZE
                    && (qb.h() - pb.h()).abs() < SAME_SIZE
            })?;
            if map.insert(p.number.clone(), q.number.clone()).is_some_and(|old| old != q.number) {
                return None;
            }
        }
        Some(map)
    })
}

/// The turn `map` (pad number to pad number) keeps `sym`'s circuit.
pub fn keeps(sym: &Symbol, map: &BTreeMap<String, String>) -> bool {
    // A pin's node: the first pin number of its name, or of its connected group; a pad the symbol has no pin for
    // (a mounting pad) is in no circuit.
    let node = |n: &str| -> String {
        if let Some(g) = sym.connected.iter().find(|g| g.iter().any(|m| m == n)) {
            return format!("group {}", g[0]);
        }
        match sym.pins.iter().find(|(num, _)| num == n) {
            Some((_, name)) if !name.is_empty() && name != "~" => format!("name {name}"),
            Some((num, _)) => format!("pin {num}"),
            None => "none".to_owned(),
        }
    };
    let mut to: BTreeMap<String, String> = BTreeMap::new();
    for (a, b) in map {
        let (na, nb) = (node(a), node(b));
        if to.insert(na, nb.clone()).is_some_and(|old| old != nb) {
            return false;
        }
    }
    let moved: Vec<(&String, &String)> = to.iter().filter(|(a, b)| a != b).collect();
    match moved.as_slice() {
        [] => true,
        [(a, b), (c, d)] => a == d && b == c && NON_POLAR.contains(&sym.prefix.as_str()),
        _ => false,
    }
}

/// Half a turn of footprint `id` keeps the circuit of every symbol that uses it, and some symbol does.
pub fn keeps_circuit(fp: &Footprint, id: &str, uses: &Uses) -> bool {
    let Some(syms) = uses.0.get(id).filter(|s| !s.is_empty()) else { return false };
    turn(fp).is_some_and(|map| syms.iter().all(|s| keeps(s, &map)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(prefix: &str, pins: &[(&str, &str)], connected: &[&[&str]]) -> Symbol {
        Symbol {
            id: "L:S".to_owned(),
            prefix: prefix.to_owned(),
            pins: pins.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect(),
            connected: connected.iter().map(|g| g.iter().map(|s| (*s).to_owned()).collect()).collect(),
        }
    }

    fn half(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .flat_map(|(a, b)| [((*a).to_owned(), (*b).to_owned()), ((*b).to_owned(), (*a).to_owned())])
            .collect()
    }

    #[test]
    fn nodes_moved_onto_themselves_or_swapped_on_a_non_polar_part() {
        // A crystal: 1 and 3 swap, the two grounds swap with each other.
        let crystal = sym("Y", &[("1", "1"), ("2", "G"), ("3", "3"), ("4", "G")], &[]);
        assert!(keeps(&crystal, &half(&[("1", "3"), ("2", "4")])));
        // The same pins on an IC: swapping two nodes changes the circuit.
        assert!(!keeps(&Symbol { prefix: "U".to_owned(), ..crystal.clone() }, &half(&[("1", "3"), ("2", "4")])));
        // A switch whose pairs are connected inside: its two sides swap.
        let switch = sym("SW", &[("1", "1"), ("2", "2"), ("3", "K"), ("4", "A")], &[&["1", "2"], &["3", "4"]]);
        assert!(keeps(&switch, &half(&[("1", "4"), ("2", "3")])));
        // Without the table: four nodes, two swaps.
        assert!(!keeps(
            &sym("SW", &[("1", "1"), ("2", "2"), ("3", "K"), ("4", "A")], &[]),
            &half(&[("1", "4"), ("2", "3")])
        ));
        // An optocoupler's LED and output side.
        let opto = sym("U", &[("1", "A"), ("2", "K"), ("3", "E"), ("4", "C")], &[]);
        assert!(!keeps(&opto, &half(&[("1", "3"), ("2", "4")])));
        // A DPDT slide switch whose throws carry the same names: every node onto itself.
        let dpdt = sym("SW", &[("1", "A"), ("2", "C"), ("3", "B"), ("4", "B"), ("5", "C"), ("6", "A")], &[]);
        assert!(keeps(&dpdt, &half(&[("1", "6"), ("2", "5"), ("3", "4")])));
    }

    #[test]
    fn loads_the_symbols_a_footprint_is_used_by() {
        let dir = crate::testdir::TestDir::new("half-turn");
        let pin = |n: &str, name: &str| {
            format!("(pin passive line (at 0 0 0) (length 2.54) (name \"{name}\") (number \"{n}\"))")
        };
        let base = format!(
            "(kicad_symbol_lib (symbol \"Crystal_GND24\" (property \"Reference\" \"Y\") (property \"Footprint\" \"\") \
             (symbol \"Crystal_GND24_1_1\" {} {} {} {})))\n",
            pin("1", "1"),
            pin("2", "G"),
            pin("3", "3"),
            pin("4", "G")
        );
        dir.write("symbols/VHRD_Device.kicad_symdir/Crystal_GND24.kicad_sym", &base);
        dir.write(
            "symbols/VHRD_Device.kicad_symdir/Crystal_GND24_3225.kicad_sym",
            "(kicad_symbol_lib (symbol \"Crystal_GND24_3225\" (extends \"Crystal_GND24\") \
             (property \"Footprint\" \"VHRD_Crystal:Crystal_3225\")))\n",
        );
        dir.write("generate/connected-pins.toml", "\"VHRD_Device:Crystal_GND24_3225\" = [[\"2\", \"4\"]]\n");
        let uses = Uses::load(dir.path()).unwrap();
        let syms = &uses.0["VHRD_Crystal:Crystal_3225"];
        assert_eq!(syms.len(), 1, "{uses:?}");
        assert_eq!(syms[0].id, "VHRD_Device:Crystal_GND24_3225");
        assert_eq!(syms[0].prefix, "Y");
        assert_eq!(syms[0].pins.len(), 4);
        assert_eq!(syms[0].connected, vec![vec!["2".to_owned(), "4".to_owned()]]);
    }

    use super::super::tests::{fp, run_in, settled};
    use super::super::{Context, Rule};

    fn pad(n: &str, x: f64, y: f64) -> String {
        format!("(pad \"{n}\" smd rect (at {x} {y}) (size 1 0.6) (layers \"F.Cu\" \"F.Mask\"))")
    }

    /// A four-pad part, pins `1`..`4` at `at`, drawn the way the library draws pin 1: a fab outline chamfered by
    /// pin 1, a silkscreen triangle, an Xray dimple and a Pin1 dot.
    fn four(name: &str, at: [(f64, f64); 4]) -> String {
        let (x1, y1) = at[0];
        let pads: Vec<String> = at.iter().enumerate().map(|(k, &(x, y))| pad(&(k + 1).to_string(), x, y)).collect();
        let sx = if x1 < 0.0 { -1.0 } else { 1.0 };
        let sy = if y1 < 0.0 { -1.0 } else { 1.0 };
        let (w, h) = (1.6, 1.6);
        let fab = [
            (w * sx, h * sy - 0.5 * sy),
            (w * sx - 0.5 * sx, h * sy),
            (-w * sx, h * sy),
            (-w * sx, -h * sy),
            (w * sx, -h * sy),
        ];
        let xy: String = fab.iter().map(|(x, y)| format!("(xy {x} {y}) ")).collect();
        let tri = format!(
            "(xy {} {y1}) (xy {} {}) (xy {} {})",
            x1 + 1.0 * sx,
            x1 + 1.3 * sx,
            y1 - 0.2,
            x1 + 1.3 * sx,
            y1 + 0.2
        );
        let (cx, cy) = (1.1 * sx, 1.1 * sy);
        let body = format!(
            "(fp_poly (pts {xy}) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.Fab\"))\n\
             (fp_poly (pts {tri}) (stroke (width 0.12) (type solid)) (fill yes) (layer \"F.SilkS\"))\n\
             (fp_rect (start -1.5 -1.5) (end 1.5 1.5) (stroke (width 0.05) (type solid)) (fill no) (layer \"User.7\"))\n\
             (fp_circle (center {cx} {cy}) (end {cx} {}) (stroke (width 0.05) (type solid)) (fill no) (layer \"User.7\"))\n\
             (fp_circle (center {x1} {y1}) (end {} {y1}) (stroke (width 0) (type solid)) (fill yes) (layer \"User.29\"))\n{}",
            cy + 0.1,
            x1 + 0.15,
            pads.join("\n")
        );
        fp(name, &body)
    }

    fn uses(id: &str, s: Symbol) -> Context {
        Context { uses: Some(Uses([(id.to_owned(), vec![s])].into())), ..Context::default() }
    }

    /// Runs the pin-1 rules: what Pin1 and the silkscreen, fab and Xray layers keep.
    fn marks(lib: &str, text: &str, ctx: &Context) -> String {
        let (new, _) = run_in(lib, text, Rule::Pin1, ctx);
        let text = new.unwrap_or_else(|| text.to_owned());
        let (new, _) = run_in(lib, &text, Rule::Pin1Body, ctx);
        new.unwrap_or(text)
    }

    #[test]
    fn a_tact_switch_with_joined_pairs_loses_every_mark() {
        // PTS815: pads 1 and 2 along the top, 3 and 4 along the bottom; 1-2 and 3-4 joined inside.
        let t = four("PTS815", [(-2.25, -1.075), (2.25, -1.075), (-2.25, 1.075), (2.25, 1.075)]);
        let pins = [("1", "1"), ("2", "2"), ("3", "K"), ("4", "A")];
        let joined = uses("VHRD_Button_Switch_SMD:PTS815", sym("SW", &pins, &[&["1", "2"], &["3", "4"]]));
        let new = marks("VHRD_Button_Switch_SMD", &t, &joined);
        assert!(!new.contains("User.29") && !new.contains("F.SilkS"), "{new}");
        assert!(new.contains("(xy -1.6 -1.6)") && !new.contains("(center -1.1 -1.1)"), "chamfer squared:\n{new}");
        assert!(new.matches("\"User.7\"").count() == 1, "dimple gone, outline kept:\n{new}");
        for rule in [Rule::Pin1, Rule::Pin1Body] {
            assert!(run_in("VHRD_Button_Switch_SMD", &new, rule, &joined).0.is_none());
        }
        // The pairs not known to be joined: four nodes, the dot stays.
        let apart = uses("VHRD_Button_Switch_SMD:PTS815", sym("SW", &pins, &[]));
        assert!(marks("VHRD_Button_Switch_SMD", &t, &apart).contains("User.29"));
        // No symbol uses it: the dot stays.
        settled("VHRD_Button_Switch_SMD", &t, Rule::Pin1Body);
        assert!(marks("VHRD_Button_Switch_SMD", &t, &Context::default()).contains("User.29"));
    }

    #[test]
    fn a_four_pad_crystal_loses_its_dot_and_an_optocoupler_keeps_it() {
        let at = [(-1.1, 0.8), (1.1, 0.8), (1.1, -0.8), (-1.1, -0.8)];
        let crystal = four("Crystal_SMD_3225-4Pin", at);
        let ctx = uses(
            "VHRD_Crystal:Crystal_SMD_3225-4Pin",
            sym("Y", &[("1", "1"), ("2", "G"), ("3", "3"), ("4", "G")], &[]),
        );
        assert!(!marks("VHRD_Crystal", &crystal, &ctx).contains("User.29"));
        // SOP-4: the LED on 1-2, the transistor on 3-4; a half turn puts the LED where the output was.
        let opto = four("SOP-4_4.4x4.3mm_P2.54mm", [(-3.3, -1.27), (-3.3, 1.27), (3.3, 1.27), (3.3, -1.27)]);
        let ctx = uses(
            "VHRD_Package_SO:SOP-4_4.4x4.3mm_P2.54mm",
            sym("U", &[("1", ""), ("2", ""), ("3", ""), ("4", "")], &[]),
        );
        let new = marks("VHRD_Package_SO", &opto, &ctx);
        assert!(new.contains("User.29") && new.contains("F.SilkS"), "{new}");
    }

    #[test]
    fn a_sot23_loses_every_mark_and_a_soic8_keeps_its_dot() {
        let pads = [pad("1", -1.1, -0.95), pad("2", -1.1, 0.95), pad("3", 1.1, 0.0)];
        let sot = fp(
            "SOT-23",
            &format!(
                "(fp_poly (pts (xy -0.325 -1.45) (xy 0.65 -1.45) (xy 0.65 1.45) (xy -0.65 1.45) (xy -0.65 -1.125)) \
                 (stroke (width 0.05) (type solid)) (fill no) (layer \"F.Fab\"))\n\
                 (fp_poly (pts (xy -1.3 -1.5) (xy -1.54 -1.83) (xy -1.06 -1.83)) (stroke (width 0.12) (type solid)) \
                 (fill yes) (layer \"F.SilkS\"))\n\
                 (fp_line (start -1.9 -0.95) (end -2.2 -0.75) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))\n\
                 (fp_line (start -2.2 -0.75) (end -2.2 -1.15) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))\n\
                 (fp_line (start -2.2 -1.15) (end -1.9 -0.95) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))\n\
                 (fp_circle (center -0.428 -1.228) (end -0.428 -1.178) (stroke (width 0.05) (type solid)) (fill no) \
                 (layer \"User.7\"))\n\
                 (fp_rect (start -0.7 -1.45) (end 0.7 1.45) (stroke (width 0.05) (type solid)) (fill no) (layer \"User.7\"))\n{}",
                pads.join("\n")
            ),
        );
        let new = marks("VHRD_Package_TO_SOT_SMD", &sot, &Context::default());
        assert!(!new.contains("F.SilkS") && !new.contains("fp_circle") && !new.contains("User.29"), "{new}");
        assert!(new.contains("(xy -0.65 -1.45)"), "{new}");
        settled("VHRD_Package_TO_SOT_SMD", &new, Rule::Pin1);
        // SOIC-8: a half turn takes pin 1 to pin 5 of another name.
        let pads: Vec<String> = (0..4)
            .flat_map(|k| {
                let y = -1.905 + 1.27 * f64::from(k);
                [pad(&(k + 1).to_string(), -2.5, y), pad(&(8 - k).to_string(), 2.5, y)]
            })
            .collect();
        let soic = fp("SOIC-8", &pads.join("\n"));
        let names = ["VCC", "SCL", "SDA", "GND", "A0", "A1", "A2", "WP"];
        let pins: Vec<(String, &str)> = (1..=8).map(|n| (n.to_string(), names[n - 1])).collect();
        let pins: Vec<(&str, &str)> = pins.iter().map(|(n, s)| (n.as_str(), *s)).collect();
        let ctx = uses("VHRD_Package_SO:SOIC-8", sym("U", &pins, &[]));
        assert!(run_in("VHRD_Package_SO", &soic, Rule::Pin1Body, &ctx).0.unwrap().contains("User.29"));
    }
}
