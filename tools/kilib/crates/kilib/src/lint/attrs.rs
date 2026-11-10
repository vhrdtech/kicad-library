//! Footprint attributes by what the footprint is:
//!
//! - test points, net ties, spark gaps, jumpers, logos and other artwork, mounting holes: copper or artwork, never
//!   bought or placed, so `exclude_from_bom` and `exclude_from_pos_files`, and no SMD or through-hole type (KiCad's
//!   "unspecified", as its own libraries have them); net ties and artwork also `allow_missing_courtyard`;
//! - fiducials: the assembler aims at them, so in the position files, not in the BOM: `smd exclude_from_bom`;
//! - parts: SMD when every copper pad is SMD, through-hole when one is through-hole; a part kept out of the BOM or
//!   the position files is left as it is and reported;
//! - mechanical parts (screws, spacers, enclosures): only reported, each is its own case.
//!
//! `board_only` is never set: the boards place their logos, test points and holes from the schematic, and a
//! board-only footprint would drop out of it. `dnp` and other flags stay as they are.

use super::{Class, Input, Outcome, insert, pad_type};
use crate::footprint::{Attr, line_start};
use crate::migrate::Edit;

/// What the attributes should be; `None` where only a report fits.
fn wanted(i: &Input) -> Option<Attr> {
    let mut a = i.fp.attr.clone();
    a.span = None;
    let set = |a: &mut Attr, flag: &str, on: bool| {
        a.flags.retain(|f| f != flag);
        if on {
            a.flags.push(flag.to_string());
        }
    };
    match i.class {
        c if c.board_feature() => {
            a.kind = None;
            set(&mut a, "exclude_from_bom", true);
            set(&mut a, "exclude_from_pos_files", true);
            if matches!(c, Class::NetTie | Class::Graphic) {
                set(&mut a, "allow_missing_courtyard", true);
            }
        }
        Class::Fiducial => {
            a.kind = Some("smd".into());
            set(&mut a, "exclude_from_bom", true);
            set(&mut a, "exclude_from_pos_files", false);
        }
        Class::Part => a.kind = pad_type(i.fp).map(String::from).or(a.kind),
        _ => return None,
    }
    Some(a)
}

fn words(a: &Attr) -> String {
    a.render().map_or_else(|| "none".to_string(), |s| s.trim_start_matches("(attr ").trim_end_matches(')').to_string())
}

fn same(a: &Attr, b: &Attr) -> bool {
    let mut fa = a.flags.clone();
    let mut fb = b.flags.clone();
    fa.sort();
    fb.sort();
    a.kind == b.kind && fa == fb
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let have = &i.fp.attr;
    match i.class {
        Class::Part => {
            for flag in ["exclude_from_bom", "exclude_from_pos_files", "board_only"] {
                if have.has(flag) {
                    out.left(format!("a part with {flag}"));
                }
            }
            if pad_type(i.fp).is_none() {
                out.left("a part without copper pads: no SMD or through-hole type to check");
            }
        }
        Class::Mechanical => {
            out.left(format!("mechanical, attributes {} left as they are", words(have)));
            return out;
        }
        _ => {}
    }
    if i.fp.copper_pads().any(|p| p.kind == crate::footprint::PadKind::Connect) {
        out.left("pads of type connect (an edge connector's): counted as SMD");
    }
    let Some(want) = wanted(i) else { return out };
    if same(have, &want) {
        return out;
    }
    let text = want.render();
    out.fixed(format!("{} attributes {} → {}", i.class.label(), words(have), words(&want)));
    match (&have.span, text) {
        (Some(span), Some(t)) => out.edits.push(Edit { range: span.clone(), text: t }),
        (Some(span), None) => out.edits.push(super::delete(i.text, span)),
        (None, Some(t)) => {
            // KiCad writes the attributes after the properties, before the drawing.
            let at = i.fp.graphics.iter().map(|g| g.span.start).chain([i.fp.insert_at]).min().unwrap_or(i.fp.insert_at);
            let at = line_start(i.text, at);
            let mut e = insert(i.fp, format!("\t{t}\n"));
            e.range = at..at;
            out.edits.push(e);
        }
        (None, None) => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};

    const SMD: &str = r#"(pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask"))"#;
    const THT: &str = r#"(pad "2" thru_hole circle (at 2 0) (size 1 1) (drill 0.5) (layers "*.Cu" "*.Mask"))"#;

    #[test]
    fn board_features_leave_bom_and_pos() {
        let t = fp("TestPoint_Pad_D1mm", &format!("(attr smd)\n{SMD}"));
        let (new, f) = run("VHRD_TestPoint", &t, Rule::Attrs);
        let new = new.unwrap();
        assert!(new.contains("\t(attr exclude_from_pos_files exclude_from_bom)\n"), "{new}");
        assert!(f[0].fixed && f[0].message.contains("smd → exclude_from_pos_files exclude_from_bom"), "{f:?}");
        settled("VHRD_TestPoint", &t, Rule::Attrs);
        let t = fp("NetTie-2", SMD);
        let new = run("VHRD_NetTie", &t, Rule::Attrs).0.unwrap();
        assert!(
            new.contains("(attr exclude_from_pos_files exclude_from_bom allow_missing_courtyard)\n\t(pad"),
            "{new}"
        );
    }

    #[test]
    fn fiducials_stay_in_pos_files() {
        let t = fp("Fiducial_1mm", &format!("(attr exclude_from_pos_files exclude_from_bom)\n{SMD}"));
        let new = run("VHRD_Fiducial", &t, Rule::Attrs).0.unwrap();
        assert!(new.contains("(attr smd exclude_from_bom)"), "{new}");
    }

    #[test]
    fn part_type_follows_the_pads() {
        let t = fp("Conn", &format!("(attr smd dnp)\n{SMD}\n{THT}"));
        let new = run("VHRD_Connector", &t, Rule::Attrs).0.unwrap();
        assert!(new.contains("(attr through_hole dnp)"), "{new}");
        let t = fp("IC", &format!("(attr smd exclude_from_bom)\n{SMD}"));
        let (new, f) = run("VHRD_Package_SO", &t, Rule::Attrs);
        assert!(new.is_none());
        assert_eq!(f[0].message, "a part with exclude_from_bom");
        assert!(!f[0].fixed);
    }

    #[test]
    fn mechanical_is_only_reported() {
        let t = fp("M1.2_6", "(attr smd)");
        let (new, f) = run("VHRD_Mechanical", &t, Rule::Attrs);
        assert!(new.is_none());
        assert_eq!(f[0].message, "mechanical, attributes smd left as they are");
    }
}
