//! The symbol side of layout priority ([`crate::lint::layout`]): every symbol carries the fields `Layout_Critical`
//! and `Layout_Important`, empty and hidden, so a schematic can set a note in one and Update PCB from Schematic
//! carries it to the footprint's property of the same name. KiCad copies a library symbol's fields, empty ones too,
//! into a symbol placed after the library has them, and "Update Symbols from Library" adds them to symbols placed
//! before; [`add_to_text`] does the same for a library file (`.kicad_sym`) or a schematic (`.kicad_sch`: each placed
//! symbol and its copy in the schematic's `lib_symbols`, so ERC finds the copy still matching the library).
//!
//! A new field is a copy of a hidden field the symbol has (`Description`, else `Datasheet`, else `Footprint`, else
//! any hidden one) under the new name with an empty value, so it is written the way that file writes its fields.
//! Power symbols and power flags (a `(power)` symbol, a placed symbol whose designator starts with `#`) get none:
//! they are no parts. Only these insertions change the text.

use std::path::{Path, PathBuf};

use crate::lint::layout::{CRITICAL, IMPORTANT};
use crate::migrate::{Edit, apply_edits};
use crate::sexpr::{self, Node};

/// The fields, in the order they are added.
pub const FIELDS: [&str; 2] = [CRITICAL, IMPORTANT];

/// The fields that serve as the pattern of a new one, best first.
const LIKE: [&str; 3] = ["Description", "Datasheet", "Footprint"];

fn property<'a>(sym: &'a Node, name: &str) -> Option<&'a Node> {
    sym.children("property").find(|p| p.arg(0) == Some(name))
}

fn is_hidden(p: &Node) -> bool {
    let yes = |n: &Node| n.child("hide").is_some_and(|h| h.arg(0).is_none_or(|v| v == "yes"));
    // KiCad 7 wrote a bare `hide` in the effects.
    let bare = |e: &Node| e.items().iter().skip(1).any(|n| n.as_atom() == Some("hide"));
    yes(p) || p.child("effects").is_some_and(|e| yes(e) || bare(e))
}

/// The whitespace the line of `at` starts with.
fn indent_of(text: &str, at: usize) -> &str {
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    &text[line..at]
}

/// The text of a new, empty field `name` modelled on the hidden field `like` of `text`.
fn field_like(text: &str, like: &Node, name: &str) -> String {
    let span = like.span();
    let rel = |r: std::ops::Range<usize>| r.start - span.start..r.end - span.start;
    let mut edits = vec![Edit { range: rel(like.items()[1].span()), text: sexpr::quote(name) }];
    match like.items().get(2).filter(|v| v.as_atom().is_some()) {
        Some(v) => edits.push(Edit { range: rel(v.span()), text: "\"\"".into() }),
        None => {
            let k = like.items()[1].span();
            edits.push(Edit { range: rel(k.end..k.end), text: " \"\"".into() });
        }
    }
    apply_edits(&text[span], &edits)
}

/// The edit that adds the missing [`FIELDS`] after the last property of `sym`; None when it has them all or is
/// a power symbol. An error when it has no hidden field to model them on.
fn symbol_edit(text: &str, sym: &Node) -> Result<Option<Edit>, String> {
    let missing: Vec<&str> = FIELDS.into_iter().filter(|f| property(sym, f).is_none()).collect();
    if missing.is_empty() {
        return Ok(None);
    }
    let props: Vec<&Node> = sym.children("property").collect();
    let last = props.last().ok_or("no properties")?;
    let like = LIKE
        .iter()
        .filter_map(|n| property(sym, n))
        .chain(props.iter().copied())
        .find(|p| is_hidden(p) && p.items().len() > 2)
        .ok_or("no hidden field to model the new ones on")?;
    let indent = indent_of(text, last.span().start);
    let indent = if indent.trim().is_empty() { indent } else { "" };
    let sep = if indent.is_empty() { " ".to_string() } else { format!("\n{indent}") };
    let added: String = missing.iter().map(|f| format!("{sep}{}", field_like(text, like, f))).collect();
    Ok(Some(Edit { range: last.span().end..last.span().end, text: added }))
}

/// A library symbol that is a power symbol (`(power)` or `(power local)`).
fn is_power(sym: &Node) -> bool {
    sym.child("power").is_some()
}

/// What [`add_to_text`] did to one file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Added {
    /// Symbols (library symbols, placed symbols, cached library symbols) that got the fields.
    pub symbols: usize,
    /// Symbols with no hidden field to model the new ones on, by name or designator: left alone.
    pub skipped: Vec<String>,
}

/// The text of a `.kicad_sym` or `.kicad_sch` with [`FIELDS`] added where a symbol lacks them.
pub fn add_to_text(text: &str) -> Result<(String, Added), String> {
    let root = sexpr::parse(text).map_err(|e| e.to_string())?;
    let mut syms: Vec<(String, &Node)> = Vec::new();
    match root.head() {
        Some("kicad_symbol_lib") => {
            syms.extend(root.children("symbol").filter(|s| !is_power(s)).map(|s| (name(s), s)));
        }
        Some("kicad_sch") => {
            if let Some(cache) = root.child("lib_symbols") {
                syms.extend(cache.children("symbol").filter(|s| !is_power(s)).map(|s| (name(s), s)));
            }
            for s in root.children("symbol") {
                let r = property(s, "Reference").and_then(|p| p.arg(1)).unwrap_or("");
                if !r.starts_with('#') {
                    syms.push((r.to_string(), s));
                }
            }
        }
        _ => return Err("not a symbol library or a schematic".into()),
    }
    let mut added = Added::default();
    let mut edits = Vec::new();
    for (name, s) in syms {
        match symbol_edit(text, s) {
            Ok(Some(e)) => {
                added.symbols += 1;
                edits.push(e);
            }
            Ok(None) => {}
            Err(why) => added.skipped.push(format!("{name}: {why}")),
        }
    }
    edits.sort_by_key(|e| e.range.start);
    Ok((apply_edits(text, &edits), added))
}

fn name(s: &Node) -> String {
    s.arg(0).unwrap_or("").to_string()
}

/// One file and what adding the fields does to it.
#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub old: String,
    pub new: Option<String>,
    pub added: Added,
    pub error: Option<String>,
}

/// Every `.kicad_sym` and `.kicad_sch` under `paths` (files, or folders searched), sorted, with what adding the
/// fields does to it.
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
            let mut item = Item { path, old: String::new(), new: None, added: Added::default(), error: None };
            match std::fs::read_to_string(&item.path) {
                Err(e) => item.error = Some(e.to_string()),
                Ok(old) => {
                    match add_to_text(&old) {
                        Ok((new, added)) => {
                            item.new = (new != old).then_some(new);
                            item.added = added;
                        }
                        Err(e) => item.error = Some(e),
                    }
                    item.old = old;
                }
            }
            item
        })
        .collect()
}

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_file() {
        if p.extension().is_some_and(|e| e == "kicad_sym" || e == "kicad_sch") {
            out.push(p.to_path_buf());
        }
        return;
    }
    let Ok(rd) = std::fs::read_dir(p) else { return };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        collect(&e.path(), out);
    }
}

/// Writes every changed file; refuses when a file changed since [`plan`] read it.
pub fn apply(items: &[Item]) -> Result<usize, String> {
    let changes: Vec<(&Item, &String)> = items.iter().filter_map(|i| Some((i, i.new.as_ref()?))).collect();
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

    const DESCR: &str = "\t\t(property \"Description\" \"Resistor\"\n\t\t\t(at 0 0 0)\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t\t(hide yes)\n\t\t\t)\n\t\t)\n";

    fn lib(body: &str) -> String {
        format!(
            "(kicad_symbol_lib\n\t(version 20241209)\n\t(symbol \"R\"\n\t\t(property \"Reference\" \"R\"\n\t\t\t(at 0 0 0)\n\t\t)\n{body}\t\t(symbol \"R_0_1\")\n\t)\n)\n"
        )
    }

    #[test]
    fn a_library_symbol_gets_both_fields_like_its_description() {
        let t = lib(DESCR);
        let (new, added) = add_to_text(&t).unwrap();
        assert_eq!(added.symbols, 1);
        let want = DESCR.replace("Description\" \"Resistor", "Layout_Critical\" \"")
            + &DESCR.replace("Description\" \"Resistor", "Layout_Important\" \"");
        assert_eq!(new, lib(&(DESCR.to_string() + &want)));
        let (again, added) = add_to_text(&new).unwrap();
        assert_eq!((again, added.symbols), (new, 0));
    }

    #[test]
    fn kicad_10_fields_keep_their_form_and_power_symbols_get_none() {
        let k10 = "\t\t(property \"Datasheet\" \"~\"\n\t\t\t(at 0 0 0)\n\t\t\t(show_name no)\n\t\t\t(hide yes)\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n";
        let (new, _) = add_to_text(&lib(k10)).unwrap();
        assert!(
            new.contains(
                "(property \"Layout_Important\" \"\"\n\t\t\t(at 0 0 0)\n\t\t\t(show_name no)\n\t\t\t(hide yes)"
            )
        );
        let power = lib(DESCR).replace("\t(symbol \"R\"\n", "\t(symbol \"R\"\n\t\t(power)\n");
        assert_eq!(add_to_text(&power).unwrap().1.symbols, 0);
        // KiCad 7's bare `hide`, all on one line.
        let k7 = "(kicad_symbol_lib (version 20220914)\n  (symbol \"L\" (in_bom yes)\n    (property \"Reference\" \"L\" (at 0 4.445 0)\n      (effects (font (size 1.27 1.27)))\n    )\n    (property \"Datasheet\" \"x.pdf\" (at 0 0 0)\n      (effects (font (size 1.27 1.27)) hide)\n    )\n  )\n)\n";
        let (new, _) = add_to_text(k7).unwrap();
        assert!(
            new.contains("hide)\n    )\n    (property \"Layout_Critical\" \"\" (at 0 0 0)\n      (effects (font (size 1.27 1.27)) hide)\n    )\n    (property \"Layout_Important\""),
            "{new}"
        );
        // No hidden field to copy: left alone and named.
        let (same, added) = add_to_text(&lib("")).unwrap();
        assert_eq!(same, lib(""));
        assert_eq!(added.skipped, ["R: no hidden field to model the new ones on"]);
    }

    #[test]
    fn a_schematic_gets_them_on_placed_symbols_and_their_library_copies() {
        let placed = |r: &str| {
            format!(
                "\t(symbol\n\t\t(lib_id \"VHRD_Device:R\")\n\t\t(at 10 20 0)\n\t\t(property \"Reference\" \"{r}\"\n\t\t\t(at 12 20 0)\n\t\t)\n\t\t(property \"Description\" \"\"\n\t\t\t(at 10 20 0)\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t\t(hide yes)\n\t\t\t)\n\t\t)\n\t)\n"
            )
        };
        let sch = format!(
            "(kicad_sch\n\t(version 20250114)\n\t(lib_symbols\n\t\t(symbol \"VHRD_Device:R\"\n\t\t\t(property \"Reference\" \"R\" (at 0 0 0))\n\t\t\t(property \"Description\" \"Resistor\" (at 0 0 0) (effects (hide yes)))\n\t\t)\n\t)\n{}{})\n",
            placed("R1"),
            placed("#PWR01")
        );
        let (new, added) = add_to_text(&sch).unwrap();
        assert_eq!(added.symbols, 2, "{new}");
        assert_eq!(new.matches("\"Layout_Critical\" \"\"").count(), 2, "{new}");
        assert!(new.contains("\t\t(property \"Layout_Critical\" \"\"\n\t\t\t(at 10 20 0)\n"), "{new}");
        assert!(
            new.contains(
                "(effects (hide yes)))\n\t\t\t(property \"Layout_Critical\" \"\" (at 0 0 0) (effects (hide yes)))\n"
            ),
            "{new}"
        );
        // The power flag keeps its fields as they are.
        assert!(new.ends_with(&(placed("#PWR01") + ")\n")), "{new}");
    }
}
