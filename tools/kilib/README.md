# kilib

Checks that KiCad boards resolve their libraries on a PC, and sets up a PC's KiCad config for a library checkout.
Part of [kicad-library](../../README.md#checking-boards-kilib); MIT licensed.

```sh
kilib check ~/git/hw              # every symbol, footprint and 3D model of the boards resolves
kilib check --json board.kicad_pcb
kilib setup --dry-run             # what setup would change in this PC's KiCad config
kilib migrate --map ../../migrate/2026-11.toml ~/git/hw         # the diff that moves boards to the new layout
kilib restructure --map migrate/2026-11.toml --vendor-dir DIR --dry-run   # inside kicad-library: every move
kilib fab-designators             # inside kicad-library: footprints whose designator is not on the fab layer alone
```

- `crates/kilib`: the library (s-expression reader, library tables, path variables, the rename map, `check`, `setup`,
  `migrate`, `restructure`, `fab`).
- `crates/kilib_cli`: the `kilib` binary.

`cargo nextest run --workspace` runs the tests; they write their own fixtures and never touch a real KiCad config.
With a checkout of the boards beside kicad-library (`../hw`, or `VHRD_BOARDS_DIR`), `tests/boards.rs` also runs
`migrate` with the real map on a temporary copy of them, and checks the result against a restructured copy of the
library; without the boards those tests skip with a note.
