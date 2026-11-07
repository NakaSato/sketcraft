//! Orbit, pan, zoom and the drawing navigation modifier.
use super::*;

#[test]
fn navigation_clears_hover_and_freezes_tool_inference_until_release() {
    for tool in [
        Tool::Select,
        Tool::Line,
        Tool::Freehand,
        Tool::Erase,
        Tool::Paint,
    ] {
        let mut h = Harness::new();
        let edge = h.app.doc.model.add_edge(-DVec3::X, DVec3::X).unwrap();
        h.app.set_tool(tool);
        let start = h.at(DVec3::ZERO);
        h.frame(
            vec![egui::Event::PointerMoved(start)],
            egui::Modifiers::NONE,
        );
        h.app.hover = Some(edge);
        h.pointer(start, egui::PointerButton::Middle, true, false);
        assert!(h.app.hover.is_none(), "{tool:?}");
        let snap = h.app.snap.map(|s| s.point);
        let line_start = h.app.line_start;
        let before = h.app.doc.camera.clone();
        let end = start + egui::vec2(12., 8.);
        h.frame(
            vec![
                egui::Event::PointerMoved(start + egui::vec2(4., 2.)),
                egui::Event::PointerMoved(end),
            ],
            egui::Modifiers::NONE,
        );
        let mut expected = before;
        expected.orbit_around(DVec2::new(12., 8.), DVec3::ZERO);
        assert_eq!(h.app.doc.camera, expected);
        h.frame(vec![], egui::Modifiers::NONE);
        assert!(h.app.hover.is_none(), "{tool:?}");
        assert!(h.app.flip_hover.is_none());
        assert_eq!(h.app.snap.map(|s| s.point), snap);
        assert_eq!(h.app.line_start, line_start);
        h.pointer(end, egui::PointerButton::Middle, false, false);
        assert_eq!(h.app.tool, tool);
        assert_eq!(h.app.doc.model.edges.len(), 1);

        h.app.set_tool(Tool::Select);
        h.frame(
            vec![egui::Event::PointerMoved(h.at(DVec3::ZERO))],
            egui::Modifiers::NONE,
        );
        assert_eq!(h.app.hover, Some(edge));
        h.key(Key::O);
        h.frame(
            vec![egui::Event::PointerMoved(h.at(DVec3::ZERO))],
            egui::Modifiers::NONE,
        );
        assert!(h.app.hover.is_none());
    }
}

#[test]
fn drawing_modifier_orbits_without_mouse_buttons_and_preserves_pending_points() {
    for modifiers in [
        egui::Modifiers::CTRL,
        egui::Modifiers::MAC_CMD,
        egui::Modifiers::COMMAND,
    ] {
        for tool in Tool::DRAWING {
            let mut h = Harness::new();
            h.app.set_tool(tool);
            let start = h.at(DVec3::ZERO);
            if tool != Tool::Freehand {
                h.click(start);
            }
            h.frame(
                vec![egui::Event::PointerMoved(start)],
                egui::Modifiers::NONE,
            );
            let line_start = h.app.line_start;
            let points = h.app.drawing_points();
            let snap = h.app.snap.map(|s| s.point);
            let before = h.app.doc.camera.clone();
            h.frame(vec![], modifiers);
            assert!(h.app.navigating(), "{tool:?}: {modifiers:?}");
            assert_eq!(h.app.doc.camera, before);
            let end = start + egui::vec2(12., 8.);
            h.frame(
                vec![
                    egui::Event::PointerMoved(start + egui::vec2(4., 2.)),
                    egui::Event::MouseMoved(egui::vec2(4., 2.)),
                    egui::Event::PointerMoved(end),
                ],
                modifiers,
            );
            let mut expected = before;
            expected.orbit_around(DVec2::new(12., 8.), DVec3::ZERO);
            assert_eq!(h.app.doc.camera, expected);
            assert!(h.app.gesture.is_none());
            assert_eq!(h.app.snap.map(|s| s.point), snap);
            assert_eq!(h.app.tool, tool);
            assert_eq!(h.app.line_start, line_start);
            assert_eq!(h.app.drawing_points(), points);
            assert!(h.app.doc.model.edges.is_empty());
            h.frame(vec![], egui::Modifiers::NONE);
            assert!(!h.app.navigating());
            h.frame(
                vec![egui::Event::PointerMoved(end + egui::vec2(6., 4.))],
                egui::Modifiers::NONE,
            );
            assert_eq!(h.app.doc.camera, expected);

            // The next normal drawing gesture uses the preserved starting point.
            let target = h.at(DVec3::new(2., 3., 0.));
            if tool == Tool::Freehand {
                h.pointer(start, egui::PointerButton::Primary, true, false);
                h.frame(
                    vec![egui::Event::PointerMoved(target)],
                    egui::Modifiers::NONE,
                );
                h.pointer(target, egui::PointerButton::Primary, false, false);
            } else {
                h.click(target);
            }
            assert!(!h.app.doc.model.edges.is_empty(), "{tool:?}: {modifiers:?}");
            h.app.doc.model.validate().unwrap();
        }
    }
}

#[test]
fn drawing_modifier_moves_camera_once_with_either_mouse_button_held() {
    for button in [egui::PointerButton::Primary, egui::PointerButton::Middle] {
        let mut h = Harness::new();
        h.key(Key::L);
        let start = h.app.viewport.center();
        h.pointer_with_modifiers(start, button, true, egui::Modifiers::CTRL);
        let before = h.app.doc.camera.clone();
        let end = start + egui::vec2(12., 8.);
        h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::CTRL);
        let mut expected = before;
        expected.orbit_around(DVec2::new(12., 8.), DVec3::ZERO);
        assert_eq!(h.app.doc.camera, expected);
        h.pointer_with_modifiers(end, button, false, egui::Modifiers::CTRL);
        assert_eq!(h.app.doc.camera, expected);
        assert!(h.app.doc.model.edges.is_empty());
    }
}

#[test]
fn drawing_modifier_does_not_orbit_over_panels_or_jump_on_reentry() {
    let mut h = Harness::new();
    h.key(Key::L);
    let start = h.app.viewport.center();
    h.frame(
        vec![egui::Event::PointerMoved(start)],
        egui::Modifiers::NONE,
    );
    let end = start + egui::vec2(12., 8.);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::CTRL);
    let before = h.app.doc.camera.clone();
    let panel = egui::pos2(490., 32.);
    h.frame(
        vec![egui::Event::PointerMoved(panel)],
        egui::Modifiers::CTRL,
    );
    assert_eq!(h.app.doc.camera, before);
    h.frame(
        vec![egui::Event::PointerMoved(start)],
        egui::Modifiers::CTRL,
    );
    assert_eq!(h.app.doc.camera, before);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::CTRL);
    let mut expected = before;
    expected.orbit_around(DVec2::new(12., 8.), DVec3::ZERO);
    assert_eq!(h.app.doc.camera, expected);
    h.frame(vec![egui::Event::PointerGone], egui::Modifiers::CTRL);
    h.frame(
        vec![egui::Event::PointerMoved(start)],
        egui::Modifiers::CTRL,
    );
    assert_eq!(h.app.doc.camera, expected);
}

#[test]
fn drawing_modifier_release_stops_orbit_without_placing_a_point() {
    for modifiers in [egui::Modifiers::CTRL, egui::Modifiers::MAC_CMD] {
        let mut h = Harness::new();
        h.key(Key::L);
        h.click(h.at(DVec3::ZERO));
        h.key(Key::ArrowUp);
        h.app.length_input = "2.5".into();
        let start = h.app.viewport.center();
        h.pointer_with_modifiers(start, egui::PointerButton::Primary, true, modifiers);
        let end = start + egui::vec2(2., 1.);
        h.frame(vec![egui::Event::PointerMoved(end)], modifiers);
        let camera = h.app.doc.camera.clone();
        h.frame(vec![], egui::Modifiers::NONE);
        assert!(!h.app.navigating());
        let release = end + egui::vec2(1., 0.);
        h.frame(
            vec![egui::Event::PointerMoved(release)],
            egui::Modifiers::NONE,
        );
        h.pointer(release, egui::PointerButton::Primary, false, false);
        assert_eq!(h.app.doc.camera, camera);
        assert!(h.app.doc.model.edges.is_empty());
        assert_eq!(h.app.line_start, Some(DVec3::ZERO));
        assert_eq!(h.app.lock, Some(Axis::Z));
        assert_eq!(h.app.length_input, "2.5");
    }
}

#[test]
fn pressing_drawing_modifier_during_a_point_click_cancels_placement() {
    for tool in [Tool::Line, Tool::Rectangle, Tool::Circle, Tool::Polygon] {
        for modifiers in [egui::Modifiers::CTRL, egui::Modifiers::MAC_CMD] {
            let mut h = Harness::new();
            h.app.set_tool(tool);
            h.click(h.at(DVec3::ZERO));
            let start = h.at(DVec3::new(2., 3., 0.));
            h.pointer(start, egui::PointerButton::Primary, true, false);
            let before = h.app.doc.camera.clone();
            let end = start + egui::vec2(2., 1.);
            h.frame(vec![egui::Event::PointerMoved(end)], modifiers);
            assert_ne!(h.app.doc.camera, before);
            h.frame(vec![], egui::Modifiers::NONE);
            h.pointer(end, egui::PointerButton::Primary, false, false);
            assert!(h.app.doc.model.edges.is_empty());
            assert!(h.app.line_start.is_some() || h.app.drawing.is_some());
        }
    }
}

#[test]
fn drawing_modifier_pauses_freehand_sampling_and_resumes_on_release() {
    for modifiers in [egui::Modifiers::CTRL, egui::Modifiers::MAC_CMD] {
        let mut h = Harness::new();
        h.key(Key::D);
        h.pointer(h.at(DVec3::ZERO), egui::PointerButton::Primary, true, false);
        let start = h.at(DVec3::X);
        h.frame(
            vec![egui::Event::PointerMoved(start)],
            egui::Modifiers::NONE,
        );
        let points = h.app.drawing.as_ref().unwrap().points.clone();
        let snap = h.app.snap.map(|s| s.point);
        let before = h.app.doc.camera.clone();
        let end = start + egui::vec2(12., 8.);
        h.frame(vec![egui::Event::PointerMoved(end)], modifiers);
        assert_ne!(h.app.doc.camera, before);
        assert_eq!(h.app.drawing.as_ref().unwrap().points, points);
        assert_eq!(h.app.snap.map(|s| s.point), snap);
        assert!(h.app.doc.model.edges.is_empty());
        h.frame(vec![], egui::Modifiers::NONE);
        let camera = h.app.doc.camera.clone();
        let target = h.at(DVec3::new(2., 3., 0.));
        h.frame(
            vec![egui::Event::PointerMoved(target)],
            egui::Modifiers::NONE,
        );
        assert!(h.app.drawing.as_ref().unwrap().points.len() > points.len());
        assert_eq!(h.app.doc.camera, camera);
        h.pointer(target, egui::PointerButton::Primary, false, false);
        assert!(!h.app.doc.model.edges.is_empty());
        h.app.doc.model.validate().unwrap();
    }
}

#[test]
fn drawing_modifier_shift_pans_and_middle_navigation_still_works() {
    let mut h = Harness::new();
    h.key(Key::L);
    h.click(h.at(DVec3::ZERO));
    let before = h.app.doc.camera.clone();
    let start = h.app.viewport.center();
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    h.frame(
        vec![egui::Event::PointerMoved(start)],
        egui::Modifiers::NONE,
    );
    h.frame(vec![], modifiers);
    let end = start + egui::vec2(60., 30.);
    h.frame(vec![egui::Event::PointerMoved(end)], modifiers);
    assert_eq!(h.app.doc.camera.yaw, before.yaw);
    assert_eq!(h.app.doc.camera.pitch, before.pitch);
    assert_ne!(h.app.doc.camera.target, before.target);
    assert!(h.app.gesture.is_none());
    h.frame(vec![], egui::Modifiers::NONE);
    h.pointer(end, egui::PointerButton::Middle, true, false);
    let before = h.app.doc.camera.clone();
    let end = end + egui::vec2(12., 8.);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    assert_ne!(h.app.doc.camera.yaw, before.yaw);
    h.pointer(end, egui::PointerButton::Middle, false, false);
    assert_eq!(h.app.line_start, Some(DVec3::ZERO));
    assert!(h.app.doc.model.edges.is_empty());
}

#[test]
fn shift_orbit_pans_and_middle_navigation_preserves_pending_line() {
    let mut h = Harness::new();
    h.key(Key::L);
    let origin = h.at(DVec3::ZERO);
    h.click(origin);
    let before = h.app.doc.camera.clone();
    let target = h.app.line_start;
    let pos = h.app.viewport.center();
    h.pointer(pos, egui::PointerButton::Middle, true, true);
    let next = pos + egui::vec2(60., 30.);
    h.frame(
        vec![egui::Event::PointerMoved(next)],
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(h.app.doc.camera.yaw, before.yaw);
    assert_eq!(h.app.doc.camera.pitch, before.pitch);
    assert!(h.app.doc.camera.target.distance(before.target) > 0.1);
    let pivot = DVec3::ZERO;
    let pivot_screen = h.at(pivot);
    let radius = h.app.doc.camera.eye().distance(pivot);
    h.frame(
        vec![egui::Event::PointerMoved(next + egui::vec2(20., 0.))],
        egui::Modifiers::NONE,
    );
    assert_ne!(h.app.doc.camera.yaw, before.yaw);
    assert!(h.at(pivot).distance(pivot_screen) < 0.001);
    assert!((h.app.doc.camera.eye().distance(pivot) - radius).abs() < 1e-6);
    h.pointer(
        next + egui::vec2(20., 0.),
        egui::PointerButton::Middle,
        false,
        false,
    );
    assert_eq!(h.app.tool, Tool::Line);
    assert_eq!(h.app.line_start, target);
    assert!(h.app.doc.model.edges.is_empty());
}

#[test]
fn orbit_tool_shift_drag_only_translates_camera() {
    let mut h = Harness::new();
    h.key(Key::O);
    let before = h.app.doc.camera.clone();
    let p = h.app.viewport.center();
    h.pointer(p, egui::PointerButton::Primary, true, true);
    h.frame(
        vec![egui::Event::PointerMoved(p + egui::vec2(-80., 40.))],
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    h.pointer(
        p + egui::vec2(-80., 40.),
        egui::PointerButton::Primary,
        false,
        true,
    );
    assert_eq!(h.app.doc.camera.yaw, before.yaw);
    assert_eq!(h.app.doc.camera.pitch, before.pitch);
    assert_ne!(h.app.doc.camera.target, before.target);
    assert!(h.app.doc.model.edges.is_empty());
}

#[test]
fn orbit_uses_visible_objects_center_after_pan_and_zoom() {
    for button in [egui::PointerButton::Primary, egui::PointerButton::Middle] {
        for hidden in [false, true] {
            let mut h = Harness::new();
            if button == egui::PointerButton::Primary {
                h.key(Key::O);
            }
            let corners = [
                DVec3::new(-8., -6., 2.),
                DVec3::new(-2., -6., 2.),
                DVec3::new(-2., 0., 2.),
                DVec3::new(-8., 0., 2.),
            ];
            for i in 0..4 {
                h.app
                    .doc
                    .model
                    .add_edge(corners[i], corners[(i + 1) % 4])
                    .unwrap();
            }
            let group = h
                .app
                .doc
                .model
                .make_object(&h.app.doc.model.root_entities(), false, "Selected panel")
                .unwrap();
            let edge = h
                .app
                .doc
                .model
                .add_edge(DVec3::new(12., 4., 4.), DVec3::new(18., 8., 10.))
                .unwrap();
            let component = h
                .app
                .doc
                .model
                .make_object(&BTreeSet::from([edge]), true, "Other object")
                .unwrap();
            h.app.selected = BTreeSet::from([group]);
            h.app.doc.model.locked.insert(component);
            if hidden {
                h.app.doc.model.hidden.insert(component);
            }
            let pivot = if hidden {
                DVec3::new(-5., -3., 2.)
            } else {
                DVec3::new(5., 1., 6.)
            };
            let size = dv(h.app.viewport.size());
            h.app.doc.camera.pan(DVec2::new(150., 90.), size.y);
            h.app.doc.camera.zoom(180., size * 0.7, size);
            let before = h.app.doc.camera.clone();
            assert!(pivot.distance(before.target) > 0.1);
            let pivot_screen = h.at(pivot);
            let radius = before.eye().distance(pivot);
            assert!(pivot_screen.distance(h.app.viewport.center()) > 1.);

            // Off-center pointer positions and the current selection do not set the pivot.
            let start = h.app.viewport.center() + egui::vec2(100., 70.);
            h.pointer(start, button, true, false);
            assert!(h.app.doc.camera.eye().distance(before.eye()) < 1e-6);
            assert!(h.at(pivot).distance(pivot_screen) < 0.001);
            for delta in [
                egui::vec2(0.01, 0.),
                egui::vec2(40., -35.),
                egui::vec2(70., -65.),
            ] {
                h.frame(
                    vec![egui::Event::PointerMoved(start + delta)],
                    egui::Modifiers::NONE,
                );
                assert!(h.at(pivot).distance(pivot_screen) < 0.001);
                assert!((h.app.doc.camera.eye().distance(pivot) - radius).abs() < 1e-6);
                assert_eq!(h.app.doc.camera.distance, before.distance);
                if delta.x < 1. {
                    assert!(h.app.doc.camera.eye().distance(before.eye()) < 0.01);
                }
            }
            assert_ne!(h.app.doc.camera.yaw, before.yaw);
            assert_ne!(h.app.doc.camera.pitch, before.pitch);

            // Releasing Shift must preserve the new framing without a recentering jump.
            let panned = start + egui::vec2(120., -40.);
            h.frame(
                vec![egui::Event::PointerMoved(panned)],
                egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            let panned_screen = h.at(pivot);
            let panned_radius = h.app.doc.camera.eye().distance(pivot);
            assert!(panned_screen.distance(pivot_screen) > 1.);
            let end = panned + egui::vec2(20., 0.);
            h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
            assert!(h.at(pivot).distance(panned_screen) < 0.001);
            assert!((h.app.doc.camera.eye().distance(pivot) - panned_radius).abs() < 1e-6);
            assert_eq!(h.app.doc.camera.distance, before.distance);
            h.pointer(end, button, false, false);
        }
    }
}

#[test]
fn orbit_center_tracks_nested_visibility_and_display_overrides() {
    let mut h = Harness::new();
    let model = &mut h.app.doc.model;
    model.add_edge(DVec3::ZERO, DVec3::X * 2.).unwrap();
    let far = model.add_edge(DVec3::X * 10., DVec3::X * 12.).unwrap();
    let inner = model
        .make_object(&BTreeSet::from([far]), true, "Nested component")
        .unwrap();
    let outer = model
        .make_object(&BTreeSet::from([inner]), false, "Outer group")
        .unwrap();
    // Grouping creates new IDs for the nested contents.
    let inner = *model.objects[&outer].entities.first().unwrap();
    let tag = model.add_tag("Assembly").unwrap();
    model.entity_tags.insert(outer, tag);
    model.locked.insert(outer);
    assert_eq!(h.app.orbit_center(), DVec3::X * 6.);

    for hidden in [outer, inner] {
        h.app.doc.model.hidden.insert(hidden);
        assert_eq!(h.app.orbit_center(), DVec3::X);
        h.app.doc.display.hidden_geometry = true;
        assert_eq!(h.app.orbit_center(), DVec3::X);
        h.app.doc.display.hidden_objects = true;
        assert_eq!(h.app.orbit_center(), DVec3::X * 6.);

        // Hidden tags still suppress objects when hidden objects are revealed.
        h.app.doc.model.tags.get_mut(&tag).unwrap().visible = false;
        assert_eq!(h.app.orbit_center(), DVec3::X);
        h.app.doc.model.tags.get_mut(&tag).unwrap().visible = true;
        h.app.doc.model.hidden.remove(&hidden);
        h.app.doc.display.hidden_objects = false;
        h.app.doc.display.hidden_geometry = false;
        assert_eq!(h.app.orbit_center(), DVec3::X * 6.);
    }
}

#[test]
fn orbit_center_ignores_hidden_geometry_and_disabled_dimensions() {
    let mut h = Harness::new();
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::X * 10., DVec3::X * 12.)
        .unwrap();
    h.app.doc.model.hidden.insert(edge);
    assert_eq!(h.app.orbit_center(), DVec3::ZERO);
    h.app.doc.display.hidden_objects = true;
    assert_eq!(h.app.orbit_center(), DVec3::ZERO);
    h.app.doc.display.hidden_geometry = true;
    assert_eq!(h.app.orbit_center(), DVec3::X * 11.);
    h.app.doc.display.hidden_geometry = false;

    let dimension = h
        .app
        .doc
        .model
        .add_dimension(DVec3::ZERO, DVec3::X * 2., DVec3::Y * 2.)
        .unwrap();
    assert_eq!(h.app.orbit_center(), DVec3::new(1., 1., 0.));
    h.app.doc.display.dimensions = false;
    assert_eq!(h.app.orbit_center(), DVec3::ZERO);
    h.app.doc.display.dimensions = true;
    h.app.doc.model.hidden.insert(dimension);
    assert_eq!(h.app.orbit_center(), DVec3::ZERO);
    let tag = h.app.doc.model.add_tag("Dimensions").unwrap();
    h.app.doc.model.entity_tags.insert(dimension, tag);
    h.app.doc.model.tags.get_mut(&tag).unwrap().visible = false;
    h.app.doc.model.hidden.remove(&dimension);
    assert_eq!(h.app.orbit_center(), DVec3::ZERO);
}

#[test]
fn orbit_center_tracks_geometry_changes_and_uses_origin_for_empty_models() {
    let mut h = Harness::new();
    h.app.doc.camera.pitch = -0.4;
    h.app.doc.camera.target = DVec3::new(3., 4., 20.);
    let before = h.app.doc.camera.clone();
    assert_eq!(h.app.orbit_center(), DVec3::ZERO);
    assert_eq!(h.app.doc.camera.target, before.target);
    assert_eq!(h.app.doc.camera.distance, before.distance);
    assert_eq!(h.app.doc.camera.yaw, before.yaw);
    assert_eq!(h.app.doc.camera.pitch, before.pitch);

    let first = h
        .app
        .doc
        .model
        .add_edge(DVec3::new(2., 4., 6.), DVec3::new(4., 8., 10.))
        .unwrap();
    assert_eq!(h.app.orbit_center(), DVec3::new(3., 6., 8.));
    h.app
        .doc
        .model
        .add_edge(DVec3::new(10., 12., 14.), DVec3::new(12., 16., 18.))
        .unwrap();
    assert_eq!(h.app.orbit_center(), DVec3::new(7., 10., 12.));
    h.app.doc.model.erase(&BTreeSet::from([first]));
    assert_eq!(h.app.orbit_center(), DVec3::new(11., 14., 16.));
    assert_eq!(h.app.doc.camera.target, before.target);
}
