//! Layout priority across the library: every symbol but the power symbols carries the empty `Layout_Critical` and
//! `Layout_Important` fields (`kilib layout-fields`), and every footprint follows `kilib lint --rule layout`.

use std::path::{Path, PathBuf};

use kilib::layout_fields;
use kilib::lint::{self, Rule};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

#[test]
fn every_symbol_has_the_layout_fields() {
    let items = layout_fields::plan(&[repo().join("symbols")]);
    assert!(items.len() > 100, "{} symbol files", items.len());
    let missing: Vec<String> = items
        .iter()
        .filter(|i| i.new.is_some() || i.error.is_some() || !i.added.skipped.is_empty())
        .map(|i| i.path.display().to_string())
        .collect();
    assert!(missing.is_empty(), "run kilib layout-fields --write:\n{}", missing.join("\n"));
}

#[test]
fn every_footprint_follows_the_layout_rule() {
    let items = lint::plan(&[repo().join("footprints")], &[Rule::Layout], &lint::Context::default());
    assert!(items.len() > 100, "{} footprints", items.len());
    let open: Vec<String> = items
        .iter()
        .filter(|i| i.new.is_some() || i.findings.iter().any(|f| !f.fixed))
        .map(|i| format!("{}: {:?}", i.id, i.findings))
        .collect();
    assert!(open.is_empty(), "run kilib lint --rule layout --write:\n{}", open.join("\n"));
}
