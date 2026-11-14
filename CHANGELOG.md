# Changelog

## [Unreleased]

### Added

- PCB Editor layer presets (LIB-33 layer-presets): `kilib presets` writes `generate/layer-presets.toml` into a project's `board.layer_presets` (`Layout.F`/`B`, `Assembly.F`/`B`, `Pins.F`/`B`, `Silk.F`/`B`); replaces a preset of the same name, keeps the project's others, exits 1 without `--write`. The three templates have them.

### Changed

- `kilib designators` and `--board`: a `Layout_Critical`/`Layout_Important` note now goes down to 0.05 mm (was 0.15 mm) before it is reported with no clean fit, so it fits the body more often; the designator and the Value keep their usual 0.2/0.15 mm floors.

## [0.5.1] - 2026-11-14

### Added

- Layout priority (LIB-32 layout-priority-marks): a schematic sets a short note in a symbol's `Layout_Critical` or `Layout_Important` field, and Update PCB from Schematic carries it to the footprint, where it shows on its own layer. README "Layout priority" says how KiCad 10 treats the fields and what a board made before needs.
- `kilib layers`: User.25/26 `Crit.F`/`Crit.B` and User.27/28 `Imp.F`/`Imp.B`; the three templates name them.
- `kilib lint --rule layout`: every real part has empty, shown `Layout_Critical` and `Layout_Important` properties on Crit and Imp, centred like the Value. Written across `footprints/` (377 footprints).
- `kilib layout-fields`: adds the empty, hidden `Layout_Critical` and `Layout_Important` fields to library symbols, and to placed symbols and their `lib_symbols` copies in schematics. Written across `symbols/` (421 symbols; power symbols get none). `kilib stock import --symbols` adds them too.
- `kilib designators --board`: a `Layout_*` note that is set is fitted to its own text; on a real part one that KiCad left hidden on the fab layer moves to Crit/Imp, shown.

### Changed

- Every library symbol and real part's footprint changed (new fields). A board's placed symbols now differ from the library until "Update Symbols from Library" (or `kilib layout-fields --write`), so ERC reports `lib_symbol_mismatch` on them; after the footprints are updated from the library, the parity DRC wants the fields on both sides.

## [0.5.0] - 2026-11-14

### Added

- TI footprints from TI's example board layouts: `TI_DBV0005A_SOT-23-5`, `TI_DCU0008A_VSSOP-8_2.3x2mm_P0.5mm`, `TI_RGT0016A_VQFN-16-1EP_3x3mm_P0.5mm_EP1.45x1.45mm`, `TI_DDA0008B_SOIC-8-1EP_..._ThermalVias` and `TI_DFG0004A_SOIC-4_4.8x3.51mm_P2.54mm` (HV land). The TI symbols that sat on a generic footprint use them or an existing TI footprint (TLV755P, TLV702, INA181, TLV9061, TS5A3160, SN74LXC1T45, INA229, ten DCU parts, TCA6408A, TXU0104, LV14360, ISOM8600, SN74LXC8T245); the generic footprints stay.
- `kilib migrate`: `[footprint_rename]` renames footprint ids, `[symbol_footprint]` gives a symbol's placed parts another footprint. `migrate/2026-11-ti.toml` moves boards to the TI footprints.
- `VHRD_Package_TO_SOT_SMD:SOT-23_CANFD-ESD`: TI DBZ0003A land, which also fits Nexperia's SOT23;
  `PESD2CANFD24V-T` uses it.

### Changed

- TI footprints are named `TI_<TI package designator>_<generic package>` (`TI-DBV0006A` is now `TI_DBV0006A_SOT-23-6`, `Texas_R-PDSO-G5_DCK-5` is `TI_DCK0005A_SC-70-5`), 15 of them; `Texas_UQFN-10_1.5x2mm_P0.5mm` merged into `TI_RSE0010A_UQFN-10_1.5x2mm_P0.5mm`. The old names are in `migrate/2026-11-ti.toml`.

### Fixed

- TI lands checked against TI's drawings: `TI_PW0008A` rows were 1 mm too close (now 5.8 mm centres), `TI_RUC0014A` pads 0.6 mm long instead of 0.875 mm, `TI_DCK0005A` pads 0.95 mm long instead of 0.9 mm.
- `TXU0101DCKR` pointed at the DRL (SOT-563) footprint; it is TI's DCK (SC-70-6) part, and the map moves placed ones to `TI_DCK0006A_SC-70-6`.
- Lands from the makers' drawings after footprint review: STM32G0 LQFP-48 (plain and both dual LQFP/UFQFPN
  variants, ST DS13560), `DFN-12-AOZ` corner pads (AOS), `Texas_RUK0020B` pad length (TI SLVSEL8).

## [0.4.0] - 2026-11-13

### Added

- Chip LED footprints (LIB-31), 48 of them, each with its generated model and its per-package symbol: 0402 (H0.2,
  H0.4, H0.5) and 0603 (H0.2, H0.35, H0.65, H0.8, H0.95), each in red, orange, yellow, green, blue and white
  (`LED_0603_1608Metric_H0.8_Red`, `VHRD_Device:LED_Red_0603_H0.8`). The land is the size's stock IPC-7351 one, the
  silkscreen the library's `_Arrow` style (a triangle outside the courtyard and one between the pads, both pointing
  at the cathode). Heights, board thickness and lens length from LiteOn, Kingbright, OSRAM and Würth drawings in
  `generate/chip-leds.toml`; heights within 0.1 mm merged, rounding up.
- The LED models show the LED's colour: a white board with gold ends, a die, and a lens tinted in the emission
  colour, see-through for a water-clear lens so the die shows; a white LED's lens is its opaque yellow phosphor.
- `kilib::step`: a solid can be see-through (`Solid::transparency`, written as `SURFACE_STYLE_TRANSPARENT`, which
  KiCad 10's 3D viewer renders).
- `VHRD_Device:LED_Yellow`, `LED_Blue` and `LED_White`, generics as `LED_Red`, for the per-package LED symbols.

### Changed

- kilib lint pin1: a footprint tagged `symmetric` wants no pin-1 mark (its pins swap without harm); the ECS 3.2x2.5 crystal and the D_*_BIDIR footprints carry the tag, so `just check-footprints` passes again.

### Removed

- `VHRD_Device:LED_Red_0603` and `LED_Green_0603` (on `LED_0603_1608Metric_Arrow`), replaced by the per-package
  symbols of the new footprints. b153a places them: move LD201 and LD202 to `LED_Red_0603_H0.8` and
  `LED_Green_0603_H0.8` by hand (KiCad's Change Symbol, then Update PCB from Schematic). The footprints
  `LED_0603_1608Metric` and `LED_0603_1608Metric_Arrow` stay.

## [0.3.0] - 2026-11-13

### Added

- Current-sense resistor footprints (LIB-30), 42 of them, each with its generated model and its per-package symbol
  on `VHRD_Device:R`: Vishay WSL/WSLP/WSLT/WSLS, Yageo PE, PE_L and PA, Bourns CRF/CRA, TT LRMAP, Rohm GMR100 and
  Isabellenhütte SMS, 0603 to 2512. Named by size, height and land (`R_Shunt_1206_3216Metric_H0.6_Pad1.85x1.85mm_Gap0.45mm`).
  The land is computed with IPC-7351B nominal rules from the body and the termination length of each resistance
  range, at least 0.30 mm apart, not copied from the makers (whose long lands carry heat-spreading copper out past
  the part); the maker's land is in the description. Bodies and terminations in `generate/current-sense.toml`.
- `kilib::ipc`: IPC-7351B nominal lands of two-terminal chips, as KiCad's footprint generator computes the stock
  ones.
- `kilib footprint shunts`: the computed current-sense lands, one line per table row and height, next to the
  makers' lands.
- `kilib model chip` and `kilib footprint chip` also make the current-sense resistors: a `shunt` model style (metal
  element under a black coat between long terminations).

## [0.2.0] - 2026-11-13

### Added

- `kilib model chip` (LIB-27): STEP models of chip passives and power inductors from the tables in `generate/`, one
  per size and height, into `${VHRD_3DMODEL_DIR}/generated/VHRD_<Lib>.3dshapes/`: body and terminations as coloured
  solids (MLCC tan, resistor alumina with black overcoat, ferrite or ceramic chip inductors, dark power inductors
  with bottom electrodes). kilib writes the STEP itself (`kilib::step`: AP214, convex prisms as B-rep solids with a
  colour each), no CAD kernel.
- `kilib footprint chip` (LIB-28): the footprints of the same tables, one per size and height. A chip part is its
  size's stock IPC-7351 nominal footprint under the new name (`C_0805_2012Metric_H1.25`) with the generated model; a
  power inductor (`L_Shielded_4.0x4.0mm_H2.0`) is drawn from the maker's land and finished by the lint fixes and
  `designators`. Writes only new files.
- Chip footprints per size and body height (LIB-28), 34 of them, each with its generated model and its per-package
  symbol: R 0201 (H0.23), 0402 (H0.35), 0603 (H0.45), 0805, 1206, 1210, 2010, 2512 (H0.55); C 0201 (H0.3), 0402
  (H0.5), 0603 (H0.8), 0805 (H0.6, H0.85, H1.25), 1206 (H0.85, H1.15, H1.6), 1210 (H1.25, H1.6, H2.0, H2.5), 1812
  (H1.6, H2.0, H2.5); L 0201 (H0.3), 0402 (H0.5), 0603 (H0.8), 0805 (H0.85, H1.25), 1206 (H0.85, H1.0, H1.8), 1210
  (H2.0, H2.2). Sizes and heights from Murata, Samsung, Yageo and TDK tables in `generate/chip-passives.toml`.
- `VHRD_Device:L`, the generic inductor (from stock `Device:L`), for the chip inductors' per-package symbols.
- Power-inductor footprints (LIB-29), 68 over 17 bodies, each with its generated model and per-package symbol on
  `VHRD_Device:L_Iron`. Shielded 2.0x1.6, 2.5x2.0, 3.0x3.0, 4.0x4.0, 5.0x5.0, 6.0x6.0, 8.0x8.0 mm; moulded 2.0x1.6,
  2.5x2.0, 3.2x2.5, 3.0x3.0, 4.0x4.0, 4.4x4.2, 5.3x5.5, 5.4x5.2, 6.4x6.6, 7.1x6.5 mm; the heights each body is sold
  in. Lands from the makers' recommended patterns (named in each description with the series that fit), in
  `generate/power-inductors.toml`.
- `VHRD_Package_QFP:LQFP-48_UFQFPN-48_7x7mm_P0.5mm_PasteLQFP` and `_PasteUFQFPN`: one land pattern for LQFP-48 or UFQFPN-48 7x7 (STM32G0B1CxT / CxU), split pads with a 0.19 mm mask web, exposed pad on pin 7; the two differ only in paste.
- Per-package symbols (LIB-26): 22 symbols derived from the generic ones, each with its footprint set, for the
  packages b153a places them with: `VHRD_Device:R_0402`, `R_0603_HandSolder`, `R_0805`, `R_1206`, `C_0402`,
  `C_0805`, `C_1206`, `Crystal_GND24_3.2x2.5mm`, `D_Schottky_SOD-323`, `D_Zener_SOD-523`, `LED_Green_0603`,
  `LED_Red_0603`, `L_Iron_NRS5030`, `NetTie_2_SMD_Pad0.15mm`, `SparkGap_015`, and `VHRD_Connector:TestPoint_Pad_D1.2mm`,
  `TestPoint_Pad_0.8x0.8mm`, `TestPoint_Probe_Pad_D1.5mm`, `TestPoint_Probe_Pad_D0.8mm`,
  `Conn_01x12_Pin_P2.54mm_Vertical`, `Conn_02x03_Odd_Even_Socket_P2.54mm_Vertical`, `USB_C_Plug_241SMT-4BC2SS`.
  What is keyed by the library symbol (berc's footprint review) now covers them. The generic symbols stay for new
  designs.
- `[per_package."Lib:Generic"]` in a rename map (`"FpLib:Footprint" = "Lib:Generic_Package"`):
  `kilib migrate` moves each placed generic symbol to the symbol of the footprint it is placed with, and gives the
  schematic's `lib_symbols` cache a copy of the generic under the new name with the footprint set (the generic's
  cached copy goes when nothing uses it any more). `migrate/2026-11-packages.toml` holds b153a's.
- `kilib per-package --map FILE [BOARD...]`: makes the per-package symbols a map names that the checkout lacks
  (`--write`), checks those there, and lists the generic symbols the boards place by footprint with the map's
  symbol for each.
- `kilib check --only VHRD_`: every library a board or the library names must start with the prefix. A symbol,
  footprint or `Footprint` field from another library is a problem, and so are a schematic's embedded symbols
  (`lib_symbols`, with their `Footprint` fields) and the library part of `ki_fp_filters` patterns, so nothing from
  KiCad's stock libraries goes unnoticed.
- `[filter_lib]` in a rename map renames the library part of `ki_fp_filters` patterns (`"Connector*" =
  "VHRD_Connector*"`), for libraries no id of the map names; `migrate/2026-11-stock.toml` has `Connector*`, `LED_SMD`
  and `LED_THT`, so `kilib migrate` moves a board's embedded copies of stock symbols fully onto the library.
- From stock, for the symbols that named them (and boards that use them): `VHRD_Package_QFP:LQFP-100_14x14mm_P0.5mm`,
  `VHRD_Package_SO:SOIC-20W_7.5x12.8mm_P1.27mm`,
  `SOIC-8-1EP_3.9x4.9mm_P1.27mm_EP2.95x4.9mm_Mask2.71x3.4mm_ThermalVias`, `TSSOP-8_3x3mm_P0.65mm`,
  `VSSOP-10_3x3mm_P0.5mm`, `VHRD_Package_TO_SOT_SMD:SOT-563`, `Texas_R-PDSO-N6_DRL-6`,
  `VHRD_Package_DFN_QFN:DFN-16-1EP_3x5mm_P0.5mm_EP1.66x4.4mm`, `DHWQFN-14-1EP_2.5x3mm_P0.5mm_EP1x1.5mm`,
  `Diodes_UDFN-10_1x2.5mm_P0.5mm`, `Texas_UQFN-10_1.5x2mm_P0.5mm`, the new libraries
  `VHRD_Package_SON:Winbond_USON-8-1EP_3x2mm_P0.5mm_EP0.2x1.6mm` and `VHRD_Connector_PCBEdge:BUS_PCIexpress_x8`,
  and `VHRD_Jumper:SolderJumper-2_P1.3mm_Open_RoundedPad1.0x1.5mm` with its symbol
  `VHRD_Jumper:SolderJumper_2_Open` (b153a). `VSSOP-10_3x3mm_P0.5mm` left KiCad's library in January 2026 (merged
  into `TSSOP-10_3x3mm_P0.5mm`); the copy is the last stock version, with the TSSOP-10 model. All are in
  `migrate/2026-11-stock.toml`, the new libraries in `migrate/2026-11.toml`. Their 3D models go in the vendor
  folder (`vendor/VHRD_<Lib>.3dshapes/`); KiCad has none for `Texas_UQFN-10`, `DHWQFN-14`, `Texas_R-PDSO-N6_DRL-6`
  and the `SOIC-8-1EP ... Mask2.71x3.4mm` variant.
- `VHRD_Capacitor_SMD:C_0805_2012Metric`, from stock, its 3D model in the vendor folder.
- `VHRD_Transistor_FET:BSS138W`: a small multi-source logic-level N-MOSFET in SOT-323 (Diodes BSS138W-7-F,
  Nexperia BSS138BKW,115).
- `Ref.Des.F`/`Ref.Des.B` (User.23/24), a layer pair for the designators of real parts, named in the three
  templates and by `kilib layers --write` (existing boards get it from there). F.Fab and B.Fab now carry only the
  drawing (outline, pin-1 and polarity marks, inner lines), no text. A real part's designator moves from the fab
  layer to Ref.Des, centred and fitted as before (no designator moved or changed size in the sweep), still clear of
  the fab drawing of its side.
- `kilib designators`, the new name of `kilib fab-designators` (the old name still works). A real part's Value on
  `Value.F`/`Value.B` is now centred on the body like the designator, no longer beside it: the two layers are shown
  one at a time. The library sizes it for the longest typical value of the kind (`100K`, `100nF`, `4.7uH`,
  `BAS16TW`, `TPS54202`); `--board` fits each placed part's own value. Test points, net ties, spark gaps, jumpers and
  mechanical parts keep their Value hidden on the fab layer instead of on Value. A mechanical footprint without a
  body (a logo drawn on the silkscreen only) is no longer skipped. Swept: 196 footprints; `L77SDE09SA4CH`'s "PCB
  Edge" note moved from F.Fab to Fab.Notes. Boards take it with `kilib layers --write`, "Update Footprints from
  Library" and `kilib designators --board`.
- `VHRD_Relay_SolidState:Photorelay_1A_SOP-4`, a 1 Form A MOSFET photorelay that several makers build: Mpn1
  VO1400AEFTR (Vishay), Mpn2 AQY282SX (Panasonic), Mpn3 G3VM-61VY2(TR) (Omron), Mpn4 AB37S (Comus), Mpn5/6
  ISOM8600DFGR / ISOM8610DFGR (TI), each with its datasheet (`Datasheet`, `Datasheet2`..`Datasheet6`). Made from
  stock `AQY282S` (pin 1 anode, 2 cathode, 3 and 4 the switch, the same on every part).
- `VHRD_Package_SO:SOP-4_4.4x4.3mm_P2.54mm`, one land pattern for all of them: 1.4 x 0.8 mm pads at x ±3.2 mm,
  P2.54, 5.0 mm between the input and output pads, 7.8 mm overall. No pad edge is more than 0.3 mm from a maker's
  recommended pattern (Vishay 1.375 x 0.9 at ±3.24; Panasonic and Omron 1.2 x 0.8 at ±3.0 to ±3.15; TI DFG's
  isolation option 1.25 x 0.6 at ±3.125), and the pads reach past the longest leads (7.3 mm span). Stock `SO-4_4.4x4.3mm_P2.54mm`'s body, silkscreen and 3D model; its
  1.2 mm pads stop at 3.6 mm, short of the Vishay and TI lead tips.
- More named layers (`kilib layers`, the three templates): User.17/18 `Value.F/B`, User.19/20 `Pins.F/B`,
  User.21/22 `Centroid.F/B`, typed front and back; KiCad's `Dwgs.User` named `Dimensions`, `Cmts.User` `Fab.Notes`
  and `User.2` `Body.F` (a part's full top view). `kilib layers` leaves User.2 unnamed on a board that draws on it
  itself and says so.
- `kilib fab-designators`: the Value goes to `Value.F`/`Value.B`, shown, fitted beside the designator (just outside
  the body where there is no room beside it); `--board` fits each placed footprint's own value. Swept: 193 of 195
  footprints. Stock imports keep the Value on `Value.F` instead of hiding it.
- `kilib lint --rule pins`: pin numbers on `Pins.F`/`Pins.B` beside the pads (connectors every pin, other multi-pin
  parts pin 1 and every 5th, every 10th past 40 pins). Swept: 125 footprints; 3 with numbers that have no room
  (USB-C plug PLUG-USBC-241SMT-TH-4BC2LS, QFN24_MICROCHIP pin 25, TI-BQA0014A pin 15) are listed by `kilib lint --left`.
- `kilib lint --rule centroid`: a 0.5 mm cross on `Centroid.F`/`Centroid.B` at every part's and fiducial's origin.
  Swept: 163 footprints.
- `kilib fab-designators --board BOARD [--write]`: fits each placed footprint's own designator (a short one gets
  larger text); only the Reference's position, angle, size and thickness change, and only where the Reference is
  already on its layer.
- `kilib layers [--write] BOARD...`: names the designator and room layers in a board's layer table (only there):
  User.9/10 `Ref.Test.F/B`, User.11/12 `Ref.Tie.F/B`, User.13/14 `Ref.Mech.F/B`, User.15/16 `Rooms.F/B`, each pair
  typed front and back so KiCad flips text with a footprint. The three templates have them.
- `kilib lint [--write] [--rule R] [--left] [--json] [PATH...]`: footprints against the library rules, each a report
  and, where the rule is clear, a fix: pin-1 and polarity marks on the fab layer and silkscreen (chamfer, triangle),
  attributes by kind (test points, net ties, spark gaps, jumpers, logos, mounting holes out of BOM and position
  files; fiducials in position files; SMD or through-hole as the pads), courtyards (present, closed, 0.25 mm, 0.1 mm
  for 0201-0603 passives), 3D models (present, under `VHRD_3DMODEL_DIR` or `KICAD10_3DMODEL_DIR`, the file there),
  silkscreen (0.12 mm lines, none onto pads, no text but pin-1 marks). `just check-footprints` runs it on the library.
- Footprints swept by `kilib lint --write` (121 of 195): 191 silkscreen fixes (lines set to 0.12 mm, lines cut
  clear of pads), 34 courtyards drawn or grown, 30 attribute sets, 25 model paths moved to `KICAD10_3DMODEL_DIR`,
  13 pin-1 marks added (12 fab chamfers, one silk triangle); 7 designators refitted by `kilib fab-designators`
  after it. `kilib lint --left` lists the 87 findings left for a person.
- `kilib lint --rule ref-outline`: test points draw their pad's outline on `Ref.Test` (a circle 0.05 mm out, or a
  rectangle for a pad that is not round), net ties, spark gaps and solder jumpers a rectangle 0.05 mm round their
  pads on `Ref.Tie`, 0.05 mm lines. Swept: 5 test points, 5 net ties, spark gaps and jumpers.
- `kilib stock import [--write] Lib:Name...` copies KiCad stock footprints into `footprints/VHRD_<Lib>.pretty` with
  the designator on F.Fab and the Value hidden; `kilib stock diff [--stat]` compares the copies with the current
  stock library (`--stock-dir`, else `KICAD10_FOOTPRINT_DIR`, else `/usr/share/kicad/footprints`).
- B153's 21 stock footprints copied in from KiCad 10.0.8 (needs `kilib setup` for the two new libraries):
  `VHRD_Resistor_SMD` (R_0402, R_0603 HandSolder, R_0805, R_1206), `VHRD_Connector_PinSocket_2.54mm`
  (PinSocket_2x03 vertical), and into `VHRD_Capacitor_SMD` (C_0402, C_1206), `VHRD_Package_TO_SOT_SMD` (SOT-23,
  SOT-23-5, SOT-23-6, SOT-23W, SOT-323, SOT-353, SOT-363, Texas DCK-5), `VHRD_Package_DFN_QFN` (QFN-36 6x6, Texas
  RUK0020B WQFN-20, VQFN-16 3x3), `VHRD_Package_QFP` (LQFP-48 7x7), `VHRD_Package_SO` (VSSOP-8 2.3x2),
  `VHRD_TestPoint` (TestPoint_Pad_D1.5mm).
- `migrate/2026-11-stock.toml`: `kilib migrate --map migrate/2026-11-stock.toml BOARD` points a board's stock ids
  of those 21 footprints at the copies.
- `kilib fab-designators [--write] [PATH...]`: checks every footprint against those rules and fixes the ones that
  break one (default: the checkout's `footprints/`).
- `VHRD_Interface_Expansion` symbol library (needs `kilib setup`): `TCA6408ARGT`, stock drawing with bus, supply
  and control pins left, P0-P7 right; `Mpn1` TCA6408ARGTR, `Mpn2` PCAL6408ABS (same pinout, VQFN-16 3x3).
- `VHRD_Diode:BZX585-C5V1`, `BZX585-C5V6` on `VHRD_Diode_SMD:D_SOD-523`; `Mpn2` BZT52C5V1T-7 / BZT52C5V6T-7 (Diodes).
- `VHRD_Diode:PESD3V3S1UB`, `PESD5V0S1UB` unidirectional ESD on `D_SOD-523`; `Mpn2` ESD5Z3.3T1G / ESD5Z5.0T1G (onsemi).
- `VHRD_Diode:PESD3V3U1UB` unidirectional ESD, 2.6 pF, on `D_SOD-523`.
- `VHRD_Transistor_FET:NX3020NAKS` dual N-MOSFET from stock `Q_Dual_NMOS_S1G1D2S2G2D1`, on stock `SOT-363_SC-70-6`.
- `VHRD_Power_Protection:PESD2CANFD24V-T` dual 24 V CAN FD TVS from stock `NUP2105L` (SOT-23, pin 3 GND); `Mpn2` ESD2CANFD24DBZRQ1, `Mpn3` ESD2CANFD24DBZR (TI, same pinout).
- `VHRD_Diode:RB751S40` Schottky on `D_SOD-523`; `Mpn2` RB751S40T1G (onsemi, 30 mA DC).
- `VHRD_Package_SO:SOIC4_DFG_TEX` (ISOM8600): larger pin-1 triangle and a "1" on silk, courtyard grown to fit.
- `VHRD_Diode:BAS40-02L` `Mpn1` → orderable `BAS4002LE6327XTMA1` (the base name doesn't resolve at Digi-Key).
- `VHRD_Diode` symbol library (needs `kilib setup`): `BAS40-02L` on new `VHRD_Diode_SMD:D_TSLP-2-1` (Infineon land
  pattern and stencil; 3D model: stock SOD-882 scaled to 0.4 mm).
- `VHRD_Logic_LevelTranslator:SN74LXC1T45DCKR` (SC70-6), `Mpn2` 74AXP1T45GWH; A↔B arrow and DIR meaning drawn.
- `VHRD_Logic_LevelTranslator`, one pinout and drawing each (A left, Bn facing An), one symbol per part since ranges
  and pulls differ:
  - dual-supply on VSSOP-8: `SN74LXC2T45DCUR` 1.1-5.5 V, `SN74LVC2T45DCUR` 1.65-5.5 V, `74LVC2T45DC` (Nexperia)
    1.2-5.5 V, `SN74AVC2T45DCUR` 1.2-3.6 V, `SN74AXC2T45DCUR` 0.65-3.6 V;
  - pass-FET, datasheet pin names, VREF_B passive (~200k feed), KiCad 3D models: `LSF0102DCUR` 0-5.5 V, `PCA9306DCUR`
    (TI, VREF1 from 1.2 V), `PCA9306DC1Z` (NXP, to 5 V) on VSSOP-8; `PCA9306DP`, `NVT2002DP` (VREFA to 5.4 V) on
    TSSOP-8.
- `VHRD_Connector:USB_C_Receptacle_USB2.0_16P`: KiCad 10 stock symbol (shield `SH`) + GCT USB4105, `Mpn1`
  USB4105-GF-A; immune to stock renumbering, pins where stock has them (swap keeps wires).
- `VHRD_Power_Protection:TPD6S300A`: connector pins face their system pins; groups CC/SBU/D/RPD; pins 16/17 `NC/D3`,
  `NC/D4`; `Mpn1` TPD6S300ARUKR, `Mpn2` TPD8S300ARUKR (pin-compatible, VPWR <= 3.6 V).
- `VHRD_TestPoint:TestPoint_Pad_D1.2mm`, `TestPoint_Pad_0.8x0.8mm`.
- `VHRD_Connector:ARJM11D7-114-AB`, `BAT-HLD-001-THM`, recovered from board caches.
- `VHRD_power:+3V8_DCDC`, `+VIO`, `+VAUX`.
- `VHRD_Amplifier_Current:INA181`: KiCad's stock symbol as is.
- Side-pin copies of B153's stock symbols (supplies top left, ground bottom left): `STM32G0B1CETx`, `USB2514B_Bi`,
  `TS5A3160DCK` (one unit; NO, NC, COM, IN), `TXS0102DCU`, `D3V3XA4B10LP`, `TLV70233_SOT23-5`.
- `STM32G0B1CETx`: pin names as DS13560 Table 12 (`PF2-NRST`, `PA14-BOOT0`, `PA11[PA9]`, OSC, VDDA/VSSA), UCPD
  dead-battery pins, pins 30/31 VSS/VDDIO2 on the `N` part.
- `LICENSE.md` (CC-BY-SA 4.0 + KiCad library exception; `tools/kilib` MIT); `README.md` for outside users.
- `migrate/2026-11.toml`: old-to-new map of every symbol, footprint and 3D model.
- `tools/kilib` (Rust CLI):
  - `check PATH...`: resolves symbols, footprints and 3D models as KiCad does (tables, path variables, `${KIPRJMOD}`,
    `KICAD6..10_3DMODEL_DIR`, `.kicad_symdir`); exit 1 with the misses, `--json`.
  - `setup`: writes the library rows and `VHRD_*` variables into this PC's KiCad config from the map or the folders;
    `--dry-run`; refuses while KiCad runs.
  - `migrate --map FILE PATH...`: rewrites only mapped strings in boards (ids, caches, `Footprint` fields, fp filters,
    model paths, old table rows); diff by default, `--write`, `--stat`; errors on dropped footprints in use;
    idempotent.
  - `restructure --map FILE --vendor-dir DIR`: builds the new layout from the old libraries; `--dry-run`.
- `just check-boards`: `kilib check` on `../hw` (or `VHRD_BOARDS_DIR`).
- `kilib stock import --symbols [--write] Lib:Name...` copies KiCad stock symbols into
  `symbols/VHRD_<Lib>.kicad_symdir`: a derived symbol made standalone, the `Footprint` field at our footprint copy,
  `ki_fp_filters` library parts prefixed `VHRD_`, an empty `Mpn1`. `kilib stock diff` compares symbols too
  (`--stock-symbol-dir`, else `KICAD10_SYMBOL_DIR`), with each copy's fields and flags laid over the fresh import;
  a kind whose stock folder is missing is skipped with a note.
- B153's stock symbols copied in from KiCad 10.0.8 (needs `kilib setup` for the three new libraries):
  `VHRD_Transistor_BJT` (BC847BS, BC847BPN), `VHRD_Amplifier_Operational` (TLV9061xDCK), `VHRD_74xGxx` (74LVC1G17,
  `Footprint` Texas DCK-5), and into `VHRD_Device` (SparkGap, NetTie_2, D_Schottky, D_Zener, L_Iron, Crystal_GND24),
  `VHRD_Connector` (TestPoint, TestPoint_Probe, Conn_01x12_Pin), `VHRD_Diode` (BAS16TW, BAT54CW, BAT54S, SMF22A),
  `VHRD_Mechanical` (MountingHole), `VHRD_Regulator_Linear` (TLV75533PDBV). The specific parts carry B153's Mpns;
  test points, net tie, spark gap and mounting hole are out of BOM and simulation.
- `migrate/2026-11-stock.toml` maps those symbols, and `Device:R`, `Device:C`,
  `Connector_Generic:Conn_02x03_Odd_Even` and `power:PWR_FLAG` to the library's own of the same name (same pins and
  graphics), so `kilib migrate` re-points a board's symbol ids and its symbol cache.
- `VHRD_Device:R`, `VHRD_Device:C`, `VHRD_Connector:Conn_02x03_Odd_Even`: an empty `Mpn1` field.

### Changed

- `kilib migrate` leaves 3D model paths alone, without a warning each, when the map has no `[model]` table (the
  stock map, the per-package one).
- `Oscillator_SMD_ECS-4Pin_3.2x2.5mm_1`: no pin-1 mark (silk triangle, fab chamfer). The 4-pad crystal is the same
  turned 180°, so the mark suggested an orientation that does not matter; the designator grows to 0.7 mm.
- Footprints keep the Value shown on its own layer (`Value.F`) instead of hidden on F.Fab; `kilib fab-designators`
  reports a hidden Value or one on another layer.
- `kilib fab-designators` fits a test point's, net tie's or jumper's designator to the drawing on its own reference
  layer when there is one (centred inside, smaller, else 0.3 or 0.2 mm beside it, nearest); the fab drawing no
  longer counts for those. 9 designators refitted.
- `kilib fab-designators` measures the designator in KiCad's stroke font (glyph advances and ink from KiCad's
  newstroke font, pcbnew's line layout; checked against `kicad-cli`), centres its ink rather than its anchor on the
  body, and sizes a library footprint for the longest designator of its kind (`R1004`, `C1001`, `TP526`, `JP104`,
  `U1004`). 0.02 mm stays free beyond the strokes (was 0.05 mm). Sizes go down to 0.2 mm in 0.01 mm steps below
  0.3 mm (was 0.3 mm), and to 0.15 mm for the smallest parts, reported as "tiny"; strokes are 15 % of the size, at
  least 0.025 mm. The report gives the size distribution.
- Designators of test points (TP) go to `Ref.Test`, of net ties, spark gaps and solder jumpers (NT, E, JP) to
  `Ref.Tie`, of mounting holes, fiducials, logos and other graphics (H, FID) to `Ref.Mech`, hidden there; the class
  comes from the footprint's library and name and the designator's prefix. Parts keep F.Fab / B.Fab.
- Footprints re-swept: every designator moved by a few hundredths to centre its ink; 30 moved to the new layers.
  0402 now fits cleanly (R_0402 0.2 mm, C_0402 0.19 mm, was 0.3 mm over the outline). Tiny (below 0.2 mm): C_0402,
  D_SOD-323_BIDIR, D_SOD-523, D_TSLP-2-1, SPARK_GAP_015, SOT-883, TestPoint_Pad_D0.6mm. No clean fit:
  D_SOD-882D_BIDIR, NetTie-2_SMD_Pad0.15mm, SPARK_GAP_011 (was 21 footprints).
- `kilib stock import` and `kilib stock diff` apply `kilib lint`'s fixes too, so the copies follow the library
  rules and the diff still shows only upstream changes.
- `kilib fab-designators` centres the designator on the body (the fab outline's centre, else the courtyard's) and lets
  it run over the pads: KiCad draws the selected fab layer on top of them, while text outside the body runs into
  neighbouring parts. The text (sized for five characters, `R1234` or `REF**`, its stroke counted) keeps 0.05 mm off
  every other line on its fab layer (outline, polarity and pin-1 marks, diode symbols). It fits by getting smaller
  (1 mm down to 0.3 mm) or turning 90°, else it moves to the largest free spot inside the body nearest the centre.
  Where nothing fits it stays centred at 0.3 mm, and the report lists it as "no clean fit". The report counts how many
  designators moved, were resized and turned.
- `kilib fab-designators` also sets every line on F.Fab and B.Fab (`fp_line`, `fp_rect`, `fp_circle`, `fp_arc`,
  `fp_poly`) to a 0.05 mm stroke and hides a shown Value. The designator's thickness stays 15 % of its size.
- Footprints: the designator lives only on the fab layer, once per footprint, centred on the body. 185 of the
  library's 195 footprints changed (112 designators moved, 101 resized, 54 turned); every fab line is 0.05 mm, every
  Value hidden. 21 footprints have no clean fit (0402 passives, SOD-323/523/882, SOD-123-size diodes with a full-body
  symbol like D_SMF, SOT-723, SOT-883, small test points, net ties, spark gaps, rods and spacers): there the
  designator sits centred at 0.3 mm over the drawing or past the body. The silkscreen and the pads are untouched.
  Boards take the new footprints with "Update Footprints from Library".
- `LSF0102DCUR`: A↔B arrows, EN meaning and `VREF_B ≥ VREF_A + 0.8 V` drawn in the body.
- `USB2514B_Bi`: reset-time markers, `S` straps without pull (CFG_SEL0/1, NON_REM0/1), `PD`/`PU` internal pulls, `RC`
  RESET_N, `!` VBUS_DET; legend with port disable, errata 100k + 220 nF, VBUS_DET, 1.2 V FILT <= 0.1 uF, weak XTALOUT;
  pins regrouped by port.
- `STM32G0B1CETx`:
  - ROM bootloader pin states beside the pins (AN2606 T112 + ROM): `H` driven high, `OD` I2C, `PU`/`PD`/`I`, `Rd`
    UCPD1 sink on PA8/PB15 (ROM strobes it, hw test T3); legend; body 7.62 mm wider.
  - reset-time pins named in every alternate (`PA14-BOOT0/…`, `PF2-NRST/…`, `PA13-SWDIO/…`), `-DB` dead-battery Rd
    pins, `-DBC` their enables.
  - `SK` on PC13-PC15: sink only, VBAT switch 3 mA, 2 MHz / 30 pF; LSE survives all but power-on.
  - alternates start with their pin (`PA9/USART1_TX`), plus GPIO modes `/PP`, `/OD`, `/IN`, `/FX`; remapped pins 33/34
    name their port.
- F.Fab: Value hidden, one centred `${REFERENCE}` per footprint (0.25-1.0 mm, rotated on tall bodies).
- Courtyards of the 128 footprints with STEP models rebuilt from the model top view + KLC F5.3 clearance (0.25; 0.15
  small; 0.5 connectors, crystals; 1.0 BGA), gaps < 2 mm closed, rectangle if >= 87 % fill; one-sided mating room kept
  on six connectors; Mechanical and Module untouched; connector top views on `User.2`.
- Pin-1 mark: one 0.48x0.33 mm silk triangle (KLC F5.1) outside the outline, 0.2 mm from copper; diodes and LEDs keep
  cathode bars, bidirectional get none.
- All footprints saved by KiCad 10 (`kicad-cli fp upgrade`); TS3USB221A, USB3315, USB4604-1080HN-TR symbols too.
- Pad mask/paste overrides from vendor exports removed (0.102 mm mask on 13 parts, 0.2 / -0.01 mm, forced 0 paste on
  SOT-723/883); deliberate ones kept.
- b135 hand edits moved into footprints: paste +1 % Relay_DPDT_Kemet_EE2_NUH_DoubleCoil, +5 % 750315125, -5 %
  D_SMA_BIDIR pad 1, 0.4 mm clearance on ID09P33E4GV00LF shell and pin 5.
- **Breaking:** `VHRD_Vendor` folded in (37 footprints, 7 symbols to their kind's library, new
  `VHRD_Button_Switch_THT`; vendor drawings, `*` texts and DigiKey descriptions dropped); models to
  `vendor/<Lib>.3dshapes/`; `kilib migrate` moves ids and paths; Nidec switches renamed `VHRD_Switch:CL-SB-12B-01T`,
  `CL-SB-22B-01T` (not in the map); SnapEDA `DX07P024AJ1R1500` dropped for `VHRD_Connector`'s.
- kilib map test: map targets must exist, every library folder needs a `[libraries]` row; migrated boards pass.
- **Breaking:** no top/bottom pins on `TPD6S300A`, `USB_C_Plug`, `USB_C_Receptacle_USB2.0_16P`, `LSF0102DCUR`,
  `SN74LXC2T45DCUR`, `AP64060`: updating a placed symbol moves those pins off their wires (receptacle on 13 boards).
- **Breaking:** `USB_C_Receptacle_GCT_USB4105-xx-A_16P_TopMnt_Horizontal` shield pads `SH`, not `S1`: a symbol still
  on `S1` loses the shield net.
- `VHRD_power` and templates: vhrd_brand 0.9.10 rail colours via `just look`.
- Layout by kind like KiCad's: `symbols/VHRD_*.kicad_symdir` (one file per symbol), `footprints/VHRD_*.pretty`.
- Templates Vhrd Default, IO30, Hammond-1455C801: new library names, vhrd-kicad 0.9.9 look (embedded A3/A4 sheets, net
  class colours, power symbols); `just look` redoes them, `just test` checks the sheets print; info pages say VHRD /
  vhrd.tech, IO30's title and `<body>` fixed.
- `B0505MT-1WR4` second model via `${VHRD_3DMODEL_DIR}`.
- `just test` / `lint` / `install` cover kilib; `just deploy` is `git pull`; test fixtures use a generic path.

### Removed

- `TLV75533PDBV`: alternate RT9013-33GB (obsolete); AP2112K-3.3TRG1 moves to Mpn2.
- `3dmodels/` (models under `${VHRD_3DMODEL_DIR}`), `keymaps/`, `default/`, `templates/default`, sheet copies
  `vhrd_a4/a3.kicad_wks` (embedded now).
- Templates RaspberryPi-HAT, RaspberryPi-uHAT, STM32_Nucleo-64_Morpho (KiCad ships them).
- Unused footprints `TEST`, `_TEMPLATE`; `09551667812741`'s Windows-path model line.

### Fixed

- No symbol names a KiCad stock library any more: the 37 symbols whose `Footprint` field named a stock footprint
  (side-pin copies made before the stock footprints were copied in, e.g. `STM32G0B1CETx` at
  `Package_QFP:LQFP-48_7x7mm_P0.5mm`) point at the `VHRD_*` copy (`kilib migrate --map migrate/2026-11-stock.toml
  symbols`), and 43 `ki_fp_filters` patterns name `VHRD_Connector*`, `VHRD_LED_SMD` and `VHRD_LED_THT` instead of
  the stock libraries. A test (`tests/decoupled.rs`, `kilib check --only VHRD_` over `symbols/` and
  `footprints/`) keeps it so, and that every `Footprint` field resolves in the checkout.
- 14 symbols named footprints that never existed under those names since the restructure (`vhrd_generic:` names
  of TI parts): `TS3A5223RSWR` now `VHRD_Package_DFN_QFN:TI-RSW0010A`, `SN74AC595BQBR` `TI-BQB0016A`, the ten
  `BQ769x2PFBR` `VHRD_Package_QFP:TI-PFB0048A`, `TCAN1044VDDF` `VHRD_Package_TO_SOT_SMD:TI-DDF0008A`,
  `LMR54410DBVR` `TI-DBV0006A`.
- `kilib stock import` writes a copy's 3D model as `${VHRD_3DMODEL_DIR}/vendor/VHRD_<Lib>.3dshapes/...`, where
  the library keeps its copies of KiCad's models, instead of KiCad's own `${KICAD10_3DMODEL_DIR}`, and `kilib stock
  diff` compares the same way: 31 of 60 copies now show as stock (2 before), the model path no longer counted as a
  change.
- `VHRD_Power_Protection:TPD6S300A`: RPD_G1 / RPD_G2 are passive pins (they are tied to C_CCx or GND, not driven), so ERC no longer reports the RPD_G2-to-GND tie.
- `kilib layers` refuses a board in KiCad 8's layer numbering (B.Cu 31) instead of naming the wrong layers by
  number (it turned `User.6` into a second `User.9`); open and save such a board in KiCad 10 first.
- `VHRD_Package_DFN_QFN:Texas_RUK0020B_WQFN-20-1EP_3x3mm_P0.4mm_EP1.7x1.7mm_ThermalVias` (U401/U501 TPD6S300A,
  b153a): the exposed pad's `F.Mask` opening matched its copper exactly (no margin), bridging the solder mask to
  every perimeter pad at this part's 0.4 mm pitch (33 DRC "solder mask aperture bridges" findings across the two
  parts). `solder_mask_margin -0.05` shrinks the opening 0.05 mm inside the copper, TI's own "solder mask defined"
  option for this land pattern (RUK0020B package drawing, `ti.com/lit/pdf/qfnd191`); confirmed against a scratch
  copy of the board (0 bridges left).
- Every stock-imported footprint's 3D model repointed from `${KICAD10_3DMODEL_DIR}` to
  `${VHRD_3DMODEL_DIR}/vendor/...` (54 footprints, 47 distinct models): no footprint depends on KiCad's stock 3D
  model package, which is not installed on these PCs. 43 of the 47 STEP files come straight from KiCad's own
  `kicad-packages3d` (CC-BY-SA 4.0, same licence as the rest of KiCad's library); 4 have no upstream model at all
  (`Texas_RUK0020B_WQFN-20-1EP_3x3mm_P0.4mm_EP1.7x1.7mm`, `VQFN-16-1EP_3x3mm_P0.5mm_EP1.45x1.45mm`,
  `SOIC-8-1EP_3.9x4.9mm_P1.27mm_EP2.35x2.35mm`, `Texas_R-PDSO-G5_DCK-5`) and stay reported by `kilib lint --rule
  models` until one is made or found.

## [0.1.0] - 2026-10-17

### Added

- AGENTS.md, CLAUDE.md, FEATURES.md, CHANGELOG.md and a justfile (`look`, `test`, `lint`, `install`, `deploy`).

### Changed

- The VHRD look from vhrd_brand's `vhrd-kicad`: `templates/vhrd_a4.kicad_wks` and
  `vhrd_a3.kicad_wks` replaced by the new drawing sheets (red edge bar, zone ring with red corner marks, open title
  block on the same fields: PRODUCT_NAME, BOARD_NUMBER, title, REVISION, COMMENT1-4, VHRD_STAMP); boards that use
  them by path get the new frame. `vhrd_power` symbols coloured: supplies orange, grounds (ISO, CHASSIS) grey. The
  "Vhrd Default" template embeds the new A3 sheet, its net classes carry the signal colours, and it has the
  VHRD_STAMP text variable.
