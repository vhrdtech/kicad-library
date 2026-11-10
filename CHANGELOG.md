# Changelog

### Fixed

- `kilib layers` refuses a board in KiCad 8's layer numbering (B.Cu 31) instead of naming the wrong layers by
  number (it turned `User.6` into a second `User.9`); open and save such a board in KiCad 10 first.

## [Unreleased]

### Added

- More named layers (`kilib layers`, the three templates): User.17/18 `Value.F/B`, User.19/20 `Pins.F/B`,
  User.21/22 `Centroid.F/B`, typed front and back; KiCad's `Dwgs.User` named `Dimensions`, `Cmts.User` `Fab.Notes`
  and `User.2` `Body.F` (a part's full top view). `kilib layers` leaves User.2 unnamed on a board that draws on it
  itself and says so.

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

- `3dmodels/` (models under `${VHRD_3DMODEL_DIR}`), `keymaps/`, `default/`, `templates/default`, sheet copies
  `vhrd_a4/a3.kicad_wks` (embedded now).
- Templates RaspberryPi-HAT, RaspberryPi-uHAT, STM32_Nucleo-64_Morpho (KiCad ships them).
- Unused footprints `TEST`, `_TEMPLATE`; `09551667812741`'s Windows-path model line.

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
