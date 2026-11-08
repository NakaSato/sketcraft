//! SketchUp's double-click with Push/Pull (`press_pull_repeat`): a flat face double-clicked while
//! Push/Pull is open moves by the last Push/Pull's distance, in one undo step.

use crate::scenario::Harness;
use serde_json::json;

/// A 20 x 20 x 10 box on the ground, seen whole from the home view.
fn a_box() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("solid.box", json!({"length":20, "width":20, "height":10, "corner":[0, 0, 0]})).unwrap();
    h.app.run("select.clear", json!({})).unwrap();
    view(&mut h, "home");
    h
}

fn view(h: &mut Harness, name: &str) {
    h.step(&json!({"call":"ui.view","params":{"view":name}})).unwrap();
    h.step(&json!({"call":"ui.view","params":{"view":"fit"}})).unwrap();
    h.settle();
}

fn bounds_z(h: &Harness) -> (f64, f64) {
    let st = h.app.session.world_state().unwrap();
    let b = st.bodies.last().expect("a body").mesh().bounds();
    (b.min.z, b.max.z)
}

fn volume(h: &Harness) -> f64 {
    let st = h.app.session.world_state().unwrap();
    st.bodies.iter().map(|b| sketcraft_engine::doc::kernel::measure(&b.body).unwrap().volume).sum()
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

/// The top pulled up 5 by a typed distance, then the bottom (parallel to it) double-clicked with
/// Push/Pull open: it is pulled down 5 too, Push/Pull stays, and one Undo takes back only that.
#[test]
fn a_double_click_repeats_the_last_push_pull_on_another_face() {
    let mut h = a_box();
    h.step(&json!({"start":"solid.press_pull"})).unwrap();
    h.step(&json!({"click":{"world":[10, 10, 10]}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.dialog.as_ref().unwrap().inputs[0].items.len(), 1, "the top is picked");
    h.step(&json!({"text":"5"})).unwrap();
    h.step(&json!({"key":"Enter"})).unwrap();
    h.settle();
    assert!(h.app.dialog.is_none(), "OK applied the Push/Pull");
    assert!(near(bounds_z(&h), (0.0, 15.0)), "the top went up 5: {:?}", bounds_z(&h));
    assert!((volume(&h) - 6000.0).abs() < 1e-3, "{}", volume(&h));
    let kept = h.app.preferences.press_pull_distance.expect("the distance is remembered");
    assert!((kept - 5.0).abs() < 1e-9, "{kept}");

    // Push/Pull again, seen from below: the bottom face is double-clicked.
    h.app.run("select.clear", json!({})).unwrap();
    view(&mut h, "bottom");
    h.step(&json!({"start":"solid.press_pull"})).unwrap();
    h.frames(2);
    assert!(crate::press_pull_repeat::active(&h.app), "Push/Pull is open");
    let features = h.app.session.doc.features.len();
    h.step(&json!({"click":{"world":[10, 10, 0]}, "double": true})).unwrap();
    h.settle();
    assert!(near(bounds_z(&h), (-5.0, 15.0)), "the bottom went down 5: {:?}", bounds_z(&h));
    assert!((volume(&h) - 8000.0).abs() < 1e-3, "{}", volume(&h));
    assert_eq!(h.app.session.doc.features.len(), features + 1, "one new feature");
    assert!(crate::press_pull_repeat::active(&h.app), "Push/Pull stays open");
    assert!(h.app.dialog.as_ref().is_some_and(|d| d.inputs[0].items.is_empty()), "waiting for the next face");
    let status = h.app.status.as_ref().map(|s| s.0.clone()).unwrap_or_default();
    assert!(status.contains("Press Pull repeated") && status.contains('5'), "{status}");

    // One Undo takes back the repeat only.
    crate::dialogs::cancel(&mut h.app);
    h.app.run("edit.undo", json!({})).unwrap();
    h.settle();
    assert!(near(bounds_z(&h), (0.0, 15.0)), "undo kept the first Push/Pull: {:?}", bounds_z(&h));
    assert!((volume(&h) - 6000.0).abs() < 1e-3, "{}", volume(&h));
}

/// With no distance remembered a double-click is only a click: nothing is pushed or pulled.
#[test]
fn a_double_click_with_no_distance_kept_moves_nothing() {
    let mut h = a_box();
    assert_eq!(h.app.preferences.press_pull_distance, None);
    h.step(&json!({"start":"solid.press_pull"})).unwrap();
    h.frames(2);
    let features = h.app.session.doc.features.len();
    h.step(&json!({"click":{"world":[10, 10, 10]}, "double": true})).unwrap();
    h.settle();
    assert_eq!(h.app.session.doc.features.len(), features, "no feature");
    assert!(near(bounds_z(&h), (0.0, 10.0)), "{:?}", bounds_z(&h));
    assert_eq!(h.app.preferences.press_pull_distance, None);
}

/// A `solid.press_pull` run keeps its distance too, a push into the face as a negative one.
#[test]
fn a_press_pull_run_keeps_its_distance() {
    let mut h = a_box();
    h.app.run("solid.press_pull", json!({"face":[10, 10, 10], "distance":"-3 mm"})).unwrap();
    assert!(near(bounds_z(&h), (0.0, 7.0)), "{:?}", bounds_z(&h));
    let kept = h.app.preferences.press_pull_distance.expect("the distance is remembered");
    assert!((kept + 3.0).abs() < 1e-9, "{kept}");
}
