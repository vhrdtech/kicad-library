# Features

Status: ✅ done · 🚧 in progress · 📋 planned · 💡 idea · 🐛 bug

## LIB: library

- ✅ LIB-1 symbols-footprints: VHRD symbols and footprints (`symbols/`, `footprints/`); their 3D models live outside
  the repo (LIB-8)
- ✅ LIB-2 drawing-sheets: the VHRD A4 and A3 sheets (vhrd_brand KICAD-2), embedded in each kept template by
  `vhrd-kicad adopt`; the repo keeps no copy of its own
- ✅ LIB-3 power-symbols: `VHRD_power` supplies orange, grounds grey (vhrd_brand KICAD-4)
- ✅ LIB-4 project-template: "Vhrd Default" with the embedded A3 sheet, net class colours and text variables
- ✅ LIB-5 kind-first-layout: symbols in KiCad 10 `.kicad_symdir` libraries and footprints in `.pretty` libraries,
  named `VHRD_` plus the KiCad library they would belong to (`VHRD_Connector_Molex`, `VHRD_Package_DFN_QFN`);
  third-party parts in the private `VHRD_Vendor` library under `${VHRD_VENDOR_LIBRARY_DIR}`; `migrate/2026-11.toml`
  maps every old symbol, footprint and 3D model to its new home; the templates are Vhrd Default, IO30 and
  Hammond-1455C801
- 🚧 LIB-6 kilib-check: `tools/kilib`, a Rust CLI: `kilib check` (every symbol, Footprint field, footprint and 3D model
  of the boards resolves through the library tables and path variables) and `kilib setup` (library rows through
  `${VHRD_LIBRARY_DIR}` / `${VHRD_VENDOR_LIBRARY_DIR}` from a rename map or the folders found, and the path variables,
  in this PC's KiCad config; `--dry-run`); `just check-boards`. Next: `just install` calling `kilib setup`
- ✅ LIB-7 kilib-migrate: `kilib migrate --map FILE PATH...` rewrites boards to a new library layout, only the strings
  the map matches (symbol ids and the `lib_symbols` cache, `Footprint` fields, `ki_fp_filters`, footprint ids,
  `${VHRD_3DMODEL_DIR}` model paths, old rows of project library tables), as a unified diff or `--write`;
  `kilib restructure --map FILE --vendor-dir DIR` builds the `VHRD_*.kicad_symdir` (one file per symbol) and
  `VHRD_*.pretty` layout from the old libraries (`--dry-run`); tests on synthetic fixtures, an extended set on a copy
  of the boards, and a check that the map covers every symbol, footprint and 3D model
- ✅ LIB-8 models-on-syncthing: no 3D models in the repo; footprints find theirs under `${VHRD_3DMODEL_DIR}`, in
  `own/` (made here) and `vendor/` (the makers', not redistributable), one `<FootprintLib>.3dshapes/` folder per
  footprint library
- 🚧 LIB-9 license: `LICENSE.md`, CC-BY-SA 4.0 with KiCad's library exception, the same as KiCad's own libraries; a README for outside users still to come
