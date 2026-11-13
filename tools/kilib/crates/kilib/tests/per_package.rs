//! Per-package symbols: `kilib migrate` moving placed generic symbols by their footprint, on a synthetic schematic,
//! and the library's own `migrate/2026-11-packages.toml` against the checkout.

use std::path::{Path, PathBuf};

use kilib::map::Map;
use kilib::migrate::{self, Severity};
use kilib::per_package;
use kilib::sexpr;
use kilib::stock::symbol::standalone;
use kilib::testdir::TestDir;

const MAP: &str = r#"
[libraries.symbol]
"VHRD_Device" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir"
[per_package."VHRD_Device:R"]
"VHRD_Resistor_SMD:R_0402_1005Metric" = "VHRD_Device:R_0402"
[per_package."VHRD_Device:C"]
"VHRD_Capacitor_SMD:C_0402_1005Metric" = "VHRD_Device:C_0402"
"#;

const SCH: &str = "(kicad_sch
\t(version 20250114)
\t(lib_symbols
\t\t(symbol \"VHRD_Device:C\"
\t\t\t(in_bom yes)
\t\t\t(property \"Footprint\" \"\"
\t\t\t\t(at 0 0 0)
\t\t\t)
\t\t\t(property \"ki_fp_filters\" \"C_*\")
\t\t\t(symbol \"C_0_1\")
\t\t\t(symbol \"C_1_1\")
\t\t)
\t\t(symbol \"VHRD_Device:R\"
\t\t\t(property \"Footprint\" \"\")
\t\t\t(property \"ki_fp_filters\" \"R_*\")
\t\t\t(symbol \"R_0_1\")
\t\t)
\t)
\t(symbol
\t\t(lib_id \"VHRD_Device:R\")
\t\t(property \"Reference\" \"R1\")
\t\t(property \"Footprint\" \"VHRD_Resistor_SMD:R_0402_1005Metric\")
\t)
\t(symbol
\t\t(lib_id \"VHRD_Device:R\")
\t\t(property \"Reference\" \"R2\")
\t\t(property \"Footprint\" \"VHRD_Resistor_SMD:R_0603_1608Metric\")
\t)
\t(symbol
\t\t(lib_id \"VHRD_Device:C\")
\t\t(property \"Reference\" \"C1\")
\t\t(property \"Footprint\" \"VHRD_Capacitor_SMD:C_0402_1005Metric\")
\t)
)
";

#[test]
fn placed_generics_move_by_footprint() {
    let dir = TestDir::new("per_package_migrate");
    dir.write("b/b.kicad_sch", SCH);
    let map = Map::parse(MAP).unwrap();
    let plan = migrate::plan(&map, &[dir.path().join("b")]);
    assert!(!plan.has_errors(), "{:#?}", plan.notes);
    assert_eq!(plan.counts.per_package, 2);
    let warnings: Vec<&str> =
        plan.notes.iter().filter(|n| n.severity == Severity::Warning).map(|n| n.message.as_str()).collect();
    assert_eq!(
        warnings,
        [
            "symbol VHRD_Device:R with VHRD_Resistor_SMD:R_0603_1608Metric: no per-package symbol in the map, left as it is"
        ]
    );
    // C has no placed symbol left: its cached symbol becomes C_0402's. R2 keeps R: R_0402 comes after it.
    let want = SCH
        .replace(
            "\t\t(symbol \"VHRD_Device:C\"\n\t\t\t(in_bom yes)\n\t\t\t(property \"Footprint\" \"\"\n\t\t\t\t(at 0 0 0)\n\t\t\t)\n\t\t\t(property \"ki_fp_filters\" \"C_*\")\n\t\t\t(symbol \"C_0_1\")\n\t\t\t(symbol \"C_1_1\")\n",
            "\t\t(symbol \"VHRD_Device:C_0402\"\n\t\t\t(in_bom yes)\n\t\t\t(property \"Footprint\" \"VHRD_Capacitor_SMD:C_0402_1005Metric\"\n\t\t\t\t(at 0 0 0)\n\t\t\t)\n\t\t\t(property \"ki_fp_filters\" \"VHRD_Capacitor_SMD:C_0402_1005Metric\")\n\t\t\t(symbol \"C_0402_0_1\")\n\t\t\t(symbol \"C_0402_1_1\")\n",
        )
        .replace(
            "\t\t\t(symbol \"R_0_1\")\n\t\t)\n",
            "\t\t\t(symbol \"R_0_1\")\n\t\t)\n\t\t(symbol \"VHRD_Device:R_0402\"\n\t\t\t(property \"Footprint\" \"VHRD_Resistor_SMD:R_0402_1005Metric\")\n\t\t\t(property \"ki_fp_filters\" \"VHRD_Resistor_SMD:R_0402_1005Metric\")\n\t\t\t(symbol \"R_0402_0_1\")\n\t\t)\n",
        )
        .replacen("(lib_id \"VHRD_Device:R\")\n\t\t(property \"Reference\" \"R1\")", "(lib_id \"VHRD_Device:R_0402\")\n\t\t(property \"Reference\" \"R1\")", 1)
        .replacen("(lib_id \"VHRD_Device:C\")", "(lib_id \"VHRD_Device:C_0402\")", 1);
    assert_eq!(plan.changes.len(), 1);
    assert_eq!(plan.changes[0].new, want);

    migrate::apply(&plan).unwrap();
    let again = migrate::plan(&map, &[dir.path().join("b")]);
    assert!(again.changes.is_empty(), "a second run changes nothing");
}

#[test]
fn a_placed_generic_without_its_cached_symbol_is_an_error() {
    let dir = TestDir::new("per_package_no_cache");
    dir.write(
        "b/b.kicad_sch",
        "(kicad_sch (lib_symbols) (symbol (lib_id \"VHRD_Device:C\") (property \"Footprint\" \"VHRD_Capacitor_SMD:C_0402_1005Metric\")))\n",
    );
    let plan = migrate::plan(&Map::parse(MAP).unwrap(), &[dir.path().join("b")]);
    let errors: Vec<&str> =
        plan.notes.iter().filter(|n| n.severity == Severity::Error).map(|n| n.message.as_str()).collect();
    assert_eq!(errors, ["symbol VHRD_Device:C is placed, but lib_symbols has no copy of it"]);
}

fn repo() -> PathBuf {
    std::path::absolute(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")).unwrap()
}

/// Every per-package symbol of the map is in the checkout as `kilib per-package` makes it, and flattened (as KiCad
/// caches it in a schematic) it is the copy `kilib migrate` puts in a board's cache of an up-to-date generic: a
/// moved board shows no difference from the library.
#[test]
fn the_library_has_every_per_package_symbol_of_its_map() {
    let repo = repo();
    let map = Map::read(&repo.join("migrate/2026-11-packages.toml")).unwrap();
    let plan = per_package::plan(&repo, &map);
    assert!(!plan.has_errors(), "{:#?}", plan.notes);
    let missing: Vec<&str> = plan.to_write().map(|i| i.id.as_str()).collect();
    assert!(missing.is_empty(), "run kilib per-package --write: {missing:?}");
    assert!(plan.items.len() >= 20);

    let dir = TestDir::new("per_package_library");
    for item in &plan.items {
        let (lib, name) = item.id.split_once(':').unwrap();
        let lib_dir = repo.join("symbols").join(format!("{lib}.kicad_symdir"));
        let gname = item.generic.split_once(':').unwrap().1;
        let generic = std::fs::read_to_string(lib_dir.join(format!("{gname}.kicad_sym"))).unwrap();
        // The generic, cached under its id one level deeper, placed once with the footprint.
        let cached = |text: &str, id: &str| -> String {
            let root = sexpr::parse(text).unwrap();
            let sym = root.child("symbol").unwrap();
            let body = &text[sym.span()];
            let body = body.replacen(&sexpr::quote(sym.arg(0).unwrap()), &sexpr::quote(id), 1);
            format!("\t\t{}", body.replace('\n', "\n\t"))
        };
        let sch = format!(
            "(kicad_sch\n\t(lib_symbols\n{}\n\t)\n\t(symbol\n\t\t(lib_id {})\n\t\t(property \"Footprint\" {})\n\t)\n)\n",
            cached(&generic, &item.generic),
            sexpr::quote(&item.generic),
            sexpr::quote(&item.footprint)
        );
        let file = format!("{lib}_{name}.kicad_sch");
        dir.write(&file, &sch);
        let plan = migrate::plan(&map, &[dir.path().join(&file)]);
        assert!(!plan.has_errors(), "{}: {:#?}", item.id, plan.notes);
        let new = &plan.changes[0].new;
        let flat = standalone(&std::fs::read_to_string(&item.path).unwrap(), &lib_dir).unwrap();
        let want = cached(&flat, &item.id);
        assert!(new.contains(&want), "{}: the cache copy differs from the library:\n{new}\nwant\n{want}", item.id);
    }
}
