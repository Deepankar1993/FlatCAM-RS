# Changelog — FlatCAM-RS

All notable changes to the Rust port are recorded here.

## [0.15.0] — engine + scripting expansion (7-module batch)

7 modules authored by agents in parallel, integrated green.

### Added
- **`fc-gcode::parse_gcode`** — read G-code text back into a `CncJob` (modal
  G0/G1, Z-cut detection, units). Round-trips with the emitter.
- **`fc-geo::shapes`** — `rounded_rect`, `slot`, `star`, `ring` constructors.
- **`fc-cam::array`** — `linear_array` / `circular_array` of geometry.
- **`fc-cam::tsp`** — `nearest_neighbor` + `two_opt` travel optimization.
- **`fc-cam::levelmap`** — `HeightMap` (IDW) + `apply_paths` for bed-leveling
  Z compensation of tool-paths.
- **`fc-script`**: `transform` commands (rotate/mirror_y/array/panelize) and
  `analyze` commands (drc/min_spacing/report) — 24 script commands total now.

### Verified
- `cargo test --workspace`: 401 passed, 0 warnings.

## [0.14.0] — scripting engine + GUI project save/load

### Added
- **`fc-script`** (new crate) — headless batch/scripting engine (parity path for
  `tclCommands/`): a `Registry` + `ScriptContext` with 22 commands authored by 5
  agents in parallel — io (open_gerber/excellon/svg/dxf, export_gcode), cam
  (isolate/paint/ncc/cutout/drill), geo_ops (offset/scale/translate/mirror_x/
  subtract/union), query (list/bounds/area/count/delete/rename), gen
  (new_rect/new_circle/drill_array). 41 tests.
- **CLI `script <file>`** — run a batch script (verified end-to-end:
  rect→subtract hole→isolate→export G-code).
- **GUI project save/load** — Open/Save Project (JSON via `fc-app`); objects
  track their source path and geometry is regenerated on load.

### Verified
- `cargo test --workspace`: 360 passed, 0 warnings. 13 crates.

## [0.13.0] — Phase 7: project tree

### Added
- **`fc-app` tree logic** (6 modules, agent-authored in parallel via separate
  `impl Project` files): `tree` (grouped/selectable rows), `naming` (unique
  names + rename with child re-parenting), `relations` (children/descendants +
  cascade delete), `properties` (inspector rows), `ordering` (visibility,
  move up/down, duplicate), `icons` (kind label/icon/color/sort). +38 tests.
  Added `visible`/`parent` to ProjectObject and `selected` to Project.
- **GUI: object-centric project tree** — the app now holds an `fc_app::Project`
  + a runtime geometry store. Loaded files and CAM results are objects in a
  left-hand tree with visibility checkboxes, selection, rename, duplicate,
  reorder, and cascade delete; a right-hand properties panel; CAM ops act on the
  selected object and append CNCJob objects (parented to their source).

### Verified
- `cargo test --workspace`: 319 passed, 0 warnings; `cargo build -p fc-gui` ok.

## [0.12.0] — Phase 7: interactive editors

### Added
- **`fc-editor`** (new crate) — GUI-free, unit-tested editor cores authored by
  4 agents in parallel: `geo_editor` (point/line/rect/circle, select, move,
  delete), `gerber_editor` (pad/track/region), `exc_editor` (drills/slots/array,
  hit-test), `gcode_editor` (line-based find/replace/renumber). 41 tests.
- **GUI editor integration** — start Geo/Gerber/Excellon editors, click-to-add
  primitives, select/delete, accumulate-and-finish paths, live overlay, and
  "Bake → region" so Isolation/Paint/NCC/Cutout run on the edited geometry
  (CAM run methods now use a unified `current_region`).

### Verified
- `cargo test --workspace`: 281 passed, 0 warnings; `cargo build -p fc-gui` ok.

## [0.11.0] — Phase 4: DXF import

### Added
- **`fc-dxf`** (new crate) — DXF → geometry importer built on the verified
  `dxf` 0.6 API: LINE, CIRCLE, ARC (flattened), LWPOLYLINE, POLYLINE (closed →
  polygon, open → polyline). Bulge/splines are a documented v1 limitation. (3 tests)
- **CLI**: `iso`/`paint`/`ncc`/`cutout` now also accept `.dxf` input.

### Verified
- The `dxf` crate API was confirmed against the crate's generated source before
  implementation (Drawing::load, EntityType variants, struct fields).
- `cargo test --workspace`: 240 passed, 0 warnings. 11 crates.

## [0.10.1] — SVG → toolpath pipeline

### Added
- **`fc-cam::isolation_geo`** — geometry-based isolation (works on any region,
  not just a parsed Gerber).
- **CLI**: `iso` / `paint` / `ncc` / `cutout` now accept `.svg` input (SVG art
  treated as the region, mm units), enabling engrave/route of SVG logos.

## [0.10.0] — Second big parallel batch: writers + 8 more features

Authored by 10 agents concurrently (~74 s), integrated with one `mut` cleanup.

### Added
- **`fc-gerber::write_gerber`** — export geometry to RS-274X (regions, with
  clear-polarity holes); round-trips through the parser within 1% area.
- **`fc-excellon::write_excellon`** — export tools/drills/slots; round-trips
  (count + diameters + units preserved).
- **`fc-cam`**: `copper_pour` (ground fill with clearance), `thermal`
  (thermal-relief pads), `teardrops`, `spiral_pocket` (contour-parallel
  pocketing), `scale_fit` (fit-to-size + mm/in conversion), `bridges` (generic
  holding tabs on any polyline).
- **`fc-geo::hatch`** — angled/cross-hatch fill lines clipped to a region.
- **`fc-gcode::dialects_more`** — Smoothie, TinyG, EMC2, GRBL dynamic-laser
  (M4); chained into `dialects::by_name`.

### Verified
- `cargo test --workspace`: 237 passed, 0 warnings. Gerber + Excellon now
  round-trip (parse → write → parse).

## [0.9.0] — Big parallel batch: 12 features at once

Authored by 12 agents concurrently (~82 s, ~429 K agent-tokens) against the
frozen API; integrated with two small fixes (unreachable arm; HPGL mnemonic
length). 10 crates total now.

### Added
- **`fc-geo::geom_utils`** — convex hull, simplify, centroid, point-in-polygon.
- **`fc-cam`**: `iso_multitool` (multi-Ø rest isolation), `ncc_multitool`,
  `drill_to_mill` (helical milling of oversized holes), `textengrave`
  (single-stroke vector font A–Z/0–9), `toolsdb` (tool presets), `drc_extra`
  (annular ring / trace width / hole-to-edge), `gcode_stats` (travel + time
  estimate), `dogbone` (corner relief), `panel_extras` (mouse-bites, v-score).
- **`fc-gcode::dialects_extra`** — Isel, Repetier, Berta, LinuxCNC, Mach3,
  Toolchange-Probe preprocessors, chained into `dialects::by_name`.
- **`fc-hpgl`** (new crate) — HPGL/2 plotter parser (IN/SP/PU/PD/PA/PR,
  absolute + relative).

### Verified
- `cargo test --workspace`: 187 passed, 0 warnings.

## [0.8.1] — GUI: NCC / Cutout / Drilling operations

### Added
- **GUI** now runs NCC, Cutout, and Drilling (multi-tool) from the toolbar, in
  addition to Isolation and Paint, each rendered and exportable to G-code.

## [0.8.0] — Phase 6: project model

### Added
- **`fc-app`** — GUI-free project model (`ObjectCollection` + project
  persistence): ordered named objects with kind/source/options, add/get/remove,
  JSON save/load with a versioned schema. Geometry regenerated from source, not
  serialized. (5 tests)

### Verified
- `cargo test --workspace`: 124 passed, 0 warnings.

## [0.7.0] — Phase 4: SVG import

### Added
- **`fc-svg`** — SVG → geometry importer (`ParseSVG` port): `<path>` (M/L/H/V/
  C/S/Q/T/A/Z with Bézier flattening), `<rect>`/`<circle>`/`<ellipse>`/`<line>`/
  `<polyline>`/`<polygon>`. Closed shapes → polygons, open → polylines. (6 tests)
- **GUI**: loads `.svg` files onto the canvas.

### Verified
- `cargo test --workspace`: 119 passed, 0 warnings; `cargo build -p fc-gui` ok.

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
