//! SketchUp-tab drawing with no Create Sketch step (`sketchup_draw`).

use crate::scenario::Harness;
use crate::workspace::SKETCHUP_TAB;
use serde_json::json;
use sketcore_record::Vec3;
use sketcraft_engine::geom::prelude::*;

fn on_sketchup_tab() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.ui.tab = SKETCHUP_TAB.into();
    h.frames(2);
    h
}

/// The open sketch's plane: (normal, the tool's first point in the world).
fn first_point(h: &Harness) -> (Vec3, Vec3) {
    let active = h.app.session.active_sketch.expect("a sketch is open");
    let state = h.app.session.world_state().unwrap();
    let ss = state.sketches.iter().find(|s| s.feature == active).expect("the open sketch");
    let first = h.app.tool.as_ref().and_then(|t| t.pts.first()).expect("a first point").0;
    (ss.plane.normal(), ss.plane.to_world(first))
}

fn near(a: Vec3, b: Vec3) -> bool {
    a.dist(b) < 0.5
}

/// Line from the SketchUp tab opens no window: the first click in the view makes the sketch on
/// the ground and is the line's first point; the next click draws the line.
#[test]
fn line_starts_on_the_ground_from_its_first_click() {
    let mut h = on_sketchup_tab();
    h.step(&json!({"click":{"handle":"button:sketch.line"}})).unwrap();
    assert!(h.app.dialog.is_none(), "no Create Sketch window");
    assert_eq!(crate::sketchup_draw::pending(&h.app), Some("sketch.line"));
    assert!(h.app.session.active_sketch.is_none(), "no sketch before the click");
    h.step(&json!({"click":{"world":[10, 10, 0]}})).unwrap();
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.line"));
    let (normal, at) = first_point(&h);
    assert!(normal.z.abs() > 0.999, "on the ground: {normal:?}");
    assert!(near(at, Vec3::new(10.0, 10.0, 0.0)), "the click is the first point: {at:?}");
    h.step(&json!({"click":{"world":[30, 10, 0]}})).unwrap();
    let sketch = h.app.session.doc.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    assert_eq!(sketch.curves.len(), 1, "the second click drew the line");
    // Finishing goes back to the SketchUp tab.
    h.step(&json!({"key":"Escape"})).unwrap();
    h.app.finish_sketch();
    h.frames(2);
    assert_eq!(h.app.ui.tab, SKETCHUP_TAB);
}

/// A click on a flat face draws on that face.
#[test]
fn rectangle_starts_on_the_face_it_is_clicked_on() {
    let mut h = on_sketchup_tab();
    h.app.run("solid.box", json!({"corner":[0, 0, 0], "length":20, "width":20, "height":10, "body_name":"Box"})).unwrap();
    h.app.ui.tab = SKETCHUP_TAB.into();
    h.frames(2);
    h.step(&json!({"click":{"handle":"button:sketch.rectangle.two_point"}})).unwrap();
    assert_eq!(crate::sketchup_draw::pending(&h.app), Some("sketch.rectangle.two_point"));
    h.step(&json!({"click":{"world":[5, 5, 10]}})).unwrap();
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.rectangle.two_point"));
    let (normal, at) = first_point(&h);
    assert!(normal.z.abs() > 0.999, "on the top face: {normal:?}");
    assert!(near(at, Vec3::new(5.0, 5.0, 10.0)), "the first corner is where the face was clicked: {at:?}");
}

/// Escape, or any other command, stops a tool still waiting for its first click; its shortcut on
/// the SketchUp tab waits the same way.
#[test]
fn a_waiting_tool_stops_on_escape_or_another_command() {
    let mut h = on_sketchup_tab();
    h.step(&json!({"key":"L"})).unwrap();
    assert!(h.app.dialog.is_none(), "the key opens no window either");
    assert_eq!(crate::sketchup_draw::pending(&h.app), Some("sketch.line"));
    h.step(&json!({"key":"Escape"})).unwrap();
    assert_eq!(crate::sketchup_draw::pending(&h.app), None);
    h.step(&json!({"click":{"world":[10, 10, 0]}})).unwrap();
    assert!(h.app.session.active_sketch.is_none(), "a click after Escape makes no sketch");
    h.step(&json!({"click":{"handle":"button:sketch.circle.center"}})).unwrap();
    assert_eq!(crate::sketchup_draw::pending(&h.app), Some("sketch.circle.center"));
    h.app.start("inspect.measure");
    assert_eq!(crate::sketchup_draw::pending(&h.app), None);
}

/// Off the SketchUp tab, drawing tools keep the Create Sketch step.
#[test]
fn other_tabs_keep_create_sketch() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.preferences.sketch_tab_always = true;
    h.app.ui.tab = "SKETCH".into();
    h.frames(2);
    h.app.start("sketch.line");
    assert!(matches!(h.app.dialog.as_ref().map(|d| &d.kind), Some(crate::dialogs::Kind::Sketch)));
    assert_eq!(crate::sketchup_draw::pending(&h.app), None);
}

/// Push/Pull chosen in a sketch begun from the SketchUp tab finishes the sketch and starts, as
/// SketchUp has no sketch to leave; its button there is not greyed. A sketch begun elsewhere
/// keeps the usual rule.
#[test]
fn push_pull_in_a_sketchup_sketch_finishes_it_first() {
    let mut h = on_sketchup_tab();
    h.step(&json!({"click":{"handle":"button:sketch.rectangle.two_point"}})).unwrap();
    h.step(&json!({"click":{"world":[0, 0, 0]}})).unwrap();
    h.step(&json!({"click":{"world":[20, 10, 0]}})).unwrap();
    let sketch = h.app.session.active_sketch.expect("drawing in a sketch");
    assert!(crate::sketchup_draw::finishes_sketch_for(&h.app, "solid.press_pull"));
    let info = crate::render_workspace::interactive_command_info(&h.app, sketcraft_engine::find_command("solid.press_pull").unwrap());
    assert!(info.enabled, "Push/Pull is not greyed: {:?}", info.disabled_reason);
    h.app.start("solid.press_pull");
    assert!(h.app.session.active_sketch.is_none(), "the sketch is finished");
    assert!(h.app.session.doc.sketch(sketch).is_ok_and(|s| !s.curves.is_empty()), "with the rectangle kept");
    assert_eq!(h.app.ui.tab, SKETCHUP_TAB);
    assert!(h.app.dialog.is_some() || h.app.tool.is_some(), "Push/Pull started");

    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    assert!(!crate::sketchup_draw::finishes_sketch_for(&h.app, "solid.press_pull"), "a sketch begun on Model");
}

/// A rectangle also drags out from its first press, as in SketchUp: the press picks the plane
/// and the release places the far corner.
#[test]
fn a_rectangle_drags_out_from_its_first_press() {
    let mut h = on_sketchup_tab();
    h.step(&json!({"click":{"handle":"button:sketch.rectangle.two_point"}})).unwrap();
    h.step(&json!({"drag":[{"world":[0, 0, 0]}, {"world":[20, 10, 0]}], "steps": 6})).unwrap();
    h.frames(2);
    let sketch = h.app.session.active_sketch.expect("the press made the sketch");
    assert_eq!(h.app.session.doc.sketch(sketch).unwrap().curves.len(), 4, "one rectangle");
    assert!(h.app.viewport.boxsel.is_none(), "the drag drew, it did not box-select");
}

/// Rotate (Q) is on the SketchUp tab and opens Move/Copy on its Rotate type with the selected
/// body picked; a quarter turn about Z then lands the box as `solid.move` turns it.
#[test]
fn rotate_opens_move_on_its_rotate_type() {
    let mut h = on_sketchup_tab();
    h.app.run("solid.box", json!({"length": 20, "width": 30, "height": 10})).unwrap();
    h.frames(2);
    assert!(crate::scenario::handle_at("button:solid.rotate").is_some(), "Rotate has a button on the SketchUp tab");
    h.app.run("select.set", json!({"items": [{"type": "body", "body": "Body1"}]})).unwrap();
    h.step(&json!({"key": "Q"})).unwrap();
    h.frames(2);
    let d = h.app.dialog.as_mut().expect("Q opens a dialog");
    let crate::dialogs::Kind::Move { mode, .. } = d.kind else { panic!("not Move/Copy: {:?}", d.kind) };
    assert_eq!(mode, crate::dialogs_move::ROTATE, "on its Rotate type");
    assert_eq!(d.inputs[0].items, vec![sketcraft_engine::Sel::Body { name: "Body1".into() }], "the selected body is picked");
    d.inputs[1].items = vec![sketcraft_engine::Sel::Axis { name: "Z".into() }];
    if let crate::dialogs::Kind::Move { angle, .. } = &mut d.kind {
        *angle = "-90 deg".into();
    }
    h.frames(2);
    h.step(&json!({"key": "Enter"})).unwrap();
    h.settle();
    assert!(h.app.dialog.is_none());
    let bounds = h.app.session.model.state().body("Body1").unwrap().mesh().bounds();
    assert!((bounds.min - Vec3::new(0.0, -20.0, 0.0)).len() < 1e-6, "{bounds:?}");
    assert!((bounds.max - Vec3::new(30.0, 0.0, 10.0)).len() < 1e-6, "{bounds:?}");
}

/// Freehand draws by press and drag: letting go turns the stroke into one fit point spline. A
/// click alone draws nothing.
#[test]
fn freehand_draws_one_curve_along_a_drag() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.start("sketch.freehand");
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.freehand"));
    let sketch = h.app.session.active_sketch.expect("a sketch is open");
    let curves = |h: &Harness| h.app.session.doc.sketch(sketch).map(|s| s.curves.len()).unwrap_or(0);
    h.step(&json!({"click": {"world": [5, 5, 0]}})).unwrap();
    h.frames(2);
    assert_eq!(curves(&h), 0, "a click draws nothing");
    h.step(&json!({"drag": [{"world": [0, 0, 0]}, {"world": [40, 20, 0]}], "steps": 16})).unwrap();
    h.frames(2);
    let sk = h.app.session.doc.sketch(sketch).unwrap();
    assert_eq!(sk.curves.len(), 1, "one curve");
    assert!(matches!(sk.curves[0].kind, sketcraft_engine::sketch::CurveKind::Spline { control: false, .. }), "a fit point spline");
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.freehand"), "the tool stays for the next stroke");
}

/// Eraser: a click deletes the curve under it, and a drag deletes every curve it crosses.
#[test]
fn eraser_deletes_a_clicked_curve_and_those_a_drag_crosses() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    for x in [0, 10, 20] {
        h.app.run("sketch.line", json!({"points": [[x, 0], [x, 10]], "connect": false})).unwrap();
    }
    let sketch = h.app.session.active_sketch.expect("a sketch is open");
    let lines = |h: &Harness| h.app.session.doc.sketch(sketch).map(|s| s.curves.len()).unwrap_or(0);
    h.app.start("sketch.erase");
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.erase"));
    h.frames(2);
    h.step(&json!({"click": {"world": [20, 5, 0]}})).unwrap();
    h.frames(2);
    assert_eq!(lines(&h), 2, "the clicked line is gone");
    h.step(&json!({"drag": [{"world": [-5, 5, 0]}, {"world": [15, 5, 0]}], "steps": 16})).unwrap();
    h.frames(2);
    assert_eq!(lines(&h), 0, "the drag erased both lines it crossed");
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(lines(&h), 2, "one undo brings the dragged-over lines back");
}

/// Pie: three clicks (centre, start, end) draw the arc and both radius lines.
#[test]
fn pie_draws_an_arc_closed_by_two_radius_lines() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.start("sketch.arc.pie");
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.arc.pie"));
    for at in [[0, 0, 0], [20, 0, 0], [0, 20, 0]] {
        h.step(&json!({"click": {"world": at}})).unwrap();
        h.frames(2);
    }
    let sketch = h.app.session.active_sketch.expect("a sketch is open");
    let sk = h.app.session.doc.sketch(sketch).unwrap();
    let (lines, arcs) = sk.curves.iter().fold((0, 0), |(l, a), c| match c.kind {
        sketcraft_engine::sketch::CurveKind::Line { .. } => (l + 1, a),
        sketcraft_engine::sketch::CurveKind::Arc { .. } => (l, a + 1),
        _ => (l, a),
    });
    assert_eq!((arcs, lines), (1, 2), "one arc and its two radius lines");
}
