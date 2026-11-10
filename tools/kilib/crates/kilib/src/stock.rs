//! KiCad's stock footprints that boards use, kept as copies of our own: `Lib:Name` from KiCad's footprint folder
//! (`${KICAD10_FOOTPRINT_DIR}`) becomes `VHRD_Lib:Name` in the checkout's `footprints/`. The copy is the stock
//! file with the designator and fab lines by [`crate::fab`] and the Value hidden; pads, other graphics, the 3D model
//! (through KiCad's own path variable) and every other byte stay as they were.
//!
//! The copies are on purpose: a KiCad update never changes our footprints unannounced. [`compare`] shows what a
//! fresh import of the current stock file would change in a copy, so upstream changes are taken in deliberately.

use std::path::{Path, PathBuf};

use crate::fab;
use crate::migrate::{Edit, apply_edits, unified_diff};
use crate::sexpr;

/// What stock libraries are prefixed with in the checkout.
pub const PREFIX: &str = "VHRD_";

/// The copy of a stock footprint's text: the designator by the fab rules, the Value hidden.
pub fn import_text(stock: &str) -> Result<String, String> {
    let text = match fab::fix(stock) {
        fab::Outcome::Fine => stock.to_string(),
        fab::Outcome::Change { new, .. } => new,
        fab::Outcome::Skipped(why) => return Err(why),
    };
    hide_value(&text)
}

/// The text with its `Value` property hidden (`(hide yes)` after its layer, where KiCad writes it).
pub fn hide_value(text: &str) -> Result<String, String> {
    let fp = sexpr::parse(text).map_err(|e| e.to_string())?;
    let value = fp.children("property").find(|p| p.arg(0) == Some("Value")).ok_or("no Value property")?;
    Ok(match fab::hide_value_edit(text, value)? {
        Some(e) => apply_edits(text, &[e]),
        None => text.to_string(),
    })
}

/// `Lib:Name` split, the library given without [`PREFIX`].
pub fn split_id(id: &str) -> Result<(&str, &str), String> {
    let (lib, name) =
        id.split_once(':').filter(|(l, n)| !l.is_empty() && !n.is_empty()).ok_or(format!("{id}: not Lib:Name"))?;
    Ok((lib.strip_prefix(PREFIX).unwrap_or(lib), name))
}

fn mod_path(dir: &Path, lib: &str, name: &str) -> PathBuf {
    dir.join(format!("{lib}.pretty")).join(format!("{name}.kicad_mod"))
}

/// One stock footprint to copy in.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// `Lib:Name` in the stock library.
    pub id: String,
    pub from: PathBuf,
    pub to: PathBuf,
    pub outcome: ImportOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportOutcome {
    /// Not in the checkout yet; `text` is the copy.
    New { text: String },
    /// The checkout has it already (`stock diff` compares it).
    Exists,
    /// Cannot be imported.
    Error(String),
}

/// What importing `ids` (`Lib:Name`) from `stock` into `footprints` (the checkout's folder) would write.
pub fn plan_import(stock: &Path, footprints: &Path, ids: &[String]) -> Vec<Import> {
    ids.iter()
        .map(|id| {
            let (from, to, outcome) = match split_id(id) {
                Err(e) => (PathBuf::new(), PathBuf::new(), ImportOutcome::Error(e)),
                Ok((lib, name)) => {
                    let from = mod_path(stock, lib, name);
                    let to = mod_path(footprints, &format!("{PREFIX}{lib}"), name);
                    let outcome = if to.exists() {
                        ImportOutcome::Exists
                    } else {
                        match std::fs::read_to_string(&from) {
                            Err(e) => ImportOutcome::Error(format!("{}: {e}", from.display())),
                            Ok(t) => match import_text(&t) {
                                Ok(text) => ImportOutcome::New { text },
                                Err(e) => ImportOutcome::Error(e),
                            },
                        }
                    };
                    (from, to, outcome)
                }
            };
            Import { id: id.clone(), from, to, outcome }
        })
        .collect()
}

/// Writes the new copies (creating a library folder where needed); refuses when any import has an error.
pub fn apply_import(imports: &[Import]) -> Result<usize, String> {
    if let Some((i, e)) = imports.iter().find_map(|i| match &i.outcome {
        ImportOutcome::Error(e) => Some((i, e)),
        _ => None,
    }) {
        return Err(format!("{}: {e}; nothing written", i.id));
    }
    let mut n = 0;
    for i in imports {
        if let ImportOutcome::New { text } = &i.outcome {
            if let Some(dir) = i.to.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&i.to, text).map_err(|e| format!("{}: {e}", i.to.display()))?;
            n += 1;
        }
    }
    Ok(n)
}

/// A copy in the checkout against the current stock file of the same name.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// `VHRD_Lib:Name`.
    pub id: String,
    pub ours: PathBuf,
    pub stock: PathBuf,
    pub outcome: Compared,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Compared {
    /// A fresh import of the stock file gives the copy byte for byte.
    Same,
    /// The unified diff from the copy to a fresh import (what taking in the stock file would change).
    Differs(String),
    Error(String),
}

/// Every footprint under `footprints/VHRD_<Lib>.pretty` that has a stock footprint `<Lib>:<Name>` in `stock`,
/// compared with a fresh import of it. Footprints of our own whose name matches a stock one are listed too.
pub fn compare(stock: &Path, footprints: &Path) -> Vec<Comparison> {
    let mut out = Vec::new();
    let mut libs: Vec<PathBuf> =
        read_dir(footprints).filter(|p| p.extension().is_some_and(|e| e == "pretty")).collect();
    libs.sort();
    for lib_dir in libs {
        let lib = lib_dir.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let Some(stock_lib) = lib.strip_prefix(PREFIX) else { continue };
        let mut mods: Vec<PathBuf> =
            read_dir(&lib_dir).filter(|p| p.extension().is_some_and(|e| e == "kicad_mod")).collect();
        mods.sort();
        for ours in mods {
            let name = ours.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let stock_path = mod_path(stock, stock_lib, &name);
            if !stock_path.is_file() {
                continue;
            }
            let outcome = match (std::fs::read_to_string(&ours), std::fs::read_to_string(&stock_path)) {
                (Err(e), _) => Compared::Error(format!("{}: {e}", ours.display())),
                (_, Err(e)) => Compared::Error(format!("{}: {e}", stock_path.display())),
                (Ok(o), Ok(s)) => match fresh(&s) {
                    Err(e) => Compared::Error(e),
                    Ok(fresh) if fresh == o => Compared::Same,
                    Ok(fresh) => {
                        let label = format!("{lib}.pretty/{name}.kicad_mod");
                        Compared::Differs(unified_diff(&label, &o, &line_edits(&o, &fresh)))
                    }
                },
            };
            out.push(Comparison { id: format!("{lib}:{name}"), ours, stock: stock_path, outcome });
        }
    }
    out
}

/// A fresh import for comparing: a footprint the designator rules cannot place (a logo without pads) is compared
/// with its stock text, the Value hidden where it has one.
fn fresh(stock: &str) -> Result<String, String> {
    match fab::fix(stock) {
        fab::Outcome::Skipped(_) if sexpr::parse(stock).is_ok() => {
            if stock.contains("(property \"Value\"") {
                hide_value(stock)
            } else {
                Ok(stock.to_string())
            }
        }
        _ => import_text(stock),
    }
}

fn read_dir(dir: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path())
}

/// The edits that turn `old` into `new`, line by line (a longest common subsequence of lines).
pub fn line_edits(old: &str, new: &str) -> Vec<Edit> {
    let a: Vec<&str> = old.split_inclusive('\n').collect();
    let b: Vec<&str> = new.split_inclusive('\n').collect();
    let (n, m) = (a.len(), b.len());
    // lcs[i][j]: the longest common subsequence of a[i..] and b[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut starts = Vec::with_capacity(n + 1);
    let mut at = 0;
    for l in &a {
        starts.push(at);
        at += l.len();
    }
    starts.push(at);
    let mut edits: Vec<Edit> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (i0, j0) = (i, j);
        while (i < n || j < m) && !(i < n && j < m && a[i] == b[j]) {
            if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
                j += 1;
            } else {
                i += 1;
            }
        }
        edits.push(Edit { range: starts[i0]..starts[i], text: b[j0..j].concat() });
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    const STOCK: &str = "(footprint \"R_0402\"\n\t(version 20260206)\n\t(layer \"F.Cu\")\n\t(property \"Reference\" \"REF**\"\n\t\t(at 0 -1.17 0)\n\t\t(layer \"F.SilkS\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n\t(property \"Value\" \"R_0402\"\n\t\t(at 0 1.17 0)\n\t\t(layer \"F.Fab\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 1 1)\n\t\t\t\t(thickness 0.15)\n\t\t\t)\n\t\t)\n\t)\n\t(fp_rect\n\t\t(start -0.5 -0.25)\n\t\t(end 0.5 0.25)\n\t\t(stroke\n\t\t\t(width 0.1)\n\t\t\t(type solid)\n\t\t)\n\t\t(layer \"F.Fab\")\n\t)\n\t(fp_text user \"${REFERENCE}\"\n\t\t(at 0 0 0)\n\t\t(layer \"F.Fab\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size 0.26 0.26)\n\t\t\t\t(thickness 0.04)\n\t\t\t)\n\t\t)\n\t)\n\t(pad \"1\" smd roundrect\n\t\t(at -0.485 0)\n\t\t(size 0.59 0.64)\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t)\n\t(pad \"2\" smd roundrect\n\t\t(at 0.485 0)\n\t\t(size 0.59 0.64)\n\t\t(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")\n\t)\n\t(model \"${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.step\"\n\t)\n)\n";

    #[test]
    fn import_moves_the_designator_and_hides_the_value() {
        let new = import_text(STOCK).unwrap();
        assert!(!new.contains("${REFERENCE}"), "{new}");
        assert!(new.contains("\t\t(layer \"F.Fab\")\n\t\t(hide yes)\n\t\t(effects"), "{new}");
        assert!(new.contains("(model \"${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402_1005Metric.step\""));
        let fp = fab::read(&new).unwrap();
        assert_eq!(fab::check(&fp), Vec::<String>::new());
        assert_eq!(fp.reference.as_ref().unwrap().layer, "F.Fab");
        // Everything from the pads on stays byte for byte.
        let tail = &STOCK[STOCK.find("\t(pad \"1\"").unwrap()..];
        assert!(new.ends_with(tail));
        assert_eq!(import_text(&new).unwrap(), new, "importing a copy again changes nothing");
    }

    #[test]
    fn hide_value_keeps_a_hidden_one_and_fixes_hide_no() {
        let t = "(footprint \"x\" (property \"Value\" \"x\" (at 0 0 0) (layer \"F.Fab\") (hide no)))";
        assert_eq!(
            hide_value(t).unwrap(),
            "(footprint \"x\" (property \"Value\" \"x\" (at 0 0 0) (layer \"F.Fab\") (hide yes)))"
        );
        let t = "(footprint \"x\" (property \"Value\" \"x\" (at 0 0 0) (layer \"F.Fab\")))";
        assert_eq!(
            hide_value(t).unwrap(),
            "(footprint \"x\" (property \"Value\" \"x\" (at 0 0 0) (layer \"F.Fab\") (hide yes)))"
        );
        let hidden = hide_value(t).unwrap();
        assert_eq!(hide_value(&hidden).unwrap(), hidden);
    }

    #[test]
    fn import_and_compare_in_folders() {
        let dir = crate::testdir::TestDir::new("stock");
        let stock = dir.path().join("stock");
        let fps = dir.path().join("lib/footprints");
        std::fs::create_dir_all(stock.join("Resistor_SMD.pretty")).unwrap();
        std::fs::create_dir_all(&fps).unwrap();
        std::fs::write(stock.join("Resistor_SMD.pretty/R_0402.kicad_mod"), STOCK).unwrap();

        let ids = vec!["Resistor_SMD:R_0402".to_string(), "Resistor_SMD:R_9999".to_string()];
        let plan = plan_import(&stock, &fps, &ids);
        assert!(matches!(plan[0].outcome, ImportOutcome::New { .. }));
        assert!(matches!(plan[1].outcome, ImportOutcome::Error(_)));
        assert!(apply_import(&plan).is_err(), "an error writes nothing");
        assert!(!fps.join("VHRD_Resistor_SMD.pretty").exists());

        let plan = plan_import(&stock, &fps, &ids[..1]);
        assert_eq!(apply_import(&plan).unwrap(), 1);
        let ours = fps.join("VHRD_Resistor_SMD.pretty/R_0402.kicad_mod");
        assert!(ours.is_file());
        assert_eq!(plan_import(&stock, &fps, &ids[..1])[0].outcome, ImportOutcome::Exists);

        let c = compare(&stock, &fps);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].id.as_str(), &c[0].outcome), ("VHRD_Resistor_SMD:R_0402", &Compared::Same));

        // Upstream widens the pads: the diff shows just that.
        std::fs::write(stock.join("Resistor_SMD.pretty/R_0402.kicad_mod"), STOCK.replace("0.59 0.64", "0.6 0.64"))
            .unwrap();
        let Compared::Differs(d) = &compare(&stock, &fps)[0].outcome else { panic!() };
        assert_eq!(d.lines().filter(|l| l.starts_with("-\t\t(size 0.59")).count(), 2, "{d}");
        assert_eq!(d.lines().filter(|l| l.starts_with("+\t\t(size 0.6 ")).count(), 2, "{d}");
    }

    #[test]
    fn line_edits_rebuild_the_new_text() {
        for (old, new) in [("a\nb\nc\n", "a\nx\nc\n"), ("a\nb\n", "a\nb\nc\n"), ("a\nb\nc\n", "c\n"), ("", "z\n")] {
            assert_eq!(apply_edits(old, &line_edits(old, new)), new, "{old:?} -> {new:?}");
        }
        assert_eq!(line_edits("a\nb\nc\nd\n", "a\nB\nc\nD\n").len(), 2);
    }
}
