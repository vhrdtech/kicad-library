//! Checks that KiCad boards resolve their libraries on this PC, and sets up a PC's KiCad config for
//! kicad-library.
//!
//! - [`check::check`]: every symbol, footprint, `Footprint` field and 3D model of the boards resolves through
//!   the library tables and path variables, the way KiCad resolves them.
//! - [`setup::plan`] and [`setup::apply`]: the global library tables and path variables for a kicad-library
//!   checkout.
//! - [`migrate::plan`] and [`migrate::apply`]: boards rewritten to a new library layout through a [`map::Map`],
//!   only the strings it matches.
//! - [`fab::plan`] and [`fab::apply`]: footprints whose designator lives only on the fab layer, centred on the body.
//! - [`layers::add_to_board`]: the user layers the designators of test points, net ties and mechanical parts
//!   live on, named in a board's layer table.
//! - [`stock::plan_import`] and [`stock::compare`]: KiCad's stock footprints copied into the checkout, and the
//!   copies compared with the current stock library.
//! - [`restructure::plan`] and [`restructure::apply`]: a checkout's old libraries moved into the map's new ones.

pub mod check;
pub mod config;
pub mod fab;
pub mod layers;
pub mod libtable;
pub mod map;
pub mod migrate;
pub mod restructure;
pub mod setup;
pub mod sexpr;
pub mod stock;
pub mod stroke;
pub mod vars;

#[doc(hidden)]
pub mod testdir;
