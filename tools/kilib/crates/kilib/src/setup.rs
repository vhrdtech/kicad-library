//! `kilib setup`: points this PC's KiCad at a kicad-library checkout.
//!
//! - The global `sym-lib-table` and `fp-lib-table` get one row per library in the checkout, through
//!   `${VHRD_LIBRARY_DIR}`, so the tables hold no absolute path. The rows it replaces are the old `vhrd_*` rows and
//!   any row already pointing into the checkout or at `${VHRD_LIBRARY_DIR}`; every other row stays byte for byte.
//! - `kicad_common.json` gets the path variables (`VHRD_LIBRARY_DIR`, `VHRD_3DMODEL_DIR`, `VHRD_TEMPLATE_DIR`,
//!   `KICAD_USER_TEMPLATE_DIR`); the rest of the file keeps its keys, their order and its numbers as written.
//!
//! [`plan`] only reads; [`apply`] writes what it planned, each file through a temporary name and a rename.

use std::path::{Path, PathBuf};

use crate::libtable::{LibRow, LibTable, TableKind};

pub const LIBRARY_VAR: &str = "VHRD_LIBRARY_DIR";

/// A library of the checkout, as a table row.
#[derive(Debug, Clone, PartialEq)]
pub struct Library {
    pub name: String,
    /// Relative to the checkout: `symbols/vhrd_analog.kicad_sym`.
    pub rel: String,
}

impl Library {
    pub fn uri(&self) -> String {
        format!("${{{LIBRARY_VAR}}}/{}", self.rel)
    }
}

/// The libraries in a checkout: `symbols/*.kicad_sym`, `symbols/*.kicad_symdir` and every folder of
/// `footprints/` that holds `.kicad_mod` files (`.pretty` or not), sorted by name.
pub fn libraries(root: &Path) -> Result<(Vec<Library>, Vec<Library>), String> {
    let mut symbols = Vec::new();
    for (name, path) in entries(&root.join("symbols"))? {
        let stem = name.strip_suffix(".kicad_sym").or_else(|| name.strip_suffix(".kicad_symdir"));
        if let Some(stem) = stem.filter(|_| path.is_file() == name.ends_with(".kicad_sym")) {
            symbols.push(Library { name: stem.to_string(), rel: format!("symbols/{name}") });
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
            let stem = name.strip_suffix(".pretty").unwrap_or(&name);
            footprints.push(Library { name: stem.to_string(), rel: format!("footprints/{name}") });
        }
    }
    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    footprints.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((symbols, footprints))
}

fn entries(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
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

/// Plans the setup of `config_dir` (the versioned KiCad config folder) for the checkout at `library`. Missing
/// tables start from `stock_template` (a system KiCad's `template/` folder with the stock tables) when given.
pub fn plan(library: &Path, config_dir: &Path, stock_template: Option<&Path>) -> Result<Plan, String> {
    let library = std::fs::canonicalize(library).map_err(|e| format!("{}: {e}", library.display()))?;
    let (symbols, footprints) = libraries(&library)?;
    let mut plan = Plan { changes: Vec::new(), unchanged: Vec::new() };
    for (kind, libs) in [(TableKind::Symbol, &symbols), (TableKind::Footprint, &footprints)] {
        let path = config_dir.join(kind.file_name());
        match table_change(kind, &path, libs, &library, stock_template)? {
            Some(change) => plan.changes.push(change),
            None => plan.unchanged.push(path),
        }
    }
    let path = config_dir.join("kicad_common.json");
    match common_change(&path, &library)? {
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

/// True for a row that belongs to the checkout: an old `vhrd_*` row, or one pointing at it.
fn ours(row: &LibRow, library: &Path) -> bool {
    row.name.starts_with("vhrd_")
        || row.uri.starts_with(&format!("${{{LIBRARY_VAR}}}"))
        || row.uri.starts_with(&*library.to_string_lossy())
}

fn table_change(
    kind: TableKind,
    path: &Path,
    libs: &[Library],
    library: &Path,
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
    let old: Vec<&LibRow> = table.rows.iter().filter(|r| ours(r, library)).collect();
    // Compared as sets: KiCad may save the rows in another order.
    let mut have: Vec<(&str, String)> =
        old.iter().filter(|r| r.kind == "KiCad" && !r.disabled).map(|r| (r.name.as_str(), r.uri.clone())).collect();
    have.sort();
    let want: Vec<(&str, String)> = libs.iter().map(|l| (l.name.as_str(), l.uri())).collect();
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
        let row = LibRow::render(&lib.name, "KiCad", &lib.uri(), "");
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

/// The variables `setup` writes, for a checkout at `library`.
pub fn variables(library: &Path) -> Vec<(&'static str, String)> {
    let at = |rel: &str| library.join(rel).to_string_lossy().to_string();
    vec![
        (LIBRARY_VAR, library.to_string_lossy().to_string()),
        ("VHRD_3DMODEL_DIR", at("3dmodels")),
        ("VHRD_TEMPLATE_DIR", at("templates")),
        ("KICAD_USER_TEMPLATE_DIR", at("templates")),
    ]
}

fn common_change(path: &Path, library: &Path) -> Result<Option<FileChange>, String> {
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
    for (name, value) in variables(library) {
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
        let (sym, fp) = libraries(&lib).unwrap();
        let names = |l: &[Library]| l.iter().map(|l| format!("{} {}", l.name, l.uri())).collect::<Vec<_>>();
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
        let plan = plan(&lib, &cfg, None).unwrap();
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
        let first = plan(&lib, &cfg, None).unwrap();
        assert_eq!(first.changes.len(), 3);
        apply(&first).unwrap();
        let second = plan(&lib, &cfg, None).unwrap();
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
        let (sym, _) = libraries(&lib).unwrap();
        let rows: String =
            sym.iter().rev().map(|l| format!("  {}\n", LibRow::render(&l.name, "KiCad", &l.uri(), ""))).collect();
        dir.write("cfg/sym-lib-table", &format!("(sym_lib_table\n  (version 7)\n{rows})\n"));
        let plan = plan(&lib, &cfg, None).unwrap();
        assert!(plan.unchanged.iter().any(|p| p.ends_with("sym-lib-table")));
    }

    #[test]
    fn kicad_common_keeps_keys_order_and_numbers() {
        let dir = TestDir::new("setup_common");
        let lib = checkout(&dir);
        let cfg = dir.path().join("cfg");
        dir.write("cfg/kicad_common.json", COMMON);
        let plan = plan(&lib, &cfg, None).unwrap();
        let c = plan.changes.iter().find(|c| c.path.ends_with("kicad_common.json")).unwrap();
        let l = lib.display();
        let expected = COMMON.replace(
            "      \"VHRD_3DMODEL_DIR\": \"/old/3dmodels\"\n",
            &format!("      \"VHRD_3DMODEL_DIR\": \"{l}/3dmodels\",\n      \"VHRD_LIBRARY_DIR\": \"{l}\",\n      \"VHRD_TEMPLATE_DIR\": \"{l}/templates\",\n      \"KICAD_USER_TEMPLATE_DIR\": \"{l}/templates\"\n"),
        );
        assert_eq!(c.after, expected);
        assert!(c.lines.contains(&Line::Removed("VHRD_3DMODEL_DIR = /old/3dmodels".into())));
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
        let plan = plan(&lib, &cfg, Some(&dir.path().join("stock"))).unwrap();
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
        let plan = plan(&lib, &cfg, None).unwrap();
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
}
