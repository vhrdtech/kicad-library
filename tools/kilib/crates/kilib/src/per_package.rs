//! Per-package symbols: a generic symbol (`VHRD_Device:R`, which leaves `Footprint` empty) gets one symbol per
//! package it is placed with (`VHRD_Device:R_0402`), so whatever is keyed by the library symbol (a footprint
//! review, a BOM line) covers one footprint. The rename map's `[per_package]` names them; [`plan`] makes each one
//! the map lacks in the checkout, and `kilib migrate` moves placed generic symbols onto them.
//!
//! A per-package symbol is derived from its generic (`extends`), in the generic's library: the generic's fields,
//! with `Footprint` and `ki_fp_filters` set to the footprint. Pins, units, graphics and flags (`in_bom`, ...) stay
//! the generic's; KiCad does not keep flags on a derived symbol.
//! [`survey`] lists the generic symbols boards place, by footprint, with the per-package symbol of the map.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::map::Map;
use crate::migrate::{Edit, Severity, apply_edits};
use crate::sexpr::{self, Node};
use crate::stock::symbol::sym_path;

/// One per-package symbol of the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// `Lib:Name` of the per-package symbol.
    pub id: String,
    pub generic: String,
    pub footprint: String,
    pub path: PathBuf,
    /// The text to write; `None` when the checkout has it already, as it should be.
    pub new: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub items: Vec<Item>,
    pub notes: Vec<(Severity, String)>,
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        self.notes.iter().any(|(s, _)| *s == Severity::Error)
    }

    pub fn to_write(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.new.is_some())
    }
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn only_symbol(root: &Node) -> Result<&Node, String> {
    let mut syms = root.children("symbol");
    match (syms.next(), syms.next()) {
        (Some(s), None) => Ok(s),
        _ => Err("not one symbol in the file".into()),
    }
}

fn property<'a>(sym: &'a Node, name: &str) -> Option<&'a Node> {
    sym.children("property").find(|p| p.arg(0) == Some(name))
}

/// A symbol file's one symbol, parsed.
fn symbol_of(path: &Path) -> Result<(String, Node), String> {
    let text = read(path)?;
    let root = sexpr::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    only_symbol(&root).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((text, root))
}

/// Every per-package symbol of `map` in the checkout `library`: made where it is missing, checked where it is
/// there (derived from its generic, `Footprint` and `ki_fp_filters` at its footprint). The generic must leave
/// `Footprint` empty, and the footprint must be in the checkout's `footprints/`.
pub fn plan(library: &Path, map: &Map) -> Plan {
    let symbols = library.join("symbols");
    let footprints = library.join("footprints");
    let mut plan = Plan::default();
    for (generic, by_fp) in &map.per_package {
        let Some((lib, gname)) = generic.split_once(':') else { continue };
        let gpath = sym_path(&symbols, lib, gname);
        let parent = match symbol_of(&gpath) {
            Ok(p) => p,
            Err(e) => {
                plan.notes.push((Severity::Error, format!("generic {generic}: {e}")));
                continue;
            }
        };
        let (gtext, groot) = &parent;
        let gsym = only_symbol(groot).expect("checked");
        if let Some(fp) = property(gsym, "Footprint").and_then(|p| p.arg(1)).filter(|f| !f.is_empty()) {
            plan.notes.push((Severity::Error, format!("generic {generic} sets Footprint {fp}: not a generic symbol")));
            continue;
        }
        for (fp, id) in by_fp {
            let Some((fplib, fpname)) = fp.split_once(':') else { continue };
            let name = id.split_once(':').map_or(id.as_str(), |(_, n)| n);
            let path = sym_path(&symbols, lib, name);
            let fp_file = footprints.join(format!("{fplib}.pretty")).join(format!("{fpname}.kicad_mod"));
            if !fp_file.is_file() {
                plan.notes.push((Severity::Error, format!("{id}: footprint {fp} is not in footprints/")));
                continue;
            }
            let item =
                |new| Item { id: id.clone(), generic: generic.clone(), footprint: fp.clone(), path: path.clone(), new };
            if path.exists() {
                match check_existing(&path, gname, fp) {
                    Ok(()) => plan.items.push(item(None)),
                    Err(e) => plan.notes.push((Severity::Error, format!("{id}: {e}"))),
                }
            } else {
                match derived(gtext, gsym, name, fp) {
                    Ok(text) => plan.items.push(item(Some(text))),
                    Err(e) => plan.notes.push((Severity::Error, format!("{id}: {e}"))),
                }
            }
        }
    }
    plan
}

/// A per-package symbol already in the checkout: derived from `generic`, its footprint and filter at `fp`.
fn check_existing(path: &Path, generic: &str, fp: &str) -> Result<(), String> {
    let (_, root) = symbol_of(path)?;
    let sym = only_symbol(&root)?;
    let extends = sym.child("extends").and_then(|e| e.arg(0));
    if extends != Some(generic) {
        return Err(format!("{} does not extend {generic}", path.display()));
    }
    for field in ["Footprint", "ki_fp_filters"] {
        let value = property(sym, field).and_then(|p| p.arg(1)).unwrap_or_default();
        if value != fp {
            return Err(format!("{field} is {value:?}, not {fp}"));
        }
    }
    Ok(())
}

/// The file of a symbol `name` derived from the generic symbol `gsym` (its file's text `gtext`): the generic's
/// header and properties, `Footprint` and `ki_fp_filters` set to `fp`.
pub fn derived(gtext: &str, gsym: &Node, name: &str, fp: &str) -> Result<String, String> {
    let gname = gsym.arg(0).ok_or("a symbol without a name")?;
    // The generic's header: KiCad reads the fields by the file's version (`hide` in `effects` before 10.0).
    let line = gtext[..gsym.span().start].rfind('\n').map_or(0, |i| i + 1);
    let mut out = gtext[..line].to_string();
    out.push_str(&format!("\t(symbol {}\n\t\t(extends {})\n", sexpr::quote(name), sexpr::quote(gname)));
    let mut has_filter = false;
    for prop in gsym.children("property") {
        let field = prop.arg(0).unwrap_or_default();
        let span = prop.span();
        let text = match (field, prop.items().get(2)) {
            ("Footprint" | "ki_fp_filters", Some(value)) => {
                has_filter |= field == "ki_fp_filters";
                let v = value.span();
                let edit = Edit { range: v.start - span.start..v.end - span.start, text: sexpr::quote(fp) };
                apply_edits(&gtext[span.clone()], &[edit])
            }
            _ => gtext[span].to_string(),
        };
        out.push_str(&format!("\t\t{text}\n"));
    }
    if property(gsym, "Footprint").is_none() || !has_filter {
        return Err(format!("generic {gname} has no Footprint or ki_fp_filters field to set"));
    }
    out.push_str("\t\t(embedded_fonts no)\n\t)\n)\n");
    Ok(out)
}

/// Writes the planned symbols. Refuses when the plan has errors.
pub fn apply(plan: &Plan) -> Result<(), String> {
    if plan.has_errors() {
        return Err("the plan has errors; nothing written".into());
    }
    for item in plan.to_write() {
        let text = item.new.as_deref().unwrap_or_default();
        std::fs::write(&item.path, text).map_err(|e| format!("{}: {e}", item.path.display()))?;
    }
    Ok(())
}

/// A generic symbol placed with one footprint, on the boards surveyed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub generic: String,
    /// The placed `Footprint` field; empty when none.
    pub footprint: String,
    /// The `Reference` fields, sorted, each once.
    pub refs: Vec<String>,
    /// The map's per-package symbol for it.
    pub to: Option<String>,
}

/// The generic symbols (a symbol of the checkout that leaves `Footprint` empty) placed in the schematics under
/// `boards`, one entry per footprint they are placed with, sorted by symbol and footprint.
pub fn survey(library: &Path, map: &Map, boards: &[PathBuf]) -> Result<Vec<Placed>, String> {
    let symbols = library.join("symbols");
    let mut files = Vec::new();
    for b in boards {
        crate::check::collect(b, &mut files, &|n: &str| n.ends_with(".kicad_sch"));
    }
    files.sort();
    files.dedup();
    let mut generic: BTreeMap<String, bool> = BTreeMap::new();
    let mut found: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for file in &files {
        let text = read(file)?;
        let root = sexpr::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        for sym in root.children("symbol") {
            let Some(id) = sym.child("lib_id").and_then(|l| l.arg(0)) else { continue };
            let is_generic = *generic.entry(id.to_string()).or_insert_with(|| {
                let Some((lib, name)) = id.split_once(':') else { return false };
                symbol_of(&sym_path(&symbols, lib, name)).is_ok_and(|(_, root)| {
                    let sym = only_symbol(&root).expect("checked");
                    sym.child("power").is_none()
                        && property(sym, "Footprint").and_then(|p| p.arg(1)).is_none_or(str::is_empty)
                })
            });
            if !is_generic {
                continue;
            }
            let field = |f: &str| property(sym, f).and_then(|p| p.arg(1)).unwrap_or_default().to_string();
            found.entry((id.to_string(), field("Footprint"))).or_default().push(field("Reference"));
        }
    }
    Ok(found
        .into_iter()
        .map(|((generic, footprint), mut refs)| {
            refs.sort_by_key(|a| natural(a));
            refs.dedup();
            let to = map.per_package(&generic, &footprint).map(str::to_string);
            Placed { generic, footprint, refs, to }
        })
        .collect())
}

/// `R12` after `R9`: the text with each run of digits as a number.
fn natural(s: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let text_end = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
        let digits = rest[text_end..].find(|c: char| !c.is_ascii_digit()).map_or(rest.len(), |i| text_end + i);
        out.push((rest[..text_end].to_string(), rest[text_end..digits].parse().unwrap_or(0)));
        rest = &rest[digits..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: &str = "(kicad_symbol_lib\n\t(version 20241209)\n\t(symbol \"R\"\n\t\t(pin_numbers\n\t\t\t(hide yes)\n\t\t)\n\t\t(exclude_from_sim no)\n\t\t(in_bom yes)\n\t\t(on_board yes)\n\t\t(property \"Reference\" \"R\"\n\t\t\t(at 0 0 0)\n\t\t)\n\t\t(property \"Footprint\" \"\"\n\t\t\t(at 0 0 0)\n\t\t)\n\t\t(property \"ki_fp_filters\" \"R_*\"\n\t\t\t(at 0 0 0)\n\t\t)\n\t\t(symbol \"R_0_1\")\n\t)\n)\n";

    #[test]
    fn derived_sets_footprint_and_filter() {
        let root = sexpr::parse(R).unwrap();
        let sym = only_symbol(&root).unwrap();
        let text = derived(R, sym, "R_0402", "VHRD_Resistor_SMD:R_0402_1005Metric").unwrap();
        assert!(
            text.contains("\t(symbol \"R_0402\"\n\t\t(extends \"R\")\n\t\t(property \"Reference\" \"R\"\n"),
            "{text}"
        );
        assert!(
            text.contains("(property \"Footprint\" \"VHRD_Resistor_SMD:R_0402_1005Metric\"\n\t\t\t(at 0 0 0)"),
            "{text}"
        );
        assert!(text.contains("(property \"ki_fp_filters\" \"VHRD_Resistor_SMD:R_0402_1005Metric\""), "{text}");
        assert!(
            !text.contains("R_0_1") && !text.contains("pin_numbers") && !text.contains("in_bom"),
            "units, pins and flags stay the generic's: {text}"
        );
        let root = sexpr::parse(&text).unwrap();
        assert_eq!(only_symbol(&root).unwrap().child("extends").and_then(|e| e.arg(0)), Some("R"));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["R10", "R9", "R101", "C2"];
        v.sort_by_key(|a| natural(a));
        assert_eq!(v, ["C2", "R9", "R10", "R101"]);
    }
}
