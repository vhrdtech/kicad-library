//! The extended set: `kilib migrate` with the real map on a temporary copy of the boards (`VHRD_BOARDS_DIR`, else
//! the `hw` checkout beside this one), and the migrated boards checked against a restructured copy of this
//! library. The boards are never written; without them each test skips with a note.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kilib::check::{self, Kind};
use kilib::libtable::{LibRow, LibTable, TableKind};
use kilib::map::{ItemKind, Map};
use kilib::migrate;
use kilib::restructure;
use kilib::sexpr;
use kilib::testdir::TestDir;
use kilib::vars::Vars;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn real_map() -> Map {
    Map::read(&repo().join("migrate/2026-11.toml")).unwrap()
}

fn boards() -> Option<PathBuf> {
    let dir = std::env::var_os("VHRD_BOARDS_DIR").map(PathBuf::from).unwrap_or_else(|| repo().join("../hw"));
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipped: no boards at {} (set VHRD_BOARDS_DIR to a checkout of the boards)", dir.display());
        None
    }
}

/// Copies what migrate and check read (KiCad files, tables, projects), not .git or backups.
fn copy_boards(from: &Path, to: &Path) -> usize {
    let mut n = 0;
    for entry in std::fs::read_dir(from).unwrap().filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        let (src, dst) = (entry.path(), to.join(&name));
        let ft = entry.file_type().unwrap();
        if ft.is_dir() {
            if !name.starts_with('.') && !name.ends_with("-backups") && name != "target" {
                n += copy_boards(&src, &dst);
            }
        } else if ft.is_file()
            && (name.ends_with("lib-table")
                || [".kicad_sch", ".kicad_pcb", ".kicad_sym", ".kicad_mod", ".kicad_pro"]
                    .iter()
                    .any(|e| name.ends_with(e)))
        {
            std::fs::create_dir_all(to).unwrap();
            std::fs::copy(&src, &dst).unwrap();
            n += 1;
        }
    }
    n
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap().filter_map(Result::ok) {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::create_dir_all(to).unwrap();
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

#[test]
fn migrate_on_a_copy_of_the_boards_changes_only_mapped_strings() {
    let Some(hw) = boards() else { return };
    let dir = TestDir::new("boards_migrate");
    let copy = dir.path().join("hw");
    assert!(copy_boards(&hw, &copy) > 0);
    let map = real_map();
    let plan = migrate::plan(&map, std::slice::from_ref(&copy));
    let errors: Vec<_> = plan.notes.iter().filter(|n| n.severity == migrate::Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(!plan.changes.is_empty(), "the boards use old libraries");
    for c in &plan.changes {
        common::assert_only_mapped(&map, &c.path, &c.old, &c.new);
    }
    migrate::apply(&plan).unwrap();
    let again = migrate::plan(&map, std::slice::from_ref(&copy));
    assert!(again.changes.is_empty(), "a second run changes {} files", again.changes.len());
}

/// Every symbol and footprint name of the library as it is today, old layout or new.
fn library_names(lib: &Path) -> BTreeSet<(ItemKind, String)> {
    let mut out = BTreeSet::new();
    let mut sym_file = |p: &Path| {
        let text = std::fs::read_to_string(p).unwrap();
        for s in sexpr::parse(&text).unwrap().children("symbol") {
            out.insert((ItemKind::Symbol, s.arg(0).unwrap().to_string()));
        }
    };
    for e in std::fs::read_dir(lib.join("symbols")).unwrap().filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            for f in std::fs::read_dir(&p).unwrap().filter_map(Result::ok) {
                sym_file(&f.path());
            }
        } else if p.extension().is_some_and(|x| x == "kicad_sym") {
            sym_file(&p);
        }
    }
    for e in std::fs::read_dir(lib.join("footprints")).unwrap().filter_map(Result::ok) {
        for f in std::fs::read_dir(e.path()).into_iter().flatten().filter_map(Result::ok) {
            let p = f.path();
            if p.extension().is_some_and(|x| x == "kicad_mod") {
                out.insert((ItemKind::Footprint, p.file_stem().unwrap().to_string_lossy().to_string()));
            }
        }
    }
    out
}

#[test]
fn migrated_boards_resolve_against_the_restructured_library() {
    let Some(hw) = boards() else { return };
    let dir = TestDir::new("boards_resolve");
    let map = real_map();
    let before = library_names(&repo());
    let (lib, vendor) = (dir.path().join("lib"), dir.path().join("vendor"));
    copy_tree(&repo().join("symbols"), &lib.join("symbols"));
    copy_tree(&repo().join("footprints"), &lib.join("footprints"));
    let plan = restructure::plan(&map, &lib, &vendor);
    assert!(!plan.has_errors(), "{:#?}", plan.notes);
    restructure::apply(&plan).unwrap();

    let copy = dir.path().join("hw");
    copy_boards(&hw, &copy);
    migrate::apply(&migrate::plan(&map, std::slice::from_ref(&copy))).unwrap();

    let table = |kind: TableKind, item: ItemKind| {
        let rows: String =
            map.libraries(item).iter().map(|(n, u)| format!("  {}\n", LibRow::render(n, "KiCad", u, ""))).collect();
        let text = format!("({}\n  (version 7)\n{rows})\n", kind.root());
        LibTable::parse(kind, Path::new(kind.file_name()), &text).unwrap()
    };
    let mut vars = Vars::new();
    vars.set("VHRD_LIBRARY_DIR", lib.to_string_lossy());
    vars.set("VHRD_VENDOR_LIBRARY_DIR", vendor.to_string_lossy());
    let setup = check::Setup {
        global_sym: Some(table(TableKind::Symbol, ItemKind::Symbol)),
        global_fp: Some(table(TableKind::Footprint, ItemKind::Footprint)),
        vars,
    };
    let report = check::check(&[copy], &setup);
    // What a board names in a new library and the library lacks must have been missing before the move too.
    let mut lost = Vec::new();
    for f in &report.problems {
        let kind = match f.kind {
            Kind::Symbol => ItemKind::Symbol,
            Kind::Footprint | Kind::FootprintField => ItemKind::Footprint,
            _ => continue,
        };
        let Some((lib, name)) = f.id.split_once(':') else { continue };
        if map.libraries(kind).contains_key(lib) && before.contains(&(kind, name.to_string())) {
            lost.push(format!("{} {}: {}", kind.label(), f.id, f.reason));
        }
    }
    assert!(lost.is_empty(), "in the library before the move, not found after it:\n{}", lost.join("\n"));
}
