# kicad-library is used in place (KiCad's paths point at this checkout): look refreshes what vhrd_brand's vhrd-kicad
# makes, test checks the templates and sheets load, deploy brings this PC's checkout up to main.

# Bare `just` only lists the recipes
default:
    @just --list

# Sheets, vhrd_power colours and the Vhrd Default template from the installed vhrd-kicad (vhrd_brand `just install`)
look:
    vhrd-kicad gen /tmp/kicad-library-look
    cp /tmp/kicad-library-look/vhrd_a4.kicad_wks /tmp/kicad-library-look/vhrd_a3.kicad_wks templates/
    vhrd-kicad power-colours symbols/vhrd_power.kicad_sym
    vhrd-kicad adopt --power "templates/Vhrd Default/default_vhrd.kicad_pro"

# The Vhrd Default template exports with its embedded sheet, and the sheets in templates/ load
test:
    kicad-cli sch export pdf -o /tmp/kicad-library-test.pdf "templates/Vhrd Default/default_vhrd.kicad_sch"
    pdftotext /tmp/kicad-library-test.pdf - | grep -q SCHEMATIC
    kicad-cli sch export pdf --drawing-sheet "$PWD/templates/vhrd_a4.kicad_wks" -o /tmp/kicad-library-test.pdf "templates/Vhrd Default/default_vhrd.kicad_sch"
    pdftotext /tmp/kicad-library-test.pdf - | grep -q SCHEMATIC

# No whitespace errors in what is staged or changed
lint:
    git diff --check HEAD

# Nothing to build: KiCad uses this checkout in place (README "Configuration")
install:
    @echo "kicad-library is used in place: KiCad's library tables and VHRD_TEMPLATE_DIR point at $PWD (README)"

# Bring this PC's checkout up to main
deploy:
    tpm repos pull kicad-library
