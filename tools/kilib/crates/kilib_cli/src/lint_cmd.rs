//! `kilib lint`: the library-wide footprint rules of [`kilib::lint`], as a report and, with `--write`, the fixes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anstream::{eprintln, println};
use clap::ValueHint;
use kilib::config;
use kilib::lint::{self, Rule};

use crate::{BOLD, CYAN, ConfigArgs, DIM, GREEN, YELLOW, library_or_cwd, parse_var};

#[derive(clap::Args)]
pub struct LintArgs {
    /// Footprints: .kicad_mod files or folders searched for them (default: the checkout's footprints/)
    #[arg(value_hint = ValueHint::AnyPath)]
    paths: Vec<PathBuf>,
    /// Only these rules (pin1, attrs, courtyard, models, silk; repeat or comma-separate; default: all)
    #[arg(long, short, value_name = "RULE", value_delimiter = ',', value_parser = parse_rule)]
    rule: Vec<Rule>,
    /// Write the fixes
    #[arg(long)]
    write: bool,
    /// Only what is left for a person, not what the fixes change
    #[arg(long)]
    left: bool,
    /// The report as JSON: one object per footprint with findings
    #[arg(long, conflicts_with = "write")]
    json: bool,
    /// Do not look for the 3D model files, only at their paths
    #[arg(long)]
    no_model_files: bool,
    /// Set or override a path variable for the 3D model files (repeatable)
    #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_var)]
    vars: Vec<(String, String)>,
    #[command(flatten)]
    config: ConfigArgs,
}

fn parse_rule(s: &str) -> Result<Rule, String> {
    Rule::parse(s).ok_or_else(|| {
        let names: Vec<&str> = Rule::ALL.iter().map(|r| r.name()).collect();
        format!("unknown rule {s:?}: one of {}", names.join(", "))
    })
}

impl LintArgs {
    pub fn usage_flags(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("write", self.write),
            ("rule", !self.rule.is_empty()),
            ("left", self.left),
            ("json", self.json),
            ("no-model-files", self.no_model_files),
            ("var", !self.vars.is_empty()),
        ]
    }
}

pub fn run(args: LintArgs) -> Result<ExitCode, String> {
    let paths = if args.paths.is_empty() { vec![library_or_cwd(None)?.join("footprints")] } else { args.paths.clone() };
    let rules: Vec<Rule> = if args.rule.is_empty() { Rule::ALL.to_vec() } else { args.rule.clone() };
    let mut ctx = lint::Context::default();
    let mut unset = Vec::new();
    if rules.contains(&Rule::Models) && !args.no_model_files {
        let share = Path::new("/usr/share/kicad");
        let mut vars = config::vars(args.config.dir().as_deref(), std::env::vars(), share.is_dir().then_some(share))?;
        for (k, v) in &args.vars {
            vars.set(k, v);
        }
        unset = lint::models::VARS.iter().filter(|v| vars.get(v).is_none()).map(|v| v.to_string()).collect();
        ctx.vars = Some(vars);
    }
    let items = lint::plan(&paths, &rules, &ctx);
    if args.json {
        print_json(&items);
    } else {
        print_items(&items, args.left);
    }
    let changed = items.iter().filter(|i| i.new.is_some()).count();
    let mut fixed: BTreeMap<Rule, usize> = BTreeMap::new();
    let mut left: BTreeMap<Rule, usize> = BTreeMap::new();
    for f in items.iter().flat_map(|i| &i.findings) {
        *if f.fixed { &mut fixed } else { &mut left }.entry(f.rule).or_default() += 1;
    }
    let per = |m: &BTreeMap<Rule, usize>| {
        let parts: Vec<String> = rules.iter().filter_map(|r| m.get(r).map(|n| format!("{r} {n}"))).collect();
        if parts.is_empty() { "none".to_string() } else { parts.join(", ") }
    };
    let skipped = items.iter().filter(|i| i.skipped.is_some()).count();
    eprintln!(
        "{BOLD}kilib lint{BOLD:#}: {} footprints, {changed} to change; fixes: {}; left for a person: {}{}",
        items.len(),
        per(&fixed),
        per(&left),
        if skipped > 0 { format!("; {skipped} skipped") } else { String::new() }
    );
    if !unset.is_empty() {
        eprintln!(
            "{CYAN}3D model files not checked{CYAN:#} under {} (unset on this PC: --var NAME=DIR)",
            unset.join(", ")
        );
    }
    if changed == 0 || args.json {
        if changed == 0 {
            eprintln!("{GREEN}✓{GREEN:#} nothing to fix");
        }
        Ok(if changed == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) })
    } else if args.write {
        let n = lint::apply(&items)?;
        eprintln!(
            "{GREEN}✓{GREEN:#} wrote {n} footprints; run kilib fab-designators after it, the designator may need to move"
        );
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply the fixes");
        Ok(ExitCode::from(1))
    }
}

fn print_items(items: &[lint::Item], left_only: bool) {
    for i in items {
        if let Some(why) = &i.skipped {
            println!("{YELLOW}!{YELLOW:#} {}: skipped, {why}", i.id);
            continue;
        }
        let shown: Vec<&lint::Finding> = i.findings.iter().filter(|f| !left_only || !f.fixed).collect();
        if shown.is_empty() {
            continue;
        }
        let class = i.class.map_or("", |c| c.label());
        println!("{BOLD}{}{BOLD:#} {DIM}({class}){DIM:#}", i.id);
        for f in shown {
            if f.fixed {
                println!("  {GREEN}fix{GREEN:#}  {DIM}{:<9}{DIM:#} {}", f.rule.name(), f.message);
            } else {
                println!("  {YELLOW}left{YELLOW:#} {DIM}{:<9}{DIM:#} {}", f.rule.name(), f.message);
            }
        }
    }
}

fn print_json(items: &[lint::Item]) {
    let list: Vec<serde_json::Value> = items
        .iter()
        .filter(|i| !i.findings.is_empty() || i.skipped.is_some())
        .map(|i| {
            serde_json::json!({
                "id": i.id,
                "path": i.path.display().to_string(),
                "class": i.class.map(|c| c.label()),
                "skipped": i.skipped,
                "findings": i.findings.iter().map(|f| serde_json::json!({
                    "rule": f.rule.name(),
                    "fixed": f.fixed,
                    "message": f.message,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
}
