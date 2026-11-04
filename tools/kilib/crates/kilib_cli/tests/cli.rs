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
        "✗ 3D model C:/Users/Evgeniy/Documents/GitHub/kicad-library/3dmodels/x.STEP",
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
