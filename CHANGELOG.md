# Changelog

## [Unreleased]

### Added

- `VHRD_Interface_Expansion` symbol library (needs `kilib setup`): `TCA6408ARGT`, stock drawing with bus, supply
  and control pins left, P0-P7 right; `Mpn1` TCA6408ARGTR, `Mpn2` PCAL6408ABS (same pinout, VQFN-16 3x3).
- `VHRD_Diode:BZX585-C5V1`, `BZX585-C5V6` on `VHRD_Diode_SMD:D_SOD-523`; `Mpn2` BZT52C5V1T-7 / BZT52C5V6T-7 (Diodes).
- `VHRD_Diode:PESD3V3S1UB`, `PESD5V0S1UB` unidirectional ESD on `D_SOD-523`; `Mpn2` ESD5Z3.3T1G / ESD5Z5.0T1G (onsemi).
- `VHRD_Diode:PESD3V3U1UB` unidirectional ESD, 2.6 pF, on `D_SOD-523`.
- `VHRD_Transistor_FET:NX3020NAKS` dual N-MOSFET from stock `Q_Dual_NMOS_S1G1D2S2G2D1`, on stock `SOT-363_SC-70-6`.
- `VHRD_Power_Protection:PESD2CANFD24V-T` dual 24 V CAN FD TVS from stock `NUP2105L` (SOT-23, pin 3 GND); `Mpn2` ESD2CANFD24DBZRQ1, `Mpn3` ESD2CANFD24DBZR (TI, same pinout).
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

### Changed

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
