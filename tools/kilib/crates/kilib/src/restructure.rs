//! `kilib restructure`: moves a kicad-library checkout's symbols and footprints into the new libraries of a
//! [`Map`], in KiCad 10's layout.
//!
//! - `symbols/<old>.kicad_sym` becomes `<New>.kicad_symdir/<Name>.kicad_sym`, one file per symbol, as KiCad 10's
//!   own libraries are laid out. Each file is the old file's header, the symbol's text as it was, and the old
//!   closing lines. A derived symbol (`extends`) goes to its parent's library, in a file of its own.
//! - `footprints/<old>.pretty/<Name>.kicad_mod` moves to `<New>.pretty/` as it is.
//! - Libraries under `${VHRD_VENDOR_LIBRARY_DIR}` go to the vendor folder, the others into the checkout.
//! - Every moved file goes through [`migrate`]'s rewrite on the way: footprint model paths per `[model]`,
//!   symbols' `Footprint` fields and filters per `[footprint]`.
//!
//! The old files are removed once every new one is written. A second run finds nothing to move.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::map::{ItemKind, Map, Target};
use crate::migrate::{self, Severity};
use crate::sexpr::{self, Node};

/// Why an item goes where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Its own line in `[symbol]` / `[footprint]`.
    Item,
    /// Its old library's default in `[symbol_lib]` / `[footprint_lib]`.
    Library,
    /// A derived symbol, with its parent.
    Parent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub kind: ItemKind,
    /// `old_lib:name`.
    pub id: String,
    pub new_lib: String,
    pub from: PathBuf,
    /// `None` for a dropped item.
    pub to: Option<PathBuf>,
    pub why: Why,
    /// The text written at `to`.
    pub text: String,
    /// Strings the migrate rewrite changed in it.
    pub rewritten: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub moves: Vec<Move>,
    /// Old files removed once the new ones are written.
    pub remove: Vec<PathBuf>,
    /// Old folders removed when they are empty after that.
    pub remove_dirs: Vec<PathBuf>,
    pub notes: Vec<(Severity, String)>,
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        self.notes.iter().any(|(s, _)| *s == Severity::Error)
    }
}

/// Where a library of the map lives: its `[libraries]` URI with `${VHRD_LIBRARY_DIR}` as `library` and
/// `${VHRD_VENDOR_LIBRARY_DIR}` as `vendor`.
pub fn library_path(map: &Map, kind: ItemKind, lib: &str, library: &Path, vendor: &Path) -> Result<PathBuf, String> {
    let uri =
        map.libraries(kind).get(lib).ok_or_else(|| format!("{} library {lib} is not in [libraries]", kind.label()))?;
    for (var, root) in [("${VHRD_LIBRARY_DIR}/", library), ("${VHRD_VENDOR_LIBRARY_DIR}/", vendor)] {
        if let Some(rest) = uri.strip_prefix(var) {
            return Ok(root.join(rest));
        }
    }
    Err(format!(
        "{} library {lib}: {uri} is under neither ${{VHRD_LIBRARY_DIR}} nor ${{VHRD_VENDOR_LIBRARY_DIR}}",
        kind.label()
    ))
}

/// A symbol name as a file name, the way KiCad 10 escapes it (`/` is `{slash}`).
pub fn file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '/' => out.push_str("{slash}"),
            '\\' => out.push_str("{backslash}"),
            '"' => out.push_str("{dblquote}"),
            '<' => out.push_str("{lt}"),
            '>' => out.push_str("{gt}"),
            '|' => out.push_str("{bar}"),
            ':' => out.push_str("{colon}"),
            '\t' => out.push_str("{tab}"),
            '\n' | '\r' => out.push_str("{return}"),
            c => out.push(c),
        }
    }
    out
}

/// Works out every move in the checkout at `library`, writing nothing.
pub fn plan(map: &Map, library: &Path, vendor: &Path) -> Plan {
    let mut plan = Plan::default();
    symbols(map, library, vendor, &mut plan);
    footprints(map, library, vendor, &mut plan);
    // Two items, one new file; or a new file that is there already with other content.
    let mut seen: BTreeMap<&Path, &str> = BTreeMap::new();
    let mut clashes = Vec::new();
    for m in &plan.moves {
        let Some(to) = &m.to else { continue };
        if let Some(other) = seen.insert(to, &m.id) {
            clashes.push(format!("{other} and {} would both be {}", m.id, to.display()));
        } else if let Ok(there) = std::fs::read_to_string(to)
            && there != m.text
        {
            clashes.push(format!("{} is there already with other content (for {})", to.display(), m.id));
        }
    }
    plan.notes.extend(clashes.into_iter().map(|c| (Severity::Error, c)));
    plan
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> =
        std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
    out.sort();
    out
}

fn rewrite(map: &Map, path: &Path, text: &str, plan: &mut Plan) -> (String, usize) {
    match migrate::rewrite_text(map, path, text) {
        Ok(r) => {
            plan.notes.extend(r.notes.into_iter().map(|(s, m)| (s, format!("{}: {m}", path.display()))));
            (migrate::apply_edits(text, &r.edits), r.edits.len())
        }
        Err(e) => {
            plan.notes.push((Severity::Error, format!("{}: {e}", path.display())));
            (text.to_string(), 0)
        }
    }
}

fn symbols(map: &Map, library: &Path, vendor: &Path, plan: &mut Plan) {
    for file in entries(&library.join("symbols")) {
        if !file.is_file() || file.extension().is_none_or(|e| e != "kicad_sym") {
            continue;
        }
        let lib = file.file_stem().unwrap_or_default().to_string_lossy().to_string();
        if !map.is_old_lib(ItemKind::Symbol, &lib) {
            plan.notes.push((Severity::Warning, format!("{}: not an old library of the map, left", file.display())));
            continue;
        }
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) => {
                plan.notes.push((Severity::Error, format!("{}: {e}", file.display())));
                continue;
            }
        };
        let root = match sexpr::parse(&text) {
            Ok(r) => r,
            Err(e) => {
                plan.notes.push((Severity::Error, format!("{}: {e}", file.display())));
                continue;
            }
        };
        let syms: Vec<&Node> = root.children("symbol").collect();
        let (Some(first), Some(last)) = (syms.first(), syms.last()) else {
            plan.remove.push(file);
            continue;
        };
        let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |i| i + 1);
        let header_end = if text[line_start(first.span().start)..first.span().start].trim().is_empty() {
            line_start(first.span().start)
        } else {
            first.span().start
        };
        let header = &text[..header_end];
        let trailer = &text[last.span().end..];
        // Where each symbol goes: its own line or its library's default; a derived one follows its parent.
        let mut placed: BTreeMap<String, (Target, Why)> = BTreeMap::new();
        for sym in &syms {
            let Some(name) = sym.arg(0) else { continue };
            let found = match map.symbol.get(&format!("{lib}:{name}")) {
                Some(to) => Some((to, Why::Item)),
                None => map.symbol_lib.get(&lib).map(|to| (to, Why::Library)),
            };
            if let Some((to, why)) = found {
                let t = if to.is_empty() { Target::Dropped } else { Target::To(to.clone()) };
                placed.insert(name.to_string(), (t, why));
            }
        }
        let mut ok = true;
        for sym in &syms {
            let Some(name) = sym.arg(0) else { continue };
            let id = format!("{lib}:{name}");
            let mut target = placed.get(name).cloned();
            if let Some(parent) = sym.child("extends").and_then(|e| e.arg(0)) {
                match placed.get(parent) {
                    Some((pt, _)) => {
                        match &target {
                            Some((own, _)) if own == pt => {}
                            own => {
                                if let Some((_, Why::Item)) = own {
                                    plan.notes.push((
                                    Severity::Warning,
                                    format!("{id} extends {parent}: it goes with its parent, not where its map line says"),
                                ));
                                }
                                target = Some((pt.clone(), Why::Parent));
                            }
                        }
                    }
                    None => {
                        plan.notes.push((
                            Severity::Error,
                            format!("{id} extends {parent}, which is not in {}", file.display()),
                        ));
                        ok = false;
                        continue;
                    }
                }
            }
            let Some((target, why)) = target else {
                plan.notes.push((Severity::Error, format!("symbol {id} is not in the map")));
                ok = false;
                continue;
            };
            let span = sym.span();
            let indent = &text[line_start(span.start)..span.start];
            let indent = if indent.trim().is_empty() { indent } else { "\t" };
            let body = format!("{header}{indent}{}{trailer}", &text[span]);
            let (to, new_lib, text, rewritten) = match target {
                Target::Dropped => (None, String::new(), String::new(), 0),
                Target::To(new_lib) => match library_path(map, ItemKind::Symbol, &new_lib, library, vendor) {
                    Ok(dir) => {
                        let to = dir.join(format!("{}.kicad_sym", file_name(name)));
                        let (t, n) = rewrite(map, &to, &body, plan);
                        (Some(to), new_lib, t, n)
                    }
                    Err(e) => {
                        plan.notes.push((Severity::Error, e));
                        ok = false;
                        continue;
                    }
                },
            };
            plan.moves.push(Move { kind: ItemKind::Symbol, id, new_lib, from: file.clone(), to, why, text, rewritten });
        }
        if ok {
            plan.remove.push(file);
        }
    }
}

fn footprints(map: &Map, library: &Path, vendor: &Path, plan: &mut Plan) {
    for dir in entries(&library.join("footprints")) {
        if !dir.is_dir() {
            continue;
        }
        let dir_name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        // vhrd_connector_usb was a footprint folder without the .pretty.
        let lib = dir_name.strip_suffix(".pretty").unwrap_or(&dir_name).to_string();
        if !map.is_old_lib(ItemKind::Footprint, &lib) {
            if !map.libraries(ItemKind::Footprint).contains_key(&lib) {
                plan.notes.push((Severity::Warning, format!("{}: not an old library of the map, left", dir.display())));
            }
            continue;
        }
        let mut all_moved = true;
        for file in entries(&dir) {
            if file.extension().is_none_or(|e| e != "kicad_mod") {
                plan.notes.push((Severity::Warning, format!("{}: not a footprint, left", file.display())));
                all_moved = false;
                continue;
            }
            let name = file.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let id = format!("{lib}:{name}");
            let why = if map.footprint.contains_key(&id) { Why::Item } else { Why::Library };
            let Some(target) = map.item(ItemKind::Footprint, &lib, &name) else {
                plan.notes.push((Severity::Error, format!("footprint {id} is not in the map")));
                all_moved = false;
                continue;
            };
            let old = match std::fs::read_to_string(&file) {
                Ok(t) => t,
                Err(e) => {
                    plan.notes.push((Severity::Error, format!("{}: {e}", file.display())));
                    all_moved = false;
                    continue;
                }
            };
            let (to, new_lib, text, rewritten) = match target {
                Target::Dropped => (None, String::new(), String::new(), 0),
                Target::To(new_lib) => match library_path(map, ItemKind::Footprint, &new_lib, library, vendor) {
                    Ok(d) => {
                        let to = d.join(file.file_name().unwrap_or_default());
                        let (t, n) = rewrite(map, &to, &old, plan);
                        (Some(to), new_lib, t, n)
                    }
                    Err(e) => {
                        plan.notes.push((Severity::Error, e));
                        all_moved = false;
                        continue;
                    }
                },
            };
            plan.moves.push(Move {
                kind: ItemKind::Footprint,
                id,
                new_lib,
                from: file.clone(),
                to,
                why,
                text,
                rewritten,
            });
            plan.remove.push(file);
        }
        if all_moved {
            plan.remove_dirs.push(dir);
        }
    }
}

/// Writes every new file, then removes the old ones. Refuses when the plan has errors.
pub fn apply(plan: &Plan) -> Result<(), String> {
    if plan.has_errors() {
        return Err("the plan has errors; nothing written".into());
    }
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    for m in &plan.moves {
        let Some(to) = &m.to else { continue };
        if std::fs::read_to_string(to).is_ok_and(|t| t == m.text) {
            continue;
        }
        std::fs::create_dir_all(to.parent().unwrap_or(Path::new("."))).map_err(|e| io(to, e))?;
        std::fs::write(to, &m.text).map_err(|e| io(to, e))?;
    }
    for f in &plan.remove {
        std::fs::remove_file(f).map_err(|e| io(f, e))?;
    }
    for d in &plan.remove_dirs {
        // Only when empty: anything else in it stays.
        let _ = std::fs::remove_dir(d);
    }
    Ok(())
}
