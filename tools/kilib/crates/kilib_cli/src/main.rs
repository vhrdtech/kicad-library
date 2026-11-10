//! `kilib`: checks that KiCad boards resolve their libraries on this PC, and sets up this PC's KiCad config for
//! kicad-library.

mod usage;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anstream::{eprintln, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use kilib::check::{self, Finding, Report};
use kilib::config;
use kilib::fab;
use kilib::libtable::{LibTable, TableKind};
use kilib::migrate;
use kilib::restructure;
use kilib::setup::{self, Line};
use kilib::stock;

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_SHA"), ", commit ", env!("BUILD_TIME"), ")");

const RED: Style = AnsiColor::Red.on_default().bold();
const GREEN: Style = AnsiColor::Green.on_default().bold();
const YELLOW: Style = AnsiColor::Yellow.on_default();
const DIM: Style = Style::new().dimmed();
const BOLD: Style = Style::new().bold();
const CYAN: Style = AnsiColor::Cyan.on_default();

#[derive(Parser)]
#[command(name = "kilib", version = VERSION, about, long_about = None, after_help = AFTER_HELP)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Colour in the output: auto (a terminal without NO_COLOR), always or never
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    color: ColorWhen,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

const AFTER_HELP: &str = "\
Examples:
  kilib check ~/git/hw                  every board in hw resolves on this PC
  kilib check --json board.kicad_pcb    the same for one file, as JSON
  kilib setup --dry-run                 what setup would change in this PC's KiCad config
  kilib migrate --map migrate/2026-11.toml ~/git/hw    the diff that moves the boards to the new layout
  kilib fab-designators                 which footprints still put the designator on the silkscreen
  kilib stock diff                      our copies of KiCad's stock footprints against the current stock library
Shell completion: source <(COMPLETE=bash kilib)   (zsh, fish: COMPLETE=zsh, COMPLETE=fish)";

#[derive(Subcommand)]
enum Command {
    /// Check that every symbol, footprint, Footprint field and 3D model of the boards resolves
    ///
    /// Reads the global sym-lib-table and fp-lib-table and kicad_common.json's path variables from the KiCad
    /// config folder (KICAD_CONFIG_HOME, else ~/.config/kicad, newest version), each project's own tables, and
    /// the environment, which wins over kicad_common.json as in KiCad. Exits 1 when anything does not resolve.
    #[command(after_help = "\
Examples:
  kilib check ~/git/hw
  kilib check --config-dir /tmp/kicad-cfg/10.0 --var KICAD10_3DMODEL_DIR=/mnt/kicad-packages3D ~/git/hw
  kilib check --json ~/git/hw > report.json")]
    Check {
        /// Boards: .kicad_sch, .kicad_pcb, .kicad_sym or .kicad_mod files, or folders searched for them
        #[arg(required = true, value_hint = ValueHint::AnyPath)]
        paths: Vec<PathBuf>,
        #[command(flatten)]
        config: ConfigArgs,
        /// The global symbol table to use instead of the config folder's
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        sym_lib_table: Option<PathBuf>,
        /// The global footprint table to use instead of the config folder's
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        fp_lib_table: Option<PathBuf>,
        /// A path variable, over every other source (repeatable)
        #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_var)]
        vars: Vec<(String, String)>,
        /// Print the report as JSON
        #[arg(long)]
        json: bool,
    },
    /// Point this PC's KiCad at a kicad-library checkout: library rows and path variables
    ///
    /// Replaces every vhrd_* and VHRD_* row of the global sym-lib-table and fp-lib-table by the rows of a rename
    /// map's [libraries.symbol] and [libraries.footprint] (--map), or else of the library folders found, through
    /// ${VHRD_LIBRARY_DIR} and ${VHRD_VENDOR_LIBRARY_DIR}. Sets VHRD_LIBRARY_DIR, VHRD_VENDOR_LIBRARY_DIR and
    /// VHRD_3DMODEL_DIR (each when given), VHRD_TEMPLATE_DIR and KICAD_USER_TEMPLATE_DIR in kicad_common.json.
    /// Every other row and setting stays as it is. Refuses while KiCad runs, since KiCad writes these files when
    /// it closes.
    #[command(after_help = "\
Examples:
  kilib setup --dry-run
  kilib setup --map migrate/2026-11.toml --vendor-dir ~/vhrd-library-vendor \\
              --models-dir ~/vhrd-packages3D
  kilib setup --config-dir /tmp/kicad-test/10.0 --dry-run")]
    Setup {
        /// The kicad-library checkout (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        #[command(flatten)]
        config: ConfigArgs,
        /// A rename map (migrate/*.toml): its [libraries.symbol] and [libraries.footprint] give the rows
        /// (default: the library folders found in the checkout and the vendor library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: Option<PathBuf>,
        /// The private vendor library, for VHRD_VENDOR_LIBRARY_DIR (left as it is when not given)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, env = "VHRD_VENDOR_LIBRARY_DIR")]
        vendor_dir: Option<PathBuf>,
        /// The 3D models, for VHRD_3DMODEL_DIR (left as it is when not given)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, env = "VHRD_3DMODEL_DIR")]
        models_dir: Option<PathBuf>,
        /// Where a new table is copied from when the config folder has none (KiCad's stock tables)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, default_value = "/usr/share/kicad/template")]
        stock_tables: PathBuf,
        /// Show what would change and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Rewrite boards to a new library layout: only the strings the map matches change
    ///
    /// Rewrites symbol ids (lib_id, lib_name, the lib_symbols cache), Footprint fields, ki_fp_filters entries,
    /// footprint ids, ${VHRD_3DMODEL_DIR} model paths and the old vhrd_* rows of project sym-lib-table and
    /// fp-lib-table; every other byte stays as it was. Prints a unified diff; --write applies it. Exits 1 on an
    /// error (a dropped footprint still in use, two cached symbols that would get one name), and then writes
    /// nothing. A second run changes nothing.
    #[command(after_help = "\
Examples:
  kilib migrate --map migrate/2026-11.toml ~/git/hw > hw.diff
  kilib migrate --map migrate/2026-11.toml --write ~/git/hw/b129_cannify_micro")]
    Migrate {
        /// The rename map (migrate/*.toml in kicad-library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: PathBuf,
        /// Boards: files or folders searched for .kicad_sch, .kicad_pcb, .kicad_sym, .kicad_mod and the tables
        #[arg(required = true, value_hint = ValueHint::AnyPath)]
        paths: Vec<PathBuf>,
        /// Write the changes instead of printing the diff
        #[arg(long)]
        write: bool,
        /// Print only the summary, no diff
        #[arg(long, conflicts_with = "write")]
        stat: bool,
    },
    /// Move a kicad-library checkout's symbols and footprints into the map's new libraries
    ///
    /// symbols/<old>.kicad_sym becomes one .kicad_sym per symbol in <New>.kicad_symdir (KiCad 10's layout; a
    /// derived symbol goes with its parent), footprints/<old>.pretty/*.kicad_mod move to <New>.pretty. Libraries
    /// under ${VHRD_VENDOR_LIBRARY_DIR} go to --vendor-dir. Symbol and footprint text is kept as it is, except
    /// what migrate rewrites (model paths, Footprint fields). The old files are removed once the new ones are
    /// written.
    #[command(after_help = "\
Examples:
  kilib restructure --map migrate/2026-11.toml --vendor-dir ~/vhrd-library-vendor --dry-run
  kilib restructure --map migrate/2026-11.toml --vendor-dir ~/vhrd-library-vendor")]
    Restructure {
        /// The rename map (migrate/*.toml in kicad-library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: PathBuf,
        /// Where the libraries under ${VHRD_VENDOR_LIBRARY_DIR} go
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        vendor_dir: PathBuf,
        /// The kicad-library checkout (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        /// List every move and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Copy KiCad's stock footprints into the library, and compare the copies with the current stock library
    ///
    /// The copies are on purpose: a KiCad update never changes our footprints unannounced. `import` copies
    /// Lib:Name from KiCad's footprint folder to footprints/VHRD_Lib.pretty/Name.kicad_mod with the designator on
    /// the fab layer (as fab-designators puts it) and the Value hidden; pads, graphics and the 3D model stay as in
    /// stock. `diff` shows, for every copy with a stock footprint of the same name, what a fresh import would
    /// change, so upstream changes are taken in deliberately.
    #[command(subcommand)]
    Stock(StockCommand),
    /// Put each footprint's designator on the fab layer only, centred on the body
    ///
    /// The Reference property goes to F.Fab (B.Fab on a bottom-side footprint), visible and unlocked, on the
    /// centre of the fab outline (else of the courtyard); ${REFERENCE} texts on the silkscreen and a second one on
    /// the fab layer go, and the Value is hidden. Pads do not matter (KiCad draws the selected fab layer on top);
    /// the other drawing on the fab layer does: the text's box (a five-character designator, the strokes counted)
    /// keeps 0.05 mm off it. It gets the largest size from 1 mm down to 0.3 mm that fits on the centre, level or
    /// turned 90°; else it moves to the largest free spot inside the body nearest the centre; else it stays on the
    /// centre at 0.3 mm and is listed as "no clean fit". Every line on F.Fab and B.Fab gets a 0.05 mm stroke. A
    /// footprint that already follows these rules is left as it is; every other byte stays. Without --write it
    /// lists what would change and exits 1 when anything would.
    #[command(after_help = "\
Examples:
  kilib fab-designators
  kilib fab-designators --write footprints/VHRD_Package_SO.pretty")]
    FabDesignators {
        /// Footprints: .kicad_mod files or folders searched for them (default: the checkout's footprints/)
        #[arg(value_hint = ValueHint::AnyPath)]
        paths: Vec<PathBuf>,
        /// Write the changes
        #[arg(long)]
        write: bool,
        /// List the footprints that are already fine too
        #[arg(long, short)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum StockCommand {
    /// Copy stock footprints (Lib:Name) into footprints/VHRD_Lib.pretty, designator on the fab layer, Value hidden
    ///
    /// Footprints the library has already are left alone (`kilib stock diff` compares them). Without --write it
    /// lists what would be written and exits 1 when anything would.
    #[command(after_help = "\
Examples:
  kilib stock import Resistor_SMD:R_0402_1005Metric Package_SO:VSSOP-8_2.3x2mm_P0.5mm
  kilib stock import --write --stock-dir /usr/share/kicad/footprints Resistor_SMD:R_0402_1005Metric")]
    Import {
        /// Stock footprints, Lib:Name
        #[arg(required = true, value_name = "LIB:NAME")]
        ids: Vec<String>,
        #[command(flatten)]
        stock: StockArgs,
        /// Write the copies
        #[arg(long)]
        write: bool,
    },
    /// Compare each VHRD_Lib:Name with a fresh import of stock Lib:Name; exits 1 when any differs
    ///
    /// Prints a unified diff from our copy to the fresh import (what taking in the stock footprint would change).
    /// Footprints of our own that happen to share a stock name are compared too.
    #[command(after_help = "\
Examples:
  kilib stock diff
  kilib stock diff --stat")]
    Diff {
        #[command(flatten)]
        stock: StockArgs,
        /// Only the list, no diff
        #[arg(long)]
        stat: bool,
    },
}

#[derive(clap::Args)]
struct StockArgs {
    /// KiCad's stock footprint folder (default: KICAD10_FOOTPRINT_DIR from the environment or KiCad's config, else
    /// /usr/share/kicad/footprints)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    stock_dir: Option<PathBuf>,
    /// The kicad-library checkout (default: the one around the current folder)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    library: Option<PathBuf>,
    #[command(flatten)]
    config: ConfigArgs,
}

impl StockArgs {
    fn stock_dir(&self) -> Result<PathBuf, String> {
        if let Some(d) = &self.stock_dir {
            return Ok(d.clone());
        }
        let share = Path::new("/usr/share/kicad");
        let vars = config::vars(self.config.dir().as_deref(), std::env::vars(), share.is_dir().then_some(share))?;
        vars.get("KICAD10_FOOTPRINT_DIR")
            .map(PathBuf::from)
            .filter(|d| d.is_dir())
            .ok_or_else(|| "no KiCad stock footprints found: set KICAD10_FOOTPRINT_DIR or pass --stock-dir DIR".into())
    }
}

#[derive(clap::Args)]
struct ConfigArgs {
    /// The versioned KiCad config folder, like ~/.config/kicad/10.0 (default: from KICAD_CONFIG_HOME or the
    /// platform's config folder)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    config_dir: Option<PathBuf>,
}

impl ConfigArgs {
    fn dir(&self) -> Option<PathBuf> {
        self.config_dir.clone().or_else(|| config::config_dir(&|k| std::env::var(k).ok()))
    }
}

fn parse_var(s: &str) -> Result<(String, String), String> {
    let (k, v) = s.split_once('=').ok_or("expected NAME=VALUE")?;
    if k.is_empty() {
        return Err("expected NAME=VALUE".into());
    }
    Ok((k.to_string(), v.to_string()))
}

fn main() -> ExitCode {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    match cli.color {
        ColorWhen::Auto => {}
        ColorWhen::Always => anstream::ColorChoice::Always.write_global(),
        ColorWhen::Never => anstream::ColorChoice::Never.write_global(),
    }
    let result = match cli.command {
        Command::Check { paths, config, sym_lib_table, fp_lib_table, vars, json } => {
            usage::record(
                "check",
                &[("json", json), ("var", !vars.is_empty()), ("config-dir", config.config_dir.is_some())],
            );
            run_check(&paths, &config, sym_lib_table, fp_lib_table, vars, json)
        }
        Command::Setup { library, config, map, vendor_dir, models_dir, stock_tables, dry_run } => {
            usage::record(
                "setup",
                &[
                    ("dry-run", dry_run),
                    ("config-dir", config.config_dir.is_some()),
                    ("map", map.is_some()),
                    ("vendor-dir", vendor_dir.is_some()),
                    ("models-dir", models_dir.is_some()),
                ],
            );
            run_setup(library, &config, map, vendor_dir, models_dir, &stock_tables, dry_run)
        }
        Command::Migrate { map, paths, write, stat } => {
            usage::record("migrate", &[("write", write), ("stat", stat)]);
            run_migrate(&map, &paths, write, stat)
        }
        Command::Restructure { map, vendor_dir, library, dry_run } => {
            usage::record("restructure", &[("dry-run", dry_run), ("library", library.is_some())]);
            run_restructure(&map, &vendor_dir, library, dry_run)
        }
        Command::Stock(StockCommand::Import { ids, stock, write }) => {
            usage::record("stock import", &[("write", write), ("stock-dir", stock.stock_dir.is_some())]);
            run_stock_import(&ids, &stock, write)
        }
        Command::Stock(StockCommand::Diff { stock, stat }) => {
            usage::record("stock diff", &[("stat", stat), ("stock-dir", stock.stock_dir.is_some())]);
            run_stock_diff(&stock, stat)
        }
        Command::FabDesignators { paths, write, verbose } => {
            usage::record("fab-designators", &[("write", write), ("paths", !paths.is_empty())]);
            run_fab(paths, write, verbose)
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{RED}error:{RED:#} {e}");
            ExitCode::from(2)
        }
    }
}

fn run_check(
    paths: &[PathBuf],
    config: &ConfigArgs,
    sym_lib_table: Option<PathBuf>,
    fp_lib_table: Option<PathBuf>,
    extra: Vec<(String, String)>,
    json: bool,
) -> Result<ExitCode, String> {
    let dir = config.dir();
    let share = Path::new("/usr/share/kicad");
    let mut vars = config::vars(dir.as_deref(), std::env::vars(), share.is_dir().then_some(share))?;
    for (k, v) in extra {
        vars.set(k, v);
    }
    let table = |kind: TableKind, flag: Option<PathBuf>| -> Result<Option<LibTable>, String> {
        match flag {
            Some(p) => LibTable::read(kind, &p).map(Some).map_err(|e| e.to_string()),
            None => match &dir {
                Some(d) => LibTable::read_optional(kind, &d.join(kind.file_name())).map_err(|e| e.to_string()),
                None => Ok(None),
            },
        }
    };
    let setup = check::Setup {
        global_sym: table(TableKind::Symbol, sym_lib_table)?,
        global_fp: table(TableKind::Footprint, fp_lib_table)?,
        vars,
    };
    let report = check::check(paths, &setup);
    if json {
        let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        println!("{text}");
    } else {
        print_report(&report, paths, &setup, dir.as_deref());
    }
    Ok(if report.ok() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

/// A path as the person gave its root: `b129/x.kicad_pcb` under `~/git/hw`.
fn shown(path: &Path, roots: &[PathBuf]) -> String {
    for r in roots {
        let r = std::path::absolute(r).unwrap_or_else(|_| r.clone());
        if let Ok(rel) = path.strip_prefix(&r)
            && !rel.as_os_str().is_empty()
        {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}

fn print_report(report: &Report, roots: &[PathBuf], setup: &check::Setup, dir: Option<&Path>) {
    let c = &report.checked;
    let total = c.symbols + c.footprint_fields + c.footprints + c.models;
    println!(
        "{BOLD}kilib check{BOLD:#}: {} projects, {} files: {} symbols, {} Footprint fields, {} footprints, {} 3D models",
        report.projects, report.files, c.symbols, c.footprint_fields, c.footprints, c.models
    );
    let table_note = |t: &Option<LibTable>, kind: TableKind| match t {
        Some(t) => format!("{} ({} rows)", t.path.display(), t.rows.len()),
        None => format!("no global {}", kind.file_name()),
    };
    println!(
        "{DIM}tables: {}; {}; config: {}{DIM:#}",
        table_note(&setup.global_sym, TableKind::Symbol),
        table_note(&setup.global_fp, TableKind::Footprint),
        dir.map_or("none found".into(), |d| d.display().to_string())
    );

    let libs = report.broken_libraries();
    for ((kind, lib), findings) in &libs {
        let uses: usize = findings.iter().map(|f| f.count()).sum();
        println!();
        println!("{RED}✗{RED:#} {kind_label} library {BOLD}{lib}{BOLD:#}: {uses} uses", kind_label = kind.label());
        let mut reasons: Vec<&str> = findings.iter().map(|f| f.reason.as_str()).collect();
        reasons.dedup();
        for r in reasons {
            println!("    {YELLOW}{r}{YELLOW:#}");
        }
        for f in findings {
            print_uses(f, roots, "    ");
        }
    }
    let items: Vec<&Finding> = report.problems.iter().filter(|f| f.library.is_none()).collect();
    for f in &items {
        println!();
        println!("{RED}✗{RED:#} {} {BOLD}{}{BOLD:#}", f.kind.label(), f.id);
        println!("    {YELLOW}{}{YELLOW:#}", f.reason);
        print_uses_files(f, roots, "    ");
    }
    if !report.unchecked.is_empty() {
        println!();
        println!("{CYAN}not checked{CYAN:#} (no library link, or a library type kilib does not read):");
        for f in &report.unchecked {
            let id = if f.id.is_empty() { "(empty id)" } else { &f.id };
            println!("  {} {id}: {} {DIM}in {}{DIM:#}", f.kind.label(), f.reason, files(f, roots));
        }
    }
    println!();
    if report.ok() {
        println!("{GREEN}✓{GREEN:#} all {total} references resolve");
    } else {
        let bad: usize = report.problems.iter().map(Finding::count).sum();
        println!(
            "{RED}✗{RED:#} {bad} of {total} references do not resolve: {} broken libraries, {} other items",
            libs.len(),
            items.len()
        );
    }
}

fn files(f: &Finding, roots: &[PathBuf]) -> String {
    let files: Vec<String> = f.uses.iter().map(|(p, n)| format!("{} ({n})", shown(p, roots))).collect();
    files.join(", ")
}

fn print_uses(f: &Finding, roots: &[PathBuf], indent: &str) {
    println!("{indent}{} {DIM}in {}{DIM:#}", f.id, files(f, roots));
}

fn print_uses_files(f: &Finding, roots: &[PathBuf], indent: &str) {
    for (p, n) in &f.uses {
        println!("{indent}{DIM}{} ({n}){DIM:#}", shown(p, roots));
    }
}

/// The kicad-library checkout around `start`: the nearest folder with `symbols/` and `footprints/`.
fn find_library(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|d| d.join("symbols").is_dir() && d.join("footprints").is_dir()).map(Path::to_path_buf)
}

fn library_or_cwd(library: Option<PathBuf>) -> Result<PathBuf, String> {
    match library {
        Some(l) => Ok(l),
        None => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            Ok(find_library(&cwd).ok_or("no kicad-library checkout around the current folder: pass --library DIR")?)
        }
    }
}

fn run_setup(
    library: Option<PathBuf>,
    config: &ConfigArgs,
    map: Option<PathBuf>,
    vendor: Option<PathBuf>,
    models: Option<PathBuf>,
    stock: &Path,
    dry_run: bool,
) -> Result<ExitCode, String> {
    let library = library_or_cwd(library)?;
    let dir = config.dir().ok_or("no KiCad config folder: pass --config-dir DIR")?;
    if !dir.is_dir() {
        return Err(format!(
            "no KiCad config at {}: start KiCad once so it makes one, or pass --config-dir DIR",
            dir.display()
        ));
    }
    let running = if cfg!(target_os = "linux") { setup::kicad_running(Path::new("/proc")) } else { Vec::new() };
    if !running.is_empty() && !dry_run {
        return Err(format!(
            "KiCad is running ({}): it writes these files when it closes; close it and run setup again",
            running.join(", ")
        ));
    }
    let canon = |p: &Path| std::fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()));
    let src = setup::Sources {
        library: canon(&library)?,
        vendor: vendor.as_deref().map(canon).transpose()?,
        models: models.as_deref().map(canon).transpose()?,
        map,
    };
    let plan = setup::plan(&src, &dir, Some(stock))?;
    let from = src.map.as_ref().map_or("the library folders".into(), |m| m.display().to_string());
    println!("{BOLD}kilib setup{BOLD:#}: {} → {} (rows from {from})", src.library.display(), dir.display());
    for change in &plan.changes {
        let name = change.path.file_name().unwrap_or_default().to_string_lossy();
        let (out, inn) = change.lines.iter().fold((0, 0), |(o, i), l| match l {
            Line::Removed(_) => (o + 1, i),
            Line::Added(_) => (o, i + 1),
            Line::Note(_) => (o, i),
        });
        println!();
        println!("{BOLD}{name}{BOLD:#}: {out} out, {inn} in");
        for line in &change.lines {
            match line {
                Line::Removed(l) => println!("  {RED}- {l}{RED:#}"),
                Line::Added(l) => println!("  {GREEN}+ {l}{GREEN:#}"),
                Line::Note(l) => println!("  {CYAN}{l}{CYAN:#}"),
            }
        }
    }
    for p in &plan.unchanged {
        println!("{DIM}{}: already set up{DIM:#}", p.file_name().unwrap_or_default().to_string_lossy());
    }
    for (name, value) in setup::variables(&src) {
        if let Ok(env) = std::env::var(name)
            && env != value
        {
            println!(
                "{YELLOW}note:{YELLOW:#} the environment sets {name}={env}, which KiCad uses over kicad_common.json"
            );
        }
    }
    println!();
    if plan.changes.is_empty() {
        println!("{GREEN}✓{GREEN:#} nothing to change");
    } else if dry_run {
        if !running.is_empty() {
            println!(
                "{YELLOW}note:{YELLOW:#} KiCad is running ({}): setup without --dry-run refuses until it is closed",
                running.join(", ")
            );
        }
        println!("{CYAN}dry run{CYAN:#}: nothing written; run without --dry-run to write {} files", plan.changes.len());
    } else {
        setup::apply(&plan)?;
        println!("{GREEN}✓{GREEN:#} wrote {} files", plan.changes.len());
    }
    let _ = std::io::stdout().flush();
    Ok(ExitCode::SUCCESS)
}

fn run_migrate(map: &Path, paths: &[PathBuf], write: bool, stat: bool) -> Result<ExitCode, String> {
    let map = kilib::map::Map::read(map)?;
    let plan = migrate::plan(&map, paths);
    let roots: Vec<PathBuf> = paths.to_vec();
    if !write && !stat {
        for c in &plan.changes {
            for line in c.diff(&shown(&c.path, &roots)).lines() {
                match line.as_bytes().first() {
                    Some(b'+') if !line.starts_with("+++") => println!("{GREEN}{line}{GREEN:#}"),
                    Some(b'-') if !line.starts_with("---") => println!("{RED}{line}{RED:#}"),
                    Some(b'@') => println!("{CYAN}{line}{CYAN:#}"),
                    _ => println!("{line}"),
                }
            }
        }
    }
    // The summary goes to stderr, so the diff on stdout stays a clean patch.
    let c = &plan.counts;
    eprintln!(
        "{BOLD}kilib migrate{BOLD:#}: {} files read, {} to change: {} symbol ids, {} Footprint fields, {} footprints, {} filters, {} 3D models, {} table rows",
        plan.files,
        plan.changes.len(),
        c.symbols,
        c.footprint_fields,
        c.footprints,
        c.filters,
        c.models,
        c.table_rows
    );
    for n in &plan.notes {
        let (mark, style) = match n.severity {
            migrate::Severity::Error => ("✗", RED),
            migrate::Severity::Warning => ("!", YELLOW),
        };
        let files: Vec<String> = n.uses.iter().map(|(p, k)| format!("{} ({k})", shown(p, &roots))).collect();
        eprintln!("{style}{mark}{style:#} {} {DIM}in {}{DIM:#}", n.message, files.join(", "));
    }
    if plan.has_errors() {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if plan.changes.is_empty() {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
    } else if write {
        migrate::apply(&plan)?;
        eprintln!("{GREEN}✓{GREEN:#} wrote {} files", plan.changes.len());
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_restructure(map: &Path, vendor: &Path, library: Option<PathBuf>, dry_run: bool) -> Result<ExitCode, String> {
    let map = kilib::map::Map::read(map)?;
    let library = std::path::absolute(library_or_cwd(library)?).map_err(|e| e.to_string())?;
    let vendor = std::path::absolute(vendor).map_err(|e| e.to_string())?;
    let plan = restructure::plan(&map, &library, &vendor);
    let roots = [library.clone(), vendor.clone()];
    let (mut symbols, mut footprints, mut dropped) = (0, 0, 0);
    for m in &plan.moves {
        let from = shown(&m.from, &roots);
        match &m.to {
            None => {
                dropped += 1;
                println!("{RED}-{RED:#} {} {} {DIM}({from}){DIM:#}: dropped", m.kind.label(), m.id);
            }
            Some(to) => {
                match m.kind {
                    kilib::map::ItemKind::Symbol => symbols += 1,
                    kilib::map::ItemKind::Footprint => footprints += 1,
                }
                let to = if to.starts_with(&library) { shown(to, &roots) } else { to.display().to_string() };
                let why = match m.why {
                    restructure::Why::Item => "",
                    restructure::Why::Library => " (library default)",
                    restructure::Why::Parent => " (with its parent)",
                };
                let rewritten =
                    if m.rewritten > 0 { format!(", {} strings rewritten", m.rewritten) } else { String::new() };
                println!("  {} {} → {to}{DIM}{why}{rewritten}{DIM:#}", m.kind.label(), m.id);
            }
        }
    }
    for (sev, msg) in &plan.notes {
        match sev {
            migrate::Severity::Error => eprintln!("{RED}✗{RED:#} {msg}"),
            migrate::Severity::Warning => eprintln!("{YELLOW}!{YELLOW:#} {msg}"),
        }
    }
    eprintln!(
        "{BOLD}kilib restructure{BOLD:#}: {symbols} symbols, {footprints} footprints to move, {dropped} dropped; {} old files out",
        plan.remove.len()
    );
    if plan.has_errors() {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if plan.moves.is_empty() && plan.remove.is_empty() {
        eprintln!("{GREEN}✓{GREEN:#} nothing to move");
    } else if dry_run {
        eprintln!("{CYAN}dry run{CYAN:#}: nothing written");
    } else {
        restructure::apply(&plan)?;
        eprintln!("{GREEN}✓{GREEN:#} moved");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_fab(paths: Vec<PathBuf>, write: bool, verbose: bool) -> Result<ExitCode, String> {
    let paths = if paths.is_empty() { vec![library_or_cwd(None)?.join("footprints")] } else { paths };
    let items = fab::plan(&paths);
    let (mut changed, mut fine, mut skipped) = (0, 0, 0);
    let (mut moved, mut resized, mut turned, mut unclean) = (0, 0, 0, Vec::new());
    for i in &items {
        match &i.outcome {
            fab::Outcome::Fine => {
                fine += 1;
                if verbose {
                    println!("{DIM}  {}: fine{DIM:#}", i.id);
                }
            }
            fab::Outcome::Skipped(why) => {
                skipped += 1;
                println!("{YELLOW}!{YELLOW:#} {}: skipped, {why}", i.id);
            }
            fab::Outcome::Change { why, placement: p, body_from, .. } => {
                changed += 1;
                let (m, r, t) = i.outcome.designator_changes();
                moved += usize::from(m);
                resized += usize::from(r);
                turned += usize::from(t);
                let angle = if p.angle == 0.0 { String::new() } else { format!(" {}°", fab::fmt(p.angle)) };
                let from = match body_from {
                    fab::BodyFrom::Fab => "",
                    fab::BodyFrom::Courtyard => ", body from the courtyard",
                    fab::BodyFrom::Pads => ", body from the pads",
                };
                let fit = if p.clean { "" } else { ", no clean fit" };
                println!(
                    "  {BOLD}{}{BOLD:#}: {} mm{angle} at ({}, {}){from}{fit} {DIM}({}){DIM:#}",
                    i.id,
                    fab::fmt(p.size),
                    fab::fmt(p.x),
                    fab::fmt(p.y),
                    why.join("; ")
                );
            }
        }
        // A footprint already fine can still be one where nothing fits cleanly.
        let fit = match &i.outcome {
            fab::Outcome::Change { placement, .. } => Some(placement.clean),
            fab::Outcome::Fine => fab::read(&i.old).ok().and_then(|f| f.shape).map(|s| fab::layout(&s).clean),
            fab::Outcome::Skipped(_) => None,
        };
        if fit == Some(false) {
            unclean.push(i.id.as_str());
        }
    }
    eprintln!(
        "{BOLD}kilib fab-designators{BOLD:#}: {} footprints: {changed} to change ({moved} moved, {resized} resized, {turned} turned), {fine} already fine, {skipped} skipped",
        items.len()
    );
    if !unclean.is_empty() {
        eprintln!(
            "{CYAN}no clean fit{CYAN:#} ({}: centred at {} mm over the drawing or past the body): {}",
            unclean.len(),
            fab::fmt(fab::MIN_SIZE),
            unclean.join(", ")
        );
    }
    if changed == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        Ok(ExitCode::SUCCESS)
    } else if write {
        let n = fab::apply(&items)?;
        eprintln!("{GREEN}✓{GREEN:#} wrote {n} footprints");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        Ok(ExitCode::from(1))
    }
}

fn run_stock_import(ids: &[String], args: &StockArgs, write: bool) -> Result<ExitCode, String> {
    let stock_dir = args.stock_dir()?;
    let library = library_or_cwd(args.library.clone())?;
    let imports = stock::plan_import(&stock_dir, &library.join("footprints"), ids);
    let (mut new, mut errors) = (0, 0);
    for i in &imports {
        match &i.outcome {
            stock::ImportOutcome::New { .. } => {
                new += 1;
                println!("  {} -> {}", i.id, shown(&i.to, std::slice::from_ref(&library)));
            }
            stock::ImportOutcome::Exists => println!("{DIM}  {}: in the library already{DIM:#}", i.id),
            stock::ImportOutcome::Error(e) => {
                errors += 1;
                println!("{RED}✗{RED:#} {}: {e}", i.id);
            }
        }
    }
    eprintln!("{BOLD}kilib stock import{BOLD:#}: {} footprints: {new} to copy, {errors} errors", imports.len());
    if errors > 0 {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if new == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to copy");
        return Ok(ExitCode::SUCCESS);
    }
    if !write {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to copy");
        return Ok(ExitCode::from(1));
    }
    let n = stock::apply_import(&imports)?;
    eprintln!(
        "{GREEN}✓{GREEN:#} copied {n} footprints; a new VHRD_* library needs its row in the map's [libraries] and kilib setup"
    );
    Ok(ExitCode::SUCCESS)
}

fn run_stock_diff(args: &StockArgs, stat: bool) -> Result<ExitCode, String> {
    let stock_dir = args.stock_dir()?;
    let library = library_or_cwd(args.library.clone())?;
    let all = stock::compare(&stock_dir, &library.join("footprints"));
    let (mut same, mut differ, mut errors) = (0, 0, 0);
    for c in &all {
        match &c.outcome {
            stock::Compared::Same => same += 1,
            stock::Compared::Differs(d) => {
                differ += 1;
                if stat {
                    println!("  {}: differs from stock", c.id);
                    continue;
                }
                for line in d.lines() {
                    match line.as_bytes().first() {
                        Some(b'+') if !line.starts_with("+++") => println!("{GREEN}{line}{GREEN:#}"),
                        Some(b'-') if !line.starts_with("---") => println!("{RED}{line}{RED:#}"),
                        Some(b'@') => println!("{CYAN}{line}{CYAN:#}"),
                        _ => println!("{line}"),
                    }
                }
            }
            stock::Compared::Error(e) => {
                errors += 1;
                eprintln!("{RED}✗{RED:#} {}: {e}", c.id);
            }
        }
    }
    eprintln!(
        "{BOLD}kilib stock diff{BOLD:#}: {} footprints with a stock namesake in {}: {same} as stock, {differ} differ, {errors} errors",
        all.len(),
        stock_dir.display()
    );
    Ok(if differ + errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
