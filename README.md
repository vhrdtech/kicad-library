Vhrd.Tech KiCad library
---

## Configuration

- Preferences - Manage Symbol Libraries - Folder down - KiCad (.kicad_sym)
    - Select all files from `symbols`
- Preferences - Manage Footprint Libraries - Folder down - KiCad (...)
    - Select all folders from `footprints`
- Preferences - Configure paths
    - Set `VHRD_3DMODEL_DIR` to `3dmodels` folder
    - Set `KICAD_USER_TEMPLATE_DIR` to `templates` folder if you want to use project templates

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
- `kilib setup --dry-run` shows what `kilib setup` would write into this PC's KiCad config: one row per library of
  this checkout through `${VHRD_LIBRARY_DIR}` in place of the old `vhrd_*` rows, and the variables `VHRD_LIBRARY_DIR`,
  `VHRD_3DMODEL_DIR`, `VHRD_TEMPLATE_DIR` and `KICAD_USER_TEMPLATE_DIR`. Every other row and setting stays as it is.
  It refuses while KiCad runs.
- Shell completion: `source <(COMPLETE=bash kilib)` (or `zsh`, `fish`).

## KiKit

Install KiKit for panelization support: [Installation](https://yaqwsx.github.io/KiKit/latest/installation/intro)

Install just to create shortcuts for various commands like: `just panelize`:
[Just](https://github.com/casey/just#installation)

[Panelization Examples](https://yaqwsx.github.io/KiKit/latest/panelization/examples/)
[Panelization Docs](https://yaqwsx.github.io/KiKit/latest/panelization/cli/)

### Arch

`sudo pacman -S python-kikit`

