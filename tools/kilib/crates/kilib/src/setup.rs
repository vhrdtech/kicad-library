//! `kilib setup`: points this PC's KiCad at a kicad-library checkout.
//!
//! - The global `sym-lib-table` and `fp-lib-table` get one row per library, through `${VHRD_LIBRARY_DIR}` (this
//!   checkout) and `${VHRD_VENDOR_LIBRARY_DIR}` (the private vendor library), so the tables hold no absolute path.
//!   The rows come from a rename map's `[libraries.symbol]` and `[libraries.footprint]` tables, or from the library
//!   folders found in the checkout and the vendor library. They replace every `vhrd_*`/`VHRD_*` row and any row on
//!   those variables or pointing into those folders; every other row stays byte for byte.
//! - `kicad_common.json` gets the path variables: `VHRD_LIBRARY_DIR`, `VHRD_VENDOR_LIBRARY_DIR` and
//!   `VHRD_3DMODEL_DIR` when given, and `VHRD_TEMPLATE_DIR` and `KICAD_USER_TEMPLATE_DIR` (the checkout's
//!   `templates/`, the drawing sheets boards use by path). The rest of the file keeps its keys, their order and its
//!   numbers as written.
//!
//! [`plan`] only reads; [`apply`] writes what it planned, each file through a temporary name and a rename.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::libtable::{LibRow, LibTable, TableKind};

pub const LIBRARY_VAR: &str = "VHRD_LIBRARY_DIR";
pub const VENDOR_VAR: &str = "VHRD_VENDOR_LIBRARY_DIR";
pub const MODELS_VAR: &str = "VHRD_3DMODEL_DIR";

/// One table row to write.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Library {
    pub name: String,
    /// Through a variable: `${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir`.
    pub uri: String,
}

/// Where setup takes the libraries and the variables from.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    /// The kicad-library checkout (`VHRD_LIBRARY_DIR`).
    pub library: PathBuf,
    /// The private vendor library (`VHRD_VENDOR_LIBRARY_DIR`), when there is one.
    pub vendor: Option<PathBuf>,
    /// The 3D models (`VHRD_3DMODEL_DIR`); left as it is when not given.
    pub models: Option<PathBuf>,
    /// A rename map whose `[libraries.symbol]` and `[libraries.footprint]` give the rows; without one the rows
    /// come from the library folders found.
    pub map: Option<PathBuf>,
}

/// The libraries in a folder, as rows through `${var}`: `symbols/*.kicad_sym`, `symbols/*.kicad_symdir` and every
/// folder of `footprints/` that holds `.kicad_mod` files (`.pretty` or not), sorted by name. A folder without
/// `symbols/` or `footprints/` has none of that kind.
pub fn libraries(root: &Path, var: &str) -> Result<(Vec<Library>, Vec<Library>), String> {
    let row = |name: &str, rel: String| Library { name: name.to_string(), uri: format!("${{{var}}}/{rel}") };
    let mut symbols = Vec::new();
    for (name, path) in entries(&root.join("symbols"))? {
        let stem = name.strip_suffix(".kicad_sym").or_else(|| name.strip_suffix(".kicad_symdir"));
        if let Some(stem) = stem.filter(|_| path.is_file() == name.ends_with(".kicad_sym")) {
            symbols.push(row(stem, format!("symbols/{name}")));
        }
    }
    let mut footprints = Vec::new();
    for (name, path) in entries(&root.join("footprints"))? {
        let holds_footprints = path.is_dir()
            && std::fs::read_dir(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?
                .filter_map(Result::ok)
                .any(|e| e.path().extension().is_some_and(|x| x == "kicad_mod"));
        if holds_footprints {
            footprints.push(row(name.strip_suffix(".pretty").unwrap_or(&name), format!("footprints/{name}")));
        }
    }
    symbols.sort();
    footprints.sort();
    Ok((symbols, footprints))
}

#[derive(serde::Deserialize)]
struct MapFile {
    libraries: MapLibraries,
}

#[derive(serde::Deserialize)]
struct MapLibraries {
    #[serde(default)]
    symbol: BTreeMap<String, String>,
    #[serde(default)]
    footprint: BTreeMap<String, String>,
}

/// The rows of a rename map: `[libraries.symbol]` and `[libraries.footprint]`, nickname = URI. The map's other
/// tables are for `kilib migrate`.
pub fn map_libraries(path: &Path) -> Result<(Vec<Library>, Vec<Library>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let map: MapFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let rows = |m: BTreeMap<String, String>| m.into_iter().map(|(name, uri)| Library { name, uri }).collect();
    Ok((rows(map.libraries.symbol), rows(map.libraries.footprint)))
}

/// The rows setup writes: from the map, or found in the checkout and the vendor library.
pub fn rows(src: &Sources) -> Result<(Vec<Library>, Vec<Library>), String> {
    if let Some(map) = &src.map {
        return map_libraries(map);
    }
    let (mut symbols, mut footprints) = libraries(&src.library, LIBRARY_VAR)?;
    if let Some(vendor) = &src.vendor {
        let (s, f) = libraries(vendor, VENDOR_VAR).unwrap_or_default();
        symbols.extend(s);
        footprints.extend(f);
        symbols.sort();
        footprints.sort();
    }
    Ok((symbols, footprints))
}

fn entries(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut out: Vec<_> = rd
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(n, _)| !n.starts_with('.'))
        .collect();
    out.sort();
    Ok(out)
}

/// What one file would become.
#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    pub path: PathBuf,
    /// `None` when the file does not exist yet.
    pub before: Option<String>,
    pub after: String,
    /// What changes, line by line, for a person to read.
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Removed(String),
    Added(String),
    Note(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Files that change; a file already right is not listed.
    pub changes: Vec<FileChange>,
    /// Files already right.
    pub unchanged: Vec<PathBuf>,
}

/// Plans the setup of `config_dir` (the versioned KiCad config folder) from `src`. Missing tables start from
/// `stock_template` (a system KiCad's `template/` folder with the stock tables) when given.
pub fn plan(src: &Sources, config_dir: &Path, stock_template: Option<&Path>) -> Result<Plan, String> {
    let canon = |p: &Path| std::fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()));
    let src = Sources {
        library: canon(&src.library)?,
        vendor: src.vendor.as_deref().map(canon).transpose()?,
        models: src.models.as_deref().map(canon).transpose()?,
        map: src.map.clone(),
    };
    let (symbols, footprints) = rows(&src)?;
    let mut ours = vec![src.library.clone()];
    ours.extend(src.vendor.clone());
    let vars = variables(&src);
    let mut plan = Plan { changes: Vec::new(), unchanged: Vec::new() };
    for (kind, libs) in [(TableKind::Symbol, &symbols), (TableKind::Footprint, &footprints)] {
        let path = config_dir.join(kind.file_name());
        match table_change(kind, &path, libs, &ours, &vars, stock_template)? {
            Some(change) => plan.changes.push(change),
            None => plan.unchanged.push(path),
        }
    }
    let path = config_dir.join("kicad_common.json");
    match common_change(&path, &vars)? {
        Some(change) => plan.changes.push(change),
        None => plan.unchanged.push(path),
    }
    Ok(plan)
}

/// Writes every planned file through a temporary name in the same folder and a rename.
pub fn apply(plan: &Plan) -> Result<(), String> {
    for change in &plan.changes {
        let tmp = PathBuf::from(format!("{}.kilib-new", change.path.display()));
        std::fs::write(&tmp, &change.after).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &change.path).map_err(|e| format!("{}: {e}", change.path.display()))?;
    }
    Ok(())
}

/// True for a row setup owns: any `vhrd_*` or `VHRD_*` row, or one on its variables or pointing into its folders.
fn ours(row: &LibRow, folders: &[PathBuf]) -> bool {
    row.name.to_ascii_lowercase().starts_with("vhrd_")
        || [LIBRARY_VAR, VENDOR_VAR].iter().any(|v| row.uri.starts_with(&format!("${{{v}}}")))
        || folders.iter().any(|f| row.uri.starts_with(&*f.to_string_lossy()))
}

fn table_change(
    kind: TableKind,
    path: &Path,
    libs: &[Library],
    folders: &[PathBuf],
    vars: &[(&'static str, String)],
    stock_template: Option<&Path>,
) -> Result<Option<FileChange>, String> {
    let mut lines = Vec::new();
    let before = match std::fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let base = match &before {
        Some(t) => t.clone(),
        None => match stock_template.map(|d| d.join(kind.file_name())).filter(|p| p.is_file()) {
            Some(stock) => {
                lines.push(Line::Note(format!("new table, from KiCad's stock {}", stock.display())));
                std::fs::read_to_string(&stock).map_err(|e| format!("{}: {e}", stock.display()))?
            }
            None => {
                lines.push(Line::Note("new table, without KiCad's stock libraries (no stock table found)".into()));
                format!("({}\n\t(version 7)\n)\n", kind.root())
            }
        },
    };
    let table = LibTable::parse(kind, path, &base).map_err(|e| e.to_string())?;
    let old: Vec<&LibRow> = table.rows.iter().filter(|r| ours(r, folders)).collect();
    // Compared as sets: KiCad may save the rows in another order.
    let mut have: Vec<(&str, String)> =
        old.iter().filter(|r| r.kind == "KiCad" && !r.disabled).map(|r| (r.name.as_str(), r.uri.clone())).collect();
    have.sort();
    let mut want: Vec<(&str, String)> = libs.iter().map(|l| (l.name.as_str(), l.uri.clone())).collect();
    want.sort();
    let same = have.len() == old.len() && have == want;
    if same && before.is_some() {
        return Ok(None);
    }

    let indent = table
        .rows
        .first()
        .map(|r| base[line_start(&base, r.span.start)..r.span.start].to_string())
        .filter(|s| s.chars().all(|c| c == ' ' || c == '\t'))
        .unwrap_or_else(|| "\t".into());
    let mut after = String::with_capacity(base.len());
    let mut cursor = 0;
    let mut insert_at = None;
    for row in &old {
        let (start, end) = whole_line(&base, row.span.start, row.span.end);
        after.push_str(&base[cursor..start]);
        insert_at.get_or_insert(after.len());
        cursor = end;
        lines.push(Line::Removed(base[row.span.clone()].to_string()));
    }
    // Without old rows, the new ones go just before the line with the table's closing parenthesis.
    let close = line_start(&base, table.end);
    let tail_from = if insert_at.is_none() && cursor <= close && base[close..table.end].trim().is_empty() {
        after.push_str(&base[cursor..close]);
        insert_at = Some(after.len());
        close
    } else {
        cursor
    };
    let mut rows = String::new();
    for lib in libs {
        let row = LibRow::render(&lib.name, "KiCad", &lib.uri, "");
        rows.push_str(&indent);
        rows.push_str(&row);
        rows.push('\n');
        lines.push(Line::Added(row));
    }
    match insert_at {
        Some(at) => {
            after.push_str(&base[tail_from..]);
            after.insert_str(at, &rows);
        }
        None => {
            // The closing parenthesis shares a line with a row: put the rows on lines of their own before it.
            after.push_str(&base[tail_from..table.end]);
            after.push('\n');
            after.push_str(&rows);
            after.push_str(&base[table.end..]);
        }
    }
    let mut expand = crate::vars::Vars::new();
    for (k, v) in vars {
        expand.set(*k, v.clone());
    }
    for lib in libs {
        match expand.expand(&lib.uri) {
            Ok(target) if !Path::new(&target).exists() => {
                lines.push(Line::Note(format!("{}: {target} does not exist (yet)", lib.name)));
            }
            Ok(_) => {}
            Err(crate::vars::Unset(names)) => lines.push(Line::Note(format!(
                "{}: this run does not set {} (--vendor-dir, or already in kicad_common.json)",
                lib.name,
                names.join(", ")
            ))),
        }
    }
    LibTable::parse(kind, path, &after).map_err(|e| format!("kilib made an unreadable table, nothing written: {e}"))?;
    Ok(Some(FileChange { path: path.to_path_buf(), before, after, lines }))
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// The span of a row widened to its whole line when nothing else shares the line.
fn whole_line(text: &str, start: usize, end: usize) -> (usize, usize) {
    let ls = line_start(text, start);
    let le = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
    if text[ls..start].trim().is_empty() && text[end..le].trim().is_empty() { (ls, le) } else { (start, end) }
}

/// The variables `setup` writes for `src`.
pub fn variables(src: &Sources) -> Vec<(&'static str, String)> {
    let show = |p: &Path| p.to_string_lossy().to_string();
    let mut vars = vec![(LIBRARY_VAR, show(&src.library))];
    if let Some(v) = &src.vendor {
        vars.push((VENDOR_VAR, show(v)));
    }
    if let Some(m) = &src.models {
        vars.push((MODELS_VAR, show(m)));
    }
    let templates = show(&src.library.join("templates"));
    vars.push(("VHRD_TEMPLATE_DIR", templates.clone()));
    vars.push(("KICAD_USER_TEMPLATE_DIR", templates));
    vars
}

fn common_change(path: &Path, variables: &[(&'static str, String)]) -> Result<Option<FileChange>, String> {
    let before = match std::fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut json: serde_json::Value = match &before {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("{}: {e}", path.display()))?,
        None => serde_json::json!({}),
    };
    let root = json.as_object_mut().ok_or_else(|| format!("{}: not a JSON object", path.display()))?;
    let env = root.entry("environment").or_insert_with(|| serde_json::json!({}));
    if !env.is_object() {
        *env = serde_json::json!({});
    }
    let vars = env.as_object_mut().expect("an object").entry("vars").or_insert(serde_json::Value::Null);
    if !vars.is_object() {
        *vars = serde_json::json!({});
    }
    let vars = vars.as_object_mut().expect("an object");
    let mut lines = Vec::new();
    for (name, value) in variables {
        let (name, value) = (*name, value.clone());
        let old = vars.get(name).and_then(|v| v.as_str()).map(str::to_string);
        if old.as_deref() == Some(value.as_str()) {
            continue;
        }
        if let Some(old) = old {
            lines.push(Line::Removed(format!("{name} = {old}")));
        }
        lines.push(Line::Added(format!("{name} = {value}")));
        vars.insert(name.to_string(), serde_json::Value::String(value));
    }
    if lines.is_empty() {
        return Ok(None);
    }
    let mut after = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    if before.as_deref().is_none_or(|b| b.ends_with('\n')) {
        after.push('\n');
    }
    Ok(Some(FileChange { path: path.to_path_buf(), before, after, lines }))
}

/// The KiCad programs that write the config, found running under `proc` (`/proc` on Linux).
pub fn kicad_running(proc: &Path) -> Vec<String> {
    const NAMES: &[&str] =
        &["kicad", "eeschema", "pcbnew", "cvpcb", "gerbview", "pl_editor", "bitmap2component", "pcb_calculator"];
    let mut found: Vec<String> = std::fs::read_dir(proc)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .map(|c| c.trim().to_string())
        .filter(|c| NAMES.contains(&c.as_str()))
        .collect();
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;

    fn src(lib: &Path) -> Sources {
        Sources { library: lib.to_path_buf(), ..Sources::default() }
    }

    /// A checkout with a symbol file, a symbol folder, a `.pretty` folder, a footprint folder without `.pretty`
    /// and an empty one (not a library).
    fn checkout(dir: &TestDir) -> PathBuf {
        dir.write("lib/symbols/vhrd_power.kicad_sym", "(kicad_symbol_lib)\n");
        dir.write("lib/symbols/vhrd_new.kicad_symdir/A.kicad_sym", "(kicad_symbol_lib)\n");
        dir.write("lib/symbols/README.md", "not a library\n");
        dir.write("lib/footprints/vhrd_generic.pretty/x.kicad_mod", "(footprint \"x\")\n");
        dir.write("lib/footprints/vhrd_connector_usb/y.kicad_mod", "(footprint \"y\")\n");
        std::fs::create_dir_all(dir.path().join("lib/footprints/empty.pretty")).unwrap();
        std::fs::canonicalize(dir.path().join("lib")).unwrap()
    }

    const SYM_TABLE: &str = "(sym_lib_table\n\t(version 7)\n\t(lib (name \"Device\") (type \"KiCad\") (uri \"${KICAD10_SYMBOL_DIR}/Device.kicad_symdir\") (options \"\") (descr \"\"))\n\t(lib (name \"vhrd_power\") (type \"KiCad\") (uri \"/home/someone/git/kicad-library/symbols/vhrd_power.kicad_sym\") (options \"\") (descr \"\"))\n\t(lib (name \"vhrd_gone\") (type \"KiCad\") (uri \"/old/vhrd_gone.kicad_sym\") (options \"\") (descr \"\"))\n\t(lib (name \"my_lib\") (type \"KiCad\") (uri \"${HOME}/my.kicad_sym\") (options \"\") (descr \"mine\"))\n)\n";

    const COMMON: &str = "{\n  \"appearance\": {\n    \"hicontrast_dimming_factor\": 0.800000011920929,\n    \"zoom_correction_factor\": 1.0\n  },\n  \"dialog\": {\n    \"controls\": {}\n  },\n  \"environment\": {\n    \"vars\": {\n      \"KICAD10_3DMODEL_DIR\": \"/stock/3d\",\n      \"VHRD_3DMODEL_DIR\": \"/old/3dmodels\"\n    }\n  },\n  \"git\": {\n    \"repositories\": []\n  },\n  \"zebra\": 1e-7\n}\n";

    #[test]
    fn finds_the_libraries_of_a_checkout() {
        let dir = TestDir::new("setup_libraries");
        let lib = checkout(&dir);
        let (sym, fp) = libraries(&lib, LIBRARY_VAR).unwrap();
        let names = |l: &[Library]| l.iter().map(|l| format!("{} {}", l.name, l.uri)).collect::<Vec<_>>();
        assert_eq!(
            names(&sym),
            vec![
                "vhrd_new ${VHRD_LIBRARY_DIR}/symbols/vhrd_new.kicad_symdir",
                "vhrd_power ${VHRD_LIBRARY_DIR}/symbols/vhrd_power.kicad_sym"
            ]
        );
        assert_eq!(
            names(&fp),
            vec![
                "vhrd_connector_usb ${VHRD_LIBRARY_DIR}/footprints/vhrd_connector_usb",
                "vhrd_generic ${VHRD_LIBRARY_DIR}/footprints/vhrd_generic.pretty"
            ]
        );
    }

    #[test]
    fn replaces_old_vhrd_rows_and_keeps_the_rest_byte_for_byte() {
        let dir = TestDir::new("setup_replace");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        dir.write("cfg/sym-lib-table", SYM_TABLE);
        let plan = plan(&src(&lib), &cfg, None).unwrap();
        let sym = plan.changes.iter().find(|c| c.path.ends_with("sym-lib-table")).unwrap();
        let expected = "(sym_lib_table\n\t(version 7)\n\t(lib (name \"Device\") (type \"KiCad\") (uri \"${KICAD10_SYMBOL_DIR}/Device.kicad_symdir\") (options \"\") (descr \"\"))\n\t(lib (name \"vhrd_new\") (type \"KiCad\") (uri \"${VHRD_LIBRARY_DIR}/symbols/vhrd_new.kicad_symdir\") (options \"\") (descr \"\"))\n\t(lib (name \"vhrd_power\") (type \"KiCad\") (uri \"${VHRD_LIBRARY_DIR}/symbols/vhrd_power.kicad_sym\") (options \"\") (descr \"\"))\n\t(lib (name \"my_lib\") (type \"KiCad\") (uri \"${HOME}/my.kicad_sym\") (options \"\") (descr \"mine\"))\n)\n";
        assert_eq!(sym.after, expected);
        let removed = sym.lines.iter().filter(|l| matches!(l, Line::Removed(_))).count();
        let added = sym.lines.iter().filter(|l| matches!(l, Line::Added(_))).count();
        assert_eq!((removed, added), (2, 2));
        assert_eq!(sym.before.as_deref(), Some(SYM_TABLE));
        // Planning wrote nothing.
        assert_eq!(std::fs::read_to_string(cfg.join("sym-lib-table")).unwrap(), SYM_TABLE);
        assert!(!cfg.join("fp-lib-table").exists());
    }

    #[test]
    fn apply_then_plan_again_changes_nothing() {
        let dir = TestDir::new("setup_idempotent");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        dir.write("cfg/sym-lib-table", SYM_TABLE);
        dir.write("cfg/kicad_common.json", COMMON);
        let first = plan(&src(&lib), &cfg, None).unwrap();
        assert_eq!(first.changes.len(), 3);
        apply(&first).unwrap();
        let second = plan(&src(&lib), &cfg, None).unwrap();
        assert_eq!(second.changes, vec![], "{:#?}", second.changes);
        assert_eq!(second.unchanged.len(), 3);
        // No temporary files left behind.
        let mut names: Vec<String> =
            std::fs::read_dir(&cfg).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, vec!["fp-lib-table", "kicad_common.json", "sym-lib-table"]);
        // The rows written are read back by the checker's table reader.
        let t = LibTable::read(TableKind::Footprint, &cfg.join("fp-lib-table")).unwrap();
        assert_eq!(t.find("vhrd_generic").unwrap().uri, "${VHRD_LIBRARY_DIR}/footprints/vhrd_generic.pretty");
    }

    #[test]
    fn rows_kicad_reordered_are_still_set_up() {
        let dir = TestDir::new("setup_order");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        let (sym, _) = libraries(&lib, LIBRARY_VAR).unwrap();
        let rows: String =
            sym.iter().rev().map(|l| format!("  {}\n", LibRow::render(&l.name, "KiCad", &l.uri, ""))).collect();
        dir.write("cfg/sym-lib-table", &format!("(sym_lib_table\n  (version 7)\n{rows})\n"));
        let plan = plan(&src(&lib), &cfg, None).unwrap();
        assert!(plan.unchanged.iter().any(|p| p.ends_with("sym-lib-table")));
    }

    #[test]
    fn kicad_common_keeps_keys_order_and_numbers() {
        let dir = TestDir::new("setup_common");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        dir.write("cfg/kicad_common.json", COMMON);
        let plan = plan(&src(&lib), &cfg, None).unwrap();
        let c = plan.changes.iter().find(|c| c.path.ends_with("kicad_common.json")).unwrap();
        let l = lib.display();
        let expected = COMMON.replace(
            "      \"VHRD_3DMODEL_DIR\": \"/old/3dmodels\"\n",
            &format!("      \"VHRD_3DMODEL_DIR\": \"/old/3dmodels\",\n      \"VHRD_LIBRARY_DIR\": \"{l}\",\n      \"VHRD_TEMPLATE_DIR\": \"{l}/templates\",\n      \"KICAD_USER_TEMPLATE_DIR\": \"{l}/templates\"\n"),
        );
        assert_eq!(c.after, expected, "VHRD_3DMODEL_DIR stays without --models-dir");

        // With the models and the vendor library given, both variables are set.
        let models = dir.path().join("models");
        let vendor = dir.path().join("vendor");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::create_dir_all(&vendor).unwrap();
        let s = Sources { models: Some(models.clone()), vendor: Some(vendor.clone()), ..src(&lib) };
        let plan = super::plan(&s, &cfg, None).unwrap();
        let c = plan.changes.iter().find(|c| c.path.ends_with("kicad_common.json")).unwrap();
        let models = std::fs::canonicalize(models).unwrap();
        assert!(c.lines.contains(&Line::Removed("VHRD_3DMODEL_DIR = /old/3dmodels".into())));
        assert!(c.lines.contains(&Line::Added(format!("VHRD_3DMODEL_DIR = {}", models.display()))));
        let vendor = std::fs::canonicalize(vendor).unwrap();
        assert!(c.lines.contains(&Line::Added(format!("VHRD_VENDOR_LIBRARY_DIR = {}", vendor.display()))));
    }

    #[test]
    fn a_fresh_config_starts_from_the_stock_tables() {
        let dir = TestDir::new("setup_fresh");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        dir.write(
            "stock/sym-lib-table",
            "(sym_lib_table\n  (version 7)\n  (lib (name \"Device\")(type \"KiCad\")(uri \"${KICAD10_SYMBOL_DIR}/Device.kicad_symdir\")(options \"\")(descr \"\"))\n)\n",
        );
        let plan = plan(&src(&lib), &cfg, Some(&dir.path().join("stock"))).unwrap();
        let sym = plan.changes.iter().find(|c| c.path.ends_with("sym-lib-table")).unwrap();
        assert_eq!(sym.before, None);
        assert!(sym.after.contains("(name \"Device\")") && sym.after.contains("(name \"vhrd_power\")"));
        let fp = plan.changes.iter().find(|c| c.path.ends_with("fp-lib-table")).unwrap();
        assert!(matches!(&fp.lines[0], Line::Note(n) if n.contains("without KiCad's stock libraries")));
        assert!(fp.after.starts_with("(fp_lib_table\n\t(version 7)\n\t(lib (name \"vhrd_connector_usb\")"));
        let common = plan.changes.iter().find(|c| c.path.ends_with("kicad_common.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&common.after).unwrap();
        assert_eq!(json["environment"]["vars"]["VHRD_LIBRARY_DIR"], lib.display().to_string());
    }

    #[test]
    fn a_closing_parenthesis_on_a_row_line() {
        let dir = TestDir::new("setup_close");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        dir.write(
            "cfg/fp-lib-table",
            "(fp_lib_table (version 7) (lib (name \"A\")(type \"KiCad\")(uri \"/a\")(options \"\")(descr \"\")))",
        );
        let plan = plan(&src(&lib), &cfg, None).unwrap();
        let fp = plan.changes.iter().find(|c| c.path.ends_with("fp-lib-table")).unwrap();
        let t = LibTable::parse(TableKind::Footprint, &fp.path, &fp.after).unwrap();
        let names: Vec<&str> = t.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["A", "vhrd_connector_usb", "vhrd_generic"]);
    }

    #[test]
    fn finds_running_kicad_programs() {
        let dir = TestDir::new("setup_proc");
        dir.write("proc/1/comm", "systemd\n");
        dir.write("proc/200/comm", "pcbnew\n");
        dir.write("proc/201/comm", "kicad\n");
        dir.write("proc/202/comm", "pcbnew\n");
        dir.write("proc/203/comm", "kicad-cli\n");
        dir.write("proc/self/comm", "kicad\n");
        assert_eq!(kicad_running(&dir.path().join("proc")), vec!["kicad", "pcbnew"]);
        assert_eq!(kicad_running(&dir.path().join("nothing")), Vec::<String>::new());
    }

    const MAP: &str = r#"# a rename map
[libraries]
[libraries.footprint]
"VHRD_Connector" = "${VHRD_LIBRARY_DIR}/footprints/VHRD_Connector.pretty"
"VHRD_Vendor" = "${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty"
[libraries.symbol]
"VHRD_Device" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir"

[symbol_lib]
"vhrd_analog" = "VHRD_Analog_Switch"
[footprint]
"vhrd_generic:x" = "VHRD_Connector:x"
[model]
"sot/SOT-23.step" = "own/VHRD_Package_TO_SOT_SMD.3dshapes/SOT-23.step"
"#;

    #[test]
    fn rows_from_a_rename_map_replace_the_old_ones() {
        let dir = TestDir::new("setup_map");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        dir.write("cfg/sym-lib-table", SYM_TABLE);
        let map = dir.write("map.toml", MAP);
        dir.write("lib/footprints/VHRD_Connector.pretty/c.kicad_mod", "(footprint \"c\")\n");
        let s = Sources { map: Some(map.clone()), ..src(&lib) };
        let plan = plan(&s, &cfg, None).unwrap();
        let sym = plan.changes.iter().find(|c| c.path.ends_with("sym-lib-table")).unwrap();
        let t = LibTable::parse(TableKind::Symbol, &sym.path, &sym.after).unwrap();
        let names: Vec<&str> = t.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Device", "VHRD_Device", "my_lib"], "old vhrd_* rows out, the map's in");
        let fp = plan.changes.iter().find(|c| c.path.ends_with("fp-lib-table")).unwrap();
        let t = LibTable::parse(TableKind::Footprint, &fp.path, &fp.after).unwrap();
        let rows: Vec<(&str, &str)> = t.rows.iter().map(|r| (r.name.as_str(), r.uri.as_str())).collect();
        assert_eq!(
            rows,
            vec![
                ("VHRD_Connector", "${VHRD_LIBRARY_DIR}/footprints/VHRD_Connector.pretty"),
                ("VHRD_Vendor", "${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty"),
            ]
        );
        // No vendor library given: VHRD_Vendor's variable is unset; VHRD_Device's folder does not exist yet.
        assert!(sym.lines.iter().any(
            |l| matches!(l, Line::Note(n) if n.starts_with("VHRD_Device: ") && n.ends_with("does not exist (yet)"))
        ));
        assert!(!fp.lines.iter().any(|l| matches!(l, Line::Note(n) if n.starts_with("VHRD_Connector"))));
        assert!(fp.lines.iter().any(|l| matches!(l, Line::Note(n) if n.starts_with("VHRD_Vendor: this run does not set VHRD_VENDOR_LIBRARY_DIR"))));

        // Once applied, the found-folder rows of the old layout replace the map's VHRD_* rows again.
        apply(&plan).unwrap();
        let again = super::plan(&src(&lib), &cfg, None).unwrap();
        let sym = again.changes.iter().find(|c| c.path.ends_with("sym-lib-table")).unwrap();
        assert!(sym.lines.contains(&Line::Removed(LibRow::render(
            "VHRD_Device",
            "KiCad",
            "${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir",
            ""
        ))));
    }

    #[test]
    fn a_vendor_library_adds_its_folders() {
        let dir = TestDir::new("setup_vendor");
        let lib = checkout(&dir);
        dir.write("vendor/footprints/VHRD_Vendor.pretty/v.kicad_mod", "(footprint \"v\")\n");
        dir.write("vendor/symbols/VHRD_Vendor.kicad_symdir/V.kicad_sym", "(kicad_symbol_lib)\n");
        let s = Sources { vendor: Some(dir.path().join("vendor")), ..src(&lib) };
        let (sym, fp) = rows(&s).unwrap();
        let fp: Vec<String> = fp.iter().map(|l| format!("{} {}", l.name, l.uri)).collect();
        assert_eq!(
            fp,
            vec![
                "VHRD_Vendor ${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty",
                "vhrd_connector_usb ${VHRD_LIBRARY_DIR}/footprints/vhrd_connector_usb",
                "vhrd_generic ${VHRD_LIBRARY_DIR}/footprints/vhrd_generic.pretty",
            ]
        );
        assert!(sym.iter().any(|l| l.uri == "${VHRD_VENDOR_LIBRARY_DIR}/symbols/VHRD_Vendor.kicad_symdir"));
    }

    #[test]
    fn a_bad_map_is_an_error() {
        let dir = TestDir::new("setup_bad_map");
        let lib = checkout(&dir);
        let map = dir.write("map.toml", "[symbol_lib]\n\"a\" = \"b\"\n");
        let s = Sources { map: Some(map), ..src(&lib) };
        let e = plan(&s, &dir.path().join("cfg"), None).unwrap_err();
        assert!(e.contains("libraries"), "{e}");
    }
}
