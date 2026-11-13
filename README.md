VHRD KiCad library
---

VHRD's own KiCad parts: the symbols and footprints behind VHRD's boards, three project templates, and `kilib`, a
small CLI that sets up and checks the library. The symbols, footprints and templates are free to reuse (see
"Licence" below); 3D models and third-party parts live outside this repo.

## Layout

Symbols live one per file in `symbols/VHRD_<KiCad library>.kicad_symdir` (KiCad 10's own layout), footprints in
`footprints/VHRD_<KiCad library>.pretty`: each library is named `VHRD_` plus the stock KiCad library it would
belong to, e.g. `VHRD_Connector_Molex` or `VHRD_Package_DFN_QFN`. Every part, whoever drew it first, is sorted the same
way; there is no separate vendor library (the private `VHRD_Vendor` one was folded in on 7 Nov 2026). KiCad's stock
footprints and symbols that a board uses are copied into the `VHRD_*` library of the same name (`VHRD_Resistor_SMD`,
`VHRD_Transistor_BJT`), so a KiCad update never changes them unannounced; `kilib stock diff` shows what upstream
changed since. No symbol of the library names a stock footprint or library: KiCad's stock libraries
need not be installed (`kilib check --only VHRD_`).

## Configuration

The library has two parts, each found through a KiCad path variable:

- `VHRD_LIBRARY_DIR`: this checkout, with `symbols/VHRD_*.kicad_symdir` and `footprints/VHRD_*.pretty`.
- `VHRD_3DMODEL_DIR`: the 3D models, in `own/` and `vendor/`. They are not part of this repo.

`kilib setup --map migrate/2026-11.toml --models-dir DIR` writes the library rows and the variables
into this PC's KiCad config (see below). `KICAD_USER_TEMPLATE_DIR` points at `templates/`, so File, New Project from
Template offers "Vhrd Default", IO30 and Hammond-1455C801. Each template embeds its drawing sheet.

Without `kilib setup`, set the two variables and the library rows by hand: in KiCad's "Configure Paths", point
`VHRD_LIBRARY_DIR` at this checkout and `VHRD_3DMODEL_DIR` at the 3D models' folder. Then add a row per library in "Manage
Symbol Libraries" / "Manage Footprint Libraries": name it after the folder, type KiCad, and point it at
`${VHRD_LIBRARY_DIR}/symbols/<name>.kicad_symdir`, and the
same under `footprints/<name>.pretty` for footprints.

## Checking boards: kilib

`tools/kilib` is a small Rust CLI (MIT) that knows how KiCad finds libraries. `just install` builds it and puts
`kilib` on PATH.

- `kilib check PATH...` reads every `.kicad_sch`, `.kicad_pcb`, `.kicad_sym` and `.kicad_mod` under the paths and
  checks that each symbol (`lib_id`), `Footprint` field, footprint and 3D model resolves on this PC. It uses the
  global `sym-lib-table` and `fp-lib-table` and the "Configure Paths" variables of the KiCad config folder
  (`KICAD_CONFIG_HOME`, else `~/.config/kicad`, the newest version), each project's own tables, `${KIPRJMOD}`, the
  environment (which wins, as in KiCad) and the older `KICAD6..9_3DMODEL_DIR` falling back to the newest set. It exits
  1 with the list of what does not resolve; `--json` gives the same as JSON. `--config-dir`, `--sym-lib-table`,
  `--fp-lib-table` and `--var NAME=VALUE` check against another PC's setup. `--only VHRD_` also fails on anything
  from a library whose name does not start with the prefix: a symbol, footprint or `Footprint` field, a schematic's
  embedded symbols (`lib_symbols`) and the library part of `ki_fp_filters`, so a board that still needs KiCad's stock
  libraries shows.
- `just check-boards` runs it on the hw checkout beside this one (or `VHRD_BOARDS_DIR`) and skips with a note where
  there is none.
- `kilib setup --dry-run` shows what `kilib setup` would write into this PC's KiCad config. The library rows go
  through `${VHRD_LIBRARY_DIR}` (this checkout) and `${VHRD_VENDOR_LIBRARY_DIR}` (the private vendor library,
  `--vendor-dir`). They come from a rename map's `[libraries.symbol]` and `[libraries.footprint]` tables (`--map
  migrate/2026-11.toml`), or else from the library folders found. They replace every `vhrd_*` and `VHRD_*` row. The
  variables are `VHRD_LIBRARY_DIR`, `VHRD_VENDOR_LIBRARY_DIR` and `VHRD_3DMODEL_DIR` (`--models-dir`), each only when
  given, plus `VHRD_TEMPLATE_DIR` and `KICAD_USER_TEMPLATE_DIR` for the drawing sheets. Every other row and setting
  stays as it is. It refuses while KiCad runs.
- `kilib migrate --map migrate/2026-11.toml PATH...` moves boards to a new library layout. It rewrites only the
  strings the map matches, and every other byte stays as it was: symbol ids (`lib_id`, `lib_name` and the schematic's
  `lib_symbols` cache), `Footprint` fields, `ki_fp_filters` entries, footprint ids, `${VHRD_3DMODEL_DIR}` model paths
  (and absolute paths into a `kicad-library/3dmodels` folder), and the old `vhrd_*` rows of a project's
  `sym-lib-table` and `fp-lib-table`, which become the rows of the new libraries. An id is looked up in `[symbol]` /
  `[footprint]` first, then its old library in `[symbol_lib]` / `[footprint_lib]`. `[filter_lib]` renames the library part of
  `ki_fp_filters` patterns (`"Connector*" = "VHRD_Connector*"`). `[per_package."Lib:Generic"]` moves a placed
  generic symbol to its per-package symbol by the footprint it is placed with (below); the schematic's `lib_symbols`
  cache gets a copy of the generic under the new name, with the footprint in `Footprint` and `ki_fp_filters`. It prints a unified diff (`git
  apply` takes it inside the folder given); `--write` applies it and `--stat` shows only the summary. A footprint the
  map dropped (`""`) that a board still uses, or two cached symbols that would get one name, is an error: it exits 1
  and writes nothing. Names the map does not know are listed and left as they are. A second run changes nothing.
- `kilib per-package --map migrate/2026-11-packages.toml [BOARD...]` makes per-package symbols. A generic symbol
  (`VHRD_Device:R`) leaves `Footprint` empty, so what is keyed by the library symbol (a footprint review, a BOM line)
  cannot cover it; each package it is placed with gets a symbol of its own (`VHRD_Device:R_0402`), derived from the
  generic in its library (`extends`), with `Footprint` and `ki_fp_filters` set to the footprint. The map's
  `[per_package]` tables name them. Those the checkout lacks are listed (exit 1) or written with `--write`; those
  there are checked. With boards, it lists the generic symbols their schematics place, by footprint, with the map's
  symbol or "not in the map", and their references. Then `kilib migrate --map migrate/2026-11-packages.toml BOARD`
  moves the board onto them. The generic symbols stay for new designs.
- `kilib restructure --map migrate/2026-11.toml --vendor-dir DIR` moves this checkout's own symbols and footprints
  into the map's new libraries. Each `symbols/<old>.kicad_sym` becomes one `.kicad_sym` per symbol in
  `symbols/<New>.kicad_symdir`, as KiCad 10's own libraries are laid out (a `/` in a name is `{slash}` in the file
  name; a derived symbol goes to its parent's library). Footprints move to `footprints/<New>.pretty`. Libraries under
  `${VHRD_VENDOR_LIBRARY_DIR}` go to `DIR`. The text of each symbol and footprint stays as it was, except what
  `migrate` rewrites in it (model paths, `Footprint` fields). The old files are removed once the new ones are written.
  `--dry-run` lists every move.
- `kilib designators [PATH...]` keeps each footprint's designator on its own layer only (default: this
  checkout's `footprints/`; `kilib fab-designators` is its old name). F.Fab and B.Fab carry the drawing alone
  (outline, pin-1 and polarity marks, inner lines), no text. A real part's designator goes to `Ref.Des.F` (User.23;
  `Ref.Des.B`, User.24, for a bottom-side footprint); test points, net ties (net ties, spark gaps, solder jumpers)
  and mechanical or graphic footprints (mounting holes, fiducials, logos) use their own layers `Ref.Test`, `Ref.Tie`
  and `Ref.Mech`, the class decided from the footprint's library and name and the designator's prefix (TP; NT, E,
  JP; H, FID). The `Reference` property goes there, visible and unlocked (a mechanical one hidden, where it was);
  `${REFERENCE}` texts on the silkscreen, the fab layer and the designator's layer go. The text's ink, measured in
  KiCad's stroke font with its strokes, sits centred on the body: the fab outline, else the courtyard. It may run
  over the pads, since KiCad draws the selected layers on top of them, while text outside the body would run into
  the neighbouring parts. A library footprint is sized for the longest designator of its kind (`R1004`, `C1001`,
  `TP526`, `JP104`, `U1004`). 0.02 mm stays free between the text's strokes and every line of the fab drawing of the
  same side (outline, polarity and pin-1 marks, diode symbols, inner lines), so the two read together. It gets the
  largest size from 1 mm down to 0.2 mm that fits on the centre, level or turned 90°. Where neither fits, it moves to
  the largest free spot inside the body nearest the centre. The smallest parts (0201, 01005, SOD-523) may go down to
  0.15 mm, and the report lists them as "tiny". Where nothing fits at all it stays on the centre at 0.2 mm, and the
  report lists the footprint as "no clean fit". The text's stroke is 15 % of its size, at least 0.025 mm. Every line
  on F.Fab and B.Fab gets a 0.05 mm stroke. A footprint that already follows these rules is left alone. It lists
  what would change, with how many designators move, get resized and turn, and the sizes given, and exits 1;
  `--write` writes it. Boards take the new footprints through pcbnew's "Update Footprints from Library". A real
  part's `Value` property goes to `Value.F` (User.17; `Value.B`, User.18, on the back), shown and unlocked, centred
  and fitted the same way for the longest typical value of its kind (`100K`, `100nF`, `4.7uH`, `BAS16TW`,
  `TPS54202`). The designator is no obstacle to it: Ref.Des and Value are both centred, so they are separate layers
  shown one at a time. The Value of a test point, net tie or mechanical footprint stays hidden on the fab layer,
  where it was.
- `kilib designators --board BOARD.kicad_pcb` fits each placed footprint's own designator the same way, so `R7`
  gets larger text than `R1004` would, and each real part's Value on `Value.F`/`Value.B` for its own text (`10K`).
  It touches only footprints whose Reference (and Value) is already on its layer (as the library puts it), and of
  those only the two properties' position, angle, size and thickness; the angle is written upright, as KiCad draws
  it. `--write` writes the board.
- `kilib layers BOARD.kicad_pcb...` names the user layers the library uses in a board's layer table, front and back
  pairs so KiCad flips a bottom-side footprint's text to the back one: User.9/10 `Ref.Test.F`/`Ref.Test.B` (test
  points), User.11/12 `Ref.Tie.F`/`Ref.Tie.B` (net ties, spark gaps, solder jumpers), User.13/14
  `Ref.Mech.F`/`Ref.Mech.B` (mounting holes, fiducials, logos; designator hidden), User.15/16 `Rooms.F`/`Rooms.B`
  (hand-drawn outlines of the circuit blocks), User.17/18 `Value.F`/`Value.B` (each real part's Value), User.19/20
  `Pins.F`/`Pins.B` (pin numbers), User.21/22 `Centroid.F`/`Centroid.B` (a cross on each placed footprint's origin),
  User.23/24 `Ref.Des.F`/`Ref.Des.B` (the designators of real parts).
  KiCad's own `Dwgs.User`, `Cmts.User` and `User.2` are named `Dimensions`, `Fab.Notes` and `Body.F` (a part's full
  top view, a connector's body from its STEP model); a board that draws on User.2 itself keeps it unnamed, and the
  report says so. A board still in KiCad 8's layer numbering is refused: open and save it in KiCad 10 first. The
  templates have them; a board made before needs them once, with
  `kilib layers --write` or by hand in Board Setup > Board Editor Layers (add the user layers, name them, set
  their type to front or back). A number the board already uses under another name is left alone and reported.
  Only the layer table changes.
- `kilib lint [PATH...]` checks footprints against the library rules (default: this checkout's `footprints/`;
  `--rule` picks some of them) and lists, per footprint, what each fix changes and what is left for a person
  (`--left` only that, `--json` as JSON). It exits 1 while a fix is not written; `--write` writes them. The designator
  belongs to `designators`: run it after `--write`, since a new chamfer or courtyard can move it. The rules:
  - `pin1`: a part with three or more pins, and a polar two-pin part (diode, LED, battery, polarised capacitor; pin 1
    is the cathode or the plus), marks pin 1 on the fab layer, and on the silkscreen where it draws one. The pads
    often fit the part turned half or a quarter round; for each such turn the drawing must show a mark (a dot, a
    filled shape, a chamfered corner, a diode's cathode bar, a `1` or `+`) by pin 1 and not by the pad the turn takes
    it to. A part is never mirrored, so a SOIC's notch counts. The fix chamfers the pin-1 corner of a rectangular
    fab outline (1 mm, at most a quarter of the shorter side) and puts a filled triangle pointing at pin 1 on the
    silkscreen, outside the body and 0.15 mm clear of the pads. Marks that are missing elsewhere, ambiguous or by
    the wrong pad are reported.
  - `attrs`: test points, net ties, spark gaps, jumpers, logos and mounting holes are excluded from the BOM and the
    position files, with no SMD or through-hole type (net ties and logos may have no courtyard); fiducials are SMD
    and excluded from the BOM only; a part is SMD when all its copper pads are, through-hole when one is. `board_only`
    is not set, since boards place these from the schematic. Mechanical footprints (screws, spacers, the enclosure,
    panel features) and parts kept out of the BOM are only reported.
  - `courtyard`: on F.CrtYd (B.CrtYd for a bottom-side footprint), closed, at least 0.25 mm from every pad and from
    the fab drawing, 0.1 mm for 0201, 0402 and 0603 passives (IPC-7351 nominal). The fix draws a missing one as a
    rectangle on a 0.01 mm grid, 0.05 mm wide, and grows one that is a single rectangle; other shapes are reported.
  - `models`: every part has a 3D model under `${VHRD_3DMODEL_DIR}` or `${KICAD10_3DMODEL_DIR}`, and the file exists
    (where the variables are set: KiCad's config, the environment, `--var NAME=DIR`; `--no-model-files` skips the
    files). The fix moves stock models from older KiCad variables to `${KICAD10_3DMODEL_DIR}` (`.wrl` to `.step`).
  - `silk`: lines are 0.12 mm (filled marks keep their stroke); no line reaches onto a pad's solder mask opening (the
    pad's mask expansion, else the footprint's, else 0); no text but pin-1 marks. The fix sets the widths and cuts
    lines, and the sides of rectangles and polygons, round the pads. Arcs and curves over a pad, and texts, are
    reported. Logos and other artwork are left alone.
  - `ref-outline`: a test point draws its pad's outline on its designator layer `Ref.Test` (User.9 / User.10): a
    circle 0.05 mm outside a round pad, else a rectangle 0.05 mm round it. A net tie, spark gap or solder jumper draws
    a rectangle 0.05 mm round its pads on `Ref.Tie` (User.11 / User.12). Lines are 0.05 mm, and the drawing replaces
    anything else on that layer. Mechanical footprints draw nothing on `Ref.Mech`. `designators` then fits the
    designator to this drawing as to a fab outline, and puts it beside the drawing (0.3 mm, else 0.2 mm, nearest)
    where nothing fits inside.
  - `pins`: pin numbers on `Pins.F` (User.19; `Pins.B`, User.20, on the back): every pin of a connector; pin 1 and
    every 5th (every 10th past 40 pins) of another part with three or more pins; pin 1 only where the pins are not
    numbered with digits (a BGA's `A1`); none on two-pin parts, test points, net ties, logos and holes, and none for
    a pad on the copper of one already numbered (USB-C's `B12` on `A1`). Each is a level stroke text beside its pad,
    0.1 mm clear of every pad, hole and other number: on a side with no other pad in line within 2 mm, at a long
    pad's outer end, else facing away from the middle of the pads. A footprint gets one size for all its numbers,
    the largest of 0.5 to 0.2 mm at which all fit; one that fits nowhere is reported. The fix replaces anything else
    on the layer.
  - `centroid`: a cross on `Centroid.F` (User.21; `Centroid.B`, User.22) at every part's and fiducial's origin, the
    point the position file gives: two 0.5 mm lines, 0.05 mm wide. Show the layer over the board to check the
    placement data. The fix replaces anything else on the layer.
- `kilib stock import Lib:Name...` copies KiCad stock footprints to `footprints/VHRD_<Lib>.pretty/<Name>.kicad_mod`:
  the fixes of `kilib lint`, the designator and the Value placed by the `designators` rules, the fab lines at 0.05 mm, the 3D model at the library's copy of KiCad's model, `${VHRD_3DMODEL_DIR}/vendor/VHRD_<Lib>.3dshapes/` (the STEP file is copied there by hand), every other byte as in stock (pads). The stock folder is `--stock-dir`,
  else `KICAD10_FOOTPRINT_DIR` (environment or KiCad's "Configure Paths"), else `/usr/share/kicad/footprints`. A
  footprint the library has already is left alone. It lists what it would copy and exits 1; `--write` copies. `kilib
  stock diff` compares every `VHRD_<Lib>:<Name>` that has a stock `<Lib>:<Name>` with a fresh import of it and prints
  the unified diff (what taking in the stock footprint would change; `--stat` only lists them); it exits 1 when any
  differs. Footprints of our own that share a stock name show up there too. `migrate/2026-11-stock.toml` maps the
  imported stock ids to the copies for `kilib migrate`.
- `kilib stock import --symbols Lib:Name...` does the same for stock symbols, into
  `symbols/VHRD_<Lib>.kicad_symdir/<Name>.kicad_sym`: a derived symbol is made standalone (its parent's pins, units and
  graphics under its own name and fields), the `Footprint` field points at our copy of the footprint where
  `footprints/` has one, library parts of `ki_fp_filters` patterns get `VHRD_`, and an empty `Mpn1` is added. The
  stock folder is `--stock-symbol-dir`, else `KICAD10_SYMBOL_DIR`, else `/usr/share/kicad/symbols`. `kilib stock
  diff` compares the symbols too; a copy's fields and flags are ours, laid over the fresh import, so the diff shows
  what stock changed in pins, units and graphics and the fields stock has that the copy has not. A kind whose stock
  folder is not found is skipped with a note.
- `kilib model chip [NAME...]` writes the STEP models of the chip passives and power inductors in `generate/`
  (`chip-passives.toml`: chip R, C and L, body length, width, termination band and the heights each size is made in;
  `power-inductors.toml`: power-inductor bodies, bottom terminations and lands), one per size and height, as
  `${VHRD_3DMODEL_DIR}/generated/VHRD_<Lib>.3dshapes/<footprint>.step` (`--out DIR` for another folder). Each model is
  the body and its terminations as coloured solids on the board, centred on the footprint origin: an MLCC tan with
  nickel-tin ends, a resistor's white alumina with its black overcoat, ferrite or ceramic chip inductors, dark power
  inductors with their bottom electrodes. The STEP file is written by kilib itself (AP214, straight prisms as
  B-rep solids with a colour each), no CAD kernel involved. It lists what it would write and exits 1; `--write`
  writes.
- Shell completion: `source <(COMPLETE=bash kilib)` (or `zsh`, `fish`).

### Migrating an older board

```sh
kilib migrate --map migrate/2026-11.toml path/to/board            # look at the diff
kilib migrate --map migrate/2026-11.toml --write path/to/board    # apply it
kilib check path/to/board                                         # everything resolves
```

## Licence

The symbols, footprints and templates are CC-BY-SA 4.0 with KiCad's library exception — designs that use them stay
yours; see [LICENSE.md](LICENSE.md). `tools/kilib` is MIT; see [tools/kilib/LICENSE](tools/kilib/LICENSE).

## KiKit

Install KiKit for panelization support: [Installation](https://yaqwsx.github.io/KiKit/latest/installation/intro)

Install just to create shortcuts for various commands like: `just panelize`:
[Just](https://github.com/casey/just#installation)

[Panelization Examples](https://yaqwsx.github.io/KiKit/latest/panelization/examples/)
[Panelization Docs](https://yaqwsx.github.io/KiKit/latest/panelization/cli/)

### Arch

`sudo pacman -S python-kikit`

