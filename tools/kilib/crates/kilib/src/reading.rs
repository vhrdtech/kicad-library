//! The direction every text kilib places reads in: left to right (level, 0°) or top to bottom, never upside down or
//! bottom to top, as the PCB editor shows it from the side the text is on.
//!
//! KiCad stores a text's angle as it is on the board and draws it as written (kilib writes `(unlocked yes)`, so the
//! editor does not flip it back upright). Seen from the front a text at 0° reads left to right and one at 270° top
//! to bottom. A back text is mirrored (`(justify mirror)`) and the board seen from the back is flipped left to
//! right, KiCad's default view, where 0° still reads left to right and 90° reads top to bottom. A footprint's
//! rotation turns its texts with it, so on a board the angle is the footprint's plus the text's own.

/// The angle at which a vertical text reads top to bottom: 270° on the front, 90° on the back.
pub fn vertical(back: bool) -> f64 {
    if back { 90.0 } else { 270.0 }
}

/// The angle a text on the board at `angle` is written at to read left to right (a level one, 0° or 180°) or top to
/// bottom (a vertical one, 90° or 270°); any other angle is kept upright as KiCad draws it.
pub fn reading(angle: f64, back: bool) -> f64 {
    let a = angle.rem_euclid(180.0);
    if a < 1e-3 || 180.0 - a < 1e-3 {
        0.0
    } else if (a - 90.0).abs() < 1e-3 {
        vertical(back)
    } else {
        upright(angle)
    }
}

/// The angle KiCad draws a text at for the angle `a` when it keeps the text upright, in (-90°, 90°].
pub fn upright(a: f64) -> f64 {
    let a = a.rem_euclid(360.0);
    if a > 270.0 + 1e-6 {
        a - 360.0
    } else if a > 90.0 + 1e-6 {
        a - 180.0
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_reads_left_to_right_and_vertical_top_to_bottom_on_each_side() {
        for a in [0.0, 180.0, -180.0, 360.0] {
            assert_eq!((reading(a, false), reading(a, true)), (0.0, 0.0), "{a}");
        }
        for a in [90.0, 270.0, -90.0, 450.0] {
            assert_eq!((reading(a, false), reading(a, true)), (270.0, 90.0), "{a}");
        }
        assert_eq!(reading(45.0, false), 45.0);
        assert_eq!(reading(225.0, false), 45.0);
    }
}
