//! Library-wide rules for footprints, each a check that reports and, where the rule is clear, a fix:
//!
//! - [`Rule::Pin1`]: polar and multi-pin parts mark pin 1 on the fab layer, and on the silkscreen where the
//!   footprint draws one ([`pin1`]).
//! - [`Rule::Attrs`]: test points, net ties, spark gaps, jumpers, logos, mounting holes and fiducials stay out of
//!   the BOM and the position files (fiducials stay in them); a part's SMD or through-hole type matches its pads
//!   ([`attrs`]).
//! - [`Rule::Courtyard`]: a closed courtyard keeps [`courtyard::CLEARANCE`] from the pads and the body
//!   ([`courtyard::SMALL_CLEARANCE`] for 0201 to 0603 passives) ([`courtyard`]).
//! - [`Rule::Models`]: every real part has a 3D model through `${VHRD_3DMODEL_DIR}` or `${KICAD10_3DMODEL_DIR}`, and
//!   the file exists ([`models`]).
//! - [`Rule::Silk`]: one line width ([`silk::WIDTH`]), no line over a pad's mask opening, no text but pin-1 marks
//!   ([`silk`]).
//! - [`Rule::RefOutline`]: test points, net ties, spark gaps and jumpers drawn on their designator's reference
//!   layer ([`refs`]).
//!
//! [`plan`] runs the rules in that order on each footprint, each on the text the one before left, and keeps what they
//! fixed and what they leave for a person; [`apply`] writes the fixed footprints. The designator (its text, layer
//! and place) belongs to [`crate::fab`]: no rule here changes it.

pub mod attrs;
pub mod courtyard;
pub mod models;
pub mod pin1;
pub mod refs;
pub mod silk;

use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::footprint::{self, Footprint, PadKind};
use crate::migrate::{Edit, apply_edits};
use crate::vars::Vars;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    Attrs,
    Models,
    Courtyard,
    Silk,
    Pin1,
    RefOutline,
}

impl Rule {
    /// In the order they run.
    pub const ALL: [Rule; 6] = [Rule::Attrs, Rule::Models, Rule::Courtyard, Rule::Silk, Rule::Pin1, Rule::RefOutline];

    pub fn name(self) -> &'static str {
        match self {
            Rule::Attrs => "attrs",
            Rule::Models => "models",
            Rule::Courtyard => "courtyard",
            Rule::Silk => "silk",
            Rule::Pin1 => "pin1",
            Rule::RefOutline => "ref-outline",
        }
    }

    pub fn parse(s: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.name() == s)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What kind of thing a footprint is, which decides the rules it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A part that is bought and placed.
    Part,
    TestPoint,
    NetTie,
    SparkGap,
    /// A solder or PCB jumper: copper only.
    Jumper,
    /// A logo, a marking or other artwork.
    Graphic,
    MountingHole,
    Fiducial,
    /// Screws, spacers, rods, enclosures and panel features: placed by hand or not at all, each its own case.
    Mechanical,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Part => "part",
            Class::TestPoint => "test point",
            Class::NetTie => "net tie",
            Class::SparkGap => "spark gap",
            Class::Jumper => "jumper",
            Class::Graphic => "graphic",
            Class::MountingHole => "mounting hole",
            Class::Fiducial => "fiducial",
            Class::Mechanical => "mechanical",
        }
    }

    /// Copper or artwork on the board, never bought or placed.
    pub fn board_feature(self) -> bool {
        matches!(
            self,
            Class::TestPoint | Class::NetTie | Class::SparkGap | Class::Jumper | Class::Graphic | Class::MountingHole
        )
    }
}

/// The class of footprint `name` in library `lib` (`VHRD_TestPoint`; empty when unknown).
pub fn classify(lib: &str, fp: &Footprint) -> Class {
    let lib = lib.strip_prefix("VHRD_").unwrap_or(lib);
    let name = fp.name.to_ascii_lowercase();
    let has = |s: &str| name.contains(s);
    if lib == "Symbol" || has("logo") {
        Class::Graphic
    } else if lib == "TestPoint" || name.starts_with("testpoint") {
        Class::TestPoint
    } else if has("spark_gap") || has("sparkgap") {
        Class::SparkGap
    } else if lib == "NetTie" || name.starts_with("nettie") {
        Class::NetTie
    } else if lib == "Jumper" || name.starts_with("solderjumper") {
        Class::Jumper
    } else if lib == "MountingHole" || name.starts_with("mountinghole") {
        Class::MountingHole
    } else if has("fiducial") {
        Class::Fiducial
    } else if lib == "Mechanical" {
        Class::Mechanical
    } else if fp.pads.is_empty() {
        Class::Graphic
    } else {
        Class::Part
    }
}

/// What the rules need beyond the footprint itself.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// The path variables 3D models resolve with; `None` leaves the files unchecked.
    pub vars: Option<Vars>,
}

/// One thing a rule found in a footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: Rule,
    pub message: String,
    /// The fix changes this; otherwise it is left for a person.
    pub fixed: bool,
}

/// What one rule makes of one footprint: edits to its text, and what it found.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub edits: Vec<Edit>,
    pub findings: Vec<(String, bool)>,
}

impl Outcome {
    pub fn fixed(&mut self, msg: impl Into<String>) {
        self.findings.push((msg.into(), true));
    }

    pub fn left(&mut self, msg: impl Into<String>) {
        self.findings.push((msg.into(), false));
    }
}

/// A footprint as one rule sees it.
pub struct Input<'a> {
    pub text: &'a str,
    pub fp: &'a Footprint,
    pub lib: &'a str,
    pub class: Class,
    pub ctx: &'a Context,
}

fn run_rule(rule: Rule, i: &Input) -> Outcome {
    match rule {
        Rule::Attrs => attrs::run(i),
        Rule::Models => models::run(i),
        Rule::Courtyard => courtyard::run(i),
        Rule::Silk => silk::run(i),
        Rule::Pin1 => pin1::run(i),
        Rule::RefOutline => refs::run(i),
    }
}

/// One footprint file and what the rules made of it.
#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    /// `Library:Name`.
    pub id: String,
    pub class: Option<Class>,
    pub old: String,
    /// The text after the fixes, when they change it.
    pub new: Option<String>,
    pub findings: Vec<Finding>,
    /// Why the footprint was not looked at.
    pub skipped: Option<String>,
}

/// Runs `rules` on one footprint's text.
pub fn lint_text(
    text: &str,
    lib: &str,
    rules: &[Rule],
    ctx: &Context,
) -> Result<(Class, Option<String>, Vec<Finding>), String> {
    let mut cur = text.to_string();
    let mut findings = Vec::new();
    let mut class = None;
    for rule in Rule::ALL.into_iter().filter(|r| rules.contains(r)) {
        let fp = footprint::read(&cur)?;
        let c = *class.get_or_insert_with(|| classify(lib, &fp));
        let mut out = run_rule(rule, &Input { text: &cur, fp: &fp, lib, class: c, ctx });
        findings.extend(out.findings.drain(..).map(|(message, fixed)| Finding { rule, message, fixed }));
        if !out.edits.is_empty() {
            out.edits.sort_by_key(|e| (e.range.start, e.range.end));
            cur = apply_edits(&cur, &out.edits);
        }
    }
    let fp = footprint::read(&cur)?;
    let class = class.unwrap_or_else(|| classify(lib, &fp));
    Ok((class, (cur != text).then_some(cur), findings))
}

/// Every `.kicad_mod` under `paths` (files, or folders searched), sorted, with what `rules` make of it.
pub fn plan(paths: &[PathBuf], rules: &[Rule], ctx: &Context) -> Vec<Item> {
    let mut files = Vec::new();
    for p in paths {
        collect(p, &mut files);
    }
    files.sort();
    files.dedup();
    files
        .into_iter()
        .map(|path| {
            let (lib, id) = footprint_id(&path);
            let mut item = Item {
                path: path.clone(),
                id,
                class: None,
                old: String::new(),
                new: None,
                findings: Vec::new(),
                skipped: None,
            };
            match std::fs::read_to_string(&path) {
                Err(e) => item.skipped = Some(format!("unreadable: {e}")),
                Ok(old) => {
                    match lint_text(&old, &lib, rules, ctx) {
                        Ok((class, new, findings)) => {
                            item.class = Some(class);
                            item.new = new;
                            item.findings = findings;
                        }
                        Err(e) => item.skipped = Some(e),
                    }
                    item.old = old;
                }
            }
            item
        })
        .collect()
}

/// The library (`VHRD_Package_SO`, from the `.pretty` folder) and `Library:Name` of a footprint file.
fn footprint_id(path: &Path) -> (String, String) {
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    match path.parent().and_then(Path::file_name).map(|d| d.to_string_lossy()) {
        Some(d) if d.ends_with(".pretty") => {
            let lib = d.trim_end_matches(".pretty").to_string();
            let id = format!("{lib}:{name}");
            (lib, id)
        }
        _ => (String::new(), name.to_string()),
    }
}

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_file() {
        if p.extension().is_some_and(|e| e == "kicad_mod") {
            out.push(p.to_path_buf());
        }
        return;
    }
    let Ok(rd) = std::fs::read_dir(p) else { return };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        collect(&e.path(), out);
    }
}

/// Writes every footprint the fixes change; refuses when a file changed since [`plan`] read it.
pub fn apply(items: &[Item]) -> Result<usize, String> {
    let changes: Vec<(&Item, &String)> = items.iter().filter_map(|i| Some((i, i.new.as_ref()?))).collect();
    for (i, _) in &changes {
        let now = std::fs::read_to_string(&i.path).map_err(|e| format!("{}: {e}", i.path.display()))?;
        if now != i.old {
            return Err(format!("{} changed since it was read; run again", i.path.display()));
        }
    }
    for (i, new) in &changes {
        std::fs::write(&i.path, new).map_err(|e| format!("{}: {e}", i.path.display()))?;
    }
    Ok(changes.len())
}

/// The whole lines a node takes, its line break included: what deleting it removes.
pub fn line_span(text: &str, span: &Range<usize>) -> Range<usize> {
    let start = footprint::line_start(text, span.start);
    let end = text[span.end..].find('\n').map_or(text.len(), |i| span.end + i + 1);
    if text[start..span.start].trim().is_empty() && text[span.end..end].trim().is_empty() {
        start..end
    } else {
        span.clone()
    }
}

/// Deletes a node, its lines with it.
pub fn delete(text: &str, span: &Range<usize>) -> Edit {
    Edit { range: line_span(text, span), text: String::new() }
}

/// Inserts whole lines before the first pad.
pub fn insert(fp: &Footprint, lines: String) -> Edit {
    Edit { range: fp.insert_at..fp.insert_at, text: lines }
}

/// The pads that make a part SMD or through-hole: copper pads, a `connect` pad with copper counting as SMD.
pub fn pad_type(fp: &Footprint) -> Option<&'static str> {
    let copper: Vec<_> = fp.copper_pads().collect();
    if copper.is_empty() {
        None
    } else if copper.iter().any(|p| p.kind == PadKind::ThruHole) {
        Some("through_hole")
    } else {
        Some("smd")
    }
}

/// Rounds outwards to `grid`: down for a low edge, up for a high one.
pub fn round_out(v: f64, grid: f64, up: bool) -> f64 {
    let q = v / grid;
    let r = if up { (q - 1e-6).ceil() } else { (q + 1e-6).floor() };
    (r * grid * 1e6).round() / 1e6
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A footprint file around `body` (one item per line, a tab in).
    pub fn fp(name: &str, body: &str) -> String {
        let body: String = body.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| format!("\t{l}\n")).collect();
        format!("(footprint \"{name}\"\n\t(version 20260206)\n\t(generator \"pcbnew\")\n\t(layer \"F.Cu\")\n{body})\n")
    }

    pub fn run(lib: &str, text: &str, rule: Rule) -> (Option<String>, Vec<Finding>) {
        let (_, new, f) = lint_text(text, lib, &[rule], &Context::default()).unwrap();
        (new, f)
    }

    /// The fix leaves nothing it would fix again.
    pub fn settled(lib: &str, text: &str, rule: Rule) {
        let (new, _) = run(lib, text, rule);
        if let Some(new) = new {
            let (again, f) = run(lib, &new, rule);
            assert!(again.is_none(), "a second run changes it again:\n{new}\n{f:?}");
        }
    }

    #[test]
    fn classifies() {
        let read = |name: &str, pads: bool| {
            let pad = if pads { "(pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\"))" } else { "" };
            footprint::read(&fp(name, pad)).unwrap()
        };
        assert_eq!(classify("VHRD_TestPoint", &read("TestPoint_Pad_D1.5mm", true)), Class::TestPoint);
        assert_eq!(classify("VHRD_NetTie", &read("SPARK_GAP_011", true)), Class::SparkGap);
        assert_eq!(classify("VHRD_NetTie", &read("NetTie-2", true)), Class::NetTie);
        assert_eq!(classify("VHRD_Symbol", &read("VHRD_TECH_LOGO", true)), Class::Graphic);
        assert_eq!(classify("VHRD_Fiducials", &read("Fiducial_1mm", true)), Class::Fiducial);
        assert_eq!(classify("VHRD_Mechanical", &read("M1.2_6", false)), Class::Mechanical);
        assert_eq!(classify("VHRD_Package_SO", &read("SOIC-8", true)), Class::Part);
        assert_eq!(classify("VHRD_Package_SO", &read("Outline", false)), Class::Graphic);
    }

    #[test]
    fn rounds_outwards() {
        assert_eq!(round_out(1.234, 0.01, true), 1.24);
        assert_eq!(round_out(-1.234, 0.01, false), -1.24);
        assert_eq!(round_out(1.23, 0.01, true), 1.23);
    }

    #[test]
    fn deletes_whole_lines() {
        let t = "a\n\t(x 1)\nb\n";
        let s = t.find('(').unwrap();
        assert_eq!(line_span(t, &(s..s + 5)), 2..9);
        assert_eq!(apply_edits(t, &[delete(t, &(s..s + 5))]), "a\nb\n");
    }
}
