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
    assert!(stderr.contains("kilib migrate: 2 files read, 2 to change: 1 symbol ids, 1 Footprint fields, 1 footprints, 0 filters, 1 3D models, 0 table rows"), "{stderr}");
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
fn fab_designators_lists_then_writes_then_finds_nothing() {
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
    std::fs::create_dir_all(dir.path().join("lib/symbols")).unwrap();

    let out = kilib(&dir, &["fab-designators", "lib/footprints"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("VHRD_Test:P: 1 mm 90° at (0, 0) inside (Reference on F.SilkS;"), "{t}");
    assert!(t.contains("1 footprints: 1 to change, 0 already fine, 0 skipped"), "{t}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), fp, "nothing written without --write");

    let out = kilib(&dir, &["fab-designators", "--write", "lib/footprints"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.contains("(layer \"F.Fab\")\n\t\t(uuid \"r\")") && !new.contains("${REFERENCE}"), "{new}");

    // From inside the checkout, without paths: its footprints/.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kilib"));
    cmd.args(["fab-designators"])
        .env_clear()
        .env("HOME", dir.path().join("home"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("NO_COLOR", "1")
        .current_dir(dir.path().join("lib"));
    let out = cmd.output().unwrap();
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("0 to change, 1 already fine") && t.contains("✓ nothing to change"), "{t}");
    let usage = std::fs::read_to_string(dir.path().join("state/kilib/usage.tsv")).unwrap();
    assert!(usage.contains("\tfab-designators\twrite,paths\n"), "{usage}");
}
