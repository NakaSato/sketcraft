//! `sketch.erase` (SketchUp's Eraser dragged across edges): every curve the path crosses goes,
//! whole, in one undo step.

use serde_json::{Value, json};

use super::run;
use crate::*;

/// A sketch with three upright lines at x = 0, 10 and 20; their ids, left to right.
fn three_lines() -> (Session, Vec<String>) {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    let ids = [0.0, 10.0, 20.0]
        .iter()
        .map(|x| run(&mut s, "sketch.line", json!({"points": [[x, 0.0], [x, 10.0]], "connect": false}))["curves"][0].as_str().unwrap().to_string())
        .collect();
    (s, ids)
}

fn curve_ids(s: &Session) -> Vec<String> {
    s.doc.sketch(s.active_sketch.unwrap()).unwrap().curves.iter().map(|c| c.id.clone()).collect()
}

#[test]
fn a_path_erases_the_curves_it_crosses_and_one_undo_brings_them_back() {
    let (mut s, ids) = three_lines();
    let reply = run(&mut s, "sketch.erase", json!({"points": [[-5, 5], [5, 6], [15, 4]]}));
    assert_eq!(reply["erased"], json!([ids[0], ids[1]]), "{reply}");
    assert_eq!(curve_ids(&s), vec![ids[2].clone()], "the line at x = 20 stays");
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(curve_ids(&s), ids);
}

#[test]
fn a_dry_run_names_what_would_go_and_changes_nothing() {
    let (mut s, ids) = three_lines();
    let revision = s.revision;
    let reply = run(&mut s, "sketch.erase", json!({"points": [[15, 5], [25, 5]], "dry_run": true}));
    assert_eq!(reply["erased"], json!([ids[2]]), "{reply}");
    assert_eq!(reply["dry_run"], Value::Bool(true));
    assert_eq!(s.revision, revision);
    assert_eq!(curve_ids(&s), ids);
}

#[test]
fn a_path_that_crosses_nothing_is_refused() {
    let (mut s, ids) = three_lines();
    for p in [json!({"points": [[1, 20], [9, 20]]}), json!({"points": [[5, 5]]}), json!({"points": "x"})] {
        assert!(s.execute("sketch.erase", &p).is_err(), "{p} was accepted");
    }
    assert_eq!(curve_ids(&s), ids);
}
