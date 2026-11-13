//! Metal-element current-sense resistors (`[[shunt]]` in `generate/current-sense.toml`).
//!
//! A low-ohm part's terminations reach far under the body, and how far depends on the resistance range, so the
//! IPC land of a thick-film chip of the same size leaves them half uncovered. The makers recommend their own
//! lands, but those carry extra copper out past the part (a heat-spreading pour baked into the footprint). Here
//! each row gives the real body and termination of one series and range, and the land is computed from it with
//! IPC-7351B nominal rules ([`crate::ipc`]): the terminations are covered, there is no extra toe copper. The
//! maker's land is kept in the description for reference.
//!
//! Rows of one size whose computed lands differ by 0.05 mm or less share one land (the smallest gap, the largest
//! span and width of them); their heights, merged where they are within 0.1 mm (the taller one kept), give one
//! footprint each: `R_Shunt_1206_3216Metric_H0.6_Pad1.78x1.85mm_Gap0.60mm`, named by size, height and land.

use serde::{Deserialize, Deserializer};

use crate::chip::{colour, mm, slab};
use crate::ipc::{self, Land, Tol};
use crate::step::Solid;

/// The smallest gap between the pads. IPC gives less where the termination tolerance is large against a short
/// body (0.10 mm for a 2 mm part with ±0.25 mm terminations), too thin to keep the solder of the two pads apart.
pub const MIN_GAP: f64 = 0.3;

/// A datasheet dimension: `[nominal, ±tol]` or `[nominal, +tol, -tol]`, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dim {
    pub nominal: f64,
    pub tol: Tol,
}

impl<'de> Deserialize<'de> for Dim {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Dim, D::Error> {
        let v = Vec::<f64>::deserialize(d)?;
        let (n, plus, minus) = match v[..] {
            [n, t] => (n, t, t),
            [n, p, m] => (n, p, m),
            _ => return Err(serde::de::Error::custom("a dimension is [nominal, tol] or [nominal, +tol, -tol]")),
        };
        if !(plus >= 0.0 && minus >= 0.0) {
            return Err(serde::de::Error::custom("tolerances are written without sign"));
        }
        Ok(Dim { nominal: n, tol: Tol::new(n, plus, minus) })
    }
}

/// The land a maker recommends, for the description.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MakerLand {
    pub pad_length: f64,
    pub pad_width: f64,
    pub gap: f64,
    pub source: String,
}

/// One `[[shunt]]` row: one series (or series of one body) in one size and resistance range.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shunt {
    /// The series the row is for, `Yageo PE1206`.
    pub series: Vec<String>,
    /// EIA inch code, `1206`.
    pub size: String,
    /// Metric code as KiCad names it, `3216`.
    pub metric: String,
    /// The resistance range, as the maker writes it (`5–8 mΩ`).
    pub range: String,
    /// Body length (along x) and width, termination length at each end.
    pub length: Dim,
    pub width: Dim,
    pub term: Dim,
    /// Body heights, nominal.
    pub heights: Vec<f64>,
    #[serde(default)]
    pub maker_land: Option<MakerLand>,
    #[serde(default)]
    pub sources: Vec<String>,
}

impl Shunt {
    /// The IPC-7351B nominal land of the row.
    /// The IPC-7351B nominal land of the row, its gap at least [`MIN_GAP`] (the pads shortened at the inner edge).
    pub fn land(&self) -> Land {
        let land =
            ipc::chip_land(self.length.tol, self.width.tol, self.term.tol, ipc::chip_offsets(self.length.nominal));
        Land { gap: land.gap.max(MIN_GAP), ..land }
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        let (l, w, t) = (self.length.nominal, self.width.nominal, self.term.nominal);
        if !(l > 0.0 && w > 0.0 && self.heights.iter().all(|&h| h > 0.0) && !self.heights.is_empty()) {
            return Err("length, width and heights must be above 0".into());
        }
        if !(t > 0.0 && 2.0 * t < l) {
            return Err("term must be above 0 and the two terminations shorter than the body".into());
        }
        let land = self.land();
        if !(land.gap > 0.0 && land.pad_length() > 0.0) {
            return Err(format!("the computed land has no gap or no pad: {land:?}"));
        }
        Ok(())
    }

    /// What the row adds to a description: `Yageo PE1206 5–8 mΩ (maker's land 2.80 x 1.84 mm, gap 0.60 mm)`.
    fn fits(&self) -> String {
        let mut s = format!("{} {}", self.series.join("/"), self.range);
        if let Some(m) = &self.maker_land {
            s.push_str(&format!(
                " (maker's land {:.2} x {:.2} mm, gap {:.2} mm, {})",
                m.pad_length, m.pad_width, m.gap, m.source
            ));
        }
        s
    }
}

/// One footprint: a size, one land, one height and the rows that fit it.
#[derive(Debug, Clone, PartialEq)]
pub struct ShuntPart {
    pub size: String,
    pub metric: String,
    pub land: Land,
    /// Every row of the land, for the description.
    pub rows: Vec<Shunt>,
    /// The row the model and the fab outline are drawn from: the tallest in this height, then the longest
    /// termination.
    pub model: Shunt,
}

/// A length in a name: two decimals, three where the third is not 0 (`1.80`, `1.775`).
fn dec(v: f64) -> String {
    let s = format!("{v:.3}");
    if s.ends_with('0') { format!("{v:.2}") } else { s }
}

impl ShuntPart {
    /// `Pad1.78x1.85mm_Gap0.60mm`.
    pub fn land_name(&self) -> String {
        format!("Pad{}x{}mm_Gap{}mm", dec(self.land.pad_length()), dec(self.land.width), dec(self.land.gap))
    }

    pub fn name(&self, h: f64) -> String {
        format!("R_Shunt_{}_{}Metric_H{}_{}", self.size, self.metric, mm(h), self.land_name())
    }

    /// The per-package symbol on `VHRD_Device:R`.
    pub fn symbol(&self, h: f64) -> String {
        format!("VHRD_Device:R_Shunt_{}_H{}_{}", self.size, mm(h), self.land_name())
    }

    pub fn description(&self, h: f64) -> String {
        let m = &self.model;
        let fits: Vec<String> = self.rows.iter().map(Shunt::fits).collect();
        format!(
            "Current-sense resistor SMD {} ({} Metric), metal element, body {} x {} x {} mm, IPC-7351B nominal land \
             from the body and terminations: pads {} x {} mm, gap {} mm; fits {}; model by kilib model chip",
            self.size,
            self.metric,
            mm(m.length.nominal),
            mm(m.width.nominal),
            mm(h),
            dec(self.land.pad_length()),
            dec(self.land.width),
            dec(self.land.gap),
            fits.join("; ")
        )
    }
}

/// Two lands that differ by 0.05 mm or less in span, gap and width.
fn close(a: &Land, b: &Land) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= 0.05 + 1e-9;
    near(a.span, b.span) && near(a.gap, b.gap) && near(a.width, b.width)
}

/// Heights merged where they are within 0.1 mm, the taller one kept: `[0.4, 0.45, 0.6, 0.64]` → `[0.45, 0.64]`.
pub fn merge_heights(mut hs: Vec<f64>) -> Vec<f64> {
    hs.sort_by(f64::total_cmp);
    let mut out: Vec<(f64, f64)> = Vec::new(); // (lowest, kept) of each group
    for h in hs {
        match out.last_mut() {
            Some((lo, kept)) if h - *lo <= 0.1 + 1e-9 => *kept = h,
            _ => out.push((h, h)),
        }
    }
    out.into_iter().map(|(_, h)| h).collect()
}

/// The footprints of the rows: rows grouped by size and close land, then one per merged height.
pub fn parts(rows: &[Shunt]) -> Result<Vec<(String, f64, ShuntPart)>, String> {
    let mut groups: Vec<(Land, Vec<Shunt>)> = Vec::new();
    for r in rows {
        r.check().map_err(|e| format!("[[shunt]] {} {} {}: {e}", r.series.join("/"), r.size, r.range))?;
        let land = r.land();
        match groups.iter_mut().find(|(l, g)| g[0].size == r.size && close(l, &land)) {
            Some((l, g)) => {
                // The shared land covers every row: the smallest gap, the largest span and width.
                *l = Land { span: l.span.max(land.span), gap: l.gap.min(land.gap), width: l.width.max(land.width) };
                g.push(r.clone());
            }
            None => groups.push((land, vec![r.clone()])),
        }
    }
    let mut out = Vec::new();
    for (land, g) in groups {
        let heights = merge_heights(g.iter().flat_map(|r| r.heights.iter().copied()).collect());
        for h in heights {
            // The row of this height: the tallest at or below it, the longest termination of those.
            let mut part = ShuntPart {
                size: g[0].size.clone(),
                metric: g[0].metric.clone(),
                land,
                rows: g.clone(),
                model: g[0].clone(),
            };
            part.model = part
                .rows_of(h)
                .max_by(|a, b| a.term.nominal.total_cmp(&b.term.nominal))
                .expect("a row of each merged height")
                .clone();
            out.push((part.name(h), h, part));
        }
    }
    Ok(out)
}

impl ShuntPart {
    /// The rows made in height `h` of this footprint (their height within 0.1 mm at or below it).
    pub fn rows_of(&self, h: f64) -> impl Iterator<Item = &Shunt> {
        self.rows.iter().filter(move |r| r.heights.iter().any(|&x| x <= h + 1e-9 && x > h - 0.1 - 1e-9))
    }
}

/// The model: the metal element as the body's lower part, a black coat over it between the terminations, and
/// tin terminations as long as the row's at both ends, wrapped round the full height.
pub(crate) fn model(p: &ShuntPart, h: f64) -> Vec<Solid> {
    let m = &p.model;
    let (l, w, t) = (m.length.nominal / 2.0, m.width.nominal, m.term.nominal);
    // The element shows as a band on the sides, under a coat of 0.4 of the height.
    let coat = 0.4 * h;
    vec![
        slab("element", (-l + t, l - t), w, (0.0, h - coat), colour::ELEMENT),
        slab("coat", (-l + t, l - t), w, (h - coat, h), colour::OVERCOAT),
        slab("terminal 1", (-l, -l + t), w, (0.0, h), colour::TIN),
        slab("terminal 2", (l - t, l), w, (0.0, h), colour::TIN),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_merge_upwards() {
        assert_eq!(merge_heights(vec![0.64, 0.4, 0.6, 0.45]), [0.45, 0.64]);
        assert_eq!(merge_heights(vec![0.3, 0.4, 0.5]), [0.4, 0.5]);
    }

    #[test]
    fn the_gap_is_never_below_the_minimum() {
        let row: Shunt = toml::from_str(
            r#"
series = ["Yageo PE0805"]
size = "0805"
metric = "2012"
range = "4 mΩ"
length = [2.0, 0.25]
width = [1.25, 0.25]
term = [0.7, 0.25]
heights = [0.6]
"#,
        )
        .unwrap();
        // IPC alone: span 2.95, gap 0.1.
        let land = row.land();
        assert_eq!((land.span, land.gap), (2.95, 0.3));
        assert_eq!(land.pad_length(), 1.325);
    }

    #[test]
    fn names_print_the_land() {
        assert_eq!(dec(1.775), "1.775");
        assert_eq!(dec(1.8), "1.80");
        assert_eq!(dec(0.6), "0.60");
    }
}
