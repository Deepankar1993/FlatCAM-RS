# FlatCAM-RS — Design & Architecture

This document records the design of the Rust port of FlatCAM Evo and the
mapping from the Python architecture to the Rust crate structure.

## Goals

1. **Performance.** Eliminate the sluggishness of the Python/PyQt6 app on large
   boards. The CPU-bound work — geometry buffering, boolean unions, offsetting,
   and G-code generation — must be fast and parallelizable (no GIL).
2. **Correctness & parity.** Reproduce FlatCAM's CAM results: the same Gerber
   interpretation, the same drill decoding, equivalent isolation/drill G-code.
3. **Isolation from Python.** The port lives entirely in `flatcam-rs/` and never
   modifies or imports the Python codebase.
4. **Testability without a GUI.** Every compute crate is GUI-free and unit
   tested; the `fc-cli` binary exercises the whole pipeline headlessly.

## Layer mapping: Python → Rust

| FlatCAM Python | Responsibility | FlatCAM-RS crate |
|----------------|----------------|------------------|
| `camlib.py` (`Geometry`, Shapely usage) | geometry algorithms | `fc-geo` |
| `appParsers/ParseGerber.py` | RS-274X parsing | `fc-gerber` |
| `appParsers/ParseExcellon.py` | Excellon parsing | `fc-excellon` |
| `camlib.py` (`CNCjob`) + `preprocessors/` | job model + G-code dialects | `fc-gcode` |
| `appPlugins/ToolIsolation`, `ToolDrilling` | CAM operations | `fc-cam` |
| `flatcam.py` / Tcl shell (headless paths) | CLI entry | `fc-cli` |
| `appMain.py`, `appGUI/`, `appEditors/` | app shell + GUI | *future* `fc-app`, `fc-gui` |

## The geometry foundation (`fc-geo`)

FlatCAM leans on Shapely for nearly everything. The Rust equivalent is built on:

- **`geo`** — primitive types (`Coord`, `LineString`, `Polygon`, `MultiPolygon`)
  and boolean ops (`BooleanOps`: union/difference/intersection, backed by the
  `i_overlay` exact-arithmetic engine). This replaces Shapely `unary_union`,
  `.union()`, `.difference()`.
- **`geo-buffer`** — straight-skeleton polygon offsetting. This replaces
  Shapely `Polygon.buffer(±d)`, which is how isolation passes are derived.

`fc-geo` exposes a small, intentional surface:

| Function | Shapely analogue | Used for |
|----------|------------------|----------|
| `circle`, `regular_polygon`, `centered_rect`, `obround` | `Point.buffer`, box, hull | flash geometry |
| `buffer_path` | `LineString.buffer(r)` round caps | traces & slots |
| `union_all`, `union`, `difference` | `unary_union`, set ops | merging flashes, clear polarity |
| `offset` | `Polygon.buffer(±d)` | isolation passes |

### A subtle correctness fix

Boolean-op output from `i_overlay` does **not** guarantee the CCW-exterior /
CW-interior winding convention. `geo-buffer` is orientation-sensitive: a CW
exterior ring is treated as a hole and offset the wrong way, collapsing to
nothing. `fc-geo::offset` therefore normalizes orientation
(`geo::Orient`) before buffering. This was found by a failing unit test where
isolation of a parsed (post-union) pad produced an empty tool path — exactly the
kind of silent geometry bug a test suite is meant to catch.

## Gerber parsing (`fc-gerber`)

A streaming tokenizer splits the file into `%…%` extended-command blocks and
`*`-terminated function words, dispatched through a state machine that mirrors
`ParseGerber.py`:

- **Format/units:** FS (int/frac digits, L/T/D zero suppression), MO (IN/MM),
  G70/G71.
- **Apertures (AD):** C (circle), R (rect), O (obround), P (regular polygon),
  and macro references.
- **Aperture macros (AM):** primitives circle(1), vector line(2/20), centre
  line(21), lower-left line(22), outline(4), polygon(5), **including** the macro
  arithmetic mini-language (`$n`, `+ - x /`, parentheses) that real macros use.
- **Drawing:** D01 (draw), D02 (move), D03 (flash); G01 linear and G02/G03
  circular interpolation with both multi-quadrant (G75) and single-quadrant
  (G74) arc center resolution; G36/G37 region fill; LP D/C polarity (dark
  accumulates, clear is subtracted via `difference`).

Output: a single `MultiPolygon` (union of dark minus clear) plus the centre-line
"follow" geometry, matching FlatCAM's `solid_geometry` / `follow_geometry`.

Coordinate decoding implements the exact FlatCAM rule:
`value = int(digits) / 10^frac` for leading/no suppression, and the
pad-trailing-zeros variant for trailing suppression.

## Excellon parsing (`fc-excellon`)

Header/body state machine handling `M48…%`/`M95`, `INCH`/`METRIC` with `LZ`/`TZ`
and inline format, `M71`/`M72`, tool definitions (`Tnn C…`), tool selection,
plain drill hits, `G85` slots, and `G00/G01` routed slots. The trickiest part —
coordinate decoding under leading vs trailing zero suppression — follows
`ParseExcellon.parse_number()` precisely, with unit inference from the tool
diameter distribution when the header omits units. Geometry is built lazily:
drills → buffered circles, slots → round-capped buffered segments.

## CNC job & preprocessors (`fc-gcode`)

`CncJob` is a dialect-independent description of motion: either `Mill { paths }`
(polylines cut at depth, with optional multi-pass plunging) or
`Drill { points }`. A `Preprocessor` trait renders it to a concrete G-code
dialect; `Grbl` and `Marlin` ship. This mirrors how `preprocessors/*.py`
subclass `PreProc` and implement `start_code`/`linear_code`/`end_code`.

## CAM operations (`fc-cam`)

- **Isolation:** for pass *i*, offset the copper `MultiPolygon` outward by
  `tool_radius + i·tool_dia·(1−overlap)` and take the resulting ring boundaries
  as cut polylines.
- **Drilling:** map an Excellon tool's drill points to a `Drill` job, carrying
  the tool diameter and units through.

## Testing strategy

22 tests today, all GUI-free and deterministic:

- `fc-geo`: area of primitives, round-cap buffer area, union merging, offset
  inflation, difference (hole punching), circle-offset regression.
- `fc-gerber`: flashes, traces, rectangle flash, region fill, macro arithmetic,
  parameterized macros.
- `fc-excellon`: metric drills, leading/trailing zero decoding, G85 slots, drill
  geometry area.
- `fc-gcode`: multi-pass depth computation, GRBL mill/drill structure.
- `fc-cam`: isolation ring count (single & multipass), G-code rendering, drill
  job from Excellon.

The `fc-cli` binary additionally validates the end-to-end pipeline against the
`examples/` fixtures.

## Release profile

`opt-level = 3`, thin LTO, `codegen-units = 1`, `panic = "abort"` — a 700 KB
binary that performs a 2-pass isolation of the sample board in ~68 ms.
