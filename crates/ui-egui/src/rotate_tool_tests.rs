//! SketchUp's Rotate on the SketchUp tab (`rotate_tool`): a protractor placed by clicks, turned
//! by a click or a typed angle, one `solid.rotate` each.

use super::*;
use crate::scenario::Harness;
use serde_json::json;

/// A 20 x 10 x 5 box with a corner at the origin, selected, on the SketchUp tab.
fn a_box() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.ui.tab = crate::workspace::SKETCHUP_TAB.into();
    h.app.run("solid.box", json!({"length": 20, "width": 10, "height": 5, "corner": [0, 0, 0]})).unwrap();
    let name = h.app.session.world_state().unwrap().bodies.last().expect("a body").name.clone();
    h.app.run("select.set", json!({"items": [{"type": "body", "name": name}]})).unwrap();
    h
}

fn bounds(h: &Harness) -> (Vec3, Vec3) {
    let st = h.app.session.world_state().unwrap();
    let b = st.bodies.last().expect("a body").mesh().bounds();
    (b.min, b.max)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Place the protractor at the origin about Z, starting along X, as two clicks would.
fn placed(h: &mut Harness) {
    let t = h.app.rotate_tool.as_mut().expect("Rotate is open");
    t.center = Some((Vec3::ZERO, Z));
    t.start = Some(X);
}

#[test]
fn rotate_on_the_sketchup_tab_opens_the_protractor_and_not_move_copy() {
    let mut h = a_box();
    h.app.start("solid.rotate");
    assert!(active(&h.app), "the protractor is open");
    assert!(h.app.dialog.is_none(), "no Move/Copy window on the SketchUp tab");
    assert_eq!(h.app.rotate_tool.as_ref().unwrap().bodies.len(), 1, "the selected body is the one to turn");
    // Escape with nothing placed closes it.
    assert!(key(&mut h.app, egui::Key::Escape, egui::Modifiers::NONE));
    assert!(!active(&h.app));
}

#[test]
fn rotate_on_another_tab_keeps_move_copy() {
    let mut h = a_box();
    h.app.ui.tab = "MODEL".into();
    h.app.start("solid.rotate");
    assert!(!active(&h.app));
    assert!(h.app.dialog.is_some(), "Move/Copy opens on its Rotate type");
}

#[test]
fn a_typed_angle_turns_the_body_about_the_protractor_in_one_undo_step() {
    let mut h = a_box();
    h.app.start("solid.rotate");
    placed(&mut h);
    let undo = h.app.session.undo.len();
    for k in [egui::Key::Num9, egui::Key::Num0] {
        assert!(key(&mut h.app, k, egui::Modifiers::NONE), "{k:?} is typed into the angle");
    }
    assert!(key(&mut h.app, egui::Key::Enter, egui::Modifiers::NONE));
    // 20 along X turned a quarter about Z through the origin lies along Y, on the -X side.
    let (min, max) = bounds(&h);
    assert!(near(min.x, -10.0) && near(max.x, 0.0), "{min:?} {max:?}");
    assert!(near(min.y, 0.0) && near(max.y, 20.0), "{min:?} {max:?}");
    assert_eq!(h.app.session.undo.len(), undo + 1, "one undo step");
    // Rotate stays, waiting for the next centre.
    let t = h.app.rotate_tool.as_ref().expect("Rotate stays open");
    assert!(t.center.is_none() && t.start.is_none() && t.typed.is_empty());
}

#[test]
fn escape_steps_back_one_click_at_a_time() {
    let mut h = a_box();
    h.app.start("solid.rotate");
    placed(&mut h);
    assert!(key(&mut h.app, egui::Key::Num4, egui::Modifiers::NONE));
    let esc = |h: &mut Harness| assert!(key(&mut h.app, egui::Key::Escape, egui::Modifiers::NONE));
    esc(&mut h);
    assert!(h.app.rotate_tool.as_ref().unwrap().typed.is_empty(), "the typed angle goes first");
    esc(&mut h);
    assert!(h.app.rotate_tool.as_ref().unwrap().start.is_none(), "then the start direction");
    esc(&mut h);
    assert!(h.app.rotate_tool.as_ref().unwrap().center.is_none(), "then the centre");
    esc(&mut h);
    assert!(!active(&h.app), "then Rotate closes");
}

#[test]
fn the_arrow_keys_lock_the_axis_before_the_centre() {
    let mut h = a_box();
    h.app.start("solid.rotate");
    assert!(key(&mut h.app, egui::Key::ArrowRight, egui::Modifiers::NONE));
    assert_eq!(h.app.rotate_tool.as_ref().unwrap().lock, Some(X));
    assert!(key(&mut h.app, egui::Key::ArrowRight, egui::Modifiers::NONE));
    assert_eq!(h.app.rotate_tool.as_ref().unwrap().lock, None, "a second press unlocks");
    assert!(key(&mut h.app, egui::Key::ArrowUp, egui::Modifiers::NONE));
    assert!(key(&mut h.app, egui::Key::ArrowDown, egui::Modifiers::NONE));
    assert_eq!(h.app.rotate_tool.as_ref().unwrap().lock, None);
    // Digits before the start direction are not an angle: they go to the shortcuts.
    assert!(!key(&mut h.app, egui::Key::Num5, egui::Modifiers::NONE));
}

#[test]
fn copy_keeps_the_original() {
    let mut h = a_box();
    let before = h.app.session.world_state().unwrap().bodies.len();
    h.app.start("solid.rotate");
    placed(&mut h);
    h.app.rotate_tool.as_mut().unwrap().copy = true;
    for k in [egui::Key::Num4, egui::Key::Num5, egui::Key::Enter] {
        assert!(key(&mut h.app, k, egui::Modifiers::NONE));
    }
    assert_eq!(h.app.session.world_state().unwrap().bodies.len(), before + 1);
}

#[test]
fn the_angle_snaps_to_fifteen_degrees_when_close() {
    assert_eq!(snapped(44.0), 45.0);
    assert_eq!(snapped(-89.0), -90.0);
    assert_eq!(snapped(40.0), 40.0);
    assert!(near(turn(X, Y, Z), 90.0));
    assert!(near(turn(X, Y, Vec3::new(0.0, 0.0, -1.0)), -90.0));
}
