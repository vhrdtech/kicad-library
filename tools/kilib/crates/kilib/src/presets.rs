//! Layer presets for the PCB Editor (`generate/layer-presets.toml`), written into a project's `.kicad_pro` as
//! `board.layer_presets`: KiCad's `LAYER_PRESET` (`include/project/board_project_settings.h` of KiCad 10), saved
//! by `PARAM_LAYER_PRESET` (`common/project/board_project_settings.cpp`) as `name`, `activeLayer` (a `PCB_LAYER_ID`
//! int), `flipBoard` (bool), `layers` (the visible board layers, `PCB_LAYER_ID` ints) and `renderLayers` (the
//! visible object types, as the strings of [`RENDER_LAYERS`], `VISIBILITY_LAYER` in
//! `common/settings/layer_settings_utils.cpp`). `.kicad_pro` is nlohmann JSON with sorted insertion and exact
//! number text kept on save; kilib's `serde_json` (`preserve_order`, `arbitrary_precision`) round-trips it the
//! same way, so only the `layer_presets` array of a rewrite differs from the original file.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::layers;

/// The file `kilib presets` reads by default, relative to a kicad-library checkout.
pub const TABLE_FILE: &str = "generate/layer-presets.toml";

/// One `[[preset]]` row.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub name: String,
    /// Board layers to show, by kilib's names or KiCad's own; the first becomes the active layer.
    pub layers: Vec<String>,
    /// Object types to show (of [`RENDER_LAYERS`]); default none.
    #[serde(default)]
    pub render: Vec<String>,
    /// View the board from below.
    #[serde(default)]
    pub flip: bool,
}

/// `generate/layer-presets.toml`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    #[serde(rename = "preset", default)]
    pub presets: Vec<Preset>,
}

impl Table {
    pub fn read(path: &Path) -> Result<Table, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// The object types the PCB Editor's Objects tab can show or hide, named as the project file does them
/// (`VISIBILITY_LAYER`, lower-cased).
pub const RENDER_LAYERS: &[&str] = &[
    "tracks",
    "vias",
    "pads",
    "zones",
    "shapes",
    "bitmaps",
    "footprints_front",
    "footprints_back",
    "footprint_values",
    "footprint_references",
    "footprint_text",
    "footprint_anchors",
    "ly_points",
    "ratsnest",
    "drc_warnings",
    "drc_errors",
    "drc_exclusions",
    "locked_item_shadows",
    "conflict_shadows",
    "board_outline_area",
    "drawing_sheet",
    "grid",
];

/// KiCad's own board layers, which need no name of ours (`PCB_LAYER_ID`, `include/layer_ids.h` of KiCad 10); the
/// `Courtyard` spelling is accepted alongside KiCad's `CrtYd`.
const STANDARD: &[(&str, i32)] = &[
    ("F.Cu", 0),
    ("F.Mask", 1),
    ("B.Cu", 2),
    ("B.Mask", 3),
    ("F.SilkS", 5),
    ("B.SilkS", 7),
    ("F.Adhes", 9),
    ("B.Adhes", 11),
    ("F.Paste", 13),
    ("B.Paste", 15),
    ("Dwgs.User", 17),
    ("Cmts.User", 19),
    ("Eco1.User", 21),
    ("Eco2.User", 23),
    ("Edge.Cuts", 25),
    ("Margin", 27),
    ("B.CrtYd", 29),
    ("B.Courtyard", 29),
    ("F.CrtYd", 31),
    ("F.Courtyard", 31),
    ("B.Fab", 33),
    ("F.Fab", 35),
];

/// Every layer name `kilib presets` resolves, and the `PCB_LAYER_ID` it means.
fn layer_ids() -> HashMap<&'static str, i32> {
    let mut ids: HashMap<&'static str, i32> = STANDARD.iter().copied().collect();
    for l in layers::LAYERS.iter().chain(layers::RENAMED.iter()) {
        ids.insert(l.name, l.ordinal as i32);
    }
    ids
}

/// The preset's JSON object, keys in the order KiCad itself writes them.
fn build(preset: &Preset, ids: &HashMap<&str, i32>) -> Result<Value, String> {
    let name = &preset.name;
    if preset.layers.is_empty() {
        return Err(format!("preset {name:?}: no layers"));
    }
    let mut layer_ids = Vec::with_capacity(preset.layers.len());
    for layer in &preset.layers {
        let id = *ids
            .get(layer.as_str())
            .ok_or_else(|| format!("preset {name:?}: layer {layer:?} is not one of kilib's or KiCad's own"))?;
        layer_ids.push(id);
    }
    for render in &preset.render {
        if !RENDER_LAYERS.contains(&render.as_str()) {
            return Err(format!("preset {name:?}: render layer {render:?} is not one of {RENDER_LAYERS:?}"));
        }
    }
    let active = layer_ids[0];
    layer_ids.sort_unstable();
    layer_ids.dedup();
    let mut obj = Map::new();
    obj.insert("activeLayer".to_string(), json!(active));
    obj.insert("flipBoard".to_string(), json!(preset.flip));
    obj.insert("layers".to_string(), json!(layer_ids));
    obj.insert("name".to_string(), json!(name));
    obj.insert("renderLayers".to_string(), json!(preset.render));
    Ok(Value::Object(obj))
}

/// What [`apply`] did to a project's presets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The table's presets added or changed.
    pub set: Vec<String>,
    /// The table's presets already matching.
    pub unchanged: Vec<String>,
    /// Presets of the project's own, left alone.
    pub kept: Vec<String>,
}

/// The custom layer names (of [`layers::LAYERS`]) the table's presets name.
fn named_layers_used(table: &Table) -> Vec<&str> {
    let custom: Vec<&str> = layers::LAYERS.iter().map(|l| l.name).collect();
    let mut used: Vec<&str> =
        table.presets.iter().flat_map(|p| p.layers.iter()).map(String::as_str).filter(|n| custom.contains(n)).collect();
    used.sort_unstable();
    used.dedup();
    used
}

/// `pro_text` (a `.kicad_pro`) with `table`'s presets set in `board.layer_presets`: presets of the same name
/// replaced, the project's own kept, in their existing order; the table's presets not already there appended.
/// `board_text`, the sibling `.kicad_pcb`'s, is checked for the user layers the table's presets name (`kilib
/// layers --write` first, else they would show the wrong layer or nothing); without a board, this is skipped.
pub fn apply(pro_text: &str, board_text: Option<&str>, table: &Table) -> Result<(String, Report), String> {
    if let Some(board_text) = board_text {
        let used = named_layers_used(table);
        let missing: Vec<&str> =
            layers::unnamed_in_board(board_text)?.into_iter().filter(|n| used.contains(n)).collect();
        if !missing.is_empty() {
            return Err(format!("the board lacks {}: run `kilib layers --write` on it first", missing.join(", ")));
        }
    }

    let mut root: Value = serde_json::from_str(pro_text).map_err(|e| e.to_string())?;
    let board = root.get_mut("board").and_then(Value::as_object_mut).ok_or("no \"board\" object")?;
    let existing = board.get("layer_presets").and_then(Value::as_array).cloned().unwrap_or_default();

    let ids = layer_ids();
    let mut built = Vec::with_capacity(table.presets.len());
    for preset in &table.presets {
        built.push((preset.name.as_str(), build(preset, &ids)?));
    }

    let mut out = Vec::with_capacity(existing.len() + built.len());
    let mut report = Report::default();
    let mut done = vec![false; built.len()];
    for item in &existing {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("");
        match built.iter().position(|(n, _)| *n == name) {
            Some(i) => {
                done[i] = true;
                if *item == built[i].1 {
                    report.unchanged.push(name.to_string());
                } else {
                    report.set.push(name.to_string());
                }
                out.push(built[i].1.clone());
            }
            None => {
                report.kept.push(name.to_string());
                out.push(item.clone());
            }
        }
    }
    for (i, (name, value)) in built.into_iter().enumerate() {
        if !done[i] {
            report.set.push(name.to_string());
            out.push(value);
        }
    }

    board.insert("layer_presets".to_string(), Value::Array(out));
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    Ok((text + "\n", report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        toml::from_str(
            r#"
            [[preset]]
            name = "Layout.F"
            layers = ["F.Cu", "F.Fab", "Crit.F", "Edge.Cuts"]
            render = ["pads", "vias"]
            flip = false
            "#,
        )
        .unwrap()
    }

    const PRO: &str = "{\n  \"board\": {\n    \"layer_presets\": []\n  },\n  \"meta\": {\n    \"version\": 1\n  }\n}\n";
    const BOARD: &str = "(kicad_pcb\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t\t(35 \"F.Fab\" user)\n\t\t(87 \"User.25\" front \"Crit.F\")\n\t)\n)\n";

    #[test]
    fn a_preset_is_built_with_the_active_layer_first_and_the_layers_sorted() {
        let ids = layer_ids();
        let preset = &table().presets[0];
        let value = build(preset, &ids).unwrap();
        assert_eq!(value["activeLayer"], json!(0));
        assert_eq!(value["flipBoard"], json!(false));
        assert_eq!(value["layers"], json!([0, 25, 35, 87]));
        assert_eq!(value["name"], json!("Layout.F"));
        assert_eq!(value["renderLayers"], json!(["pads", "vias"]));
    }

    #[test]
    fn an_unknown_layer_or_render_name_is_rejected() {
        let ids = layer_ids();
        let mut preset = table().presets[0].clone();
        preset.layers.push("F.NoSuch".into());
        assert!(build(&preset, &ids).unwrap_err().contains("F.NoSuch"));
        let mut preset = table().presets[0].clone();
        preset.render.push("no_such_render".into());
        assert!(build(&preset, &ids).unwrap_err().contains("no_such_render"));
    }

    #[test]
    fn apply_sets_keeps_and_reports_only_the_layer_presets_array() {
        let (new, report) = apply(PRO, Some(BOARD), &table()).unwrap();
        assert_eq!(report.set, vec!["Layout.F"]);
        assert!(report.unchanged.is_empty() && report.kept.is_empty());
        let root: Value = serde_json::from_str(&new).unwrap();
        assert_eq!(root["board"]["layer_presets"][0]["name"], json!("Layout.F"));
        assert_eq!(root["meta"]["version"], json!(1), "untouched content stays");

        // Applying again changes nothing.
        let (again, report) = apply(&new, Some(BOARD), &table()).unwrap();
        assert_eq!(again, new);
        assert_eq!(report.unchanged, vec!["Layout.F"]);
        assert!(report.set.is_empty());
    }

    #[test]
    fn the_projects_own_preset_is_kept_and_a_same_named_one_replaced() {
        let pro = PRO.replace(
            "\"layer_presets\": []",
            "\"layer_presets\": [{\"name\": \"Mine\"}, {\"name\": \"Layout.F\", \"flipBoard\": true}]",
        );
        let (new, report) = apply(&pro, Some(BOARD), &table()).unwrap();
        let root: Value = serde_json::from_str(&new).unwrap();
        let presets = root["board"]["layer_presets"].as_array().unwrap();
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0]["name"], json!("Mine"));
        assert_eq!(presets[1]["flipBoard"], json!(false), "replaced, not merged");
        assert_eq!(report.kept, vec!["Mine"]);
        assert_eq!(report.set, vec!["Layout.F"]);
    }

    #[test]
    fn a_board_missing_the_named_user_layer_is_refused() {
        let bare = "(kicad_pcb\n\t(layers\n\t\t(0 \"F.Cu\" signal)\n\t)\n)\n";
        let err = apply(PRO, Some(bare), &table()).unwrap_err();
        assert!(err.contains("Crit.F") && err.contains("kilib layers"), "{err}");
    }
}
