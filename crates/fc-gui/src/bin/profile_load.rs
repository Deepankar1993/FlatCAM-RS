//! Profiling harness for the Gerber load pipeline.
//!
//! Measures, per file, the time of each stage the GUI performs when loading a
//! Gerber for display: read-to-string, parse, and triangulation of the fill
//! geometry (what `make_stored()` does). Helps decide whether to optimize
//! triangulation or move loading to a background thread.
//!
//! Usage: cargo run --release -p fc-gui --bin profile_load -- <file.gbr> [...]

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: profile_load <file.gbr> [more.gbr ...]");
        std::process::exit(2);
    }

    // Collected rows: (name, read_ms, parse_ms, tri_ms, polys, verts, tris)
    let mut rows: Vec<(String, f64, f64, f64, usize, usize, usize)> = Vec::new();

    for path in &args {
        let display = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());

        if !path.to_lowercase().ends_with(".gbr") {
            eprintln!("skip (not .gbr): {}", path);
            continue;
        }

        // Stage 1: read file to string.
        let t0 = Instant::now();
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("skip (read error: {}): {}", e, path);
                continue;
            }
        };
        let read_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Stage 2: parse.
        let t1 = Instant::now();
        let gerber = match fc_gerber::parse(&text) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("skip (parse error: {:?}): {}", e, path);
                continue;
            }
        };
        let parse_ms = t1.elapsed().as_secs_f64() * 1000.0;

        // Geometry stats.
        let polys = gerber.solid_geometry.0.len();
        let mut verts = 0usize;
        for poly in &gerber.solid_geometry.0 {
            verts += poly.exterior().0.len();
            for hole in poly.interiors() {
                verts += hole.0.len();
            }
        }

        // Stage 3: triangulate the fill (the suspected hog).
        let t2 = Instant::now();
        let tris = fc_geo::triangulate(&gerber.solid_geometry);
        let tri_ms = t2.elapsed().as_secs_f64() * 1000.0;

        rows.push((display, read_ms, parse_ms, tri_ms, polys, verts, tris.len()));
    }

    if rows.is_empty() {
        eprintln!("no .gbr files processed");
        std::process::exit(1);
    }

    // Tidy table.
    let name_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max(4);
    println!(
        "{:<nw$}  {:>9}  {:>9}  {:>11}  {:>8}  {:>9}  {:>9}",
        "file", "read ms", "parse ms", "triang ms", "polys", "verts", "tris",
        nw = name_w
    );
    println!(
        "{:-<nw$}  {:->9}  {:->9}  {:->11}  {:->8}  {:->9}  {:->9}",
        "", "", "", "", "", "", "",
        nw = name_w
    );
    for (name, read_ms, parse_ms, tri_ms, polys, verts, tris) in &rows {
        println!(
            "{:<nw$}  {:>9.2}  {:>9.2}  {:>11.2}  {:>8}  {:>9}  {:>9}",
            name, read_ms, parse_ms, tri_ms, polys, verts, tris,
            nw = name_w
        );
    }
}
