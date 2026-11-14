//! The templates (`templates/*/*.kicad_pro`) against `generate/layer-presets.toml`: every preset of the table is
//! already written into each one, so the two stay in step.

use std::path::{Path, PathBuf};

use kilib::presets::{self, Table};

fn repo() -> PathBuf {
    std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")).unwrap()
}

#[test]
fn every_template_already_has_the_tables_presets() {
    let repo = repo();
    let table = Table::read(&repo.join(presets::TABLE_FILE)).unwrap();
    assert!(!table.presets.is_empty());
    for stem in ["Hammond-1455C801/hammond_1455c801", "IO30/io30", "Vhrd Default/default_vhrd"] {
        let pro_path = repo.join("templates").join(format!("{stem}.kicad_pro"));
        let board_path = repo.join("templates").join(format!("{stem}.kicad_pcb"));
        let pro = std::fs::read_to_string(&pro_path).map_err(|e| format!("{}: {e}", pro_path.display())).unwrap();
        let board = std::fs::read_to_string(&board_path).map_err(|e| format!("{}: {e}", board_path.display())).unwrap();
        let (new, report) = presets::apply(&pro, Some(&board), &table).unwrap();
        assert!(report.set.is_empty(), "{}: run `kilib presets --write` ({:?})", pro_path.display(), report.set);
        assert_eq!(new, pro, "{}: differs from `kilib presets --write`", pro_path.display());
    }
}
