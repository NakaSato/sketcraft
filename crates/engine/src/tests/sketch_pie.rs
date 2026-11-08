//! `sketch.arc.pie` (SketchUp's Pie): a centre point arc closed by its two radius lines, all
//! sharing the centre and the arc's ends.

use serde_json::json;
use sketcraft_sketch::CurveKind;

use super::run;
use crate::*;

fn in_sketch() -> Session {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    s
}

/// The ends of every line and the arc's (centre, start, end) as point indices.
fn shape(s: &Session) -> (Vec<(usize, usize)>, Vec<(usize, usize, usize)>) {
    let sk = s.doc.sketch(s.active_sketch.unwrap()).unwrap();
    let lines = sk.curves.iter().filter_map(|c| if let CurveKind::Line { a, b } = c.kind { Some((a, b)) } else { None }).collect();
    let arcs = sk.curves.iter().filter_map(|c| if let CurveKind::Arc { c: m, a, b } = c.kind { Some((m, a, b)) } else { None }).collect();
    (lines, arcs)
}

#[test]
fn a_pie_is_an_arc_and_two_radius_lines_on_shared_points() {
    let mut s = in_sketch();
    let reply = run(&mut s, "sketch.arc.pie", json!({"center": [0, 0], "start": [10, 0], "end": [0, 25]}));
    assert_eq!(reply["curves"].as_array().map(Vec::len), Some(3), "{reply}");
    let (lines, arcs) = shape(&s);
    let [(c, a, b)] = arcs[..] else { panic!("one arc: {arcs:?}") };
    assert_eq!(lines, vec![(c, a), (c, b)], "the radius lines run from the arc's centre to its ends");
    let sk = s.doc.sketch(s.active_sketch.unwrap()).unwrap();
    let end = sk.point(b).unwrap();
    assert!((end.x - 0.0).abs() < 1e-9 && (end.y - 10.0).abs() < 1e-9, "the end keeps the radius: {end:?}");
}

#[test]
fn a_signed_sweep_turns_either_way() {
    for (sweep, want) in [(90.0, [0.0, 10.0]), (-90.0, [0.0, -10.0])] {
        let mut s = in_sketch();
        run(&mut s, "sketch.arc.pie", json!({"center": [0, 0], "start": [10, 0], "sweep": sweep}));
        let (lines, _) = shape(&s);
        let sk = s.doc.sketch(s.active_sketch.unwrap()).unwrap();
        let ends: Vec<[f64; 2]> = lines.iter().map(|(_, b)| sk.point(*b).map(|q| [q.x, q.y]).unwrap()).collect();
        assert!(ends.iter().any(|e| (e[0] - want[0]).abs() < 1e-9 && (e[1] - want[1]).abs() < 1e-9), "{sweep}: {ends:?}");
    }
}

#[test]
fn bad_pies_are_refused_and_change_nothing() {
    let mut s = in_sketch();
    let revision = s.revision;
    for p in [
        json!({"center": [0, 0], "start": [0, 0], "end": [0, 10]}),
        json!({"center": [0, 0], "start": [10, 0], "end": [20, 0]}),
        json!({"center": [0, 0], "start": [10, 0], "sweep": 0}),
        json!({"center": [0, 0], "start": [10, 0], "sweep": 400}),
        json!({"center": [0, 0], "start": [10, 0]}),
    ] {
        assert!(s.execute("sketch.arc.pie", &p).is_err(), "{p} was accepted");
        assert_eq!(s.revision, revision, "{p} changed the design");
    }
}
