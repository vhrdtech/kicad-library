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
use kilib::libtable::{LibTable, TableKind};
use kilib::setup::{self, Line};

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
    /// Replaces the old vhrd_* rows of the global sym-lib-table and fp-lib-table by one row per library of the
    /// checkout, through ${VHRD_LIBRARY_DIR}, and sets VHRD_LIBRARY_DIR, VHRD_3DMODEL_DIR, VHRD_TEMPLATE_DIR and
    /// KICAD_USER_TEMPLATE_DIR in kicad_common.json. Every other row and setting stays as it is. Refuses while
    /// KiCad runs, since KiCad writes these files when it closes.
    #[command(after_help = "\
Examples:
  kilib setup --dry-run
  kilib setup --library ~/git/kicad-library
  kilib setup --config-dir /tmp/kicad-test/10.0 --dry-run")]
    Setup {
        /// The kicad-library checkout (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        #[command(flatten)]
        config: ConfigArgs,
        /// Where a new table is copied from when the config folder has none (KiCad's stock tables)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, default_value = "/usr/share/kicad/template")]
        stock_tables: PathBuf,
        /// Show what would change and write nothing
        #[arg(long)]
        dry_run: bool,
    },
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
        Command::Setup { library, config, stock_tables, dry_run } => {
            usage::record("setup", &[("dry-run", dry_run), ("config-dir", config.config_dir.is_some())]);
            run_setup(library, &config, &stock_tables, dry_run)
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

fn run_setup(library: Option<PathBuf>, config: &ConfigArgs, stock: &Path, dry_run: bool) -> Result<ExitCode, String> {
    let library = match library {
        Some(l) => l,
        None => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            find_library(&cwd).ok_or("no kicad-library checkout around the current folder: pass --library DIR")?
        }
    };
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
    let plan = setup::plan(&library, &dir, Some(stock))?;
    println!("{BOLD}kilib setup{BOLD:#}: {} → {}", library.display(), dir.display());
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
    let library = std::fs::canonicalize(&library).map_err(|e| e.to_string())?;
    for (name, value) in setup::variables(&library) {
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
