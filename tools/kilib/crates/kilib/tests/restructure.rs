//! `kilib::restructure` on a synthetic library checkout: the new layout, byte for byte, and a second run.

mod common;

use kilib::migrate::Severity;
use kilib::restructure::{self, Why};
use kilib::testdir::TestDir;

/// KiCad 10's layout: tabs, a header of three lines.
const DEVICE: &str = "(kicad_symbol_lib
\t(version 20251024)
\t(generator \"kicad_symbol_editor\")
\t(generator_version \"10.0\")
\t(symbol \"R\"
\t\t(property \"Footprint\" \"vhrd_generic:SOT-23\")
\t\t(symbol \"R_0_1\")
\t)
\t(symbol \"SW_Push\"
\t\t(property \"Footprint\" \"\")
\t)
\t(symbol \"SW_Push_Dual\"
\t\t(extends \"SW_Push\")
\t)
\t(symbol \"CHS-01TB\")
)
";

/// The old layout of KiCad 6: two spaces, the header on the first line.
const CONNECTOR: &str = "(kicad_symbol_lib (version 20220914) (generator kicad_symbol_editor)
  (symbol \"Hirose BM50U-4DP/2-0.35V(53)\" (in_bom yes) (on_board yes)
    (property \"Reference\" \"J\" (at 0 0 0))
  )
)
";

const SOT23: &str = "(footprint \"SOT-23\"
\t(layer \"F.Cu\")
\t(model \"${VHRD_3DMODEL_DIR}/sot/SOT-23.step\"
\t\t(offset (xyz 0 0 0))
\t)
)
";

fn library(dir: &TestDir) {
    dir.write("lib/symbols/vhrd_device.kicad_sym", DEVICE);
    dir.write("lib/symbols/vhrd_connector.kicad_sym", CONNECTOR);
    dir.write("lib/footprints/vhrd_generic.pretty/SOT-23.kicad_mod", SOT23);
    dir.write("lib/footprints/vhrd_generic.pretty/SOIC-8.kicad_mod", "(footprint \"SOIC-8\")\n");
    dir.write("lib/footprints/vhrd_generic.pretty/CHS-01TB.kicad_mod", "(footprint \"CHS-01TB\")\n");
    dir.write("lib/footprints/vhrd_generic.pretty/_TEMPLATE.kicad_mod", "(footprint \"_TEMPLATE\")\n");
    // Without the .pretty, as vhrd_connector_usb was; a stray file stays.
    dir.write("lib/footprints/vhrd_connector_usb/USB4105.kicad_mod", "(footprint \"USB4105\")\n");
    dir.write("lib/footprints/vhrd_connector_usb/notes.txt", "x\n");
}

#[test]
fn builds_the_new_layout_and_a_second_run_moves_nothing() {
    let dir = TestDir::new("restructure");
    library(&dir);
    let map = common::map();
    let (lib, vendor) = (dir.path().join("lib"), dir.path().join("vendor"));
    let plan = restructure::plan(&map, &lib, &vendor);
    assert!(!plan.has_errors(), "{:#?}", plan.notes);
    let read = |p: &str| std::fs::read_to_string(dir.path().join(p)).unwrap();

    let why = |id: &str| plan.moves.iter().find(|m| m.id == id).unwrap().why;
    assert_eq!(why("vhrd_device:R"), Why::Item);
    // SW_Push_Dual's own line says VHRD_Device; it extends SW_Push, so it goes to VHRD_Switch.
    assert_eq!(why("vhrd_device:SW_Push_Dual"), Why::Parent);
    assert_eq!(why("vhrd_connector_usb:USB4105"), Why::Item);
    let dropped: Vec<&str> = plan.moves.iter().filter(|m| m.to.is_none()).map(|m| m.id.as_str()).collect();
    assert_eq!(dropped, vec!["vhrd_generic:_TEMPLATE"]);
    let notes: Vec<&(Severity, String)> = plan.notes.iter().collect();
    assert_eq!(notes.len(), 2, "{notes:#?}");
    assert!(notes[0].1.contains("vhrd_device:SW_Push_Dual extends SW_Push: it goes with its parent"));
    assert!(notes[1].1.ends_with("notes.txt: not a footprint, left"));

    restructure::apply(&plan).unwrap();

    let header = "(kicad_symbol_lib\n\t(version 20251024)\n\t(generator \"kicad_symbol_editor\")\n\t(generator_version \"10.0\")\n";
    assert_eq!(
        read("lib/symbols/VHRD_Device.kicad_symdir/R.kicad_sym"),
        format!(
            "{header}\t(symbol \"R\"\n\t\t(property \"Footprint\" \"VHRD_Package_TO_SOT_SMD:SOT-23\")\n\t\t(symbol \"R_0_1\")\n\t)\n)\n"
        )
    );
    assert_eq!(
        read("lib/symbols/VHRD_Switch.kicad_symdir/SW_Push_Dual.kicad_sym"),
        format!("{header}\t(symbol \"SW_Push_Dual\"\n\t\t(extends \"SW_Push\")\n\t)\n)\n")
    );
    assert!(dir.path().join("lib/symbols/VHRD_Switch.kicad_symdir/SW_Push.kicad_sym").is_file());
    assert!(dir.path().join("vendor/symbols/VHRD_Vendor.kicad_symdir/CHS-01TB.kicad_sym").is_file());
    // The old file's own header and indentation; `/` as KiCad 10 writes it in a file name.
    assert_eq!(read("lib/symbols/VHRD_Connector.kicad_symdir/Hirose BM50U-4DP{slash}2-0.35V(53).kicad_sym"), CONNECTOR);
    assert_eq!(
        read("lib/footprints/VHRD_Package_TO_SOT_SMD.pretty/SOT-23.kicad_mod"),
        SOT23.replace("sot/SOT-23.step", "vendor/VHRD_Package_TO_SOT_SMD.3dshapes/SOT-23.step")
    );
    assert_eq!(read("lib/footprints/VHRD_Package_SO.pretty/SOIC-8.kicad_mod"), "(footprint \"SOIC-8\")\n");
    assert!(dir.path().join("vendor/footprints/VHRD_Vendor.pretty/CHS-01TB.kicad_mod").is_file());
    assert!(dir.path().join("lib/footprints/VHRD_Connector_USB.pretty/USB4105.kicad_mod").is_file());

    // The old files are gone; a folder with something else in it stays with that.
    assert!(!dir.path().join("lib/symbols/vhrd_device.kicad_sym").exists());
    assert!(!dir.path().join("lib/footprints/vhrd_generic.pretty").exists());
    assert!(!dir.path().join("lib/footprints/vhrd_connector_usb/USB4105.kicad_mod").exists());
    assert!(dir.path().join("lib/footprints/vhrd_connector_usb/notes.txt").exists());

    let again = restructure::plan(&map, &lib, &vendor);
    assert!(again.moves.is_empty() && again.remove.is_empty(), "{:#?}", again.moves);
}

#[test]
fn a_symbol_the_map_does_not_know_and_a_clash_stop_it() {
    let dir = TestDir::new("restructure_errors");
    dir.write(
        "lib/symbols/vhrd_sim.kicad_sym",
        "(kicad_symbol_lib (version 1)\n  (symbol \"A\")\n  (symbol \"B\" (extends \"Z\"))\n)\n",
    );
    dir.write(
        "lib/symbols/VHRD_Simulation_SPICE.kicad_symdir/A.kicad_sym",
        "(kicad_symbol_lib (symbol \"A\" (other)))\n",
    );
    let map = common::map();
    let plan = restructure::plan(&map, &dir.path().join("lib"), &dir.path().join("vendor"));
    let errors: Vec<&str> = plan.notes.iter().filter(|(s, _)| *s == Severity::Error).map(|(_, m)| m.as_str()).collect();
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert!(errors[0].starts_with("vhrd_sim:B extends Z, which is not in "), "{errors:#?}");
    assert!(errors[1].contains("is there already with other content (for vhrd_sim:A)"), "{errors:#?}");
    assert!(restructure::apply(&plan).is_err());
    assert!(dir.path().join("lib/symbols/vhrd_sim.kicad_sym").exists());
}
