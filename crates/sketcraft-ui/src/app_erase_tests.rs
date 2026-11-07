//! Eraser hover, sweeps and undo.
use super::*;

#[test]
fn erase_and_freehand_hover_batch_motion_until_a_stroke_starts() {
    for tool in [Tool::Erase, Tool::Freehand] {
        let mut h = Harness::new();
        let edge = h.app.doc.model.add_edge(-DVec3::X, DVec3::X).unwrap();
        let before = h.app.doc.model.clone();
        h.app.set_tool(tool);
        let target = h.at(DVec3::ZERO);
        h.app.picking_preparations = 0;
        h.frame(
            native_motion_burst(target - egui::vec2(100., 50.), target, 400),
            egui::Modifiers::NONE,
        );
        assert!(
            (1..=2).contains(&h.app.picking_preparations),
            "{tool:?}: a motion burst must use bounded picking work across UI layout passes: {}",
            h.app.picking_preparations,
        );
        assert_eq!(h.app.hover, Some(edge), "{tool:?}");
        assert!(h.app.gesture.is_none());
        assert!(h.app.drawing.is_none());
        assert_eq!(h.app.doc.model, before);
    }
}

#[test]
fn erase_stroke_keeps_intermediate_motion_when_the_pointer_returns_to_its_start() {
    let mut h = Harness::new();
    let edge = h.app.doc.model.add_edge(-DVec3::X, DVec3::X).unwrap();
    let before = h.app.doc.model.clone();
    h.app.set_tool(Tool::Erase);
    let start = h.at(-DVec3::Y * 2.);
    let across = h.at(DVec3::Y * 2.);
    h.pointer(start, egui::PointerButton::Primary, true, false);
    let mut events = native_motion_burst(start, across, 10);
    events.extend(native_motion_burst(across, start, 10));
    h.frame(events, egui::Modifiers::NONE);
    let GestureKind::Erase { ids } = &h.app.gesture.as_ref().unwrap().kind else {
        panic!("The erase stroke should remain active until release");
    };
    assert_eq!(ids, &BTreeSet::from([edge]));
    assert_eq!(h.app.doc.model, before);
    h.pointer(start, egui::PointerButton::Primary, false, false);
    assert!(h.app.doc.model.edges.is_empty());
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
}

#[test]
fn eraser_sweep_is_one_undo_step_and_ui_panels_never_draw() {
    let mut h = Harness::new();
    h.app
        .doc
        .model
        .add_edge(-DVec3::X * 2., DVec3::X * 2.)
        .unwrap();
    h.app
        .doc
        .model
        .add_edge(DVec3::Y - DVec3::X * 2., DVec3::Y + DVec3::X * 2.)
        .unwrap();
    h.key(Key::E);
    let a = h.at(-DVec3::Y);
    let b = h.at(DVec3::Y * 2.);
    h.pointer(a, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(b)], egui::Modifiers::NONE);
    h.pointer(b, egui::PointerButton::Primary, false, false);
    assert!(h.app.doc.model.edges.is_empty());
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edges.len(), 2);
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edges.len(), 2);
    h.app.set_tool(Tool::Line);
    h.click(egui::pos2(1300., 500.));
    assert!(h.app.line_start.is_none());
}

#[test]
fn eraser_hovers_and_erases_face_interiors_without_removing_boundary_edges() {
    let mut h = Harness::new();
    let face = modeling_rectangle(&mut h);
    let before = h.app.doc.model.clone();
    h.key(Key::E);
    let cursor = h.at(DVec3::new(2., 2., 0.));
    h.frame(
        vec![egui::Event::PointerMoved(cursor)],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.hover, Some(face));
    h.click(cursor);
    assert!(!h.app.doc.model.faces.contains_key(&face));
    assert_eq!(h.app.doc.model.edges, before.edges);
    assert_eq!(h.app.doc.model.vertices, before.vertices);
    assert_eq!(h.app.doc.history.undo_label(), Some("Erase geometry"));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
    h.app.undo(&h.ctx, true);
    assert!(!h.app.doc.model.faces.contains_key(&face));
    assert_eq!(h.app.doc.model.edges, before.edges);
}

#[test]
fn eraser_stroke_within_a_face_previews_and_commits_one_undo_step() {
    let mut h = Harness::new();
    let face = modeling_rectangle(&mut h);
    let before = h.app.doc.model.clone();
    h.key(Key::E);
    let start = h.at(DVec3::new(1., 2., 0.));
    let end = h.at(DVec3::new(3., 2., 0.));
    h.pointer(start, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    let GestureKind::Erase { ids } = &h.app.gesture.as_ref().unwrap().kind else {
        panic!("The erase stroke should remain active until release");
    };
    assert_eq!(ids, &BTreeSet::from([face]));
    assert_eq!(h.app.doc.model, before);
    h.pointer(end, egui::PointerButton::Primary, false, false);
    assert!(h.app.doc.model.faces.is_empty());
    assert_eq!(h.app.doc.model.edges, before.edges);
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
    assert!(h.app.doc.history.undo_label().is_none());
}

#[test]
fn eraser_face_targets_respect_hidden_and_locked_objects() {
    for grouped in [false, true] {
        for hidden in [false, true] {
            let mut h = Harness::new();
            let face = modeling_rectangle(&mut h);
            let target = if grouped {
                h.app
                    .doc
                    .model
                    .make_object(&BTreeSet::from([face]), false, "Group")
                    .unwrap()
            } else {
                face
            };
            if hidden {
                h.app.doc.model.hidden.insert(target);
            } else {
                h.app.doc.model.locked.insert(target);
            }
            let before = h.app.doc.model.clone();
            h.key(Key::E);
            let cursor = h.at(DVec3::new(2., 2., 0.));
            h.frame(
                vec![egui::Event::PointerMoved(cursor)],
                egui::Modifiers::NONE,
            );
            assert_eq!(h.app.hover, None);
            h.click(cursor);
            assert_eq!(h.app.doc.model, before);
            assert!(h.app.doc.history.undo_label().is_none());
        }
    }
}

#[test]
fn eraser_does_not_delete_an_edge_hidden_behind_a_filled_face() {
    let mut h = Harness::new();
    let square = [
        DVec3::new(-3., -3., 2.),
        DVec3::new(3., -3., 2.),
        DVec3::new(3., 3., 2.),
        DVec3::new(-3., 3., 2.),
    ];
    for i in 0..4 {
        h.app
            .doc
            .model
            .add_edge(square[i], square[(i + 1) % 4])
            .unwrap();
    }
    let cursor = h.at(DVec3::Z * 2.);
    let hidden = h
        .app
        .doc
        .camera
        .ray(dv(cursor - h.app.viewport.min), dv(h.app.viewport.size()))
        .plane_z(0.)
        .unwrap();
    let hidden_edge = h
        .app
        .doc
        .model
        .add_edge(hidden - DVec3::X, hidden + DVec3::X)
        .unwrap();
    let before = h.app.doc.model.clone();
    h.key(Key::E);
    h.click(cursor);
    assert!(h.app.doc.model.faces.is_empty());
    assert!(h.app.doc.model.edges.contains_key(&hidden_edge));
    assert_eq!(h.app.doc.model.edges, before.edges);
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
}
