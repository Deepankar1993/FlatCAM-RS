# Changelog — FlatCAM-RS

All notable changes to the Rust port are recorded here.

## [0.6.0] — Phase 3: utilities batch + GUI export

### Added
- **`fc-cam`** modules (6, agent-authored in parallel): `calculators` (V-bit
  width/depth, unit conversion, electroplating time), `film` (negative export),
  `align` (2-point similarity transform), `distance` (point + geometry distance),
  `optimal` (minimum feature spacing), `report` (object statistics).
- **GUI**: G-code export via save dialog + preprocessor selector combo box.

### Changed
- `distance`/`optimal` use the non-deprecated `geo::Euclidean::distance`.

### Verified
- `cargo test --workspace`: 113 passed, 0 warnings.

## [0.5.0] — Phase 7 start: desktop GUI scaffold

### Added
- **`fc-gui`** (binary `flatcam-gui`) — native `eframe`/`egui` desktop app:
  open Gerber/Excellon, render geometry on a pan/zoom 2D canvas, run isolation
  and paint, overlay tool-paths, adjust tool Ø / passes / overlap. Replaces the
  PyQt6 + VisPy/matplotlib stack; all compute runs through the Rust crates.

### Verified
- `cargo build -p fc-gui`: compiles (eframe/egui 0.29).
- `cargo test --workspace`: 84 passed (GUI binary compiles in the test build).

## [0.4.0] — Phase 2/3: second orchestrated batch

5 more modules authored by agents in parallel (~70 s), integrated green first try.

### Added
- **`fc-cam::follow`** — trace centre-line engraving job (`ToolFollow`).
- **`fc-cam::solderpaste`** — paste dispense paths over pads (`ToolSolderPaste`).
- **`fc-cam::thieving`** — copper thieving dot-grid fill kept clear of copper
  (`ToolCopperThieving`).
- **`fc-cam::rulescheck`** — minimum-clearance DRC (`ToolRulesCheck`).
- **`fc-cam::levelling`** — bed-levelling probe grid (`ToolLevelling`).

### Verified
- `cargo test --workspace`: 84 passed, 0 warnings.

## [0.3.0] — Phase 2/3: orchestrated feature batch

Authored by 6 agents in parallel (~73 s) against the frozen API in
`docs/AGENT_GUIDE.md`, integrated with a single `cargo test` gate (green first
try). Demonstrates the modular parallel-development model.

### Added
- **`fc-cam::milling`** — general profile + pocket milling (`ToolMilling` core).
- **`fc-cam::sub`** — geometry boolean subtract (`ToolSub` core).
- **`fc-cam::etch`** — etch compensation widening (`ToolEtchCompensation` core).
- **`fc-cam::invert`** — invert Gerber copper within a bbox (`ToolInvertGerber`).
- **`fc-cam::drilloptim`** — greedy nearest-neighbor drill ordering to cut rapid
  travel (`ToolDrilling` enhancement).
- **`fc-cam::fiducials`** — fiducial/marker geometry (`ToolFiducials`/`ToolMarkers`).

### Verified
- `cargo test --workspace`: 64 passed, 0 warnings.

## [0.2.0] — Phase 2 (in progress): paint, NCC, cutout, panelize, transforms, dialects

### Added
- **`fc-geo::transform`** — affine primitives (translate, rotate, scale, skew,
  mirror X/Y) and `bounds()`. Foundation for `ToolTransform`, panelize,
  double-sided. (1 test)
- **`fc-cam::paint`** — area paint / pocket infill (`ToolPaint` core): inset by
  tool radius + margin, horizontal scanline fill spaced by `tool_dia·(1−overlap)`
  with even-odd hole handling and zig-zag direction alternation, plus optional
  boundary contour pass. (3 tests)
- **`fc-cam::ncc`** — non-copper clear (`ToolNCC` core): board-rect minus copper,
  infilled via the paint engine. (tests)
- **`fc-cam::cutout`** — board outline milling (`ToolCutOut` core) with evenly
  spaced holding tabs (densified ring, mid-edge tab placement). (tests)
- **`fc-cam::panelize`** — panelization (nx×ny tiling, auto-pitch) and
  double-sided mirror (`ToolPanelize`/`ToolDblSided` cores). (tests)
- **`fc-gcode::dialects`** — additional preprocessors: GenericDefault, GrblNoM6,
  GrblLaser (laser power on S, no Z), RolandMDX, plus `by_name()` lookup. (tests)
- **`fc-cli`** — new commands `paint`, `ncc`, `cutout`; `--preproc` now selects
  any registered dialect (grbl/marlin/default/grbl_no_m6/grbl_laser/roland).

### Verified
- `cargo test --workspace`: 46 passed.
- `cargo build --release`: ok. NCC/cutout/laser verified end-to-end on the
  sample board (scanlines correctly split around copper; cutout leaves 4 tabs).

## [0.1.0] — Phase 1: CAM engine core

Initial working, tested, GUI-free CAM pipeline. Purely additive to the Python
project (no Python files touched).

### Added
- **`fc-geo`** — geometry foundation over `geo` + `geo-buffer`: circle, regular
  polygon, rect, obround, round-capped path buffering, union/difference, and
  orientation-safe polygon offsetting. (6 tests)
- **`fc-gerber`** — RS-274X parser: FS/MO format & units, C/R/O/P apertures,
  aperture macros (primitives 1/2/4/5/20/21/22) with a `$n`/`+ - x /`/parens
  arithmetic evaluator, D01/D02/D03, G01 linear and G02/G03 arcs (single- &
  multi-quadrant), G36/G37 regions, LP dark/clear polarity. Produces a unified
  solid `MultiPolygon` + follow geometry. (7 tests)
- **`fc-excellon`** — Excellon parser: header/body, INCH/METRIC, LZ/TZ and
  inline format, exact zero-suppression coordinate decoding, tool defs &
  selection, drills, G85 slots, G00/G01 routed slots, unit inference. (5 tests)
- **`fc-gcode`** — dialect-independent `CncJob` (mill/drill) with multi-pass
  plunging and a `Preprocessor` trait; `Grbl` and `Marlin` dialects. (4 tests)
- **`fc-cam`** — isolation routing (offset-based, multi-pass + overlap) and
  drilling job generation. (4 tests)
- **`fc-cli`** — headless `flatcam-rs` binary: `info`, `iso`, `drill`.
- `examples/` Gerber + Excellon fixtures and end-to-end CLI verification.
- `docs/DESIGN.md`, `docs/ROADMAP.md`.

### Verified
- `cargo test --workspace`: 22 passed.
- `cargo build --release`: 700 KB binary; 2-pass isolation of the sample board
  in ~68 ms.

### Fixed
- Orientation normalization before `geo-buffer` offset: boolean-op (i_overlay)
  output can emit clockwise exterior rings, which were being offset inward and
  collapsing isolation tool paths to empty. Caught by a unit test.
