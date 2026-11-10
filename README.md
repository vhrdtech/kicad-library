VHRD KiCad library
---

VHRD's own KiCad parts: the symbols and footprints behind VHRD's boards, three project templates, and `kilib`, a
small CLI that sets up and checks the library. The symbols, footprints and templates are free to reuse (see
"Licence" below); 3D models and third-party parts live outside this repo.

## Layout

Symbols live one per file in `symbols/VHRD_<KiCad library>.kicad_symdir` (KiCad 10's own layout), footprints in
`footprints/VHRD_<KiCad library>.pretty`: each library is named `VHRD_` plus the stock KiCad library it would
belong to, e.g. `VHRD_Connector_Molex` or `VHRD_Package_DFN_QFN`. Every part, whoever drew it first, is sorted the same
way; there is no separate vendor library (the private `VHRD_Vendor` one was folded in on 7 Nov 2026).

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
  `--fp-lib-table` and `--var NAME=VALUE` check against another PC's setup.
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
  `[footprint]` first, then its old library in `[symbol_lib]` / `[footprint_lib]`. It prints a unified diff (`git
  apply` takes it inside the folder given); `--write` applies it and `--stat` shows only the summary. A footprint the
  map dropped (`""`) that a board still uses, or two cached symbols that would get one name, is an error: it exits 1
  and writes nothing. Names the map does not know are listed and left as they are. A second run changes nothing.
- `kilib restructure --map migrate/2026-11.toml --vendor-dir DIR` moves this checkout's own symbols and footprints
  into the map's new libraries. Each `symbols/<old>.kicad_sym` becomes one `.kicad_sym` per symbol in
  `symbols/<New>.kicad_symdir`, as KiCad 10's own libraries are laid out (a `/` in a name is `{slash}` in the file
  name; a derived symbol goes to its parent's library). Footprints move to `footprints/<New>.pretty`. Libraries under
  `${VHRD_VENDOR_LIBRARY_DIR}` go to `DIR`. The text of each symbol and footprint stays as it was, except what
  `migrate` rewrites in it (model paths, `Footprint` fields). The old files are removed once the new ones are written.
  `--dry-run` lists every move.
- `kilib fab-designators [PATH...]` keeps each footprint's designator on the fab layer only (default: this
  checkout's `footprints/`). The `Reference` property goes to F.Fab (B.Fab for a bottom-side footprint), visible and
  unlocked; `${REFERENCE}` texts on the silkscreen, and a second one on the fab layer, go. The text gets the largest
  size up to 1 mm whose box, for a four-character designator in KiCad's stroke font, fits inside the fab outline at
  least 0.1 mm from every pad, at 0° or 90°. When not even 0.25 mm fits there (pads under the body, an exposed pad),
  it goes next to the body instead, level and above it where it can. A footprint without a fab outline round its
  pads is measured by its courtyard. A footprint that already follows these rules is left alone. It lists what
  would change and exits 1; `--write` writes it. Boards take the new footprints through pcbnew's "Update
  Footprints from Library".
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

