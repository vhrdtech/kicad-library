# Working on kicad-library

VHRD Tech's KiCad library: symbols (`symbols/VHRD_*.kicad_symdir`, one file per symbol), footprints
(`footprints/VHRD_*.pretty`), the rename maps (`migrate/`), and the project templates (`templates/`, KiCad's user
template folder: Vhrd Default, IO30, Hammond-1455C801). 3D models live outside the repo (`VHRD_3DMODEL_DIR`). Every
part, a vendor's or our own, lives in a `VHRD_*` library. Boards in `hw` depend on it, so a change here changes how
they open: keep names stable, never rename or remove a symbol or footprint a board uses; a move goes through a rename
map and `kilib migrate`.

## Stock footprints and symbols are copied in

KiCad's stock footprints that a board uses are imported into the `VHRD_*` library of the same name
(`Resistor_SMD:R_0402_1005Metric` becomes `VHRD_Resistor_SMD:R_0402_1005Metric`), so their text is ours: the
designator centred on `Ref.Des.F` and the Value centred on `Value.F` (`kilib designators`), the fab lines 0.05 mm and no
text on the fab layer. Pads, other
graphics stay as in stock, the 3D model is the library's copy of KiCad's (`${VHRD_3DMODEL_DIR}/vendor/VHRD_<Lib>.3dshapes/`, the STEP copied there by hand), apart from the fixes of
`kilib lint` (courtyard, silkscreen, pin-1 marks, attributes, pin numbers, the centroid cross), which `stock import` and `stock diff` apply too. The
copies are on purpose: a KiCad update never changes our footprints unannounced. We compare them with the stock library
and take upstream changes in deliberately, when the time is right:

```sh
kilib stock import --write Lib:Name...   # copy stock footprints in (KICAD10_FOOTPRINT_DIR, or --stock-dir DIR)
kilib stock import --symbols --write Lib:Name...   # stock symbols (KICAD10_SYMBOL_DIR, or --stock-symbol-dir DIR)
kilib stock diff                         # what a fresh import of the current stock would change in each copy
```

Stock symbols go the same way into `symbols/VHRD_<Lib>.kicad_symdir`, one file each: a derived symbol (`extends`)
becomes standalone, the `Footprint` field points at our footprint copy, `ki_fp_filters` library parts get `VHRD_`,
an empty `Mpn1` is added. Then fill in the fields by hand: the `Mpn1`.. of the specific parts, a `Footprint`
default, `in_bom`/`exclude_from_sim` for test points, net ties and mechanical parts. Fields and flags are ours:
`kilib stock diff` lays them over the fresh import, so it shows only pins, units, graphics and new stock fields. A
stock symbol the library already has under the same name with the same pins (`Device:R`, `power:PWR_FLAG`) is not
copied; the map points it at ours.

`migrate/2026-11-stock.toml` maps each imported stock id to its copy; `kilib migrate --map
migrate/2026-11-stock.toml BOARD` re-points a board (only footprint and symbol lib ids, the symbol cache names and
`Footprint` fields change). A new `VHRD_*` library also needs its row in `migrate/2026-11.toml`'s `[libraries]`
(what `kilib setup` writes).

Nothing in `symbols/` or `footprints/` names a stock library: a symbol's `Footprint` field and its `ki_fp_filters`
libraries are `VHRD_*` of this checkout (`tests/decoupled.rs`). A stock footprint a symbol needs is imported first.

## Per-package symbols

A generic symbol (`VHRD_Device:R`, `Footprint` empty) is placed on a board through a per-package symbol derived from
it (`VHRD_Device:R_0402`, `Footprint` and `ki_fp_filters` at the footprint), so the footprint review covers it.
`migrate/2026-11-packages.toml` names them; `kilib per-package --map ... --write` makes them, `kilib per-package --map
... BOARD` lists a board's placed generics, `kilib migrate --map ... BOARD` moves it (`tests/per_package.rs` keeps map
and library in step). A new package: a line in the map, then `kilib per-package --write`.

## Designator layers

User.9-24 are the library's: front/back pairs `Ref.Test`, `Ref.Tie`, `Ref.Mech` (designators of test points, net
ties, mechanical parts), `Rooms` (block outlines), `Value` (a real part's Value), `Pins` (pin numbers, `kilib lint
--rule pins`), `Centroid` (a cross on the origin) and `Ref.Des` (User.23/24, the designators of real parts), named in
the templates and by `kilib layers`; don't use those numbers for anything else. F.Fab and B.Fab carry only the
drawing, no text: Ref.Des and Value are both centred on the body, so they are separate layers shown one at a time;
the Value of a test point, net tie or mechanical part stays hidden on the fab layer. KiCad's `Dwgs.User`, `Cmts.User` and `User.2` carry the names `Dimensions`, `Fab.Notes`
and `Body.F` (a part's full top view). Which footprint goes where is `kilib::layers::class_of`, never decided by hand.

## The look comes from vhrd_brand

The drawing sheets, the colours of `VHRD_power` and the templates' embedded sheets, net class colours and power
symbols are made by `vhrd-kicad` (vhrd_brand `kicad/`, installed by its `just install`), never edited by hand:
`just look` refreshes them from the installed `vhrd-kicad`. Change the
look in vhrd_brand, land it there, then run `just look` here.

## kilib

`tools/kilib` is a Cargo workspace of its own (MIT, `tools/kilib/LICENSE`): the library `kilib` (s-expression reader,
library tables, path variables, the rename map, `check`, `setup`, `migrate`, `restructure`, `fab`, `stroke`, `layers`,
`stock`, `per_package`, `footprint`, `lint`) and the CLI `kilib_cli`
(binary `kilib`). Dependencies are pure Rust with `default-features = false`; no `-sys` crate. Tests write their own
fixtures (`kilib::testdir`) and run with a cleared environment: they never read or write a real KiCad config, and
never write the boards. `kilib check` and `kilib setup --dry-run` only read; `kilib setup` writes only the KiCad
config folder, never library files. `kilib migrate` writes only with `--write`, and then only the strings its map
matches; `kilib restructure` writes only inside the checkout and the `--vendor-dir`; `kilib stock import` writes only
new files under the checkout's `footprints/` (`symbols/` with `--symbols`), with `--write`. `kilib per-package` writes only new
symbol files under the checkout's `symbols/`, with `--write`. `kilib designators` (old name
`fab-designators`) writes only with `--write`, and then only the `Reference` and `Value` properties, `${REFERENCE}` texts and the stroke
widths of fab lines in the footprints given; with `--board` only the Reference's and the Value's `at`, `size` and
`thickness` on that board. `kilib layers` writes only with `--write`, and then only the board's `(layers ...)` table. The extended tests read the
boards (`VHRD_BOARDS_DIR`, else `../hw`) and work on a temporary copy, never on the boards themselves.

`kilib lint` writes only with `--write`, and then only the items its fixes name (attributes, model paths,
courtyards, silkscreen lines and widths, pin-1 marks, what is on the Pins and Centroid layers) in the footprints
given; it never touches the designator or the Value.

## FEATURES.md and CHANGELOG.md

[FEATURES.md](FEATURES.md) tracks what exists with stable IDs (`LIB-1`); new ids come from `tpm work claim LIB`.
[CHANGELOG.md](CHANGELOG.md) gets an entry under `## [Unreleased]` for every visible change; `tpm land --version
x.y.z` writes the version heading and the tag (there is no manifest to bump).

## Commands

```sh
just look              # VHRD_power colours and the three templates (adopt --power) from vhrd-kicad
just test              # the templates print their embedded sheets, kilib's tests (--changed: kilib only)
just lint              # no whitespace errors, kilib fmt and clippy
just check-boards      # kilib check on ../hw (or VHRD_BOARDS_DIR); skips without boards
just check-footprints  # kilib lint on footprints/: exits 1 while a fix is not written
just install           # builds kilib and puts it on PATH
```

## Commits

Small commits saying what changed and why; CHANGELOG entries under `[Unreleased]`; sessions land with `tpm land`.
