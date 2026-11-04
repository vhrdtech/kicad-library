# Changelog

## [Unreleased]

### Added

- `tools/kilib`, a small Rust CLI (MIT): `kilib check PATH...` resolves every symbol, `Footprint` field, footprint and
  3D model of the boards the way KiCad does (global and project library tables, `kicad_common.json` path variables and
  the environment, `${KIPRJMOD}`, `KICAD6..10_3DMODEL_DIR`, `VHRD_3DMODEL_DIR`, stock `.kicad_symdir` libraries,
  nested tables) and exits 1 with the list of what does not resolve (`--json` for scripts). `kilib setup` writes the
  library rows through `${VHRD_LIBRARY_DIR}` and `${VHRD_VENDOR_LIBRARY_DIR}` into this PC's KiCad config, from a
  rename map's `[libraries.*]` tables (`--map`) or the library folders found, in place of the old `vhrd_*` rows; it
  also sets `VHRD_LIBRARY_DIR`, `VHRD_VENDOR_LIBRARY_DIR`, `VHRD_3DMODEL_DIR` and the template variables. `--dry-run`
  only shows it; it refuses while KiCad runs.
- `just check-boards`: `kilib check` on the hw checkout beside this one (or `VHRD_BOARDS_DIR`), skipped with a note
  where there is none.

### Changed

- `just test` also runs kilib's tests (`just test --changed`: kilib's changed crates only), `just lint` checks its
  format and clippy, and `just install` builds and installs `kilib`.

## [0.1.0] - 2026-10-17

### Added

- AGENTS.md, CLAUDE.md, FEATURES.md, CHANGELOG.md and a justfile (`look`, `test`, `lint`, `install`, `deploy`).

### Changed

- The VHRD look from vhrd_brand's `vhrd-kicad` (P2534, 17 Oct 2026): `templates/vhrd_a4.kicad_wks` and
  `vhrd_a3.kicad_wks` replaced by the new drawing sheets (red edge bar, zone ring with red corner marks, open title
  block on the same fields: PRODUCT_NAME, BOARD_NUMBER, title, REVISION, COMMENT1-4, VHRD_STAMP); boards that use
  them by path get the new frame. `vhrd_power` symbols coloured: supplies orange, grounds (ISO, CHASSIS) grey. The
  "Vhrd Default" template embeds the new A3 sheet, its net classes carry the signal colours, and it has the
  VHRD_STAMP text variable.
