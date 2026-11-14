//! `kilib` driven through its binary, on the synthetic fixture of `kilib::testdir`: a clean environment (no
//! `CARGO_*`, `HOME` in the fixture), so nothing of this PC's KiCad config is read or written.

use std::path::Path;
use std::process::{Command, Output};

use kilib::testdir::{TestDir, fixture};

fn kilib(dir: &TestDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kilib"))
        .args(args)
        .env_clear()
        .env("HOME", dir.path().join("home"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("NO_COLOR", "1")
        .current_dir(dir.path())
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr)
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn check_lists_what_does_not_resolve_and_exits_1() {
    let dir = TestDir::new("cli_check");
    fixture(&dir);
    let cfg = dir.path().join("cfg/10.0");
    let out = kilib(&dir, &["check", "--config-dir", p(&cfg), "boards"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    for want in [
        "kilib check: 1 projects, 3 files: 11 symbols, 3 Footprint fields, 4 footprints, 8 3D models",
        "✗ symbol library vhrd_connector_debug: 1 uses",
        "library vhrd_connector_debug is not in any sym-lib-table",
        "vhrd_connector_debug:TC2050 in b1/b1.kicad_sch (1)",
        "✗ symbol Device:Q",
        "✗ Footprint field vhrd_generic:MISSING",
        "✗ 3D model ${VHRD_3DMODEL_DIR}/modules/cannify_micro.step",
        "✗ 3D model C:/Users/someone/Documents/GitHub/kicad-library/3dmodels/x.STEP",
        "    a Windows path",
        "missing .wrl, the .step beside it exists",
        "not checked (no library link, or a library type kilib does not read):",
        "footprint NPTH: no library link in b1/b1.kicad_pcb (1)",
        "symbol old:Y: library old is of type Legacy",
        "✗ 9 of 26 references do not resolve: 3 broken libraries, 6 other items",
    ] {
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }
    assert!(!t.contains("gone:gone"), "backups and hidden folders are skipped:\n{t}");
    let usage = std::fs::read_to_string(dir.path().join("state/kilib/usage.tsv")).unwrap();
    assert!(usage.ends_with("\tcheck\tconfig-dir\n"), "{usage}");
}

#[test]
fn check_json_and_kicad_config_home() {
    let dir = TestDir::new("cli_json");
    fixture(&dir);
    // KICAD_CONFIG_HOME instead of --config-dir: kilib picks the newest version folder in it.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
    cmd.args(["check", "--json", "boards"])
        .env_clear()
        .env("HOME", dir.path().join("home"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("KICAD_CONFIG_HOME", dir.path().join("cfg"))
        .current_dir(dir.path());
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["problems"].as_array().unwrap().len(), 9);
    assert_eq!(json["unchecked"].as_array().unwrap().len(), 2);
    assert_eq!(json["checked"]["symbols"], 11);
    let debug = json["problems"].as_array().unwrap().iter().find(|f| f["id"] == "vhrd_connector_debug:TC2050").unwrap();
    assert_eq!(debug["kind"], "symbol");
    assert_eq!(debug["library"], "vhrd_connector_debug");
    assert_eq!(debug["uses"][0][1], 1);
}

#[test]
fn check_a_clean_tree_exits_0() {
    let dir = TestDir::new("cli_clean");
    fixture(&dir);
    let cfg = dir.path().join("cfg/10.0");
    let out = kilib(&dir, &["check", "--config-dir", p(&cfg), "home/lib"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("✓ all 2 references resolve"), "{t}");
}

#[test]
fn flags_override_tables_and_variables() {
    let dir = TestDir::new("cli_flags");
    fixture(&dir);
    let cfg = dir.path().join("cfg/10.0");
    // Without HOME's library: vhrd_generic and vhrd_power go missing; --var puts them back.
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let home = dir.path().join("home");
    let args = |var: &str| -> Vec<String> {
        ["check", "--config-dir", p(&cfg), "--var", var, "home/lib/symbols"].iter().map(|s| s.to_string()).collect()
    };
    let bad = kilib(&dir, &args(&format!("HOME={}", empty.display())).iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(bad.status.code(), Some(1), "{}", text(&bad));
    assert!(text(&bad).contains("library vhrd_generic is missing"), "{}", text(&bad));
    let good = kilib(&dir, &args(&format!("HOME={}", home.display())).iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(good.status.code(), Some(0), "{}", text(&good));

    // --fp-lib-table replaces the global footprint table.
    let table = dir.write("other-fp-lib-table", "(fp_lib_table\n  (version 7)\n)\n");
    let out = kilib(&dir, &["check", "--config-dir", p(&cfg), "--fp-lib-table", p(&table), "home/lib/symbols"]);
    assert!(text(&out).contains("library vhrd_generic is not in any fp-lib-table"), "{}", text(&out));

    let out = kilib(&dir, &["check", "--var", "NOEQUALS", "boards"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out).contains("expected NAME=VALUE"));
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .map(|e| (e.file_name().into_string().unwrap(), std::fs::read(e.path()).unwrap()))
        .collect();
    files.sort();
    files
}

#[test]
fn setup_dry_run_shows_the_change_and_writes_nothing() {
    let dir = TestDir::new("cli_setup");
    let lib = dir.path().join("lib");
    dir.write("lib/symbols/vhrd_power.kicad_sym", "(kicad_symbol_lib)\n");
    dir.write("lib/footprints/vhrd_generic.pretty/x.kicad_mod", "(footprint \"x\")\n");
    dir.write(
        "cfg/10.0/sym-lib-table",
        "(sym_lib_table\n\t(version 7)\n\t(lib (name \"vhrd_power\") (type \"KiCad\") (uri \"/old/vhrd_power.kicad_sym\") (options \"\") (descr \"\"))\n)\n",
    );
    dir.write("cfg/10.0/kicad_common.json", "{\n  \"environment\": {\n    \"vars\": {}\n  }\n}\n");
    let cfg = dir.path().join("cfg/10.0");
    let before = snapshot(&cfg);
    let stock = dir.path().join("no-stock");
    let out = kilib(
        &dir,
        &["setup", "--dry-run", "--library", p(&lib), "--config-dir", p(&cfg), "--stock-tables", p(&stock)],
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    for want in [
        "sym-lib-table: 1 out, 1 in",
        "- (lib (name \"vhrd_power\") (type \"KiCad\") (uri \"/old/vhrd_power.kicad_sym\")",
        "+ (lib (name \"vhrd_power\") (type \"KiCad\") (uri \"${VHRD_LIBRARY_DIR}/symbols/vhrd_power.kicad_sym\")",
        "fp-lib-table: 0 out, 1 in",
        "new table, without KiCad's stock libraries",
        "+ VHRD_LIBRARY_DIR = ",
        "dry run: nothing written; run without --dry-run to write 3 files",
    ] {
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }
    assert_eq!(snapshot(&cfg), before, "a dry run writes nothing");

    // Run from inside the checkout, --library is found by itself.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
    cmd.args(["setup", "--dry-run", "--config-dir", p(&cfg), "--stock-tables", p(&stock)])
        .env_clear()
        .env("HOME", dir.path().join("home"))
        .env("NO_COLOR", "1")
        .current_dir(lib.join("symbols"));
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert_eq!(snapshot(&cfg), before);
}

#[test]
fn setup_refuses_without_a_config_folder() {
    let dir = TestDir::new("cli_setup_none");
    dir.write("lib/symbols/vhrd_power.kicad_sym", "(kicad_symbol_lib)\n");
    std::fs::create_dir_all(dir.path().join("lib/footprints")).unwrap();
    let missing = dir.path().join("cfg/10.0");
    let out = kilib(&dir, &["setup", "--dry-run", "--library", "lib", "--config-dir", p(&missing)]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out).contains("start KiCad once"), "{}", text(&out));
    assert!(!missing.exists());

    let out = kilib(&dir, &["setup", "--dry-run", "--config-dir", p(dir.path())]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out).contains("no kicad-library checkout around the current folder"), "{}", text(&out));
}

#[test]
fn color_always_and_never() {
    let dir = TestDir::new("cli_color");
    fixture(&dir);
    let cfg = dir.path().join("cfg/10.0");
    let run = |when: &str| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
        cmd.args(["check", "--color", when, "--config-dir", p(&cfg), "home/lib"])
            .env_clear()
            .env("HOME", dir.path().join("home"))
            .env("XDG_STATE_HOME", dir.path().join("state"))
            .current_dir(dir.path());
        String::from_utf8(cmd.output().unwrap().stdout).unwrap()
    };
    assert!(run("always").contains("\u{1b}["), "colour when asked, even into a pipe");
    assert!(!run("never").contains('\u{1b}'));
    assert!(!run("auto").contains('\u{1b}'), "no colour into a pipe");
}

#[test]
fn version_and_completion() {
    let dir = TestDir::new("cli_version");
    let out = kilib(&dir, &["--version"]);
    let t = text(&out);
    assert!(t.starts_with(&format!("kilib {} (", env!("CARGO_PKG_VERSION"))), "{t}");
    assert!(t.contains(", commit "), "{t}");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
    let out = cmd.env_clear().env("COMPLETE", "bash").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("complete"), "a bash completion script");
}

#[test]
fn setup_rows_from_a_map_and_the_three_variables() {
    let dir = TestDir::new("cli_setup_map");
    let lib = dir.path().join("lib");
    dir.write("lib/symbols/vhrd_power.kicad_sym", "(kicad_symbol_lib)\n");
    dir.write("lib/footprints/VHRD_Connector.pretty/c.kicad_mod", "(footprint \"c\")\n");
    dir.write("vendor/footprints/VHRD_Vendor.pretty/v.kicad_mod", "(footprint \"v\")\n");
    std::fs::create_dir_all(dir.path().join("models/own")).unwrap();
    let map = dir.write(
        "map.toml",
        "[libraries.footprint]\n\"VHRD_Connector\" = \"${VHRD_LIBRARY_DIR}/footprints/VHRD_Connector.pretty\"\n\"VHRD_Vendor\" = \"${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty\"\n[libraries.symbol]\n[model]\n\"a.step\" = \"own/a.step\"\n",
    );
    dir.write("cfg/10.0/kicad_common.json", "{}\n");
    let cfg = dir.path().join("cfg/10.0");
    let before = snapshot(&cfg);
    let (vendor, models, stock) = (dir.path().join("vendor"), dir.path().join("models"), dir.path().join("none"));
    let out = kilib(
        &dir,
        &[
            "setup",
            "--dry-run",
            "--library",
            p(&lib),
            "--config-dir",
            p(&cfg),
            "--map",
            p(&map),
            "--vendor-dir",
            p(&vendor),
            "--models-dir",
            p(&models),
            "--stock-tables",
            p(&stock),
        ],
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    for want in [
        "(rows from ",
        "fp-lib-table: 0 out, 2 in",
        "+ (lib (name \"VHRD_Vendor\") (type \"KiCad\") (uri \"${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty\")",
        "sym-lib-table: 0 out, 0 in",
        "+ VHRD_VENDOR_LIBRARY_DIR = ",
        "+ VHRD_3DMODEL_DIR = ",
        "+ VHRD_LIBRARY_DIR = ",
    ] {
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }
    assert!(!t.contains("does not exist"), "every row's folder is there:\n{t}");
    assert_eq!(snapshot(&cfg), before);
}

const MAP: &str = r#"
[libraries.symbol]
"VHRD_Device" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir"
[libraries.footprint]
"VHRD_Package_SO" = "${VHRD_LIBRARY_DIR}/footprints/VHRD_Package_SO.pretty"
"VHRD_Vendor" = "${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty"
[symbol]
"vhrd_device:R" = "VHRD_Device"
[footprint]
"vhrd_generic:SOIC-8" = "VHRD_Package_SO"
"vhrd_generic:PTS815" = "VHRD_Vendor"
"vhrd_generic:_TEMPLATE" = ""
[model]
"ic/SOIC-8.STEP" = "vendor/VHRD_Package_SO.3dshapes/SOIC-8.step"
"#;

fn migrate_fixture(dir: &TestDir) {
    dir.write("map.toml", MAP);
    dir.write("b/b.kicad_sch", "(kicad_sch\n\t(symbol\n\t\t(lib_id \"vhrd_device:R\")\n\t\t(property \"Footprint\" \"vhrd_generic:SOIC-8\")\n\t)\n)\n");
    dir.write("b/b.kicad_pcb", "(kicad_pcb\n\t(footprint \"vhrd_generic:SOIC-8\"\n\t\t(model \"${VHRD_3DMODEL_DIR}/ic/SOIC-8.STEP\")\n\t)\n)\n");
}

#[test]
fn migrate_prints_a_diff_and_writes_with_write() {
    let dir = TestDir::new("cli_migrate");
    migrate_fixture(&dir);
    let before = std::fs::read_to_string(dir.path().join("b/b.kicad_pcb")).unwrap();
    let out = kilib(&dir, &["migrate", "--map", "map.toml", "b"]);
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert_eq!(
        stdout,
        "--- a/b.kicad_pcb\n+++ b/b.kicad_pcb\n@@ -1,5 +1,5 @@\n (kicad_pcb\n-\t(footprint \"vhrd_generic:SOIC-8\"\n-\t\t(model \"${VHRD_3DMODEL_DIR}/ic/SOIC-8.STEP\")\n+\t(footprint \"VHRD_Package_SO:SOIC-8\"\n+\t\t(model \"${VHRD_3DMODEL_DIR}/vendor/VHRD_Package_SO.3dshapes/SOIC-8.step\")\n \t)\n )\n\
--- a/b.kicad_sch\n+++ b/b.kicad_sch\n@@ -1,6 +1,6 @@\n (kicad_sch\n \t(symbol\n-\t\t(lib_id \"vhrd_device:R\")\n-\t\t(property \"Footprint\" \"vhrd_generic:SOIC-8\")\n+\t\t(lib_id \"VHRD_Device:R\")\n+\t\t(property \"Footprint\" \"VHRD_Package_SO:SOIC-8\")\n \t)\n )\n"
    );
    assert!(stderr.contains("kilib migrate: 2 files read, 2 to change: 1 symbol ids, 0 per-package symbols, 0 symbol footprints, 1 Footprint fields, 1 footprints, 0 filters, 1 3D models, 0 table rows"), "{stderr}");
    assert!(stderr.contains("not written: run with --write to apply"), "{stderr}");
    assert_eq!(std::fs::read_to_string(dir.path().join("b/b.kicad_pcb")).unwrap(), before);

    let out = kilib(&dir, &["migrate", "--map", "map.toml", "--stat", "b"]);
    assert!(out.stdout.is_empty(), "--stat prints no diff");

    let out = kilib(&dir, &["migrate", "--map", "map.toml", "--write", "b"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("✓ wrote 2 files"), "{}", text(&out));
    let pcb = std::fs::read_to_string(dir.path().join("b/b.kicad_pcb")).unwrap();
    assert!(pcb.contains("(footprint \"VHRD_Package_SO:SOIC-8\""), "{pcb}");
    let out = kilib(&dir, &["migrate", "--map", "map.toml", "b"]);
    assert!(out.stdout.is_empty() && text(&out).contains("✓ nothing to change"), "{}", text(&out));
    let usage = std::fs::read_to_string(dir.path().join("state/kilib/usage.tsv")).unwrap();
    assert!(usage.contains("\tmigrate\twrite\n") && usage.contains("\tmigrate\tstat\n"), "{usage}");
}

#[test]
fn migrate_refuses_a_dropped_footprint_in_use_and_a_bad_map() {
    let dir = TestDir::new("cli_migrate_errors");
    migrate_fixture(&dir);
    dir.write("b/c.kicad_pcb", "(kicad_pcb (footprint \"vhrd_generic:_TEMPLATE\"))\n");
    let out = kilib(&dir, &["migrate", "--map", "map.toml", "--write", "b"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("✗ footprint vhrd_generic:_TEMPLATE was dropped from the library (\"\" in the map) and is still used in c.kicad_pcb (1)"), "{}", text(&out));
    assert!(text(&out).contains("errors: nothing written"));
    assert!(std::fs::read_to_string(dir.path().join("b/b.kicad_pcb")).unwrap().contains("vhrd_generic:SOIC-8"));

    dir.write("bad.toml", "[symbol]\n\"vhrd_x:A\" = \"VHRD_Nowhere\"\n");
    let out = kilib(&dir, &["migrate", "--map", "bad.toml", "b"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out).contains("targets not in [libraries]: symbol vhrd_x:A = \"VHRD_Nowhere\""), "{}", text(&out));
}

#[test]
fn per_package_lists_placed_generics_and_makes_their_symbols() {
    let dir = TestDir::new("cli_per_package");
    dir.write(
        "lib/symbols/VHRD_Device.kicad_symdir/R.kicad_sym",
        "(kicad_symbol_lib\n\t(version 20241209)\n\t(symbol \"R\"\n\t\t(property \"Footprint\" \"\")\n\t\t(property \"ki_fp_filters\" \"R_*\")\n\t\t(symbol \"R_0_1\")\n\t)\n)\n",
    );
    dir.write(
        "lib/footprints/VHRD_Resistor_SMD.pretty/R_0402_1005Metric.kicad_mod",
        "(footprint \"R_0402_1005Metric\")\n",
    );
    dir.write(
        "map.toml",
        "[libraries.symbol]\n\"VHRD_Device\" = \"${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir\"\n[per_package.\"VHRD_Device:R\"]\n\"VHRD_Resistor_SMD:R_0402_1005Metric\" = \"VHRD_Device:R_0402\"\n",
    );
    let placed = |r: &str, fp: &str| {
        format!(
            "\t(symbol\n\t\t(lib_id \"VHRD_Device:R\")\n\t\t(property \"Reference\" \"{r}\")\n\t\t(property \"Footprint\" \"{fp}\")\n\t)\n"
        )
    };
    let sch = format!(
        "(kicad_sch\n{}{}{})\n",
        placed("R10", "VHRD_Resistor_SMD:R_0402_1005Metric"),
        placed("R9", "VHRD_Resistor_SMD:R_0402_1005Metric"),
        placed("R1", "VHRD_Resistor_SMD:R_0603_1608Metric")
    );
    dir.write("b/b.kicad_sch", &sch);
    let args = ["per-package", "--map", "map.toml", "--library", "lib", "b"];
    let out = kilib(&dir, &args);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    for want in [
        "  VHRD_Device:R VHRD_Resistor_SMD:R_0402_1005Metric ×2 → VHRD_Device:R_0402 R9 R10\n",
        "  VHRD_Device:R VHRD_Resistor_SMD:R_0603_1608Metric ×1 not in the map R1\n",
        "kilib per-package: 1 generic symbols placed with 2 footprints, 1 not in the map",
        "+ VHRD_Device:R_0402 extends VHRD_Device:R, VHRD_Resistor_SMD:R_0402_1005Metric (symbols/VHRD_Device.kicad_symdir/R_0402.kicad_sym)",
        "kilib per-package: 1 per-package symbols in the map, 1 to make",
        "not written: run with --write to make them",
    ] {
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }
    let made = dir.path().join("lib/symbols/VHRD_Device.kicad_symdir/R_0402.kicad_sym");
    assert!(!made.exists());
    let out = kilib(&dir, &["per-package", "--map", "map.toml", "--library", "lib", "--write"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let r_0402 = std::fs::read_to_string(&made).unwrap();
    assert!(
        r_0402.contains("(extends \"R\")") && r_0402.contains("\"VHRD_Resistor_SMD:R_0402_1005Metric\""),
        "{r_0402}"
    );
    let out = kilib(&dir, &["per-package", "--map", "map.toml", "--library", "lib"]);
    assert!(text(&out).contains("✓ every per-package symbol is in the library"), "{}", text(&out));
}

#[test]
fn restructure_dry_run_lists_every_move_then_moves() {
    let dir = TestDir::new("cli_restructure");
    dir.write("map.toml", MAP);
    dir.write(
        "lib/symbols/vhrd_device.kicad_sym",
        "(kicad_symbol_lib (version 20220914) (generator kicad_symbol_editor)\n  (symbol \"R\")\n)\n",
    );
    dir.write(
        "lib/footprints/vhrd_generic.pretty/SOIC-8.kicad_mod",
        "(footprint \"SOIC-8\"\n  (model \"${VHRD_3DMODEL_DIR}/ic/SOIC-8.STEP\"))\n",
    );
    dir.write("lib/footprints/vhrd_generic.pretty/PTS815.kicad_mod", "(footprint \"PTS815\")\n");
    dir.write("lib/footprints/vhrd_generic.pretty/_TEMPLATE.kicad_mod", "(footprint \"_TEMPLATE\")\n");
    let args = ["restructure", "--map", "map.toml", "--vendor-dir", "vendor", "--library", "lib"];
    let out = kilib(&dir, &[&args[..], &["--dry-run"]].concat());
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    let vendor = dir.path().join("vendor/footprints/VHRD_Vendor.pretty/PTS815.kicad_mod");
    for want in [
        "  symbol vhrd_device:R → symbols/VHRD_Device.kicad_symdir/R.kicad_sym\n".to_string(),
        "  footprint vhrd_generic:SOIC-8 → footprints/VHRD_Package_SO.pretty/SOIC-8.kicad_mod, 1 strings rewritten\n"
            .into(),
        format!("  footprint vhrd_generic:PTS815 → {}\n", vendor.display()),
        "- footprint vhrd_generic:_TEMPLATE (footprints/vhrd_generic.pretty/_TEMPLATE.kicad_mod): dropped\n".into(),
        "kilib restructure: 1 symbols, 2 footprints to move, 1 dropped; 4 old files out".into(),
        "dry run: nothing written".into(),
    ] {
        assert!(t.contains(&want), "missing {want:?} in:\n{t}");
    }
    assert!(dir.path().join("lib/symbols/vhrd_device.kicad_sym").exists() && !dir.path().join("vendor").exists());

    // From inside the checkout, without --library.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
    cmd.args(["restructure", "--map", "../map.toml", "--vendor-dir", "../vendor"])
        .env_clear()
        .env("HOME", dir.path().join("home"))
        .env("NO_COLOR", "1")
        .current_dir(dir.path().join("lib"));
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(vendor.is_file());
    assert!(dir.path().join("lib/symbols/VHRD_Device.kicad_symdir/R.kicad_sym").is_file());
    assert!(!dir.path().join("lib/footprints/vhrd_generic.pretty").exists());
    let out = kilib(&dir, &args);
    assert!(text(&out).contains("✓ nothing to move"), "{}", text(&out));
}

#[test]
fn designators_lists_then_writes_then_finds_nothing() {
    let dir = TestDir::new("cli_fab");
    // A SOIC-like part: Reference on silk, a second ${REFERENCE} on the fab layer.
    let mut fp = String::from(
        "(footprint \"P\"\n\t(version 20260206)\n\t(layer \"F.Cu\")\n\t(property \"Reference\" \"REF**\"\n\t\t(at 0 -3 0)\n\t\t(layer \"F.SilkS\")\n\t\t(uuid \"r\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n\t(fp_text user \"${REFERENCE}\"\n\t\t(at 0 0 0)\n\t\t(layer \"F.Fab\")\n\t\t(uuid \"u\")\n\t)\n\t(fp_rect\n\t\t(start -1.95 -2.45)\n\t\t(end 1.95 2.45)\n\t\t(layer \"F.Fab\")\n\t)\n",
    );
    for i in 0..4 {
        let y = -1.905 + 1.27 * f64::from(i);
        fp += &format!("\t(pad \"{}\" smd rect\n\t\t(at -2.7 {y})\n\t\t(size 1.5 0.6)\n\t)\n", i + 1);
        fp += &format!("\t(pad \"{}\" smd rect\n\t\t(at 2.7 {y})\n\t\t(size 1.5 0.6)\n\t)\n", 8 - i);
    }
    fp += ")\n";
    let path = dir.write("lib/footprints/VHRD_Test.pretty/P.kicad_mod", &(fp));
    // A 1 x 0.5 body takes a five-character designator only below 0.2 mm: listed as tiny.
    let small = fp.replace("(start -1.95 -2.45)\n\t\t(end 1.95 2.45)", "(start -0.5 -0.25)\n\t\t(end 0.5 0.25)");
    let small = small.split("\t(pad").next().unwrap().to_string() + ")\n";
    dir.write("lib/footprints/VHRD_Test.pretty/Small.kicad_mod", &small);
    // A body too small for any designator: centred at the smallest size, listed as no clean fit.
    let tiny = small.replace("(start -0.5 -0.25)\n\t\t(end 0.5 0.25)", "(start -0.3 -0.15)\n\t\t(end 0.3 0.15)");
    dir.write("lib/footprints/VHRD_Test.pretty/Tiny.kicad_mod", &tiny);
    std::fs::create_dir_all(dir.path().join("lib/symbols")).unwrap();

    let out = kilib(&dir, &["designators", "lib/footprints"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("VHRD_Test:P: 1 mm 90° at (0.021, 0.024) (Reference on F.SilkS;"), "{t}");
    assert!(t.contains("VHRD_Test:Small: 0.19 mm at (-0.005, 0.004) (Reference on F.SilkS;"), "{t}");
    assert!(t.contains("VHRD_Test:Tiny: 0.2 mm at (-0.005, 0.004), no clean fit ("), "{t}");
    assert!(t.contains("3 footprints: 3 to change (3 moved, 2 resized, 1 turned), 0 already fine, 0 skipped"), "{t}");
    assert!(t.contains("sizes: 1 mm ×1, 0.2 mm ×1, 0.19 mm ×1"), "{t}");
    assert!(t.contains("tiny (1: below 0.2 mm, nothing larger fits): VHRD_Test:Small (0.19 mm)"), "{t}");
    assert!(t.contains("no clean fit (1: centred at 0.2 mm over the drawing or past the body): VHRD_Test:Tiny"), "{t}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), fp, "nothing written without --write");

    // The old name still works.
    let out = kilib(&dir, &["fab-designators", "--write", "lib/footprints"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.contains("(layer \"User.23\")\n\t\t(uuid \"r\")") && !new.contains("${REFERENCE}"), "{new}");

    // From inside the checkout, without paths: its footprints/.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
    cmd.args(["designators"])
        .env_clear()
        .env("HOME", dir.path().join("home"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("NO_COLOR", "1")
        .current_dir(dir.path().join("lib"));
    let out = cmd.output().unwrap();
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("0 to change (0 moved, 0 resized, 0 turned), 3 already fine"), "{t}");
    assert!(t.contains("✓ nothing to change") && t.contains("no clean fit (1:"), "{t}");
    let usage = std::fs::read_to_string(dir.path().join("state/kilib/usage.tsv")).unwrap();
    assert!(usage.contains("\tdesignators\twrite,paths\n"), "{usage}");
}

#[test]
fn designators_board_fits_each_designator_and_value_to_its_own_text() {
    let dir = TestDir::new("cli_fab_board");
    // Two 0402s already updated from the library (Reference on Ref.Des at 0.2 mm, Value on Value), one turned 90°
    // on the board, and one not updated yet (Reference on the silkscreen, Value hidden on the fab layer).
    let part = |r: &str, at: &str, layer: &str, value: &str| {
        format!(
            "\t(footprint \"VHRD_Resistor_SMD:R_0402_1005Metric\"\n\t\t(layer \"F.Cu\")\n\t\t(at {at})\n\t\t(property \"Reference\" \"{r}\"\n\t\t\t(at 0 0 0)\n\t\t\t(unlocked yes)\n\t\t\t(layer \"{layer}\")\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 0.2 0.2)\n\t\t\t\t\t(thickness 0.03)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n\t\t(property \"Value\" \"10K\"\n\t\t\t(at 0 0 0)\n\t\t\t(unlocked yes)\n\t\t\t(layer \"{value}\")\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 0.2 0.2)\n\t\t\t\t\t(thickness 0.03)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n\t\t(fp_rect\n\t\t\t(start -0.525 -0.27)\n\t\t\t(end 0.525 0.27)\n\t\t\t(stroke\n\t\t\t\t(width 0.05)\n\t\t\t\t(type solid)\n\t\t\t)\n\t\t\t(layer \"F.Fab\")\n\t\t)\n\t\t(pad \"1\" smd rect\n\t\t\t(at -0.48 0)\n\t\t\t(size 0.56 0.62)\n\t\t)\n\t\t(pad \"2\" smd rect\n\t\t\t(at 0.48 0)\n\t\t\t(size 0.56 0.62)\n\t\t)\n\t)\n"
        )
    };
    let board = format!(
        "(kicad_pcb\n\t(version 20260206)\n{}{}{})\n",
        part("R806", "10 10", "User.23", "User.17"),
        part("R1", "20 10 90", "User.23", "User.17"),
        part("R2", "30 10", "F.SilkS", "F.Fab")
    );
    // R806 is marked critical in the schematic; an empty note is not looked at.
    let note = |name: &str, layer: &str, v: &str| {
        format!(
            "\t\t(property \"{name}\" \"{v}\"\n\t\t\t(at 0 0 0)\n\t\t\t(unlocked yes)\n\t\t\t(layer \"{layer}\")\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 0.2 0.2)\n\t\t\t\t\t(thickness 0.03)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n"
        )
    };
    let notes = note("Layout_Critical", "User.25", "C1") + &note("Layout_Important", "User.27", "");
    let board = board.replacen("\t\t(fp_rect", &format!("{notes}\t\t(fp_rect"), 1);
    let path = dir.write("b.kicad_pcb", &board);
    let out = kilib(&dir, &["designators", "--board", "b.kicad_pcb"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    // R806 is shorter than R1004: 0.26 mm fits; R1 larger still, written upright at the board's 90°.
    assert!(t.contains("R806: 0.26 mm 0° at ("), "{t}");
    assert!(t.contains("R1: 0.35 mm 90° at ("), "{t}");
    // Each 10K value centred on its own, larger than its designator.
    assert!(t.contains("R806 value: 0.35 mm 0° at (0, 0.016)") && t.contains("R1 value: 0.35 mm 90° at ("), "{t}");
    assert!(t.contains("1 skipped; 2 values to change, 1 layout notes"), "{t}");
    assert!(t.contains("R806 Layout_Critical: 0.35 mm 0° at (0, 0.016)"), "{t}");
    assert!(!t.contains("Layout_Important"), "{t}");
    assert!(t.contains("3 footprints: 2 to change, 0 already fine, 1 skipped"), "{t}");
    assert!(
        t.contains("skipped (1: Reference on F.SilkS, not User.23 (update the footprint from the library)): R2"),
        "{t}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), board, "nothing written without --write");

    let out = kilib(&dir, &["designators", "--board", "b.kicad_pcb", "--write"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.contains("(size 0.26 0.26)\n\t\t\t\t\t(thickness 0.039)"), "{new}");
    // Only the Reference's and the Value's at, size and thickness changed.
    let strip = |s: &str| {
        s.lines()
            .filter(|l| !l.contains("(at ") && !l.contains("(size 0.") && !l.contains("(thickness "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(&new), strip(&board));
    assert_eq!(new.lines().count(), board.lines().count());
    let out = kilib(&dir, &["designators", "--board", "b.kicad_pcb"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("0 to change, 2 already fine, 1 skipped") && t.contains("✓ nothing to change"), "{t}");
}

#[test]
fn stock_import_writes_copies_and_diff_shows_upstream_changes() {
    let dir = TestDir::new("cli_stock");
    let mut fp = String::from(
        "(footprint \"P\"\n\t(version 20260206)\n\t(layer \"F.Cu\")\n\t(property \"Reference\" \"REF**\"\n\t\t(at 0 -3 0)\n\t\t(layer \"F.SilkS\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n\t(property \"Value\" \"P\"\n\t\t(at 0 3 0)\n\t\t(layer \"F.Fab\")\n\t)\n\t(fp_rect\n\t\t(start -1.95 -2.45)\n\t\t(end 1.95 2.45)\n\t\t(layer \"F.Fab\")\n\t)\n",
    );
    for i in 0..4 {
        let y = -1.905 + 1.27 * f64::from(i);
        fp += &format!("\t(pad \"{}\" smd rect\n\t\t(at -2.7 {y})\n\t\t(size 1.5 0.6)\n\t)\n", i + 1);
        fp += &format!("\t(pad \"{}\" smd rect\n\t\t(at 2.7 {y})\n\t\t(size 1.5 0.6)\n\t)\n", 8 - i);
    }
    fp += ")\n";
    let stock = dir.write("stock/Package_SO.pretty/P.kicad_mod", &fp);
    std::fs::create_dir_all(dir.path().join("lib/symbols")).unwrap();
    std::fs::create_dir_all(dir.path().join("lib/footprints")).unwrap();
    let copy = dir.path().join("lib/footprints/VHRD_Package_SO.pretty/P.kicad_mod");
    let s = dir.path().join("stock");
    let args = |extra: &[&'static str]| -> Vec<String> {
        let mut a: Vec<String> = vec!["stock".into()];
        a.extend(extra.iter().map(|s| s.to_string()));
        a.extend(["--stock-dir".into(), p(&s).into(), "--library".into(), "lib".into()]);
        a
    };
    let run = |a: Vec<String>| kilib(&dir, &a.iter().map(String::as_str).collect::<Vec<_>>());

    let out = run(args(&["import", "Package_SO:P"]));
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("Package_SO:P -> lib/footprints/VHRD_Package_SO.pretty/P.kicad_mod"), "{t}");
    assert!(!copy.exists(), "nothing written without --write");

    let out = run(args(&["import", "--write", "Package_SO:P"]));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let new = std::fs::read_to_string(&copy).unwrap();
    assert!(new.contains("(unlocked yes)\n\t\t(layer \"User.17\")"), "{new}");
    assert!(new.contains("(unlocked yes)\n\t\t(layer \"User.23\")"), "{new}");

    let out = run(args(&["diff"]));
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("1 footprints with a stock namesake in") && t.contains("1 as stock, 0 differ"), "{t}");

    std::fs::write(&stock, fp.replace("(size 1.5 0.6)", "(size 1.55 0.6)")).unwrap();
    let out = run(args(&["diff"]));
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("--- a/VHRD_Package_SO.pretty/P.kicad_mod") && t.contains("+\t\t(size 1.55 0.6)"), "{t}");
    let usage = std::fs::read_to_string(dir.path().join("state/kilib/usage.tsv")).unwrap();
    assert!(
        usage.contains("\tstock import\twrite,stock-dir\n") && usage.contains("\tstock diff\tstock-dir\n"),
        "{usage}"
    );
}

#[test]
fn lint_reports_fixes_then_writes_then_only_what_is_left() {
    let dir = TestDir::new("cli_lint");
    // A SOIC-like part: a plain fab rectangle, no courtyard, a 0.2 mm silk line across pad 1, an old model variable.
    let mut fp = String::from(
        "(footprint \"P\"\n\t(version 20260206)\n\t(layer \"F.Cu\")\n\t(attr through_hole)\n\t(fp_line\n\t\t(start -3.6 -1.905)\n\t\t(end -3.6 1.905)\n\t\t(stroke\n\t\t\t(width 0.2)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.SilkS\")\n\t)\n\t(fp_rect\n\t\t(start -1.95 -2.45)\n\t\t(end 1.95 2.45)\n\t\t(stroke\n\t\t\t(width 0.05)\n\t\t\t(type solid)\n\t\t)\n\t\t(fill no)\n\t\t(layer \"F.Fab\")\n\t)\n",
    );
    for i in 0..4 {
        let y = -1.905 + 1.27 * f64::from(i);
        fp += &format!(
            "\t(pad \"{}\" smd rect\n\t\t(at -2.7 {y})\n\t\t(size 1.9 0.6)\n\t\t(layers \"F.Cu\" \"F.Mask\")\n\t)\n",
            i + 1
        );
        fp += &format!(
            "\t(pad \"{}\" smd rect\n\t\t(at 2.7 {y})\n\t\t(size 1.9 0.6)\n\t\t(layers \"F.Cu\" \"F.Mask\")\n\t)\n",
            8 - i
        );
    }
    fp += "\t(model \"${KICAD6_3DMODEL_DIR}/Package_SO.3dshapes/SOIC-8.wrl\"\n\t)\n)\n";
    let path = dir.write("lib/footprints/VHRD_Package_SO.pretty/P.kicad_mod", &fp);

    let out = kilib(&dir, &["lint", "lib/footprints"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    for want in [
        "VHRD_Package_SO:P (part)",
        "fix  attrs     part attributes through_hole → smd",
        "fix  models    3D model ${KICAD6_3DMODEL_DIR}/Package_SO.3dshapes/SOIC-8.wrl → ${KICAD10_3DMODEL_DIR}/Package_SO.3dshapes/SOIC-8.step",
        "fix  courtyard no courtyard: drawn round the pads and body at 0.25 mm",
        "fix  silk      silk fp_line cut clear of pad 1",
        "fix  pin1      no pin-1 mark on F.Fab (nothing tells pin 1 from pad 5): pin-1 corner chamfered",
        "kilib lint: 1 footprints, 1 to change; fixes: attrs 1, models 1, courtyard 1, silk 1, pin1 1, pins 1, centroid 1, layout 2; left for a person: none",
        "fix  pins      User.19: 2 pin numbers at 0.5 mm",
        "fix  centroid  User.21: a cross on the origin",
        "fix  layout    Layout_Critical: added on User.25, empty",
        "fix  layout    Layout_Important: added on User.27, empty",
        "3D model files not checked under VHRD_3DMODEL_DIR, KICAD10_3DMODEL_DIR",
    ] {
        assert!(t.contains(want), "missing {want:?} in:\n{t}");
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), fp, "nothing written without --write");

    let out = kilib(&dir, &["lint", "--json", "--rule", "attrs", "lib/footprints"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["findings"][0]["rule"], "attrs");
    assert_eq!(json[0]["findings"].as_array().unwrap().len(), 1);

    let out = kilib(&dir, &["lint", "--write", "lib/footprints"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let out = kilib(&dir, &["lint", "lib/footprints"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("✓ nothing to fix"), "{t}");

    let out = kilib(&dir, &["lint", "--rule", "bogus", "lib/footprints"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains(
        "unknown rule \"bogus\": one of attrs, models, courtyard, silk, pin1, ref-outline, pins, centroid, layout"
    ));
    let usage = std::fs::read_to_string(dir.path().join("state/kilib/usage.tsv")).unwrap();
    assert!(usage.contains("\tlint\twrite\n") && usage.contains("\tlint\trule,json\n"), "{usage}");
}

#[test]
fn stock_import_symbols_makes_them_standalone_and_diff_compares_them() {
    let dir = TestDir::new("cli_stock_sym");
    let prop = |name: &str, value: &str| {
        format!(
            "\t\t(property \"{name}\" \"{value}\"\n\t\t\t(at 0 0 0)\n\t\t\t(hide yes)\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t)\n\t\t)\n"
        )
    };
    let head = "(kicad_symbol_lib\n\t(version 20251024)\n\t(generator \"kicad_symbol_editor\")\n";
    let parent = format!(
        "{head}\t(symbol \"D_Dual\"\n\t\t(in_bom yes)\n{}{}{}\t\t(symbol \"D_Dual_1_1\"\n\t\t\t(pin passive line\n\t\t\t\t(at 0 2.54 270)\n\t\t\t\t(length 1.27)\n\t\t\t)\n\t\t)\n\t)\n)\n",
        prop("Reference", "D"),
        prop("Footprint", ""),
        prop("Description", "Dual diode"),
    );
    let child = format!(
        "{head}\t(symbol \"BAS16TW\"\n\t\t(extends \"D_Dual\")\n{}{}{}\t)\n)\n",
        prop("Reference", "D"),
        prop("Footprint", "Package_TO_SOT_SMD:SOT-363_SC-70-6"),
        prop("Description", "Triple diode"),
    );
    dir.write("stock/symbols/Diode.kicad_symdir/D_Dual.kicad_sym", &parent);
    dir.write("stock/symbols/Diode.kicad_symdir/BAS16TW.kicad_sym", &child);
    dir.write("lib/footprints/VHRD_Package_TO_SOT_SMD.pretty/SOT-363_SC-70-6.kicad_mod", "(footprint \"x\")\n");
    std::fs::create_dir_all(dir.path().join("lib/symbols")).unwrap();
    let (s, f) = (dir.path().join("stock/symbols"), dir.path().join("stock/footprints"));
    std::fs::create_dir_all(&f).unwrap();
    let copy = dir.path().join("lib/symbols/VHRD_Diode.kicad_symdir/BAS16TW.kicad_sym");
    let args = |extra: &[&'static str]| -> Vec<String> {
        let mut a: Vec<String> = vec!["stock".into()];
        a.extend(extra.iter().map(|s| s.to_string()));
        a.extend(["--stock-symbol-dir".into(), p(&s).into(), "--stock-dir".into(), p(&f).into()]);
        a.extend(["--library".into(), "lib".into()]);
        a
    };
    let run = |a: Vec<String>| kilib(&dir, &a.iter().map(String::as_str).collect::<Vec<_>>());

    let out = run(args(&["import", "--symbols", "--write", "Diode:BAS16TW"]));
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("Diode:BAS16TW -> lib/symbols/VHRD_Diode.kicad_symdir/BAS16TW.kicad_sym"), "{t}");
    let new = std::fs::read_to_string(&copy).unwrap();
    assert!(!new.contains("extends") && new.contains("(symbol \"BAS16TW_1_1\""), "{new}");
    assert!(new.contains("\"VHRD_Package_TO_SOT_SMD:SOT-363_SC-70-6\"") && new.contains("\"Mpn1\" \"\""), "{new}");

    let out = run(args(&["diff"]));
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("1 symbols with a stock namesake in") && t.contains("0 footprints with"), "{t}");

    dir.write("stock/symbols/Diode.kicad_symdir/D_Dual.kicad_sym", &parent.replace("(length 1.27)", "(length 2.54)"));
    let out = run(args(&["diff", "--stat"]));
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("VHRD_Diode:BAS16TW: differs from stock"), "{t}");
}

#[test]
fn layout_fields_lists_then_writes_then_finds_nothing() {
    let dir = TestDir::new("cli_layout_fields");
    let field = |name: &str, v: &str| {
        format!(
            "\t\t(property \"{name}\" \"{v}\"\n\t\t\t(at 0 0 0)\n\t\t\t(effects\n\t\t\t\t(font\n\t\t\t\t\t(size 1.27 1.27)\n\t\t\t\t)\n\t\t\t\t(hide yes)\n\t\t\t)\n\t\t)\n"
        )
    };
    let sym = format!(
        "(kicad_symbol_lib\n\t(version 20241209)\n\t(symbol \"R\"\n{}\t)\n)\n",
        field("Reference", "R") + &field("Description", "Resistor")
    );
    let path = dir.write("lib/symbols/VHRD_Device.kicad_symdir/R.kicad_sym", &sym);
    dir.write(
        "lib/symbols/VHRD_power.kicad_symdir/GND.kicad_sym",
        &sym.replace("\t(symbol \"R\"\n", "\t(symbol \"GND\"\n\t\t(power)\n"),
    );
    let out = kilib(&dir, &["layout-fields", "lib/symbols"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("R.kicad_sym: 1 symbols") && !t.contains("GND.kicad_sym"), "{t}");
    assert!(t.contains("kilib layout-fields: 2 files: 1 to change, 1 symbols get the fields; 0 left alone"), "{t}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), sym, "nothing written without --write");
    let out = kilib(&dir, &["layout-fields", "--write", "lib/symbols"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let new = std::fs::read_to_string(&path).unwrap();
    let want = field("Reference", "R")
        + &field("Description", "Resistor")
        + &field("Layout_Critical", "")
        + &field("Layout_Important", "");
    assert!(new.contains(&want), "{new}");
    let out = kilib(&dir, &["layout-fields", "lib/symbols"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("✓ nothing to change"));
}

#[test]
fn presets_lists_then_writes_then_finds_nothing() {
    let dir = TestDir::new("cli_presets");
    let pro = "{\n  \"board\": {\n    \"layer_presets\": []\n  }\n}\n";
    let pro_path = dir.write("board.kicad_pro", pro);
    dir.write(
        "board.kicad_pcb",
        "(kicad_pcb\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t\t(87 \"User.25\" front \"Crit.F\")\n\t)\n)\n",
    );
    let table_path = dir.write(
        "layer-presets.toml",
        "[[preset]]\nname = \"Layout.F\"\nlayers = [\"F.Cu\", \"Crit.F\"]\nrender = [\"pads\"]\nflip = false\n",
    );
    let out = kilib(&dir, &["presets", "--table", p(&table_path), "board.kicad_pro"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("Layout.F set"), "{t}");
    assert_eq!(std::fs::read_to_string(&pro_path).unwrap(), pro, "nothing written without --write");

    let out = kilib(&dir, &["presets", "--write", "--table", p(&table_path), "board.kicad_pro"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let new = std::fs::read_to_string(&pro_path).unwrap();
    assert!(new.contains("\"name\": \"Layout.F\"") && new.contains("\"flipBoard\": false"), "{new}");

    let out = kilib(&dir, &["presets", "--table", p(&table_path), "board.kicad_pro"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("✓ nothing to change"));
}

#[test]
fn presets_refuses_a_board_missing_the_named_layer() {
    let dir = TestDir::new("cli_presets_missing");
    dir.write("board.kicad_pro", "{\n  \"board\": {\n    \"layer_presets\": []\n  }\n}\n");
    dir.write("board.kicad_pcb", "(kicad_pcb\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t)\n)\n");
    let table_path = dir.write(
        "layer-presets.toml",
        "[[preset]]\nname = \"Layout.F\"\nlayers = [\"F.Cu\", \"Crit.F\"]\nrender = []\nflip = false\n",
    );
    let out = kilib(&dir, &["presets", "--table", p(&table_path), "board.kicad_pro"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(t.contains("Crit.F") && t.contains("kilib layers"), "{t}");
}
