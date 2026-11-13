//! IPC-7351B land patterns of two-terminal chip parts, computed the way KiCad's footprint generator computes its
//! stock chip lands (`ipc_body_edge_inside` with the `ipc7351B_2terminal` offsets, F = 0.1, P = 0.05), so a land
//! made here and a stock one agree for the same body:
//!
//! - Zmax = Lmin + 2 JT + √(CL² + F² + P²), the outer span;
//! - Gmin = Smax(RMS) − 2 JH − √(CS(RMS)² + F² + P²), the gap, with S = L − 2 T and its tolerance taken as RMS,
//!   CS(RMS) = √(CL² + 2 CT²), Smax(RMS) = Smax − (CS − CS(RMS)) / 2;
//! - Xmax = Wmin + 2 JS + √(CW² + F² + P²), the pad width;
//!
//! each rounded to its grid (0.05 mm from 0603 up, 0.02 mm below).

/// A toleranced size, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tol {
    pub min: f64,
    pub max: f64,
}

impl Tol {
    pub fn new(nominal: f64, plus: f64, minus: f64) -> Tol {
        Tol { min: nominal - minus, max: nominal + plus }
    }

    pub fn tol(&self) -> f64 {
        self.max - self.min
    }
}

/// The density level B (nominal) offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offsets {
    pub toe: f64,
    pub heel: f64,
    pub side: f64,
    /// Rounding grid of Z, G and X.
    pub grid: f64,
}

/// Fabrication tolerance F and placement tolerance P.
pub const F: f64 = 0.1;
pub const P: f64 = 0.05;

/// Level B offsets for a chip of `length_mm` (nominal body length): 0603 (1608 Metric) and up, or smaller.
pub fn chip_offsets(length_mm: f64) -> Offsets {
    if length_mm >= 1.5 {
        Offsets { toe: 0.35, heel: 0.0, side: 0.0, grid: 0.05 }
    } else {
        Offsets { toe: 0.2, heel: 0.0, side: 0.0, grid: 0.02 }
    }
}

/// A land of two pads: outer span Z, gap G and pad width X.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Land {
    pub span: f64,
    pub gap: f64,
    pub width: f64,
}

impl Land {
    pub fn pad_length(&self) -> f64 {
        (self.span - self.gap) / 2.0
    }

    /// The centre of pad 2 (pad 1 at minus that).
    pub fn pad_x(&self) -> f64 {
        (self.span + self.gap) / 4.0
    }
}

fn round_to(v: f64, grid: f64) -> f64 {
    // Through an integer count of grid steps, so 1.75 comes out as 1.75 and not 1.7500000000000002.
    let n = (v / grid).round();
    (n * grid * 1e6).round() / 1e6
}

/// The land of a chip body `length` x `width` with terminations `term` long at each end.
pub fn chip_land(length: Tol, width: Tol, term: Tol, o: Offsets) -> Land {
    let fp = F * F + P * P;
    let span = length.min + 2.0 * o.toe + (length.tol().powi(2) + fp).sqrt();
    let (s_min, s_max) = (length.min - 2.0 * term.max, length.max - 2.0 * term.min);
    let s_tol = s_max - s_min;
    let s_tol_rms = (length.tol().powi(2) + 2.0 * term.tol().powi(2)).sqrt();
    let s_max_rms = s_max - (s_tol - s_tol_rms) / 2.0;
    let gap = s_max_rms - 2.0 * o.heel - (s_tol_rms.powi(2) + fp).sqrt();
    let pad_width = width.min + 2.0 * o.side + (width.tol().powi(2) + fp).sqrt();
    Land { span: round_to(span, o.grid), gap: round_to(gap, o.grid), width: round_to(pad_width, o.grid) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// KiCad's stock lands from the generator's size definitions (resistor_chip.yaml).
    #[test]
    fn stock_chip_lands() {
        let t = |min: f64, max: f64| Tol { min, max };
        // R_1206_3216Metric: pads 1.125 x 1.75 at ±1.4625.
        let l = chip_land(t(3.05, 3.35), t(1.45, 1.75), t(0.25, 0.75), chip_offsets(3.2));
        assert_eq!(l, Land { span: 4.05, gap: 1.8, width: 1.75 });
        assert_eq!((l.pad_length(), l.pad_x()), (1.125, 1.4625));
        // R_0805_2012Metric: pads 1.025 x 1.4 at ±0.9125.
        let l = chip_land(t(1.85, 2.15), t(1.1, 1.4), t(0.15, 0.65), chip_offsets(2.0));
        assert_eq!(l, Land { span: 2.85, gap: 0.8, width: 1.4 });
    }
}
