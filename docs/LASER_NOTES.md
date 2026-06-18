# Laser beam-shape compensation — design notes & continuation

This file is the working reference for the diode-laser subsystem (`crates/fc-laser`)
so work can resume cleanly in a later session. It covers the physics model, the
compensations, the calibration procedure, what is implemented, and the planned
next steps.

## Why this exists

Low-cost diode laser modules focus to an **elliptical / rectangular spot**, not a
circle, and the spot is **astigmatic** (the two axes focus at different Z heights).
Because the spot is non-circular and moving, both the cut **kerf** and the **burn
intensity** depend on travel direction. Mainstream tools don't compensate this
(LightBurn offers only a single scalar kerf + a "cut at 45°" workaround; FlatCAM
collapses everything to a circular aperture), so this is a genuine differentiator.

## Physics model (validated by research)

Spot = ellipse, semi-axes `a` (X), `b` (Y), mount angle `φ`. Ellipse radius in a
machine direction `θ`:

```
r(θ) = a·b / √( (b·cos(θ−φ))² + (a·sin(θ−φ))² )
```

- **Kerf** (cut width) for motion at angle θ = spot extent **perpendicular** to
  travel = `2·r(θ+90°)`.
- **Fluence** (areal burn) `H ∝ P / (v · L⊥)`. Note the along-travel length does
  **not** appear: the longer dwell on the long axis is cancelled by the larger
  spot area (lower intensity). So to equalise burn at fixed feed, scale power with
  the perpendicular kerf:
  - `power_factor(θ) = kerf_perpendicular(θ) / max_extent`  ∈ (0,1].
  - A horizontally-elongated spot → less power on horizontal moves (they'd over-burn).
- Depth-vs-fluence is **non-linear**; `power_factor` corrects the *relative*
  directionality — absolute power/feed still need calibration (below).

### Astigmatism (Z dependence)

Each axis follows Gaussian-beam propagation:

```
W(z) = W0 · √( 1 + ((z − z_f) / z_R)² )
```

`W0` = waist (min) full width, `z_f` = that axis's focus Z, `z_R` = Rayleigh range
(defocus for √2 growth). The two axes have different `W0`, `z_f`, `z_R`, so the
spot's aspect — and which axis is wider — changes with Z. There is generally a
**round-spot Z** between the foci where `W_x(z)=W_y(z)`.

## Code map (`crates/fc-laser`)

| Module | Key API | Purpose |
|--------|---------|---------|
| `beam` | `BeamShape{width_x,width_y,angle_deg}`; `radius_in_dir`, `kerf_perpendicular`, `dwell_extent`, `power_factor`; `segment_angle` | flat (single-Z) spot + directional metrics |
| `astig` | `AstigmaticBeam{waist_x/y,focus_x/y,rayleigh_x/y,angle_deg}`; `width_x_at/width_y_at`, `at(z)->BeamShape`, `round_spot_z`, `best_focus` | Z-dependent spot; `at(z)` feeds everything else |
| `offset` | `anisotropic_offset(geom,&beam,k)` | elliptical kerf offset via the affine Minkowski trick (rotate→scale→circular offset→unscale→unrotate) |
| `emit` | `compensate_power(paths,&beam)->Vec<Vec<(x,y,power)>>`; `laser_gcode(paths,&JobParams,dynamic)` | per-segment S, M4/M3 |
| `cam` | `laser_isolation(geom,&beam,passes,overlap,kerf)` | anisotropic kerf + power comp in one op |
| `simulate` | `simulate(paths,&beam,feed,power,cell)->BurnMap` (`.at`,`.max`) | fluence raster for the visual heatmap |
| `optimize` | `optimal_fill_angle(region,&beam,spacing,feed,power)->(angle,cv)`; `burn_uniformity` | min-variance fill angle |
| `calibration` | `CalParams`; `direction_fan`, `power_feed_grid`, `focus_ramp` | G-code test grids to measure the model |
| `calfit` | `KerfMeasurement{z,width_x,width_y}`; `fit_astig(meas,angle)->AstigmaticBeam`, `fit_axis_params` | least-squares fit of the astigmatic model from a measured per-Z H/V kerf table (closed-form parabola in `W²`) |
| `powercurve` | `PowerCurve::{from_samples,depth_at,power_for_depth,visual_factor}` | monotone (isotonic/PAVA) power↔depth LUT; `visual_factor` maps a fluence-uniform factor to a *visually*-uniform one |
| `crosshatch` | `crosshatch_fill(region,spacing,angles)`, `crosshatch_orthogonal`, `crosshatch_for_beam` | multi-angle hatch passes (e.g. 0/90) to average out residual directional burn |
| `banding` | `scan_offset_distance(feed,latency)`, `apply_scan_offset`, `compensate_banding`, `overscan` | timing (not shape) comp: bidirectional latency position-offset + raster overscan |
| `densify` | `densify_rings(geom,max_seg)`, `densify_for_beam(geom,&beam,frac)` | ring densification pre-pass to fix arc-chord stretch under the affine elliptical offset |
| `polar` | `PolarSample`; `polar_samples`, `polar_kerf_points`, `polar_power_points`, `polar_extents` | GUI-free polar-plot data (kerf/power-factor vs travel angle) |
| `emit_curve` | `compensate_power_curve(paths,&beam,&PowerCurve)`, `recompensate_with_curve(annotated,&PowerCurve)` | composes the directional factor with the calibrated power curve for *visually*-uniform S |
| `fill` | `laser_fill_paths(region,&beam,spacing,angles)`, `laser_fill_for_beam`, `laser_fill_gcode` | cross-hatch area-fill as a full op (hatch → power comp → G-code) |
| `raster` | `raster_scan(region,spacing,angle)`, `raster_fill_banded(region,spacing,angle,feed,latency,overscan)` | boustrophedon raster + banding/overscan timing comp |
| `calcsv` | `parse_kerf_csv`, `parse_power_csv`, `kerf_to_csv` | lenient CSV/whitespace parsing of measured calibration tables (no file I/O) |

CLI: `flatcam-rs laser-iso <file> --beam-x --beam-y --beam-angle [--no-kerf] [--no-dynamic]`;
astigmatic mode (auto-detected when any `--astig-*` is passed):
`--astig-waist-x/-y --astig-focus-x/-y --astig-rayleigh-x/-y --z <focusZ>`
(omit `--z` → uses the model's round-spot Z, falling back to best-focus).
`laser-iso` also accepts `[--densify[=frac]]` (arc-chord fidelity pre-pass) and
`[--power-curve <csv>]` (measured power→depth correction).
`flatcam-rs laser-fill <file> [--spacing --angles a,b --overlap | --raster --angle
--feed --latency --overscan] [--power-curve <csv>]` — cross-hatch / bidirectional
raster area-fill with beam comp.
`flatcam-rs laser-fit <kerf.csv> [--beam-angle]` — fit the astigmatic model from a
measured `z,width_x,width_y` focus-ramp table and print the parameters.
And `flatcam-rs laser-cal --cal direction|power|focus [-o out.ngc] [--feed --power --mark-len --spacing --travel-z --angles --z-start --z-end --z-steps]`.

GUI: Laser panel — flat beam **or** astigmatic editor (waist/focus/rayleigh per
axis + mount angle), a **Focus-Z** slider with **Round-spot Z / Best-focus Z**
buttons (shows the resolved beam dims), kerf + M4 toggles, a **polar plot** of
kerf/power-factor/**dwell** vs travel angle, a **simulate feed/power** picker
feeding the optimiser + burn sim, **Laser-Iso** + **Cross-hatch fill** buttons,
Optimize fill∠, Burn-preview heatmap overlay with a legend strip, a **Power curve**
entry (`power,depth`) with an "apply to S" toggle, and a **Fit astig** form that
parses a pasted `z,width_x,width_y` table and populates the astig editor.

## Calibration procedure (how to fit the model)

Run on scrap of the target material at a fixed, known focus unless noted.

1. **Orientation + aspect** — `laser-cal --cal direction --angles 12`.
   Engrave; measure each line's width/darkness. The widest line's angle = the
   **long-axis direction** → set `BeamShape.angle_deg`. `width_min : width_max`
   across angles gives the aspect; pick `width_x`,`width_y` so the perpendicular
   kerf matches (kerf at θ = perpendicular extent).
2. **Power / depth curve** — `laser-cal --cal power`.
   Matrix of marks at 20–100 % power × several feeds. Find the lasing threshold
   and the (non-linear) depth response. Use it to set the absolute `S`/feed and,
   later, a calibrated power-curve LUT (see TODO).
3. **Focus + astigmatism** — `laser-cal --cal focus --z-start -0.3 --z-end 0.3 --z-steps 7`.
   Each Z prints a cross (H + V mark). Measure the **horizontal** kerf (= Y-axis
   spot extent) and **vertical** kerf (= X-axis spot extent) at each Z:
   - Z where the vertical mark (X extent) is thinnest → `focus_x`; its width → `waist_x`.
   - Z where the horizontal mark (Y extent) is thinnest → `focus_y`; width → `waist_y`.
   - Fit `z_R` per axis from how fast the width grows away from focus
     (`W(z_f ± z_R) = √2 · W0`).
   - The Z where H and V kerf match = `round_spot_z` (cross-check vs the model).

Then `AstigmaticBeam{..}.at(z)` gives the `BeamShape` to use at any chosen focus,
and `round_spot_z()` / `best_focus()` suggest good operating heights.

## Status (implemented, 93 fc-laser tests, all green; workspace 576 tests)

- ✅ Flat `BeamShape` + directional kerf/power model (research-corrected formula).
- ✅ Anisotropic elliptical offset (affine trick).
- ✅ Per-segment power compensation + laser G-code (M4/M3); `laser_isolation`.
- ✅ Burn simulation + fill-angle optimizer.
- ✅ Astigmatic Z-dependent model (`astig`) with `at(z)`, `round_spot_z`, `best_focus`.
- ✅ Calibration grids (`calibration`) + CLI `laser-cal`.
- ✅ **Astigmatism wired into operations** — CLI `laser-iso --astig-* --z`; GUI
  astig editor + Focus-Z slider + Round-spot/Best-focus Z buttons (TODO 1).
- ✅ **`calfit::fit_astig`** — closed-form least-squares fit of the astigmatic
  model from a measured per-Z H/V kerf table (TODO 2).
- ✅ **`powercurve::PowerCurve`** — monotone (PAVA isotonic) power↔depth LUT;
  `visual_factor` corrects for the non-linear depth response (TODO 3).
- ✅ **`crosshatch`** — multi-angle / orthogonal hatch fill to average residual
  directional burn (TODO 4).
- ✅ **`banding`** — bidirectional latency scan-offset + raster overscan helpers
  (timing, not shape) (TODO 5).
- ✅ **GUI ergonomics** — polar kerf/power-factor plot, burn-heatmap legend,
  user-set simulate feed/power (TODO 6).
- ✅ **`densify`** — ring densification pre-pass for arc-chord fidelity under the
  affine offset (TODO 7).
- ✅ **`powercurve` applied in the emission path** (`emit_curve`): CLI
  `laser-iso/laser-fill --power-curve <csv>`; GUI "Power curve" entry + toggle.
- ✅ **Cross-hatch surfaced as an op** (`fill`): CLI `laser-fill`; GUI "Cross-hatch
  fill" button.
- ✅ **Auto-densify** (`densify`): CLI `laser-iso --densify[=frac]`.
- ✅ **Banding wired into a raster generator** (`raster`): CLI `laser-fill --raster`
  applies `overscan` + `compensate_banding`.
- ✅ **`calfit` data entry** (`calcsv`): CLI `laser-fit <csv>`; GUI "Fit astig" form.
- ✅ **Polar-plot dwell overlay** added (kerf + power + dwell curves).
- ✅ Validated on the real KiCad SmartPowerMonitor B_Cu (0.10×0.06 beam → S1000
  vertical, ~S600 horizontal, 148 distinct direction-dependent S values).

## Next session — TODO (nice-to-haves only; the model + ops are complete)

All prior items are implemented and wired across CLI + GUI. What's left is minor
polish, not missing capability:

1. **Connected raster path.** `raster_scan` returns per-line segments; optionally
   join them into one continuous boustrophedon polyline (with travel moves) for a
   single laser-on stream, and expose a GUI raster-fill button with banding inputs.
2. **Polar-plot ticks.** Numeric radial/angle labels on the GUI polar plot.
3. **Power-curve from the `power` grid image.** Today the curve is entered as
   `power,depth` samples; a helper to derive it from a scanned/measured grid would
   close the calibration loop end-to-end.
4. **Real-material validation** of the astig fit + power curve against an actual
   diode engraver (the model is unit-tested and round-trip-verified, not yet
   bench-calibrated on hardware).

## References (from the research pass)

- Minkowski sum is GL(n)-covariant (`L(A⊕B)=L(A)⊕L(B)`) — Gardner/Hug/Weil,
  arXiv:1301.5267; ellipsoid C-space offset — Ruan/Chirikjian arXiv:2012.15461.
- Elliptical spot ↔ kerf/overlap — ResearchGate fig 275466220; kerf tracks the
  across-motion axis — US20050017156A1.
- GRBL M4 dynamic power — gnea/grbl wiki (Laser-Mode). LightBurn kerf/dot-width —
  docs.lightburnsoftware.com (Test-KerfOffset, PerfectImageEngraving).
- Diode astigmatism / beam quality — Edmund Optics beam-quality note.
