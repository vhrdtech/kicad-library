# kicad-library is used in place (KiCad's paths point at this checkout): look refreshes what vhrd_brand's vhrd-kicad
# makes, test checks the templates and sheets load and runs kilib's tests, check-boards checks that the boards
# resolve their libraries, deploy brings this PC's checkout up to main.

# The boards check-boards reads: the hw checkout beside this one, or VHRD_BOARDS_DIR
boards := env_var_or_default("VHRD_BOARDS_DIR", "../hw")

# Bare `just` only lists the recipes
default:
    @just --list

# Sheets, vhrd_power colours and the Vhrd Default template from the installed vhrd-kicad (vhrd_brand `just install`)
look:
    vhrd-kicad gen /tmp/kicad-library-look
    cp /tmp/kicad-library-look/vhrd_a4.kicad_wks /tmp/kicad-library-look/vhrd_a3.kicad_wks templates/
    vhrd-kicad power-colours symbols/vhrd_power.kicad_sym
    vhrd-kicad adopt --power "templates/Vhrd Default/default_vhrd.kicad_pro"

# The templates and sheets load in kicad-cli, and kilib's tests (`--changed`: only the kilib crates changed against main)
test *flags:
    if [ "{{flags}}" = "--changed" ] && command -v tpm >/dev/null; then cd tools/kilib && tpm test changed -- cargo nextest run --workspace; else {{just_executable()}} test-all; fi

# Every test: the Vhrd Default template exports with its embedded sheet, the sheets in templates/ load, kilib
test-all:
    kicad-cli sch export pdf -o /tmp/kicad-library-test.pdf "templates/Vhrd Default/default_vhrd.kicad_sch"
    pdftotext /tmp/kicad-library-test.pdf - | grep -q SCHEMATIC
    kicad-cli sch export pdf --drawing-sheet "$PWD/templates/vhrd_a4.kicad_wks" -o /tmp/kicad-library-test.pdf "templates/Vhrd Default/default_vhrd.kicad_sch"
    pdftotext /tmp/kicad-library-test.pdf - | grep -q SCHEMATIC
    cd tools/kilib && cargo nextest run --workspace

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
