//! Pin-1 marks across the library, with the checkout's symbols: every footprint follows `kilib lint --rule
//! pin1,pin1-body`, and the parts that work the same turned half round have none.

use std::path::{Path, PathBuf};

use kilib::lint::{self, Rule};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn footprint(id: &str) -> String {
    let (lib, name) = id.split_once(':').unwrap();
    std::fs::read_to_string(repo().join(format!("footprints/{lib}.pretty/{name}.kicad_mod"))).unwrap()
}

#[test]
fn every_footprint_follows_the_pin1_rules() {
    let ctx = lint::Context::of_checkout(&repo()).unwrap();
    let items = lint::plan(&[repo().join("footprints")], &[Rule::Pin1, Rule::Pin1Body], &ctx);
    assert!(items.len() > 100, "{} footprints", items.len());
    let open: Vec<String> =
        items.iter().filter(|i| i.new.is_some()).map(|i| format!("{}: {:?}", i.id, i.findings)).collect();
    assert!(open.is_empty(), "run kilib lint --rule pin1,pin1-body --write:\n{}", open.join("\n"));
}

#[test]
fn symmetric_and_self_telling_parts_have_no_pin1_mark() {
    for id in [
        "VHRD_Button_Switch_SMD:PTS815",
        "VHRD_Button_Switch_SMD:CL-SB-22B-01T",
        "VHRD_Package_TO_SOT_SMD:SOT-23",
        "VHRD_Package_TO_SOT_SMD:SOT-323_SC-70",
    ] {
        let t = footprint(id);
        assert!(!t.contains("(layer \"User.29\")"), "{id} has a Pin1 mark");
        assert!(!t.contains("(fill yes)\n\t\t(layer \"F.SilkS\")"), "{id} has a filled silkscreen mark");
    }
    for id in ["VHRD_Package_SO:SOP-4_4.4x4.3mm_P2.54mm", "VHRD_Package_SO:SOIC-8-ANALOG"] {
        assert!(footprint(id).contains("(layer \"User.29\")"), "{id} has no Pin1 dot");
    }
}
