//! The maps in `migrate/` fit the library as it is: every item they send into this checkout is there, every
//! library folder of the checkout has its row in `2026-11.toml`'s `[libraries]` (that row is what `kilib setup`
//! writes into KiCad), and the other maps' rows say the same. The library grows after the move, so an item need
//! not be a target of a map.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kilib::map::{ItemKind, Map};
use kilib::sexpr;

const OWN: &str = "${VHRD_LIBRARY_DIR}/";

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

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e == ext)
}

/// `lib:name` of every symbol and footprint in the checkout, and its library folders, each with its kind.
type Named = BTreeSet<(ItemKind, String)>;
fn library(repo: &Path) -> (Named, Named) {
    let (mut items, mut libs) = (BTreeSet::new(), BTreeSet::new());
    for dir in entries(&repo.join("symbols")).into_iter().filter(|p| has_ext(p, "kicad_symdir")) {
        let lib = stem(&dir);
        for f in entries(&dir) {
            for name in symbol_names(&f) {
                items.insert((ItemKind::Symbol, format!("{lib}:{name}")));
            }
        }
        libs.insert((ItemKind::Symbol, lib));
    }
    for dir in entries(&repo.join("footprints")).into_iter().filter(|p| has_ext(p, "pretty")) {
        let lib = stem(&dir);
        for f in entries(&dir).into_iter().filter(|f| has_ext(f, "kicad_mod")) {
            items.insert((ItemKind::Footprint, format!("{lib}:{}", stem(&f))));
        }
        libs.insert((ItemKind::Footprint, lib));
    }
    (items, libs)
}

fn maps(repo: &Path) -> Vec<(PathBuf, Map)> {
    let maps: Vec<(PathBuf, Map)> = entries(&repo.join("migrate"))
        .into_iter()
        .filter(|p| has_ext(p, "toml"))
        .map(|p| {
            let m = Map::read(&p).unwrap();
            (p, m)
        })
        .collect();
    assert!(maps.len() >= 2, "migrate/ holds 2026-11.toml and 2026-11-stock.toml");
    maps
}

#[test]
fn every_map_target_is_in_the_library_and_every_library_has_a_row() {
    let repo = repo();
    let setup = Map::read(&repo.join("migrate/2026-11.toml")).unwrap();
    let (items, libs) = library(&repo);
    assert!(!items.is_empty(), "nothing found under {}", repo.display());
    let mut wrong = Vec::new();

    for (path, map) in maps(&repo) {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        for (kind, targets) in [(ItemKind::Symbol, &map.symbol), (ItemKind::Footprint, &map.footprint)] {
            let rows = map.libraries(kind);
            for (key, lib) in targets {
                // "" is dropped; a library outside this checkout is not checked here
                if lib.is_empty() || !rows.get(lib).is_some_and(|uri| uri.starts_with(OWN)) {
                    continue;
                }
                let name = key.split_once(':').map_or(key.as_str(), |(_, n)| n);
                if !items.contains(&(kind, format!("{lib}:{name}"))) {
                    wrong.push(format!("{file}: {} {key} = {lib:?}: no {lib}:{name} in the library", kind.label()));
                }
            }
            for (lib, uri) in rows {
                if setup.libraries(kind).get(lib) != Some(uri) {
                    wrong.push(format!("{file}: {} library {lib} = {uri:?} differs from 2026-11.toml", kind.label()));
                }
            }
        }
    }
    for (kind, lib) in &libs {
        if !setup.libraries(*kind).contains_key(lib) {
            wrong.push(format!("{} library {lib} has no row in [libraries]", kind.label()));
        }
    }
    assert!(wrong.is_empty(), "the maps do not fit the library:\n{}", wrong.join("\n"));
}
