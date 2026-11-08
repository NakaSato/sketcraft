//! `sketch.freehand` (SketchUp's Freehand): a stroke's samples thinned to what the curve needs,
//! then one fit point spline through them.

use serde_json::{Value, json};
use sketcraft_sketch::CurveKind;

use super::run;
use crate::*;

fn in_sketch() -> Session {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    s
}

/// The fit points of the one spline the active sketch holds.
fn fit_points(s: &Session) -> usize {
    let id = s.active_sketch.expect("a sketch is open");
    let sk = s.doc.sketch(id).expect("the sketch");
    let splines: Vec<usize> = sk.curves.iter().filter_map(|c| if let CurveKind::Spline { pts, control: false, .. } = &c.kind { Some(pts.len()) } else { None }).collect();
    assert_eq!(splines.len(), 1, "one fit point spline");
    splines[0]
}

fn samples(f: impl Fn(f64) -> [f64; 2], n: usize) -> Value {
    json!((0..n).map(|i| f(i as f64 / (n - 1) as f64)).collect::<Vec<_>>())
}

#[test]
fn a_straight_stroke_keeps_only_its_ends() {
    let mut s = in_sketch();
    let reply = run(&mut s, "sketch.freehand", json!({"points": samples(|t| [100.0 * t, 20.0 * t], 300)}));
    assert_eq!(reply["curves"].as_array().map(Vec::len), Some(1), "{reply}");
    assert_eq!(fit_points(&s), 2);
}

/// A half circle of radius 50 sampled 400 times: the default tolerance (0.5% of the stroke's
/// size, here 0.56 mm) keeps a handful of points, far fewer than were drawn, and more than its
/// ends; a tighter tolerance keeps more.
#[test]
fn a_curved_stroke_is_thinned_to_its_tolerance() {
    let arc = |t: f64| {
        let a = std::f64::consts::PI * t;
        [50.0 * a.cos(), 50.0 * a.sin()]
    };
    let mut s = in_sketch();
    run(&mut s, "sketch.freehand", json!({"points": samples(arc, 400)}));
    let loose = fit_points(&s);
    assert!((4..=40).contains(&loose), "{loose} fit points");
    let mut s = in_sketch();
    run(&mut s, "sketch.freehand", json!({"points": samples(arc, 400), "tolerance": 0.01}));
    let tight = fit_points(&s);
    assert!(tight > loose && tight <= 500, "{tight} fit points against {loose}");
}

#[test]
fn bad_strokes_are_refused_and_change_nothing() {
    let mut s = in_sketch();
    let revision = s.revision;
    for p in [
        json!({"points": [[0, 0]]}),
        json!({"points": [[0, 0], [0, 0], [0, 0]]}),
        json!({"points": [[0, 0], [1, "x"]]}),
        json!({"points": [[0, 0], [10, 0]], "tolerance": 0}),
        json!({"points": vec![[0.0, 0.0]; 2001]}),
    ] {
        assert!(s.execute("sketch.freehand", &p).is_err(), "{p} was accepted");
        assert_eq!(s.revision, revision, "{p} changed the design");
    }
    let mut outside = Session::default();
    assert!(outside.execute("sketch.freehand", &json!({"points": [[0, 0], [10, 0]]})).is_err(), "needs an open sketch");
}
