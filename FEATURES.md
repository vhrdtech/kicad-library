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
  third-party parts sorted the same way (the private `VHRD_Vendor` library folded in, 7 Nov 2026); `migrate/2026-11.toml`
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
- ✅ LIB-9 license: `LICENSE.md`, CC-BY-SA 4.0 with KiCad's library exception, the same as KiCad's own libraries;
  `README.md` for outside users, and the repo scrubbed of tpm ids, PC names and people's names
- ✅ LIB-10 fab-designators: every footprint's designator only on F.Fab (B.Fab for bottom-side parts), visible and
  unlocked, never on the silkscreen and never twice, the Value hidden; centred on the body (fab outline, else
  courtyard), pads ignored, 0.05 mm clear of the other fab drawing with the strokes counted: smaller (1 to 0.3 mm),
  turned 90°, else the largest free spot nearest the centre, else centred at 0.3 mm and reported as no clean fit;
  every fab line 0.05 mm; `kilib fab-designators` lists what breaks these rules with counts and `--write` fixes it
- ✅ LIB-11 stock-copies: KiCad stock footprints that boards use are copied into `VHRD_*` libraries of the same name
  (designator on F.Fab, fab lines 0.05 mm, Value hidden, all else as in stock), so KiCad updates never change them
  unannounced;
  `kilib stock import` copies, `kilib stock diff` compares the copies with the current stock library;
  `migrate/2026-11-stock.toml` re-points boards to the copies (21 footprints of B153: `VHRD_Resistor_SMD`,
  `VHRD_Connector_PinSocket_2.54mm` and existing libraries)
