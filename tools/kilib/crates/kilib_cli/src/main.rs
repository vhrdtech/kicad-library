//! `kilib`: checks that KiCad boards resolve their libraries on this PC, and sets up this PC's KiCad config for
//! kicad-library.

mod chip_cmd;
mod lint_cmd;
mod usage;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anstream::{eprintln, println};
use anstyle::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use kilib::check::{self, Finding, Report};
use kilib::config;
use kilib::fab;
use kilib::layers;
use kilib::layout_fields;
use kilib::libtable::{LibTable, TableKind};
use kilib::migrate;
use kilib::per_package;
use kilib::presets;
use kilib::restructure;
use kilib::setup::{self, Line};
use kilib::stock;

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_SHA"), ", commit ", env!("BUILD_TIME"), ")");

const RED: Style = AnsiColor::Red.on_default().bold();
const GREEN: Style = AnsiColor::Green.on_default().bold();
const YELLOW: Style = AnsiColor::Yellow.on_default();
const DIM: Style = Style::new().dimmed();
const BOLD: Style = Style::new().bold();
const CYAN: Style = AnsiColor::Cyan.on_default();

#[derive(Parser)]
#[command(name = "kilib", version = VERSION, about, long_about = None, after_help = AFTER_HELP)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Colour in the output: auto (a terminal without NO_COLOR), always or never
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    color: ColorWhen,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

const AFTER_HELP: &str = "\
Examples:
  kilib check ~/git/hw                  every board in hw resolves on this PC
  kilib check --json board.kicad_pcb    the same for one file, as JSON
  kilib setup --dry-run                 what setup would change in this PC's KiCad config
  kilib migrate --map migrate/2026-11.toml ~/git/hw    the diff that moves the boards to the new layout
  kilib per-package --map migrate/2026-11-packages.toml ~/git/hw/b153_typec_serial    placed generic symbols by footprint
  kilib designators                     which footprints' designators and values are not yet on their layers
  kilib layers --write board.kicad_pcb  name the designator and room layers in an existing board
  kilib stock diff                      our copies of KiCad's stock footprints against the current stock library
  kilib lint --left                     what the footprint rules leave for a person (pin 1, courtyards, models...)
  kilib model chip --write              STEP models of the chip passives in generate/*.toml
  kilib footprint chip --write          their footprints, one per size and height
Shell completion: source <(COMPLETE=bash kilib)   (zsh, fish: COMPLETE=zsh, COMPLETE=fish)";

#[derive(Subcommand)]
enum Command {
    /// Check that every symbol, footprint, Footprint field and 3D model of the boards resolves
    ///
    /// Reads the global sym-lib-table and fp-lib-table and kicad_common.json's path variables from the KiCad
    /// config folder (KICAD_CONFIG_HOME, else ~/.config/kicad, newest version), each project's own tables, and
    /// the environment, which wins over kicad_common.json as in KiCad. Exits 1 when anything does not resolve.
    #[command(after_help = "\
Examples:
  kilib check ~/git/hw
  kilib check --config-dir /tmp/kicad-cfg/10.0 --var KICAD10_3DMODEL_DIR=/mnt/kicad-packages3D ~/git/hw
  kilib check --json ~/git/hw > report.json
  kilib check --only VHRD_ ~/git/hw/b153_typec_serial    no symbol or footprint from KiCad's stock libraries")]
    Check {
        /// Boards: .kicad_sch, .kicad_pcb, .kicad_sym or .kicad_mod files, or folders searched for them
        #[arg(required = true, value_hint = ValueHint::AnyPath)]
        paths: Vec<PathBuf>,
        #[command(flatten)]
        config: ConfigArgs,
        /// The global symbol table to use instead of the config folder's
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        sym_lib_table: Option<PathBuf>,
        /// The global footprint table to use instead of the config folder's
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        fp_lib_table: Option<PathBuf>,
        /// A path variable, over every other source (repeatable)
        #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_var)]
        vars: Vec<(String, String)>,
        /// Every library named must start with PREFIX: symbols, footprints, Footprint fields, a schematic's
        /// embedded symbols and the library part of ki_fp_filters (`--only VHRD_`: nothing from KiCad's stock)
        #[arg(long, value_name = "PREFIX")]
        only: Option<String>,
        /// Print the report as JSON
        #[arg(long)]
        json: bool,
    },
    /// Point this PC's KiCad at a kicad-library checkout: library rows and path variables
    ///
    /// Replaces every vhrd_* and VHRD_* row of the global sym-lib-table and fp-lib-table by the rows of a rename
    /// map's [libraries.symbol] and [libraries.footprint] (--map), or else of the library folders found, through
    /// ${VHRD_LIBRARY_DIR} and ${VHRD_VENDOR_LIBRARY_DIR}. Sets VHRD_LIBRARY_DIR, VHRD_VENDOR_LIBRARY_DIR and
    /// VHRD_3DMODEL_DIR (each when given), VHRD_TEMPLATE_DIR and KICAD_USER_TEMPLATE_DIR in kicad_common.json.
    /// Every other row and setting stays as it is. Refuses while KiCad runs, since KiCad writes these files when
    /// it closes.
    #[command(after_help = "\
Examples:
  kilib setup --dry-run
  kilib setup --map migrate/2026-11.toml --vendor-dir ~/vhrd-library-vendor \\
              --models-dir ~/vhrd-packages3D
  kilib setup --config-dir /tmp/kicad-test/10.0 --dry-run")]
    Setup {
        /// The kicad-library checkout (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        #[command(flatten)]
        config: ConfigArgs,
        /// A rename map (migrate/*.toml): its [libraries.symbol] and [libraries.footprint] give the rows
        /// (default: the library folders found in the checkout and the vendor library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: Option<PathBuf>,
        /// The private vendor library, for VHRD_VENDOR_LIBRARY_DIR (left as it is when not given)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, env = "VHRD_VENDOR_LIBRARY_DIR")]
        vendor_dir: Option<PathBuf>,
        /// The 3D models, for VHRD_3DMODEL_DIR (left as it is when not given)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, env = "VHRD_3DMODEL_DIR")]
        models_dir: Option<PathBuf>,
        /// Where a new table is copied from when the config folder has none (KiCad's stock tables)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath, default_value = "/usr/share/kicad/template")]
        stock_tables: PathBuf,
        /// Show what would change and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Rewrite boards to a new library layout: only the strings the map matches change
    ///
    /// Rewrites symbol ids (lib_id, lib_name, the lib_symbols cache), Footprint fields, ki_fp_filters entries,
    /// footprint ids, ${VHRD_3DMODEL_DIR} model paths and the old vhrd_* rows of project sym-lib-table and
    /// fp-lib-table; every other byte stays as it was. Prints a unified diff; --write applies it. Exits 1 on an
    /// error (a dropped footprint still in use, two cached symbols that would get one name), and then writes
    /// nothing. A second run changes nothing.
    #[command(after_help = "\
Examples:
  kilib migrate --map migrate/2026-11.toml ~/git/hw > hw.diff
  kilib migrate --map migrate/2026-11.toml --write ~/git/hw/b129_cannify_micro")]
    Migrate {
        /// The rename map (migrate/*.toml in kicad-library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: PathBuf,
        /// Boards: files or folders searched for .kicad_sch, .kicad_pcb, .kicad_sym, .kicad_mod and the tables
        #[arg(required = true, value_hint = ValueHint::AnyPath)]
        paths: Vec<PathBuf>,
        /// Write the changes instead of printing the diff
        #[arg(long)]
        write: bool,
        /// Print only the summary, no diff
        #[arg(long, conflicts_with = "write")]
        stat: bool,
    },
    /// Move a kicad-library checkout's symbols and footprints into the map's new libraries
    ///
    /// symbols/<old>.kicad_sym becomes one .kicad_sym per symbol in <New>.kicad_symdir (KiCad 10's layout; a
    /// derived symbol goes with its parent), footprints/<old>.pretty/*.kicad_mod move to <New>.pretty. Libraries
    /// under ${VHRD_VENDOR_LIBRARY_DIR} go to --vendor-dir. Symbol and footprint text is kept as it is, except
    /// what migrate rewrites (model paths, Footprint fields). The old files are removed once the new ones are
    /// written.
    #[command(after_help = "\
Examples:
  kilib restructure --map migrate/2026-11.toml --vendor-dir ~/vhrd-library-vendor --dry-run
  kilib restructure --map migrate/2026-11.toml --vendor-dir ~/vhrd-library-vendor")]
    Restructure {
        /// The rename map (migrate/*.toml in kicad-library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: PathBuf,
        /// Where the libraries under ${VHRD_VENDOR_LIBRARY_DIR} go
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        vendor_dir: PathBuf,
        /// The kicad-library checkout (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        /// List every move and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Make the per-package symbols of a rename map, and list the generic symbols boards place by footprint
    ///
    /// A generic symbol (VHRD_Device:R, its Footprint empty) gets a symbol per package, named in the map's
    /// [per_package."Lib:Generic"] table ("FpLib:Footprint" = "Lib:Generic_Package"). Each is derived from the
    /// generic (extends) in its library, with Footprint and ki_fp_filters set to the footprint. Those missing in
    /// the checkout are listed (exit 1) or, with --write, written; those there are checked. With boards, the
    /// generic symbols their schematics place are listed by footprint, with the map's per-package symbol or
    /// "not in the map". `kilib migrate` with the same map then moves the placed symbols onto them.
    #[command(after_help = "\
Examples:
  kilib per-package --map migrate/2026-11-packages.toml ../hw/b153_typec_serial/source/b153a
  kilib per-package --map migrate/2026-11-packages.toml --write
  kilib migrate --map migrate/2026-11-packages.toml --write ../hw/b153_typec_serial/source/b153a")]
    PerPackage {
        /// The rename map with the [per_package] tables (migrate/*.toml in kicad-library)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        map: PathBuf,
        /// Boards whose placed generic symbols to list: schematics or folders searched for them
        #[arg(value_hint = ValueHint::AnyPath)]
        boards: Vec<PathBuf>,
        /// The kicad-library checkout (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        /// Write the missing per-package symbols
        #[arg(long)]
        write: bool,
    },
    /// Copy KiCad's stock footprints into the library, and compare the copies with the current stock library
    ///
    /// The copies are on purpose: a KiCad update never changes our footprints unannounced. `import` copies
    /// Lib:Name from KiCad's footprint folder to footprints/VHRD_Lib.pretty/Name.kicad_mod with the designator on
    /// Ref.Des.F, the Value on Value.F and the fab lines at 0.05 mm (as `designators` puts them); pads, other
    /// graphics and the 3D model stay as in stock. `diff` shows, for every copy with a stock footprint of the same
    /// name, what a fresh import would change, so upstream changes are taken in deliberately.
    #[command(subcommand)]
    Stock(StockCommand),
    /// Name the user layers kicad-library uses in boards' layer tables
    ///
    /// Real parts carry their designator on User.23/24 Ref.Des.F/B; test points, net ties and mechanical
    /// footprints on a layer of their own (User.9/10 Ref.Test.F/B, User.11/12 Ref.Tie.F/B, User.13/14
    /// Ref.Mech.F/B); F.Fab and B.Fab keep the drawing alone. User.15/16 Rooms.F/B hold hand-drawn outlines of the
    /// circuit blocks. User.17/18 Value.F/B carry a real part's Value, centred like the designator, User.19/20
    /// Pins.F/B pin numbers next to the pads, User.21/22 Centroid.F/B a cross on the placement origin, User.25/26
    /// Crit.F/B and User.27/28 Imp.F/B a real part's Layout_Critical and Layout_Important notes, User.5/6 Body.F/B
    /// a part's full top view (its 3D model's outline), User.7/8 Xray.F/B every edge of that model seen from
    /// above, hidden ones included. Each pair is typed front and back, so KiCad flips a
    /// bottom-side footprint's drawing onto the back layer. KiCad's own Dwgs.User and Cmts.User are named
    /// Dimensions and Fab.Notes. This adds the layers a board lacks, and names and types ones it has unnamed; a
    /// number the board already uses under another name is left alone and reported. Bodies from before the pair
    /// (footprint drawings on User.2, then named Body.F) move to Body.F or Body.B by the footprint's side, and User.2
    /// loses that name. A board in KiCad 8's layer numbering is refused (save it in KiCad 10 first). Only the
    /// (layers ...) table and those layers of footprint drawings change. Without --write it lists what would change
    /// and exits 1 when anything would.
    #[command(after_help = "\
Examples:
  kilib layers ../hw/b153_typec_serial/source/b153a/b153a_typec_serial.kicad_pcb
  kilib layers --write templates/IO30/io30.kicad_pcb")]
    Layers {
        /// Boards (.kicad_pcb)
        #[arg(required = true, value_hint = ValueHint::FilePath)]
        boards: Vec<PathBuf>,
        /// Write the changes
        #[arg(long)]
        write: bool,
    },
    /// Write the PCB Editor's layer presets of generate/layer-presets.toml into a project's .kicad_pro
    ///
    /// Each preset becomes a row of board.layer_presets: the layers it shows, by kilib's names (Crit.F, Pins.B...)
    /// or KiCad's own (F.Cu, Edge.Cuts, F.Courtyard...), its render items (pads, vias, ratsnest...) and whether the
    /// board is flipped. A preset already in the project under the same name is replaced; the project's other
    /// presets are kept, in their place. A project's sibling board (same name, .kicad_pcb) is checked for the user
    /// layers the presets name; a board that lacks one is told to run `kilib layers --write` first. Only
    /// board.layer_presets changes; every other byte of the project stays. Without --write it lists what would
    /// change and exits 1 when anything would.
    #[command(after_help = "\
Examples:
  kilib presets templates/*/*.kicad_pro
  kilib presets --write templates/IO30/io30.kicad_pro
  kilib presets --table generate/layer-presets.toml --write board.kicad_pro")]
    Presets {
        /// Projects (.kicad_pro)
        #[arg(required = true, value_hint = ValueHint::FilePath)]
        projects: Vec<PathBuf>,
        /// The presets table (default: generate/layer-presets.toml of the kicad-library checkout)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        table: Option<PathBuf>,
        /// The kicad-library checkout with generate/ (default: the one around the current folder)
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        library: Option<PathBuf>,
        /// Write the changes
        #[arg(long)]
        write: bool,
    },
    /// Put each footprint's designator on its designator layer and a real part's Value on Value, both centred
    ///
    /// The Reference property goes to Ref.Des.F (User.23; Ref.Des.B, User.24, on a bottom-side footprint; Ref.Test,
    /// Ref.Tie or Ref.Mech for test points, net ties and mechanical footprints), visible and unlocked; ${REFERENCE}
    /// texts on the silkscreen, the fab layer and the designator's layer go, so F.Fab and B.Fab keep the drawing
    /// alone. The text's ink, measured in KiCad's stroke font with the strokes counted, is centred on the fab
    /// outline (else the courtyard). Pads do not matter (KiCad draws the selected layers on top); the drawing on the
    /// fab layer of the same side does: 0.02 mm stays free between the strokes. A library footprint is sized for the
    /// longest designator of its kind (R1004, C1001, TP526, U1004). It gets the largest size from 1 mm down to
    /// 0.2 mm that fits on the centre, level or turned to read top to bottom; else it moves to the largest free spot inside the body
    /// nearest the centre; else it may go down to 0.15 mm (0201, 01005: listed as "tiny"); else it stays on the
    /// centre at 0.2 mm and is listed as "no clean fit". Strokes are 15 % of the size, at least 0.025 mm. Every line
    /// on F.Fab and B.Fab gets a 0.05 mm stroke. A real part's Value goes to Value.F (User.17; Value.B, User.18, on
    /// the back), shown and unlocked, fitted the same way for the longest typical value of its kind (100K, 100nF,
    /// 4.7uH, BAS16TW, TPS54202); the designator is no obstacle, as Ref.Des and Value are shown one at a time. The
    /// Value of a test point, net tie or mechanical footprint stays hidden on the fab layer, where it was. A
    /// footprint that already follows these rules is left as it is; every other byte stays. With --board, each
    /// footprint on that board whose Reference is already on its designator layer gets its own designator fitted
    /// the same way (a short one gets larger text), and a real part whose Value is on Value its own value; only the
    /// properties' position, angle, size and thickness change. A Layout_Critical or Layout_Important note that is
    /// set (from the schematic) is fitted to its own text the same way, on Crit or Imp, but down to 0.05 mm (a
    /// short note reads fine much smaller than a designator, and fitting it on the body matters more) before it
    /// is listed as "no clean fit"; on a real part a set note that sits elsewhere or hidden (where Update PCB from
    /// Schematic puts a field the footprint lacked) is moved there, shown. Each pin number on Pins.F/B of a part
    /// placed at a quarter turn is turned by a half turn to read left to right or top to bottom, its ink where it
    /// was; ones at another angle and on the back are listed. Without --write it lists
    /// what would change and exits 1 when anything would. `fab-designators` is the old name of this command.
    #[command(
        visible_alias = "fab-designators",
        after_help = "\
Examples:
  kilib designators
  kilib designators --write footprints/VHRD_Package_SO.pretty
  kilib designators --board ../hw/b153_typec_serial/source/b153a/b153a_typec_serial.kicad_pcb --write"
    )]
    Designators {
        /// Footprints: .kicad_mod files or folders searched for them (default: the checkout's footprints/)
        #[arg(value_hint = ValueHint::AnyPath, conflicts_with = "board")]
        paths: Vec<PathBuf>,
        /// Fit the designators and values of the footprints on this board instead, each to its own text
        #[arg(long, value_name = "PCB", value_hint = ValueHint::FilePath)]
        board: Option<PathBuf>,
        /// Write the changes
        #[arg(long)]
        write: bool,
        /// List the footprints that are already fine too
        #[arg(long, short)]
        verbose: bool,
    },
    /// Add the empty Layout_Critical and Layout_Important fields to library symbols and placed symbols
    ///
    /// Layout priority: a schematic sets a short note (U101.5 VDD, FB U301) in a symbol's Layout_Critical or
    /// Layout_Important field; Update PCB from Schematic carries it to the footprint's property of the same name,
    /// which shows on Crit.F/B (User.25/26) or Imp.F/B (User.27/28) (`kilib lint --rule layout` puts them on every
    /// real part's footprint). Every symbol carries both fields, empty and hidden. This adds them where a symbol
    /// lacks them: to each symbol of a library file (.kicad_sym), and in a schematic (.kicad_sch) to each placed
    /// symbol and to its copy in the schematic's lib_symbols (KiCad's "Update Symbols from Library" does the same).
    /// A new field copies a hidden field of the symbol (Description, else Datasheet, else Footprint), empty, so it
    /// is written the way the file writes its fields. Power symbols and power flags get none. Only the new fields
    /// are written. Without --write it lists what would change and exits 1 when anything would.
    #[command(after_help = "\
Examples:
  kilib layout-fields
  kilib layout-fields --write symbols/VHRD_Device.kicad_symdir
  kilib layout-fields ../hw/b153_typec_serial/source/b153a")]
    LayoutFields {
        /// Symbol files (.kicad_sym), schematics (.kicad_sch) or folders searched for them (default: the checkout's
        /// symbols/)
        #[arg(value_hint = ValueHint::AnyPath)]
        paths: Vec<PathBuf>,
        /// Write the changes
        #[arg(long)]
        write: bool,
    },
    /// Check footprints against the library rules: pin-1 marks, attributes, courtyards, 3D models, silkscreen
    ///
    /// pin1: parts with three or more pins and polar two-pin parts mark pin 1 on the fab layer, and on the
    /// silkscreen where they draw one, so that no turn of the part that fits its pads leaves pin 1 in doubt. Fix: a
    /// chamfered pin-1 corner on a rectangular fab outline, a filled triangle pointing at pin 1 on the silkscreen.
    ///
    /// pin1-body: the same parts show pin 1 as a filled dot on Pin1.F (User.29), 0.3 to 0.8 mm across, by pin 1 inside the
    /// body outline (Body.F, User.5) where it fits, else just outside it, nearer pin 1 than any pad a turn of the part
    /// takes it to. Fix: drawn, replacing anything else on that layer; a part with no such place is left for a person.
    ///
    /// attrs: test points, net ties, spark gaps, jumpers, logos and mounting holes are kept out of the BOM and the
    /// position files, fiducials out of the BOM only; a part is SMD or through-hole as its pads are. Fix: the
    /// attributes set.
    ///
    /// courtyard: present, closed, 0.25 mm from the pads and the body (0.1 mm for 0201 to 0603 passives). Fix: drawn
    /// where missing, grown where it is one rectangle.
    ///
    /// models: every part has one under ${VHRD_3DMODEL_DIR} or ${KICAD10_3DMODEL_DIR}, and the file exists where
    /// the variables point. Fix: older KiCad model variables moved to KICAD10.
    ///
    /// silk: 0.12 mm lines, none onto a pad's mask opening, no text but pin-1 marks. Fix: widths set, lines cut.
    ///
    /// ref-outline: test points draw their pad's outline on Ref.Test, net ties, spark gaps and jumpers a box round
    /// their pads on Ref.Tie, 0.05 mm out and 0.05 mm wide. Fix: drawn, replacing anything else on that layer.
    ///
    /// pins: pin numbers beside the pads on Pins.F (User.19): every pin of a connector; pin 1 and every 5th of
    /// another part with three or more pins (every 10th past 40); none on two-pin parts. Level stroke text, 0.1 mm
    /// clear of every pad and other number, one size per footprint (0.5 mm down to 0.2 mm). Fix: written,
    /// replacing anything else on that layer; a number with no room is left for a person.
    ///
    /// centroid: a 0.5 mm cross of 0.05 mm lines on the origin on Centroid.F (User.21) for every part and
    /// fiducial, the point the position file gives. Fix: drawn, replacing anything else on that layer.
    ///
    /// layout: every real part has the properties Layout_Critical on Crit.F (User.25) and Layout_Important on Imp.F
    /// (User.27), visible, unlocked and empty, centred on the body for the note U1001.48, down to 0.05 mm where the
    /// body needs it. The schematic sets a note in the symbol field of the same name; Update PCB from Schematic carries it to the
    /// footprint, where it shows on its own layer. Test points, net ties, mechanical and graphic footprints carry
    /// neither. Fix: added or put back in place; a stray one removed; a note set in the library left for a person.
    ///
    /// The designator and the Value are left to `kilib designators`; run it after --write. It lists what each fix changes and
    /// what is left for a person, and exits 1 when a fix is not written yet.
    #[command(after_help = "\
Examples:
  kilib lint
  kilib lint --left --rule pin1,courtyard
  kilib lint --write footprints/VHRD_Package_SO.pretty
  kilib lint --rule models --var VHRD_3DMODEL_DIR=~/sync/kicad-3d --json > models.json")]
    Lint(lint_cmd::LintArgs),
    /// Generate 3D models from data (chip passives, power inductors)
    #[command(subcommand)]
    Model(chip_cmd::ModelCommand),
    /// Generate footprints from data (chip passives, power inductors)
    #[command(subcommand)]
    Footprint(chip_cmd::FootprintCommand),
}

#[derive(Subcommand)]
enum StockCommand {
    /// Copy stock footprints (Lib:Name) into footprints/VHRD_Lib.pretty, designator on Ref.Des, Value on Value
    ///
    /// With --symbols, copy stock symbols into symbols/VHRD_Lib.kicad_symdir instead: made standalone (a derived
    /// symbol gets its parent's pins and graphics), the Footprint field at our copy of the footprint where the
    /// library has one, library parts of ki_fp_filters prefixed VHRD_, an empty Mpn1 field.
    ///
    /// Items the library has already are left alone (`kilib stock diff` compares them). Without --write it lists
    /// what would be written and exits 1 when anything would.
    #[command(after_help = "\
Examples:
  kilib stock import Resistor_SMD:R_0402_1005Metric Package_SO:VSSOP-8_2.3x2mm_P0.5mm
  kilib stock import --write --stock-dir /usr/share/kicad/footprints Resistor_SMD:R_0402_1005Metric
  kilib stock import --symbols --write Device:SparkGap Transistor_BJT:BC847BS")]
    Import {
        /// Stock footprints (or with --symbols, symbols), Lib:Name
        #[arg(required = true, value_name = "LIB:NAME")]
        ids: Vec<String>,
        /// The ids are stock symbols
        #[arg(long)]
        symbols: bool,
        #[command(flatten)]
        stock: StockArgs,
        /// Write the copies
        #[arg(long)]
        write: bool,
    },
    /// Compare each VHRD_Lib:Name with a fresh import of stock Lib:Name; exits 1 when any differs
    ///
    /// Prints a unified diff from our copy to the fresh import (what taking in the stock item would change).
    /// Footprints and symbols of our own that happen to share a stock name are compared too. A symbol's fields
    /// and flags are ours: they are laid over the fresh import, so the diff shows pins, units, graphics and fields
    /// only stock has. A kind whose stock folder is not found is skipped with a note.
    #[command(after_help = "\
Examples:
  kilib stock diff
  kilib stock diff --stat")]
    Diff {
        #[command(flatten)]
        stock: StockArgs,
        /// Only the list, no diff
        #[arg(long)]
        stat: bool,
    },
}

#[derive(clap::Args)]
struct StockArgs {
    /// KiCad's stock footprint folder (default: KICAD10_FOOTPRINT_DIR from the environment or KiCad's config, else
    /// /usr/share/kicad/footprints)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    stock_dir: Option<PathBuf>,
    /// KiCad's stock symbol folder (default: KICAD10_SYMBOL_DIR from the environment or KiCad's config, else
    /// /usr/share/kicad/symbols)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    stock_symbol_dir: Option<PathBuf>,
    /// The kicad-library checkout (default: the one around the current folder)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    library: Option<PathBuf>,
    #[command(flatten)]
    config: ConfigArgs,
}

impl StockArgs {
    fn stock_dir(&self) -> Result<PathBuf, String> {
        self.find(&self.stock_dir, "KICAD10_FOOTPRINT_DIR", "footprints", "--stock-dir")
    }

    fn stock_symbol_dir(&self) -> Result<PathBuf, String> {
        self.find(&self.stock_symbol_dir, "KICAD10_SYMBOL_DIR", "symbols", "--stock-symbol-dir")
    }

    fn find(&self, flag: &Option<PathBuf>, var: &str, what: &str, option: &str) -> Result<PathBuf, String> {
        if let Some(d) = flag {
            return Ok(d.clone());
        }
        let share = Path::new("/usr/share/kicad");
        let vars = config::vars(self.config.dir().as_deref(), std::env::vars(), share.is_dir().then_some(share))?;
        vars.get(var)
            .map(PathBuf::from)
            .filter(|d| d.is_dir())
            .ok_or_else(|| format!("no KiCad stock {what} found: set {var} or pass {option} DIR"))
    }
}

#[derive(clap::Args)]
struct ConfigArgs {
    /// The versioned KiCad config folder, like ~/.config/kicad/10.0 (default: from KICAD_CONFIG_HOME or the
    /// platform's config folder)
    #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
    config_dir: Option<PathBuf>,
}

impl ConfigArgs {
    fn dir(&self) -> Option<PathBuf> {
        self.config_dir.clone().or_else(|| config::config_dir(&|k| std::env::var(k).ok()))
    }
}

fn parse_var(s: &str) -> Result<(String, String), String> {
    let (k, v) = s.split_once('=').ok_or("expected NAME=VALUE")?;
    if k.is_empty() {
        return Err("expected NAME=VALUE".into());
    }
    Ok((k.to_string(), v.to_string()))
}

fn main() -> ExitCode {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    match cli.color {
        ColorWhen::Auto => {}
        ColorWhen::Always => anstream::ColorChoice::Always.write_global(),
        ColorWhen::Never => anstream::ColorChoice::Never.write_global(),
    }
    let result = match cli.command {
        Command::Check { paths, config, sym_lib_table, fp_lib_table, vars, only, json } => {
            usage::record(
                "check",
                &[
                    ("json", json),
                    ("var", !vars.is_empty()),
                    ("config-dir", config.config_dir.is_some()),
                    ("only", only.is_some()),
                ],
            );
            run_check(&paths, &config, sym_lib_table, fp_lib_table, vars, only, json)
        }
        Command::Setup { library, config, map, vendor_dir, models_dir, stock_tables, dry_run } => {
            usage::record(
                "setup",
                &[
                    ("dry-run", dry_run),
                    ("config-dir", config.config_dir.is_some()),
                    ("map", map.is_some()),
                    ("vendor-dir", vendor_dir.is_some()),
                    ("models-dir", models_dir.is_some()),
                ],
            );
            run_setup(library, &config, map, vendor_dir, models_dir, &stock_tables, dry_run)
        }
        Command::Migrate { map, paths, write, stat } => {
            usage::record("migrate", &[("write", write), ("stat", stat)]);
            run_migrate(&map, &paths, write, stat)
        }
        Command::Restructure { map, vendor_dir, library, dry_run } => {
            usage::record("restructure", &[("dry-run", dry_run), ("library", library.is_some())]);
            run_restructure(&map, &vendor_dir, library, dry_run)
        }
        Command::PerPackage { map, boards, library, write } => {
            usage::record(
                "per-package",
                &[("write", write), ("boards", !boards.is_empty()), ("library", library.is_some())],
            );
            run_per_package(&map, &boards, library, write)
        }
        Command::Stock(StockCommand::Import { ids, symbols, stock, write }) => {
            usage::record(
                "stock import",
                &[
                    ("write", write),
                    ("symbols", symbols),
                    ("stock-dir", stock.stock_dir.is_some()),
                    ("stock-symbol-dir", stock.stock_symbol_dir.is_some()),
                ],
            );
            run_stock_import(&ids, symbols, &stock, write)
        }
        Command::Stock(StockCommand::Diff { stock, stat }) => {
            usage::record(
                "stock diff",
                &[
                    ("stat", stat),
                    ("stock-dir", stock.stock_dir.is_some()),
                    ("stock-symbol-dir", stock.stock_symbol_dir.is_some()),
                ],
            );
            run_stock_diff(&stock, stat)
        }
        Command::Lint(args) => {
            usage::record("lint", &args.usage_flags());
            lint_cmd::run(args)
        }
        Command::Model(cmd) => {
            let (name, flags) = cmd.usage();
            usage::record(name, &flags);
            chip_cmd::run(cmd)
        }
        Command::Footprint(cmd) => {
            let (name, flags) = cmd.usage();
            usage::record(name, &flags);
            chip_cmd::run_footprints(cmd)
        }
        Command::LayoutFields { paths, write } => {
            usage::record("layout-fields", &[("write", write), ("paths", !paths.is_empty())]);
            run_layout_fields(paths, write)
        }
        Command::Layers { boards, write } => {
            usage::record("layers", &[("write", write)]);
            run_layers(&boards, write)
        }
        Command::Presets { projects, table, library, write } => {
            usage::record("presets", &[("write", write), ("table", table.is_some()), ("library", library.is_some())]);
            run_presets(&projects, table, library, write)
        }
        Command::Designators { paths, board, write, verbose } => {
            usage::record("designators", &[("write", write), ("paths", !paths.is_empty()), ("board", board.is_some())]);
            match board {
                Some(board) => run_fab_board(&board, write, verbose),
                None => run_fab(paths, write, verbose),
            }
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{RED}error:{RED:#} {e}");
            ExitCode::from(2)
        }
    }
}

fn run_check(
    paths: &[PathBuf],
    config: &ConfigArgs,
    sym_lib_table: Option<PathBuf>,
    fp_lib_table: Option<PathBuf>,
    extra: Vec<(String, String)>,
    only: Option<String>,
    json: bool,
) -> Result<ExitCode, String> {
    let dir = config.dir();
    let share = Path::new("/usr/share/kicad");
    let mut vars = config::vars(dir.as_deref(), std::env::vars(), share.is_dir().then_some(share))?;
    for (k, v) in extra {
        vars.set(k, v);
    }
    let table = |kind: TableKind, flag: Option<PathBuf>| -> Result<Option<LibTable>, String> {
        match flag {
            Some(p) => LibTable::read(kind, &p).map(Some).map_err(|e| e.to_string()),
            None => match &dir {
                Some(d) => LibTable::read_optional(kind, &d.join(kind.file_name())).map_err(|e| e.to_string()),
                None => Ok(None),
            },
        }
    };
    let setup = check::Setup {
        global_sym: table(TableKind::Symbol, sym_lib_table)?,
        global_fp: table(TableKind::Footprint, fp_lib_table)?,
        vars,
        only,
    };
    let report = check::check(paths, &setup);
    if json {
        let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        println!("{text}");
    } else {
        print_report(&report, paths, &setup, dir.as_deref());
    }
    Ok(if report.ok() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

/// A path as the person gave its root: `b129/x.kicad_pcb` under `~/git/hw`.
fn shown(path: &Path, roots: &[PathBuf]) -> String {
    for r in roots {
        let r = std::path::absolute(r).unwrap_or_else(|_| r.clone());
        if let Ok(rel) = path.strip_prefix(&r)
            && !rel.as_os_str().is_empty()
        {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}

fn print_report(report: &Report, roots: &[PathBuf], setup: &check::Setup, dir: Option<&Path>) {
    let c = &report.checked;
    let total = c.symbols + c.footprint_fields + c.footprints + c.models;
    println!(
        "{BOLD}kilib check{BOLD:#}: {} projects, {} files: {} symbols, {} Footprint fields, {} footprints, {} 3D models",
        report.projects, report.files, c.symbols, c.footprint_fields, c.footprints, c.models
    );
    let table_note = |t: &Option<LibTable>, kind: TableKind| match t {
        Some(t) => format!("{} ({} rows)", t.path.display(), t.rows.len()),
        None => format!("no global {}", kind.file_name()),
    };
    println!(
        "{DIM}tables: {}; {}; config: {}{DIM:#}",
        table_note(&setup.global_sym, TableKind::Symbol),
        table_note(&setup.global_fp, TableKind::Footprint),
        dir.map_or("none found".into(), |d| d.display().to_string())
    );
    if let Some(prefix) = &setup.only {
        println!("{DIM}only {prefix}* libraries, the schematics' embedded symbols and footprint filters too{DIM:#}");
    }

    let libs = report.broken_libraries();
    for ((kind, lib), findings) in &libs {
        let uses: usize = findings.iter().map(|f| f.count()).sum();
        println!();
        println!("{RED}✗{RED:#} {kind_label} library {BOLD}{lib}{BOLD:#}: {uses} uses", kind_label = kind.label());
        let mut reasons: Vec<&str> = findings.iter().map(|f| f.reason.as_str()).collect();
        reasons.dedup();
        for r in reasons {
            println!("    {YELLOW}{r}{YELLOW:#}");
        }
        for f in findings {
            print_uses(f, roots, "    ");
        }
    }
    let items: Vec<&Finding> = report.problems.iter().filter(|f| f.library.is_none()).collect();
    for f in &items {
        println!();
        println!("{RED}✗{RED:#} {} {BOLD}{}{BOLD:#}", f.kind.label(), f.id);
        println!("    {YELLOW}{}{YELLOW:#}", f.reason);
        print_uses_files(f, roots, "    ");
    }
    if !report.unchecked.is_empty() {
        println!();
        println!("{CYAN}not checked{CYAN:#} (no library link, or a library type kilib does not read):");
        for f in &report.unchecked {
            let id = if f.id.is_empty() { "(empty id)" } else { &f.id };
            println!("  {} {id}: {} {DIM}in {}{DIM:#}", f.kind.label(), f.reason, files(f, roots));
        }
    }
    println!();
    if report.ok() {
        println!("{GREEN}✓{GREEN:#} all {total} references resolve");
    } else {
        let bad: usize = report.problems.iter().map(Finding::count).sum();
        println!(
            "{RED}✗{RED:#} {bad} of {total} references do not resolve: {} broken libraries, {} other items",
            libs.len(),
            items.len()
        );
    }
}

fn files(f: &Finding, roots: &[PathBuf]) -> String {
    let files: Vec<String> = f.uses.iter().map(|(p, n)| format!("{} ({n})", shown(p, roots))).collect();
    files.join(", ")
}

fn print_uses(f: &Finding, roots: &[PathBuf], indent: &str) {
    println!("{indent}{} {DIM}in {}{DIM:#}", f.id, files(f, roots));
}

fn print_uses_files(f: &Finding, roots: &[PathBuf], indent: &str) {
    for (p, n) in &f.uses {
        println!("{indent}{DIM}{} ({n}){DIM:#}", shown(p, roots));
    }
}

/// The kicad-library checkout around `start`: the nearest folder with `symbols/` and `footprints/`.
fn find_library(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|d| d.join("symbols").is_dir() && d.join("footprints").is_dir()).map(Path::to_path_buf)
}

fn library_or_cwd(library: Option<PathBuf>) -> Result<PathBuf, String> {
    match library {
        Some(l) => Ok(l),
        None => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            Ok(find_library(&cwd).ok_or("no kicad-library checkout around the current folder: pass --library DIR")?)
        }
    }
}

fn run_setup(
    library: Option<PathBuf>,
    config: &ConfigArgs,
    map: Option<PathBuf>,
    vendor: Option<PathBuf>,
    models: Option<PathBuf>,
    stock: &Path,
    dry_run: bool,
) -> Result<ExitCode, String> {
    let library = library_or_cwd(library)?;
    let dir = config.dir().ok_or("no KiCad config folder: pass --config-dir DIR")?;
    if !dir.is_dir() {
        return Err(format!(
            "no KiCad config at {}: start KiCad once so it makes one, or pass --config-dir DIR",
            dir.display()
        ));
    }
    let running = if cfg!(target_os = "linux") { setup::kicad_running(Path::new("/proc")) } else { Vec::new() };
    if !running.is_empty() && !dry_run {
        return Err(format!(
            "KiCad is running ({}): it writes these files when it closes; close it and run setup again",
            running.join(", ")
        ));
    }
    let canon = |p: &Path| std::fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()));
    let src = setup::Sources {
        library: canon(&library)?,
        vendor: vendor.as_deref().map(canon).transpose()?,
        models: models.as_deref().map(canon).transpose()?,
        map,
    };
    let plan = setup::plan(&src, &dir, Some(stock))?;
    let from = src.map.as_ref().map_or("the library folders".into(), |m| m.display().to_string());
    println!("{BOLD}kilib setup{BOLD:#}: {} → {} (rows from {from})", src.library.display(), dir.display());
    for change in &plan.changes {
        let name = change.path.file_name().unwrap_or_default().to_string_lossy();
        let (out, inn) = change.lines.iter().fold((0, 0), |(o, i), l| match l {
            Line::Removed(_) => (o + 1, i),
            Line::Added(_) => (o, i + 1),
            Line::Note(_) => (o, i),
        });
        println!();
        println!("{BOLD}{name}{BOLD:#}: {out} out, {inn} in");
        for line in &change.lines {
            match line {
                Line::Removed(l) => println!("  {RED}- {l}{RED:#}"),
                Line::Added(l) => println!("  {GREEN}+ {l}{GREEN:#}"),
                Line::Note(l) => println!("  {CYAN}{l}{CYAN:#}"),
            }
        }
    }
    for p in &plan.unchanged {
        println!("{DIM}{}: already set up{DIM:#}", p.file_name().unwrap_or_default().to_string_lossy());
    }
    for (name, value) in setup::variables(&src) {
        if let Ok(env) = std::env::var(name)
            && env != value
        {
            println!(
                "{YELLOW}note:{YELLOW:#} the environment sets {name}={env}, which KiCad uses over kicad_common.json"
            );
        }
    }
    println!();
    if plan.changes.is_empty() {
        println!("{GREEN}✓{GREEN:#} nothing to change");
    } else if dry_run {
        if !running.is_empty() {
            println!(
                "{YELLOW}note:{YELLOW:#} KiCad is running ({}): setup without --dry-run refuses until it is closed",
                running.join(", ")
            );
        }
        println!("{CYAN}dry run{CYAN:#}: nothing written; run without --dry-run to write {} files", plan.changes.len());
    } else {
        setup::apply(&plan)?;
        println!("{GREEN}✓{GREEN:#} wrote {} files", plan.changes.len());
    }
    let _ = std::io::stdout().flush();
    Ok(ExitCode::SUCCESS)
}

fn run_migrate(map: &Path, paths: &[PathBuf], write: bool, stat: bool) -> Result<ExitCode, String> {
    let map = kilib::map::Map::read(map)?;
    let plan = migrate::plan(&map, paths);
    let roots: Vec<PathBuf> = paths.to_vec();
    if !write && !stat {
        for c in &plan.changes {
            for line in c.diff(&shown(&c.path, &roots)).lines() {
                match line.as_bytes().first() {
                    Some(b'+') if !line.starts_with("+++") => println!("{GREEN}{line}{GREEN:#}"),
                    Some(b'-') if !line.starts_with("---") => println!("{RED}{line}{RED:#}"),
                    Some(b'@') => println!("{CYAN}{line}{CYAN:#}"),
                    _ => println!("{line}"),
                }
            }
        }
    }
    // The summary goes to stderr, so the diff on stdout stays a clean patch.
    let c = &plan.counts;
    eprintln!(
        "{BOLD}kilib migrate{BOLD:#}: {} files read, {} to change: {} symbol ids, {} per-package symbols, {} symbol footprints, {} Footprint fields, {} footprints, {} filters, {} 3D models, {} table rows",
        plan.files,
        plan.changes.len(),
        c.symbols,
        c.per_package,
        c.symbol_footprints,
        c.footprint_fields,
        c.footprints,
        c.filters,
        c.models,
        c.table_rows
    );
    for n in &plan.notes {
        let (mark, style) = match n.severity {
            migrate::Severity::Error => ("✗", RED),
            migrate::Severity::Warning => ("!", YELLOW),
        };
        let files: Vec<String> = n.uses.iter().map(|(p, k)| format!("{} ({k})", shown(p, &roots))).collect();
        eprintln!("{style}{mark}{style:#} {} {DIM}in {}{DIM:#}", n.message, files.join(", "));
    }
    if plan.has_errors() {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if plan.changes.is_empty() {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
    } else if write {
        migrate::apply(&plan)?;
        eprintln!("{GREEN}✓{GREEN:#} wrote {} files", plan.changes.len());
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_per_package(map: &Path, boards: &[PathBuf], library: Option<PathBuf>, write: bool) -> Result<ExitCode, String> {
    let map = kilib::map::Map::read(map)?;
    let library = std::path::absolute(library_or_cwd(library)?).map_err(|e| e.to_string())?;
    if !boards.is_empty() {
        let placed = per_package::survey(&library, &map, boards)?;
        for p in &placed {
            let fp = if p.footprint.is_empty() { "(no footprint)" } else { p.footprint.as_str() };
            let to = match &p.to {
                Some(to) => format!("{GREEN}→{GREEN:#} {to}"),
                None => format!("{YELLOW}not in the map{YELLOW:#}"),
            };
            println!("  {} {fp} ×{} {to} {DIM}{}{DIM:#}", p.generic, p.refs.len(), p.refs.join(" "));
        }
        let open = placed.iter().filter(|p| p.to.is_none()).count();
        eprintln!(
            "{BOLD}kilib per-package{BOLD:#}: {} generic symbols placed with {} footprints, {open} not in the map",
            placed.iter().map(|p| &p.generic).collect::<std::collections::BTreeSet<_>>().len(),
            placed.len()
        );
    }
    let plan = per_package::plan(&library, &map);
    let roots = [library.clone()];
    for item in plan.to_write() {
        println!(
            "{GREEN}+{GREEN:#} {} {DIM}extends {}, {} ({}){DIM:#}",
            item.id,
            item.generic,
            item.footprint,
            shown(&item.path, &roots)
        );
    }
    for (sev, msg) in &plan.notes {
        match sev {
            migrate::Severity::Error => eprintln!("{RED}✗{RED:#} {msg}"),
            migrate::Severity::Warning => eprintln!("{YELLOW}!{YELLOW:#} {msg}"),
        }
    }
    let new = plan.to_write().count();
    eprintln!("{BOLD}kilib per-package{BOLD:#}: {} per-package symbols in the map, {new} to make", plan.items.len());
    if plan.has_errors() {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if new == 0 {
        eprintln!("{GREEN}✓{GREEN:#} every per-package symbol is in the library");
    } else if write {
        per_package::apply(&plan)?;
        eprintln!("{GREEN}✓{GREEN:#} wrote {new} symbols");
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to make them");
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn run_restructure(map: &Path, vendor: &Path, library: Option<PathBuf>, dry_run: bool) -> Result<ExitCode, String> {
    let map = kilib::map::Map::read(map)?;
    let library = std::path::absolute(library_or_cwd(library)?).map_err(|e| e.to_string())?;
    let vendor = std::path::absolute(vendor).map_err(|e| e.to_string())?;
    let plan = restructure::plan(&map, &library, &vendor);
    let roots = [library.clone(), vendor.clone()];
    let (mut symbols, mut footprints, mut dropped) = (0, 0, 0);
    for m in &plan.moves {
        let from = shown(&m.from, &roots);
        match &m.to {
            None => {
                dropped += 1;
                println!("{RED}-{RED:#} {} {} {DIM}({from}){DIM:#}: dropped", m.kind.label(), m.id);
            }
            Some(to) => {
                match m.kind {
                    kilib::map::ItemKind::Symbol => symbols += 1,
                    kilib::map::ItemKind::Footprint => footprints += 1,
                }
                let to = if to.starts_with(&library) { shown(to, &roots) } else { to.display().to_string() };
                let why = match m.why {
                    restructure::Why::Item => "",
                    restructure::Why::Library => " (library default)",
                    restructure::Why::Parent => " (with its parent)",
                };
                let rewritten =
                    if m.rewritten > 0 { format!(", {} strings rewritten", m.rewritten) } else { String::new() };
                println!("  {} {} → {to}{DIM}{why}{rewritten}{DIM:#}", m.kind.label(), m.id);
            }
        }
    }
    for (sev, msg) in &plan.notes {
        match sev {
            migrate::Severity::Error => eprintln!("{RED}✗{RED:#} {msg}"),
            migrate::Severity::Warning => eprintln!("{YELLOW}!{YELLOW:#} {msg}"),
        }
    }
    eprintln!(
        "{BOLD}kilib restructure{BOLD:#}: {symbols} symbols, {footprints} footprints to move, {dropped} dropped; {} old files out",
        plan.remove.len()
    );
    if plan.has_errors() {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if plan.moves.is_empty() && plan.remove.is_empty() {
        eprintln!("{GREEN}✓{GREEN:#} nothing to move");
    } else if dry_run {
        eprintln!("{CYAN}dry run{CYAN:#}: nothing written");
    } else {
        restructure::apply(&plan)?;
        eprintln!("{GREEN}✓{GREEN:#} moved");
    }
    Ok(ExitCode::SUCCESS)
}

fn run_fab(paths: Vec<PathBuf>, write: bool, verbose: bool) -> Result<ExitCode, String> {
    let paths = if paths.is_empty() { vec![library_or_cwd(None)?.join("footprints")] } else { paths };
    let items = fab::plan(&paths);
    let (mut changed, mut fine, mut skipped) = (0, 0, 0);
    let (mut moved, mut resized, mut turned, mut unclean) = (0, 0, 0, Vec::new());
    let (mut placements, mut value_placements) = (Vec::new(), Vec::new());
    for i in &items {
        match &i.outcome {
            fab::Outcome::Fine => {
                fine += 1;
                if verbose {
                    println!("{DIM}  {}: fine{DIM:#}", i.id);
                }
            }
            fab::Outcome::Skipped(why) => {
                skipped += 1;
                println!("{YELLOW}!{YELLOW:#} {}: skipped, {why}", i.id);
            }
            fab::Outcome::Change { why, placement: p, body_from, .. } => {
                changed += 1;
                let (m, r, t) = i.outcome.designator_changes();
                moved += usize::from(m);
                resized += usize::from(r);
                turned += usize::from(t);
                let angle = if p.angle == 0.0 { String::new() } else { format!(" {}°", fab::fmt(p.angle)) };
                let from = match body_from {
                    Some(fab::BodyFrom::Fab) => "",
                    Some(fab::BodyFrom::RefLayer) => ", body from its reference-layer drawing",
                    Some(fab::BodyFrom::Courtyard) => ", body from the courtyard",
                    Some(fab::BodyFrom::Pads) => ", body from the pads",
                    None => ", no body (kept where it was)",
                };
                let fit = if p.clean { "" } else { ", no clean fit" };
                println!(
                    "  {BOLD}{}{BOLD:#}: {} mm{angle} at ({}, {}){from}{fit} {DIM}({}){DIM:#}",
                    i.id,
                    fab::fmt(p.size),
                    fab::fmt(p.x),
                    fab::fmt(p.y),
                    why.join("; ")
                );
            }
        }
        // A footprint already fine can still be one where nothing fits cleanly.
        // A mechanical footprint's designator is hidden: not sized.
        let shown = layers::class_of(&i.id, "").shown();
        let read = fab::read_as(&i.old, &i.id).ok();
        let placed = match &i.outcome {
            _ if !shown => None,
            fab::Outcome::Change { placement, .. } => Some(*placement),
            fab::Outcome::Fine => read.as_ref().and_then(|f| f.shape.as_ref()).map(fab::layout),
            fab::Outcome::Skipped(_) => None,
        };
        let fit = placed.map(|p| p.clean);
        if fit == Some(false) {
            unclean.push(i.id.as_str());
        }
        if let Some(p) = placed {
            placements.push((i.id.as_str(), p));
            // Only a real part's Value is shown and fitted.
            let value = read.as_ref().filter(|f| f.value_shown()).and_then(|f| match &i.outcome {
                fab::Outcome::Change { value, .. } => *value,
                _ => f.value_placement(),
            });
            if let Some(v) = value {
                value_placements.push((i.id.as_str(), v));
            }
        }
    }
    eprintln!(
        "{BOLD}kilib designators{BOLD:#}: {} footprints: {changed} to change ({moved} moved, {resized} resized, {turned} turned), {fine} already fine, {skipped} skipped",
        items.len()
    );
    report_sizes(&placements);
    if !unclean.is_empty() {
        eprintln!(
            "{CYAN}no clean fit{CYAN:#} ({}: centred at {} mm over the drawing or past the body): {}",
            unclean.len(),
            fab::fmt(fab::MIN_SIZE),
            unclean.join(", ")
        );
    }
    let unclean_values: Vec<&str> = value_placements.iter().filter(|(_, v)| !v.clean).map(|(id, _)| *id).collect();
    let sizes: Vec<f64> = value_placements.iter().filter(|(_, v)| v.clean).map(|(_, v)| v.size).collect();
    if !sizes.is_empty() {
        let (lo, hi) = sizes.iter().fold((f64::MAX, 0.0_f64), |(l, h), s| (l.min(*s), h.max(*s)));
        eprintln!(
            "values: {} centred on the body ({} to {} mm), {} with no clean fit",
            sizes.len(),
            fab::fmt(lo),
            fab::fmt(hi),
            unclean_values.len()
        );
    }
    if !unclean_values.is_empty() {
        eprintln!("{CYAN}value with no clean fit{CYAN:#} ({}): {}", unclean_values.len(), unclean_values.join(", "));
    }
    if changed == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        Ok(ExitCode::SUCCESS)
    } else if write {
        let n = fab::apply(&items)?;
        eprintln!("{GREEN}✓{GREEN:#} wrote {n} footprints");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        Ok(ExitCode::from(1))
    }
}

fn run_layout_fields(paths: Vec<PathBuf>, write: bool) -> Result<ExitCode, String> {
    let paths = if paths.is_empty() { vec![library_or_cwd(None)?.join("symbols")] } else { paths };
    let items = layout_fields::plan(&paths);
    let (mut files, mut symbols, mut left) = (0, 0, 0);
    for i in &items {
        if let Some(e) = &i.error {
            left += 1;
            println!("{YELLOW}!{YELLOW:#} {}: {e}", i.path.display());
            continue;
        }
        for why in &i.added.skipped {
            left += 1;
            println!("{YELLOW}!{YELLOW:#} {}: {why}", i.path.display());
        }
        if i.new.is_some() {
            files += 1;
            symbols += i.added.symbols;
            println!("  {BOLD}{}{BOLD:#}: {} symbols", i.path.display(), i.added.symbols);
        }
    }
    eprintln!(
        "{BOLD}kilib layout-fields{BOLD:#}: {} files: {files} to change, {symbols} symbols get the fields; {left} left alone",
        items.len()
    );
    if files == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        Ok(ExitCode::SUCCESS)
    } else if write {
        let n = layout_fields::apply(&items)?;
        eprintln!("{GREEN}✓{GREEN:#} wrote {n} files");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        Ok(ExitCode::from(1))
    }
}

fn run_layers(boards: &[PathBuf], write: bool) -> Result<ExitCode, String> {
    let mut plans = Vec::new();
    let (mut changed, mut taken) = (0, 0);
    for b in boards {
        let old = std::fs::read_to_string(b).map_err(|e| format!("{}: {e}", b.display()))?;
        let (new, changes) = layers::add_to_board(&old).map_err(|e| format!("{}: {e}", b.display()))?;
        for c in &changes {
            match c {
                layers::LayerChange::Fine(_) => {}
                layers::LayerChange::Set(l) => {
                    println!("  {BOLD}{}{BOLD:#}: {} {} \"{}\"", b.display(), l.id, l.side, l.name)
                }
                layers::LayerChange::Taken { layer: l, line } => {
                    taken += 1;
                    println!("{YELLOW}!{YELLOW:#} {}: {} is taken ({line}); {} not added", b.display(), l.id, l.name);
                }
                layers::LayerChange::Retired { id, name } => {
                    println!("  {BOLD}{}{BOLD:#}: {id} no longer named \"{name}\"", b.display())
                }
                layers::LayerChange::BodiesMoved { front, back } => println!(
                    "  {BOLD}{}{BOLD:#}: footprint bodies off User.2: {front} to Body.F, {back} to Body.B",
                    b.display()
                ),
            }
        }
        if new != old {
            changed += 1;
        }
        plans.push((b, old, new));
    }
    eprintln!("{BOLD}kilib layers{BOLD:#}: {} boards: {changed} to change, {taken} layers taken", boards.len());
    if changed == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        return Ok(if taken == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) });
    }
    if !write {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        return Ok(ExitCode::from(1));
    }
    for (b, old, new) in &plans {
        if old == new {
            continue;
        }
        let now = std::fs::read_to_string(b).map_err(|e| format!("{}: {e}", b.display()))?;
        if &now != old {
            return Err(format!("{} changed since it was read; run again", b.display()));
        }
        std::fs::write(b, new).map_err(|e| format!("{}: {e}", b.display()))?;
    }
    eprintln!("{GREEN}✓{GREEN:#} wrote {changed} boards");
    Ok(if taken == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

fn run_presets(
    projects: &[PathBuf],
    table: Option<PathBuf>,
    library: Option<PathBuf>,
    write: bool,
) -> Result<ExitCode, String> {
    let table_path = match table {
        Some(p) => p,
        None => library_or_cwd(library)?.join(presets::TABLE_FILE),
    };
    let table = presets::Table::read(&table_path)?;
    let mut plans = Vec::new();
    let mut changed = 0;
    for p in projects {
        let old = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let board_path = p.with_extension("kicad_pcb");
        let board = board_path
            .exists()
            .then(|| std::fs::read_to_string(&board_path).map_err(|e| format!("{}: {e}", board_path.display())))
            .transpose()?;
        let (new, report) =
            presets::apply(&old, board.as_deref(), &table).map_err(|e| format!("{}: {e}", p.display()))?;
        if !report.set.is_empty() {
            changed += 1;
            println!("  {BOLD}{}{BOLD:#}: {} set", p.display(), report.set.join(", "));
        }
        plans.push((p, old, new));
    }
    eprintln!("{BOLD}kilib presets{BOLD:#}: {} projects: {changed} to change", projects.len());
    if changed == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        return Ok(ExitCode::SUCCESS);
    }
    if !write {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        return Ok(ExitCode::from(1));
    }
    for (p, old, new) in &plans {
        if old == new {
            continue;
        }
        let now = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        if &now != old {
            return Err(format!("{} changed since it was read; run again", p.display()));
        }
        std::fs::write(p, new).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    eprintln!("{GREEN}✓{GREEN:#} wrote {changed} projects");
    Ok(ExitCode::SUCCESS)
}

/// How many designators got each size, and which fit only below the usual floor.
fn report_sizes(placements: &[(&str, fab::Placement)]) {
    if placements.is_empty() {
        return;
    }
    let mut sizes: Vec<(i64, usize)> = Vec::new();
    for (_, p) in placements {
        let k = (p.size * 100.0).round() as i64;
        match sizes.iter_mut().find(|(s, _)| *s == k) {
            Some((_, n)) => *n += 1,
            None => sizes.push((k, 1)),
        }
    }
    sizes.sort_by_key(|a| std::cmp::Reverse(a.0));
    let list: Vec<String> = sizes.iter().map(|(s, n)| format!("{} mm ×{n}", fab::fmt(*s as f64 / 100.0))).collect();
    eprintln!("{CYAN}sizes{CYAN:#}: {}", list.join(", "));
    let tiny: Vec<String> = placements
        .iter()
        .filter(|(_, p)| p.clean && p.tiny())
        .map(|(id, p)| format!("{id} ({} mm)", fab::fmt(p.size)))
        .collect();
    if !tiny.is_empty() {
        eprintln!(
            "{CYAN}tiny{CYAN:#} ({}: below {} mm, nothing larger fits): {}",
            tiny.len(),
            fab::fmt(fab::MIN_SIZE),
            tiny.join(", ")
        );
    }
}

fn run_fab_board(path: &Path, write: bool, verbose: bool) -> Result<ExitCode, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let plan = fab::plan_board(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let (mut changed, mut fine, mut skipped, mut unclean) = (0, 0, Vec::new(), Vec::new());
    let (mut values, mut notes, mut pins) = (0, 0, 0);
    let mut pins_left = Vec::new();
    let mut placements = Vec::new();
    for i in &plan.items {
        let name = if i.reference.is_empty() { i.id.as_str() } else { i.reference.as_str() };
        match &i.outcome {
            fab::BoardOutcome::Fine(p) => {
                fine += 1;
                placements.push((name, *p));
                if verbose {
                    println!("{DIM}  {name} ({}): fine{DIM:#}", i.id);
                }
            }
            fab::BoardOutcome::Change { before: b, placement: p, angle } => {
                changed += 1;
                placements.push((name, *p));
                let fit = if p.clean { "" } else { ", no clean fit" };
                println!(
                    "  {BOLD}{name}{BOLD:#}: {} mm {}° at ({}, {}){fit} {DIM}(was {} mm {}° at ({}, {}); {}){DIM:#}",
                    fab::fmt(p.size),
                    fab::fmt(*angle),
                    fab::fmt(p.x),
                    fab::fmt(p.y),
                    fab::fmt(b.size),
                    fab::fmt(b.angle),
                    fab::fmt(b.x),
                    fab::fmt(b.y),
                    i.id
                );
            }
            fab::BoardOutcome::Skipped(why) => {
                skipped.push((name, why));
                if verbose {
                    println!("{DIM}  {name} ({}): skipped, {why}{DIM:#}", i.id);
                }
            }
        }
        if let fab::BoardOutcome::Fine(p) | fab::BoardOutcome::Change { placement: p, .. } = &i.outcome
            && !p.clean
        {
            unclean.push(name);
        }
        match &i.value {
            fab::BoardOutcome::Change { before: b, placement: p, angle } => {
                values += 1;
                let fit = if p.clean { "" } else { ", no clean fit" };
                println!(
                    "  {BOLD}{name}{BOLD:#} value: {} mm {}° at ({}, {}){fit} {DIM}(was {} mm {}° at ({}, {})){DIM:#}",
                    fab::fmt(p.size),
                    fab::fmt(*angle),
                    fab::fmt(p.x),
                    fab::fmt(p.y),
                    fab::fmt(b.size),
                    fab::fmt(b.angle),
                    fab::fmt(b.x),
                    fab::fmt(b.y),
                );
            }
            fab::BoardOutcome::Skipped(why) if verbose => println!("{DIM}  {name} value: skipped, {why}{DIM:#}"),
            _ => {}
        }
        for (field, outcome) in &i.notes {
            match outcome {
                fab::BoardOutcome::Change { before: b, placement: p, angle } => {
                    notes += 1;
                    let fit = if p.clean { "" } else { ", no clean fit" };
                    println!(
                        "  {BOLD}{name}{BOLD:#} {field}: {} mm {}° at ({}, {}){fit} {DIM}(was {} mm {}° at ({}, {})){DIM:#}",
                        fab::fmt(p.size),
                        fab::fmt(*angle),
                        fab::fmt(p.x),
                        fab::fmt(p.y),
                        fab::fmt(b.size),
                        fab::fmt(b.angle),
                        fab::fmt(b.x),
                        fab::fmt(b.y),
                    );
                }
                fab::BoardOutcome::Skipped(why) => {
                    println!("  {YELLOW}{name} {field}: skipped{YELLOW:#}, {why}")
                }
                fab::BoardOutcome::Fine(_) => {}
            }
        }
        for (text, from, to) in &i.pins.turned {
            pins += 1;
            if verbose {
                println!("  {BOLD}{name}{BOLD:#} pin {text}: {}° to {}°", fab::fmt(*from), fab::fmt(*to));
            }
        }
        pins_left.extend(i.pins.skipped.iter().map(|w| format!("{name} {w}")));
    }
    eprintln!(
        "{BOLD}kilib designators{BOLD:#} {}: {} footprints: {changed} to change, {fine} already fine, {} skipped; {values} values to change, {notes} layout notes, {pins} pin numbers to turn",
        path.display(),
        plan.items.len(),
        skipped.len()
    );
    if !skipped.is_empty() {
        let mut why: Vec<&String> = skipped.iter().map(|(_, w)| *w).collect();
        why.sort();
        why.dedup();
        for w in why {
            let names: Vec<&str> = skipped.iter().filter(|(_, x)| *x == w).map(|(n, _)| *n).collect();
            eprintln!("{YELLOW}skipped{YELLOW:#} ({}: {w}): {}", names.len(), names.join(", "));
        }
    }
    report_sizes(&placements);
    if !pins_left.is_empty() {
        eprintln!(
            "{YELLOW}pin numbers left{YELLOW:#} ({}: not level or vertical, or on the back): {}",
            pins_left.len(),
            pins_left.join(", ")
        );
    }
    if !unclean.is_empty() {
        eprintln!(
            "{CYAN}no clean fit{CYAN:#} ({}: centred at {} mm over the drawing or past the body): {}",
            unclean.len(),
            fab::fmt(fab::MIN_SIZE),
            unclean.join(", ")
        );
    }
    if changed == 0 && values == 0 && notes == 0 && pins == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to change");
        Ok(ExitCode::SUCCESS)
    } else if write {
        fab::apply_board(path, &plan)?;
        eprintln!(
            "{GREEN}✓{GREEN:#} wrote {changed} designators, {values} values, {notes} layout notes and {pins} pin numbers to {}",
            path.display()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to apply");
        Ok(ExitCode::from(1))
    }
}

fn run_stock_import(ids: &[String], symbols: bool, args: &StockArgs, write: bool) -> Result<ExitCode, String> {
    let library = library_or_cwd(args.library.clone())?;
    let (imports, what) = if symbols {
        let fps = library.join("footprints");
        (stock::symbol::plan_import(&args.stock_symbol_dir()?, &library.join("symbols"), &fps, ids), "symbols")
    } else {
        (stock::plan_import(&args.stock_dir()?, &library.join("footprints"), ids), "footprints")
    };
    let (mut new, mut errors) = (0, 0);
    for i in &imports {
        match &i.outcome {
            stock::ImportOutcome::New { .. } => {
                new += 1;
                println!("  {} -> {}", i.id, shown(&i.to, std::slice::from_ref(&library)));
            }
            stock::ImportOutcome::Exists => println!("{DIM}  {}: in the library already{DIM:#}", i.id),
            stock::ImportOutcome::Error(e) => {
                errors += 1;
                println!("{RED}✗{RED:#} {}: {e}", i.id);
            }
        }
    }
    eprintln!("{BOLD}kilib stock import{BOLD:#}: {} {what}: {new} to copy, {errors} errors", imports.len());
    if errors > 0 {
        eprintln!("{RED}✗{RED:#} errors: nothing written");
        return Ok(ExitCode::from(1));
    }
    if new == 0 {
        eprintln!("{GREEN}✓{GREEN:#} nothing to copy");
        return Ok(ExitCode::SUCCESS);
    }
    if !write {
        eprintln!("{CYAN}not written{CYAN:#}: run with --write to copy");
        return Ok(ExitCode::from(1));
    }
    let n = stock::apply_import(&imports)?;
    eprintln!(
        "{GREEN}✓{GREEN:#} copied {n} {what}; a new VHRD_* library needs its row in the map's [libraries] and kilib setup"
    );
    Ok(ExitCode::SUCCESS)
}

fn run_stock_diff(args: &StockArgs, stat: bool) -> Result<ExitCode, String> {
    let library = library_or_cwd(args.library.clone())?;
    let footprints = library.join("footprints");
    let kinds = [
        ("footprints", args.stock_dir().map(|d| (stock::compare(&d, &footprints), d))),
        (
            "symbols",
            args.stock_symbol_dir().map(|d| (stock::symbol::compare(&d, &library.join("symbols"), &footprints), d)),
        ),
    ];
    if let [(_, Err(a)), (_, Err(b))] = &kinds {
        return Err(format!("{a}; {b}"));
    }
    let mut failed = false;
    for (what, found) in &kinds {
        let (all, stock_dir) = match found {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{YELLOW}{what} skipped{YELLOW:#}: {e}");
                continue;
            }
        };
        let (mut same, mut differ, mut errors) = (0, 0, 0);
        for c in all {
            match &c.outcome {
                stock::Compared::Same => same += 1,
                stock::Compared::Differs(d) => {
                    differ += 1;
                    if stat {
                        println!("  {}: differs from stock", c.id);
                        continue;
                    }
                    for line in d.lines() {
                        match line.as_bytes().first() {
                            Some(b'+') if !line.starts_with("+++") => println!("{GREEN}{line}{GREEN:#}"),
                            Some(b'-') if !line.starts_with("---") => println!("{RED}{line}{RED:#}"),
                            Some(b'@') => println!("{CYAN}{line}{CYAN:#}"),
                            _ => println!("{line}"),
                        }
                    }
                }
                stock::Compared::Error(e) => {
                    errors += 1;
                    eprintln!("{RED}✗{RED:#} {}: {e}", c.id);
                }
            }
        }
        failed |= differ + errors > 0;
        eprintln!(
            "{BOLD}kilib stock diff{BOLD:#}: {} {what} with a stock namesake in {}: {same} as stock, {differ} differ, {errors} errors",
            all.len(),
            stock_dir.display()
        );
    }
    Ok(if failed { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
