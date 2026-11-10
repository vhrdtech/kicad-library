//! The user layers kicad-library's footprints and templates use, and which footprint's designator goes where.
//!
//! Real parts carry their designator on F.Fab / B.Fab. Test points, net ties and mechanical or graphic footprints
//! put theirs on a layer of their own, so the fab print of the parts stays readable: [`Class`] decides from the
//! footprint's library and name and the designator's prefix. Each such layer is a front/back pair of KiCad user
//! layers, typed `front` and `back` and numbered one after the other, so KiCad flips a bottom-side footprint's text
//! to the back one. One more pair, Rooms, holds hand-drawn outlines of the circuit blocks (no footprint uses it).
//! [`add_to_board`] names them in a `.kicad_pcb`'s layer table (and only there).

use crate::migrate::{Edit, apply_edits};
use crate::sexpr::{self, Node};

/// One user layer: KiCad's id (`User.9`), its ordinal in the board file, front or back, and the name it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    pub id: &'static str,
    pub ordinal: u32,
    pub side: &'static str,
    pub name: &'static str,
}

/// KiCad numbers user layer N as 39 + 2 (N - 1) in the board file.
const fn user(n: u32, side: &'static str, name: &'static str, id: &'static str) -> Layer {
    Layer { id, ordinal: 39 + 2 * (n - 1), side, name }
}

/// The layers, front then back of each pair.
pub const LAYERS: [Layer; 8] = [
    user(9, "front", "Ref.Test.F", "User.9"),
    user(10, "back", "Ref.Test.B", "User.10"),
    user(11, "front", "Ref.Tie.F", "User.11"),
    user(12, "back", "Ref.Tie.B", "User.12"),
    user(13, "front", "Ref.Mech.F", "User.13"),
    user(14, "back", "Ref.Mech.B", "User.14"),
    user(15, "front", "Rooms.F", "User.15"),
    user(16, "back", "Rooms.B", "User.16"),
];

/// Where a footprint's designator lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A real part (R C L D Q U Y LD SW, connectors): on F.Fab / B.Fab.
    Part,
    /// A test point (TP): on Ref.Test.
    Test,
    /// A net tie (NT), spark gap (E) or solder jumper (JP): on Ref.Tie.
    Tie,
    /// A mounting hole (H), fiducial (FID), logo or other graphic: on Ref.Mech, hidden.
    Mech,
}

impl Class {
    /// The layer the designator belongs on, on the front or the back.
    pub fn layer(self, back: bool) -> &'static str {
        let pair = |i: usize| LAYERS[i + usize::from(back)].id;
        match self {
            Class::Part => {
                if back {
                    "B.Fab"
                } else {
                    "F.Fab"
                }
            }
            Class::Test => pair(0),
            Class::Tie => pair(2),
            Class::Mech => pair(4),
        }
    }

    /// The designator is shown (a mechanical footprint's is kept, hidden).
    pub fn shown(self) -> bool {
        self != Class::Mech
    }
}

/// The letters a designator starts with (`TP` of `TP12`; `REF` of `REF**`).
pub fn prefix(reference: &str) -> &str {
    let end = reference.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(reference.len());
    &reference[..end]
}

/// The class of footprint `id` (`Lib:Name`, or a bare name) whose designator is `reference`: its library first
/// (VHRD_TestPoint, VHRD_NetTie, VHRD_Jumper, VHRD_MountingHole, VHRD_Mechanical, VHRD_Symbol and KiCad's stock
/// namesakes), then its name (`TestPoint...`, `NetTie...`, `SolderJumper...`, `MountingHole...`, `Fiducial...`),
/// then the designator's prefix (TP; NT, E; H, MH, FID).
pub fn class_of(id: &str, reference: &str) -> Class {
    let (lib, name) = id.rsplit_once(':').unwrap_or(("", id));
    let lib = lib.strip_prefix("VHRD_").unwrap_or(lib);
    let named = |p: &[&str]| p.iter().any(|p| name.starts_with(p));
    match lib {
        "TestPoint" => return Class::Test,
        "NetTie" | "Jumper" => return Class::Tie,
        "MountingHole" | "Mechanical" | "Symbol" | "Fiducial" => return Class::Mech,
        _ => {}
    }
    if named(&["TestPoint"]) {
        return Class::Test;
    }
    if named(&["NetTie", "SolderJumper", "SPARK_GAP"]) {
        return Class::Tie;
    }
    if named(&["MountingHole", "Fiducial"]) {
        return Class::Mech;
    }
    match prefix(reference) {
        "TP" => Class::Test,
        "NT" | "E" => Class::Tie,
        "H" | "MH" | "FID" => Class::Mech,
        _ => Class::Part,
    }
}

/// What [`add_to_board`] does to one layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerChange {
    /// Named and typed as it should be.
    Fine(Layer),
    /// Added to the table, or its line rewritten (it was there unnamed or untyped).
    Set(Layer),
    /// The board uses this number for something else (another name); left alone.
    Taken { layer: Layer, line: String },
}

/// The board text with [`LAYERS`] in its `(layers ...)` table, and what happened to each; only that table changes.
pub fn add_to_board(text: &str) -> Result<(String, Vec<LayerChange>), String> {
    let pcb = sexpr::parse(text).map_err(|e| e.to_string())?;
    if pcb.head() != Some("kicad_pcb") {
        return Err("not a (kicad_pcb ...) file".into());
    }
    let table = pcb.child("layers").ok_or("no (layers ...) table")?;
    let entries: Vec<&Node> = table.items().iter().skip(1).filter(|n| n.as_atom().is_none()).collect();
    let last = entries.last().ok_or("an empty (layers ...) table")?;
    let start = text[..last.span().start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[start..last.span().start];
    let line = |l: &Layer| format!("({} {} {} {})", l.ordinal, sexpr::quote(l.id), l.side, sexpr::quote(l.name));
    let mut edits = Vec::new();
    let mut out = Vec::new();
    let mut added = String::new();
    for l in &LAYERS {
        let found = entries.iter().find(|e| e.head() == Some(&l.ordinal.to_string()) || e.arg(0) == Some(l.id));
        match found {
            Some(e) => {
                let (side, name) = (e.arg(1), e.arg(2));
                if side == Some(l.side) && name == Some(l.name) {
                    out.push(LayerChange::Fine(*l));
                } else if name.is_none() || name == Some(l.id) || name == Some(l.name) {
                    edits.push(Edit { range: e.span(), text: line(l) });
                    out.push(LayerChange::Set(*l));
                } else {
                    out.push(LayerChange::Taken { layer: *l, line: text[e.span()].to_string() });
                }
            }
            None => {
                added += &format!("\n{indent}{}", line(l));
                out.push(LayerChange::Set(*l));
            }
        }
    }
    if !added.is_empty() {
        edits.push(Edit { range: last.span().end..last.span().end, text: added });
    }
    edits.sort_by_key(|e| e.range.start);
    Ok((apply_edits(text, &edits), out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_come_from_library_name_and_prefix() {
        assert_eq!(class_of("VHRD_Resistor_SMD:R_0402_1005Metric", "R806"), Class::Part);
        assert_eq!(class_of("VHRD_TestPoint:TestPoint_Pad_D1.0mm", "REF**"), Class::Test);
        assert_eq!(class_of("TestPoint_Pad_D1.0mm", "REF**"), Class::Test);
        assert_eq!(class_of("VHRD_NetTie:SPARK_GAP_011", "REF**"), Class::Tie);
        assert_eq!(class_of("SPARK_GAP_011", "REF**"), Class::Tie);
        assert_eq!(class_of("Jumper:SolderJumper-2_P1.3mm_Open_Pad1.0x1.5mm", "JP3"), Class::Tie);
        assert_eq!(class_of("VHRD_Symbol:VHRD_TECH_LOGO", "REF**"), Class::Mech);
        assert_eq!(class_of("Fiducial_1mm_Mask2mm", "FID1"), Class::Mech);
        assert_eq!(class_of("Some:Pad", "TP12"), Class::Test);
        assert_eq!(class_of("Some:Pad", "H3"), Class::Mech);
        assert_eq!(class_of("Some:Pad", "E1"), Class::Tie);
        assert_eq!(class_of("Connector:Pins", "JP3"), Class::Part, "a pin-header jumper is a part");
        assert_eq!(class_of("Some:Pad", "TPS1"), Class::Part);
        assert_eq!((Class::Test.layer(false), Class::Test.layer(true)), ("User.9", "User.10"));
        assert_eq!((Class::Mech.layer(true), Class::Part.layer(true)), ("User.14", "B.Fab"));
    }

    #[test]
    fn layers_are_added_named_and_typed_only_in_the_table() {
        let board = "(kicad_pcb\n\t(version 20260206)\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t\t(39 \"User.1\" user)\n\t\t(55 \"User.9\" user)\n\t)\n\t(setup)\n)\n";
        let (new, changes) = add_to_board(board).unwrap();
        assert!(
            new.contains("\t\t(55 \"User.9\" front \"Ref.Test.F\")\n\t\t(57 \"User.10\" back \"Ref.Test.B\")\n"),
            "{new}"
        );
        assert!(new.contains("(69 \"User.16\" back \"Rooms.B\")\n\t)\n\t(setup)\n)\n"), "{new}");
        assert!(changes.iter().all(|c| matches!(c, LayerChange::Set(_))));
        let (again, changes) = add_to_board(&new).unwrap();
        assert_eq!(again, new);
        assert!(changes.iter().all(|c| matches!(c, LayerChange::Fine(_))));
        // A number used for something else is left alone and reported.
        let taken = board.replace("(55 \"User.9\" user)", "(55 \"User.9\" user \"Assembly\")");
        let (new, changes) = add_to_board(&taken).unwrap();
        assert!(new.contains("(55 \"User.9\" user \"Assembly\")") && !new.contains("Ref.Test.F"), "{new}");
        assert!(matches!(&changes[0], LayerChange::Taken { line, .. } if line.contains("Assembly")));
    }
}
