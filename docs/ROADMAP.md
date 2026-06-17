# FlatCAM-RS — Porting Roadmap

The Python FlatCAM Evo is ~150 K LOC of application code (GUI 28 K, plugins
46 K, editors 24 K, CAM engine + parsers 14 K). A faithful full port is a large,
multi-phase effort. This roadmap sequences it so that the **performance-critical
CAM engine lands first** (done), and the GUI — the largest but least
compute-heavy layer — comes last on a stable, tested foundation.

Legend: ✅ done · 🔶 in progress · ⬜ planned.

---

## Phase 1 — CAM engine core ✅ (this milestone)

The headless, GUI-free pipeline that produces correct G-code.

- ✅ `fc-geo`: geometry foundation (geo + geo-buffer), 6 tests
- ✅ `fc-gerber`: RS-274X parser incl. aperture macros + arithmetic, 7 tests
- ✅ `fc-excellon`: drill/route parser incl. zero-suppression decoding, 5 tests
- ✅ `fc-gcode`: CNC job model + GRBL/Marlin preprocessors, 4 tests
- ✅ `fc-cam`: isolation routing + drilling, 4 tests
- ✅ `fc-cli`: `info` / `iso` / `drill` headless commands
- ✅ release build (700 KB), end-to-end verified on sample board

## Phase 2 — Core CAM completeness ⬜

Round out the compute that most boards need (Group A from the plugin audit).

| Work | Python source | Notes |
|------|---------------|-------|
| ✅ Paint / flood-fill | `ToolPaint` | scanline line-fill + contour, even-odd hole handling (`fc-cam::paint`, CLI `paint`) |
| ✅ NCC (non-copper clear) | `ToolNCC` | board-rect minus copper, paint-filled (`fc-cam::ncc`, CLI `ncc`) |
| ✅ Cutout / board outline | `ToolCutOut` | outline mill + holding tabs (`fc-cam::cutout`, CLI `cutout`) |
| ✅ Milling (general) | `ToolMilling` | profile + pocket milling (`fc-cam::milling`) |
| ✅ Geometry boolean Sub | `ToolSub` | difference of geometries (`fc-cam::sub`) |
| ✅ Follow (trace centre-line) | `ToolFollow` | `fc-cam::follow` (engrave along `follow_geometry`) |
| ✅ Copper thieving | `ToolCopperThieving` | dot-grid fill kept clear of copper (`fc-cam::thieving`) |
| ✅ Laser paths (preproc) | `ToolLaser` | mill paths + `grbl_laser` dialect (engrave UI pending) |
| ⬜ Multi-depth, tabs, rest-machining | isolation/cutout | toolpath refinements |
| ⬜ Infill primitives in `fc-geo` | — | line-fill and contour-fill scanlines |

## Phase 3 — Geometry transforms & utilities ⬜

Group B/C tools — moderate compute, no new subsystems.

- ✅ Transform primitives (translate/rotate/scale/skew/mirror) — `fc-geo::transform`
- ✅ Panelize (array of boards) — `fc-cam::panelize`
- ✅ Double-sided mirror/flip — `fc-cam::panelize::mirror_for_bottom`
- ✅ Etch compensation — `fc-cam::etch`
- ✅ Invert Gerber — `fc-cam::invert`
- ✅ Fiducials / markers — `fc-cam::fiducials`
- ✅ Drill path optimization (nearest-neighbor) — `fc-cam::drilloptim`
- ⬜ Film / negative export — `ToolFilm`
- ✅ Rules check (DRC, min-clearance) — `fc-cam::rulescheck`
- ✅ Bed levelling probe grid — `fc-cam::levelling`
- ✅ Solder-paste dispense paths — `fc-cam::solderpaste`
- ⬜ Calculators, distance, optimal, QR code — small utilities

## Phase 4 — Additional parsers ⬜

| Parser | Python | Complexity | Candidate Rust crate |
|--------|--------|-----------|----------------------|
| ⬜ SVG | `ParseSVG` | MED | `usvg` / `svgtypes` |
| ⬜ Font → glyph polygons | `ParseFont` | MED | `ttf-parser` + outline flattening |
| ⬜ HPGL2 | `ParseHPGL2` | MED | custom (small command set) |
| ⬜ DXF (+ splines) | `ParseDXF` | HIGH | `dxf` crate; spline tessellation |
| ⬜ PDF vector extract | `ParsePDF` | HIGH | `pdfium-render` / `lopdf` (lowest priority) |

## Phase 5 — Preprocessor coverage ⬜

The Python project ships ~28 G-code dialects. GRBL + Marlin exist; port the
common remainder behind the existing `Preprocessor` trait.

- ⬜ `default` / `Default_no_M6` (generic + MACH3-style)
- ⬜ GRBL variants: `GRBL_11`, `GRBL_11_no_M6`, laser variants (z / air-assist)
- ⬜ Marlin laser variants (FAN pin / Spindle pin)
- ⬜ Roland MDX-20 / MDX-540
- ⬜ ISEL CNC / ICP, Repetier, Berta, NCCAD9, Line_xyz
- ⬜ Toolchange manual / probe (MACH3), solder-paste dispensing dialects

## Phase 6 — Application shell (headless project model) ⬜

- ⬜ `fc-app`: object collection, project (open/save), defaults/options
  (`LoudDict` analogue), units handling, object kinds (gerber/excellon/geometry/
  cncjob). GUI-free so it stays testable.
- ⬜ Tcl-style or new scripting/batch interface (parity with `tclCommands/`).
- ⬜ Project file format (load/save `.FlatPrj` or a new format).

## Phase 7 — GUI ⬜ (largest surface, ~28 K LOC PyQt6)

Decision required before starting:

| Toolkit | Pros | Cons |
|---------|------|------|
| **egui/eframe** (recommended for MVP) | immediate-mode, fast iteration, easy canvas, wgpu | fewer native widgets, custom polish needed |
| **slint** | declarative (Qt-like), compiled, pixel-perfect | DSL learning curve, custom rendering harder |
| **iced** | Elm architecture, idiomatic Rust | heavier, slower iteration |

Sub-work:
- ⬜ 2D/3D plot canvas (replaces VisPy/OpenGL `PlotCanvas` + matplotlib legacy) —
  render `geo` geometry via wgpu; this is where Python stutters most.
- ⬜ Object tree / notebook / preferences UI.
- ⬜ Interactive editors (Group: GUI-heavy): Geo, **Gerber** (largest, ~7 K LOC),
  Excellon, G-code, Text editors.
- ⬜ Tool plugin panels for all Phase 2–3 operations.

## Suggested execution order

1. **Phase 2** (Paint/NCC/Cutout/Milling) — completes the value proposition for
   real PCB jobs, all headless and testable.
2. **Phase 5 + 3** in parallel — preprocessors and transforms are independent,
   low-risk, agent-parallelizable.
3. **Phase 4** parsers as needed (SVG/DXF unlock import workflows).
4. **Phase 6** app/project model.
5. **Phase 7** GUI last, on a proven engine — and use the headless CLI as the
   regression oracle for the GUI.

## Parallelization note

Phases 2–5 decompose into many independent, well-bounded tasks (one tool / one
preprocessor / one parser each) with shared types already defined in `fc-geo`
and `fc-gcode`. They are ideal for fan-out across multiple agents, each shipping
a crate module + unit tests verified against the Python tool's output.
