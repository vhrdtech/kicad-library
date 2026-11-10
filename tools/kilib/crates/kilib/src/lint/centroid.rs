//! A cross on Centroid.F (User.21; Centroid.B, User.22, on the back) at the footprint's origin, the point the
//! position file gives for it, so a print of the layer over the board shows where the placement data puts each
//! part: two lines [`SIZE`] long, [`WIDTH`] wide, crossing at (0, 0). Only footprints that are placed get one
//! (parts and fiducials); test points, net ties, logos and holes, which stay out of the position files, get none.
//! The layer belongs to this rule: whatever else is there is replaced.

use super::{Class, Input, Outcome, delete, insert};
use crate::footprint::{Graphic, render_graphic};
use crate::layers::{CENTROID, side_of};

pub const SIZE: f64 = 0.5;
pub const WIDTH: f64 = 0.05;

/// The two arms: level, then upright.
fn arms() -> [[(f64, f64); 2]; 2] {
    let h = SIZE / 2.0;
    [[(-h, 0.0), (h, 0.0)], [(0.0, -h), (0.0, h)]]
}

fn is_arm(g: &Graphic, arm: &[(f64, f64); 2]) -> bool {
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
    g.kind == "fp_line"
        && (g.width - WIDTH).abs() < 1e-9
        && g.pts.len() == 2
        && ((near(g.pts[0], arm[0]) && near(g.pts[1], arm[1])) || (near(g.pts[0], arm[1]) && near(g.pts[1], arm[0])))
}

pub fn run(i: &Input) -> Outcome {
    let mut out = Outcome::default();
    let layer = side_of(CENTROID, i.fp.back);
    let placed = matches!(i.class, Class::Part | Class::Fiducial);
    let have: Vec<_> = i.fp.on(layer).collect();
    let fine = if placed {
        have.len() == 2 && arms().iter().all(|a| have.iter().any(|g| is_arm(g, a)))
    } else {
        have.is_empty()
    };
    if fine {
        return out;
    }
    let had = if have.is_empty() { String::new() } else { format!(" (replacing {})", have.len()) };
    if placed {
        out.fixed(format!("{layer}: a cross on the origin{had}"));
    } else {
        out.fixed(format!("{layer}: no cross, not placed{had}"));
    }
    for g in &have {
        out.edits.push(delete(i.text, &g.span));
    }
    if placed {
        let lines: String = arms().iter().map(|a| render_graphic("fp_line", a, WIDTH, false, layer)).collect();
        out.edits.push(insert(i.fp, lines));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Rule;
    use super::super::tests::{fp, run, settled};

    const PAD: &str = r#"(pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask"))"#;

    #[test]
    fn a_part_gets_a_cross_on_its_origin() {
        let t = fp("SOT-23", PAD);
        let (new, f) = run("VHRD_Package_TO_SOT_SMD", &t, Rule::Centroid);
        let new = new.unwrap();
        assert!(new.contains("(start -0.25 0)\n\t\t(end 0.25 0)\n\t\t(stroke\n\t\t\t(width 0.05)"), "{new}");
        assert!(new.contains("(start 0 -0.25)\n\t\t(end 0 0.25)") && new.matches("\"User.21\"").count() == 2, "{new}");
        assert_eq!(f[0].message, "User.21: a cross on the origin");
        settled("VHRD_Package_TO_SOT_SMD", &t, Rule::Centroid);
        // A wrong cross is replaced.
        let off = new.replacen("(start -0.25 0)", "(start -0.3 0)", 1);
        let (again, f) = run("VHRD_Package_TO_SOT_SMD", &off, Rule::Centroid);
        assert_eq!(again.unwrap(), new);
        assert!(f[0].message.ends_with("(replacing 2)"));
    }

    #[test]
    fn test_points_and_logos_get_none() {
        assert!(run("VHRD_TestPoint", &fp("TestPoint_Pad_D1.0mm", PAD), Rule::Centroid).0.is_none());
        assert!(run("VHRD_Symbol", &fp("VHRD_TECH_LOGO", PAD), Rule::Centroid).0.is_none());
        let fid = run("VHRD_Fiducial", &fp("Fiducial_1mm", PAD), Rule::Centroid).0.unwrap();
        assert!(fid.contains("User.21"));
    }
}
