//! `kilib migrate`: rewrites boards (and libraries) to a new library layout through a [`Map`], changing only
//! the strings the map matches. Every other byte of a file stays as it was.
//!
//! What is rewritten:
//! - symbol ids: `lib_id`, `lib_name` and the names of the `lib_symbols` cache in `.kicad_sch`;
//! - `Footprint` properties (`old:Name`) and `ki_fp_filters` entries with an old library;
//! - footprint ids in `.kicad_pcb`;
//! - model paths under `${VHRD_3DMODEL_DIR}` (and absolute ones into a `kicad-library/3dmodels` folder);
//! - rows of project `sym-lib-table` and `fp-lib-table`: an old library's row becomes the rows of the new
//!   libraries it feeds, with their URIs from `[libraries]`.
//!
//! A second run changes nothing: no new name is a key of the map.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::check;
use crate::libtable::{LibRow, LibTable, TableKind};
use crate::map::{ItemKind, MODEL_VAR, Map, Target};
use crate::sexpr::{self, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Blocks `--write`: a dropped item still in use, a name clash, a file that cannot be read.
    Error,
    /// Left as it is, worth a look: an old-looking name the map does not know.
    Warning,
}

/// One thing to report, with every file it was found in.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub severity: Severity,
    pub message: String,
    pub uses: Vec<(PathBuf, usize)>,
}

/// One replacement in a file's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    pub path: PathBuf,
    pub old: String,
    pub new: String,
    /// Sorted, never overlapping.
    pub edits: Vec<Edit>,
}

/// Rewritten strings, by kind (each use counts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    pub symbols: usize,
    pub footprint_fields: usize,
    pub footprints: usize,
    pub filters: usize,
    pub models: usize,
    pub table_rows: usize,
}

impl Counts {
    pub fn total(&self) -> usize {
        self.symbols + self.footprint_fields + self.footprints + self.filters + self.models + self.table_rows
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Files read.
    pub files: usize,
    pub changes: Vec<FileChange>,
    pub counts: Counts,
    /// Errors first, then warnings, each sorted by message.
    pub notes: Vec<Note>,
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        self.notes.iter().any(|n| n.severity == Severity::Error)
    }
}

const EXTENSIONS: &[&str] = &["kicad_sch", "kicad_pcb", "kicad_sym", "kicad_mod"];

fn wanted(name: &str) -> bool {
    name == "sym-lib-table"
        || name == "fp-lib-table"
        || Path::new(name).extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e))
}

/// Works out every change under `paths` (files or folders) without writing anything.
pub fn plan(map: &Map, paths: &[PathBuf]) -> Plan {
    let mut files = Vec::new();
    for p in paths {
        check::collect(&std::path::absolute(p).unwrap_or_else(|_| p.clone()), &mut files, &wanted);
    }
    files.sort();
    files.dedup();
    let mut plan = Plan { files: files.len(), ..Plan::default() };
    let mut notes: BTreeMap<(Severity, String), BTreeMap<PathBuf, usize>> = BTreeMap::new();
    for file in &files {
        let mut note = |sev: Severity, msg: String| {
            *notes.entry((sev, msg)).or_default().entry(file.clone()).or_default() += 1;
        };
        let old = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                note(Severity::Error, format!("cannot read: {e}"));
                continue;
            }
        };
        let Rewrite { edits, counts, notes: found } = match rewrite_text(map, file, &old) {
            Ok(r) => r,
            Err(e) => {
                note(Severity::Error, format!("cannot parse: {e}"));
                continue;
            }
        };
        for (sev, msg) in found {
            note(sev, msg);
        }
        if edits.windows(2).any(|w| w[0].range.end > w[1].range.start) {
            note(Severity::Error, "overlapping rewrites (a kilib bug)".into());
            continue;
        }
        let new = apply_edits(&old, &edits);
        if new == old {
            continue;
        }
        add_counts(&mut plan.counts, &counts);
        plan.changes.push(FileChange { path: file.clone(), old, new, edits });
    }
    plan.notes = notes
        .into_iter()
        .map(|((severity, message), uses)| Note { severity, message, uses: uses.into_iter().collect() })
        .collect();
    plan
}

/// Writes the planned changes. Refuses when the plan has errors, or when a file changed since it was read.
pub fn apply(plan: &Plan) -> Result<(), String> {
    if plan.has_errors() {
        return Err("the plan has errors; nothing written".into());
    }
    for c in &plan.changes {
        let now = std::fs::read_to_string(&c.path).map_err(|e| format!("{}: {e}", c.path.display()))?;
        if now != c.old {
            return Err(format!("{} changed since it was read; run again", c.path.display()));
        }
    }
    for c in &plan.changes {
        std::fs::write(&c.path, &c.new).map_err(|e| format!("{}: {e}", c.path.display()))?;
    }
    Ok(())
}

fn add_counts(to: &mut Counts, c: &Counts) {
    to.symbols += c.symbols;
    to.footprint_fields += c.footprint_fields;
    to.footprints += c.footprints;
    to.filters += c.filters;
    to.models += c.models;
    to.table_rows += c.table_rows;
}

pub fn apply_edits(text: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for e in edits {
        out.push_str(&text[at..e.range.start]);
        out.push_str(&e.text);
        at = e.range.end;
    }
    out.push_str(&text[at..]);
    out
}

/// What [`rewrite_text`] found in one file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rewrite {
    /// Sorted.
    pub edits: Vec<Edit>,
    pub counts: Counts,
    pub notes: Vec<(Severity, String)>,
}

/// Rewrites the strings of one file's text that `map` matches. `file` names the file (a library table by its
/// name).
pub fn rewrite_text(map: &Map, file: &Path, text: &str) -> Result<Rewrite, String> {
    let mut r = Rewriter { map, edits: Vec::new(), notes: Vec::new(), counts: Counts::default() };
    r.file(file, text)?;
    r.edits.sort_by_key(|e| e.range.start);
    r.edits.dedup();
    Ok(Rewrite { edits: r.edits, counts: r.counts, notes: r.notes })
}

struct Rewriter<'a> {
    map: &'a Map,
    edits: Vec<Edit>,
    notes: Vec<(Severity, String)>,
    counts: Counts,
}

impl Rewriter<'_> {
    fn error(&mut self, m: String) {
        self.notes.push((Severity::Error, m));
    }

    fn warn(&mut self, m: String) {
        self.notes.push((Severity::Warning, m));
    }

    fn file(&mut self, file: &Path, text: &str) -> Result<(), String> {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name == "sym-lib-table" || name == "fp-lib-table" {
            let kind = if name == "sym-lib-table" { TableKind::Symbol } else { TableKind::Footprint };
            let table = LibTable::parse(kind, file, text).map_err(|e| e.to_string())?;
            self.table(text, &table);
            return Ok(());
        }
        let root = sexpr::parse(text).map_err(|e| e.to_string())?;
        if matches!(root.head(), Some("kicad_pcb")) {
            for fp in root.items().iter().filter(|n| matches!(n.head(), Some("footprint" | "module"))) {
                if let Some(id) = fp.items().get(1) {
                    self.footprint_id(text, id, false);
                }
            }
        }
        self.walk(text, &root);
        Ok(())
    }

    fn walk(&mut self, text: &str, node: &Node) {
        match node.head() {
            Some("lib_id") => {
                if let Some(a) = node.items().get(1) {
                    self.symbol_id(text, a);
                }
            }
            Some("lib_name") => {
                if let Some(a) = node.items().get(1).filter(|a| a.as_atom().is_some_and(|t| t.contains(':'))) {
                    self.symbol_id(text, a);
                }
            }
            Some("lib_symbols") => self.cache(text, node),
            Some("property") => match node.arg(0) {
                Some("Footprint") => {
                    if let Some(a) = node.items().get(2) {
                        self.footprint_id(text, a, true);
                    }
                }
                Some("ki_fp_filters") => {
                    if let Some(a) = node.items().get(2) {
                        self.filters(text, a);
                    }
                }
                _ => {}
            },
            Some("model") => {
                if let Some(a) = node.items().get(1) {
                    self.model(text, a);
                }
            }
            _ => {}
        }
        for child in node.items().iter().filter(|n| matches!(n, Node::List { .. })) {
            self.walk(text, child);
        }
    }

    /// The cached symbols: each name is rewritten, and two that end up with one name are an error.
    fn cache(&mut self, text: &str, node: &Node) {
        let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for sym in node.children("symbol") {
            let Some(a) = sym.items().get(1) else { continue };
            let old = a.as_atom().unwrap_or_default().to_string();
            let new = self.symbol_id(text, a).unwrap_or_else(|| old.clone());
            names.entry(new).or_default().push(old);
        }
        for (new, olds) in names.into_iter().filter(|(_, o)| o.len() > 1) {
            self.error(format!("cached symbols {} would all become {new}", olds.join(", ")));
        }
    }

    /// Rewrites a symbol id; the new id when it changes.
    fn symbol_id(&mut self, text: &str, atom: &Node) -> Option<String> {
        let id = atom.as_atom()?;
        let (lib, name) = id.split_once(':')?;
        match self.map.item(ItemKind::Symbol, lib, name) {
            Some(Target::To(new_lib)) => {
                self.counts.symbols += 1;
                self.edits.push(replace_lib(text, atom, lib, &new_lib));
                Some(format!("{new_lib}:{name}"))
            }
            Some(Target::Dropped) => {
                self.error(format!("symbol {id} was dropped from the library (\"\" in the map) and is still used"));
                None
            }
            None => {
                self.unknown(ItemKind::Symbol, id, lib);
                None
            }
        }
    }

    /// A footprint id of a board footprint or a `Footprint` field.
    fn footprint_id(&mut self, text: &str, atom: &Node, field: bool) {
        let Some(id) = atom.as_atom() else { return };
        let Some((lib, name)) = id.split_once(':') else { return };
        match self.map.item(ItemKind::Footprint, lib, name) {
            Some(Target::To(new_lib)) => {
                if field {
                    self.counts.footprint_fields += 1
                } else {
                    self.counts.footprints += 1
                }
                self.edits.push(replace_lib(text, atom, lib, &new_lib));
            }
            Some(Target::Dropped) => {
                self.error(format!("footprint {id} was dropped from the library (\"\" in the map) and is still used"));
            }
            None => self.unknown(ItemKind::Footprint, id, lib),
        }
    }

    /// An id of a `vhrd_*` library the map does not know is worth a warning; others are not ours.
    fn unknown(&mut self, kind: ItemKind, id: &str, lib: &str) {
        if lib.starts_with("vhrd_") {
            self.warn(format!("{} {id}: not in the map, left as it is", kind.label()));
        }
    }

    /// `ki_fp_filters`: space-separated patterns, some `lib:pattern`.
    fn filters(&mut self, text: &str, atom: &Node) {
        let Some(value) = atom.as_atom() else { return };
        let mut out = String::with_capacity(value.len());
        let mut changed = 0;
        let mut rest = value;
        while !rest.is_empty() {
            let ws = rest.len() - rest.trim_start().len();
            out.push_str(&rest[..ws]);
            rest = &rest[ws..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let token = &rest[..end];
            rest = &rest[end..];
            match self.filter(token) {
                Some(new) => {
                    out.push_str(&new);
                    changed += 1;
                }
                None => out.push_str(token),
            }
        }
        if changed > 0 && out != value {
            self.counts.filters += changed;
            self.edits.push(Edit { range: atom.span(), text: requote(text, atom, &out) });
        }
    }

    fn filter(&mut self, token: &str) -> Option<String> {
        let (lib, pat) = token.split_once(':')?;
        if lib.contains(['*', '?']) || !self.map.is_old_lib(ItemKind::Footprint, lib) {
            return None;
        }
        if !pat.contains(['*', '?']) {
            return match self.map.item(ItemKind::Footprint, lib, pat)? {
                Target::To(new) => Some(format!("{new}:{pat}")),
                Target::Dropped => {
                    self.warn(format!("footprint filter {token}: the footprint was dropped, left as it is"));
                    None
                }
            };
        }
        // A pattern: the libraries its matches went to. One: that library; none: the old library's default;
        // several: the pattern without a library, which KiCad matches against every library.
        let prefix = format!("{lib}:");
        let targets: BTreeSet<&String> = self
            .map
            .footprint
            .iter()
            .filter(|(k, v)| !v.is_empty() && k.strip_prefix(&prefix).is_some_and(|n| glob(pat, n)))
            .map(|(_, v)| v)
            .collect();
        match targets.len() {
            0 => self.map.footprint_lib.get(lib).filter(|l| !l.is_empty()).map(|l| format!("{l}:{pat}")),
            1 => Some(format!("{}:{pat}", targets.into_iter().next()?)),
            _ => {
                let libs: Vec<&str> = targets.iter().map(|s| s.as_str()).collect();
                self.warn(format!(
                    "footprint filter {token}: its footprints went to {}; written without a library",
                    libs.join(", ")
                ));
                Some(pat.to_string())
            }
        }
    }

    fn model(&mut self, text: &str, atom: &Node) {
        let Some(path) = atom.as_atom() else { return };
        let rel = if let Some(rel) = path.strip_prefix(MODEL_VAR) {
            rel
        } else if let Some(i) = path.find("kicad-library/3dmodels/") {
            // An absolute path into a kicad-library checkout (another PC's): the same model.
            &path[i + "kicad-library/3dmodels/".len()..]
        } else {
            return;
        };
        match self.map.model(rel) {
            Some(Target::To(new)) => {
                self.counts.models += 1;
                self.edits.push(Edit { range: atom.span(), text: requote(text, atom, &format!("{MODEL_VAR}{new}")) });
            }
            Some(Target::Dropped) => self.warn(format!("3D model {path}: dropped from the library, left as it is")),
            None if self.map.is_new_model(rel) => {}
            None => self.warn(format!("3D model {path}: not in the map, left as it is")),
        }
    }

    /// An old library's row becomes the rows of the libraries it feeds that the table lacks.
    fn table(&mut self, text: &str, table: &LibTable) {
        let kind = if table.kind == TableKind::Symbol { ItemKind::Symbol } else { ItemKind::Footprint };
        let mut present: BTreeSet<String> = table.rows.iter().map(|r| r.name.clone()).collect();
        for row in &table.rows {
            if !self.map.is_old_lib(kind, &row.name) {
                if row.name.starts_with("vhrd_") {
                    self.warn(format!("{} library row {}: not in the map, left as it is", kind.label(), row.name));
                }
                continue;
            }
            let targets = self.map.targets_of(kind, &row.name);
            // One new library for the old one: it keeps the row's description.
            let descr = if targets.len() == 1 { row.descr.as_str() } else { "" };
            let mut rows = Vec::new();
            for lib in targets {
                if present.insert(lib.clone()) {
                    let uri = &self.map.libraries(kind)[&lib];
                    rows.push(LibRow::render(&lib, "KiCad", uri, descr));
                }
            }
            self.counts.table_rows += 1;
            let line_start = text[..row.span.start].rfind('\n').map_or(0, |i| i + 1);
            let indent = &text[line_start..row.span.start];
            if rows.is_empty() {
                // Its libraries are in the table already: the row goes, with its line when it has one alone.
                let after = &text[row.span.end..];
                let eol = after.find('\n').map(|i| row.span.end + i + 1);
                match eol {
                    Some(end) if indent.trim().is_empty() && text[row.span.end..end].trim().is_empty() => {
                        self.edits.push(Edit { range: line_start..end, text: String::new() })
                    }
                    _ => self.edits.push(Edit { range: row.span.clone(), text: String::new() }),
                }
            } else {
                let sep = if indent.trim().is_empty() { format!("\n{indent}") } else { " ".to_string() };
                self.edits.push(Edit { range: row.span.clone(), text: rows.join(&sep) });
            }
        }
    }
}

/// Replaces only the library part of an id atom (`"old:Name"` → `"new:Name"`), so the rest of the atom stays
/// byte for byte, whatever escapes it has.
fn replace_lib(text: &str, atom: &Node, old: &str, new: &str) -> Edit {
    let span = atom.span();
    let raw = &text[span.clone()];
    let skip = usize::from(raw.starts_with('"'));
    if raw[skip..].starts_with(old) && raw[skip + old.len()..].starts_with(':') {
        let start = span.start + skip;
        return Edit { range: start..start + old.len(), text: new.to_string() };
    }
    let value = atom.as_atom().unwrap_or_default();
    let rest = value.split_once(':').map_or("", |(_, n)| n);
    Edit { range: span, text: sexpr::quote(&format!("{new}:{rest}")) }
}

/// A new value for an atom, quoted when the old one was (or when it must be).
fn requote(text: &str, atom: &Node, value: &str) -> String {
    let quoted = text[atom.span()].starts_with('"');
    if quoted || value.is_empty() || value.contains(|c: char| c.is_whitespace() || "()\"\\".contains(c)) {
        sexpr::quote(value)
    } else {
        value.to_string()
    }
}

/// `*` and `?` as in KiCad's footprint filters, case-insensitive.
pub fn glob(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let (mut pi, mut si, mut star, mut mark) = (0, 0, None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

impl FileChange {
    /// A unified diff (`--- a/LABEL`, `+++ b/LABEL`, 3 lines of context), made from the edits.
    pub fn diff(&self, label: &str) -> String {
        unified_diff(label, &self.old, &self.edits)
    }
}

/// A unified diff of `old` and `old` with `edits` applied, from the edits themselves: each edit's lines are
/// one region, regions on shared lines merge, and hunks join when their context touches.
pub fn unified_diff(label: &str, old: &str, edits: &[Edit]) -> String {
    const CONTEXT: usize = 3;
    let starts: Vec<usize> = std::iter::once(0).chain(old.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let lines: Vec<&str> = old.split_inclusive('\n').collect();
    let line_of = |at: usize| starts.partition_point(|&s| s <= at) - 1;
    // Regions: old lines lo..hi replaced by new text (whole lines).
    let mut regions: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i < edits.len() {
        let lo = line_of(edits[i].range.start);
        let mut hi = line_of(edits[i].range.end.saturating_sub(1).max(edits[i].range.start)) + 1;
        let mut j = i + 1;
        while j < edits.len() && line_of(edits[j].range.start) <= hi {
            hi = hi.max(line_of(edits[j].range.end.saturating_sub(1).max(edits[j].range.start)) + 1);
            j += 1;
        }
        let hi = hi.min(lines.len());
        let end = starts.get(hi).copied().unwrap_or(old.len());
        let mut text = String::new();
        let mut at = starts[lo];
        for e in &edits[i..j] {
            text.push_str(&old[at..e.range.start]);
            text.push_str(&e.text);
            at = e.range.end;
        }
        text.push_str(&old[at..end]);
        regions.push((lo, hi, text));
        i = j;
    }
    let mut out = format!("--- a/{label}\n+++ b/{label}\n");
    let push = |out: &mut String, sign: char, line: &str| {
        out.push(sign);
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push_str("\n\\ No newline at end of file\n");
        }
    };
    let mut delta: isize = 0;
    let mut r = 0;
    while r < regions.len() {
        let mut last = r;
        while last + 1 < regions.len() && regions[last + 1].0 <= regions[last].1 + 2 * CONTEXT {
            last += 1;
        }
        let from = regions[r].0.saturating_sub(CONTEXT);
        let to = (regions[last].1 + CONTEXT).min(lines.len());
        let mut body = String::new();
        let (mut old_n, mut new_n) = (0usize, 0usize);
        let mut at = from;
        let mut hunk_delta = 0isize;
        for (lo, hi, text) in &regions[r..=last] {
            for l in &lines[at..*lo] {
                push(&mut body, ' ', l);
                old_n += 1;
                new_n += 1;
            }
            for l in &lines[*lo..*hi] {
                push(&mut body, '-', l);
                old_n += 1;
            }
            let new_lines: Vec<&str> = text.split_inclusive('\n').collect();
            for l in &new_lines {
                push(&mut body, '+', l);
                new_n += 1;
            }
            hunk_delta += new_lines.len() as isize - (*hi - *lo) as isize;
            at = *hi;
        }
        for l in &lines[at..to] {
            push(&mut body, ' ', l);
            old_n += 1;
            new_n += 1;
        }
        let new_from = (from as isize + delta) as usize;
        let start = |n: usize, s: usize| if n == 0 { s } else { s + 1 };
        out.push_str(&format!("@@ -{},{old_n} +{},{new_n} @@\n", start(old_n, from), start(new_n, new_from)));
        out.push_str(&body);
        delta += hunk_delta;
        r = last + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob("SOT?23*", "SOT-23-5"));
        assert!(glob("*", ""));
        assert!(glob("r_*", "R_0402"));
        assert!(!glob("SOT?23", "SOT-23-5"));
        assert!(glob("*_1x??_*", "PinHeader_1x04_P2.54mm"));
    }

    #[test]
    fn diff_from_edits() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\n";
        let at = |s: &str| old.find(s).unwrap();
        let edits = vec![
            Edit { range: at("b")..at("b") + 1, text: "B".into() },
            Edit { range: at("c")..at("c") + 1, text: "C1\nC2".into() },
            Edit { range: at("m")..at("m") + 1, text: "M".into() },
        ];
        let d = unified_diff("x", old, &edits);
        assert_eq!(
            d,
            "--- a/x\n+++ b/x\n@@ -1,6 +1,7 @@\n a\n-b\n-c\n+B\n+C1\n+C2\n d\n e\n f\n@@ -10,4 +11,4 @@\n j\n k\n l\n-m\n+M\n"
        );
        let d = unified_diff("x", "one", &[Edit { range: 0..3, text: "two".into() }]);
        assert!(d.ends_with("-one\n\\ No newline at end of file\n+two\n\\ No newline at end of file\n"), "{d}");
    }
}
