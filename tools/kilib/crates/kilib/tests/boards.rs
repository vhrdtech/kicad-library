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
    // boards that are already migrated have nothing to change, and that is fine
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

/// Every board's designators fitted (`fab-designators --board`) and the user layers named (`kilib layers`) on a
/// temporary copy: both parse every board, change only what they own (the Reference's at, size and thickness; the
/// layer table) and change nothing on a second run.
#[test]
fn board_designators_and_layers_on_a_copy_of_the_boards() {
    let Some(from) = boards() else { return };
    let dir = TestDir::new("boards_fab");
    copy_boards(&from, dir.path());
    let mut pcbs = Vec::new();
    collect_pcbs(dir.path(), &mut pcbs);
    assert!(!pcbs.is_empty(), "no boards in {}", from.display());
    let owned = |l: &str| ["(at ", "(size ", "(thickness "].iter().any(|k| l.trim_start().starts_with(k));
    for pcb in &pcbs {
        let old = std::fs::read_to_string(pcb).unwrap();
        let plan = kilib::fab::plan_board(&old).unwrap_or_else(|e| panic!("{}: {e}", pcb.display()));
        assert_eq!(plan.new.lines().count(), old.lines().count(), "{}", pcb.display());
        for (a, b) in old.lines().zip(plan.new.lines()) {
            assert!(a == b || (owned(a) && owned(b)), "{}: {a:?} -> {b:?}", pcb.display());
        }
        kilib::fab::apply_board(pcb, &plan).unwrap();
        let again = kilib::fab::plan_board(&plan.new).unwrap();
        assert_eq!(again.new, plan.new, "{}: a second run changes nothing", pcb.display());

        // A stub board without a layer table has nothing to name.
        let Ok((named, _)) = kilib::layers::add_to_board(&plan.new) else { continue };
        let (twice, _) = kilib::layers::add_to_board(&named).unwrap();
        assert_eq!(twice, named, "{}", pcb.display());
        assert!(sexpr::parse(&named).is_ok(), "{}", pcb.display());
    }
}

fn collect_pcbs(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            collect_pcbs(&p, out);
        } else if p.extension().is_some_and(|x| x == "kicad_pcb") {
            out.push(p);
        }
    }
}
