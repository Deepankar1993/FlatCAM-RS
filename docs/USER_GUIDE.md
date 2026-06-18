# FlatCAM-RS — User Guide

## What FlatCAM-RS is

FlatCAM-RS is a ground-up **native Rust** reimplementation of FlatCAM Evo's CAM
pipeline — the part that turns PCB design files (Gerber/Excellon/SVG/DXF/…) into
CNC G-code for isolation routing, drilling, area clearing and board cut-out.

It is a **parallel rewrite, not a binding**: it does not import, modify, or depend
on any file in the Python FlatCAM project. The Python application continues to
ship unchanged. FlatCAM-RS targets the parts of FlatCAM that are CPU-bound and
slow under Python's GIL — geometry buffering/union, G-code generation, and
interactive rendering — and rebuilds them on the Rust `geo` stack so they run
fast and memory-lean in a single small native binary.

The engine is split into small, GUI-free library crates (geometry, parsers, CAM
ops, G-code) that are exercised by two front-ends:

| Crate group | Role (FlatCAM analogue) |
|-------------|-------------------------|
| `fc-geo` | geometry foundation: circle, buffer, union/difference, offset (Shapely) |
| `fc-gerber`, `fc-excellon`, `fc-svg`, `fc-dxf`, `fc-hpgl`, `fc-pdf` | file-format parsers (`appParsers/`) |
| `fc-gcode` | CNC job model + preprocessor framework (`preprocessors/`) |
| `fc-cam` | CAM operations: isolation, paint, NCC, cut-out, drilling, panelize, DRC |
| `fc-editor` | interactive Geo/Gerber/Excellon editors (`appEditors/`) |
| `fc-app` | object/project model (`appObjects/`, `ObjectCollection`) |
| `fc-script` | batch/scripting engine (`tclCommands/`) — see `SCRIPTING.md` |
| `fc-cli` | headless CLI binary `flatcam-rs` |
| `fc-gui` | native desktop app binary `flatcam-gui` (eframe/egui) |

## Building

Requires a stable Rust toolchain (`cargo`).

```sh
cd flatcam-rs
cargo build --release      # builds the whole workspace, optimized
cargo test --workspace     # run the unit/integration test suite
```

The release profile is tuned for speed (`opt-level=3`, thin LTO, single codegen
unit). Binaries land in `target/release/`:

- `flatcam-rs` — the headless CLI
- `flatcam-gui` — the desktop application

You can also run a binary directly without installing:

```sh
cargo run --release -p fc-cli -- info examples/two_pads.gbr
cargo run --release -p fc-gui
```

## The two binaries

- **`flatcam-rs`** (from `fc-cli`) — a GUI-free front-end for batch processing,
  scripting, and verifying the engine end-to-end. Reads a source file, runs one
  CAM operation, and writes G-code to a file or stdout.
- **`flatcam-gui`** (from `fc-gui`) — the interactive desktop app: open files
  into a project tree, run CAM ops on the selected object, view tool-paths on a
  pan/zoom canvas, edit geometry, and save G-code or the project.

## CLI commands

General form:

```
flatcam-rs <command> <input-file> [options]
```

If no `-o` is given, G-code is written to stdout (status text goes to stderr).

| Command | Purpose |
|---------|---------|
| `info`   | Parse a file and print statistics (units, apertures/tools, polygon/drill counts, bounds) |
| `iso`    | Isolation-route a Gerber/SVG/DXF/PDF region to G-code |
| `paint`  | Area-fill (pocket) the copper regions |
| `ncc`    | Non-copper clear — clear all non-copper area inside the boundary |
| `cutout` | Mill the board outline (bounding box) with holding tabs |
| `drill`  | Drill an Excellon file, one block per tool |
| `script` | Run a batch script (see `SCRIPTING.md`) |

### Common flags

| Flag | Applies to | Meaning |
|------|-----------|---------|
| `-o <path>` | all CAM cmds | output G-code file (default: stdout) |
| `--tool-dia <f>` | iso/paint/ncc/cutout | tool diameter |
| `--passes <n>` | iso | number of isolation passes |
| `--overlap <f>` | iso/paint/ncc | pass overlap fraction (0..1) |
| `--margin <f>` | paint/ncc | extra clearance / boundary margin |
| `--tabs <n>` | cutout | number of holding tabs |
| `--tab-gap <f>` | cutout | tab width |
| `--cut-z <f>` | all | cut depth (negative) |
| `--travel-z <f>` | all | clearance/travel height |
| `--depth-per-pass <f>` | all | Z step per pass |
| `--feed-xy <f>` | all | cutting feedrate |
| `--feed-z <f>` | all | plunge feedrate |
| `--rpm <f>` | all | spindle speed |
| `--preproc <name>` | all | G-code dialect (default: `grbl`) |

### Examples

```sh
# Inspect a file
flatcam-rs info examples/two_pads.gbr
flatcam-rs info examples/two_pads.drl

# Isolation routing: 2 passes, 0.4 mm V-bit, 25% overlap, GRBL G-code
flatcam-rs iso examples/two_pads.gbr --tool-dia 0.4 --passes 2 --overlap 0.25 \
               --cut-z -0.05 -o board_iso.gcode

# Area paint a copper region with a 0.5 mm bit, 30% overlap
flatcam-rs paint examples/two_pads.gbr --tool-dia 0.5 --overlap 0.3 -o paint.gcode

# Non-copper clear with a 1 mm boundary margin
flatcam-rs ncc examples/two_pads.gbr --tool-dia 0.6 --margin 1.0 -o ncc.gcode

# Cut the board out with 6 tabs using a 1.0 mm end mill
flatcam-rs cutout examples/two_pads.gbr --tool-dia 1.0 --tabs 6 -o cutout.gcode

# Drill an Excellon file, 1.8 mm depth, Marlin dialect
flatcam-rs drill examples/two_pads.drl --cut-z -1.8 --preproc marlin -o drill.gcode

# Run a batch script
flatcam-rs script jobs/board.fcs
```

## Supported input formats

| Format | Extensions | Parser crate | Notes |
|--------|-----------|--------------|-------|
| Gerber (RS-274X) | `.gbr` (default) | `fc-gerber` | copper / solid geometry, carries its own units |
| Excellon | `.drl`, `.nc`, `.xln`, `.exc`, `.txt` | `fc-excellon` | tools, drills and slots |
| SVG | `.svg` | `fc-svg` | treated as millimetres |
| DXF | `.dxf` | `fc-dxf` | treated as millimetres |
| HPGL / HPGL-2 | — | `fc-hpgl` | plotter-language parser |
| PDF | `.pdf` | `fc-pdf` | vector outlines, treated as millimetres |

In the CLI, `iso`/`paint`/`ncc`/`cutout` accept Gerber, SVG, DXF and PDF region
sources; `drill` accepts Excellon. SVG/DXF/PDF have no document units and are
read as millimetres.

## G-code dialects (preprocessors)

Selected with `--preproc <name>` on the CLI (and the preprocessor dropdown in the
GUI). Each is a self-registering preprocessor in `fc-gcode`. The dialects
reachable through the standard name resolver are:

`grbl`, `marlin`, `default` (a.k.a. `generic`), `grbl_no_m6`,
`grbl_laser` (a.k.a. `laser`), `roland` (a.k.a. `roland_mdx`),
`smoothie`, `tinyg`, `emc2`, `grbl_m4` (GRBL dynamic-laser M4),
`isel`, `repetier`, `berta`, `linuxcnc`, `mach3`, `toolchange_probe`,
`solderpaste` (a.k.a. `paste`), and `toolchange_manual` (a.k.a. `manual`).

Names are matched case-insensitively; an unknown name falls back to GRBL.
Additional laser dialects (air-assist, Marlin fan/spindle) exist in the codebase
and are being wired into the resolver.

## GUI walkthrough (`flatcam-gui`)

1. **Open a file.** Click **Open…** and pick a Gerber/Excellon/SVG/DXF/PDF file
   (or pass a path on the command line: `flatcam-gui examples/two_pads.gbr`).
   The file appears as an object in the **Project** tree on the left.
2. **The project tree.** Each loaded file and CAM result is an object with a
   visibility checkbox and a kind icon. Use **Up/Down** to reorder, **Dup** to
   duplicate, **Del** to delete, and the rename field to rename. Selecting an
   object shows its details in the **Properties** panel on the right.
3. **Select an object** in the tree. CAM operations act on the current selection.
4. **Set parameters** in the left panel: Tool Ø, Passes, Overlap, Lead in/out,
   and the preprocessor dropdown.
5. **Run a CAM op** from the toolbar: **Isolation**, **Paint**, **NCC**,
   **Cutout** (act on a Gerber/Geometry object) or **Drilling** (acts on an
   Excellon object).
6. **Results appear as CNCJob objects** added under the source object in the tree,
   with their tool-paths overlaid on the canvas. The source stays selected so you
   can chain operations.
7. **Save G-code.** Click **Save G-code…** to write the most recent job's G-code
   to disk.
8. **Canvas.** Drag to pan, scroll to zoom; the view auto-fits when objects
   change. The selected object is drawn with a thicker outline.
9. **Fill toggle.** The **Fill** checkbox shades region objects (triangulated)
   so copper areas read as solid rather than as outlines.
10. **Editors.** Open the **Editor** section to start a **Geo**, **Gerber**, or
    **Excellon** editor. Pick a tool (Point/Rect/Circle/Line, Pad/Track, or
    Drill), click on the canvas to add features, **Finish path** to close a
    polyline, **Delete sel** to remove a selection, then **Bake → object** to
    turn the edit into a project Geometry object (or **Close** to discard).
11. **Open/Save Project.** **Save Project** writes the project tree to a `.json`
    file; **Open Project** reloads it and regenerates geometry from each object's
    source file. Objects whose source file is missing are reported in the status
    bar.
