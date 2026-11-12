//! The library stands on its own: every symbol's `Footprint` field and every `ki_fp_filters` library is a `VHRD_*`
//! library of this checkout, and resolves there (`kilib check --only VHRD_` with the
//! rows of `migrate/2026-11.toml`). KiCad's stock libraries are never needed; a stock part comes in through
//! `kilib stock import`. 3D models live outside the repo and are not checked here.

use std::path::{Path, PathBuf};

use kilib::check::{self, Kind};
use kilib::libtable::{LibRow, LibTable, TableKind};
use kilib::map::{ItemKind, Map};
use kilib::vars::Vars;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

#[test]
fn the_library_names_only_its_own_libraries() {
    let repo = std::path::absolute(repo()).unwrap();
    let map = Map::read(&repo.join("migrate/2026-11.toml")).unwrap();
    let table = |kind: TableKind, item: ItemKind| {
        let rows: String =
            map.libraries(item).iter().map(|(n, u)| format!("  {}\n", LibRow::render(n, "KiCad", u, ""))).collect();
        let text = format!("({}\n  (version 7)\n{rows})\n", kind.root());
        LibTable::parse(kind, Path::new(kind.file_name()), &text).unwrap()
    };
    let mut vars = Vars::new();
    vars.set("VHRD_LIBRARY_DIR", repo.to_string_lossy());
    let setup = check::Setup {
        global_sym: Some(table(TableKind::Symbol, ItemKind::Symbol)),
        global_fp: Some(table(TableKind::Footprint, ItemKind::Footprint)),
        vars,
        only: Some("VHRD_".into()),
    };
    let paths = ["symbols", "footprints"].map(|p| repo.join(p));
    let report = check::check(&paths, &setup);
    let problems: Vec<String> = report
        .problems
        .iter()
        .filter(|f| f.kind != Kind::Model)
        .map(|f| {
            let files: Vec<String> =
                f.uses.iter().map(|(p, _)| p.strip_prefix(&repo).unwrap_or(p).display().to_string()).collect();
            format!("{} {}: {} ({})", f.kind.label(), f.id, f.reason, files.join(", "))
        })
        .collect();
    assert!(report.checked.footprint_fields > 100, "{:?}", report.checked);
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}
