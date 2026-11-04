//! Scratch folders and synthetic KiCad fixtures for tests: written by the test code itself, so no board, vendor
//! file or real KiCad config is needed (or touched).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A folder under the system temp folder, removed when dropped.
pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(name: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kilib-test-{}-{n}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TestDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `text` at `rel`, making the folders on the way.
    pub fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn row(name: &str, kind: &str, uri: &str, extra: &str) -> String {
    format!("  (lib (name \"{name}\")(type \"{kind}\")(uri \"{uri}\")(options \"\")(descr \"\"){extra})\n")
}

/// A PC with stock KiCad libraries, a library checkout under `home/lib`, a KiCad config folder at `cfg/10.0` and
/// one board at `boards/b1`, whose references cover every way to resolve and to break. `HOME` is `home`.
///
/// The board names, by outcome:
/// - resolve: `Device:R` (symdir file), `Device:C_Small` (a derived symbol inside `C.kicad_sym`), `power:GND`,
///   `local:X` (project table through `${KIPRJMOD}`), `nested:N` (a nested table), the footprints
///   `Resistor_SMD:R_0402_1005Metric` and `vhrd_generic:SOT-23`, the models through `${KICAD6_3DMODEL_DIR}` (falls
///   back to KiCad 10's), `${VHRD_3DMODEL_DIR}`, a relative path and `kicad-embed://`;
/// - break: `Device:Q` (not in the library), `vhrd_connector_debug:TC2050` (no such library), `off:Z` (disabled),
///   `unset:U` (its URI names an unset variable), the Footprint field `vhrd_generic:MISSING`, the models
///   `${VHRD_3DMODEL_DIR}/modules/cannify_micro.step` (missing), `.../R_0402_1005Metric.wrl` (only the .step
///   exists), `C:/Users/Evgeniy/x.STEP` and `${UNSET_DIR}/a.step`;
/// - not checked: `old:Y` (a Legacy library) and the footprint `NPTH` (no library link).
pub fn fixture(dir: &TestDir) {
    let root = dir.path().display().to_string();
    dir.write("stock/symbols/Device.kicad_symdir/R.kicad_sym", "(kicad_symbol_lib (version 1) (symbol \"R\"))\n");
    dir.write(
        "stock/symbols/Device.kicad_symdir/C.kicad_sym",
        "(kicad_symbol_lib (version 1) (symbol \"C\") (symbol \"C_Small\" (extends \"C\")))\n",
    );
    dir.write("stock/symbols/power.kicad_sym", "(kicad_symbol_lib (version 1) (symbol \"GND\" (power)))\n");
    dir.write("stock/footprints/Resistor_SMD.pretty/R_0402_1005Metric.kicad_mod", "(footprint \"R_0402_1005Metric\")\n");
    dir.write("stock/3d/Resistor_SMD.3dshapes/R_0402_1005Metric.step", "");

    dir.write(
        "home/lib/symbols/vhrd_power.kicad_sym",
        "(kicad_symbol_lib (version 1)\n  (symbol \"VBUS\" (property \"Footprint\" \"vhrd_generic:SOT-23\")))\n",
    );
    dir.write(
        "home/lib/footprints/vhrd_generic.pretty/SOT-23.kicad_mod",
        "(footprint \"SOT-23\"\n  (model \"${VHRD_3DMODEL_DIR}/sot/SOT-23.step\"))\n",
    );
    dir.write("home/lib/3dmodels/sot/SOT-23.step", "");

    let cfg = "cfg/10.0";
    dir.write(
        &format!("{cfg}/sym-lib-table"),
        &format!(
            "(sym_lib_table\n  (version 7)\n{}{}{}{}{}{})\n",
            row("Device", "KiCad", "${KICAD10_SYMBOL_DIR}/Device.kicad_symdir", ""),
            row("power", "KiCad", "${KICAD10_SYMBOL_DIR}/power.kicad_sym", ""),
            row("vhrd_power", "KiCad", "${HOME}/lib/symbols/vhrd_power.kicad_sym", ""),
            row("old", "Legacy", "${HOME}/old.lib", ""),
            row("off", "KiCad", "${HOME}/lib/symbols/vhrd_power.kicad_sym", "(disabled)"),
            row("unset", "KiCad", "${NOT_SET}/u.kicad_sym", ""),
        ),
    );
    dir.write(
        &format!("{cfg}/fp-lib-table"),
        &format!(
            "(fp_lib_table\n  (version 7)\n{}{})\n",
            row("Resistor_SMD", "KiCad", "${KICAD10_FOOTPRINT_DIR}/Resistor_SMD.pretty", ""),
            row("vhrd_generic", "KiCad", "${HOME}/lib/footprints/vhrd_generic.pretty", ""),
        ),
    );
    dir.write(
        &format!("{cfg}/kicad_common.json"),
        &format!(
            "{{\n  \"environment\": {{\n    \"vars\": {{\n      \"KICAD10_SYMBOL_DIR\": \"{root}/stock/symbols\",\n      \"KICAD10_FOOTPRINT_DIR\": \"{root}/stock/footprints\",\n      \"KICAD10_3DMODEL_DIR\": \"{root}/stock/3d\",\n      \"VHRD_3DMODEL_DIR\": \"{root}/home/lib/3dmodels\"\n    }}\n  }}\n}}\n"
        ),
    );

    dir.write("boards/b1/b1.kicad_pro", "{}\n");
    dir.write(
        "boards/b1/sym-lib-table",
        &format!(
            "(sym_lib_table\n  (version 7)\n{}{})\n",
            row("local", "KiCad", "${KIPRJMOD}/local.kicad_sym", ""),
            row("more", "Table", "${KIPRJMOD}/nested-table", ""),
        ),
    );
    dir.write(
        "boards/b1/nested-table",
        &format!("(sym_lib_table\n  (version 7)\n{})\n", row("nested", "KiCad", "${KIPRJMOD}/local.kicad_sym", "")),
    );
    dir.write("boards/b1/local.kicad_sym", "(kicad_symbol_lib (version 1) (symbol \"X\") (symbol \"N\"))\n");
    let sym = |id: &str, fp: &str| format!("  (symbol (lib_id \"{id}\") (property \"Reference\" \"U\") (property \"Footprint\" \"{fp}\"))\n");
    dir.write(
        "boards/b1/b1.kicad_sch",
        &[
            "(kicad_sch (version 20250114)\n  (lib_symbols (symbol \"Device:R\"))\n".to_string(),
            sym("Device:R", "Resistor_SMD:R_0402_1005Metric"),
            sym("Device:R", "Resistor_SMD:R_0402_1005Metric"),
            sym("Device:C_Small", ""),
            sym("power:GND", ""),
            sym("local:X", "vhrd_generic:MISSING"),
            sym("nested:N", ""),
            sym("Device:Q", ""),
            sym("vhrd_connector_debug:TC2050", ""),
            sym("old:Y", ""),
            sym("off:Z", ""),
            sym("unset:U", ""),
            ")\n".to_string(),
        ]
        .concat(),
    );
    dir.write(
        "boards/b1/b1.kicad_pcb",
        "(kicad_pcb (version 20250114)
  (footprint \"Resistor_SMD:R_0402_1005Metric\"
    (model \"${KICAD6_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.wrl\")
    (model \"${KICAD6_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.step\"))
  (footprint \"vhrd_generic:SOT-23\"
    (model \"${VHRD_3DMODEL_DIR}/sot/SOT-23.step\")
    (model \"models/own.step\")
    (model \"kicad-embed://own.step\"))
  (footprint \"vhrd_generic:SOT-23\"
    (model \"${VHRD_3DMODEL_DIR}/modules/cannify_micro.step\")
    (model \"C:/Users/Evgeniy/Documents/GitHub/kicad-library/3dmodels/x.STEP\")
    (model \"${UNSET_DIR}/a.step\"))
  (footprint \"NPTH\"))
",
    );
    dir.write("boards/b1/models/own.step", "");
    // Backups and hidden folders are not boards.
    dir.write("boards/b1/b1-backups/b1.kicad_pcb", "(kicad_pcb (footprint \"gone:gone\"))\n");
    dir.write("boards/b1/.history/b1.kicad_sch", "(kicad_sch (symbol (lib_id \"gone:gone\")))\n");
}

/// The variables of `fixture`'s config plus `HOME`, as the CLI would see them.
pub fn fixture_env(dir: &TestDir) -> Vec<(String, String)> {
    vec![("HOME".into(), dir.path().join("home").display().to_string())]
}
