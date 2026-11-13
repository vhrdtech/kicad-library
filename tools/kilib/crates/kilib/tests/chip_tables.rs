//! The checkout's chip tables (`generate/*.toml`) against its footprints and `migrate/2026-11-packages.toml`: every
//! size and height has its footprint, pointing at its generated model, and a per-package symbol in the map.

use std::path::{Path, PathBuf};

use kilib::chip::Tables;
use kilib::map::Map;

fn repo() -> PathBuf {
    std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")).unwrap()
}

#[test]
fn every_part_has_its_footprint_and_symbol() {
    let repo = repo();
    let parts = Tables::load(&repo).unwrap().parts().unwrap();
    let map = Map::read(&repo.join("migrate/2026-11-packages.toml")).unwrap();
    let mut problems = Vec::new();
    let mut missing_lines: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for part in &parts {
        let path =
            repo.join("footprints").join(format!("{}.pretty", part.library())).join(format!("{}.kicad_mod", part.name));
        match std::fs::read_to_string(&path) {
            Err(_) => problems.push(format!("{}: no footprint (kilib footprint chip --write)", part.footprint_id())),
            Ok(text) if !text.contains(&format!("(model \"{}\"", part.model_ref())) => {
                problems.push(format!("{}: model is not {}", part.footprint_id(), part.model_ref()))
            }
            Ok(_) => {}
        }
        let (generic, symbol) = part.symbols();
        if map.per_package.get(&generic).and_then(|m| m.get(&part.footprint_id())) != Some(&symbol) {
            missing_lines.entry(generic).or_default().push(format!("\"{}\" = \"{symbol}\"", part.footprint_id()));
        }
    }
    for (generic, lines) in missing_lines {
        problems.push(format!(
            "migrate/2026-11-packages.toml lacks, under [per_package.\"{generic}\"]:\n{}",
            lines.join("\n")
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
