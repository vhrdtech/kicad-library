//! The map in `migrate/` covers the library as it is: every symbol, footprint and 3D model of the old layout has
//! its own line, and every item of a new library is a target of one.

use std::path::{Path, PathBuf};

use kilib::map::{ItemKind, Map};
use kilib::sexpr;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> =
        std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
    out.sort();
    out
}

fn symbol_names(file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap();
    let root = sexpr::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    root.children("symbol").filter_map(|s| s.arg(0)).map(str::to_string).collect()
}

fn stem(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().to_string()
}

#[test]
fn the_map_covers_every_symbol_footprint_and_model() {
    let repo = repo();
    let map = Map::read(&repo.join("migrate/2026-11.toml")).unwrap();
    let is_target = |kind: ItemKind, lib: &str, name: &str| {
        let items = if kind == ItemKind::Symbol { &map.symbol } else { &map.footprint };
        items.iter().any(|(k, v)| v == lib && k.split_once(':').is_some_and(|(_, n)| n == name))
    };
    let mut missing = Vec::new();
    let mut seen = 0;

    for p in entries(&repo.join("symbols")) {
        let lib = stem(&p);
        if p.is_file() && p.extension().is_some_and(|e| e == "kicad_sym") {
            for name in symbol_names(&p) {
                seen += 1;
                if !map.symbol.contains_key(&format!("{lib}:{name}")) {
                    missing.push(format!("symbol {lib}:{name}"));
                }
            }
        } else if p.extension().is_some_and(|e| e == "kicad_symdir") {
            for f in entries(&p) {
                for name in symbol_names(&f) {
                    seen += 1;
                    if !is_target(ItemKind::Symbol, &lib, &name) {
                        missing.push(format!("symbol {lib}:{name} (new layout) is no target of [symbol]"));
                    }
                }
            }
        }
    }
    for dir in entries(&repo.join("footprints")).into_iter().filter(|d| d.is_dir()) {
        let dir_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let lib = dir_name.strip_suffix(".pretty").unwrap_or(&dir_name).to_string();
        let new = map.libraries(ItemKind::Footprint).contains_key(&lib);
        for f in entries(&dir).into_iter().filter(|f| f.extension().is_some_and(|e| e == "kicad_mod")) {
            let name = stem(&f);
            seen += 1;
            if new {
                if !is_target(ItemKind::Footprint, &lib, &name) {
                    missing.push(format!("footprint {lib}:{name} (new layout) is no target of [footprint]"));
                }
            } else if !map.footprint.contains_key(&format!("{lib}:{name}")) {
                missing.push(format!("footprint {lib}:{name}"));
            }
        }
    }
    // The models, while they are in the repo.
    fn files(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for p in entries(dir) {
            if p.is_dir() {
                files(&p, base, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut models = Vec::new();
    files(&repo.join("3dmodels"), &repo.join("3dmodels"), &mut models);
    for m in &models {
        seen += 1;
        if map.model(m).is_none() {
            missing.push(format!("3D model {m}"));
        }
    }
    assert!(seen > 0, "nothing found under {}", repo.display());
    assert!(missing.is_empty(), "not in the map:\n{}", missing.join("\n"));
}
