//! The pin ruler across the library: every footprint follows `kilib lint --rule ruler`, and no area-array package
//! (BGA, LGA, WLCSP) carries one.

use std::path::{Path, PathBuf};

use kilib::lint::{self, Rule};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

#[test]
fn every_footprint_follows_the_ruler_rule() {
    let items = lint::plan(&[repo().join("footprints")], &[Rule::Ruler], &lint::Context::default());
    assert!(items.len() > 100, "{} footprints", items.len());
    let open: Vec<String> =
        items.iter().filter(|i| i.new.is_some()).map(|i| format!("{}: {:?}", i.id, i.findings)).collect();
    assert!(open.is_empty(), "run kilib lint --rule ruler --write:\n{}", open.join("\n"));
    let with: Vec<&lint::Item> = items.iter().filter(|i| i.old.contains("(layer \"User.31\")")).collect();
    assert!(with.len() > 15, "{} footprints with a ruler", with.len());
    for i in with {
        let upper = i.id.to_ascii_uppercase();
        assert!(!lint::ruler::EXCLUDED.iter().any(|k| upper.contains(k)), "{} has a ruler", i.id);
    }
}
