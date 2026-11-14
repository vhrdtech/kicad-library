//! KiCad's stock symbols that boards use, kept as copies of our own: `Lib:Name` from KiCad's symbol folder
//! (`${KICAD10_SYMBOL_DIR}`, a `Lib.kicad_symdir` per library) becomes `VHRD_Lib:Name` in the checkout's
//! `symbols/`, one file per symbol. The copy is the stock symbol made standalone (a derived one gets its parent's
//! pins, units and graphics under its own name and fields), with
//! - the `Footprint` field pointing at our copy of the footprint, where `footprints/` has one;
//! - the library part of each `ki_fp_filters` pattern (`Connector*:*_1x??_*`) prefixed `VHRD_`;
//! - an empty, hidden `Mpn1` field where the symbol has no `Mpn` field;
//! - the empty, hidden `Layout_Critical` and `Layout_Important` fields ([`crate::layout_fields`]).
//!
//! The fields and flags (`in_bom`, `exclude_from_sim`, ...) of a copy are ours to fill in; pins, units and graphics
//! are KiCad's. [`compare`] therefore lays a copy's fields and flags over a fresh import of the stock symbol, and
//! the diff that is left shows what taking in the stock symbol would change: pins, units, graphics and fields stock
//! has that the copy has not.

use std::path::{Path, PathBuf};

use super::{Compared, Comparison, Import, ImportOutcome, PREFIX, line_edits, read_dir, split_id};
use crate::migrate::{Edit, apply_edits, unified_diff};
use crate::sexpr::{self, Node};

/// The flags of a symbol that are ours in a copy.
const FLAGS: &[&str] = &["exclude_from_sim", "in_bom", "on_board", "in_pos_files", "duplicate_pin_numbers_are_jumpers"];

/// What a derived symbol may set over its parent's.
const OVERRIDES: &[&str] = &[
    "power",
    "pin_numbers",
    "pin_names",
    "exclude_from_sim",
    "in_bom",
    "on_board",
    "in_pos_files",
    "duplicate_pin_numbers_are_jumpers",
];

/// The file a symbol lives in: KiCad writes `/` in a name as `{slash}`.
pub fn sym_path(dir: &Path, lib: &str, name: &str) -> PathBuf {
    dir.join(format!("{lib}.kicad_symdir")).join(format!("{}.kicad_sym", name.replace('/', "{slash}")))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The one symbol of a `.kicad_sym` file of a symbol folder.
fn only_symbol(root: &Node) -> Result<&Node, String> {
    let mut syms = root.children("symbol");
    match (syms.next(), syms.next()) {
        (Some(s), None) => Ok(s),
        _ => Err("not one symbol in the file".into()),
    }
}

fn parse(text: &str) -> Result<Node, String> {
    sexpr::parse(text).map_err(|e| e.to_string())
}

/// The span from the first to the last `property` of a symbol; they must follow each other.
fn property_block(sym: &Node) -> Result<std::ops::Range<usize>, String> {
    let items = sym.items();
    let first = items.iter().position(|n| n.head() == Some("property")).ok_or("no properties")?;
    let last = items.iter().rposition(|n| n.head() == Some("property")).unwrap_or(first);
    if items[first..=last].iter().any(|n| n.head() != Some("property")) {
        return Err("the properties are not together".into());
    }
    Ok(items[first].span().start..items[last].span().end)
}

/// The whitespace a node's line starts with.
fn indent_of(text: &str, at: usize) -> &str {
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    &text[line..at]
}

/// The stock symbol `text` (in `lib_dir`, a `.kicad_symdir`) made standalone: a derived symbol becomes its parent's
/// text under its own name, with its own fields and the flags it sets.
pub fn standalone(text: &str, lib_dir: &Path) -> Result<String, String> {
    standalone_at(text, lib_dir, 0)
}

fn standalone_at(text: &str, lib_dir: &Path, depth: usize) -> Result<String, String> {
    let root = parse(text)?;
    let sym = only_symbol(&root)?;
    let Some(parent) = sym.child("extends").and_then(|e| e.arg(0)) else { return Ok(text.to_string()) };
    if depth > 8 {
        return Err(format!("extends {parent}: too deep"));
    }
    let name = sym.arg(0).ok_or("a symbol without a name")?;
    let parent_path = lib_dir.join(format!("{}.kicad_sym", parent.replace('/', "{slash}")));
    let parent_text = standalone_at(&read(&parent_path)?, lib_dir, depth + 1)?;
    let proot = parse(&parent_text)?;
    let psym = only_symbol(&proot)?;
    let props = property_block(psym)?;
    let mut edits = vec![Edit { range: psym.items()[1].span(), text: sexpr::quote(name) }];
    let indent = indent_of(&parent_text, props.start);
    for flag in OVERRIDES {
        let Some(c) = sym.child(flag) else { continue };
        let own = text[c.span()].to_string();
        match psym.child(flag) {
            Some(p) => edits.push(Edit { range: p.span(), text: own }),
            None => edits.push(Edit { range: props.start..props.start, text: format!("{own}\n{indent}") }),
        }
    }
    edits.push(Edit { range: props, text: text[property_block(sym)?].to_string() });
    for unit in psym.children("symbol") {
        let Some(n) = unit.items().get(1) else { continue };
        if let Some(rest) = n.as_atom().and_then(|u| u.strip_prefix(parent)).filter(|r| r.starts_with('_')) {
            edits.push(Edit { range: n.span(), text: sexpr::quote(&format!("{name}{rest}")) });
        }
    }
    edits.sort_by_key(|e| (e.range.start, e.range.end));
    Ok(apply_edits(&parent_text, &edits))
}

fn property<'a>(sym: &'a Node, name: &str) -> Option<&'a Node> {
    sym.children("property").find(|p| p.arg(0) == Some(name))
}

/// The library's fields on a standalone symbol: `Footprint` at our copy (where `footprints` has it), library parts
/// of `ki_fp_filters` prefixed, an empty `Mpn1` where no `Mpn` field is.
pub fn library_fields(text: &str, footprints: &Path) -> Result<String, String> {
    let root = parse(text)?;
    let sym = only_symbol(&root)?;
    let mut edits = Vec::new();
    if let Some(fp) = property(sym, "Footprint")
        && let Some(value) = fp.items().get(2)
        && let Some((lib, name)) = value.as_atom().and_then(|v| v.split_once(':'))
        && !lib.starts_with(PREFIX)
        && footprints.join(format!("{PREFIX}{lib}.pretty")).join(format!("{name}.kicad_mod")).is_file()
    {
        edits.push(Edit { range: value.span(), text: sexpr::quote(&format!("{PREFIX}{lib}:{name}")) });
    }
    if let Some(filters) = property(sym, "ki_fp_filters")
        && let Some(value) = filters.items().get(2)
        && let Some(v) = value.as_atom()
    {
        let new: Vec<String> = v
            .split_whitespace()
            .map(|f| if f.contains(':') && !f.starts_with(PREFIX) { format!("{PREFIX}{f}") } else { f.to_string() })
            .collect();
        let new = new.join(" ");
        if new != v {
            edits.push(Edit { range: value.span(), text: sexpr::quote(&new) });
        }
    }
    if !sym.children("property").any(|p| p.arg(0).is_some_and(|n| n.starts_with("Mpn"))) {
        let like = property(sym, "Description").or_else(|| property(sym, "Datasheet")).ok_or("no Description")?;
        let span = like.span();
        let (key, value) = (&like.items()[1], &like.items()[2]);
        let rel = |r: std::ops::Range<usize>| r.start - span.start..r.end - span.start;
        let mut field = apply_edits(
            &text[span.clone()],
            &[
                Edit { range: rel(key.span()), text: "\"Mpn1\"".into() },
                Edit { range: rel(value.span()), text: "\"\"".into() },
            ],
        );
        if !field.contains("(hide yes)") {
            return Err("the Description field is not hidden".into());
        }
        field.insert_str(0, &format!("\n{}", indent_of(text, span.start)));
        edits.push(Edit { range: span.end..span.end, text: field });
    }
    edits.sort_by_key(|e| e.range.start);
    Ok(apply_edits(text, &edits))
}

/// The copy of the stock symbol `text` from `lib_dir`.
pub fn import_text(text: &str, lib_dir: &Path, footprints: &Path) -> Result<String, String> {
    let text = library_fields(&standalone(text, lib_dir)?, footprints)?;
    Ok(crate::layout_fields::add_to_text(&text)?.0)
}

/// What importing `ids` (`Lib:Name`) from `stock` (KiCad's symbol folder) into `symbols` would write; `footprints`
/// is the checkout's footprint folder the `Footprint` fields point into.
pub fn plan_import(stock: &Path, symbols: &Path, footprints: &Path, ids: &[String]) -> Vec<Import> {
    ids.iter()
        .map(|id| {
            let (from, to, outcome) = match split_id(id) {
                Err(e) => (PathBuf::new(), PathBuf::new(), ImportOutcome::Error(e)),
                Ok((lib, name)) => {
                    let from = sym_path(stock, lib, name);
                    let to = sym_path(symbols, &format!("{PREFIX}{lib}"), name);
                    let outcome = if to.exists() {
                        ImportOutcome::Exists
                    } else {
                        match read(&from)
                            .and_then(|t| import_text(&t, &stock.join(format!("{lib}.kicad_symdir")), footprints))
                        {
                            Ok(text) => ImportOutcome::New { text },
                            Err(e) => ImportOutcome::Error(e),
                        }
                    };
                    (from, to, outcome)
                }
            };
            Import { id: id.clone(), from, to, outcome }
        })
        .collect()
}

/// `fresh` (an import of the stock symbol) with the fields and flags of `ours`: our fields in our order, then the
/// fields only stock has.
pub fn overlay(fresh: &str, ours: &str) -> Result<String, String> {
    let (froot, oroot) = (parse(fresh)?, parse(ours)?);
    let (fsym, osym) = (only_symbol(&froot)?, only_symbol(&oroot)?);
    let block = property_block(fsym)?;
    let indent = indent_of(fresh, block.start);
    let mut props = ours[property_block(osym)?].to_string();
    for p in fsym.children("property") {
        if p.arg(0).is_some_and(|n| property(osym, n).is_none()) {
            props.push_str(&format!("\n{indent}{}", &fresh[p.span()]));
        }
    }
    let mut edits = vec![Edit { range: block, text: props }];
    for flag in FLAGS {
        if let (Some(f), Some(o)) = (fsym.child(flag), osym.child(flag)) {
            edits.push(Edit { range: f.span(), text: ours[o.span()].to_string() });
        }
    }
    edits.sort_by_key(|e| e.range.start);
    Ok(apply_edits(fresh, &edits))
}

/// Every symbol under `symbols/VHRD_<Lib>.kicad_symdir` that has a stock symbol `<Lib>:<Name>` in `stock`,
/// compared with a fresh import of it under its own fields and flags. Symbols of our own whose name matches a
/// stock one are listed too.
pub fn compare(stock: &Path, symbols: &Path, footprints: &Path) -> Vec<Comparison> {
    let mut out = Vec::new();
    let mut libs: Vec<PathBuf> =
        read_dir(symbols).filter(|p| p.extension().is_some_and(|e| e == "kicad_symdir")).collect();
    libs.sort();
    for lib_dir in libs {
        let lib = lib_dir.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let Some(stock_lib) = lib.strip_prefix(PREFIX) else { continue };
        let stock_dir = stock.join(format!("{stock_lib}.kicad_symdir"));
        let mut syms: Vec<PathBuf> =
            read_dir(&lib_dir).filter(|p| p.extension().is_some_and(|e| e == "kicad_sym")).collect();
        syms.sort();
        for ours in syms {
            let file = ours.file_name().unwrap_or_default().to_string_lossy().to_string();
            let stock_path = stock_dir.join(&file);
            if !stock_path.is_file() {
                continue;
            }
            let outcome = match (read(&ours), read(&stock_path)) {
                (Err(e), _) | (_, Err(e)) => Compared::Error(e),
                (Ok(o), Ok(s)) => match import_text(&s, &stock_dir, footprints).and_then(|f| overlay(&f, &o)) {
                    Err(e) => Compared::Error(e),
                    Ok(fresh) if fresh == o => Compared::Same,
                    Ok(fresh) => {
                        let label = format!("{lib}.kicad_symdir/{file}");
                        Compared::Differs(unified_diff(&label, &o, &line_edits(&o, &fresh)))
                    }
                },
            };
            let name = file.trim_end_matches(".kicad_sym").replace("{slash}", "/");
            out.push(Comparison { id: format!("{lib}:{name}"), ours, stock: stock_path, outcome });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(name: &str, value: &str, hide: bool) -> String {
        let hide = if hide { "\n\t\t\t(hide yes)" } else { "" };
        format!(
            "\t\t(property \"{name}\" \"{value}\"\n\t\t\t(at 0 0 0){hide}\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n"
        )
    }

    fn file(body: &str) -> String {
        format!(
            "(kicad_symbol_lib\n\t(version 20251024)\n\t(generator \"kicad_symbol_editor\")\n\t(generator_version \"10.0\")\n{body})\n"
        )
    }

    fn parent() -> String {
        file(&format!(
            "\t(symbol \"Q_Dual\"\n\t\t(pin_names\n\t\t\t(offset 0)\n\t\t\t(hide yes)\n\t\t)\n\t\t(exclude_from_sim no)\n\t\t(in_bom yes)\n\t\t(on_board yes)\n{}{}{}{}{}\t\t(symbol \"Q_Dual_0_1\"\n\t\t\t(circle\n\t\t\t\t(center 0 0)\n\t\t\t\t(radius 2)\n\t\t\t)\n\t\t)\n\t\t(symbol \"Q_Dual_1_1\"\n\t\t\t(pin input line\n\t\t\t\t(at -5 0 0)\n\t\t\t\t(length 2)\n\t\t\t\t(name \"B\")\n\t\t\t\t(number \"2\")\n\t\t\t)\n\t\t)\n\t\t(embedded_fonts no)\n\t)\n",
            prop("Reference", "Q", false),
            prop("Value", "Q_Dual", false),
            prop("Footprint", "", true),
            prop("Datasheet", "~", true),
            prop("Description", "Dual", true),
        ))
    }

    fn child() -> String {
        file(&format!(
            "\t(symbol \"BC847BS\"\n\t\t(extends \"Q_Dual\")\n\t\t(in_bom no)\n{}{}{}{}{}{}\t\t(embedded_fonts no)\n\t)\n",
            prop("Reference", "Q", false),
            prop("Value", "BC847BS", false),
            prop("Footprint", "Package_TO_SOT_SMD:SOT-363_SC-70-6", true),
            prop("Datasheet", "https://example.com/bc847bs.pdf", true),
            prop("Description", "Dual NPN", true),
            prop("ki_fp_filters", "SOT?363* Connector*:*_1x??_*", true),
        ))
    }

    #[test]
    fn a_derived_symbol_becomes_standalone() {
        let dir = crate::testdir::TestDir::new("stock_sym");
        let lib = dir.path().join("Transistor_BJT.kicad_symdir");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("Q_Dual.kicad_sym"), parent()).unwrap();
        let s = standalone(&child(), &lib).unwrap();
        let root = sexpr::parse(&s).unwrap();
        let sym = only_symbol(&root).unwrap();
        assert_eq!(sym.arg(0), Some("BC847BS"));
        assert!(sym.child("extends").is_none());
        assert_eq!(sym.value("in_bom"), Some("no"), "the child's flag wins");
        assert_eq!(sym.value("on_board"), Some("yes"), "the parent's flag stays");
        let units: Vec<_> = sym.children("symbol").filter_map(|u| u.arg(0)).collect();
        assert_eq!(units, ["BC847BS_0_1", "BC847BS_1_1"]);
        assert_eq!(property(sym, "Value").and_then(|p| p.arg(1)), Some("BC847BS"));
        assert_eq!(property(sym, "Description").and_then(|p| p.arg(1)), Some("Dual NPN"));
        assert!(s.contains("(name \"B\")") && s.contains("(radius 2)"));
        assert_eq!(standalone(&s, &lib).unwrap(), s, "a standalone symbol stays as it is");
        std::fs::remove_file(lib.join("Q_Dual.kicad_sym")).unwrap();
        assert!(standalone(&child(), &lib).is_err(), "a missing parent is an error");
    }

    #[test]
    fn library_fields_point_at_our_footprints() {
        let dir = crate::testdir::TestDir::new("stock_sym_fields");
        let fps = dir.path().join("footprints");
        let lib = dir.path().join("Transistor_BJT.kicad_symdir");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("Q_Dual.kicad_sym"), parent()).unwrap();
        let s = import_text(&child(), &lib, &fps).unwrap();
        assert!(s.contains("\"Package_TO_SOT_SMD:SOT-363_SC-70-6\""), "no copy of the footprint: as stock");
        assert!(s.contains("\"SOT?363* VHRD_Connector*:*_1x??_*\""), "{s}");
        let mpn = "\t\t(property \"Description\" \"Dual NPN\"\n\t\t\t(at 0 0 0)\n\t\t\t(hide yes)\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n\t\t(property \"Mpn1\" \"\"\n\t\t\t(at 0 0 0)\n\t\t\t(hide yes)";
        assert!(s.contains(mpn), "{s}");
        assert_eq!(library_fields(&s, &fps).unwrap(), s, "fields are added once");

        std::fs::create_dir_all(fps.join("VHRD_Package_TO_SOT_SMD.pretty")).unwrap();
        std::fs::write(fps.join("VHRD_Package_TO_SOT_SMD.pretty/SOT-363_SC-70-6.kicad_mod"), "(footprint)").unwrap();
        let s = import_text(&child(), &lib, &fps).unwrap();
        assert!(s.contains("\"VHRD_Package_TO_SOT_SMD:SOT-363_SC-70-6\""), "{s}");
    }

    #[test]
    fn import_and_compare_keep_our_fields() {
        let dir = crate::testdir::TestDir::new("stock_sym_cmp");
        let stock = dir.path().join("stock");
        let (syms, fps) = (dir.path().join("lib/symbols"), dir.path().join("lib/footprints"));
        let lib = stock.join("Transistor_BJT.kicad_symdir");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("Q_Dual.kicad_sym"), parent()).unwrap();
        std::fs::write(lib.join("BC847BS.kicad_sym"), child()).unwrap();

        let ids = vec!["Transistor_BJT:BC847BS".to_string(), "Transistor_BJT:Nope".to_string()];
        let plan = plan_import(&stock, &syms, &fps, &ids);
        assert!(matches!(plan[0].outcome, ImportOutcome::New { .. }));
        assert!(matches!(plan[1].outcome, ImportOutcome::Error(_)));
        assert_eq!(super::super::apply_import(&plan[..1]).unwrap(), 1);
        let ours = syms.join("VHRD_Transistor_BJT.kicad_symdir/BC847BS.kicad_sym");
        assert_eq!(plan_import(&stock, &syms, &fps, &ids[..1])[0].outcome, ImportOutcome::Exists);

        let c = compare(&stock, &syms, &fps);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].id.as_str(), &c[0].outcome), ("VHRD_Transistor_BJT:BC847BS", &Compared::Same));

        // Our fields and flags are ours: filling in an Mpn and leaving the BOM changes nothing in the comparison.
        let text = std::fs::read_to_string(&ours).unwrap();
        std::fs::write(
            &ours,
            text.replace("\"Mpn1\" \"\"", "\"Mpn1\" \"BC847BS,115\"").replace("(on_board yes)", "(on_board no)"),
        )
        .unwrap();
        assert_eq!(compare(&stock, &syms, &fps)[0].outcome, Compared::Same);

        // Upstream moves a pin and adds a field: the diff shows just that.
        std::fs::write(lib.join("Q_Dual.kicad_sym"), parent().replace("(at -5 0 0)", "(at -5.08 0 0)")).unwrap();
        let filters = prop("ki_fp_filters", "SOT?363* Connector*:*_1x??_*", true);
        let more = child().replacen(&filters, &format!("{filters}{}", prop("Sim.Device", "NPN", true)), 1);
        std::fs::write(lib.join("BC847BS.kicad_sym"), more).unwrap();
        let Compared::Differs(d) = &compare(&stock, &syms, &fps)[0].outcome else { panic!() };
        assert!(d.contains("-\t\t\t\t(at -5 0 0)\n+\t\t\t\t(at -5.08 0 0)"), "{d}");
        assert!(d.contains("+\t\t(property \"Sim.Device\" \"NPN\""), "{d}");
        assert!(!d.contains("Mpn1") && !d.contains("on_board"), "{d}");
    }
}
