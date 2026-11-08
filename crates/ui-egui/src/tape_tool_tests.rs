//! SketchUp's Tape Measure on the SketchUp tab (`tape_tool`): measuring, and guide lines and
//! points as construction axes and points.

use super::*;
use crate::scenario::Harness;
use sketcraft_engine::doc::FeatureKind;

fn sketchup_tab() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.ui.tab = crate::workspace::SKETCHUP_TAB.into();
    h.app.run("solid.box", json!({"length": 20, "width": 10, "height": 5, "corner": [0, 0, 0]})).unwrap();
    h
}

fn construction(h: &Harness) -> usize {
    h.app.session.doc.features.iter().filter(|f| matches!(f.kind, FeatureKind::ConstructionAxis { .. } | FeatureKind::ConstructionPoint { .. })).count()
}

fn type_keys(h: &mut Harness, keys: &[egui::Key]) {
    for k in keys {
        assert!(key(&mut h.app, *k, egui::Modifiers::NONE), "{k:?} is taken");
    }
}

#[test]
fn measure_on_the_sketchup_tab_opens_the_tape_and_other_tabs_keep_measure() {
    let mut h = sketchup_tab();
    h.app.start("inspect.measure");
    assert!(active(&h.app));
    assert!(key(&mut h.app, egui::Key::Escape, egui::Modifiers::NONE));
    assert!(!active(&h.app), "Escape with nothing started closes it");
    h.app.ui.tab = "INSPECT".into();
    h.app.start("inspect.measure");
    assert!(!active(&h.app), "Measure stays Measure elsewhere");
}

#[test]
fn a_typed_distance_off_an_edge_makes_a_parallel_guide_line() {
    let mut h = sketchup_tab();
    h.app.start("inspect.measure");
    let undo = h.app.session.undo.len();
    let before = construction(&h);
    {
        let t = h.app.tape_tool.as_mut().unwrap();
        // The bottom front edge, along X; the pointer off it towards +Y.
        t.from = Some(From::Edge { at: Vec3::new(5.0, 0.0, 0.0), dir: Vec3::new(1.0, 0.0, 0.0) });
        t.pointer = Some(Vec3::new(8.0, 3.0, 0.0));
    }
    type_keys(&mut h, &[egui::Key::Num7, egui::Key::Enter]);
    assert_eq!(construction(&h), before + 1, "one guide line");
    assert_eq!(h.app.session.undo.len(), undo + 1, "one undo step");
    let t = h.app.tape_tool.as_ref().expect("the tape stays open");
    assert!(t.from.is_none() && t.typed.is_empty());
}

#[test]
fn from_a_vertex_the_guide_is_a_point_and_ctrl_turns_guides_off() {
    let mut h = sketchup_tab();
    h.app.start("inspect.measure");
    let before = construction(&h);
    let start = |h: &mut Harness| {
        let t = h.app.tape_tool.as_mut().unwrap();
        t.from = Some(From::Vertex { at: Vec3::ZERO });
        t.pointer = Some(Vec3::new(0.0, 0.0, 10.0));
    };
    start(&mut h);
    type_keys(&mut h, &[egui::Key::Num4, egui::Key::Enter]);
    assert_eq!(construction(&h), before + 1, "one guide point");
    // Guides off: the tape only measures.
    h.app.tape_tool.as_mut().unwrap().guides = false;
    start(&mut h);
    type_keys(&mut h, &[egui::Key::Num4, egui::Key::Enter]);
    assert_eq!(construction(&h), before + 1, "no guide with guides off");
}

#[test]
fn letters_typed_into_a_length_are_not_shortcuts() {
    let mut h = sketchup_tab();
    h.app.start("inspect.measure");
    h.app.tape_tool.as_mut().unwrap().from = Some(From::Point { at: Vec3::ZERO });
    type_keys(&mut h, &[egui::Key::Num2, egui::Key::Period, egui::Key::Num5, egui::Key::M]);
    assert_eq!(h.app.tape_tool.as_ref().unwrap().typed, "2.5m");
    assert!(h.app.dialog.is_none(), "M did not start Move");
    // Before the first click, keys are shortcuts as usual.
    h.app.tape_tool.as_mut().unwrap().from = None;
    assert!(!key(&mut h.app, egui::Key::M, egui::Modifiers::NONE));
}

#[test]
fn the_guide_offset_is_the_part_across_the_edge() {
    let v = guide_offset(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(7.0, 3.0, -4.0));
    assert!((v.x).abs() < 1e-12 && (v.y - 3.0).abs() < 1e-12 && (v.z + 4.0).abs() < 1e-12);
}
