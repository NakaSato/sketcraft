//! Line, shape and freehand drawing through the real UI.
use super::*;

#[test]
fn shape_tools_create_faces_and_undo_as_one_operation() {
    for (key, tool, edges) in [
        (Some(Key::R), Tool::Rectangle, 4),
        (Some(Key::C), Tool::Circle, 48),
        (None, Tool::Polygon, 6), // Polygon has no default key; G makes components.
    ] {
        let mut h = Harness::new();
        match key {
            Some(key) => h.key(key),
            None => h.app.set_tool(tool),
        }
        assert_eq!(h.app.tool, tool);
        h.click(h.at(DVec3::ZERO));
        let end = h.at(DVec3::new(2., 1.5, 0.));
        h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
        assert!(
            h.app.doc.model.edges.is_empty(),
            "preview must not edit geometry"
        );
        h.click(end);
        assert_eq!(h.app.doc.model.edges.len(), edges, "{tool:?}");
        assert_eq!(h.app.doc.model.faces.len(), 1, "{tool:?}");
        h.app.doc.model.validate().unwrap();
        h.app.undo(&h.ctx, false);
        assert!(h.app.doc.model.edges.is_empty());
        h.app.undo(&h.ctx, true);
        assert_eq!(h.app.doc.model.edges.len(), edges);
        assert_eq!(h.app.doc.model.faces.len(), 1);
    }
}

#[test]
fn drawing_menu_switches_tools_and_remembers_the_choice() {
    let mut h = Harness::new();
    let rail = egui::pos2(38., 169.);
    for tool in Tool::DRAWING {
        h.click(rail);
        let output = h.frame(vec![], egui::Modifiers::NONE);
        for option in Tool::DRAWING {
            assert_menu_icon(&output, option.label());
        }
        let pos = menu_text_position(&output, tool.label());
        h.click(pos);
        assert_eq!(h.app.tool, tool);
        assert_eq!(h.app.drawing_tool, tool);
        assert!(h.app.doc.model.edges.is_empty());
        h.key(Key::Space);
        assert_eq!(h.app.drawing_tool, tool);
    }
}

#[test]
fn freehand_drag_commits_a_stroke_and_escape_cancels_it() {
    let mut h = Harness::new();
    h.key(Key::D);
    let start = h.at(DVec3::ZERO);
    h.pointer(start, egui::PointerButton::Primary, true, false);
    for point in [
        DVec3::new(1., 0.5, 0.),
        DVec3::new(2., 0.3, 0.),
        DVec3::new(3., 1., 0.),
    ] {
        let pos = h.at(point);
        h.frame(vec![egui::Event::PointerMoved(pos)], egui::Modifiers::NONE);
    }
    assert!(h.app.doc.model.edges.is_empty());
    let end = h.at(DVec3::new(3., 1., 0.));
    h.pointer(end, egui::PointerButton::Primary, false, false);
    assert_eq!(h.app.doc.model.edges.len(), 3);
    h.app.doc.model.validate().unwrap();
    h.app.undo(&h.ctx, false);
    assert!(h.app.doc.model.edges.is_empty());
    h.pointer(start, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    h.key(Key::Escape);
    h.pointer(end, egui::PointerButton::Primary, false, false);
    assert!(h.app.doc.model.edges.is_empty());
    assert!(h.app.drawing.is_none());
}

#[test]
fn shape_previews_cancel_and_degenerate_rectangles_do_not_commit() {
    let mut h = Harness::new();
    h.key(Key::R);
    h.click(h.at(DVec3::ZERO));
    h.click(h.at(DVec3::X * 2.));
    assert!(h.app.doc.model.edges.is_empty());
    assert!(h.app.drawing.is_some());
    h.key(Key::Escape);
    assert!(h.app.drawing.is_none());
    h.click(h.at(DVec3::ZERO));
    h.key(Key::C);
    assert!(h.app.drawing.is_none());
    assert!(h.app.doc.model.edges.is_empty());
}

#[test]
fn shapes_stay_on_the_starting_face_plane() {
    let mut h = Harness::new();
    let corners = [
        DVec3::new(-4., 0., 0.),
        DVec3::new(4., 0., 0.),
        DVec3::new(4., 0., 5.),
        DVec3::new(-4., 0., 5.),
    ];
    for i in 0..4 {
        h.app
            .doc
            .model
            .add_edge(corners[i], corners[(i + 1) % 4])
            .unwrap();
    }
    h.key(Key::R);
    h.click(h.at(DVec3::new(-1., 0., 1.)));
    assert!(h.app.drawing.as_ref().unwrap().normal.dot(DVec3::Y).abs() > 0.99);
    let end = h.at(DVec3::new(1., 0., 3.));
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    let preview = h.app.drawing_points();
    assert!(
        preview.iter().all(|point| point.y.abs() < 1e-6),
        "preview={preview:?}, snap={:?}, normal={:?}",
        h.app.snap,
        h.app.drawing.as_ref().map(|d| d.normal)
    );
    h.click(end);
    assert_eq!(h.app.doc.model.edges.len(), 8);
    h.app.doc.model.validate().unwrap();
}

#[test]
fn click_line_precision_and_axis_lock_through_real_ui() {
    let mut h = Harness::new();
    h.key(Key::L);
    let start = h.at(DVec3::ZERO);
    h.click(start);
    assert_eq!(h.app.line_start, Some(DVec3::ZERO));
    h.key(Key::ArrowUp);
    assert_eq!(h.app.lock, Some(Axis::Z));
    let end = h.at(DVec3::Z * 3.);
    h.frame(
        vec![
            egui::Event::PointerMoved(end),
            egui::Event::Text("2.5".into()),
        ],
        egui::Modifiers::NONE,
    );
    h.key(Key::Enter);
    assert_eq!(h.app.doc.model.edges.len(), 1);
    let e = h.app.doc.model.edges.values().next().unwrap();
    let (a, b) = h.app.doc.model.points(e);
    assert!(a.distance(DVec3::ZERO) < 1e-7);
    assert!(b.distance(DVec3::Z * 2.5) < 1e-5);
    h.key(Key::Escape);
    assert!(h.app.line_start.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 1);
}

#[test]
fn live_preview_and_click_snap_every_segment_in_a_line_chain() {
    let mut h = Harness::new();
    h.key(Key::L);
    h.click(h.at(DVec3::ZERO));
    for (index, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
        let start = h.app.line_start.unwrap();
        let screen_start = h.at(start);
        let direction = (h.at(start + axis.vector()) - screen_start).normalized();
        // Include a short second segment inside the old 16-pixel dead zone.
        let distance = if index == 1 { 9. } else { 75. };
        let cursor =
            screen_start + direction * distance + egui::vec2(-direction.y, direction.x) * 0.5;
        h.frame(
            vec![egui::Event::PointerMoved(cursor)],
            egui::Modifiers::NONE,
        );
        let preview = h.app.snap.unwrap();
        assert_eq!(preview.kind, Inference::Axis(axis));
        assert!((preview.point - start).cross(axis.vector()).length() < 1e-6);
        assert!(preview.point.distance(start) > 1e-3);
        assert_eq!(
            h.app.doc.model.edges.len(),
            index,
            "Moving the preview must not create an edge"
        );
        h.click(cursor);
        assert!(
            h.app.line_start.unwrap().distance(preview.point) < 1e-6,
            "Click must apply the preview's snapped point"
        );
        assert_eq!(h.app.doc.model.edges.len(), index + 1);
    }
}

#[test]
fn perpendicular_preview_and_click_acquire_nearby_existing_edge() {
    let mut h = Harness::new();
    let start = DVec3::X * 4.;
    h.app.doc.model.add_edge(DVec3::ZERO, start).unwrap();
    h.app
        .doc
        .model
        .add_edge(DVec3::new(4.15, 1., 0.), DVec3::new(4.15, 7., 0.))
        .unwrap();
    let group = h.app.doc.model.make_object(
        &h.app.doc.model.selectable_entities(), false, "Connected edges",
    ).unwrap();
    assert!(h.app.doc.model.enter_object(group));
    h.key(Key::L);
    h.click(h.at(start));
    let target = DVec3::new(4.15, 3., 0.);
    let cursor = h.at(target);
    h.frame(
        vec![egui::Event::PointerMoved(cursor)],
        egui::Modifiers::NONE,
    );
    let preview = h.app.snap.unwrap();
    assert_eq!(preview.kind, Inference::OnEdge);
    assert!(preview.point.distance(target) < 1e-5);
    h.click(cursor);
    assert!(h.app.line_start.unwrap().distance(preview.point) < 1e-6);
    assert_eq!(h.app.doc.model.edges.len(), 4);
    h.app.doc.model.validate().unwrap();
}

#[test]
fn precision_commit_uses_current_pointer_even_when_it_moved_in_the_same_frame() {
    let mut h = Harness::new();
    h.key(Key::L);
    h.click(h.at(DVec3::ZERO));
    let start = h.app.line_start.unwrap();
    h.frame(
        vec![egui::Event::PointerMoved(h.at(start + DVec3::X * 3.))],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.snap.unwrap().kind, Inference::Axis(Axis::X));
    let current = h.at(start + DVec3::Y * 3.);
    h.frame(
        vec![
            egui::Event::PointerMoved(current),
            egui::Event::Text("2".into()),
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.doc.model.edges.len(), 1);
    assert!(h.app.line_start.unwrap().distance(start + DVec3::Y * 2.) < 1e-6);
}

#[test]
fn finishing_freehand_after_orbit_does_not_add_the_orbit_endpoint() {
    for release_modifier_first in [false, true] {
        let mut h = Harness::new();
        h.key(Key::D);
        h.pointer(h.at(DVec3::ZERO), egui::PointerButton::Primary, true, false);
        let start = h.at(DVec3::X);
        h.frame(
            vec![egui::Event::PointerMoved(start)],
            egui::Modifiers::NONE,
        );
        let points = h.app.drawing.as_ref().unwrap().points.clone();
        assert_eq!(points.len(), 2);
        let end = start + egui::vec2(60., 30.);
        h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::CTRL);
        let modifiers = if release_modifier_first {
            h.frame(vec![], egui::Modifiers::NONE);
            egui::Modifiers::NONE
        } else {
            egui::Modifiers::CTRL
        };
        h.pointer_with_modifiers(end, egui::PointerButton::Primary, false, modifiers);
        assert_eq!(h.app.doc.model.edges.len(), 1);
        let edge = h.app.doc.model.edges.values().next().unwrap();
        let (a, b) = h.app.doc.model.points(edge);
        assert!(a.distance(points[0]) < 1e-6);
        assert!(b.distance(points[1]) < 1e-6);
    }
}

#[test]
fn close_snapped_outline_then_select_and_delete_face_or_edge() {
    let mut h = Harness::new();
    assert!(h.app.doc.snap_to_geometry);
    h.key(Key::L);
    for point in [
        DVec3::ZERO,
        DVec3::new(4., 0., 0.),
        DVec3::new(4., 3., 0.),
        DVec3::new(0., 3., 0.),
    ] {
        h.click(h.at(point));
    }
    h.click(h.at(DVec3::ZERO) + egui::vec2(3., 2.));
    assert_eq!(h.app.doc.model.faces.len(), 1);
    assert_eq!(h.app.doc.model.edges.len(), 4);
    assert!(h.app.line_start.is_none());
    assert!(h.app.gesture.is_none());
    assert_eq!(h.app.tool, Tool::Line);
    // Dismiss the completed root drawing's naming dialog first.
    assert!(h.app.object_rename.is_some());
    h.key(Key::Escape);
    // The next click starts a fresh line, just as it would after Escape.
    h.click(h.at(DVec3::new(6., 1., 0.)));
    assert_eq!(h.app.doc.model.edges.len(), 4);
    assert!(h.app.line_start.is_some());
    let face = *h.app.doc.model.faces.keys().next().unwrap();
    h.key(Key::Space);
    let group = h.app.doc.model.parent(face).unwrap();
    assert!(h.app.doc.model.enter_object(group));
    h.app.selected.clear();
    h.click(h.at(DVec3::new(2., 1.5, 0.)));
    assert_eq!(h.app.selected, BTreeSet::from([face]));
    h.key(Key::Delete);
    assert!(h.app.doc.model.faces.is_empty());
    assert_eq!(h.app.doc.model.edges.len(), 4);
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.faces.len(), 1);
    h.click(h.at(DVec3::new(2., 0., 0.)));
    assert_eq!(h.app.selected.len(), 1);
    assert!(
        h.app
            .doc
            .model
            .edges
            .contains_key(h.app.selected.first().unwrap())
    );
    h.key(Key::Delete);
    assert_eq!(h.app.doc.model.edges.len(), 3);
    assert!(h.app.doc.model.faces.is_empty());
}

#[test]
fn closing_a_shape_against_existing_geometry_stops_drawing_like_escape() {
    let mut h = Harness::new();
    let a = DVec3::ZERO;
    let b = DVec3::X * 4.;
    let c = DVec3::new(4., 3., 0.);
    h.app.doc.model.add_edge(a, b).unwrap();
    h.app.doc.model.add_edge(b, c).unwrap();
    let group = h.app.doc.model.make_object(
        &h.app.doc.model.selectable_entities(), false, "Outline",
    ).unwrap();
    assert!(h.app.doc.model.enter_object(group));
    h.app.set_tool(Tool::Line);
    h.click(h.at(c));
    h.click(h.at(DVec3::Y * 3.));
    assert!(
        h.app.line_start.is_some(),
        "Open outlines must keep drawing"
    );
    assert!(h.app.doc.model.faces.is_empty());
    h.app.set_axis_lock(Some(Axis::Y), &h.ctx);
    h.click(h.at(a));
    assert_eq!(h.app.doc.model.faces.len(), 1);
    assert_eq!(h.app.doc.model.edges.len(), 4);
    assert!(h.app.line_start.is_none());
    assert!(h.app.lock.is_none());
    assert!(h.app.drawing.is_none());
    assert!(h.app.length_input.is_empty());
    assert_eq!(h.app.tool, Tool::Line);
    assert_eq!(h.app.doc.history.undo_label(), Some("Draw line"));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edges.len(), 3);
    assert!(h.app.doc.model.faces.is_empty());
    h.app.undo(&h.ctx, true);
    assert_eq!(h.app.doc.model.faces.len(), 1);
    assert!(h.app.line_start.is_none());
}

#[test]
fn snapping_menu_has_independent_targets_without_a_master_switch() {
    let mut h = Harness::new();
    assert!(h.app.doc.snap_to_geometry);
    assert!(h.app.doc.snap_to_axes);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let snapping = dropdown_position(&output);
    h.click(snapping);
    let menu = h.frame(vec![], egui::Modifiers::NONE);
    fn has_master_switch(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.job.text == "Snap to objects",
            egui::Shape::Vec(shapes) => shapes.iter().any(has_master_switch),
            _ => false,
        }
    }
    assert!(
        !menu
            .shapes
            .iter()
            .any(|shape| has_master_switch(&shape.shape))
    );
    for label in ["Snap to edge", "Snap to face", "Snap to face center"] {
        let menu = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&menu, label));
    }
    assert!(!h.app.doc.snap_to_geometry);
    assert!(!h.app.snap_to_edge);
    assert!(!h.app.snap_to_face);
    assert!(!h.app.snap_to_face_center);
    assert!(h.app.doc.snap_to_axes);
    assert!(!h.app.project.modified);
    let menu = h.frame(vec![], egui::Modifiers::NONE);
    let axes = text_position(&menu, "Snap to axes");
    h.click(axes);
    assert!(!h.app.doc.snap_to_axes);
    assert!(!h.app.doc.snap_to_geometry);
    h.click(axes);
    assert!(h.app.doc.snap_to_axes);
    h.key(Key::Escape);
    h.app
        .doc
        .model
        .add_edge(DVec3::new(0., 0., 2.), DVec3::new(6., 0., 2.))
        .unwrap();
    h.key(Key::L);
    let pointer = h.at(DVec3::new(1.5, 0., 2.));
    h.frame(
        vec![egui::Event::PointerMoved(pointer)],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.snap.unwrap().kind, Inference::Plane);
    h.click(snapping);
    let menu = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&menu, "Snap to edge"));
    assert!(h.app.doc.snap_to_geometry);
    assert!(h.app.snap_to_edge);
    assert!(!h.app.snap_to_face);
    assert!(!h.app.snap_to_face_center);
    h.key(Key::Escape);
    h.frame(
        vec![egui::Event::PointerMoved(pointer)],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.snap.unwrap().kind, Inference::OnEdge);
}

#[test]
fn text_leader_axis_locks_preserve_attachment_and_constrain_label_placement() {
    let anchor = DVec3::new(0.2, 0.3, 0.4);
    for (key, axis) in [
        (Key::ArrowRight, Axis::X),
        (Key::ArrowLeft, Axis::Y),
        (Key::ArrowUp, Axis::Z),
    ] {
        let mut h = Harness::new();
        h.app.set_tool(Tool::Text);
        h.app.measurement = Some(measurement_tools::Measurement {
            points: vec![anchor],
            ..Default::default()
        });
        let pos = h.at(anchor + DVec3::new(2., 3., 4.));
        h.frame(vec![egui::Event::PointerMoved(pos)], egui::Modifiers::NONE);
        h.key(key);
        assert_eq!(h.app.lock, Some(axis));
        let measurement = h.app.measurement.as_ref().unwrap();
        assert_eq!(measurement.points, vec![anchor]);
        assert!(measurement.offset.cross(axis.vector()).length() < 1e-7);
        h.key(Key::ArrowDown);
        assert_eq!(h.app.lock, None);
        h.app.update_text_pointer(pos);
        assert!(
            h.app
                .measurement
                .as_ref()
                .unwrap()
                .offset
                .cross(axis.vector())
                .length()
                > 1e-4
        );
    }
}
