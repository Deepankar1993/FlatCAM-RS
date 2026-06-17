# Agent Contribution Guide — modular parallel development

FlatCAM-RS is built so that **many agents can add features in parallel without
conflicts**. This document is the contract. An orchestrator (lead model) fans
out one feature per agent; each agent writes **one new module file plus tests**
against the stable API below; the orchestrator then does the trivial serial
integration (one `mod` line) and runs the single `cargo test --workspace` gate.

## The rules that prevent breakage

1. **One feature = one new file.** Add `crates/<crate>/src/<feature>.rs`. Never
   edit another agent's file in the same batch.
2. **Never edit shared files in a batch.** `lib.rs`/`Cargo.toml`/`main.rs` are
   touched only by the orchestrator during integration. (Adding a `pub mod x;`
   is a one-line, conflict-free serial step.)
3. **Every module is independently tested.** Include `#[cfg(test)] mod tests`
   with deterministic asserts. The module is "done" only when its tests pass.
4. **No new dependencies** without orchestrator approval (keeps the build green).
5. **Don't run `cargo` inside a parallel batch** (target-dir lock contention).
   The orchestrator compiles once after collecting all files.
6. **Pure & GUI-free.** Compute modules take geometry/params in and return
   geometry/paths/`CncJob` out. No I/O, no globals.

## Stable API surface (depend only on these)

### `fc_geo`
Types (re-exported `geo`): `Coord<f64>`, `LineString<f64>`, `Polygon<f64>`,
`MultiPolygon<f64>` (field `.0: Vec<Polygon<f64>>`, ctor `MultiPolygon::new(vec)`),
`Point`, `Rect`.

Functions:
- `circle(cx,cy,r,steps) -> Polygon`
- `regular_polygon(cx,cy,diameter,n,rotation_deg) -> Polygon`
- `centered_rect(cx,cy,w,h) -> Polygon`
- `obround(cx,cy,w,h,steps) -> MultiPolygon`
- `buffer_path(&[Coord], radius, steps) -> MultiPolygon`
- `union_all(Vec<Polygon>) -> MultiPolygon`
- `union(&MultiPolygon,&MultiPolygon) -> MultiPolygon`
- `difference(&MultiPolygon,&MultiPolygon) -> MultiPolygon`
- `offset(&MultiPolygon, distance) -> MultiPolygon`  (+grows / −shrinks)
- `area(&MultiPolygon) -> f64`
- `bounds(&MultiPolygon) -> Option<(minx,miny,maxx,maxy)>`
- `transform::{translate, scale(mp,sx,sy,origin), rotate(mp,deg,origin), skew, mirror_x(mp,axis), mirror_y(mp,axis)}`

### `fc_gcode`
- `enum Units { Inch, Mm }`
- `struct JobParams { units, tool_diameter, cut_z, travel_z, depth_per_pass, feed_xy, feed_z, spindle_rpm }` + `Default` (Mm)
- `type Polyline = Vec<(f64,f64)>`
- `enum JobKind { Mill { paths: Vec<Polyline> }, Drill { points: Vec<(f64,f64)> } }` (non-exhaustive in matches — add a catch-all arm)
- `struct CncJob { params: JobParams, kind: JobKind }`, method `to_gcode(&dyn Preprocessor) -> String`
- `pass_depths(cut_z, depth_per_pass) -> Vec<f64>`
- `trait Preprocessor { name; header; footer; rapid_z; rapid_xy; plunge; linear }`
- dialects: `Grbl`, `Marlin`, `dialects::{GenericDefault,GrblNoM6,GrblLaser,RolandMDX, by_name(name)->Option<Box<dyn Preprocessor>>}`

### `fc_cam` (existing modules to reuse, not reimplement)
- `isolation(&Gerber,&IsolationParams)->CncJob`, `drilling`, `drilling_all`
- `paint::paint_region(&MultiPolygon,&PaintParams)->Vec<Polyline>`
- `cutout`, `ncc`, `panelize`

### `fc_gerber` / `fc_excellon`
- `fc_gerber::parse(&str)->Result<Gerber>`; `Gerber{ units, apertures, solid_geometry:MultiPolygon, follow_geometry }`
- `fc_excellon::parse(&str)->Result<Excellon>`; `Excellon{ units, tools: BTreeMap<i32,Tool> }`, `Tool{ diameter, drills:Vec<(f64,f64)>, slots }`

## Integration checklist (orchestrator, serial, ~1 min)

1. For each delivered file, add `pub mod <feature>;` (+ optional `pub use`) to the
   crate's `lib.rs`.
2. `cargo test --workspace` — the single gate. Fix any integration mismatch
   (usually an API-name typo) or hand it back to the authoring agent.
3. Wire any user-facing command into `fc-cli` (optional, can batch later).
4. Update `CHANGELOG.md` / `ROADMAP.md`; commit (only `flatcam-rs/`).

## Why this is 10–20× faster

The expensive part of each feature (algorithm design + implementation + tests)
runs concurrently across agents. The orchestrator's serial work per feature is
~one line plus a shared compile. With the API frozen, agents never block on each
other, and a broken module fails its own tests in isolation rather than breaking
the build for everyone.
