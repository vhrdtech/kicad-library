//! `kilib ruler`: the silkscreen copy of each footprint's pin ruler on a board, made or taken off
//! ([`kilib::lint::ruler::board`]).

use std::path::PathBuf;
use std::process::ExitCode;

use anstream::{eprintln, println};
use clap::ValueHint;
use kilib::lint::ruler;

use crate::{BOLD, CYAN, DIM, GREEN};

/// The silkscreen copy on or off.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Silk {
    /// Copy each footprint's ruler onto F.SilkS/B.SilkS
    On,
    /// Take the copy off again
    Off,
}

#[derive(clap::Args)]
pub struct RulerArgs {
    /// Boards (.kicad_pcb)
    #[arg(long = "board", value_name = "PCB", required = true, value_hint = ValueHint::FilePath)]
    boards: Vec<PathBuf>,
    /// Copy the ruler onto the silkscreen, or take the copy off
    #[arg(long, value_enum)]
    silk: Silk,
    /// Write the changes
    #[arg(long)]
    write: bool,
}

impl RulerArgs {
    pub fn usage_flags(&self) -> Vec<(&'static str, bool)> {
        vec![("write", self.write), ("silk-on", self.silk == Silk::On), ("silk-off", self.silk == Silk::Off)]
    }
}

pub fn run(args: RulerArgs) -> Result<ExitCode, String> {
    let on = args.silk == Silk::On;
    let mut plans = Vec::new();
    let mut parts = 0;
    for b in &args.boards {
        let old = std::fs::read_to_string(b).map_err(|e| format!("{}: {e}", b.display()))?;
        let (new, items) = ruler::board(&old, on).map_err(|e| format!("{}: {e}", b.display()))?;
        for i in &items {
            parts += 1;
            if on {
                let gone = if i.left_out.is_empty() {
                    String::new()
                } else {
                    format!(", {DIM}no room for {}{DIM:#}", i.left_out.join(", "))
                };
                println!(
                    "  {BOLD}{}{BOLD:#}: {} ticks, {} numbers on the silkscreen{gone}",
                    i.reference, i.ticks, i.numbers
                );
            } else {
                println!("  {BOLD}{}{BOLD:#}: {} silkscreen items of the ruler taken off", i.reference, i.removed);
            }
        }
        if new != old {
            plans.push((b, old, new));
        }
    }
    let what = if on { "copied onto" } else { "taken off" };
    eprintln!(
        "{BOLD}kilib ruler{BOLD:#}: {} boards, {} to change: {parts} footprints' rulers {what} the silkscreen",
        args.boards.len(),
        plans.len()
    );
    if plans.is_empty() {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        return Ok(ExitCode::SUCCESS);
    }
    if !args.write {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        return Ok(ExitCode::from(1));
    }
    for (b, old, new) in &plans {
        let now = std::fs::read_to_string(b).map_err(|e| format!("{}: {e}", b.display()))?;
        if &now != old {
            return Err(format!("{} changed since it was read; run again", b.display()));
        }
        std::fs::write(b, new).map_err(|e| format!("{}: {e}", b.display()))?;
    }
    eprintln!("{GREEN}✓{GREEN:#} wrote {} boards", plans.len());
    Ok(ExitCode::SUCCESS)
}
