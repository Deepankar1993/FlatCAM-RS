# FlatCAM-RS Scripting Reference (`fc-script`)

`fc-script` is the headless batch language for FlatCAM-RS — the parity equivalent
of FlatCAM's Tcl shell (`tclCommands/`). A script is a plain-text file of
commands, one per line, operating on a **script context**: a named collection of
objects. Three object kinds live in the context:

- **region** — 2-D geometry (a `MultiPolygon`) with units, from a Gerber/SVG/DXF
  file or a generator/transform command.
- **excellon** — a drill program (tools + drill points), from `open_excellon` or
  `drill_array`.
- **cnc** — a finished CNC job (tool-paths + rendered G-code), produced by the
  CAM commands. CNC G-code is rendered with the GRBL dialect.

## Running a script

```sh
flatcam-rs script <file.fcs>
```

Each command's output line is printed to stdout; a summary
(`script ok: N objects in context`) goes to stderr. Any error aborts the run.

## Syntax

- One command per line; arguments are whitespace-separated tokens.
- Blank lines are ignored.
- `#` begins a comment — a line whose first non-space character is `#` is skipped.
- Numbers are plain decimals (e.g. `0.4`, `-1.8`, `90`). Distances/coordinates are
  in the object's own units; generated objects (`new_rect`, `new_circle`,
  `drill_array`, `fiducials`, `thermal`) are in millimetres.
- `<...>` arguments are required; `[...]` are optional with defaults.

## Command reference

### `io` — load source files / export G-code

| Command | Signature | Result |
|---------|-----------|--------|
| `open_gerber` | `open_gerber <path> <name>` | parse a Gerber file → region |
| `open_excellon` | `open_excellon <path> <name>` | parse an Excellon file → excellon |
| `open_svg` | `open_svg <path> <name>` | parse an SVG file → region (mm) |
| `open_dxf` | `open_dxf <path> <name>` | parse a DXF file → region (mm) |
| `export_gcode` | `export_gcode <name> <path>` | write a cnc object's G-code to disk |

### `cam` — CAM operations (produce cnc objects)

| Command | Signature | Result |
|---------|-----------|--------|
| `isolate` | `isolate <src> <dst> <tool_dia> [passes] [overlap]` | isolation tool-paths (default passes=1, overlap=0) |
| `paint` | `paint <src> <dst> <tool_dia> [overlap]` | area-fill (pocket) tool-paths |
| `ncc` | `ncc <src> <dst> <tool_dia> [overlap]` | non-copper-clear tool-paths |
| `cutout` | `cutout <src> <dst> <tool_dia> [tabs]` | rectangular board cut-out with tabs (default tabs=4) |
| `drill` | `drill <src> <dst>` | drilling job from an excellon source |

`isolate`/`paint`/`ncc`/`cutout` read a **region** `src`; `drill` reads an
**excellon** `src`. All store a **cnc** object under `dst`.

### `geo_ops` — geometry transforms & booleans (produce region objects)

| Command | Signature | Result |
|---------|-----------|--------|
| `offset` | `offset <src> <dst> <distance>` | buffer/offset by distance (negative shrinks) |
| `scale` | `scale <src> <dst> <factor>` | scale about the origin |
| `translate` | `translate <src> <dst> <dx> <dy>` | move by (dx, dy) |
| `mirror_x` | `mirror_x <src> <dst> <axis>` | mirror across the horizontal line y = axis |
| `subtract` | `subtract <a> <b> <dst>` | boolean difference a − b |
| `union` | `union <a> <b> <dst>` | boolean union a ∪ b |

Results inherit the source's units.

### `query` — introspection & object management

| Command | Signature | Result |
|---------|-----------|--------|
| `list` | `list` | one `name:kind` line per object |
| `bounds` | `bounds <name>` | region bounding box as `minx miny maxx maxy` |
| `area` | `area <name>` | unsigned area of a region |
| `count` | `count <name>` | element count (region→polygons, excellon→drills, cnc→paths) |
| `delete` | `delete <name>` | remove an object |
| `rename` | `rename <old> <new>` | rename an object |

### `gen` — create new objects from scratch (millimetres)

| Command | Signature | Result |
|---------|-----------|--------|
| `new_rect` | `new_rect <name> <x> <y> <w> <h>` | rectangle with lower-left corner at (x, y) → region |
| `new_circle` | `new_circle <name> <cx> <cy> <r>` | filled circle centred at (cx, cy) → region |
| `drill_array` | `drill_array <name> <ox> <oy> <dx> <dy> <nx> <ny> <dia>` | nx×ny grid of drills from (ox, oy), pitch (dx, dy) → excellon |

### `transform` — array / panelize / rotate / mirror (produce region objects)

| Command | Signature | Result |
|---------|-----------|--------|
| `rotate` | `rotate <src> <dst> <deg>` | rotate about the origin |
| `mirror_y` | `mirror_y <src> <dst> <axis>` | mirror across the vertical line x = axis |
| `array` | `array <src> <dst> <dx> <dy> <n>` | linear array of n copies stepped by (dx, dy) |
| `panelize` | `panelize <src> <dst> <nx> <ny> <gutter>` | nx×ny panel; pitch = source size + gutter |

### `analyze` — design-rule / reporting (read-only, return text)

| Command | Signature | Result |
|---------|-----------|--------|
| `drc` | `drc <name> <min_clearance>` | `DRC pass` or `DRC FAIL` for the clearance rule |
| `min_spacing` | `min_spacing <name>` | smallest gap between distinct features |
| `report` | `report <name>` | `polygons … area … width … height …` summary |

### `edit` — board-editing tools (produce region objects)

| Command | Signature | Result |
|---------|-----------|--------|
| `etch` | `etch <src> <dst> <factor>` | etch-compensation: widen copper to counter undercut |
| `copper_pour` | `copper_pour <src> <dst> <clearance>` | flood-fill copper around tracks, keeping clearance |
| `fiducials` | `fiducials <src> <dst> <margin> <dia>` | corner fiducial dots inset `margin` from the bounds (mm) |
| `thermal` | `thermal <dst> <cx> <cy> <pad_dia> <hole_dia> <gap> <spokes>` | one thermal-relief pad (mm) |

## Worked example

Build a 50 × 30 mm board, punch a mounting hole, isolation-route it, and export
the G-code:

```sh
# board.fcs — build a board, subtract a hole, isolate, export

# 1. Build a 50 x 30 mm board (lower-left at origin)
new_rect board 0 0 50 30

# 2. Make a 6 mm mounting hole near the lower-left and cut it out of the board
new_circle hole 8 8 3
subtract board hole board_drilled

# 3. Inspect what we have
list
area board_drilled
bounds board_drilled

# 4. Isolation-route the result: 0.4 mm tool, 2 passes, 15% overlap
isolate board_drilled board_iso 0.4 2 0.15

# 5. Export the G-code
export_gcode board_iso board_iso.gcode
```

Run it with:

```sh
flatcam-rs script board.fcs
```
