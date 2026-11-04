//! `kilib::migrate` on a synthetic board: what changes, what stays byte for byte, what is reported.

mod common;

use kilib::migrate::{self, Severity};
use kilib::testdir::TestDir;

/// A schematic with every place a symbol or footprint id sits, in KiCad 10's tab layout.
const SCH: &str = "(kicad_sch
\t(version 20250114)
\t(lib_symbols
\t\t(symbol \"vhrd_device:R\"
\t\t\t(property \"Footprint\" \"vhrd_generic:SOT-23\")
\t\t\t(property \"ki_fp_filters\" \"vhrd_generic:SOT?23 vhrd_generic:SOIC*  vhrd_generic:S* R_*  Connector*:*_1x??_*\")
\t\t\t(symbol \"R_0_1\")
\t\t)
\t\t(symbol \"Device:C\")
\t)
\t(symbol
\t\t(lib_id \"vhrd_device:R\")
\t\t(property \"Footprint\" \"vhrd_generic:SOT-23\")
\t)
\t(symbol
\t\t(lib_name \"vhrd_device:SW_Push\")
\t\t(lib_id \"vhrd_device:SW_Push\")
\t\t(property \"Footprint\" \"vhrd_connector_usb:USB4105\")
\t)
\t(symbol
\t\t(lib_id \"vhrd_connector:Hirose BM50U-4DP/2-0.35V(53)\")
\t)
\t(symbol
\t\t(lib_id \"vhrd_sim:AMMETER\")
\t\t(property \"Footprint\" \"\")
\t)
\t(symbol
\t\t(lib_id \"Device:C\")
\t\t(property \"Footprint\" \"Capacitor_SMD:C_0402_1005Metric\")
\t\t(property \"Value\" \"vhrd_device:R is only text here\")
\t)
\t(symbol
\t\t(lib_id \"vhrd_cristal:X\")
\t)
)
";

const PCB: &str = "(kicad_pcb (version 20250114)
  (footprint \"vhrd_generic:SOIC-8\" (layer \"F.Cu\")
    (model \"${VHRD_3DMODEL_DIR}/ic/SOIC 8.STEP\" (offset (xyz 0 0 0)))
    (model \"${VHRD_3DMODEL_DIR}/vendor/VHRD_Package_SO.3dshapes/SOIC_8.step\"))
  (footprint \"vhrd_generic:SOT-23\"
    (model \"C:/Users/Someone/Documents/GitHub/kicad-library/3dmodels/sot/SOT-23.step\")
    (model \"${VHRD_3DMODEL_DIR}/switches/CHS-01TB.SLDPRT\")
    (model \"${VHRD_3DMODEL_DIR}/modules/missing.step\")
    (model \"${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.step\"))
  (footprint \"vhrd_generic:QFN-16\")
  (footprint \"Resistor_SMD:R_0402_1005Metric\"))
";

const SYM_TABLE: &str = "(sym_lib_table
  (version 7)
  (lib (name \"vhrd_sim\")(type \"KiCad\")(uri \"${HOME}/git/kicad-library/symbols/vhrd_sim.kicad_sym\")(options \"\")(descr \"simulation-only symbols\"))
  (lib (name \"local\")(type \"KiCad\")(uri \"${KIPRJMOD}/local.kicad_sym\")(options \"\")(descr \"\"))
)
";

const FP_TABLE: &str = "(fp_lib_table
  (version 7)
  (lib (name \"vhrd_generic\")(type \"KiCad\")(uri \"${HOME}/git/kicad-library/footprints/vhrd_generic.pretty\")(options \"\")(descr \"x\"))
  (lib (name \"VHRD_Vendor\")(type \"KiCad\")(uri \"${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty\")(options \"\")(descr \"\"))
  (lib (name \"vhrd_connector_usb\")(type \"KiCad\")(uri \"${HOME}/x\")(options \"\")(descr \"\"))
)
";

fn board(dir: &TestDir) {
    dir.write("b1/b1.kicad_pro", "{}\n");
    dir.write("b1/b1.kicad_sch", SCH);
    dir.write("b1/b1.kicad_pcb", PCB);
    dir.write("b1/sym-lib-table", SYM_TABLE);
    dir.write("b1/fp-lib-table", FP_TABLE);
    dir.write("b1/local.kicad_sym", "(kicad_symbol_lib (version 1) (symbol \"X\"))\n");
}

#[test]
fn rewrites_only_mapped_strings() {
    let dir = TestDir::new("migrate_board");
    board(&dir);
    let map = common::map();
    let plan = migrate::plan(&map, &[dir.path().join("b1")]);
    assert_eq!(plan.files, 5);
    assert!(!plan.has_errors(), "{:#?}", plan.notes);
    let new = |name: &str| &plan.changes.iter().find(|c| c.path.ends_with(name)).unwrap().new;
    for c in &plan.changes {
        common::assert_only_mapped(&map, &c.path, &c.old, &c.new);
    }

    let sch = new("b1.kicad_sch");
    for want in [
        "(symbol \"VHRD_Device:R\"\n",
        "(lib_id \"VHRD_Device:R\")",
        "(lib_name \"VHRD_Switch:SW_Push\")",
        "(lib_id \"VHRD_Switch:SW_Push\")",
        "(lib_id \"VHRD_Connector:Hirose BM50U-4DP/2-0.35V(53)\")",
        "(lib_id \"VHRD_Simulation_SPICE:AMMETER\")",
        "(property \"Footprint\" \"VHRD_Package_TO_SOT_SMD:SOT-23\")",
        "(property \"Footprint\" \"VHRD_Connector_USB:USB4105\")",
        // one target: that library; several (SOIC-8, SO-14, ...): no library; spacing kept
        "\"VHRD_Package_TO_SOT_SMD:SOT?23 VHRD_Package_SO:SOIC*  S* R_*  Connector*:*_1x??_*\"",
        "(symbol \"R_0_1\")",
        "(lib_id \"Device:C\")",
        "\"vhrd_device:R is only text here\"",
        "(lib_id \"vhrd_cristal:X\")",
    ] {
        assert!(sch.contains(want), "missing {want:?} in\n{sch}");
    }
    let pcb = new("b1.kicad_pcb");
    for want in [
        "(footprint \"VHRD_Package_SO:SOIC-8\" (layer \"F.Cu\")",
        "(model \"${VHRD_3DMODEL_DIR}/vendor/VHRD_Package_SO.3dshapes/SOIC_8.step\" (offset (xyz 0 0 0)))",
        "(model \"${VHRD_3DMODEL_DIR}/vendor/VHRD_Package_TO_SOT_SMD.3dshapes/SOT-23.step\")",
        "(model \"${VHRD_3DMODEL_DIR}/switches/CHS-01TB.SLDPRT\")",
        "(footprint \"VHRD_Package_TO_SOT_SMD:QFN-16\")",
        "(footprint \"Resistor_SMD:R_0402_1005Metric\"))",
    ] {
        assert!(pcb.contains(want), "missing {want:?} in\n{pcb}");
    }
    assert_eq!(
        new("sym-lib-table"),
        &SYM_TABLE.replace(
            "(lib (name \"vhrd_sim\")(type \"KiCad\")(uri \"${HOME}/git/kicad-library/symbols/vhrd_sim.kicad_sym\")(options \"\")(descr \"simulation-only symbols\"))",
            "(lib (name \"VHRD_Simulation_SPICE\") (type \"KiCad\") (uri \"${VHRD_LIBRARY_DIR}/symbols/VHRD_Simulation_SPICE.kicad_symdir\") (options \"\") (descr \"simulation-only symbols\"))"
        )
    );
    // vhrd_generic feeds three libraries (one is there already); vhrd_connector_usb's only one is new too.
    let fp = new("fp-lib-table");
    assert_eq!(fp.matches("(lib (name").count(), 4, "{fp}");
    assert!(fp.contains("  (lib (name \"VHRD_Package_SO\") (type \"KiCad\") (uri \"${VHRD_LIBRARY_DIR}/footprints/VHRD_Package_SO.pretty\") (options \"\") (descr \"\"))\n  (lib (name \"VHRD_Package_TO_SOT_SMD\")"), "{fp}");
    assert!(!fp.contains("vhrd_generic") && !fp.contains("vhrd_connector_usb"), "{fp}");
    assert!(!plan.changes.iter().any(|c| c.path.ends_with("local.kicad_sym")));

    assert_eq!(plan.counts.symbols, 6);
    assert_eq!(plan.counts.footprint_fields, 3);
    assert_eq!(plan.counts.footprints, 3);
    assert_eq!(plan.counts.filters, 3);
    assert_eq!(plan.counts.models, 2);
    assert_eq!(plan.counts.table_rows, 3);

    let warnings: Vec<&str> =
        plan.notes.iter().filter(|n| n.severity == Severity::Warning).map(|n| n.message.as_str()).collect();
    assert_eq!(
        warnings,
        vec![
            "3D model ${VHRD_3DMODEL_DIR}/modules/missing.step: not in the map, left as it is",
            "3D model ${VHRD_3DMODEL_DIR}/switches/CHS-01TB.SLDPRT: dropped from the library, left as it is",
            "footprint filter vhrd_generic:S*: its footprints went to VHRD_Package_SO, VHRD_Package_TO_SOT_SMD; written without a library",
            "symbol vhrd_cristal:X: not in the map, left as it is",
        ]
    );
}

#[test]
fn write_then_a_second_run_changes_nothing() {
    let dir = TestDir::new("migrate_twice");
    board(&dir);
    let map = common::map();
    let plan = migrate::plan(&map, &[dir.path().to_path_buf()]);
    // The diff is the edits, line by line.
    let sch = plan.changes.iter().find(|c| c.path.ends_with("b1.kicad_sch")).unwrap();
    let diff = sch.diff("b1/b1.kicad_sch");
    assert!(diff.starts_with("--- a/b1/b1.kicad_sch\n+++ b/b1/b1.kicad_sch\n@@ -1,"), "{diff}");
    // Changed lines next to each other are one block: the old lines, then the new ones.
    assert!(diff.contains("\n-\t\t(lib_id \"vhrd_device:R\")\n-\t\t(property \"Footprint\" \"vhrd_generic:SOT-23\")\n+\t\t(lib_id \"VHRD_Device:R\")\n"), "{diff}");
    migrate::apply(&plan).unwrap();
    let again = migrate::plan(&map, &[dir.path().to_path_buf()]);
    assert!(again.changes.is_empty(), "{:#?}", again.changes.iter().map(|c| &c.path).collect::<Vec<_>>());
}

#[test]
fn a_dropped_footprint_in_use_and_a_cache_clash_are_errors() {
    let dir = TestDir::new("migrate_errors");
    dir.write(
        "b/b.kicad_pcb",
        "(kicad_pcb (footprint \"vhrd_generic:_TEMPLATE\") (footprint \"vhrd_generic:SOT-23\"))\n",
    );
    dir.write(
        "b/b.kicad_sch",
        "(kicad_sch (lib_symbols (symbol \"vhrd_device:SW_Push_Dual\") (symbol \"VHRD_Device:SW_Push_Dual\")))\n",
    );
    let map = common::map();
    let plan = migrate::plan(&map, &[dir.path().join("b")]);
    let errors: Vec<&str> =
        plan.notes.iter().filter(|n| n.severity == Severity::Error).map(|n| n.message.as_str()).collect();
    assert_eq!(
        errors,
        vec![
            "cached symbols vhrd_device:SW_Push_Dual, VHRD_Device:SW_Push_Dual would all become VHRD_Device:SW_Push_Dual",
            "footprint vhrd_generic:_TEMPLATE was dropped from the library (\"\" in the map) and is still used",
        ]
    );
    assert!(migrate::apply(&plan).is_err());
    let pcb = std::fs::read_to_string(dir.path().join("b/b.kicad_pcb")).unwrap();
    assert!(pcb.contains("vhrd_generic:SOT-23"), "nothing written");
}
