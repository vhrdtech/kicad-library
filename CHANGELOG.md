# Changelog

## [Unreleased]

### Changed

- The VHRD look from vhrd_brand's `vhrd-kicad` (P2534, 17 Oct 2026): `templates/vhrd_a4.kicad_wks` and
  `vhrd_a3.kicad_wks` replaced by the new drawing sheets (red edge bar, zone ring with red corner marks, open title
  block on the same fields: PRODUCT_NAME, BOARD_NUMBER, title, REVISION, COMMENT1-4, VHRD_STAMP); boards that use
  them by path get the new frame. `vhrd_power` symbols coloured: supplies orange, grounds (ISO, CHASSIS) grey. The
  "Vhrd Default" template embeds the new A3 sheet, its net classes carry the signal colours, and it has the
  VHRD_STAMP text variable.

### Added

- AGENTS.md, CLAUDE.md, FEATURES.md, CHANGELOG.md and a justfile (`look`, `test`, `lint`, `install`, `deploy`).
