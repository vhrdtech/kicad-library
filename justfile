# kicad-library is used in place (KiCad's paths point at this checkout): look refreshes what vhrd_brand's vhrd-kicad
# makes, test checks the templates print their sheets and runs kilib's tests, check-boards checks that the boards
# resolve their libraries, deploy brings this PC's checkout up to main.

# The boards check-boards reads: the hw checkout beside this one, or VHRD_BOARDS_DIR
boards := env_var_or_default("VHRD_BOARDS_DIR", "../hw")

# Bare `just` only lists the recipes
default:
    @just --list

# vhrd_power's colours and the kept templates' sheets, net class colours and power symbols, from the installed vhrd-kicad (vhrd_brand `just install`)
look:
    vhrd-kicad power-colours symbols/VHRD_power.kicad_symdir
    vhrd-kicad adopt --power "templates/Vhrd Default/default_vhrd.kicad_pro"
    vhrd-kicad adopt --power templates/IO30/io30.kicad_pro
    vhrd-kicad adopt --power templates/Hammond-1455C801/hammond_1455c801.kicad_pro

# The kept templates print their embedded VHRD sheet, and kilib's tests (`--changed`: only the kilib crates changed against main)
test *flags:
    if [ "{{flags}}" = "--changed" ] && command -v tpm >/dev/null; then cd tools/kilib && tpm test changed -- cargo nextest run --workspace; else {{just_executable()}} test-all; fi

# Every test: each kept template's schematic and board print their embedded VHRD sheet, kilib
test-all:
    {{just_executable()}} test-sheet "templates/Vhrd Default/default_vhrd"
    {{just_executable()}} test-sheet templates/IO30/io30
    {{just_executable()}} test-sheet templates/Hammond-1455C801/hammond_1455c801
    cd tools/kilib && cargo nextest run --workspace

# One template's schematic and board export with the VHRD sheet they embed (it prints vhrd.tech)
[private]
test-sheet base:
    kicad-cli sch export pdf -o /tmp/kicad-library-test.pdf "{{base}}.kicad_sch"
    pdftotext /tmp/kicad-library-test.pdf - | grep -q 'vhrd\.tech' || { echo "{{base}}.kicad_sch: no VHRD sheet"; exit 1; }
    kicad-cli pcb export pdf --layers Edge.Cuts --include-border-title -o /tmp/kicad-library-test.pdf "{{base}}.kicad_pcb"
    pdftotext /tmp/kicad-library-test.pdf - | grep -q 'vhrd\.tech' || { echo "{{base}}.kicad_pcb: no VHRD sheet"; exit 1; }

# Every symbol, footprint and 3D model of the boards resolves on this PC (kilib check, ARGS passed on); skips without boards
check-boards *args:
    @if [ -d "{{boards}}" ]; then cargo run --quiet --release --manifest-path tools/kilib/Cargo.toml -- check {{args}} "{{boards}}"; else echo "check-boards: skipped, no boards at {{boards}} (set VHRD_BOARDS_DIR to a checkout of the boards)"; fi

# No whitespace errors in what is staged or changed; kilib formatted and without clippy warnings
lint:
    git diff --check HEAD
    cd tools/kilib && cargo fmt --all --check
    cd tools/kilib && cargo clippy --workspace --all-targets --quiet -- -D warnings

# Build and install kilib; the library itself is used in place (README "Configuration")
install:
    cd tools/kilib && cargo build --profile deploy --locked -p kilib_cli
    if command -v tpm >/dev/null; then tpm install tools/kilib/target/deploy/kilib; else cargo install --locked --path tools/kilib/crates/kilib_cli; fi
    @echo "kicad-library is used in place: KiCad's library tables and VHRD_TEMPLATE_DIR point at $PWD (README)"

# Bring this PC's checkout up to main
deploy:
    tpm repos pull kicad-library
