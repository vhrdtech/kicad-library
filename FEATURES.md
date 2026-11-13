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
- ✅ LIB-10 fab-designators: every footprint's designator only on its layer (F.Fab, B.Fab for bottom-side parts,
  Ref.Des since LIB-24;
  LIB-13's layers for test points, net ties and mechanical parts), visible and unlocked (mechanical: hidden), never on
  the silkscreen and never twice (the Value: LIB-21); the ink, measured in KiCad's stroke font, centred on the body
  (fab outline, else courtyard), pads ignored, 0.02 mm clear of the other drawing on its layer beyond the strokes;
  sized for the longest designator of the footprint's kind (R1004, C1001, TP526, U1004): smaller (1 to 0.2 mm),
  turned 90°, else the largest free spot nearest the centre, else down to 0.15 mm reported as tiny, else centred at
  0.2 mm and reported as no clean fit; strokes 15 % of the size, at least 0.025 mm; every fab line 0.05 mm;
  `kilib fab-designators` lists what breaks these rules with counts and sizes and `--write` fixes it
- ✅ LIB-11 stock-copies: KiCad stock footprints and symbols that boards use are copied into `VHRD_*` libraries of
  the same name (footprints: designator on F.Fab, fab lines 0.05 mm, Value on Value.F, all else as in stock; symbols:
  standalone, `Footprint` at our copy, `Mpn` fields, all pins and graphics as in stock), so KiCad updates never
  change them unannounced;
  `kilib stock import [--symbols]` copies, `kilib stock diff` compares the copies with the current stock library;
  `migrate/2026-11-stock.toml` re-points boards to the copies (21 footprints and 23 symbols of B153:
  `VHRD_Resistor_SMD`, `VHRD_Connector_PinSocket_2.54mm`, `VHRD_Transistor_BJT`, `VHRD_Amplifier_Operational`,
  `VHRD_74xGxx` and existing libraries)
- ✅ LIB-12 board-designators: `kilib fab-designators --board PCB` fits each placed footprint's own designator by the
  LIB-10 rules (a short one gets larger text), writing only the Reference's position, angle (upright), size and
  thickness, and only where the Reference is already on its layer
- ✅ LIB-13 designator-layers: front/back pairs of user layers, typed so KiCad flips them, User.9/10 Ref.Test,
  User.11/12 Ref.Tie, User.13/14 Ref.Mech, User.15/16 Rooms, named in the three templates; the class of a footprint
  from its library, name and designator prefix; `kilib layers` names them in an existing board's layer table only
- ✅ LIB-14 pin1-marks: parts with three or more pins and polar two-pin parts mark pin 1 on the fab layer, and on the
  silkscreen where they draw one, so no turn of the part that fits its pads leaves pin 1 in doubt; `kilib lint
  --rule pin1` reports missing, ambiguous and misplaced marks, `--write` chamfers a rectangular fab outline's pin-1
  corner and adds a silkscreen triangle
- ✅ LIB-15 footprint-attributes: test points, net ties, spark gaps, jumpers, logos and mounting holes out of the BOM
  and position files, fiducials in the position files only, parts SMD or through-hole as their pads are (`kilib lint
  --rule attrs`)
- ✅ LIB-16 courtyard-clearance: a closed courtyard 0.25 mm from pads and body, 0.1 mm for 0201 to 0603 passives;
  `kilib lint --rule courtyard` draws missing ones and grows rectangular ones
- ✅ LIB-17 model-paths: every part has a 3D model under `${VHRD_3DMODEL_DIR}` or `${KICAD10_3DMODEL_DIR}` that exists;
  `kilib lint --rule models` moves older KiCad variables to KICAD10 and lists missing models and files. Swept: every
  stock-imported footprint moved off `${KICAD10_3DMODEL_DIR}` onto `${VHRD_3DMODEL_DIR}/vendor/...` (no part depends
  on KiCad's stock 3D package); 4 of the 47 models have no upstream STEP file at all and stay reported missing
- ✅ LIB-18 silk-rules: silkscreen lines 0.12 mm, none onto a pad's mask opening, no text but pin-1 marks; `kilib lint
  --rule silk` sets widths and cuts lines round pads; `just check-footprints`
- ✅ LIB-19 ref-layer-outlines: test points draw their pad outline on Ref.Test, net ties, spark gaps and jumpers a box
  round their pads on Ref.Tie (0.05 mm out, 0.05 mm lines; `kilib lint --rule ref-outline`), and `kilib
  fab-designators` fits the designator to that drawing, beside it where nothing fits inside
- ✅ LIB-20 more-layers: User.17/18 Value.F/B, User.19/20 Pins.F/B, User.21/22 Centroid.F/B (front/back pairs), and
  KiCad's Dwgs.User, Cmts.User and User.2 named Dimensions, Fab.Notes and Body.F, in the three templates; `kilib
  layers` names them in an existing board (User.2 only where the board draws nothing of its own there), refuses
  boards in KiCad 8's layer numbering
- ✅ LIB-21 value-layer: each footprint's Value on Value.F/B (User.17/18), shown and unlocked (mechanical: hidden),
  fitted like the designator for a long value of the kind (100k, 4.7uF, TPS54202DDC) with the designator's ink as an
  obstacle, so both read side by side; just outside the body where it holds only the designator; `kilib
  fab-designators` checks and writes it, `--board` fits each placed footprint's own value (LIB-24: real parts only,
  centred on its own)
- ✅ LIB-22 pin-numbers: pin numbers on Pins.F/B (User.19/20) beside the pads: every pin of a connector, pin 1 and
  every 5th (10th past 40) of other parts with three or more pins; level, 0.1 mm clear of pads, holes and each other,
  never between two pads in line, one size per footprint (0.5 to 0.2 mm); `kilib lint --rule pins`. 125 footprints;
  3 with numbers that have no room are listed
- ✅ LIB-23 centroid-cross: a 0.5 mm cross of 0.05 mm lines on Centroid.F/B (User.21/22) at the origin of every part
  and fiducial, for checking the position file; `kilib lint --rule centroid`. 163 footprints
- ✅ LIB-24 ref-des-layer: F.Fab/B.Fab carry the drawing alone (no text); real parts' designators on User.23/24
  Ref.Des.F/B, centred and fitted by the LIB-10 rules clear of the fab drawing of the same side; a real part's Value
  on Value.F/B centred the same way (the designator no obstacle, the two layers shown one at a time), sized in the
  library for the longest typical value of its kind (100K, 100nF, 4.7uH, BAS16TW, TPS54202); test points, net ties
  and mechanical parts keep their Value hidden on the fab layer; named in the three templates and by `kilib
  layers`; `kilib designators` (old name `fab-designators`) checks and writes it, `--board` fits each placed
  footprint's own designator and value. 196 footprints
- ✅ LIB-25 stock-decoupled: the library needs none of KiCad's stock libraries: every symbol's `Footprint` field and
  `ki_fp_filters` library is a `VHRD_*` library of the checkout, kept so by `tests/decoupled.rs`; `kilib check
  --only VHRD_` flags anything from another library on a board too (symbols, footprints, Footprint fields, the
  schematic's embedded symbols, filters); the stock map's `[filter_lib]` moves filter libraries. B153a is the first
  board on it. Next: the templates (power symbols and logos still stock, from vhrd_brand), the other boards
- 🚧 LIB-26 per-package-symbols: a generic symbol (`VHRD_Device:R`, `Footprint` empty) gets a symbol per package
  it is placed with (`VHRD_Device:R_0402`), derived from it with `Footprint` and `ki_fp_filters` set, so a
  footprint review or BOM line keyed by the library symbol covers it; `migrate/2026-11-packages.toml`
  (`[per_package]`) names them, `kilib per-package` makes them and lists a board's placed generics by footprint,
  `kilib migrate` moves placed symbols by their footprint (the schematic's cache gets the copies);
  `tests/per_package.rs` keeps the library and the map in step. 22 symbols for b153a's 15 generics. Next: the
  other boards
- ✅ LIB-27 chip-step-models: `kilib model chip` writes STEP models of chip passives and power inductors from the
  tables in `generate/` (one per size and height) into `${VHRD_3DMODEL_DIR}/generated/`; a STEP writer of kilib's
  own (`kilib::step`, AP214, coloured prism solids), no CAD kernel. KiCad's 3D viewer reads them with their colours
- ✅ LIB-28 chip-footprints: chip R, C and L footprints, one per size and body height (`C_0805_2012Metric_H1.25`):
  KiCad's stock IPC-7351 nominal land of the size, the generated model of that height (LIB-27); sizes and heights
  from the makers' tables in `generate/chip-passives.toml` (R 0201-2512, C 0201-1812, L 0201-1210, multilayer and
  wire-wound); `kilib footprint chip` makes them, each with its per-package symbol (`VHRD_Device:C_0805_H1.25`,
  LIB-26) on the generic `R`, `C` or `L`; `tests/chip_tables.rs` keeps tables, footprints and map in step. 34
  footprints
- ✅ LIB-29 power-inductors: shielded (ferrite drum: Sunlord SWPA, Bourns SRN, TDK VLS, Würth LQS) and moulded
  (metal composite: Murata DFE, Coilcraft XAL/XFL, Würth MAPI/XHMI, Cyntec, Bourns SRP, TDK SPM) power inductors,
  one footprint per body and height (`L_Shielded_4.0x4.0mm_H2.0`, `L_Molded_4.0x4.0mm_H3.1`), the land of the maker
  named in the description with the series it fits; bodies, electrodes and lands in `generate/power-inductors.toml`,
  generated models, per-package symbols on `VHRD_Device:L_Iron`. 68 footprints over 17 bodies
- ✅ LIB-30 current-sense-shunts: metal-element current-sense resistors 0603 to 2512 (Vishay WSL/WSLP/WSLT/WSLS,
  Yageo PE, PE_L and PA, Bourns CRF/CRA, TT LRMAP, Rohm GMR100, Isabellenhütte SMS), one footprint per size,
  height and land (`R_Shunt_1206_3216Metric_H0.6_Pad1.85x1.85mm_Gap0.45mm`). The land is IPC-7351B nominal,
  computed from each series' body and termination length per resistance range (`kilib::ipc`, the formula of
  KiCad's stock chip lands, gap at least 0.30 mm), so the long terminations are covered without the extra copper of
  the makers' lands; the maker's land is in the description. Data in `generate/current-sense.toml`,
  `kilib footprint shunts` prints the table; a `shunt` model style (metal element under a black coat, long
  terminations); per-package symbols on `VHRD_Device:R`. 42 footprints
- ✅ LIB-31 chip-led-models: chip LEDs 0402 (H0.2, H0.4, H0.5) and 0603 (H0.2, H0.35, H0.65, H0.8, H0.95) in red,
  orange, yellow, green, blue and white, one footprint per size, height and colour
  (`LED_0603_1608Metric_H0.8_Red`): the stock IPC land with the `_Arrow` silkscreen. The generated model shows the
  LED's colour: a white board with gold ends and a lens tinted in the emission colour, see-through with the die
  inside for a water-clear lens (`SURFACE_STYLE_TRANSPARENT`), the opaque yellow phosphor for white. Heights, board
  and lens from LiteOn, Kingbright, OSRAM and Würth drawings in `generate/chip-leds.toml` (`kilib::chip::led`);
  per-package symbols on `VHRD_Device:LED_<Colour>` (`LED_Red_0603_H0.8`). 48 footprints
