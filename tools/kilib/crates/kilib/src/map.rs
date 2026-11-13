//! The rename map of a library layout change (`migrate/2026-11.toml`): where every old symbol, footprint and 3D
//! model went, and where each new library lives.
//!
//! A symbol or footprint id `old_lib:name` is looked up in `[symbol]` / `[footprint]` first, then its library
//! in `[symbol_lib]` / `[footprint_lib]` (for items only a board still has). The name stays; only the library
//! changes. A footprint mapped to `""` was dropped. A model path is the part after `${VHRD_3DMODEL_DIR}/`.
//! `[filter_lib]` renames the library part of `ki_fp_filters` patterns (`"Connector*" = "VHRD_Connector*"`), for
//! libraries a filter names that no id of the map does.
//!
//! `[per_package."Lib:Generic"]` moves a placed generic symbol (one that leaves `Footprint` empty) to the symbol
//! made for its package, chosen by the footprint it is placed with: `"FpLib:Footprint" = "Lib:Generic_Package"`.
//! The target is a whole id in the generic's own library, derived from it (`kilib per-package`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

/// The variable every model path of the map is under.
pub const MODEL_VAR: &str = "${VHRD_3DMODEL_DIR}/";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Map {
    #[serde(default)]
    pub libraries: Libraries,
    #[serde(default)]
    pub symbol_lib: BTreeMap<String, String>,
    #[serde(default)]
    pub footprint_lib: BTreeMap<String, String>,
    /// The library part of a `ki_fp_filters` pattern, old to new; looked up before the footprint ids.
    #[serde(default)]
    pub filter_lib: BTreeMap<String, String>,
    #[serde(default)]
    pub symbol: BTreeMap<String, String>,
    #[serde(default)]
    pub footprint: BTreeMap<String, String>,
    #[serde(default)]
    pub model: BTreeMap<String, String>,
    #[serde(default)]
    pub model_dropped: ModelDropped,
    /// A generic symbol id, then its placed footprint id, to the per-package symbol id.
    #[serde(default)]
    pub per_package: BTreeMap<String, BTreeMap<String, String>>,
}

/// Each new library's URI, through `${VHRD_LIBRARY_DIR}` or `${VHRD_VENDOR_LIBRARY_DIR}`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Libraries {
    #[serde(default)]
    pub symbol: BTreeMap<String, String>,
    #[serde(default)]
    pub footprint: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDropped {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemKind {
    Symbol,
    Footprint,
}

impl ItemKind {
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Symbol => "symbol",
            ItemKind::Footprint => "footprint",
        }
    }
}

/// Where an old item or model went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The new library (for an item) or the new path under `${VHRD_3DMODEL_DIR}` (for a model).
    To(String),
    /// Listed with `""` (or in `[model_dropped]`): it has no new home.
    Dropped,
}

impl Map {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Parses a map and checks that every target library is one of `[libraries]`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let map: Map = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut unknown = BTreeSet::new();
        for (kind, items, libs) in [
            (ItemKind::Symbol, &map.symbol, &map.symbol_lib),
            (ItemKind::Footprint, &map.footprint, &map.footprint_lib),
        ] {
            let known = map.libraries(kind);
            for (key, lib) in items.iter().chain(libs.iter()) {
                if !lib.is_empty() && !known.contains_key(lib) {
                    unknown.insert(format!("{} {key} = {lib:?}", kind.label()));
                }
            }
            for key in items.keys() {
                if key.split_once(':').is_none_or(|(l, n)| l.is_empty() || n.is_empty()) {
                    unknown.insert(format!("{} {key:?} is not lib:name", kind.label()));
                }
            }
        }
        let is_id = |s: &str| s.split_once(':').is_some_and(|(l, n)| !l.is_empty() && !n.is_empty());
        for (generic, by_fp) in &map.per_package {
            if !is_id(generic) {
                unknown.insert(format!("per_package {generic:?} is not lib:name"));
                continue;
            }
            let lib = generic.split_once(':').map_or("", |(l, _)| l);
            for (fp, to) in by_fp {
                if !is_id(fp) {
                    unknown.insert(format!("per_package {generic} footprint {fp:?} is not lib:name"));
                } else if !is_id(to) || to == generic {
                    unknown.insert(format!("per_package {generic} {fp} = {to:?} is not another lib:name"));
                } else if to.split_once(':').is_some_and(|(l, _)| l != lib) {
                    unknown.insert(format!("per_package {generic} {fp} = {to:?}: not in {lib}, the generic's library"));
                } else if !map.libraries.symbol.contains_key(lib) {
                    unknown.insert(format!("per_package {generic}: {lib:?} not in [libraries.symbol]"));
                }
            }
        }
        if !unknown.is_empty() {
            let list: Vec<String> = unknown.into_iter().collect();
            return Err(format!("targets not in [libraries]: {}", list.join("; ")));
        }
        Ok(map)
    }

    pub fn libraries(&self, kind: ItemKind) -> &BTreeMap<String, String> {
        match kind {
            ItemKind::Symbol => &self.libraries.symbol,
            ItemKind::Footprint => &self.libraries.footprint,
        }
    }

    fn items(&self, kind: ItemKind) -> (&BTreeMap<String, String>, &BTreeMap<String, String>) {
        match kind {
            ItemKind::Symbol => (&self.symbol, &self.symbol_lib),
            ItemKind::Footprint => (&self.footprint, &self.footprint_lib),
        }
    }

    /// Where `lib:name` went: `[symbol]`/`[footprint]` first, then the library default. `None` when the map does
    /// not know it (another library, or one already moved).
    pub fn item(&self, kind: ItemKind, lib: &str, name: &str) -> Option<Target> {
        let (items, libs) = self.items(kind);
        let to = items.get(&format!("{lib}:{name}")).or_else(|| libs.get(lib))?;
        Some(if to.is_empty() { Target::Dropped } else { Target::To(to.clone()) })
    }

    /// Whether `lib` is an old library of the map (a key of the library defaults, or of an item).
    pub fn is_old_lib(&self, kind: ItemKind, lib: &str) -> bool {
        let (items, libs) = self.items(kind);
        libs.contains_key(lib) || items.keys().any(|k| k.split_once(':').is_some_and(|(l, _)| l == lib))
    }

    /// Every new library an old one feeds: its items' targets and its default.
    pub fn targets_of(&self, kind: ItemKind, lib: &str) -> BTreeSet<String> {
        let (items, libs) = self.items(kind);
        let mut out: BTreeSet<String> = items
            .iter()
            .filter(|(k, _)| k.split_once(':').is_some_and(|(l, _)| l == lib))
            .map(|(_, v)| v.clone())
            .collect();
        out.extend(libs.get(lib).cloned());
        out.retain(|l| !l.is_empty());
        out
    }

    /// Where a model path (after `${VHRD_3DMODEL_DIR}/`) went.
    pub fn model(&self, path: &str) -> Option<Target> {
        if let Some(to) = self.model.get(path) {
            return Some(Target::To(to.clone()));
        }
        self.model_dropped.paths.iter().any(|p| p == path).then_some(Target::Dropped)
    }

    /// The per-package symbol for generic symbol `id` placed with footprint `fp`.
    pub fn per_package(&self, id: &str, fp: &str) -> Option<&str> {
        self.per_package.get(id)?.get(fp).map(String::as_str)
    }

    /// Whether `path` is already one of the new model paths.
    pub fn is_new_model(&self, path: &str) -> bool {
        self.model.values().any(|v| v == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"
[libraries.symbol]
"VHRD_Device" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir"
"VHRD_Switch" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Switch.kicad_symdir"
[libraries.footprint]
"VHRD_Diode_SMD" = "${VHRD_LIBRARY_DIR}/footprints/VHRD_Diode_SMD.pretty"
[symbol_lib]
"vhrd_device" = "VHRD_Device"
[footprint_lib]
"vhrd_diode" = "VHRD_Diode_SMD"
[symbol]
"vhrd_device:SW" = "VHRD_Switch"
[footprint]
"vhrd_diode:GONE" = ""
[model]
"diodes/a.STEP" = "vendor/VHRD_Diode_SMD.3dshapes/a.step"
[model_dropped]
paths = ["diodes/a.SLDPRT"]
"#;

    #[test]
    fn looks_up_items_then_libraries() {
        let map = Map::parse(MAP).unwrap();
        let s = |l, n| map.item(ItemKind::Symbol, l, n);
        assert_eq!(s("vhrd_device", "SW"), Some(Target::To("VHRD_Switch".into())));
        assert_eq!(s("vhrd_device", "R"), Some(Target::To("VHRD_Device".into())));
        assert_eq!(s("VHRD_Device", "R"), None);
        assert_eq!(map.item(ItemKind::Footprint, "vhrd_diode", "GONE"), Some(Target::Dropped));
        assert_eq!(map.targets_of(ItemKind::Symbol, "vhrd_device").len(), 2);
        assert!(map.is_old_lib(ItemKind::Footprint, "vhrd_diode"));
        assert!(!map.is_old_lib(ItemKind::Symbol, "vhrd_diode"));
        assert_eq!(map.model("diodes/a.SLDPRT"), Some(Target::Dropped));
        assert!(map.is_new_model("vendor/VHRD_Diode_SMD.3dshapes/a.step"));
    }

    #[test]
    fn refuses_targets_without_a_library() {
        let bad = MAP.replace("\"vhrd_device:SW\" = \"VHRD_Switch\"", "\"vhrd_device:SW\" = \"VHRD_Nope\"");
        let e = Map::parse(&bad).unwrap_err();
        assert!(e.contains("symbol vhrd_device:SW = \"VHRD_Nope\""), "{e}");
        assert!(Map::parse("[symbols]\n").is_err(), "unknown sections are refused");
    }

    #[test]
    fn per_package_targets_stay_in_the_generic_library() {
        let ok =
            format!("{MAP}[per_package.\"VHRD_Device:R\"]\n\"VHRD_Resistor_SMD:R_0402\" = \"VHRD_Device:R_0402\"\n");
        let map = Map::parse(&ok).unwrap();
        assert_eq!(map.per_package("VHRD_Device:R", "VHRD_Resistor_SMD:R_0402"), Some("VHRD_Device:R_0402"));
        assert_eq!(map.per_package("VHRD_Device:R", "VHRD_Resistor_SMD:R_0603"), None);
        let e = Map::parse(&ok.replace("= \"VHRD_Device:R_0402\"", "= \"VHRD_Switch:R_0402\"")).unwrap_err();
        assert!(e.contains("not in VHRD_Device, the generic's library"), "{e}");
        let e = Map::parse(&ok.replace("= \"VHRD_Device:R_0402\"", "= \"R_0402\"")).unwrap_err();
        assert!(e.contains("is not another lib:name"), "{e}");
    }
}
