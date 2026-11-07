//! SketchUp-tab drawing with no Create Sketch step (`sketchup_draw`).

use crate::scenario::Harness;
use crate::workspace::SKETCHUP_TAB;
use serde_json::json;
use sketcraft_engine::geom::prelude::*;
use solidcore_record::Vec3;

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
