//! `solid.rotate` (SketchUp's Rotate): a turn about an axis through a point, kept about that
//! point when the angle is a parameter that changes.

use serde_json::json;

use super::run;
use crate::*;

fn bbox(s: &Session, body: &str) -> ([f64; 3], [f64; 3]) {
    let b = s.model.state().body(body).map(|b| b.body.clone()).unwrap();
    let m = sketcraft_kernel::measure(&b).unwrap();
    ([m.bbox.min.x, m.bbox.min.y, m.bbox.min.z], [m.bbox.max.x, m.bbox.max.y, m.bbox.max.z])
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

fn cube(s: &mut Session) {
    run(s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Block"}));
}

/// A quarter turn about Z through (10, 0, 0): (x, y) goes to (10 − y, x − 10), so the cube on
/// [0, 10]² lands on x ∈ [0, 10], y ∈ [−10, 0].
#[test]
fn a_quarter_turn_about_a_point_off_the_origin() {
    let mut s = Session::default();
    cube(&mut s);
    let reply = run(&mut s, "solid.rotate", json!({"bodies": ["Block"], "angle": 90, "origin": [10, 0, 0]}));
    assert_eq!(reply["changed_bodies"], json!(["Block"]), "{reply}");
    let (lo, hi) = bbox(&s, "Block");
    assert!(near(lo, [0.0, -10.0, 0.0]) && near(hi, [10.0, 0.0, 10.0]), "{lo:?} {hi:?}");
}

/// About X through (0, 0, 10) by 180°: (y, z) goes to (−y, 20 − z).
#[test]
fn the_axis_can_be_named_and_the_default_is_z() {
    let mut s = Session::default();
    cube(&mut s);
    run(&mut s, "solid.rotate", json!({"bodies": ["Block"], "angle": "180 deg", "axis": "X", "origin": [0, 0, 10]}));
    let (lo, hi) = bbox(&s, "Block");
    assert!(near(lo, [0.0, -10.0, 10.0]) && near(hi, [10.0, 0.0, 20.0]), "{lo:?} {hi:?}");
    // No axis: Z, and no origin: the world origin.
    run(&mut s, "solid.rotate", json!({"bodies": ["Block"], "angle": "180 deg"}));
    let (lo, hi) = bbox(&s, "Block");
    assert!(near(lo, [-10.0, 0.0, 10.0]) && near(hi, [0.0, 10.0, 20.0]), "{lo:?} {hi:?}");
}

/// The turn's translation is written in the angle: a parameter change turns the body further
/// about the same point. 180° about Z through (10, 0, 0): (x, y) goes to (20 − x, −y).
#[test]
fn a_parameter_angle_stays_about_the_origin_given() {
    let mut s = Session::default();
    run(&mut s, "parameters.change", json!({"name": "turn", "expression": "90 deg"}));
    cube(&mut s);
    run(&mut s, "solid.rotate", json!({"bodies": ["Block"], "angle": "turn", "origin": [10, 0, 0]}));
    let (lo, hi) = bbox(&s, "Block");
    assert!(near(lo, [0.0, -10.0, 0.0]) && near(hi, [10.0, 0.0, 10.0]), "{lo:?} {hi:?}");
    run(&mut s, "parameters.change", json!({"name": "turn", "expression": "180 deg"}));
    let (lo, hi) = bbox(&s, "Block");
    assert!(near(lo, [10.0, -10.0, 0.0]) && near(hi, [20.0, 0.0, 10.0]), "{lo:?} {hi:?}");
}

#[test]
fn a_copy_keeps_the_original_and_one_undo_takes_it_back() {
    let mut s = Session::default();
    cube(&mut s);
    let reply = run(&mut s, "solid.rotate", json!({"bodies": ["Block"], "angle": 90, "origin": [10, 0, 0], "copy": true}));
    let copies = reply["created_bodies"].as_array().cloned().unwrap_or_default();
    assert_eq!(copies.len(), 1, "{reply}");
    let (lo, hi) = bbox(&s, "Block");
    assert!(near(lo, [0.0, 0.0, 0.0]) && near(hi, [10.0, 10.0, 10.0]), "the original stays: {lo:?} {hi:?}");
    let (lo, _) = bbox(&s, copies[0].as_str().unwrap());
    assert!(near(lo, [0.0, -10.0, 0.0]), "{lo:?}");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.model.state().bodies.len(), 1);
}

#[test]
fn bad_calls_are_refused_and_change_nothing() {
    let mut s = Session::default();
    cube(&mut s);
    let revision = s.revision;
    for p in [
        json!({"bodies": [], "angle": 90}),
        json!({"bodies": ["Block"]}),
        json!({"bodies": ["Block"], "angle": "10 mm"}),
        json!({"bodies": ["Block"], "angle": 90, "axis": [0, 0, 0]}),
        json!({"bodies": ["Block"], "angle": 90, "origin": [0, "x", 0]}),
        json!({"bodies": ["Nope"], "angle": 90}),
    ] {
        assert!(s.execute("solid.rotate", &p).is_err(), "{p} was accepted");
        assert_eq!(s.revision, revision, "{p} changed the design");
    }
}
