# Working on kicad-library

VHRD Tech's KiCad library: symbols (`symbols/vhrd_*.kicad_sym`), footprints, 3D models, project templates
(`templates/`, KiCad's user template folder) and the drawing sheets boards use by path
(`${VHRD_TEMPLATE_DIR}/vhrd_a4.kicad_wks`, `vhrd_a3.kicad_wks`). Boards in `hw` depend on it, so a change here
changes how they open: keep names stable, never rename or remove a symbol or footprint a board uses.

## The look comes from vhrd_brand

The drawing sheets, the colours of `vhrd_power` and the "Vhrd Default" template's sheet and net class colours are
made by `vhrd-kicad` (vhrd_brand `kicad/`, installed by its `just install`), never edited by hand:
`just look` refreshes them from the installed `vhrd-kicad`. Change the
look in vhrd_brand, land it there, then run `just look` here.

## kilib

`tools/kilib` is a Cargo workspace of its own (MIT, `tools/kilib/LICENSE`): the library `kilib` (s-expression
reader, library tables, path variables, `check`, `setup`) and the CLI `kilib_cli` (binary `kilib`). Dependencies
are pure Rust with `default-features = false`; no `-sys` crate. Tests write their own fixtures
(`kilib::testdir`) and run with a cleared environment: they never read or write a real KiCad config or the boards.
`kilib check` and `kilib setup --dry-run` only read; `kilib setup` writes only the KiCad config folder, never
library files.

## FEATURES.md and CHANGELOG.md

[FEATURES.md](FEATURES.md) tracks what exists with stable IDs (`LIB-1`); new ids come from `tpm work claim LIB`.
[CHANGELOG.md](CHANGELOG.md) gets an entry under `## [Unreleased]` for every visible change; `tpm land --version
x.y.z` writes the version heading and the tag (there is no manifest to bump).

## Commands

```sh
just look          # sheets, vhrd_power colours and the Vhrd Default template from vhrd-kicad
just test          # the templates and the sheets load in kicad-cli, kilib's tests (--changed: kilib only)
just lint          # no whitespace errors, kilib fmt and clippy
just check-boards  # kilib check on ../hw (or VHRD_BOARDS_DIR); skips without boards
just install       # builds kilib and puts it on PATH
```

## Commits

Small commits saying what changed and why; CHANGELOG entries under `[Unreleased]`; sessions land with `tpm land`.
