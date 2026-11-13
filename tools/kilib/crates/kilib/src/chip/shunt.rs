//! Metal-element current-sense resistors (`[[shunt]]` in `generate/current-sense.toml`): a chip body with the
//! maker's land, which is longer than the IPC land of a thick-film chip of the same size and changes with the
//! resistance range, because the terminations of a low-ohm part reach further under the body.
//!
//! A row is one size, one land and the heights the series that fit it are made in. The footprint is named by size,
//! height and land (`R_Shunt_1206_3216Metric_H0.6_Pad2.80x1.84mm_Gap0.60mm`), the model shows the metal element
//! under a black coat between long terminations.

use serde::Deserialize;

use crate::chip::{colour, mm, slab};
use crate::step::Solid;

/// One `[[shunt]]` row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shunt {
    /// EIA inch code, `1206`.
    pub size: String,
    /// Metric code as KiCad names it, `3216`.
    pub metric: String,
    /// Body length (along x) and width, nominal, mm.
    pub length: f64,
    pub width: f64,
    pub heights: Vec<f64>,
    /// Termination length at each end in the resistance range of this land, for the model.
    pub term: f64,
    /// One pad's length (along x) and width, and the gap between the two pads.
    pub pad_length: f64,
    pub pad_width: f64,
    pub gap: f64,
    /// The resistance range the land is for, as the maker writes it (`5–8 mΩ`).
    #[serde(default)]
    pub range: String,
    #[serde(default)]
    pub land_source: String,
    #[serde(default)]
    pub fits: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

impl Shunt {
    /// The land part of the names: `Pad2.80x1.84mm_Gap0.60mm`, two decimals as KiCad's `Pad` names write them.
    pub fn land(&self) -> String {
        format!("Pad{:.2}x{:.2}mm_Gap{:.2}mm", self.pad_length, self.pad_width, self.gap)
    }

    pub fn name(&self, h: f64) -> String {
        format!("R_Shunt_{}_{}Metric_H{}_{}", self.size, self.metric, mm(h), self.land())
    }

    /// The per-package symbol on `VHRD_Device:R`.
    pub fn symbol(&self, h: f64) -> String {
        format!("VHRD_Device:R_Shunt_{}_H{}_{}", self.size, mm(h), self.land())
    }

    /// The centre of pad 2 (pad 1 at minus that).
    pub fn pad_x(&self) -> f64 {
        (self.gap + self.pad_length) / 2.0
    }

    pub(crate) fn check(&self, h: f64) -> Result<(), String> {
        if !(self.length > 0.0 && self.width > 0.0 && h > 0.0) {
            return Err("length, width and height must be above 0".into());
        }
        if !(self.term > 0.0 && 2.0 * self.term < self.length) {
            return Err("term must be above 0 and the two terminations shorter than the body".into());
        }
        if !(self.pad_length > 0.0 && self.pad_width > 0.0 && self.gap > 0.0) {
            return Err("pad_length, pad_width and gap must be above 0".into());
        }
        Ok(())
    }

    pub fn description(&self, h: f64) -> String {
        let mut d = format!(
            "Current-sense resistor SMD {} ({} Metric), metal element, body {} x {} x {} mm, land {:.2} x {:.2} mm pads {:.2} mm apart",
            self.size,
            self.metric,
            mm(self.length),
            mm(self.width),
            mm(h),
            self.pad_length,
            self.pad_width,
            self.gap
        );
        if !self.range.is_empty() {
            d.push_str(&format!(" for {}", self.range));
        }
        if !self.land_source.is_empty() {
            d.push_str(&format!(", land from {}", self.land_source));
        }
        if !self.fits.is_empty() {
            d.push_str(&format!(", fits {}", self.fits.join(", ")));
        }
        d.push_str(", model by kilib model chip");
        d
    }
}

/// The model: the metal element as the body's lower part, a black coat over it between the terminations, and
/// tin terminations `term` long at both ends, wrapped round the full height.
pub(crate) fn model(s: &Shunt, h: f64) -> Vec<Solid> {
    let (l, w, t) = (s.length / 2.0, s.width, s.term);
    // The element shows as a band on the sides, under a coat of 0.4 of the height.
    let coat = 0.4 * h;
    vec![
        slab("element", (-l + t, l - t), w, (0.0, h - coat), colour::ELEMENT),
        slab("coat", (-l + t, l - t), w, (h - coat, h), colour::OVERCOAT),
        slab("terminal 1", (-l, -l + t), w, (0.0, h), colour::TIN),
        slab("terminal 2", (l - t, l), w, (0.0, h), colour::TIN),
    ]
}
