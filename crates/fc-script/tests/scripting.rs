//! Integration tests for the `fc-script` headless scripting engine.
//!
//! These drive the public API exactly the way the in-app shell does: build a
//! [`Registry`], run multi-line scripts against a [`ScriptContext`], then assert
//! on both the per-command output lines and the resulting object collection.
//! Everything here is deterministic — no file I/O, so `open_*` / `export_*`
//! (which need files on disk) are deliberately avoided.

use fc_script::{Registry, ScriptContext};

/// Run a whole script and return its collected (non-empty) output lines,
/// panicking with the offending line on the first error.
fn run(reg: &Registry, ctx: &mut ScriptContext, script: &str) -> Vec<String> {
    reg.run_script(ctx, script).expect("script should run without error")
}

/// Parse the float embedded in an `area` command's output (`"571.7257"`).
fn parse_area(line: &str) -> f64 {
    line.trim().parse::<f64>().unwrap_or_else(|_| panic!("not an area line: {line:?}"))
}

#[test]
fn rect_minus_circle_isolate_pipeline() {
    let reg = Registry::new();
    let mut ctx = ScriptContext::new();

    // The canonical end-to-end pipeline: draw a board, punch a hole, subtract,
    // isolate the result, then introspect it.
    let script = "\
new_rect board 0 0 30 20
new_circle hole 15 10 3
subtract board hole board2
isolate board2 iso 0.4 2 0.2
list
area board2";

    let out = run(&reg, &mut ctx, script);

    // All four named objects must exist in the context.
    let names = ctx.names();
    for expected in ["board", "hole", "board2", "iso"] {
        assert!(names.contains(&expected.to_string()), "missing object {expected}; have {names:?}");
    }

    // The isolated output is a CNC object; the source regions stay regions.
    assert_eq!(ctx.get("iso").unwrap().kind(), "cnc");
    assert_eq!(ctx.get("board").unwrap().kind(), "region");
    assert_eq!(ctx.get("board2").unwrap().kind(), "region");
    assert_eq!(ctx.get("hole").unwrap().kind(), "region");

    // `list` output is one of the produced lines and must mention board2:region.
    let list_line = out
        .iter()
        .find(|l| l.contains("board2:region"))
        .unwrap_or_else(|| panic!("no list line with board2:region in {out:?}"));
    assert!(list_line.contains("board:region"));
    assert!(list_line.contains("iso:cnc"));

    // 30x20 rectangle (600) minus a r=3 circle (~28.27) => ~571.7.
    let area_line = out.last().expect("area is the final output line");
    let area = parse_area(area_line);
    assert!((area - 571.73).abs() < 1.0, "expected ~571.73, got {area}");
}

#[test]
fn isolate_yields_cnc_object_via_run_line() {
    let reg = Registry::new();
    let mut ctx = ScriptContext::new();

    reg.run_line(&mut ctx, "new_rect plate 0 0 10 10").unwrap();
    let msg = reg.run_line(&mut ctx, "isolate plate plate_iso 0.4").unwrap();

    // isolate reports "<dst>: <n> paths".
    assert!(msg.starts_with("plate_iso:"), "unexpected isolate msg: {msg}");
    assert!(msg.contains("paths"), "isolate should report path count: {msg}");
    assert_eq!(ctx.get("plate_iso").unwrap().kind(), "cnc");
}

#[test]
fn transform_rotate_and_array() {
    let reg = Registry::new();
    let mut ctx = ScriptContext::new();

    // A 10x10 square at origin, rotated, then arrayed three times with a 20mm
    // pitch (so the three copies are disjoint and the area triples).
    let script = "\
new_rect cell 0 0 10 10
area cell
rotate cell cell_r 90
area cell_r
array cell_r grid 20 0 3
area grid";

    let out = run(&reg, &mut ctx, script);

    assert_eq!(ctx.get("cell_r").unwrap().kind(), "region");
    assert_eq!(ctx.get("grid").unwrap().kind(), "region");

    // out = [area cell, area cell_r, area grid] (the new_rect/rotate/array
    // emit message lines too, so locate the numeric ones).
    let areas: Vec<f64> = out
        .iter()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .collect();
    assert_eq!(areas.len(), 3, "expected three area lines, got {out:?}");

    let (base, rotated, gridded) = (areas[0], areas[1], areas[2]);
    assert!((base - 100.0).abs() < 1e-3, "base area {base}");
    // Rotation preserves area.
    assert!((rotated - base).abs() < 1e-3, "rotate changed area: {rotated} vs {base}");
    // Three disjoint copies => ~3x area.
    assert!((gridded - base * 3.0).abs() < 1e-3, "expected {}, got {gridded}", base * 3.0);
}

#[test]
fn analyze_drc_min_spacing_report() {
    let reg = Registry::new();
    let mut ctx = ScriptContext::new();

    // Two separated circles (centers 6 apart, r=1 each) => ~4mm gap.
    let script = "\
new_circle a 0 0 1
new_circle b 6 0 1
union a b board
drc board 0.5
drc board 10
min_spacing board
report board";

    let out = run(&reg, &mut ctx, script);
    assert_eq!(ctx.get("board").unwrap().kind(), "region");

    // The board has two features; check each analysis line is present.
    let joined = out.join("\n");
    assert!(joined.contains("DRC pass"), "expected a DRC pass at 0.5mm: {out:?}");
    assert!(
        joined.contains("DRC FAIL"),
        "expected a DRC FAIL at 10mm clearance: {out:?}"
    );

    let spacing_line = out
        .iter()
        .find(|l| l.starts_with("min spacing"))
        .unwrap_or_else(|| panic!("no min_spacing line in {out:?}"));
    let d: f64 = spacing_line
        .trim_start_matches("min spacing")
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("could not parse spacing from {spacing_line:?}"));
    assert!(d > 3.0 && d < 5.0, "unexpected min spacing {d}");

    let report_line = out
        .iter()
        .find(|l| l.contains("polygons"))
        .unwrap_or_else(|| panic!("no report line in {out:?}"));
    assert!(report_line.contains("polygons 2"), "report should see 2 features: {report_line}");
    assert!(report_line.contains("area"));
}

#[test]
fn gen_drill_array_yields_excellon() {
    let reg = Registry::new();
    let mut ctx = ScriptContext::new();

    // 3x4 grid of holes, 2mm pitch, 0.8mm dia => 12 drills in an Excellon obj.
    let script = "\
drill_array pads 0 0 2 2 3 4 0.8
count pads";

    let out = run(&reg, &mut ctx, script);

    assert_eq!(ctx.get("pads").unwrap().kind(), "excellon");

    // `count` of an Excellon object is the total drill count.
    let count_line = out.last().expect("count output");
    assert_eq!(count_line.trim(), "12", "expected 12 drills, got {count_line:?}");

    // The drill array can be turned into a CNC drilling job.
    reg.run_line(&mut ctx, "drill pads drilljob").unwrap();
    assert_eq!(ctx.get("drilljob").unwrap().kind(), "cnc");
}

#[test]
fn unknown_command_is_an_error() {
    let reg = Registry::new();
    let mut ctx = ScriptContext::new();

    // A bogus command anywhere in a script aborts the whole run with an error.
    let err = reg.run_script(
        &mut ctx,
        "new_rect r 0 0 10 10\nnot_a_real_command r d\narea r",
    );
    assert!(err.is_err(), "unknown command should make run_script fail");

    // Single-line form errors too; comments and blank lines do not.
    assert!(reg.run_line(&mut ctx, "definitely_not_a_command").is_err());
    assert!(reg.run_line(&mut ctx, "# just a comment").unwrap().is_empty());
    assert!(reg.run_line(&mut ctx, "   ").unwrap().is_empty());
}
