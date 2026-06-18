//! `flatcam-rs` — headless CLI for the FlatCAM Rust port.
//!
//! A GUI-free front-end over the CAM crates, useful for batch processing and
//! for verifying the engine end-to-end without a desktop session. It mirrors
//! the most-used FlatCAM operations: isolation routing of a Gerber and drilling
//! of an Excellon file.
//!
//! Usage:
//!   flatcam-rs info   <file>
//!   flatcam-rs iso    <gerber>   [options]
//!   flatcam-rs drill  <excellon> [options]
//!
//! Common options:
//!   -o <path>          output G-code file (default: stdout)
//!   --tool-dia <f>     tool diameter (iso)
//!   --passes <n>       isolation passes (iso)
//!   --overlap <f>      pass overlap fraction 0..1 (iso)
//!   --cut-z <f>        cut depth, negative
//!   --travel-z <f>     clearance height
//!   --feed-xy <f>      XY feedrate
//!   --feed-z <f>       plunge feedrate
//!   --rpm <f>          spindle speed
//!   --preproc <name>   grbl | marlin

use anyhow::{bail, Context, Result};
use fc_gcode::{Grbl, JobParams, Preprocessor, Units};
use std::collections::HashMap;
use std::path::Path;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Ok(());
    }
    let cmd = args[0].as_str();
    let positional: Vec<String> = args[1..]
        .iter()
        .take_while(|a| !a.starts_with('-'))
        .cloned()
        .collect();
    let opts = parse_opts(&args[1..]);

    match cmd {
        "info" => cmd_info(&positional),
        "iso" => cmd_iso(&positional, &opts),
        "drill" => cmd_drill(&positional, &opts),
        "paint" => cmd_paint(&positional, &opts),
        "ncc" => cmd_ncc(&positional, &opts),
        "cutout" => cmd_cutout(&positional, &opts),
        "laser-iso" => cmd_laser_iso(&positional, &opts),
        "laser-cal" => cmd_laser_cal(&opts),
        "script" => cmd_script(&positional),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        other => bail!("unknown command '{other}' (try: info | iso | drill)"),
    }
}

fn print_usage() {
    println!(
        "flatcam-rs — FlatCAM Rust port (headless CAM)\n\
         \n\
         COMMANDS:\n\
         \x20 info  <file>         parse and report statistics\n\
         \x20 iso    <gerber>      isolation-route a Gerber to G-code\n\
         \x20 paint  <gerber>      area-fill (pocket) the copper regions\n\
         \x20 ncc    <gerber>      non-copper clear (clear all non-copper area)\n\
         \x20 cutout <gerber>      mill the board outline with holding tabs\n\
         \x20 laser-iso <file>     isolation with diode beam-shape compensation\n\
         \x20                      (--beam-x --beam-y --beam-angle [--no-kerf] [--no-dynamic])\n\
         \x20                      astigmatic: --astig-waist-x/-y --astig-focus-x/-y\n\
         \x20                      --astig-rayleigh-x/-y --z <focusZ> (omit --z for round-spot Z)\n\
         \x20 laser-cal            emit a calibration grid (--cal direction|power|focus)\n\
         \x20 drill  <excellon>    drill an Excellon file to G-code\n\
         \x20 script <file>        run a batch script (see fc-script commands)\n\
         \n\
         Preprocessors (--preproc): grbl, marlin, default, grbl_no_m6, grbl_laser, roland\n\
         \n\
         See source header for the full option list."
    );
}

fn parse_opts(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(key) = args[i].strip_prefix("--") {
            // Known value-less boolean flags; everything else consumes a value
            // (which may be a negative number like -0.05).
            const BOOL_FLAGS: &[&str] = &["mirror", "origin", "no-contour", "on-line", "no-kerf", "no-dynamic"];
            if BOOL_FLAGS.contains(&key) {
                m.insert(key.to_string(), String::new());
                i += 1;
            } else {
                let val = args.get(i + 1).cloned().unwrap_or_default();
                m.insert(key.to_string(), val);
                i += 2;
            }
        } else if args[i] == "-o" {
            m.insert("o".to_string(), args.get(i + 1).cloned().unwrap_or_default());
            i += 2;
        } else {
            i += 1;
        }
    }
    m
}

fn getf(opts: &HashMap<String, String>, key: &str, default: f64) -> f64 {
    opts.get(key).and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn job_params_from_opts(opts: &HashMap<String, String>, units: Units) -> JobParams {
    let mut p = JobParams::default();
    p.units = units;
    p.cut_z = getf(opts, "cut-z", p.cut_z);
    p.travel_z = getf(opts, "travel-z", p.travel_z);
    p.depth_per_pass = getf(opts, "depth-per-pass", p.depth_per_pass);
    p.feed_xy = getf(opts, "feed-xy", p.feed_xy);
    p.feed_z = getf(opts, "feed-z", p.feed_z);
    p.spindle_rpm = getf(opts, "rpm", p.spindle_rpm);
    p
}

fn preproc_from_opts(opts: &HashMap<String, String>) -> Box<dyn Preprocessor> {
    opts.get("preproc")
        .and_then(|n| fc_gcode::dialects::by_name(n))
        .unwrap_or_else(|| Box::new(Grbl))
}

fn read(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {path}"))
}

fn write_output(opts: &HashMap<String, String>, content: &str) -> Result<()> {
    match opts.get("o") {
        Some(path) if !path.is_empty() => {
            std::fs::write(path, content).with_context(|| format!("writing {path}"))?;
            eprintln!("wrote {path} ({} bytes)", content.len());
        }
        _ => print!("{content}"),
    }
    Ok(())
}

fn cmd_info(pos: &[String]) -> Result<()> {
    let path = pos.first().context("info: expected a file path")?;
    let text = read(path)?;
    match classify(path) {
        FileKind::Gerber => {
            let g = fc_gerber::parse(&text)?;
            let area = fc_geo::area(&g.solid_geometry);
            println!("Gerber: {path}");
            println!("  units:      {:?}", g.units);
            println!("  apertures:  {}", g.apertures.len());
            println!("  polygons:   {}", g.solid_geometry.0.len());
            println!("  copper area:{area:.4}");
            if let Some((x0, y0, x1, y1)) = g.bounds() {
                println!("  bounds:     ({x0:.3},{y0:.3}) .. ({x1:.3},{y1:.3})");
            }
        }
        FileKind::Excellon => {
            let e = fc_excellon::parse(&text)?;
            println!("Excellon: {path}");
            println!("  units:  {:?}", e.units);
            println!("  tools:  {}", e.tools.len());
            for (t, tool) in &e.tools {
                println!(
                    "    T{t}: dia {:.4}  drills {}  slots {}",
                    tool.diameter,
                    tool.drills.len(),
                    tool.slots.len()
                );
            }
            println!("  total drills: {}", e.drill_count());
        }
    }
    Ok(())
}

fn cmd_iso(pos: &[String], opts: &HashMap<String, String>) -> Result<()> {
    let path = pos.first().context("iso: expected a gerber or svg path")?;
    let (geom, units) = load_geometry(path, opts)?;
    let params = fc_cam::IsolationParams {
        tool_diameter: getf(opts, "tool-dia", 0.1),
        passes: getf(opts, "passes", 1.0) as usize,
        overlap: getf(opts, "overlap", 0.0),
        job: job_params_from_opts(opts, units),
    };
    let job = fc_cam::isolation_geo(&geom, units, &params);
    let pp = preproc_from_opts(opts);
    let gcode = job.to_gcode(pp.as_ref());
    eprintln!(
        "isolation: {} pass(es), tool {:.3}, preproc {}",
        params.passes,
        params.tool_diameter,
        pp.name()
    );
    write_output(opts, &gcode)
}

fn cmd_drill(pos: &[String], opts: &HashMap<String, String>) -> Result<()> {
    let path = pos.first().context("drill: expected an excellon path")?;
    let text = read(path)?;
    let e = fc_excellon::parse(&text)?;
    let units = match e.units {
        fc_excellon::Units::Mm => Units::Mm,
        fc_excellon::Units::Inch => Units::Inch,
    };
    let base = job_params_from_opts(opts, units);
    let pp = preproc_from_opts(opts);
    let mut gcode = String::new();
    for (tool, job) in fc_cam::drilling_all(&e, base) {
        gcode.push_str(&format!("(--- tool T{tool} ---)\n"));
        gcode.push_str(&job.to_gcode(pp.as_ref()));
    }
    eprintln!("drill: {} tools, preproc {}", e.tools.len(), pp.name());
    write_output(opts, &gcode)
}

fn cmd_paint(pos: &[String], opts: &HashMap<String, String>) -> Result<()> {
    let path = pos.first().context("paint: expected a gerber or svg path")?;
    let (geom, units) = load_geometry(path, opts)?;
    let params = fc_cam::PaintParams {
        tool_diameter: getf(opts, "tool-dia", 0.5),
        overlap: getf(opts, "overlap", 0.2),
        margin: getf(opts, "margin", 0.0),
        add_contour: opts.get("no-contour").is_none(),
        job: job_params_from_opts(opts, units),
    };
    let job = fc_cam::paint_job(&geom, &params, units);
    let pp = preproc_from_opts(opts);
    let gcode = job.to_gcode(pp.as_ref());
    let passes = match &job.kind {
        fc_gcode::JobKind::Mill { paths } => paths.len(),
        _ => 0,
    };
    eprintln!(
        "paint: tool {:.3}, overlap {:.0}%, {} pass(es), preproc {}",
        params.tool_diameter,
        params.overlap * 100.0,
        passes,
        pp.name()
    );
    write_output(opts, &gcode)
}

fn cmd_ncc(pos: &[String], opts: &HashMap<String, String>) -> Result<()> {
    let path = pos.first().context("ncc: expected a gerber path")?;
    let (geom, units) = load_geometry(path, opts)?;
    let params = fc_cam::NccParams {
        tool_diameter: getf(opts, "tool-dia", 0.5),
        overlap: getf(opts, "overlap", 0.4),
        boundary_margin: getf(opts, "margin", 1.0),
        job: job_params_from_opts(opts, units),
    };
    let job = fc_cam::ncc_job(&geom, &params, units);
    let pp = preproc_from_opts(opts);
    let gcode = job.to_gcode(pp.as_ref());
    eprintln!("ncc: clear non-copper, tool {:.3}, preproc {}", params.tool_diameter, pp.name());
    write_output(opts, &gcode)
}

fn cmd_cutout(pos: &[String], opts: &HashMap<String, String>) -> Result<()> {
    let path = pos.first().context("cutout: expected a gerber or svg path")?;
    let (geom, units) = load_geometry(path, opts)?;
    let params = fc_cam::CutoutParams {
        tool_diameter: getf(opts, "tool-dia", 1.0),
        tabs: getf(opts, "tabs", 4.0) as usize,
        tab_gap: getf(opts, "tab-gap", 2.0),
        outside: opts.get("on-line").is_none(),
        job: job_params_from_opts(opts, units),
    };
    // Use the geometry's bounding box as the board outline.
    let (minx, miny, maxx, maxy) = geo_bounds(&geom).context("cutout: empty geometry")?;
    let paths = fc_cam::cutout_rectangular(minx, miny, maxx, maxy, &params);
    let mut jp = params.job.clone();
    jp.units = units;
    jp.tool_diameter = params.tool_diameter;
    let job = fc_gcode::CncJob { params: jp, kind: fc_gcode::JobKind::Mill { paths } };
    let pp = preproc_from_opts(opts);
    let gcode = job.to_gcode(pp.as_ref());
    eprintln!("cutout: {} tabs, tool {:.3}, preproc {}", params.tabs, params.tool_diameter, pp.name());
    write_output(opts, &gcode)
}

/// Load a 2-D region from a Gerber or SVG file (the geometry CAM ops act on).
/// SVG has no document units, so it is treated as millimetres.
fn load_geometry(
    path: &str,
    opts: &HashMap<String, String>,
) -> Result<(geo::MultiPolygon<f64>, Units)> {
    let lower = path.to_lowercase();
    let (mut geom, units) = if lower.ends_with(".pdf") {
        let bytes = std::fs::read(path).with_context(|| format!("reading {path}"))?;
        (fc_pdf::parse(&bytes)?.polygons, Units::Mm)
    } else {
        let text = read(path)?;
        if lower.ends_with(".svg") {
            (fc_svg::parse(&text)?.polygons, Units::Mm)
        } else if lower.ends_with(".dxf") {
            (fc_dxf::parse(&text)?.polygons, Units::Mm)
        } else {
            let g = fc_gerber::parse(&text)?;
            let u = match g.units {
                fc_gerber::Units::Mm => Units::Mm,
                fc_gerber::Units::Inch => Units::Inch,
            };
            (g.solid_geometry, u)
        }
    };
    // Optional board-positioning transforms (mirror a bottom layer, move to origin).
    if opts.contains_key("mirror") {
        geom = fc_geo::transform::mirror_bottom(&geom);
    }
    if opts.contains_key("origin") {
        geom = fc_geo::transform::normalize_origin(&geom);
    }
    Ok((geom, units))
}

fn geo_bounds(mp: &geo::MultiPolygon<f64>) -> Option<(f64, f64, f64, f64)> {
    use geo::BoundingRect;
    mp.bounding_rect()
        .map(|r| (r.min().x, r.min().y, r.max().x, r.max().y))
}

fn cmd_laser_iso(pos: &[String], opts: &HashMap<String, String>) -> Result<()> {
    let path = pos.first().context("laser-iso: expected a gerber/svg/dxf/pdf path")?;
    let (geom, _units) = load_geometry(path, opts)?;
    // Astigmatic mode: if any --astig-* option is present, build a Z-dependent
    // AstigmaticBeam and evaluate it at the chosen focus Z (--z, or the model's
    // round-spot Z when --z is omitted) to get the flat BeamShape for this run.
    let astig_keys = ["astig-waist-x", "astig-waist-y", "astig-focus-x", "astig-focus-y",
                      "astig-rayleigh-x", "astig-rayleigh-y"];
    let beam = if astig_keys.iter().any(|k| opts.contains_key(*k)) {
        let ab = fc_laser::AstigmaticBeam {
            waist_x: getf(opts, "astig-waist-x", 0.06),
            waist_y: getf(opts, "astig-waist-y", 0.10),
            focus_x: getf(opts, "astig-focus-x", 0.0),
            focus_y: getf(opts, "astig-focus-y", 0.0),
            rayleigh_x: getf(opts, "astig-rayleigh-x", 1.0),
            rayleigh_y: getf(opts, "astig-rayleigh-y", 1.0),
            angle_deg: getf(opts, "beam-angle", 0.0),
        };
        let z = match opts.get("z") {
            Some(s) => s.parse::<f64>().unwrap_or(0.0),
            None => ab.round_spot_z().unwrap_or_else(|| ab.best_focus()),
        };
        let b = ab.at(z);
        eprintln!(
            "laser-iso: astigmatic beam @ Z={:.4} -> {:.3}x{:.3} (round-spot Z {:?}, best-focus Z {:.4})",
            z, b.width_x, b.width_y, ab.round_spot_z(), ab.best_focus()
        );
        b
    } else {
        fc_laser::BeamShape {
            width_x: getf(opts, "beam-x", 0.1),
            width_y: getf(opts, "beam-y", 0.1),
            angle_deg: getf(opts, "beam-angle", 0.0),
        }
    };
    let passes = getf(opts, "passes", 1.0) as usize;
    let overlap = getf(opts, "overlap", 0.0);
    let compensate_kerf = !opts.contains_key("no-kerf");
    let dynamic = !opts.contains_key("no-dynamic");
    let paths = fc_laser::laser_isolation(&geom, &beam, passes, overlap, compensate_kerf);
    // spindle_rpm is reused as the laser max S-value.
    let jp = job_params_from_opts(opts, Units::Mm);
    let gcode = fc_laser::laser_gcode(&paths, &jp, dynamic);
    eprintln!(
        "laser-iso: beam {:.3}x{:.3} @ {:.0}deg, {} pass(es), kerf-comp {}, {} path(s), {} (S<=.{:.0})",
        beam.width_x, beam.width_y, beam.angle_deg, passes.max(1), compensate_kerf,
        paths.len(), if dynamic { "M4 dynamic" } else { "M3" }, jp.spindle_rpm
    );
    write_output(opts, &gcode)
}

fn cmd_laser_cal(opts: &HashMap<String, String>) -> Result<()> {
    let p = fc_laser::CalParams {
        feed: getf(opts, "feed", 600.0),
        power_max: getf(opts, "power", 1000.0),
        mark_len: getf(opts, "mark-len", 5.0),
        spacing: getf(opts, "spacing", 3.0),
        travel_z: getf(opts, "travel-z", 5.0),
        dynamic: !opts.contains_key("no-dynamic"),
    };
    let origin = (getf(opts, "x", 0.0), getf(opts, "y", 0.0));
    let kind = opts.get("cal").map(|s| s.as_str()).unwrap_or("direction");
    let gcode = match kind {
        "direction" => fc_laser::calibration::direction_fan(origin, getf(opts, "angles", 12.0) as usize, &p),
        "power" => fc_laser::calibration::power_feed_grid(
            origin,
            &[0.2, 0.4, 0.6, 0.8, 1.0],
            &[300.0, 600.0, 900.0, 1200.0],
            &p,
        ),
        "focus" => {
            let z0 = getf(opts, "z-start", -0.3);
            let z1 = getf(opts, "z-end", 0.3);
            let n = (getf(opts, "z-steps", 7.0) as usize).max(2);
            let zs: Vec<f64> = (0..n).map(|i| z0 + (z1 - z0) * (i as f64) / ((n - 1) as f64)).collect();
            fc_laser::calibration::focus_ramp(origin, &zs, &p)
        }
        other => bail!("laser-cal: unknown --cal '{other}' (use: direction | power | focus)"),
    };
    eprintln!("laser-cal: {kind} grid generated");
    write_output(opts, &gcode)
}

fn cmd_script(pos: &[String]) -> Result<()> {
    let path = pos.first().context("script: expected a script file path")?;
    let text = read(path)?;
    let reg = fc_script::Registry::new();
    let mut ctx = fc_script::ScriptContext::new();
    match reg.run_script(&mut ctx, &text) {
        Ok(outputs) => {
            for line in outputs {
                println!("{line}");
            }
            eprintln!("script ok: {} objects in context", ctx.names().len());
            Ok(())
        }
        Err(e) => bail!("script error: {e}"),
    }
}

enum FileKind {
    Gerber,
    Excellon,
}

fn classify(path: &str) -> FileKind {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "drl" | "nc" | "txt" | "xln" | "exc" => FileKind::Excellon,
        _ => FileKind::Gerber,
    }
}

