# Features

Status: ✅ done · 🚧 in progress · 📋 planned · 💡 idea · 🐛 bug

## LIB: library

- ✅ LIB-1 symbols-footprints: VHRD symbols, footprints and 3D models (`symbols/`, `footprints/`, `3dmodels/`)
- ✅ LIB-2 drawing-sheets: `templates/vhrd_a4.kicad_wks` and `vhrd_a3.kicad_wks` in the VHRD look (vhrd_brand
  KICAD-2), for boards that use them by path
- ✅ LIB-3 power-symbols: `vhrd_power` supplies orange, grounds grey (vhrd_brand KICAD-4)
- ✅ LIB-4 project-template: "Vhrd Default" with the embedded A3 sheet, net class colours and text variables
- 🚧 LIB-6 kilib-check: `tools/kilib`, a Rust CLI: `kilib check` (every symbol, Footprint field, footprint and 3D model
  of the boards resolves through the library tables and path variables) and `kilib setup` (the library rows through
  `${VHRD_LIBRARY_DIR}` and the path variables in this PC's KiCad config, `--dry-run`); `just check-boards`.
  Next: `just install` calling `kilib setup`, once the new layout is agreed
