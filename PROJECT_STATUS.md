# FlatCAM-RS — Project Status & Development Progress

_Last updated: 2026-06-19_

A ground-up **Rust** reimplementation of [FlatCAM Evo](https://github.com/Deepankar1993/FlatCAM-Plus)
(PyQt6 + Shapely) — a PCB CAM application that turns Gerber/Excellon/SVG/DXF/PDF
into CNC G-code (isolation routing, drilling, paint/clear, cutout, and more).
The goal of the port is speed and memory-leanness: the Python engine is CPU-bound
on large boards (geometry buffering/union, rendering, G-code gen) and fights the
GIL. This rebuild keeps the Python original untouched and reimplements the engine
+ a native desktop GUI in Rust.

## Snapshot

| | |
|---|---|
| **Language / toolchain** | Rust 2021, Cargo workspace |
| **Crates** | 17 (`crates/*`) |
| **Tests** | ~819 passing across the workspace, 0 warnings (last full run) |
| **GUI** | Native desktop app on `eframe`/`egui` 0.29 — working |
| **CLI** | Headless `flatcam-rs` binary — working |
| **Validation** | Real KiCad board (Gerber X2 + Excellon) parses and machines correctly |
| **Status** | CAM engine complete & hardened; GUI at stock-FlatCAM feature parity for menus/plugins; remaining work is GUI depth + niche ops |

## Architecture — crate → original Python layer

| Crate | Replaces / mirrors | Notes |
|-------|--------------------|-------|
| `fc-geo` | `camlib` Shapely usage | `geo` + `geo-buffer`. Offset is orientation-normalized (i_overlay can emit CW); affine transforms (translate/rotate/scale/skew/mirror); triangulation (earcut); hull/simplify/centroid/contains; hatch; balanced O(n log n) `union_all`. |
| `fc-gerber` | `ParseGerber.py` | RS-274X + aperture macros (arithmetic evaluator), thermal/moiré primitives, X2/X3 attributes, Gerber writer (round-trips). |
| `fc-excellon` | `ParseExcellon.py` | Zero-suppression decoding, slots, Excellon writer, PcbWizard (.INF/.DRL) import. `Excellon` is `Clone`. |
| `fc-gcode` | `camlib` CNCjob + `preprocessors/` | `Preprocessor` trait; ~34 dialects (GRBL/Marlin/Smoothie/TinyG/EMC2/Roland MDX-20/540/ISEL+ICP/NCCAD9/Line-xyz/Check-points/HPGL/laser variants incl. GRBL-z, Marlin-z, default-laser, eleks-drd/SolderPaste Paste_1·GRBL·Marlin…); G-code reader; collinear optimizer; stats. |
| `fc-cam` | `appPlugins/*` (Tool*) | Isolation, drilling, paint/infill, NCC, cutout+tabs/bridges, panelize, double-sided, transforms, milling, sub, etch, invert, drill-optimize, fiducials (circular/cross/chess), follow, solderpaste, thieving (dots/squares/lines/solid + robber bar), punch-Gerber, extract-drills, corner markers, rules-check (DRC), levelling, teardrops, copper-pour, spiral pocket, scale-fit, dogbone, TSP path order, text engrave, tools DB. |
| `fc-laser` | _(new — not in original)_ | Diode-laser beam-shape compensation: anisotropic kerf/power model, astigmatic Z-beam, calibration grids, power-curve LUT, cross-hatch/raster fill, burn simulation. See `docs/LASER_NOTES.md`. |
| `fc-svg` / `fc-dxf` / `fc-pdf` / `fc-hpgl` | SVG/DXF/PDF/HPGL2 importers | Vector import → geometry with **ring-nesting** (holes reconstructed); SVG/DXF/PDF now also **export** (writers, round-trip tested). |
| `fc-image` | _(new)_ `ToolImage` | Raster import: decode BMP/PNG/JPG, threshold, merge ink pixels into geometry (`trace_bytes`/`trace_file`). |
| `fc-qr` | _(new)_ | QR code → geometry. |
| `fc-app` | object model / project | `Project` tree (visibility/parent/select/rename/dup/reorder/cascade-delete), `Preferences`, JSON save/load. |
| `fc-script` | Tcl shell | Headless batch/scripting engine (~73 commands: io/cam/geo/query/transform/analyze/edit + cncjob, skew, mirror, buffer, follow, ncr, exteriors/interiors, new_geometry/gerber, set_origin, version/help/list_pp, export_gerber/excellon/svg/dxf, write_gcode, save_project, open_gcode, add_circle/poly/polyline/rect/drill/slot, subtract_poly/rect, geo_union, milldrills/millslots, join_geometries/excellon). |
| `fc-editor` | `appEditors/*` | GUI-free editor cores (geo/gerber/excellon/gcode). |
| `fc-gui` | `appGUI/*` | Native desktop app (binary `flatcam-gui`) + headless `screenshot` binary. |
| `fc-cli` | _(new)_ | Headless `flatcam-rs` binary. |

## Feature coverage

**Parsing/import:** Gerber (RS-274X, macros, X2), Excellon (slots, zero-suppression),
SVG, DXF, PDF (vector), HPGL2.

**CAM operations:** isolation (single + multi-tool), drilling, paint/infill, NCC
(non-copper clear), cutout with tabs/bridges, panelize, double-sided alignment,
transforms (rotate/skew/scale/mirror/offset), milling (drill→mill, slots), etch,
invert, fiducials (circular/cross/chess), follow, solderpaste dispensing,
thieving (dots/squares/lines/solid + robber bar + plating mask)/copper-pour,
punch-Gerber, extract-drills, corner markers, paint (lines/contour/seed),
cutout (rect/outline/freeform), PCB calculators (V-bit, electroplating, track
resistance, IPC-2221 current/width), teardrops, dogbone/T-bone, DRC rules-check,
bed-leveling (IDW), TSP drill ordering, text engrave, raster image trace.

**G-code:** ~30 preprocessor dialects, lead-in/out, ramps, collinear optimization,
renumbering, stats; G-code reader for re-import.

**Laser (beyond the original):** anisotropic beam compensation, astigmatic focus
model, direction/power/focus calibration grids, isotonic power curve, cross-hatch
and banded raster fill, burn-uniformity simulation. Validated on real KiCad B_Cu.

**Scripting:** headless engine with ~84 commands (io/cam/geo/query/transform/
analyze/edit/build/sys: open/export every format, object construction, milling,
joins, splits, system vars, project save/load); CLI `script <file>`.

**More IO:** SVG/DXF/PDF **export** (hand-rolled writers), raster image **import**
(`fc-image`), project save/load with optional **LZMA** compression + auto-detect.

## GUI status (`fc-gui`)

- **Canvas:** pan/zoom, adaptive 1-2-5 grid, origin axes, margin-gutter rulers,
  cursor crosshair + coordinate HUD, layer-ordered opaque rendering (copper green,
  edge red dashed, drills orange crosshairs), per-object colors, fill toggle,
  workspace outline, dark default theme (light available).
- **Layout:** menu bar, grouped icon toolbar, Project/Properties/Plugin left
  notebook, Plot Area / G-code center tabs, status bar (snap/units/idle), G-code
  viewer window, tabbed Preferences dialog, Help/About/Shortcuts windows.
- **Menus (stock-FlatCAM parity):** full File/Edit/View/Options/Objects/Help menus
  matching the original's labels, shortcuts, order and submenus, plus keyboard
  accelerators. Includes view toggles (axis/grid/HUD/workspace/notebook/plot-area/
  fullscreen), numeric dialogs (Num Move / Jump / Custom Origin / Rotate / Skew),
  conversions, Error Log & View Source windows, a project right-click context menu
  (Enable/Disable Plot, Set Color, View Source, Edit, Copy, Delete, Save,
  Properties) on both tree and canvas, and a contextual Geo/Gerber/Excellon editor
  menu while editing.
- **Plugins:** 31 tools in the Plugins menu (19 wired to real `fc_cam` ops, the
  rest honest disabled stubs), each with a generic parameter form + Apply.
- **Multi-object selection:** Ctrl/Cmd+click toggles objects in/out of the
  selection, plain click replaces, Select All / Deselect All and Ctrl+A drive the
  real multi-set; all selected objects highlight in tree and canvas.
- **Import/Export wired:** File ▸ Import ▸ Image (raster→geometry via `fc-image`),
  File ▸ Export ▸ SVG / DXF / PNG / **Print (PDF)** write real files for the
  selection; **Edit ▸ Join** unions selected objects; **Tools Database** window
  lists the `fc_cam::toolsdb` presets.
- **Context menu:** Delete / Copy / Enable-Disable Plot act over the whole
  multi-selection when the clicked object is part of it.
- **Laser panel:** beam editor, astigmatism/focus controls, polar plot, burn
  heatmap overlay, fill/raster ops.

**Known GUI limitations:** the in-app scripting shell is still stubbed; the Tools
Database window is read-only (no add/edit yet); Join always produces a unioned
Geometry object (no kind-preserving Excellon→Excellon / Gerber→Gerber join);
interactive editors are basic (tool placement + bake). The GUI needs a display —
verify with `cargo run --release -p fc-gui` or render a PNG via the `screenshot`
binary.

## Testing & validation

- ~604 unit/integration tests across the workspace (`cargo test --workspace`), 0 warnings.
- Adversarial 6-agent correctness review fixed real parser/CAM edge-case bugs.
- Validated against a real KiCad board (SmartPowerMonitor: B_Cu/F_Cu/Mask/Edge/Silk
  Gerbers + PTH/NPTH Excellon): all layers parse (X2/macros/ground-pour/slots), CAM
  toolpaths match the source, and `--mirror --origin` output matches the FlatCAM
  reference G-code coordinate space.

## Build & run

```sh
# Build everything (release recommended — debug geometry ops are 10–50× slower)
cargo build --release

# Run the desktop GUI (needs a display)
cargo run --release -p fc-gui

# Headless CLI
cargo run --release -p fc-cli -- --help        # info / iso / paint / ncc / cutout / drill / laser-* / script

# Render a GUI screenshot to PNG (real eframe window, self-closing)
cargo run --release -p fc-gui --bin screenshot -- out.png [files...] [--dark|--light]

# Tests
cargo test --workspace
```

## Roadmap / next steps

See `docs/ROADMAP.md` and `docs/LASER_NOTES.md`. Short list:

- GUI depth: true multi-object selection; wire the remaining File-menu items to
  the now-available library backends (SVG/DXF export writers exist in
  `fc-svg`/`fc-dxf`; still need GUI hookup for PNG export, object join, tools
  database, print-to-PDF); richer interactive editors.
- Custom color picker + per-object opacity in the context menu.
- Laser: connected raster path, hardware calibration loop (model is complete).
- More preprocessor dialects as needed.

## Development model

Built largely via an orchestrated multi-agent workflow: a frozen modular contract
(`docs/AGENT_GUIDE.md`) lets agents each add **one new module file + tests** without
touching shared files; the orchestrator wires `pub mod` lines and runs a single
`cargo test --workspace` gate. New features land as isolated modules to keep the
parallelism conflict-free.
