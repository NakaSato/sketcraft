//! Push/Pull, Offset, Move, Scale and Paint through the viewport.
use super::*;

#[test]
fn push_pull_selected_single_face_group_without_opening_it() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    let group = h.app.doc.model.make_object(
        &h.app.doc.model.selectable_entities(), false, "Panel",
    ).unwrap();
    h.app.selected = BTreeSet::from([group]);
    let face = *h.app.doc.model.faces.keys().next().unwrap();
    h.key(Key::P);
    assert_eq!(h.app.selected_modeling_face(), Some(face));
    let before = h.app.doc.model.clone();
    let at = h.at(DVec3::new(2., 2., 0.));
    h.click(at);
    assert_eq!(h.app.modeling.as_ref().unwrap().face, Some(face));
    assert_eq!(h.app.doc.model.edit_context, None);
    h.frame(vec![egui::Event::PointerMoved(h.at(DVec3::new(2., 2., 1.)))], egui::Modifiers::NONE);
    assert!(h.app.modeling.as_ref().unwrap().preview.as_ref().unwrap()
        .object_contents(group).unwrap().faces.len() > 1);
    assert_eq!(h.app.doc.model, before);
    h.key(Key::Escape);
    assert_eq!(h.app.doc.model, before);
    assert_eq!(h.app.selected, BTreeSet::from([group]));
    h.click(at);
    modeling_number(&mut h, "2");
    assert_eq!(h.app.doc.model.object_contents(group).unwrap().faces.len(), 6);
    assert_eq!(h.app.doc.model.edit_context, None);
    assert_eq!(h.app.selected, BTreeSet::from([group]));
    assert_eq!(h.app.selected_modeling_face(), None, "Solid groups need explicit face editing");
    h.app.doc.model.validate().unwrap();
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
}

#[test]
fn push_pull_pointer_preview_numeric_commit_cancel_and_undo() {
    let mut h = Harness::new();
    let face = modeling_rectangle(&mut h);
    let original = h.app.doc.model.clone();
    h.key(Key::P);
    let at = h.at(DVec3::new(2., 2., 0.));
    h.click(at);
    assert!(h.app.modeling.is_some());
    let end = h.at(DVec3::new(2., 2., 2.));
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    assert_eq!(
        h.app.doc.model, original,
        "Preview must not change saved geometry"
    );
    let edit = h.app.modeling.as_ref().unwrap();
    assert!(edit.preview.as_ref().unwrap().faces.len() > 1);
    modeling_number(&mut h, "2");
    assert!(h.app.modeling.is_none());
    assert_eq!(h.app.doc.model.faces.len(), 6);
    assert_eq!(h.app.doc.history.undo_label(), Some("Push/Pull"));
    assert_ne!(h.app.selected, BTreeSet::from([face]));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, original);
    h.click(at);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    h.key(Key::Escape);
    assert!(h.app.modeling.is_none());
    assert_eq!(h.app.doc.model, original);
    assert!(h.app.doc.history.undo_label().is_none());
}

#[test]
fn push_pull_locked_axes_constrain_pointer_previews_and_typed_commits() {
    for (axis, key, input, distance) in [
        (Axis::X, Key::ArrowRight, "-2m", -2.),
        (Axis::Y, Key::ArrowLeft, "2m", 2.),
        (Axis::Z, Key::ArrowUp, "2m", 2.),
    ] {
        let mut h = Harness::new();
        let points = [
            DVec3::new(1., 1., 3.),
            DVec3::new(4., 1., 0.),
            DVec3::new(4., 4., -3.),
            DVec3::new(1., 4., 0.),
        ];
        for i in 0..4 {
            h.app
                .doc
                .model
                .add_edge(points[i], points[(i + 1) % 4])
                .unwrap();
        }
        h.app.doc.snap_to_geometry = false;
        h.app.doc.snap_to_axes = false;
        let before = h.app.doc.model.clone();
        h.key(Key::P);
        h.key(key);
        assert_eq!(h.app.lock, Some(axis));
        h.click(h.at(DVec3::new(2.5, 2.5, 0.)));
        let anchor = h.app.modeling.as_ref().unwrap().anchor;
        h.frame(
            vec![egui::Event::PointerMoved(
                h.at(anchor + axis.vector() * 1.5),
            )],
            egui::Modifiers::NONE,
        );
        let edit = h.app.modeling.as_ref().unwrap();
        assert_eq!(edit.direction, axis.vector());
        assert!((edit.value - 1.5).abs() < 1e-5);
        let check = |model: &Model, delta: DVec3| {
            let cap = model
                .faces
                .values()
                .find(|face| {
                    !before.faces.contains_key(&face.id)
                        && face.vertices.iter().all(|id| {
                            points.iter().any(|point| {
                                model.vertices[id].position.distance(*point + delta) < 1e-5
                            })
                        })
                })
                .unwrap();
            assert_eq!(cap.vertices.len(), 4);
            model.validate().unwrap();
        };
        check(edit.preview.as_ref().unwrap(), axis.vector() * 1.5);
        assert_eq!(h.app.doc.model, before);
        // Clicking commits the same directional extrusion as the preview.
        h.click(h.at(anchor + axis.vector() * 1.5));
        check(&h.app.doc.model, axis.vector() * 1.5);
        h.app.undo(&h.ctx, false);
        assert_eq!(h.app.doc.model, before);
        h.click(h.at(DVec3::new(2.5, 2.5, 0.)));
        h.app.length_input = input.into();
        h.app.update_modeling(h.at(anchor + axis.vector()));
        check(
            h.app.modeling.as_ref().unwrap().preview.as_ref().unwrap(),
            axis.vector() * distance,
        );
        h.key(Key::Enter);
        assert!(h.app.modeling.is_none());
        check(&h.app.doc.model, axis.vector() * distance);
        assert_eq!(h.app.doc.history.undo_label(), Some("Push/Pull"));
        h.app.undo(&h.ctx, false);
        assert_eq!(h.app.doc.model, before);
    }
}

#[test]
fn push_pull_switches_and_clears_axis_locks_during_a_pull() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    h.key(Key::P);
    h.click(h.at(DVec3::new(2., 2., 0.)));
    let normal_direction = h.app.modeling.as_ref().unwrap().direction;
    let before = h.app.doc.model.clone();
    h.app.length_input = "2m".into();
    for (key, axis) in [(Key::ArrowRight, Axis::X), (Key::ArrowLeft, Axis::Y)] {
        h.key(Key::ArrowDown);
        h.key(key);
        let edit = h.app.modeling.as_ref().unwrap();
        assert_eq!(h.app.lock, Some(axis));
        assert_eq!(edit.direction, axis.vector());
        assert!(
            edit.preview.is_none(),
            "A parallel lock must not extrude along the normal"
        );
        assert!(edit.error.unwrap().contains("parallel"));
        h.key(Key::Enter);
        assert!(h.app.modeling.is_some());
        assert_eq!(h.app.doc.model, before);
        assert!(h.app.doc.history.undo_label().is_none());
    }
    h.key(Key::ArrowDown);
    h.key(Key::ArrowUp);
    assert_eq!(h.app.modeling.as_ref().unwrap().direction, DVec3::Z);
    assert!(h.app.modeling.as_ref().unwrap().preview.is_some());
    h.key(Key::ArrowUp);
    assert_eq!(h.app.lock, None);
    assert_eq!(h.app.modeling.as_ref().unwrap().direction, normal_direction);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert_eq!(h.app.lock, None);
    assert_eq!(h.app.modeling.as_ref().unwrap().direction, normal_direction);
    // Toolbar locks must refresh even while the pointer is outside the viewport.
    h.frame(
        vec![egui::Event::PointerMoved(egui::pos2(2., 2.))],
        egui::Modifiers::NONE,
    );
    h.app.set_axis_lock(Some(Axis::X), &h.ctx);
    assert_eq!(h.app.modeling.as_ref().unwrap().direction, DVec3::X);
    assert!(h.app.modeling.as_ref().unwrap().preview.is_none());
    h.key(Key::Enter);
    assert_eq!(h.app.doc.model, before);
    assert!(h.app.modeling.is_some());
    h.app.set_axis_lock(Some(Axis::Z), &h.ctx);
    h.key(Key::Enter);
    assert!(h.app.modeling.is_none());
    h.app.doc.model.validate().unwrap();
}

#[test]
fn offset_then_push_down_previews_and_commits_an_open_recess() {
    let mut h = Harness::new();
    let rim = modeling_rectangle(&mut h);
    h.key(Key::F);
    let offset_at = h.at(DVec3::new(2., 0.25, 0.));
    h.click(offset_at);
    modeling_number(&mut h, "0.5");
    let before = h.app.doc.model.clone();
    assert_eq!(before.faces[&rim].holes.len(), 1);

    h.key(Key::P);
    let at = h.at(DVec3::new(2., 2., 0.));
    h.click(at);
    let inner = h.app.modeling.as_ref().unwrap().face.unwrap();
    let end = h.at(DVec3::new(2., 2., -1.));
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    let preview = h.app.modeling.as_ref().unwrap().preview.as_ref().unwrap();
    assert!(!preview.faces.contains_key(&inner));
    let ray = ketchup_core::Ray {
        origin: DVec3::new(2., 2., 10.),
        direction: -DVec3::Z,
    };
    let (_, _, point) = preview.nearest_face(ray).unwrap();
    assert!(point.distance(DVec3::new(2., 2., -1.)) < 1e-6);
    assert_eq!(h.app.doc.model, before);
    h.key(Key::Escape);
    assert_eq!(h.app.doc.model, before);

    h.click(at);
    modeling_number(&mut h, "-1");
    assert!(h.app.modeling.is_none());
    assert!(!h.app.doc.model.faces.contains_key(&inner));
    assert_eq!(h.app.doc.model.faces.len(), 6);
    let (floor, _, point) = h.app.doc.model.nearest_face(ray).unwrap();
    assert!(point.distance(DVec3::new(2., 2., -1.)) < 1e-6);
    assert_eq!(h.app.selected, BTreeSet::from([floor]));
    assert_eq!(h.app.doc.history.undo_label(), Some("Push/Pull"));
    h.app.doc.model.validate().unwrap();
    let after = h.app.doc.model.clone();
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
    h.app.undo(&h.ctx, true);
    assert_eq!(h.app.doc.model, after);
}

#[test]
fn move_and_scale_real_pointer_clicks_and_axis_locked_numeric_distance() {
    let mut h = Harness::new();
    let face = modeling_rectangle(&mut h);
    h.app.selected = BTreeSet::from([face]);
    h.key(Key::M);
    let start = h.at(DVec3::new(2., 2., 0.));
    h.click(start);
    h.key(Key::ArrowUp);
    let end = h.at(DVec3::new(2., 2., 2.));
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    modeling_number(&mut h, "3");
    assert!(
        h.app
            .doc
            .model
            .vertices
            .values()
            .all(|v| (v.position.z - 3.).abs() < 1e-6)
    );
    assert_eq!(h.app.doc.history.undo_label(), Some("Move"));
    h.key(Key::S);
    let reference = h.at(DVec3::new(3., 3., 3.));
    h.click(reference);
    modeling_number(&mut h, "0.5");
    assert!((h.app.doc.model.face_area(&h.app.doc.model.faces[&face]) - 4.).abs() < 1e-7);
    assert_eq!(h.app.doc.history.undo_label(), Some("Scale"));
    h.key(Key::M);
    let start = h.at(DVec3::new(2., 2., 3.));
    h.click(start);
    let end = h.at(DVec3::new(4., 2., 3.));
    h.click(end);
    assert!(h.app.modeling.is_none());
    let center = h.app.doc.model.selection_center(&h.app.selected).unwrap();
    assert!((center - DVec3::new(4., 2., 3.)).length() < 1e-6);
}

#[test]
fn offset_and_paint_through_viewport_preserve_history_and_saved_color() {
    let mut h = Harness::new();
    let face = modeling_rectangle(&mut h);
    h.key(Key::F);
    let at = h.at(DVec3::new(2., 0.25, 0.));
    h.click(at);
    modeling_number(&mut h, "0.5");
    assert!(h.app.modeling.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 8);
    assert_eq!(h.app.doc.history.undo_label(), Some("Offset"));
    h.key(Key::B);
    assert!(h.app.inspector);
    h.app.paint_color = [45, 103, 188];
    let inside = h.at(DVec3::new(2., 2., 0.));
    h.click(inside);
    let colors = h.app.doc.model.face_colors.clone();
    assert_eq!(colors.len(), 1);
    assert_eq!(colors.values().next(), Some(&[45, 103, 188]));
    assert_eq!(h.app.doc.history.undo_label(), Some("Paint face"));
    h.app.undo(&h.ctx, false);
    assert!(h.app.doc.model.face_colors.is_empty());
    h.app.undo(&h.ctx, true);
    assert_eq!(h.app.doc.model.face_colors, colors);
    h.app.doc.model.locked.insert(face);
    h.app.paint_color = [0, 0, 0];
    let border = h.at(DVec3::new(2., 0.25, 0.));
    h.click(border);
    assert!(
        !h.app
            .doc
            .model
            .face_colors
            .values()
            .any(|c| *c == [0, 0, 0])
    );
}

#[test]
fn move_preview_and_commit_combine_two_reference_axes_and_undo_exactly() {
    let mut h = Harness::new();
    let start = DVec3::new(-4., -3., 2.);
    let target = DVec3::new(3., 4., 2.);
    let moving = h
        .app
        .doc
        .model
        .add_edge(start, start + DVec3::X * 0.5)
        .unwrap();
    let vertex = h.app.doc.model.edges[&moving].a;
    for reference in [DVec3::new(0., 4., 2.), DVec3::new(3., 0., 2.)] {
        h.app
            .doc
            .model
            .add_edge(reference, reference + DVec3::new(0.31, 0.27, 0.19))
            .unwrap();
    }
    h.app.selected = BTreeSet::from([moving]);
    let before = h.app.doc.model.clone();
    h.key(Key::M);
    let anchor = h.at(start);
    h.click(anchor);
    let cursor = h.at(target) + egui::vec2(2., -2.);
    h.frame(
        vec![egui::Event::PointerMoved(cursor)],
        egui::Modifiers::NONE,
    );
    let edit = h.app.modeling.as_ref().unwrap();
    assert_eq!(edit.guides.len(), 2);
    assert!(edit.guides.iter().any(|g| g.axis == Axis::X));
    assert!(edit.guides.iter().any(|g| g.axis == Axis::Y));
    assert!((edit.preview.as_ref().unwrap().vertices[&vertex].position - target).length() < 1e-7);
    assert_eq!(h.app.doc.model, before);
    h.click(cursor);
    assert!(h.app.modeling.is_none());
    assert!((h.app.doc.model.vertices[&vertex].position - target).length() < 1e-7);
    assert_eq!(h.app.doc.history.undo_label(), Some("Move"));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
}

#[test]
fn push_pull_snaps_to_another_groups_edge_and_honors_preferences() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    let source = h
        .app
        .doc
        .model
        .make_object(&h.app.doc.model.selectable_entities(), false, "Source")
        .unwrap();
    let a = DVec3::new(5., 0., 2.5);
    let b = DVec3::new(5., 4., 2.5);
    let edge = h.app.doc.model.add_edge(a, b).unwrap();
    let target_group = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Target")
        .unwrap();
    h.app.doc.model.edit_context = Some(source);
    h.app.set_tool(Tool::PushPull);
    h.app.doc.snap_to_axes = false;
    h.app.begin_modeling(h.at(DVec3::new(2., 2., 0.)), &h.ctx);
    assert!(h.app.modeling.is_some());
    let target = a.lerp(b, 0.3); // Neither endpoint nor midpoint; off the pull axis.
    let at = h.at(target);
    h.app.update_modeling(at);
    let edit = h.app.modeling.as_ref().unwrap();
    assert_eq!(edit.snap.unwrap().kind, Inference::OnEdge);
    assert!((edit.value.abs() - 2.5).abs() < 1e-7);
    let preview = edit.preview.as_ref().unwrap();
    assert!(
        preview
            .vertices
            .values()
            .any(|v| (v.position.z - 2.5).abs() < 1e-7 && v.position.x <= 4.)
    );

    h.app.snap_to_edge = false;
    h.app.update_modeling(at);
    assert!(h.app.modeling.as_ref().unwrap().snap.is_none());
    h.app.snap_to_edge = true;
    h.app.doc.snap_to_geometry = false;
    h.app.update_modeling(at);
    assert!(h.app.modeling.as_ref().unwrap().snap.is_none());
    h.app.doc.snap_to_geometry = true;
    h.app.doc.model.hidden.insert(target_group);
    h.app.update_modeling(at);
    assert!(h.app.modeling.as_ref().unwrap().snap.is_none());
    h.app.doc.model.hidden.remove(&target_group);
    h.app.update_modeling(at);
    assert_eq!(
        h.app.modeling.as_ref().unwrap().snap.unwrap().kind,
        Inference::OnEdge
    );
    h.app.commit_modeling(&h.ctx);
    assert!(h.app.modeling.is_none());
    assert!(
        h.app
            .doc
            .model
            .vertices
            .values()
            .any(|v| (v.position.z - 2.5).abs() < 1e-7 && v.position.x <= 4.)
    );
    assert_eq!(h.app.doc.history.undo_label(), Some("Push/Pull"));
}

#[test]
fn push_pull_face_snapping_yields_to_typed_distances() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    let points = [
        DVec3::new(5., 0., 3.),
        DVec3::new(9., 0., 3.),
        DVec3::new(9., 4., 3.),
        DVec3::new(5., 4., 3.),
    ];
    for i in 0..4 {
        h.app
            .doc
            .model
            .add_edge(points[i], points[(i + 1) % 4])
            .unwrap();
    }
    h.app.set_tool(Tool::PushPull);
    h.app.begin_modeling(h.at(DVec3::new(2., 2., 0.)), &h.ctx);
    let at = h.at(DVec3::new(6.5, 1.5, 3.));
    h.app.update_modeling(at);
    assert_eq!(
        h.app.modeling.as_ref().unwrap().snap.unwrap().kind,
        Inference::OnFace
    );
    assert!((h.app.modeling.as_ref().unwrap().value.abs() - 3.).abs() < 1e-7);
    h.app.snap_to_face = false;
    h.app.update_modeling(at);
    assert!(h.app.modeling.as_ref().unwrap().snap.is_none());
    h.app.snap_to_face = true;
    h.app.length_input = "1m".into();
    h.app.update_modeling(at);
    assert!(h.app.modeling.as_ref().unwrap().snap.is_none());
    h.app.commit_modeling(&h.ctx);
    let cap = h.app.selected.iter().next().unwrap();
    assert!(
        h.app.doc.model.faces[cap]
            .vertices
            .iter()
            .all(|id| (h.app.doc.model.vertices[id].position.z - 1.).abs() < 1e-7)
    );
}

#[test]
fn push_pull_press_drag_release_pulls_the_face_in_one_gesture() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    h.app.set_tool(Tool::PushPull);
    let primary = egui::PointerButton::Primary;
    let start = h.at(DVec3::new(2., 2., 0.));
    let end = h.at(DVec3::new(2., 2., 1.));
    h.pointer(start, primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(start.lerp(end, 0.5))], egui::Modifiers::NONE);
    assert!(h.app.modeling.is_some(), "dragging starts Push/Pull");
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    h.pointer(end, primary, false, false);
    assert!(h.app.modeling.is_none(), "releasing commits Push/Pull");
    assert!(
        h.app.doc.model.vertices.values().any(|v| v.position.z > 0.5),
        "the face was pulled up"
    );

    // A click without dragging still starts the click-move-click flow.
    let before = h.app.doc.model.vertices.len();
    h.click(start);
    assert!(h.app.modeling.is_some());
    h.key(Key::Escape);
    assert!(h.app.modeling.is_none());
    assert_eq!(h.app.doc.model.vertices.len(), before);
}

#[test]
fn push_pull_grid_steps_preserve_normal_and_yield_to_typed_distance() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    h.app.doc.display.grid_auto_size = false;
    h.app.doc.display.grid_spacing = 0.5;
    h.app.set_tool(Tool::PushPull);
    h.app.begin_modeling(h.at(DVec3::new(2., 2., 0.)), &h.ctx);
    let target = h.at(DVec3::new(2., 2., 1.37));
    h.app.update_modeling(target);
    let free = h.app.modeling.as_ref().unwrap().value;
    h.app.snap_to_grid = true;
    h.app.update_modeling(target);
    let edit = h.app.modeling.as_ref().unwrap();
    assert!((edit.value - (free / 0.5).round() * 0.5).abs() < 1e-7);
    assert!(edit.direction.cross(DVec3::Z).length() < 1e-7);
    h.app.snap_to_grid = false;
    h.app.update_modeling(target);
    assert!((h.app.modeling.as_ref().unwrap().value - free).abs() < 1e-7);
    h.app.snap_to_grid = true;
    h.app.length_input = "1.37m".into();
    h.app.update_modeling(target);
    let edit = h.app.modeling.as_ref().unwrap();
    assert!(
        edit.preview
            .as_ref()
            .unwrap()
            .vertices
            .values()
            .any(|v| (v.position.z - 1.37).abs() < 1e-7)
    );
}

#[test]
fn push_pull_axis_references_work_independently_of_object_snapping() {
    let mut h = Harness::new();
    modeling_rectangle(&mut h);
    h.app.set_tool(Tool::PushPull);
    h.app.begin_modeling(h.at(DVec3::new(2., 2., 0.)), &h.ctx);
    h.app.doc.snap_to_geometry = false;
    h.app.snap_to_edge = false;
    h.app.snap_to_face = false;
    h.app.doc.snap_to_axes = true;
    let target = h.at(DVec3::new(0., 0., 2.5));
    h.app.update_modeling(target);
    assert!(matches!(
        h.app.modeling.as_ref().unwrap().snap.unwrap().kind,
        Inference::OnAxis(Axis::Z) | Inference::Axis(Axis::Z)
    ));
    assert!((h.app.modeling.as_ref().unwrap().value.abs() - 2.5).abs() < 1e-6);
    h.app.doc.snap_to_axes = false;
    h.app.update_modeling(target);
    assert!(h.app.modeling.as_ref().unwrap().snap.is_none());
}

#[test]
fn move_hover_previews_the_anchor_used_by_first_click() {
    for (point, kind) in [
        (DVec3::new(2., 2., 0.), Inference::Endpoint),
        (DVec3::new(3., 2., 0.), Inference::OnEdge),
    ] {
        let mut h = Harness::new();
        h.app.doc.model.add_edge(DVec3::new(2., 2., 0.), DVec3::new(6., 2., 0.)).unwrap();
        h.app.tool = Tool::Move;
        h.app.doc.snap_to_geometry = true;
        h.app.snap_to_edge = true;
        let pos = h.at(point);
        h.app.update_pointer(pos);
        let snap = h.app.snap.expect("Move must preview an anchor before clicking");
        assert_eq!(snap.kind, kind);
        // An edge position is reconstructed from f32 screen coordinates.
        // Endpoints remain exact; continuous edge anchors round-trip to the cursor.
        assert!(h.at(snap.point).distance(pos) < 0.001);
        if kind == Inference::Endpoint {
            assert_eq!(snap.point, point);
        }
        assert!(h.app.modeling.is_none());
        h.app.begin_modeling(pos, &h.ctx);
        assert!(h.app.modeling.as_ref().unwrap().anchor.distance(snap.point) < 1e-7);
    }
}
