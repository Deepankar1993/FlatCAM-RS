# FlatCAM-RS

A ground-up **Rust** reimplementation of [FlatCAM Evo](../README.md) — the PCB
CAM application that turns Gerber/Excellon files into CNC G-code.

The Python original (PyQt6 + Shapely) is powerful but becomes sluggish on large
boards: geometry buffering/union, interactive rendering, and G-code generation
are CPU-bound and fighting the GIL. This port rebuilds the engine in Rust to be
fast, memory-lean, and embeddable, while keeping the Python codebase untouched.

> **Status: working CAM core (Phase 1 complete).** The geometry engine, Gerber
> and Excellon parsers, isolation routing, drilling, and a GRBL/Marlin G-code
> backend are implemented, unit-tested, and exposed through a headless CLI. The
> GUI and the full plugin set are scoped in [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Why Rust

| Pain point in Python FlatCAM | Rust approach |
|------------------------------|---------------|
| Shapely `.buffer()`/`unary_union` on big boards is slow | `geo` + `i_overlay` boolean ops + `geo-buffer`, no GIL, `rayon`-ready |
| GUI stutters during compute (single-threaded) | compute is pure/GUI-free in library crates; trivially threadable |
| Large memory footprint | compact `geo` primitives, no Python object overhead |
| Slow startup / heavy deps | a single 700 KB native binary |

## Workspace layout

```
flatcam-rs/
├── crates/
│   ├── fc-geo        # geometry foundation (Shapely analogue): circle, buffer,
│   │                 #   union/difference, polygon offset
│   ├── fc-gerber     # RS-274X Gerber parser  -> geo::MultiPolygon
│   ├── fc-excellon   # Excellon drill parser   -> tools/drills/slots
│   ├── fc-gcode      # CNC job model + preprocessor framework (GRBL, Marlin)
│   ├── fc-cam        # CAM ops: isolation, drilling, paint, NCC, cutout, …
│   ├── fc-cli        # headless front-end: `flatcam-rs`
│   └── fc-gui        # native desktop app (eframe/egui): `flatcam-gui`
├── examples/         # sample Gerber/Excellon fixtures
└── docs/             # DESIGN.md, ROADMAP.md
```

Each crate maps to a FlatCAM Python layer — see
[`docs/DESIGN.md`](docs/DESIGN.md) for the full mapping.

The crate boundaries are deliberately modular so the port can be built by **many
agents in parallel**: each feature is one self-contained module file with its own
tests, authored against a frozen API. See
[`docs/AGENT_GUIDE.md`](docs/AGENT_GUIDE.md) for the contribution contract and
[`docs/ROADMAP.md`](docs/ROADMAP.md) for the work breakdown.

## Build & test

```sh
cd flatcam-rs
cargo test --workspace      # 22 unit/integration tests
cargo build --release       # -> target/release/flatcam-rs
```

## CLI usage

```sh
# Inspect a file
flatcam-rs info  examples/two_pads.gbr
flatcam-rs info  examples/two_pads.drl

# Isolation routing: 2 passes, 0.4 mm V-bit, 25% overlap -> GRBL G-code
flatcam-rs iso   examples/two_pads.gbr --tool-dia 0.4 --passes 2 --overlap 0.25 \
                 --cut-z -0.05 -o board_iso.gcode

# Drilling: all tools, 1.8 mm depth, Marlin dialect
flatcam-rs drill examples/two_pads.drl --cut-z -1.8 --preproc marlin -o board_drill.gcode
```

## Desktop GUI

```sh
cargo run -p fc-gui                       # launch the native app
cargo run -p fc-gui -- examples/two_pads.gbr   # open a file on start
```

The `flatcam-gui` window opens Gerber/Excellon files, renders them on a pan/zoom
canvas (drag to pan, scroll to zoom), and runs Isolation/Paint with the results
overlaid as tool-paths. It is an early scaffold — the engine is complete; the
UI surface is being filled in (see the roadmap).

## Relationship to the Python project

This folder is **purely additive**. It does not import, modify, or depend on any
Python file in the parent repository. The Python FlatCAM Evo continues to live
and ship unchanged; FlatCAM-RS is a parallel rewrite that can eventually replace
it or be spun into its own repository.

## License

MIT, matching the porting effort's intent. (The original FlatCAM is MIT/other —
see the parent repo's `LICENSE`.)
