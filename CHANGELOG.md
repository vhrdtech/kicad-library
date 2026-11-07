# Changelog

## [Unreleased]

### Added

- `VHRD_Logic_LevelTranslator:SN74LXC2T45DCUR` (1.1-5.5 V), `SN74LVC2T45DCUR` (TI, 1.65-5.5 V), `74LVC2T45DC`
  (Nexperia, 1.2-5.5 V), `SN74AVC2T45DCUR` (1.2-3.6 V) and `SN74AXC2T45DCUR` (0.65-3.6 V), 2-bit dual-supply
  transceivers on stock VSSOP-8 2.3x2 mm with one pinout and one drawing (A side left, each Bn facing its An, DIR on
  its own). One symbol each, not alternates of one another: their supply ranges and pull-downs differ.
- `VHRD_Logic_LevelTranslator:LSF0102DCUR`, `PCA9306DCUR` (TI) and `PCA9306DC1Z` (NXP) on stock VSSOP-8 2.3x2 mm,
  `PCA9306DP` (NXP) and `NVT2002DP` on stock TSSOP-8 3x3 mm: 2-bit pass-FET level translators with one pinout and one
  drawing (A side left, each Bn facing its An, VREF_B a passive pin since it is fed through about 200 kOhm), each with
  its datasheet's pin names. One symbol each, not alternates of one another: their reference voltage ranges differ
  (TI PCA9306 VREF1 from 1.2 V, NXP PCA9306 up to 5 V, NVT2002 VREFA up to 5.4 V, LSF0102 0-5.5 V). The footprints
  carry KiCad's own 3D models.
- `VHRD_Connector:USB_C_Receptacle_USB2.0_16P`, KiCad 10's stock symbol (shield pin `SH`) with the GCT USB4105
  footprint and `Mpn1` USB4105-GF-A, so a KiCad library update cannot renumber it under the boards again. Pins sit
  where the stock symbol has them: changing a placed stock symbol to this one keeps its wires.
- `VHRD_Power_Protection:TPD6S300A`, from KiCad's stock symbol: each connector-side pin (C_CC1, C_CC2, C_SBU1,
  C_SBU2) faces its system-side pin on one row, `Mpn1` TPD6S300ARUKR and `Mpn2` TPD8S300ARUKR (pin to pin; pins 16 /
  17 are N.C. on TPD6S300A and D4 / D3 on TPD8S300A, and TPD8S300A needs VPWR at or below 3.6 V). The pins come in
  groups with a free row between them (CC, SBU, D, RPD); pins 16 / 17 are visible as `NC/D3` and `NC/D4`, and only
  the grounds and the pad are stacked under one GND pin.
- `VHRD_TestPoint:TestPoint_Pad_D1.2mm`, a 1.2 mm SMD test point pad.
- `VHRD_TestPoint:TestPoint_Pad_0.8x0.8mm`, a square 0.8 × 0.8 mm SMD test point pad.
- `LICENSE.md`: the library is CC-BY-SA 4.0 with KiCad's library exception (designs that use it stay yours);
  `tools/kilib` is MIT.
- `README.md` for outside users: what the library is, its layout, setting it up with `kilib setup` or by hand,
  migrating an older board, and the licence.
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
- Side-pin copies of B153's stock symbols, every pin on the left or right edge, none on top or bottom (supplies top
  left, ground bottom left): `VHRD_MCU_ST_STM32G0:STM32G0B1CETx`, `VHRD_Interface_USB:USB2514B_Bi`,
  `VHRD_Amplifier_Current:INA181`, `VHRD_Analog_Switch:TS5A3160DCK` (one unit, datasheet pin names NO, NC, COM, IN),
  `VHRD_Logic_LevelTranslator:TXS0102DCU`, `VHRD_Power_Protection:D3V3XA4B10LP` and
  `VHRD_Regulator_Linear:TLV70233_SOT23-5`.
- `VHRD_MCU_ST_STM32G0:STM32G0B1CETx` names its pins as DS13560 Table 12 prints them (`PF2-NRST`, `PA14-BOOT0`,
  `PA11[PA9]`, `PA12[PA10]`, the OSC and VDDA/VSSA functions), adds the UCPD dead-battery pins (CC1/CC2 on PA8, PB15,
  PD0, PD2 carry an Rd at reset while their DBCC pin PA9, PA10, PD1, PD3 is high) and marks pins 30/31 as VSS and
  VDDIO2 on the `N` part.

### Changed

- `VHRD_MCU_ST_STM32G0:STM32G0B1CETx`: every port pin offers its GPIO modes as alternates, `PA9/PP` (push-pull,
  output), `PA9/OD` (open drain), `PA9/IN` (input) and `PA9/FX` (flex, bidirectional), in place of the bare `PA9`;
  the remappable pins 33 and 34 offer them for both their ports.
- `VHRD_MCU_ST_STM32G0:STM32G0B1CETx`: every alternate pin function starts with its port pin (`PA9/USART1_TX`,
  `PF2/LPUART2_TX`, `VREF+/VREFBUF_OUT`), so a pin with an alternate chosen still says which pin it is; on the remapped
  pins each function names the port it uses (`PA11/COMP1_OUT`, `PA9/DAC1_EXTI9`).
- The fab layer shows only the reference: Value is hidden there (still a property, so BOMs keep it) and every
  footprint has one `${REFERENCE}` at its body's centre, sized to fit (0.25 to 1.0 mm) and turned along tall bodies,
  so F.Fab stays readable on a dense board and while routing.
- Courtyards (F.CrtYd) of the 128 footprints with a STEP model are rebuilt from the model's top view: the body and
  the pads, grown by the KLC F5.3 clearance (0.25 mm, 0.15 mm for parts under 1.5 mm, 0.5 mm for connectors and
  crystals, 1.0 mm for BGA), gaps under 2 mm closed, a rectangle where the outline fills at least 87 % of its box and
  a polygon on the 0.01 mm grid otherwise. Where the old courtyard reached further on one side only (mating room on
  CUI_MSD-1-A, DX07P024AJ1R1500, the TF31-16S FPC, Pico-Lock, 1054301204 and 610112249121) that side keeps it.
  VHRD_Mechanical and VHRD_Module keep theirs. Connectors also get their full top view on `User.2`.
- Every polarized footprint has one pin-1 mark: a filled 0.48 x 0.33 mm silk triangle (KLC F5.1) just outside the
  outline next to pin 1, pointing at it, 0.2 mm from copper, in place of the old dots, squares and triangles. Diodes
  and LEDs keep their cathode bars (the bar is the mark there, no triangle) and arrows, marks inside the body stay,
  and bidirectional diodes get none.
- Every footprint is saved by KiCad 10 (`kicad-cli fp upgrade`).
- Pads follow the board's mask and paste rules again: the vendor-export 0.102 mm solder mask margin is gone from
  BAT-HLD-001-THM, BAT_3013, CUI_MSD-1-A, 1-1734264-1, 09551667812741, 3220-10-0300-00, 20021321-00008T4LF,
  B0505MT-1WR4, DM-OLED042-647, D5V0P4UR6SO-7, PA1134NL and some pads of JUST-SCREW-IT-6P and TC2050-IDC-NL, as are
  the 0.2 mm on 009155005741006, the -0.01 mm on AXF382700 and the forced 0 paste ratio on SOT-723 and SOT-883.
  Deliberate overrides stay (Tag-Connect pads, BGA and module mask, paste windows, fine-pitch paste).
- Hand edits from b135 moved into the footprints: paste ratio +1 % on Relay_DPDT_Kemet_EE2_NUH_DoubleCoil, +5 % on
  750315125, -5 % on pad 1 of D_SMA_BIDIR, and 0.4 mm clearance on the shell pads and pin 5 of ID09P33E4GV00LF.
- **Breaking:** the private `VHRD_Vendor` library is folded into this one and gone. Its 37 footprints and 7 symbols
  sit in the library their kind belongs to (`VHRD_Package_SO`, `VHRD_Button_Switch_SMD`, `VHRD_Connector_USB`, the
  new `VHRD_Button_Switch_THT` and others; `migrate/2026-11.toml` lists each), with the vendor drawing copies on
  Dwgs.User / Cmts.User, the `*` and spare `REF**` texts and the DigiKey descriptions replaced by our own (a silk dot
  marks pin 1 where a `*` did). The Nidec slide switch symbols are `VHRD_Switch:CL-SB-12B-01T` and `CL-SB-22B-01T`; the
  SnapEDA copy of `JAE DX07P024AJ1R1500` is dropped for the one in `VHRD_Connector`. Their 3D models moved from
  `vendor/VHRD_Vendor.3dshapes/` to `vendor/<FootprintLib>.3dshapes/`. `kilib migrate --map migrate/2026-11.toml`
  moves a board's `VHRD_Vendor:` ids and model paths; the two symbol renames are not in the map.
- kilib's map test checks the map against the library as it grows: every item the map sends into this checkout
  must be there, and every library folder needs its `[libraries]` row (the row `kilib setup` writes). It no longer
  asks that every item be a target of the map. The boards test accepts boards that are already migrated.
- **Breaking:** `VHRD_Power_Protection:TPD6S300A`, `VHRD_Connector:USB_C_Plug`,
  `VHRD_Connector:USB_C_Receptacle_USB2.0_16P` (GND and SHIELD on the left edge), `VHRD_Logic_LevelTranslator:LSF0102DCUR`,
  `VHRD_Logic_LevelTranslator:SN74LXC2T45DCUR` and `VHRD_Regulator_Switching:AP64060` have no pin on the top or bottom
  edge any more: supplies and grounds moved to the sides. Updating a placed symbol from the library moves those pins
  off their wires (the receptacle is on thirteen boards).

- **Breaking:** `VHRD_Connector_USB:USB_C_Receptacle_GCT_USB4105-xx-A_16P_TopMnt_Horizontal` numbers its four shield
  pads `SH` instead of `S1` (saved with KiCad 10). A board whose connector symbol still numbers the shield pin `S1`
  loses the shield's net on its next update from the schematic.
- `VHRD_power` and the templates take the rail colours of vhrd_brand 0.9.10 through `just look`: ground symbols green, Ground net class dark green, Power net class burnt orange.
- The library is laid out by kind, like KiCad's own: symbols in KiCad 10 `symbols/VHRD_*.kicad_symdir` libraries (one
  file per symbol), footprints in `footprints/VHRD_*.pretty`, each named `VHRD_` plus the KiCad library it would
  belong to. `migrate/2026-11.toml` says where each old symbol, footprint and 3D model went; `kilib migrate` moves a
  board. Third-party footprints and symbols moved to the private `VHRD_Vendor` library (`${VHRD_VENDOR_LIBRARY_DIR}`).
- The templates Vhrd Default, IO30 and Hammond-1455C801 use the new library names and carry the current VHRD look from
  vhrd-kicad 0.9.9: the A3 sheet embedded in the schematic, the A4 sheet in the board, net class colours and coloured
  power symbols. `just look` colours `VHRD_power` and redoes all three (`vhrd-kicad adopt --power`, then `vhrd-kicad
  embed` of the A4 sheet into each board); `just test` checks that each template's schematic and board print their
  embedded VHRD sheet. Grounds and `PWR_FLAG` in `VHRD_power` are grey now.
- Footprint model paths: `B0505MT-1WR4`'s second model goes through `${VHRD_3DMODEL_DIR}` instead of a Windows path.
- `just test` also runs kilib's tests (`just test --changed`: kilib's changed crates only), `just lint` checks its
  format and clippy, and `just install` builds and installs `kilib`.
- The templates' `meta/info.html` titles and copyright lines say VHRD and vhrd.tech, not `Vhrd.Tech`; IO30's info
  page had Vhrd Default's title and a broken `<body>` tag, both fixed.
- `just deploy` is a plain `git pull` instead of a tpm command.
- kilib's test fixtures use a generic `C:/Users/someone` path instead of a real name.

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

- The VHRD look from vhrd_brand's `vhrd-kicad`: `templates/vhrd_a4.kicad_wks` and
  `vhrd_a3.kicad_wks` replaced by the new drawing sheets (red edge bar, zone ring with red corner marks, open title
  block on the same fields: PRODUCT_NAME, BOARD_NUMBER, title, REVISION, COMMENT1-4, VHRD_STAMP); boards that use
  them by path get the new frame. `vhrd_power` symbols coloured: supplies orange, grounds (ISO, CHASSIS) grey. The
  "Vhrd Default" template embeds the new A3 sheet, its net classes carry the signal colours, and it has the
  VHRD_STAMP text variable.
