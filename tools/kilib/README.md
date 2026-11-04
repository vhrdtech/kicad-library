# kilib

Checks that KiCad boards resolve their libraries on a PC, and sets up a PC's KiCad config for a library checkout.
Part of [kicad-library](../../README.md#checking-boards-kilib); MIT licensed.

```sh
kilib check ~/git/hw              # every symbol, footprint and 3D model of the boards resolves
kilib check --json board.kicad_pcb
kilib setup --dry-run             # what setup would change in this PC's KiCad config
```

- `crates/kilib`: the library (s-expression reader, library tables, path variables, `check`, `setup`).
- `crates/kilib_cli`: the `kilib` binary.

`cargo nextest run --workspace` runs the tests; they write their own fixtures and never touch a real KiCad config.
