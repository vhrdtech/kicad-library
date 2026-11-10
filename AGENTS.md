# Working on kicad-library

VHRD Tech's KiCad library: symbols (`symbols/VHRD_*.kicad_symdir`, one file per symbol), footprints
(`footprints/VHRD_*.pretty`), the rename maps (`migrate/`), and the project templates (`templates/`, KiCad's user
template folder: Vhrd Default, IO30, Hammond-1455C801). 3D models live outside the repo (`VHRD_3DMODEL_DIR`). Every
part, a vendor's or our own, lives in a `VHRD_*` library. Boards in `hw` depend on it, so a change here changes how
they open: keep names stable, never rename or remove a symbol or footprint a board uses; a move goes through a rename
map and `kilib migrate`.

## Stock footprints are copied in

KiCad's stock footprints that a board uses are imported into the `VHRD_*` library of the same name
(`Resistor_SMD:R_0402_1005Metric` becomes `VHRD_Resistor_SMD:R_0402_1005Metric`), so their text is ours: the
designator on the fab layer (`kilib fab-designators`), the Value hidden. Pads, graphics and the 3D model (through
KiCad's own `${KICAD10_3DMODEL_DIR}`) stay as in stock. The copies are on purpose: a KiCad update never changes our
footprints unannounced. We compare them with the stock library and take upstream changes in deliberately, when the
time is right:

```sh
kilib stock import --write Lib:Name...   # copy stock footprints in (KICAD10_FOOTPRINT_DIR, or --stock-dir DIR)
kilib stock diff                         # what a fresh import of the current stock would change in each copy
```

`migrate/2026-11-stock.toml` maps each imported stock id to its copy; `kilib migrate --map
migrate/2026-11-stock.toml BOARD` re-points a board (only footprint lib ids and `Footprint` fields change). A new
`VHRD_*` library also needs its row in `migrate/2026-11.toml`'s `[libraries]` (what `kilib setup` writes).

## The look comes from vhrd_brand

The drawing sheets, the colours of `VHRD_power` and the templates' embedded sheets, net class colours and power
symbols are made by `vhrd-kicad` (vhrd_brand `kicad/`, installed by its `just install`), never edited by hand:
`just look` refreshes them from the installed `vhrd-kicad`. Change the
look in vhrd_brand, land it there, then run `just look` here.

## kilib

`tools/kilib` is a Cargo workspace of its own (MIT, `tools/kilib/LICENSE`): the library `kilib` (s-expression reader,
library tables, path variables, the rename map, `check`, `setup`, `migrate`, `restructure`, `fab`, `stock`) and the CLI `kilib_cli`
(binary `kilib`). Dependencies are pure Rust with `default-features = false`; no `-sys` crate. Tests write their own
fixtures (`kilib::testdir`) and run with a cleared environment: they never read or write a real KiCad config, and
never write the boards. `kilib check` and `kilib setup --dry-run` only read; `kilib setup` writes only the KiCad
config folder, never library files. `kilib migrate` writes only with `--write`, and then only the strings its map
matches; `kilib restructure` writes only inside the checkout and the `--vendor-dir`; `kilib stock import` writes only
new files under the checkout's `footprints/`, with `--write`. `kilib fab-designators` writes only
with `--write`, and then only the `Reference` property and `${REFERENCE}` texts of the footprints given. The extended tests read the
boards (`VHRD_BOARDS_DIR`, else `../hw`) and work on a temporary copy, never on the boards themselves.

## FEATURES.md and CHANGELOG.md

[FEATURES.md](FEATURES.md) tracks what exists with stable IDs (`LIB-1`); new ids come from `tpm work claim LIB`.
[CHANGELOG.md](CHANGELOG.md) gets an entry under `## [Unreleased]` for every visible change; `tpm land --version
x.y.z` writes the version heading and the tag (there is no manifest to bump).

## Commands

```sh
just look          # VHRD_power colours and the three templates (adopt --power) from vhrd-kicad
just test          # the templates print their embedded sheets, kilib's tests (--changed: kilib only)
just lint          # no whitespace errors, kilib fmt and clippy
just check-boards  # kilib check on ../hw (or VHRD_BOARDS_DIR); skips without boards
just install       # builds kilib and puts it on PATH
```

## Commits

Small commits saying what changed and why; CHANGELOG entries under `[Unreleased]`; sessions land with `tpm land`.
