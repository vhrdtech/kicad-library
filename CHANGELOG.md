# Changelog

## [Unreleased]

### Added

- `migrate/2026-11.toml`: where every symbol, footprint and 3D model of the old layout goes in the new kind-first one
  (`VHRD_<KiCad library>`), for `kilib migrate` to rewrite boards with.
- `tools/kilib`, a small Rust CLI (MIT): `kilib check PATH...` resolves every symbol, `Footprint` field, footprint and
  3D model of the boards the way KiCad does (global and project library tables, `kicad_common.json` path variables and
  the environment, `${KIPRJMOD}`, `KICAD6..10_3DMODEL_DIR`, `VHRD_3DMODEL_DIR`, stock `.kicad_symdir` libraries,
  nested tables) and exits 1 with the list of what does not resolve (`--json` for scripts). `kilib setup` writes the
  library rows through `${VHRD_LIBRARY_DIR}` and `${VHRD_VENDOR_LIBRARY_DIR}` into this PC's KiCad config, from a
  rename map's `[libraries.*]` tables (`--map`) or the library folders found, in place of the old `vhrd_*` rows; it
  also sets `VHRD_LIBRARY_DIR`, `VHRD_VENDOR_LIBRARY_DIR`, `VHRD_3DMODEL_DIR` and the template variables. `--dry-run`
  only shows it; it refuses while KiCad runs.
- `kilib migrate --map FILE PATH...`: rewrites boards to a new library layout, changing only the strings the map
  matches and every other byte as it was: symbol ids (`lib_id`, `lib_name`, the `lib_symbols` cache), `Footprint`
  fields, `ki_fp_filters` entries, footprint ids, `${VHRD_3DMODEL_DIR}` model paths (also absolute ones into a
  `kicad-library/3dmodels` folder) and the old `vhrd_*` rows of project library tables. It prints a unified diff;
  `--write` applies it, `--stat` shows only the summary. A dropped footprint still in use is an error, and nothing is
  written. A second run changes nothing.
- `kilib restructure --map FILE --vendor-dir DIR`: builds the new layout from the old libraries inside a kicad-library
  checkout. Each symbol gets its own `.kicad_sym` in `symbols/VHRD_*.kicad_symdir` (KiCad 10's layout; derived symbols
  go with their parent), footprints move to `footprints/VHRD_*.pretty`, and `VHRD_Vendor` goes to `DIR`. Text stays as
  it was except what `migrate` rewrites. `--dry-run` lists every move.
- `just check-boards`: `kilib check` on the hw checkout beside this one (or `VHRD_BOARDS_DIR`), skipped with a note
  where there is none.

- `symbols/VHRD_Connector.kicad_symdir`: `ARJM11D7-114-AB` and `BAT-HLD-001-THM`, recovered from the symbol caches of
  the boards that used them as `vhrd_connector_abracon:` and `vhrd_connector_battery:` symbols no library had.

### Changed

- The library is laid out by kind, like KiCad's own: symbols in KiCad 10 `symbols/VHRD_*.kicad_symdir` libraries (one
  file per symbol), footprints in `footprints/VHRD_*.pretty`, each named `VHRD_` plus the KiCad library it would
  belong to. `migrate/2026-11.toml` says where each old symbol, footprint and 3D model went; `kilib migrate` moves a
  board. Third-party footprints and symbols moved to the private `VHRD_Vendor` library (`${VHRD_VENDOR_LIBRARY_DIR}`).
- The templates Vhrd Default, IO30 and Hammond-1455C801 use the new library names. `just look` colours `VHRD_power`
  and runs `vhrd-kicad adopt --power` on the three templates; `just test` checks that each template's schematic and
  board print their embedded VHRD sheet.
- Footprint model paths: `B0505MT-1WR4`'s second model goes through `${VHRD_3DMODEL_DIR}` instead of a Windows path.
- `just test` also runs kilib's tests (`just test --changed`: kilib's changed crates only), `just lint` checks its
  format and clippy, and `just install` builds and installs `kilib`.

### Removed

- `3dmodels/`: the 3D models live outside the repo, under `${VHRD_3DMODEL_DIR}` (`own/`, `vendor/`).
- `keymaps/`, `default/`, `templates/default`, and the sheet copies `templates/vhrd_a4.kicad_wks` and
  `vhrd_a3.kicad_wks` (vhrd-kicad makes the sheets, and the templates embed them; a board that used a sheet by path
  embeds it instead).
- The templates RaspberryPi-HAT, RaspberryPi-uHAT and STM32_Nucleo-64_Morpho: unchanged copies of KiCad's stock
  templates, which KiCad ships.
- The unused footprints `TEST` and `_TEMPLATE`, and `09551667812741`'s 3D model line, which pointed at a file on a
  Windows PC.

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
