//! Layout priority: every real part carries two properties, [`CRITICAL`] (`Layout_Critical`) on Crit.F (User.25;
//! Crit.B, User.26, on the back) and [`IMPORTANT`] (`Layout_Important`) on Imp.F (User.27; Imp.B, User.28),
//! visible, unlocked and empty. The schematic sets a note in the symbol field of the same name (`U101.5 VDD`,
//! `FB U301`) for a part that needs care in layout; Update PCB from Schematic carries it to the footprint, where it
//! shows on its own layer. Empty, nothing shows. Each hugs an edge of the real courtyard, clear of the designator's
//! own ink and of the other note, for the typical note [`TYPICAL`] ([`crate::fab::layout_note`], much smaller than
//! a designator or Value before it is reported with no clean fit: a short note reads fine small, and fitting it
//! beside the body matters more than its size); where the footprint draws no courtyard it falls back to the
//! centre, reported. On a board `kilib designators --board` fits the note that is set, the same way. Test points,
//! net ties, mechanical and graphic footprints (the classes of [`crate::layers::class_of`] other than a real part)
//! carry neither: a stray one is removed. A note set in the library footprint is left for a person: the note
//! belongs to the schematic.

use super::{Input, Outcome, delete};
use crate::fab;
use crate::layers::{CRIT, Class, IMP, side_of};
use crate::migrate::Edit;
use crate::sexpr;

pub const CRITICAL: &str = "Layout_Critical";
pub const IMPORTANT: &str = "Layout_Important";
/// Each property and the layer pair it lives on.
pub const FIELDS: [(&str, [&str; 2]); 2] = [(CRITICAL, CRIT), (IMPORTANT, IMP)];
/// The note a library footprint's properties are sized for: a pin of a large part.
pub const TYPICAL: &str = "U1001.48";

/// The whitespace the line of `at` starts with, a tab where something else is before it.
fn indent_at(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[start..at];
    if indent.trim().is_empty() { indent } else { "\t" }
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let Ok(root) = sexpr::parse(i.text) else { return out };
    let id = format!("{}:{}", i.lib, i.fp.name);
    let fp = fab::read_node(&root, &id, false);
    let part = fp.class == Class::Part;
    let props: Vec<_> = root.children("property").collect();
    let mut added = String::new();
    // Critical takes the courtyard's longer edge, Important the opposite one, each clear of the designator's own
    // ink and, for Important, of Critical once it is placed, so the two never overlap.
    let edges =
        fp.shape.as_ref().and_then(|s| s.courtyard).map(fab::note_edges).unwrap_or([fab::Edge::Top, fab::Edge::Bottom]);
    let mut avoid: Vec<fab::Rect> = fp
        .shape
        .as_ref()
        .filter(|_| part)
        .map(|s| {
            let d = fab::layout(s);
            s.text_rect(d.x, d.y, d.angle, d.size)
        })
        .into_iter()
        .collect();
    for (idx, (name, pair)) in FIELDS.into_iter().enumerate() {
        let node = props.iter().find(|n| n.arg(0) == Some(name));
        if !part {
            if let Some(n) = node {
                out.fixed(format!("{name}: removed, not a real part"));
                out.edits.push(delete(i.text, &n.span()));
            }
            continue;
        }
        let Some(shape) = &fp.shape else {
            out.left(format!("{name}: no fab outline, courtyard or pads to place it by"));
            continue;
        };
        let layer = side_of(pair, fp.back);
        let (p, ink, edge) = fab::layout_note(shape, &[TYPICAL.to_string()], edges[idx], &avoid);
        avoid.push(ink);
        let fallback = if edge.is_none() { ", centred: no room beside the body" } else { "" };
        match node {
            Some(n) => {
                let r = fab::read_property(n);
                if !r.value.is_empty() {
                    out.left(format!("{name}: {:?} set in the library (the schematic sets the note)", r.value));
                }
                let mut why = Vec::new();
                if r.layer != layer {
                    why.push(format!("on {}", r.layer));
                }
                if r.hidden {
                    why.push("hidden".into());
                }
                if !r.unlocked {
                    why.push("locked".into());
                }
                fab::justified(name, &r, fp.back, &mut why);
                fab::misplaced(name, &r, &p, &mut why);
                if why.is_empty() {
                    continue;
                }
                out.fixed(format!("{name}: {}{fallback}", why.join(", ")));
                let indent = indent_at(i.text, n.span().start);
                let text = fab::property_text(name, &r.value, r.uuid.as_deref(), layer, fp.back, false, &p, indent);
                out.edits.push(Edit { range: n.span(), text });
            }
            None => {
                out.fixed(format!("{name}: added on {layer}, empty{fallback}"));
                let at = props.last().map_or(i.fp.insert_at, |n| n.span().start);
                let indent = indent_at(i.text, at);
                added +=
                    &format!("\n{indent}{}", fab::property_text(name, "", None, layer, fp.back, false, &p, indent));
            }
        }
    }
    if !added.is_empty() {
        match props.last() {
            Some(n) => out.edits.push(Edit { range: n.span().end..n.span().end, text: added }),
            // No property to follow: whole lines before the first pad.
            None => out.edits.push(super::insert(i.fp, format!("{}\n", added.trim_start_matches('\n')))),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};

    const BODY: &str = r#"(property "Reference" "REF**" (at 0 0 0) (layer "User.23"))
        (property "Value" "SOIC-8" (at 0 0 0) (layer "User.17"))
        (fp_rect (start -2 -2.5) (end 2 2.5) (stroke (width 0.05) (type solid)) (layer "F.Fab"))
        (fp_rect (start -2.25 -2.75) (end 2.25 2.75) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))
        (pad "1" smd rect (at -2.5 -1.905) (size 1.5 0.6) (layers "F.Cu"))
        (pad "2" smd rect (at 2.5 -1.905) (size 1.5 0.6) (layers "F.Cu"))"#;
    /// The same body, no courtyard: a note has no edge to hug.
    const NO_COURTYARD: &str = r#"(property "Reference" "REF**" (at 0 0 0) (layer "User.23"))
        (property "Value" "SOIC-8" (at 0 0 0) (layer "User.17"))
        (fp_rect (start -2 -2.5) (end 2 2.5) (stroke (width 0.05) (type solid)) (layer "F.Fab"))
        (pad "1" smd rect (at -2.5 -1.905) (size 1.5 0.6) (layers "F.Cu"))
        (pad "2" smd rect (at 2.5 -1.905) (size 1.5 0.6) (layers "F.Cu"))"#;

    #[test]
    fn a_part_gets_both_properties_empty_on_their_layers() {
        let t = fp("SOIC-8", BODY);
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Layout);
        let new = new.unwrap();
        assert!(
            new.contains("(property \"Value\" \"SOIC-8\" (at 0 0 0) (layer \"User.17\"))\n\t(property \"Layout_Critical\" \"\"\n\t\t(at "),
            "{new}"
        );
        assert!(
            new.contains("(unlocked yes)\n\t\t(layer \"User.25\")\n\t\t(effects\n\t\t\t(font\n\t\t\t\t(size "),
            "{new}"
        );
        assert!(new.contains("(property \"Layout_Important\" \"\"") && new.contains("(layer \"User.27\")"), "{new}");
        assert!(!new.contains("(hide yes)"), "{new}");
        let msgs: Vec<_> = f.iter().map(|f| f.message.as_str()).collect();
        // The body (4 x 5) is taller than wide: Critical hugs the left edge of the courtyard, Important the right.
        assert_eq!(msgs, ["Layout_Critical: added on User.25, empty", "Layout_Important: added on User.27, empty"]);
        settled("VHRD_Package_SO", &t, Rule::Layout);
        // Moved or hidden, it is put back where it belongs, its note kept and left for a person.
        let moved = new.replacen("(layer \"User.25\")", "(layer \"F.Fab\")\n\t\t(hide yes)", 1).replacen(
            "\"Layout_Critical\" \"\"",
            "\"Layout_Critical\" \"U1.3\"",
            1,
        );
        let (again, f) = run("VHRD_Package_SO", &moved, Rule::Layout);
        assert_eq!(again.unwrap(), new.replacen("\"Layout_Critical\" \"\"", "\"Layout_Critical\" \"U1.3\"", 1));
        assert!(f.iter().any(|f| !f.fixed && f.message.contains("\"U1.3\" set in the library")), "{f:?}");
        assert!(f.iter().any(|f| f.fixed && f.message.starts_with("Layout_Critical: on F.Fab, hidden")), "{f:?}");
    }

    #[test]
    fn no_courtyard_falls_back_to_the_centre_and_says_so() {
        let t = fp("SOIC-8", NO_COURTYARD);
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Layout);
        assert!(new.is_some());
        let msgs: Vec<_> = f.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(
            msgs,
            [
                "Layout_Critical: added on User.25, empty, centred: no room beside the body",
                "Layout_Important: added on User.27, empty, centred: no room beside the body",
            ]
        );
        settled("VHRD_Package_SO", &t, Rule::Layout);
    }

    #[test]
    fn the_back_side_mirrors_onto_the_back_layers() {
        let t = fp("SOIC-8", &BODY.replace("F.Fab", "B.Fab").replace("F.Cu", "B.Cu"))
            .replace("(layer \"F.Cu\")", "(layer \"B.Cu\")");
        let (new, _) = run("VHRD_Package_SO", &t, Rule::Layout);
        let new = new.unwrap();
        assert!(new.contains("(layer \"User.26\")") && new.contains("(layer \"User.28\")"), "{new}");
        assert_eq!(new.matches("(justify mirror)").count(), 2, "{new}");
    }

    #[test]
    fn test_points_carry_none() {
        let tp = fp("TestPoint_Pad_D1.0mm", r#"(pad "1" smd circle (at 0 0) (size 1 1) (layers "F.Cu"))"#);
        assert!(run("VHRD_TestPoint", &tp, Rule::Layout).0.is_none());
        let stray = fp(
            "TestPoint_Pad_D1.0mm",
            r#"(property "Layout_Critical" "" (at 0 0 0) (layer "User.25"))
            (pad "1" smd circle (at 0 0) (size 1 1) (layers "F.Cu"))"#,
        );
        let (new, f) = run("VHRD_TestPoint", &stray, Rule::Layout);
        assert_eq!(new.unwrap(), tp);
        assert_eq!(f[0].message, "Layout_Critical: removed, not a real part");
    }
}
