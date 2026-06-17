# Changelog — FlatCAM-RS

All notable changes to the Rust port are recorded here.

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
