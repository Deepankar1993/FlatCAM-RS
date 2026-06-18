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

CLI: `flatcam-rs laser-iso <file> --beam-x --beam-y --beam-angle [--no-kerf] [--no-dynamic]`
and `flatcam-rs laser-cal --cal direction|power|focus [-o out.ngc] [--feed --power --mark-len --spacing --travel-z --angles --z-start --z-end --z-steps]`.

GUI: Laser panel (beam X/Y/angle, kerf + M4 toggles, Laser-Iso, Optimize fill∠,
Burn-preview heatmap overlay).

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

## Status (implemented, 34 fc-laser tests, all green)

- ✅ Flat `BeamShape` + directional kerf/power model (research-corrected formula).
- ✅ Anisotropic elliptical offset (affine trick).
- ✅ Per-segment power compensation + laser G-code (M4/M3); `laser_isolation`.
- ✅ Burn simulation + fill-angle optimizer.
- ✅ Astigmatic Z-dependent model (`astig`) with `at(z)`, `round_spot_z`, `best_focus`.
- ✅ Calibration grids (`calibration`) + CLI `laser-cal`.
- ✅ CLI `laser-iso`; GUI Laser panel + burn heatmap.
- ✅ Validated on the real KiCad SmartPowerMonitor B_Cu (0.10×0.06 beam → S1000
  vertical, ~S600 horizontal, 148 distinct direction-dependent S values).

## Next session — TODO

1. **Wire astigmatism into the operations.** Add a focus-`Z` input so `laser-iso`
   / GUI derive `beam = AstigmaticBeam::at(z)` instead of a fixed `BeamShape`.
   CLI: `--astig-waist-x/-y --astig-focus-x/-y --astig-rayleigh-x/-y --z`.
   GUI: an `AstigmaticBeam` editor + a Z slider + a "use round-spot Z" button.
2. **Calibration fitting helper.** A `fit_astig(measurements) -> AstigmaticBeam`
   in `astig` (or a new `calfit` module) that takes the measured per-Z H/V kerf
   table and least-squares-fits `waist/focus/rayleigh` per axis. GUI form to
   enter measurements; CLI to read a CSV.
3. **Power-curve calibration (LUT).** Capture the non-linear depth↔fluence
   response from the `power` grid into a monotone LUT; apply it so `power_factor`
   produces *visually* uniform burn, not just uniform fluence.
4. **Cross-hatch fill option** (0°/90° or angle-stepped) in paint/NCC to average
   out residual directionality, surfaced via the optimizer.
5. **Banding / scanning-offset** (separate, timing not shape): per-direction
   position offset ≈ ½·latency·feed for bidirectional raster; and overscan at
   raster line ends. Out of scope for shape comp but belongs in the laser path.
6. **GUI ergonomics:** show kerf/power-factor vs angle as a small polar plot;
   colour the burn heatmap with a legend; let the user pick the simulate feed/power.
7. **Bulge/arc fidelity:** the affine offset stretches arc-segment chords on the
   most-scaled axis (research caveat). If accuracy matters for high aspect ratios,
   densify rings before `anisotropic_offset` or raise circle resolution.

## References (from the research pass)

- Minkowski sum is GL(n)-covariant (`L(A⊕B)=L(A)⊕L(B)`) — Gardner/Hug/Weil,
  arXiv:1301.5267; ellipsoid C-space offset — Ruan/Chirikjian arXiv:2012.15461.
- Elliptical spot ↔ kerf/overlap — ResearchGate fig 275466220; kerf tracks the
  across-motion axis — US20050017156A1.
- GRBL M4 dynamic power — gnea/grbl wiki (Laser-Mode). LightBurn kerf/dot-width —
  docs.lightburnsoftware.com (Test-KerfOffset, PerfectImageEngraving).
- Diode astigmatism / beam quality — Edmund Optics beam-quality note.
