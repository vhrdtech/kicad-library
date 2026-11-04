//! Shared by the migrate tests: a small map, and a check, independent of the rewriter, that two versions of a
//! file differ only in strings the map matches.

#![allow(dead_code)]

use kilib::libtable::{LibTable, TableKind};
use kilib::map::{ItemKind, MODEL_VAR, Map, Target};
use kilib::sexpr::{self, Node};
use std::path::Path;

/// A map with every kind of entry: item lines, library defaults, a split library, a dropped footprint, models.
pub const MAP: &str = r#"
[libraries.symbol]
"VHRD_Device" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Device.kicad_symdir"
"VHRD_Switch" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Switch.kicad_symdir"
"VHRD_Simulation_SPICE" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Simulation_SPICE.kicad_symdir"
"VHRD_Connector" = "${VHRD_LIBRARY_DIR}/symbols/VHRD_Connector.kicad_symdir"
"VHRD_Vendor" = "${VHRD_VENDOR_LIBRARY_DIR}/symbols/VHRD_Vendor.kicad_symdir"
[libraries.footprint]
"VHRD_Package_SO" = "${VHRD_LIBRARY_DIR}/footprints/VHRD_Package_SO.pretty"
"VHRD_Package_TO_SOT_SMD" = "${VHRD_LIBRARY_DIR}/footprints/VHRD_Package_TO_SOT_SMD.pretty"
"VHRD_Connector_USB" = "${VHRD_LIBRARY_DIR}/footprints/VHRD_Connector_USB.pretty"
"VHRD_Vendor" = "${VHRD_VENDOR_LIBRARY_DIR}/footprints/VHRD_Vendor.pretty"

[symbol_lib]
"vhrd_device" = "VHRD_Device"
"vhrd_sim" = "VHRD_Simulation_SPICE"
"vhrd_connector" = "VHRD_Connector"

[footprint_lib]
"vhrd_generic" = "VHRD_Package_TO_SOT_SMD"
"vhrd_connector_usb" = "VHRD_Connector_USB"

[symbol]
"vhrd_device:R" = "VHRD_Device"
"vhrd_device:SW_Push" = "VHRD_Switch"
"vhrd_device:SW_Push_Dual" = "VHRD_Device"
"vhrd_device:CHS-01TB" = "VHRD_Vendor"
"vhrd_connector:Hirose BM50U-4DP/2-0.35V(53)" = "VHRD_Connector"

[footprint]
"vhrd_generic:SOT-23" = "VHRD_Package_TO_SOT_SMD"
"vhrd_generic:SOIC-8" = "VHRD_Package_SO"
"vhrd_generic:SO-14" = "VHRD_Package_SO"
"vhrd_generic:CHS-01TB" = "VHRD_Vendor"
"vhrd_generic:_TEMPLATE" = ""
"vhrd_connector_usb:USB4105" = "VHRD_Connector_USB"

[model]
"sot/SOT-23.step" = "vendor/VHRD_Package_TO_SOT_SMD.3dshapes/SOT-23.step"
"ic/SOIC 8.STEP" = "vendor/VHRD_Package_SO.3dshapes/SOIC_8.step"

[model_dropped]
paths = ["switches/CHS-01TB.SLDPRT"]
"#;

pub fn map() -> Map {
    Map::parse(MAP).unwrap()
}

/// Asserts that `new` is `old` with only mapped strings changed: the same tree, the same bytes between atoms,
/// and every atom that differs a mapped pair. Library tables: every row that is not an old library's is kept
/// as it was, and every new row is a library of the map.
pub fn assert_only_mapped(map: &Map, path: &Path, old: &str, new: &str) {
    let name = path.file_name().unwrap().to_str().unwrap();
    if name == "sym-lib-table" || name == "fp-lib-table" {
        let (kind, item) = if name == "sym-lib-table" {
            (TableKind::Symbol, ItemKind::Symbol)
        } else {
            (TableKind::Footprint, ItemKind::Footprint)
        };
        let a = LibTable::parse(kind, path, old).unwrap();
        let b = LibTable::parse(kind, path, new).unwrap();
        for row in a.rows.iter().filter(|r| !map.is_old_lib(item, &r.name)) {
            assert!(new.contains(&old[row.span.clone()]), "{}: row {} changed", path.display(), row.name);
        }
        for row in b.rows.iter().filter(|r| a.find(&r.name).is_none()) {
            assert_eq!(map.libraries(item).get(&row.name), Some(&row.uri), "{}: new row {}", path.display(), row.name);
        }
        return;
    }
    let a = sexpr::parse(old).unwrap();
    let b = sexpr::parse(new).unwrap();
    assert_eq!(gaps(old, &a), gaps(new, &b), "{}: bytes between atoms changed", path.display());
    same_tree(map, path, &a, &b);
}

/// The text with every atom taken out: what must stay byte for byte.
fn gaps(text: &str, root: &Node) -> String {
    let mut spans = Vec::new();
    atoms(root, &mut spans);
    let mut out = String::new();
    let mut at = 0;
    for s in spans {
        out.push_str(&text[at..s.start]);
        out.push('\u{0}');
        at = s.end;
    }
    out.push_str(&text[at..]);
    out
}

fn atoms(n: &Node, out: &mut Vec<std::ops::Range<usize>>) {
    match n {
        Node::Atom { span, .. } => out.push(span.clone()),
        Node::List { items, .. } => items.iter().for_each(|i| atoms(i, out)),
    }
}

fn same_tree(map: &Map, path: &Path, a: &Node, b: &Node) {
    match (a, b) {
        (Node::List { items: x, .. }, Node::List { items: y, .. }) => {
            assert_eq!(x.len(), y.len(), "{}: list shape changed", path.display());
            x.iter().zip(y).for_each(|(p, q)| same_tree(map, path, p, q));
        }
        (Node::Atom { text: x, .. }, Node::Atom { text: y, .. }) => {
            if x != y {
                assert!(mapped(map, x, y), "{}: {x:?} became {y:?}, which the map does not say", path.display());
            }
        }
        _ => panic!("{}: an atom became a list", path.display()),
    }
}

/// Whether `old` → `new` is a rewrite the map stands for.
fn mapped(map: &Map, old: &str, new: &str) -> bool {
    let id = |o: &str, n: &str| {
        let (Some((ol, on)), Some((nl, nn))) = (o.split_once(':'), n.split_once(':')) else { return false };
        on == nn
            && [ItemKind::Symbol, ItemKind::Footprint]
                .iter()
                .any(|&k| map.item(k, ol, on) == Some(Target::To(nl.to_string())))
    };
    if id(old, new) {
        return true;
    }
    if let Some(rel) = new.strip_prefix(MODEL_VAR) {
        let from = old.strip_prefix(MODEL_VAR).or_else(|| old.split_once("kicad-library/3dmodels/").map(|(_, r)| r));
        if from.is_some_and(|f| map.model(f) == Some(Target::To(rel.to_string()))) {
            return true;
        }
    }
    // Footprint filters: the same patterns, each the same or with its library mapped (or dropped).
    let (x, y): (Vec<&str>, Vec<&str>) = (old.split_whitespace().collect(), new.split_whitespace().collect());
    x.len() == y.len()
        && x.iter().zip(&y).all(|(o, n)| {
            o == n
                || id(o, n)
                || o.split_once(':').is_some_and(|(l, p)| {
                    map.is_old_lib(ItemKind::Footprint, l) && {
                        let lib_of = n.split_once(':').map(|(l, _)| l);
                        let pat = n.split_once(':').map_or(*n, |(_, p)| p);
                        pat == p && lib_of.is_none_or(|nl| map.libraries(ItemKind::Footprint).contains_key(nl))
                    }
                })
        })
}
