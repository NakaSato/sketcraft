//! Tape Measure, Protractor, Dimensions, units and entity properties.
use super::*;

#[test]
fn tape_measure_snaps_reads_distance_and_restarts_without_editing_geometry() {
    let mut h = Harness::new();
    let (a, b) = (DVec3::new(0., 0., 2.), DVec3::new(3., 4., 2.));
    h.app.doc.model.add_edge(a, b).unwrap();
    let before = h.app.doc.model.clone();
    h.key(Key::T);
    h.click(h.at(a));
    h.frame(
        vec![egui::Event::PointerMoved(h.at(b))],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.measurement_value().as_deref(), Some("5.000 m"));
    h.click(h.at(b));
    assert!(h.app.measurement.as_ref().unwrap().complete);
    h.frame(
        vec![egui::Event::PointerMoved(h.at(DVec3::X))],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.measurement_value().as_deref(), Some("5.000 m"));
    assert_eq!(h.app.doc.model, before);
    assert!(h.app.doc.history.undo_label().is_none());
    h.click(h.at(a));
    assert_eq!(h.app.measurement.as_ref().unwrap().points.len(), 1);
    h.key(Key::Escape);
    assert!(h.app.measurement.is_none());
    assert_eq!(h.app.doc.model, before);
}

#[test]
fn protractor_uses_three_snapped_points_and_measures_in_three_dimensions() {
    let mut h = Harness::new();
    let center = DVec3::ZERO;
    let baseline = DVec3::Z * 3.;
    let target = DVec3::new(2., 0., 2.);
    h.app.doc.model.add_edge(center, baseline).unwrap();
    h.app.doc.model.add_edge(center, target).unwrap();
    let before = h.app.doc.model.clone();
    h.key(Key::A);
    h.click(h.at(center));
    h.click(h.at(baseline));
    h.frame(
        vec![egui::Event::PointerMoved(h.at(target))],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.measurement_value().as_deref(), Some("45.00°"));
    h.click(h.at(target));
    assert!(h.app.measurement.as_ref().unwrap().complete);
    assert_eq!(h.app.measurement_value().as_deref(), Some("45.00°"));
    assert_eq!(h.app.doc.model, before);
    assert!(h.app.doc.history.undo_label().is_none());
    h.key(Key::M);
    assert!(h.app.measurement.is_none());
}

#[test]
fn dimensions_commit_only_after_placement_and_support_selection_delete_and_undo() {
    let mut h = Harness::new();
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 4.)
        .unwrap();
    h.key(Key::I);
    h.click(h.at(DVec3::ZERO));
    h.click(h.at(DVec3::X * 4.));
    assert_eq!(h.app.measurement.as_ref().unwrap().points.len(), 2);
    assert!(h.app.doc.model.dimensions.is_empty());
    let placement = h.at(DVec3::new(2., -1.5, 0.));
    h.click(placement);
    assert!(h.app.measurement.is_none());
    assert_eq!(h.app.doc.model.dimensions.len(), 1);
    assert_eq!(h.app.doc.model.edges.len(), 1);
    assert_eq!(h.app.doc.history.undo_label(), Some("Add dimension"));
    let id = *h.app.doc.model.dimensions.keys().next().unwrap();
    let dimension = &h.app.doc.model.dimensions[&id];
    assert!((dimension.length(&h.app.doc.model) - 4.).abs() < 1e-7);
    assert!(dimension.offset.length() > 0.1);
    let [_, _, a, b] = dimension.layout(&h.app.doc.model);
    h.key(Key::Space);
    h.click(h.at((a + b) * 0.5));
    assert_eq!(h.app.selected, BTreeSet::from([id]));
    h.key(Key::Delete);
    assert!(h.app.doc.model.dimensions.is_empty());
    assert!(h.app.doc.model.edges.contains_key(&edge));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.dimensions.len(), 1);
    h.app
        .doc
        .model
        .scale_selection(&BTreeSet::from([edge]), DVec3::ZERO, 2.)
        .unwrap();
    assert!((h.app.doc.model.dimensions[&id].length(&h.app.doc.model) - 8.).abs() < 1e-7);
}

#[test]
fn measurement_cancellation_and_middle_navigation_preserve_pending_points_and_context() {
    let mut h = Harness::new();
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 4.)
        .unwrap();
    let group = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Measured group")
        .unwrap();
    assert!(h.app.doc.model.enter_object(group));
    h.key(Key::I);
    h.click(h.at(DVec3::ZERO));
    let start = h.app.measurement.as_ref().unwrap().points.clone();
    let from = h.app.viewport.center();
    let to = from + egui::vec2(20., 15.);
    h.pointer(from, egui::PointerButton::Middle, true, false);
    h.frame(vec![egui::Event::PointerMoved(to)], egui::Modifiers::NONE);
    h.pointer(to, egui::PointerButton::Middle, false, false);
    assert_eq!(h.app.measurement.as_ref().unwrap().points, start);
    assert_eq!(h.app.tool, Tool::Dimensions);
    h.key(Key::Escape);
    assert!(h.app.measurement.is_none());
    assert_eq!(h.app.doc.model.edit_context, Some(group));
    assert!(h.app.doc.model.dimensions.is_empty());
}

#[test]
fn properties_show_edge_lengths_totals_units_and_geometry_changes() {
    let mut h = Harness::new();
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::new(3., 0., 4.))
        .unwrap();
    h.app.selected = BTreeSet::from([edge]);
    let output = properties_output(&mut h);
    text_position(&output, "Length");
    text_position(&output, "5.000 m");

    let other = h
        .app
        .doc
        .model
        .add_edge(DVec3::splat(10.), DVec3::new(10., 12., 10.))
        .unwrap();
    h.app.selected.insert(other);
    h.app.doc.display.units.length = ketchup_core::LengthUnit::Millimeters;
    h.app.doc.display.units.precision.length = Some(1);
    let output = properties_output(&mut h);
    text_position(&output, "Total edge length");
    text_position(&output, "7000.0 mm");

    let end = h.app.doc.model.edges[&edge].b;
    h.app.doc.model.vertices.get_mut(&end).unwrap().position = DVec3::new(0., 0., 10.);
    h.app.selected = BTreeSet::from([edge]);
    text_position(&properties_output(&mut h), "10000.0 mm");
}

#[test]
fn properties_show_face_sides_and_include_holes_in_the_perimeter() {
    let mut h = Harness::new();
    // A tilted square ensures lengths are measured in 3D, not projected onto XY.
    let face = properties_outline(
        &mut h.app.doc.model,
        &[
            DVec3::ZERO,
            DVec3::new(3., 0., 4.),
            DVec3::new(3., 5., 4.),
            DVec3::new(0., 5., 0.),
        ],
    );
    properties_outline(
        &mut h.app.doc.model,
        &[
            DVec3::new(0.6, 1., 0.8),
            DVec3::new(1.2, 1., 1.6),
            DVec3::new(1.2, 2., 1.6),
            DVec3::new(0.6, 2., 0.8),
        ],
    );
    assert_eq!(h.app.doc.model.faces[&face].holes.len(), 1);
    h.app.selected = BTreeSet::from([face]);
    h.app.doc.display.units.precision.length = Some(2);
    let output = properties_output(&mut h);
    text_position(&output, "Perimeter");
    text_position(&output, "24.00 m");
    text_position(&output, "Side lengths");
    text_position(&output, "5.00 m · 5.00 m · 5.00 m · 5.00 m");
    text_position(&output, "Hole 1 side lengths");
    text_position(&output, "1.00 m · 1.00 m · 1.00 m · 1.00 m");
    text_position(&output, "Surface area");
}

#[test]
fn properties_keep_face_perimeters_separate_from_selected_edge_lengths() {
    let mut h = Harness::new();
    let first = properties_outline(
        &mut h.app.doc.model,
        &[DVec3::ZERO, DVec3::X, DVec3::new(1., 1., 0.), DVec3::Y],
    );
    let second = properties_outline(
        &mut h.app.doc.model,
        &[
            DVec3::X,
            DVec3::new(2., 0., 0.),
            DVec3::new(2., 1., 0.),
            DVec3::new(1., 1., 0.),
        ],
    );
    let edge = *h.app.doc.model.edges.keys().next().unwrap();
    h.app.selected = BTreeSet::from([first, second, edge]);
    let output = properties_output(&mut h);
    text_position(&output, "Length");
    text_position(&output, "1.000 m");
    text_position(&output, "Total perimeter");
    text_position(&output, "8.000 m");
}

#[test]
fn selected_object_volume_is_reused_across_inspector_redraws_and_selection_changes() {
    let mut h = Harness::new();
    let a = ui_solid_box(&mut h.app.doc.model, DVec3::ZERO, DVec3::splat(2.));
    let b = ui_solid_box(&mut h.app.doc.model, DVec3::splat(4.), DVec3::splat(5.));
    h.app.tool = Tool::Union;
    h.app.solid_first = Some(a);
    for id in [a, a, b, b, a] {
        h.app.selected = BTreeSet::from([id]);
        let _ = h.ctx.run_test_ui(egui::RawInput::default(), |ui| {
            h.app.entity_info_contents(ui);
        });
    }
    // Both inspector paths share the result, even when selecting another body.
    assert_eq!(h.app.solid_volume_cache.computations, 2);
    h.app.doc.camera.distance *= 2.;
    h.app.doc.display.units.precision.length = Some(1);
    let _ = h.ctx.run_test_ui(egui::RawInput::default(), |ui| {
        h.app.entity_info_contents(ui);
    });
    assert_eq!(h.app.solid_volume_cache.computations, 2);
}

#[test]
fn volume_cache_tracks_direct_geometry_edits_undo_context_and_replacement_models() {
    let mut model = Model::default();
    let id = ui_solid_box(&mut model, DVec3::ZERO, DVec3::splat(2.));
    let original = model.clone();
    let mut cache = solid_tools::VolumeCache::default();
    assert!((cache.volume(&model, id).unwrap() - 8.).abs() < 1e-6);
    for vertex in model.vertices.values_mut() {
        vertex.position.x *= 2.;
    }
    assert!((cache.volume(&model, id).unwrap() - 16.).abs() < 1e-6);
    model = original;
    assert!((cache.volume(&model, id).unwrap() - 8.).abs() < 1e-6);
    model.enter_object(id);
    let error = cache.volume(&model, id);
    assert!(error.is_err());
    assert_eq!(cache.volume(&model, id), error);
    assert_eq!(cache.computations, 4);

    // Opening another project may reuse the same entity IDs.
    let mut replacement = Model::default();
    let replacement_id = ui_solid_box(&mut replacement, DVec3::ZERO, DVec3::splat(3.));
    assert_eq!(replacement_id, id);
    assert!((cache.volume(&replacement, id).unwrap() - 27.).abs() < 1e-6);
    replacement.faces.clear();
    let error = cache.volume(&replacement, id);
    assert!(error.is_err());
    assert_eq!(cache.volume(&replacement, id), error);
    assert_eq!(cache.computations, 6);
}

#[test]
fn axis_lengths_only_measure_solids_in_mixed_and_nested_selections() {
    let mut model = Model::default();
    let size = DVec3::new(1., 2., 3.);
    let solid = ui_solid_box(&mut model, DVec3::ZERO, size);
    let dimension = model
        .add_dimension(DVec3::ZERO, DVec3::X, DVec3::Y * 100.)
        .unwrap();
    let text = model
        .add_text(DVec3::ZERO, DVec3::splat(200.), "Note")
        .unwrap();
    let wire = model
        .add_edge(DVec3::splat(300.), DVec3::splat(301.))
        .unwrap();
    let open = ui_solid_box(&mut model, DVec3::splat(20.), DVec3::splat(25.));
    let missing_face = *model
        .expand_entities(&BTreeSet::from([open]))
        .iter()
        .find(|id| model.faces.contains_key(id))
        .unwrap();
    model.faces.remove(&missing_face);
    model
        .objects
        .get_mut(&open)
        .unwrap()
        .entities
        .remove(&missing_face);
    let mut cache = solid_tools::VolumeCache::default();
    for selected in [
        BTreeSet::from([solid]),
        BTreeSet::from([solid, dimension, text, wire, open]),
    ] {
        assert_eq!(cache.solid_selection_size(&model, &selected), Some(size));
    }
    for id in [dimension, text, wire, open, missing_face] {
        assert_eq!(
            cache.solid_selection_size(&model, &BTreeSet::from([id])),
            None
        );
    }
    let contents = model.objects[&solid].entities.clone();
    let loose = model.copy_entities(&contents);
    let assembly = model
        .make_object(
            &BTreeSet::from([solid, dimension, text, wire, open]),
            false,
            "Mixed assembly",
        )
        .unwrap();
    assert_eq!(
        cache.solid_selection_size(&model, &BTreeSet::from([assembly])),
        Some(size)
    );
    // A fully selected ungrouped body still qualifies; an isolated face does not.
    assert_eq!(
        cache.solid_selection_size(&loose, &loose.root_entities()),
        Some(size)
    );
    let face = *loose.faces.keys().next().unwrap();
    assert_eq!(
        cache.solid_selection_size(&loose, &BTreeSet::from([face])),
        None
    );
}

#[test]
fn solid_axis_lengths_refresh_after_geometry_edits_and_model_restoration() {
    let mut model = Model::default();
    let solid = ui_solid_box(&mut model, DVec3::ZERO, DVec3::new(1., 2., 3.));
    let selected = BTreeSet::from([solid]);
    let original = model.clone();
    let mut cache = solid_tools::VolumeCache::default();
    for _ in 0..2 {
        assert_eq!(
            cache.solid_selection_size(&model, &selected),
            Some(DVec3::new(1., 2., 3.))
        );
    }
    for vertex in model.vertices.values_mut() {
        vertex.position.z *= 2.;
    }
    assert_eq!(
        cache.solid_selection_size(&model, &selected),
        Some(DVec3::new(1., 2., 6.))
    );
    assert_eq!(
        cache.solid_selection_size(&original, &selected),
        Some(DVec3::new(1., 2., 3.))
    );
}

#[test]
fn dimensions_locked_during_selection_or_placement_keep_three_lines_and_a_label() {
    fn has_label(shape: &egui::Shape, label: &str, at: Pos2) -> bool {
        match shape {
            egui::Shape::Text(text) => {
                text.galley.job.text == label
                    && (text.pos + text.galley.size() * 0.5).distance(at) < 1.
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_label(shape, label, at)),
            _ => false,
        }
    }

    // Occlusion clipping can tessellate annotation strokes into meshes.
    fn colored_points(shape: &egui::Shape, color: Color32, points: &mut Vec<Pos2>) {
        match shape {
            egui::Shape::LineSegment {
                points: ends,
                stroke,
            } if stroke.color == color => {
                points.extend([ends[0], ends[0].lerp(ends[1], 0.5), ends[1]]);
            }
            egui::Shape::Mesh(mesh) => points.extend(
                mesh.vertices
                    .iter()
                    .filter(|v| v.color == color)
                    .map(|v| v.pos),
            ),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    colored_points(shape, color, points);
                }
            }
            _ => {}
        }
    }

    let a = DVec3::new(-1., -1., 1.);
    let diagonal = DVec3::new(2., 3., 3.);
    let placement = DVec3::new(3., -2., 3.);
    for lock_stage in [0, 1, 2] {
        for (key, axis, b, expected, placement) in [
            (Key::ArrowRight, Axis::X, diagonal, 3., placement),
            (Key::ArrowLeft, Axis::Y, diagonal, 4., placement),
            (Key::ArrowUp, Axis::Z, diagonal, 2., placement),
            (
                Key::ArrowRight,
                Axis::X,
                DVec3::new(-1., 2., 5.),
                5.,
                DVec3::new(1., 0.5, 3.),
            ),
            (
                Key::ArrowLeft,
                Axis::Y,
                DVec3::new(2., -1., 5.),
                5.,
                DVec3::new(0.5, 1., 3.),
            ),
            (
                Key::ArrowUp,
                Axis::Z,
                DVec3::new(2., 3., 1.),
                5.,
                DVec3::new(0.5, 1., 3.),
            ),
        ] {
            let mut h = Harness::new();
            h.app.doc.model.add_edge(a, b).unwrap();
            h.key(Key::I);
            if lock_stage == 0 {
                h.key(key);
            }
            h.click(h.at(a));
            if lock_stage == 1 {
                h.key(key);
            }
            let second = h.at(b);
            h.frame(
                vec![egui::Event::PointerMoved(second)],
                egui::Modifiers::NONE,
            );
            assert!(h.app.snap.unwrap().point.distance(b) < 1e-6);
            if lock_stage < 2 {
                assert_eq!(
                    h.app.measurement_value(),
                    Some(h.app.doc.display.units.format_length(expected))
                );
            }
            h.click(second);
            if lock_stage == 2 {
                h.key(key);
            }
            assert_eq!(
                h.app.measurement_value(),
                Some(h.app.doc.display.units.format_length(expected))
            );
            let points = &h.app.measurement.as_ref().unwrap().points;
            assert_eq!(points.len(), 2);
            assert!(points[0].distance(a) < 1e-6);
            assert!(points[1].distance(b) < 1e-6);

            let placement = h.at(placement);
            let preview = h.frame(
                vec![egui::Event::PointerMoved(placement)],
                egui::Modifiers::NONE,
            );
            let dimension = ketchup_core::Dimension {
                id: 0,
                start: ketchup_core::DimensionAnchor {
                    point: a,
                    weights: vec![],
                },
                end: ketchup_core::DimensionAnchor {
                    point: b,
                    weights: vec![],
                },
                offset: h.app.measurement.as_ref().unwrap().offset,
                axis: Some(axis),
                plane: None,
            };
            let layout = dimension.layout(&h.app.doc.model).map(|point| h.at(point));
            let lines = [
                [layout[0], layout[2]],
                [layout[1], layout[3]],
                [layout[2], layout[3]],
            ];
            assert!(
                lines
                    .iter()
                    .all(|[a, b]| a.is_finite() && b.is_finite() && a.distance(*b) > 1.)
            );
            let mut ink = vec![];
            for shape in &preview.shapes {
                colored_points(&shape.shape, axis_color(axis), &mut ink);
            }
            for [a, b] in lines {
                assert!(
                    ink.iter().any(|p| p.distance(a) < 2.)
                        && ink.iter().any(|p| p.distance(b) < 2.),
                    "Missing dimension stroke {a:?}–{b:?}, axis {axis:?}, stage {lock_stage}"
                );
            }
            let label_at = lines[2][0].lerp(lines[2][1], 0.5);
            assert!(preview.shapes.iter().any(|shape| has_label(
                &shape.shape,
                &h.app.doc.display.units.format_length(expected),
                label_at
            )));

            let placed = h.click(placement);
            let dimension = h.app.doc.model.dimensions.values().next().unwrap();
            let (start, end) = dimension.points(&h.app.doc.model);
            assert!(start.distance(a) < 1e-6);
            assert!(end.distance(b) < 1e-6);
            assert_eq!(dimension.axis, Some(axis));
            assert_eq!(dimension.start.weights.len(), 1);
            assert_eq!(dimension.end.weights.len(), 1);
            assert!((dimension.length(&h.app.doc.model) - expected).abs() < 1e-6);
            if (b - a).dot(axis.vector()).abs() < 1e-7 {
                let [_, _, line_start, line_end] = dimension.layout(&h.app.doc.model);
                assert!((line_start - start).cross(axis.vector()).length() < 1e-6);
                assert!((line_end - end).cross(axis.vector()).length() < 1e-6);
                assert!((line_start - start).length() > 1.);
                assert!((line_end - end).length() > 1.);
            }
            let placed_layout = dimension.layout(&h.app.doc.model).map(|point| h.at(point));
            assert_eq!(placed_layout, layout);
            let mut ink = vec![];
            for shape in &placed.shapes {
                colored_points(&shape.shape, ACCENT, &mut ink);
            }
            for [a, b] in lines {
                assert!(
                    ink.iter().any(|p| p.distance(a) < 2.)
                        && ink.iter().any(|p| p.distance(b) < 2.)
                );
            }
            assert!(placed.shapes.iter().any(|shape| has_label(
                &shape.shape,
                &h.app.doc.display.units.format_length(expected),
                label_at
            )));
        }
    }
}

#[test]
fn dimensions_axis_lock_updates_preview_and_placement_without_moving_endpoints() {
    let (a, b) = (DVec3::new(-1., -1., 1.), DVec3::new(2., 3., 3.));
    for (key, axis, expected) in [
        (Key::ArrowRight, Axis::X, 3.),
        (Key::ArrowLeft, Axis::Y, 4.),
        (Key::ArrowUp, Axis::Z, 2.),
    ] {
        let mut h = Harness::new();
        h.app.doc.model.add_edge(a, b).unwrap();
        let before = h.app.doc.model.clone();
        h.key(Key::I);
        h.click(h.at(a));
        h.click(h.at(b));
        let placement = h.at(DVec3::new(3., -2., 3.));
        h.frame(
            vec![egui::Event::PointerMoved(placement)],
            egui::Modifiers::NONE,
        );
        let endpoints = h.app.measurement.as_ref().unwrap().points.clone();
        h.key(key);
        assert_eq!(h.app.lock, Some(axis));
        assert_eq!(
            h.app.measurement_value(),
            Some(h.app.doc.display.units.format_length(expected))
        );
        assert_eq!(h.app.measurement.as_ref().unwrap().points, endpoints);
        h.click(placement);
        let dimension = h.app.doc.model.dimensions.values().next().unwrap();
        assert_eq!(dimension.axis, Some(axis));
        assert!((dimension.length(&h.app.doc.model) - expected).abs() < 1e-6);
        let [start, end, line_start, line_end] = dimension.layout(&h.app.doc.model);
        assert!((start - a).length() < 1e-6);
        assert!((end - b).length() < 1e-6);
        assert!((line_end - line_start).cross(axis.vector()).length() < 1e-6);
        assert!((line_start - start).dot(axis.vector()).abs() < 1e-6);
        assert!((line_end - end).dot(axis.vector()).abs() < 1e-6);
        assert_eq!(dimension.start.weights.len(), 1);
        assert_eq!(dimension.end.weights.len(), 1);
        h.app.doc.model.validate().unwrap();
        h.app.undo(&h.ctx, false);
        assert_eq!(h.app.doc.model, before);
        h.app.undo(&h.ctx, true);
        assert_eq!(
            h.app.doc.model.dimensions.values().next().unwrap().axis,
            Some(axis)
        );
    }
}

#[test]
fn dimensions_axis_lock_switches_and_clears_during_placement() {
    let mut h = Harness::new();
    let (a, b) = (DVec3::new(-1., -1., 1.), DVec3::new(2., 3., 3.));
    h.app.doc.model.add_edge(a, b).unwrap();
    h.key(Key::I);
    h.click(h.at(a));
    h.click(h.at(b));
    let aligned = h.app.measurement_value();
    h.key(Key::ArrowRight);
    assert_eq!(h.app.measurement_value().as_deref(), Some("3.000 m"));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowLeft);
    assert_eq!(h.app.measurement_value().as_deref(), Some("4.000 m"));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowUp);
    assert_eq!(h.app.measurement_value().as_deref(), Some("2.000 m"));
    h.key(Key::ArrowUp);
    assert_eq!(h.app.lock, None);
    assert_eq!(h.app.measurement_value(), aligned);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert_eq!(h.app.lock, None);
    assert_eq!(h.app.measurement_value(), aligned);
    h.click(h.at(DVec3::new(3., -2., 3.)));
    let dimension = h.app.doc.model.dimensions.values().next().unwrap();
    assert_eq!(dimension.axis, None);
    assert!((dimension.length(&h.app.doc.model) - a.distance(b)).abs() < 1e-6);
}

#[test]
fn units_section_controls_change_measurements_without_editing_geometry() {
    use ketchup_core::{AngleUnit, AreaUnit, LengthUnit, UnitSettings, VolumeUnit};

    let mut h = Harness::new();
    h.ctx.style_mut_of(h.ctx.theme(), |s| s.animation_time = 0.);
    h.size.y = 2000.;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    assert!(text_position(&output, "Properties").y < text_position(&output, "Units").y);
    assert!(text_position(&output, "Units").y < text_position(&output, "Components").y);
    let model = h.app.doc.model.clone();
    for (current, choice) in [
        ("Meters (m)", "Feet (ft)"),
        ("Square meters (m²)", "Square feet (ft²)"),
        ("Cubic meters (m³)", "Liters (L)"),
        ("Degrees (°)", "Radians (rad)"),
    ] {
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, current));
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, choice));
    }
    assert_eq!(
        h.app.doc.display.units,
        UnitSettings {
            length: LengthUnit::Feet,
            area: AreaUnit::SquareFeet,
            volume: VolumeUnit::Liters,
            angle: AngleUnit::Radians,
            ..Default::default()
        }
    );
    assert_eq!(h.app.doc.model, model);
    assert!(!h.app.project.modified);

    let solid = ui_solid_box(&mut h.app.doc.model, DVec3::ZERO, DVec3::ONE);
    h.app.selected = BTreeSet::from([solid]);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "1000.000 L");
    text_position(&output, "64.583 ft²");

    h.app.set_tool(Tool::TapeMeasure);
    h.app.measurement = Some(measurement_tools::Measurement {
        points: vec![DVec3::ZERO, DVec3::X * 0.3048],
        complete: true,
        ..Default::default()
    });
    assert_eq!(h.app.measurement_value().as_deref(), Some("1.000 ft"));
    h.app.set_tool(Tool::Protractor);
    h.app.measurement = Some(measurement_tools::Measurement {
        points: vec![DVec3::ZERO, DVec3::X, DVec3::Y],
        complete: true,
        ..Default::default()
    });
    assert_eq!(h.app.measurement_value().as_deref(), Some("1.5708 rad"));
}

#[test]
fn precision_controls_independently_update_measurement_displays() {
    fn control_on_row(shape: &egui::Shape, y: f32) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(text)
                if matches!(text.galley.job.text.as_str(), "0.000" | "0.00") =>
            {
                let center = text.pos + text.galley.size() * 0.5;
                ((center.y - y).abs() < 8.).then_some(center)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| control_on_row(shape, y)),
            _ => None,
        }
    }

    let mut h = Harness::new();
    h.ctx.style_mut_of(h.ctx.theme(), |s| s.animation_time = 0.);
    h.size.y = 2000.;
    let model = h.app.doc.model.clone();
    for (label, choice) in [
        ("Length precision", "0"),
        ("Area precision", "0.0"),
        ("Volume precision", "0.0000"),
        ("Angle precision", "0.000000"),
    ] {
        let output = h.frame(vec![], egui::Modifiers::NONE);
        let y = text_position(&output, label).y;
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| control_on_row(&shape.shape, y))
            .unwrap();
        h.click(pos);
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, choice));
    }
    assert_eq!(
        h.app.doc.display.units.precision,
        ketchup_core::UnitPrecision {
            length: Some(0),
            area: Some(1),
            volume: Some(4),
            angle: Some(6),
        }
    );
    assert_eq!(h.app.doc.model, model);
    assert!(!h.app.project.modified);

    let solid = ui_solid_box(&mut h.app.doc.model, DVec3::ZERO, DVec3::ONE);
    h.app.selected = BTreeSet::from([solid]);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "6.0 m²");
    text_position(&output, "1.0000 m³");

    h.app.set_tool(Tool::TapeMeasure);
    h.app.measurement = Some(measurement_tools::Measurement {
        points: vec![DVec3::ZERO, DVec3::X * 1.23456],
        complete: true,
        ..Default::default()
    });
    assert_eq!(h.app.measurement_value().as_deref(), Some("1 m"));
    h.app.set_tool(Tool::Protractor);
    h.app.doc.display.units.angle = ketchup_core::AngleUnit::Radians;
    h.app.measurement = Some(measurement_tools::Measurement {
        points: vec![DVec3::ZERO, DVec3::X, DVec3::Y],
        complete: true,
        ..Default::default()
    });
    assert_eq!(h.app.measurement_value().as_deref(), Some("1.570796 rad"));
}

#[test]
fn tape_measure_and_protractor_pointer_respect_each_explicit_axis() {
    let origin = DVec3::new(0.2, 0.3, 0.4);
    for tool in [Tool::TapeMeasure, Tool::Protractor] {
        for (key, axis) in [
            (Key::ArrowRight, Axis::X),
            (Key::ArrowLeft, Axis::Y),
            (Key::ArrowUp, Axis::Z),
        ] {
            let mut h = Harness::new();
            h.app.set_tool(tool);
            h.app.measurement = Some(measurement_tools::Measurement {
                points: vec![origin],
                normal: DVec3::Z,
                ..Default::default()
            });
            let pos = h.at(origin + DVec3::new(2., 3., 4.));
            h.frame(vec![egui::Event::PointerMoved(pos)], egui::Modifiers::NONE);
            h.key(key);
            h.app.update_measurement(pos);
            let delta = h
                .app
                .snap
                .expect("axis-constrained measurement preview")
                .point
                - origin;
            assert!(delta.cross(axis.vector()).length() < 1e-7);
            assert_eq!(h.app.measurement.as_ref().unwrap().points, vec![origin]);
        }
    }
}
