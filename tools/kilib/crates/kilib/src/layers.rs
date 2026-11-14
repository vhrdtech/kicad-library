//! The user layers kicad-library's footprints and templates use, and which footprint's designator goes where.
//!
//! F.Fab / B.Fab carry only the drawing of a part (outline, pin-1 and polarity marks, inner lines), no text. Real
//! parts carry their designator on Ref.Des; test points, net ties and mechanical or graphic footprints put theirs on
//! a layer of their own (Ref.Test, Ref.Tie, Ref.Mech), so a print shows one kind at a time: [`Class`] decides from
//! the footprint's library and name and the designator's prefix. Each such layer is a front/back pair of KiCad user
//! layers, typed `front` and `back` and numbered one after the other, so KiCad flips a bottom-side footprint's text
//! to the back one. One more pair, Rooms, holds hand-drawn outlines of the circuit blocks (no footprint uses it).
//! Three pairs more carry what a fab or assembly print may want on demand: Value (a real part's Value, centred on
//! the body like the designator, so Ref.Des and Value are shown one at a time), Pins (pin numbers next to the pads)
//! and Centroid (a cross on the placement origin). Two pairs show layout priority: Crit and Imp carry a real part's
//! `Layout_Critical` and `Layout_Important` notes, empty (so nothing shows) until the schematic sets one. A few of KiCad's own layers get a name of their own
//! ([`RENAMED`]). [`add_to_board`] names them in a `.kicad_pcb`'s layer table (and only there).

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
pub const LAYERS: [Layer; 20] = [
    user(9, "front", "Ref.Test.F", "User.9"),
    user(10, "back", "Ref.Test.B", "User.10"),
    user(11, "front", "Ref.Tie.F", "User.11"),
    user(12, "back", "Ref.Tie.B", "User.12"),
    user(13, "front", "Ref.Mech.F", "User.13"),
    user(14, "back", "Ref.Mech.B", "User.14"),
    user(15, "front", "Rooms.F", "User.15"),
    user(16, "back", "Rooms.B", "User.16"),
    user(17, "front", "Value.F", "User.17"),
    user(18, "back", "Value.B", "User.18"),
    user(19, "front", "Pins.F", "User.19"),
    user(20, "back", "Pins.B", "User.20"),
    user(21, "front", "Centroid.F", "User.21"),
    user(22, "back", "Centroid.B", "User.22"),
    user(23, "front", "Ref.Des.F", "User.23"),
    user(24, "back", "Ref.Des.B", "User.24"),
    user(25, "front", "Crit.F", "User.25"),
    user(26, "back", "Crit.B", "User.26"),
    user(27, "front", "Imp.F", "User.27"),
    user(28, "back", "Imp.B", "User.28"),
];

/// The Ref.Des pair: the designators of real parts, centred on the body ([`crate::fab`]).
pub const REF_DES: [&str; 2] = ["User.23", "User.24"];

/// The Value pair: a real part's Value, shown, centred on the body like its designator ([`crate::fab`]).
pub const VALUE: [&str; 2] = ["User.17", "User.18"];
/// The Pins pair: pin numbers beside the pads of connectors and multi-pin parts ([`crate::lint::pins`]).
pub const PINS: [&str; 2] = ["User.19", "User.20"];
/// The Centroid pair: a small cross on the footprint's placement origin ([`crate::lint::centroid`]).
pub const CENTROID: [&str; 2] = ["User.21", "User.22"];
/// The Crit pair: a real part's `Layout_Critical` note, set in the schematic for a part that needs care in layout
/// ([`crate::lint::layout`]).
pub const CRIT: [&str; 2] = ["User.25", "User.26"];
/// The Imp pair: a real part's `Layout_Important` note, the second level of layout priority.
pub const IMP: [&str; 2] = ["User.27", "User.28"];

/// The front or back layer of a pair.
pub fn side_of(pair: [&'static str; 2], back: bool) -> &'static str {
    pair[usize::from(back)]
}

/// Layers KiCad has anyway, given a name of their own (their type stays `user`): drawing dimensions on Dwgs.User,
/// notes for the fab on Cmts.User, the full top view of a part (a connector's body from its STEP model) on User.2.
pub const RENAMED: [Layer; 3] = [
    Layer { id: "Dwgs.User", ordinal: 17, side: "user", name: "Dimensions" },
    Layer { id: "Cmts.User", ordinal: 19, side: "user", name: "Fab.Notes" },
    user(2, "user", "Body.F", "User.2"),
];

/// The name KiCad gives a layer by itself: such a layer counts as unnamed.
fn default_name(id: &str) -> Option<&'static str> {
    match id {
        "Dwgs.User" => Some("User.Drawings"),
        "Cmts.User" => Some("User.Comments"),
        _ => None,
    }
}

/// Where a footprint's designator lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A real part (R C L D Q U Y LD SW, connectors): on Ref.Des, its Value on Value.
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
            Class::Part => side_of(REF_DES, back),
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
    /// A layer KiCad has anyway ([`RENAMED`]) that the board draws on itself (`items` graphics, texts or
    /// dimensions outside footprints): left unnamed, as that drawing may mean something else.
    InUse { layer: Layer, items: usize },
}

/// Board items (not those inside footprints) on `layer`.
fn board_items_on(pcb: &Node, layer: &str) -> usize {
    pcb.items()
        .iter()
        .filter(|n| n.head().is_some_and(|h| h.starts_with("gr_") || h == "dimension" || h == "zone"))
        .filter(|n| {
            n.value("layer") == Some(layer)
                || n.child("layers").is_some_and(|l| l.items().iter().skip(1).any(|a| a.as_atom() == Some(layer)))
        })
        .count()
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
    // KiCad 8 numbered the layers otherwise (B.Cu 31, User.1 50): its numbers would name the wrong layers.
    if entries.iter().any(|e| e.arg(0) == Some("B.Cu") && e.head() == Some("31")) {
        return Err("a board in KiCad 8's layer numbering (B.Cu is 31): open and save it in KiCad 10 first".into());
    }
    let start = text[..last.span().start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[start..last.span().start];
    let line = |l: &Layer| format!("({} {} {} {})", l.ordinal, sexpr::quote(l.id), l.side, sexpr::quote(l.name));
    let mut edits = Vec::new();
    let mut out = Vec::new();
    let mut added = String::new();
    for l in LAYERS.iter().chain(&RENAMED) {
        let found = entries.iter().find(|e| e.head() == Some(&l.ordinal.to_string()) || e.arg(0) == Some(l.id));
        let in_use = if l.id == "User.2" { board_items_on(&pcb, l.id) } else { 0 };
        match found {
            Some(e) if in_use > 0 && e.arg(2) != Some(l.name) => {
                out.push(LayerChange::InUse { layer: *l, items: in_use })
            }
            Some(e) => {
                let (side, name) = (e.arg(1), e.arg(2));
                if side == Some(l.side) && name == Some(l.name) {
                    out.push(LayerChange::Fine(*l));
                } else if name.is_none()
                    || name == Some(l.id)
                    || name == Some(l.name)
                    || name.is_some_and(|n| default_name(l.id) == Some(n))
                {
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
        assert_eq!((Class::Mech.layer(true), Class::Part.layer(true)), ("User.14", "User.24"));
    }

    #[test]
    fn layers_are_added_named_and_typed_only_in_the_table() {
        let board = "(kicad_pcb\n\t(version 20260206)\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t\t(39 \"User.1\" user)\n\t\t(55 \"User.9\" user)\n\t)\n\t(setup)\n)\n";
        let (new, changes) = add_to_board(board).unwrap();
        assert!(
            new.contains("\t\t(55 \"User.9\" front \"Ref.Test.F\")\n\t\t(57 \"User.10\" back \"Ref.Test.B\")\n"),
            "{new}"
        );
        assert!(new.contains("(69 \"User.16\" back \"Rooms.B\")\n\t\t(71 \"User.17\" front \"Value.F\")"), "{new}");
        assert!(
            new.contains("(81 \"User.22\" back \"Centroid.B\")\n\t\t(83 \"User.23\" front \"Ref.Des.F\")"),
            "{new}"
        );
        assert!(new.contains("(85 \"User.24\" back \"Ref.Des.B\")\n\t\t(87 \"User.25\" front \"Crit.F\")"), "{new}");
        assert!(new.contains("(91 \"User.27\" front \"Imp.F\")\n\t\t(93 \"User.28\" back \"Imp.B\")"), "{new}");
        assert!(new.contains("(41 \"User.2\" user \"Body.F\")\n\t)\n\t(setup)\n)\n"), "{new}");
        assert!(changes.iter().all(|c| matches!(c, LayerChange::Set(_))));
        let (again, changes) = add_to_board(&new).unwrap();
        assert_eq!(again, new);
        assert!(changes.iter().all(|c| matches!(c, LayerChange::Fine(_))));
        // KiCad's own layers under KiCad's names are renamed; under a name of the board's they are left alone.
        let own = board.replace(
            "(39 \"User.1\" user)",
            "(17 \"Dwgs.User\" user \"User.Drawings\")\n\t\t(19 \"Cmts.User\" user \"Notes\")",
        );
        let (new, changes) = add_to_board(&own).unwrap();
        assert!(new.contains("(17 \"Dwgs.User\" user \"Dimensions\")") && new.contains("\"Notes\""), "{new}");
        assert!(changes.iter().any(|c| matches!(c, LayerChange::Taken { layer, .. } if layer.id == "Cmts.User")));
        // A number used for something else is left alone and reported.
        let taken = board.replace("(55 \"User.9\" user)", "(55 \"User.9\" user \"Assembly\")");
        let (new, changes) = add_to_board(&taken).unwrap();
        assert!(new.contains("(55 \"User.9\" user \"Assembly\")") && !new.contains("Ref.Test.F"), "{new}");
        assert!(matches!(&changes[0], LayerChange::Taken { line, .. } if line.contains("Assembly")));
        // KiCad 8's numbers are not touched.
        let old = "(kicad_pcb\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t\t(31 \"B.Cu\" signal)\n\t\t(55 \"User.6\" user)\n\t)\n)\n";
        assert!(add_to_board(old).unwrap_err().contains("KiCad 8"));
        // User.2 with the board's own drawing on it stays unnamed.
        let drawn = board
            .replace("(39 \"User.1\" user)", "(41 \"User.2\" user)")
            .replace("\t(setup)", "\t(gr_line (start 0 0) (end 1 0) (layer \"User.2\"))\n\t(setup)");
        let (new, changes) = add_to_board(&drawn).unwrap();
        assert!(new.contains("(41 \"User.2\" user)\n"), "{new}");
        assert!(changes.iter().any(|c| matches!(c, LayerChange::InUse { items: 1, .. })));
    }
}
