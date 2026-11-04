//! Checks that KiCad boards resolve their libraries on this PC, and sets up a PC's KiCad config for
//! kicad-library.
//!
//! - [`check::check`]: every symbol, footprint, `Footprint` field and 3D model of the boards resolves through
//!   the library tables and path variables, the way KiCad resolves them.
//! - [`setup::plan`] and [`setup::apply`]: the global library tables and path variables for a kicad-library
//!   checkout.
//! - [`migrate::plan`] and [`migrate::apply`]: boards rewritten to a new library layout through a [`map::Map`],
//!   only the strings it matches.
//! - [`restructure::plan`] and [`restructure::apply`]: a checkout's old libraries moved into the map's new ones.

pub mod check;
pub mod config;
pub mod libtable;
pub mod map;
pub mod migrate;
pub mod restructure;
pub mod setup;
pub mod sexpr;
pub mod vars;

#[doc(hidden)]
pub mod testdir;
