//! `kilib check`: every symbol, footprint, `Footprint` field and 3D model a board names resolves the way KiCad
//! would resolve it on this PC.
//!
//! Resolution follows KiCad: a `lib:name` id looks up `lib` in the project's library table (beside the
//! `.kicad_pro`), then in the global one; the row's URI is expanded with the path variables (`KIPRJMOD` is the
//! project folder) and must name a library that holds `name`. A model path is expanded the same way; a relative
//! one is tried against the project folder, then against `KICAD10_3DMODEL_DIR`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::libtable::{LibRow, LibTable, TableKind};
use crate::sexpr::{self, Node};
use crate::vars::{Unset, Vars};

/// What `check` resolves against.
#[derive(Debug, Clone, Default)]
pub struct Setup {
    pub global_sym: Option<LibTable>,
    pub global_fp: Option<LibTable>,
    /// Path variables, without `KIPRJMOD` (set per project).
    pub vars: Vars,
    /// Every library named must start with this (`VHRD_`): a symbol, footprint or `Footprint` field from another
    /// library is a problem, and so are the schematic's embedded symbols and the library part of `ki_fp_filters`.
    pub only: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A placed symbol's `lib_id` in a schematic.
    Symbol,
    /// A `Footprint` field of a schematic symbol or a library symbol.
    FootprintField,
    /// A footprint's library id in a board.
    Footprint,
    /// A symbol of a schematic's `lib_symbols`, the copy it carries (checked only with [`Setup::only`]).
    EmbeddedSymbol,
    /// The library part of a `ki_fp_filters` pattern (checked only with [`Setup::only`]).
    FootprintFilter,
    /// A 3D model path in a board or a footprint.
    Model,
    /// A board file that could not be read.
    File,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Symbol => "symbol",
            Kind::FootprintField => "Footprint field",
            Kind::Footprint => "footprint",
            Kind::EmbeddedSymbol => "embedded symbol",
            Kind::FootprintFilter => "footprint filter",
            Kind::Model => "3D model",
            Kind::File => "file",
        }
    }
}

/// One thing that did not resolve (or could not be checked), with every place that names it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub kind: Kind,
    /// `lib:name`, or the model path as written.
    pub id: String,
    /// Why, in a few words: "library vhrd_x is not in any sym-lib-table".
    pub reason: String,
    /// The library nickname, when the reason is about the library rather than the item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// File (as found under the checked path) and how many times it names `id`.
    pub uses: Vec<(PathBuf, usize)>,
}

impl Finding {
    pub fn count(&self) -> usize {
        self.uses.iter().map(|(_, n)| n).sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Counts {
    pub symbols: usize,
    pub footprint_fields: usize,
    pub footprints: usize,
    pub models: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    pub projects: usize,
    pub files: usize,
    /// References checked (each use counts), resolved or not.
    pub checked: Counts,
    /// What did not resolve, sorted by kind and id.
    pub problems: Vec<Finding>,
    /// What kilib cannot check (a library of a type it does not read, like `Legacy` or `Database`).
    pub unchecked: Vec<Finding>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }

    /// Problems whose reason is about the library: one line per library instead of per item.
    pub fn broken_libraries(&self) -> BTreeMap<(Kind, String), Vec<&Finding>> {
        let mut out: BTreeMap<(Kind, String), Vec<&Finding>> = BTreeMap::new();
        for f in &self.problems {
            if let Some(lib) = &f.library {
                out.entry((f.kind, lib.clone())).or_default().push(f);
            }
        }
        out
    }
}

/// Checks every `.kicad_sch`, `.kicad_pcb`, `.kicad_sym` and `.kicad_mod` under `paths` (files or folders).
pub fn check(paths: &[PathBuf], setup: &Setup) -> Report {
    let mut files = Vec::new();
    for p in paths {
        // Absolute, so KIPRJMOD is too: KiCad's own is always absolute.
        collect(&std::path::absolute(p).unwrap_or_else(|_| p.clone()), &mut files, &is_board_file);
    }
    files.sort();
    files.dedup();

    let mut checker = Checker { setup, libs: HashMap::new(), tables: HashMap::new(), found: BTreeMap::new() };
    let mut report = Report { files: files.len(), ..Report::default() };
    let mut projects = HashSet::new();
    for file in &files {
        let project = project_dir(file);
        projects.insert(project.clone());
        checker.file(file, &project, &mut report.checked);
    }
    report.projects = projects.len();
    for ((kind, id, reason), noted) in checker.found {
        let finding = Finding { kind, id, reason, library: noted.library, uses: noted.uses.into_iter().collect() };
        if noted.unchecked { report.unchecked.push(finding) } else { report.problems.push(finding) }
    }
    report
}

const EXTENSIONS: &[&str] = &["kicad_sch", "kicad_pcb", "kicad_sym", "kicad_mod"];

fn is_board_file(name: &str) -> bool {
    Path::new(name).extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e))
}

/// The files under `path` whose name `want` takes; a path that is a file is taken as it is. Hidden folders,
/// KiCad's backups and autosaves are skipped.
pub(crate) fn collect(path: &Path, out: &mut Vec<PathBuf>, want: &dyn Fn(&str) -> bool) {
    let Ok(meta) = std::fs::metadata(path) else {
        out.push(path.to_path_buf()); // reported as unreadable
        return;
    };
    if meta.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else { return };
    for entry in entries.filter_map(Result::ok) {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            // Hidden folders, KiCad's own backups, build output; symlinked folders could loop.
            if name.starts_with('.') || name.ends_with("-backups") || name == "target" {
                continue;
            }
            collect(&p, out, want);
        } else if want(&name) && !name.starts_with("_autosave-") && !name.starts_with('~') {
            out.push(p);
        }
    }
}

/// The folder of the nearest `.kicad_pro` at or above the file, else the file's own folder.
fn project_dir(file: &Path) -> PathBuf {
    let start = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut dir = Some(start.as_path());
    while let Some(d) = dir {
        let has_pro = std::fs::read_dir(d).ok().is_some_and(|rd| {
            rd.filter_map(Result::ok).any(|e| e.path().extension().is_some_and(|x| x == "kicad_pro"))
        });
        if has_pro {
            return d.to_path_buf();
        }
        dir = d.parent();
    }
    start
}

/// What is in a library, read once per run.
enum Contents {
    /// A `.kicad_sym` file: its symbol names.
    Symbols(HashSet<String>),
    /// A `.kicad_symdir` folder: one file per symbol, names read from the files only when a stem does not match.
    SymDir { dir: PathBuf, stems: HashSet<String>, names: Option<HashSet<String>> },
    /// A `.pretty` folder: footprint file stems.
    Footprints(HashSet<String>),
}

/// How a library nickname resolved for one project.
enum Lib {
    Found(PathBuf),
    /// Present but not of a type kilib reads.
    Unchecked(String),
    Broken(String),
}

/// (kind, id, reason)
type Key = (Kind, String, String);

/// What is noted for a key: the library the reason is about, whether it is only unchecked, uses per file.
struct Noted {
    library: Option<String>,
    unchecked: bool,
    uses: BTreeMap<PathBuf, usize>,
}

struct Checker<'a> {
    setup: &'a Setup,
    libs: HashMap<PathBuf, Result<Contents, String>>,
    /// Project tables and nested tables, by path.
    tables: HashMap<PathBuf, Option<LibTable>>,
    found: BTreeMap<Key, Noted>,
}

impl Checker<'_> {
    fn note(&mut self, kind: Kind, id: &str, reason: String, library: Option<&str>, unchecked: bool, file: &Path) {
        let entry = self.found.entry((kind, id.to_string(), reason)).or_insert_with(|| Noted {
            library: library.map(str::to_string),
            unchecked,
            uses: BTreeMap::new(),
        });
        *entry.uses.entry(file.to_path_buf()).or_default() += 1;
    }

    fn file(&mut self, file: &Path, project: &Path, counts: &mut Counts) {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => return self.note(Kind::File, &file.to_string_lossy(), e.to_string(), None, false, file),
        };
        let root = match sexpr::parse(&text) {
            Ok(r) => r,
            Err(e) => return self.note(Kind::File, &file.to_string_lossy(), e.to_string(), None, false, file),
        };
        let mut vars = self.setup.vars.clone();
        vars.set("KIPRJMOD", project.to_string_lossy());
        let ctx = Ctx { file, project, vars: &vars };
        match root.head() {
            Some("kicad_sch") => {
                for sym in root.children("symbol") {
                    if let Some(id) = sym.value("lib_id") {
                        counts.symbols += 1;
                        self.lib_item(&ctx, Kind::Symbol, id);
                    }
                    self.footprint_field(&ctx, sym, counts);
                }
                for cached in root.children("lib_symbols").flat_map(|l| l.children("symbol")) {
                    self.embedded(&ctx, cached);
                }
            }
            Some("kicad_symbol_lib") => {
                for sym in root.children("symbol") {
                    self.footprint_field(&ctx, sym, counts);
                    self.filters(&ctx, sym);
                }
            }
            Some("kicad_pcb") => {
                for fp in root.items().iter().filter(|n| matches!(n.head(), Some("footprint" | "module"))) {
                    if let Some(id) = fp.arg(0) {
                        counts.footprints += 1;
                        self.lib_item(&ctx, Kind::Footprint, id);
                    }
                    self.models(&ctx, fp, counts);
                }
            }
            Some("footprint" | "module") => self.models(&ctx, &root, counts),
            _ => {}
        }
    }

    fn footprint_field(&mut self, ctx: &Ctx, sym: &Node, counts: &mut Counts) {
        for prop in sym.children("property") {
            if prop.arg(0) == Some("Footprint")
                && let Some(value) = prop.arg(1).filter(|v| !v.trim().is_empty())
            {
                counts.footprint_fields += 1;
                self.lib_item(ctx, Kind::FootprintField, value);
            }
        }
    }

    /// With [`Setup::only`], a schematic's embedded symbol: its name, `Footprint` field and filters name only such
    /// libraries. Nothing here is resolved: the placed symbols and the board are.
    fn embedded(&mut self, ctx: &Ctx, sym: &Node) {
        if self.setup.only.is_none() {
            return;
        }
        if let Some(id) = sym.arg(0) {
            self.foreign(ctx, Kind::EmbeddedSymbol, id);
        }
        for prop in sym.children("property") {
            if prop.arg(0) == Some("Footprint")
                && let Some(value) = prop.arg(1).filter(|v| !v.trim().is_empty())
            {
                self.foreign(ctx, Kind::FootprintField, value);
            }
        }
        self.filters(ctx, sym);
    }

    /// With [`Setup::only`], the library part of each `ki_fp_filters` pattern.
    fn filters(&mut self, ctx: &Ctx, sym: &Node) {
        if self.setup.only.is_none() {
            return;
        }
        let filters = sym.children("property").find(|p| p.arg(0) == Some("ki_fp_filters")).and_then(|p| p.arg(1));
        for token in filters.unwrap_or("").split_whitespace() {
            if token.contains(':') {
                self.foreign(ctx, Kind::FootprintFilter, token);
            }
        }
    }

    /// Notes `id` when [`Setup::only`] is set and its library does not start with it; true when it was noted.
    fn foreign(&mut self, ctx: &Ctx, kind: Kind, id: &str) -> bool {
        let Some(prefix) = &self.setup.only else { return false };
        let Some((lib, _)) = id.split_once(':').filter(|(l, _)| !l.is_empty()) else { return false };
        if lib.starts_with(prefix.as_str()) {
            return false;
        }
        let reason = format!("library {lib} is not a {prefix} library");
        self.note(kind, id, reason, Some(lib), false, ctx.file);
        true
    }

    fn models(&mut self, ctx: &Ctx, fp: &Node, counts: &mut Counts) {
        for model in fp.children("model") {
            let Some(raw) = model.arg(0).filter(|m| !m.is_empty()) else { continue };
            counts.models += 1;
            if let Some(reason) = model_problem(raw, ctx.project, ctx.vars) {
                self.note(Kind::Model, raw, reason, None, false, ctx.file);
            }
        }
    }

    /// `lib:name` for a symbol (`Kind::Symbol`) or a footprint (the other two kinds).
    fn lib_item(&mut self, ctx: &Ctx, kind: Kind, id: &str) {
        let table = if kind == Kind::Symbol { TableKind::Symbol } else { TableKind::Footprint };
        let Some((lib, name)) = id.split_once(':').filter(|(l, _)| !l.is_empty()) else {
            // A board footprint without a library link (a panel's tabs and holes, one drawn in the board) is
            // fine in KiCad; only "update from library" has nothing to go by.
            let unchecked = kind == Kind::Footprint;
            return self.note(kind, id, "no library link".into(), None, unchecked, ctx.file);
        };
        if lib == "clipboard" && kind == Kind::Footprint {
            return self.note(kind, id, "pasted from the clipboard, no library link".into(), None, true, ctx.file);
        }
        if self.foreign(ctx, kind, id) {
            return;
        }
        let path = match self.resolve(ctx, table, lib) {
            Lib::Found(p) => p,
            Lib::Unchecked(why) => return self.note(kind, id, why, Some(lib), true, ctx.file),
            Lib::Broken(why) => return self.note(kind, id, why, Some(lib), false, ctx.file),
        };
        let contents = self.libs.entry(path.clone()).or_insert_with(|| read_lib(table, &path));
        let has = match contents {
            Err(e) => Err(format!("library {lib}: {e}")),
            Ok(Contents::Symbols(names)) | Ok(Contents::Footprints(names)) => Ok(names.contains(name)),
            Ok(Contents::SymDir { dir, stems, names }) => {
                Ok(stems.contains(name) || names.get_or_insert_with(|| symdir_names(dir)).contains(name))
            }
        };
        let has = match has {
            Ok(has) => has,
            Err(reason) => return self.note(kind, id, reason, Some(lib), false, ctx.file),
        };
        if !has {
            let reason = format!("not in {lib} ({})", path.display());
            self.note(kind, id, reason, None, false, ctx.file);
        }
    }

    fn resolve(&mut self, ctx: &Ctx, kind: TableKind, lib: &str) -> Lib {
        let project_table = ctx.project.join(kind.file_name());
        let project = self.table(&project_table, kind).cloned();
        let global = match kind {
            TableKind::Symbol => self.setup.global_sym.clone(),
            TableKind::Footprint => self.setup.global_fp.clone(),
        };
        for table in project.iter().chain(global.iter()) {
            if let Some(found) = self.lookup(ctx, table, lib, 0) {
                return found;
            }
        }
        let where_ = if global.is_none() { " (and there is no global table)" } else { "" };
        Lib::Broken(format!("library {lib} is not in any {}{where_}", kind.file_name()))
    }

    fn table(&mut self, path: &Path, kind: TableKind) -> Option<&LibTable> {
        self.tables
            .entry(path.to_path_buf())
            .or_insert_with(|| LibTable::read_optional(kind, path).ok().flatten())
            .as_ref()
    }

    fn lookup(&mut self, ctx: &Ctx, table: &LibTable, lib: &str, depth: usize) -> Option<Lib> {
        if let Some(row) = table.find(lib) {
            return Some(row_lib(row, table, ctx.vars, ctx.project));
        }
        // Nested tables (`(type "Table")`), as KiCad 9 and later allow.
        if depth > 4 {
            return None;
        }
        for row in table.rows.iter().filter(|r| r.kind == "Table" && !r.disabled) {
            let Ok(uri) = ctx.vars.expand(&row.uri) else { continue };
            let path = absolute(&uri, ctx.project);
            let Some(nested) = self.table(&path, table.kind).cloned() else { continue };
            if let Some(found) = self.lookup(ctx, &nested, lib, depth + 1) {
                return Some(found);
            }
        }
        None
    }
}

struct Ctx<'a> {
    file: &'a Path,
    project: &'a Path,
    vars: &'a Vars,
}

fn row_lib(row: &LibRow, table: &LibTable, vars: &Vars, project: &Path) -> Lib {
    let lib = &row.name;
    let at = table.path.display();
    if row.disabled {
        return Lib::Broken(format!("library {lib} is disabled in {at}"));
    }
    if row.kind != "KiCad" {
        return Lib::Unchecked(format!("library {lib} is of type {}, which kilib does not read", row.kind));
    }
    let uri = match vars.expand(&row.uri) {
        Ok(u) => u,
        Err(Unset(names)) => {
            return Lib::Broken(format!("library {lib}: {} unset ({at}: {})", names.join(", "), row.uri));
        }
    };
    let path = absolute(&uri, project);
    if !path.exists() {
        return Lib::Broken(format!("library {lib} is missing: {} ({at})", path.display()));
    }
    Lib::Found(path)
}

fn absolute(path: &str, base: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
}

fn read_lib(kind: TableKind, path: &Path) -> Result<Contents, String> {
    if path.is_dir() {
        let stems = stems(path, if kind == TableKind::Symbol { "kicad_sym" } else { "kicad_mod" })?;
        return Ok(match kind {
            TableKind::Symbol => Contents::SymDir { dir: path.to_path_buf(), stems, names: None },
            TableKind::Footprint => Contents::Footprints(stems),
        });
    }
    if kind == TableKind::Footprint {
        return Err(format!("{} is a file, a footprint library is a folder", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let root = sexpr::parse(&text).map_err(|e| e.to_string())?;
    Ok(Contents::Symbols(symbol_names(&root)))
}

fn symbol_names(root: &Node) -> HashSet<String> {
    root.children("symbol").filter_map(|s| s.arg(0)).map(str::to_string).collect()
}

fn symdir_names(dir: &Path) -> HashSet<String> {
    let mut names = HashSet::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok) {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "kicad_sym")
            && let Ok(text) = std::fs::read_to_string(&p)
            && let Ok(root) = sexpr::parse(&text)
        {
            names.extend(symbol_names(&root));
        }
    }
    names
}

fn stems(dir: &Path, ext: &str) -> Result<HashSet<String>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    Ok(rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .filter_map(|p| Some(p.file_stem()?.to_string_lossy().to_string()))
        .collect())
}

/// Why a model path does not resolve, or `None` when it does.
pub fn model_problem(raw: &str, project: &Path, vars: &Vars) -> Option<String> {
    if raw.starts_with("kicad-embed://") {
        return None; // embedded in the board itself
    }
    let expanded = match vars.expand(raw) {
        Ok(e) => e,
        Err(Unset(names)) => return Some(format!("{} unset", names.join(", "))),
    };
    let windows = expanded.len() > 2 && expanded.as_bytes()[1] == b':' && expanded.as_bytes()[0].is_ascii_alphabetic();
    if windows && !cfg!(windows) {
        return Some("a Windows path".into());
    }
    let p = Path::new(&expanded);
    let tried = if p.is_absolute() {
        vec![p.to_path_buf()]
    } else {
        let mut tried = vec![project.join(p)];
        if let Some(dir) = vars.get("KICAD10_3DMODEL_DIR") {
            tried.push(Path::new(dir).join(p));
        }
        tried
    };
    if tried.iter().any(|t| t.is_file()) {
        return None;
    }
    // KiCad 10's stock models are STEP only; it offers to switch a board's .wrl references when it opens one.
    let step = tried
        .iter()
        .map(|t| t.with_extension("step"))
        .find(|s| s.is_file() && tried.iter().any(|t| t.extension().is_some_and(|e| e.eq_ignore_ascii_case("wrl"))));
    let tried: Vec<String> = tried.iter().map(|t| t.display().to_string()).collect();
    let tried = tried.join(", ");
    Some(match step {
        Some(_) => format!("missing .wrl, the .step beside it exists: {tried}"),
        None => format!("missing: {tried}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libtable::{LibTable, TableKind};

    #[test]
    fn model_paths() {
        let dir = crate::testdir::TestDir::new("model_paths");
        std::fs::create_dir_all(dir.path().join("p/m")).unwrap();
        std::fs::write(dir.path().join("p/m/a.step"), "").unwrap();
        let mut vars = Vars::new();
        vars.set("M", dir.path().join("p/m").to_string_lossy());
        let project = dir.path().join("p");
        assert_eq!(model_problem("${M}/a.step", &project, &vars), None);
        assert_eq!(model_problem("m/a.step", &project, &vars), None);
        assert_eq!(model_problem("kicad-embed://x.step", &project, &vars), None);
        assert_eq!(model_problem("${N}/a.step", &project, &vars).unwrap(), "N unset");
        assert!(model_problem("${M}/b.step", &project, &vars).unwrap().starts_with("missing: "));
        assert!(model_problem("${M}/a.wrl", &project, &vars).unwrap().starts_with("missing .wrl, the .step beside"));
        if !cfg!(windows) {
            assert_eq!(model_problem("C:/Users/x/a.step", &project, &vars).unwrap(), "a Windows path");
        }
    }

    fn fixture_report() -> (crate::testdir::TestDir, Report) {
        let dir = crate::testdir::TestDir::new("check_fixture");
        crate::testdir::fixture(&dir);
        let cfg = dir.path().join("cfg/10.0");
        let setup = Setup {
            global_sym: LibTable::read_optional(TableKind::Symbol, &cfg.join("sym-lib-table")).unwrap(),
            global_fp: LibTable::read_optional(TableKind::Footprint, &cfg.join("fp-lib-table")).unwrap(),
            vars: crate::config::vars(Some(&cfg), crate::testdir::fixture_env(&dir), None).unwrap(),
            only: None,
        };
        let report = check(&[dir.path().join("boards"), dir.path().join("home/lib")], &setup);
        (dir, report)
    }

    #[test]
    fn fixture_resolves_and_breaks_as_documented() {
        let (_dir, report) = fixture_report();
        let mut problems: Vec<(Kind, &str, &str)> =
            report.problems.iter().map(|f| (f.kind, f.id.as_str(), f.reason.as_str())).collect();
        problems.sort();
        let ids: Vec<(Kind, &str)> = problems.iter().map(|(k, id, _)| (*k, *id)).collect();
        assert_eq!(
            ids,
            vec![
                (Kind::Symbol, "Device:Q"),
                (Kind::Symbol, "off:Z"),
                (Kind::Symbol, "unset:U"),
                (Kind::Symbol, "vhrd_connector_debug:TC2050"),
                (Kind::FootprintField, "vhrd_generic:MISSING"),
                (Kind::Model, "${KICAD6_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.wrl"),
                (Kind::Model, "${UNSET_DIR}/a.step"),
                (Kind::Model, "${VHRD_3DMODEL_DIR}/modules/cannify_micro.step"),
                (Kind::Model, "C:/Users/someone/Documents/GitHub/kicad-library/3dmodels/x.STEP"),
            ],
            "{problems:#?}"
        );
        let reason = |id: &str| problems.iter().find(|p| p.1 == id).unwrap().2;
        assert!(reason("Device:Q").starts_with("not in Device ("));
        assert!(reason("off:Z").starts_with("library off is disabled in "));
        assert!(reason("unset:U").starts_with("library unset: NOT_SET unset"));
        assert_eq!(reason("vhrd_connector_debug:TC2050"), "library vhrd_connector_debug is not in any sym-lib-table");
        assert!(
            reason("${KICAD6_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.wrl").starts_with("missing .wrl")
        );
        assert_eq!(reason("${UNSET_DIR}/a.step"), "UNSET_DIR unset");

        let unchecked: Vec<&str> = report.unchecked.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(unchecked, vec!["old:Y", "NPTH"]);

        // Library-level reasons group by library, and the uses count every placement.
        let libs = report.broken_libraries();
        let names: Vec<&str> = libs.keys().map(|(_, l)| l.as_str()).collect();
        assert_eq!(names, vec!["off", "unset", "vhrd_connector_debug"]);
        // b1.kicad_sch, b1.kicad_pcb, local.kicad_sym, vhrd_power.kicad_sym, SOT-23.kicad_mod; not the backups
        assert_eq!(report.files, 5);
    }

    #[test]
    fn only_flags_every_other_library() {
        let dir = crate::testdir::TestDir::new("check_only");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            b.join("b.kicad_sch"),
            r#"(kicad_sch (version 20250114)
  (lib_symbols
    (symbol "Jumper:SolderJumper_2_Open"
      (property "Footprint" "Jumper:SolderJumper-2_P1.3mm_Open" (at 0 0 0))
      (property "ki_fp_filters" "SolderJumper*Open* Connector*:*_1x??_* VHRD_Jumper:*" (at 0 0 0)))
    (symbol "VHRD_Device:R" (property "Footprint" "" (at 0 0 0))))
  (symbol (lib_id "Jumper:SolderJumper_2_Open") (property "Footprint" "VHRD_Jumper:SJ" (at 0 0 0)))
  (symbol (lib_id "VHRD_Device:R") (property "Footprint" "Resistor_SMD:R_0402_1005Metric" (at 0 0 0))))
"#,
        )
        .unwrap();
        std::fs::write(b.join("b.kicad_pcb"), r#"(kicad_pcb (footprint "Jumper:SJ") (footprint "VHRD_R:R"))"#).unwrap();
        let only = |only: Option<&str>| {
            let setup = Setup { only: only.map(str::to_string), ..Setup::default() };
            let mut found: Vec<(Kind, String)> = check(std::slice::from_ref(&b), &setup)
                .problems
                .into_iter()
                .filter(|f| f.reason.contains("is not a VHRD_ library"))
                .map(|f| (f.kind, f.id))
                .collect();
            found.sort();
            found
        };
        assert_eq!(only(None), vec![]);
        let s = |k: Kind, id: &str| (k, id.to_string());
        assert_eq!(
            only(Some("VHRD_")),
            vec![
                s(Kind::Symbol, "Jumper:SolderJumper_2_Open"),
                s(Kind::FootprintField, "Jumper:SolderJumper-2_P1.3mm_Open"),
                s(Kind::FootprintField, "Resistor_SMD:R_0402_1005Metric"),
                s(Kind::Footprint, "Jumper:SJ"),
                s(Kind::EmbeddedSymbol, "Jumper:SolderJumper_2_Open"),
                s(Kind::FootprintFilter, "Connector*:*_1x??_*"),
            ]
        );
    }

    #[test]
    fn fixture_counts_every_reference() {
        let (_dir, report) = fixture_report();
        // 11 placed symbols; Footprint fields: 3 in the schematic + 1 in vhrd_power.kicad_sym; 4 board footprints;
        // models: 8 in the board + 1 in SOT-23.kicad_mod; projects: b1 and the two library folders.
        assert_eq!(report.checked, Counts { symbols: 11, footprint_fields: 4, footprints: 4, models: 9 });
        assert_eq!(report.projects, 3);
        let r = report.problems.iter().find(|f| f.id == "Device:Q").unwrap();
        assert_eq!(r.count(), 1);
    }
}
