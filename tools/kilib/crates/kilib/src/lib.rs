//! Checks that KiCad boards resolve their libraries on this PC, and sets up a PC's KiCad config for
//! kicad-library.
//!
//! - [`check::check`]: every symbol, footprint, `Footprint` field and 3D model of the boards resolves through
//!   the library tables and path variables, the way KiCad resolves them.
//! - [`setup::plan`] and [`setup::apply`]: the global library tables and path variables for a kicad-library
//!   checkout.
//! - [`migrate::plan`] and [`migrate::apply`]: boards rewritten to a new library layout through a [`map::Map`],
//!   only the strings it matches.
//! - [`fab::plan`] and [`fab::apply`]: footprints whose designator lives only on the fab layer, centred on the body,
//!   and whose Value sits beside it on its own layer.
//! - [`layers::add_to_board`]: the user layers the library uses (designators of test points, net ties and
//!   mechanical parts, rooms, values, pin numbers, centroids), named in a board's layer table.
//! - [`stock::plan_import`] and [`stock::compare`]: KiCad's stock footprints copied into the checkout, and the
//!   copies compared with the current stock library; [`stock::symbol`] the same for its stock symbols.
//! - [`lint::plan`] and [`lint::apply`]: library-wide footprint rules (pin-1 marks, attributes, courtyards, 3D
//!   models, silkscreen, pin numbers, centroid crosses, layout-priority properties), each a check and, where the
//!   rule is clear, a fix.
//! - [`layout_fields::plan`] and [`layout_fields::apply`]: the empty `Layout_Critical` and `Layout_Important`
//!   fields added to library symbols and to the symbols placed in schematics.
//! - [`per_package::plan`] and [`per_package::apply`]: a symbol per package for generic symbols, derived from
//!   them with the footprint set, as the rename map's `[per_package]` names them; [`per_package::survey`] the
//!   generic symbols boards place, by footprint.
//! - [`restructure::plan`] and [`restructure::apply`]: a checkout's old libraries moved into the map's new ones.

pub mod check;
pub mod chip;
pub mod config;
pub mod fab;
pub mod footprint;
pub mod ipc;
pub mod layers;
pub mod layout_fields;
pub mod libtable;
pub mod lint;
pub mod map;
pub mod migrate;
pub mod per_package;
pub mod presets;
pub mod restructure;
pub mod setup;
pub mod sexpr;
pub mod step;
pub mod stock;
pub mod stroke;
pub mod vars;

#[doc(hidden)]
pub mod testdir;
