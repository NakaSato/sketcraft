//! Solid tools: union, trim, split, intersect and flip.
use super::*;

#[test]
fn solid_flyout_contains_all_six_tools_and_union_applies_to_selection_with_undo() {
    let mut h = Harness::new();
    let (a, b) = ui_solid_pair(&mut h);
    h.app.selected = BTreeSet::from([a, b]);
    let before = h.app.doc.model.clone();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Union"));
    let menu = h.frame(vec![], egui::Modifiers::NONE);
    for label in ["Flip", "Combine", "Union", "Intersect", "Trim", "Split"] {
        assert_menu_icon(&menu, label);
    }
    h.click(menu_text_position(&menu, "Union"));
    assert_eq!(h.app.tool, Tool::Union);
    assert!(
        !egui::Popup::is_any_open(&h.ctx),
        "Choosing an operation should close its flyout"
    );
    assert_eq!(h.app.solid_tool, Tool::Union);
    assert_eq!(h.app.doc.model.objects.len(), 1);
    assert_eq!(h.app.doc.history.undo_label(), Some("Union"));
    let result = *h.app.selected.first().unwrap();
    assert!((h.app.doc.model.solid_volume(result).unwrap() - 12.).abs() < 1e-6);
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Union"));
    let menu = h.frame(vec![], egui::Modifiers::NONE);
    h.click(menu_text_position(&menu, "Combine"));
    assert_eq!(h.app.solid_tool, Tool::OuterShell);
}

#[test]
fn trim_picks_cutter_then_target_and_cancel_never_edits_geometry() {
    let mut h = Harness::new();
    let (a, b) = ui_solid_pair(&mut h);
    let before = h.app.doc.model.clone();
    h.app.start_solid_tool(Tool::Trim, &h.ctx);
    let cutter = h.at(DVec3::new(0.5, 0., 1.));
    h.click(cutter);
    assert_eq!(h.app.solid_first, Some(a));
    assert_eq!(h.app.doc.model, before);
    h.key(Key::Escape);
    assert!(h.app.solid_first.is_none());
    assert_eq!(h.app.doc.model, before);
    h.click(cutter);
    let target = h.at(DVec3::new(2.5, 0., 1.));
    h.click(target);
    assert!(h.app.doc.model.objects.contains_key(&a));
    assert!(!h.app.doc.model.objects.contains_key(&b));
    assert_eq!(h.app.doc.history.undo_label(), Some("Trim"));
    let result = *h.app.selected.first().unwrap();
    assert!((h.app.doc.model.solid_volume(result).unwrap() - 4.).abs() < 1e-6);
}

#[test]
fn split_and_intersection_keep_the_expected_editable_bodies() {
    for (tool, count, volume) in [(Tool::Split, 3, 12.), (Tool::Intersect, 1, 4.)] {
        let mut h = Harness::new();
        let (a, b) = ui_solid_pair(&mut h);
        h.app.selected = BTreeSet::from([a, b]);
        h.app.start_solid_tool(tool, &h.ctx);
        assert_eq!(h.app.selected.len(), count);
        let actual: f64 = h
            .app
            .selected
            .iter()
            .map(|id| h.app.doc.model.solid_volume(*id).unwrap())
            .sum();
        assert!((actual - volume).abs() < 1e-6);
        h.app.doc.model.validate().unwrap();
    }
}

#[test]
fn intersect_accepts_a_preselected_face_and_group_or_a_face_as_the_first_click() {
    for preselect_both in [false, true] {
        let mut h = Harness::new();
        let object = ui_solid_box(&mut h.app.doc.model, DVec3::ZERO, DVec3::splat(2.));
        let mut sheet = Model::default();
        let points = [
            DVec3::new(-1., -1., 1.),
            DVec3::new(3., -1., 1.),
            DVec3::new(3., 3., 1.),
            DVec3::new(-1., 3., 1.),
        ];
        for i in 0..4 {
            sheet.add_edge(points[i], points[(i + 1) % 4]).unwrap();
        }
        let ids = h.app.doc.model.paste_entities(&sheet, DVec3::ZERO);
        let face = *ids
            .iter()
            .find(|id| h.app.doc.model.faces.contains_key(id))
            .unwrap();
        let before = h.app.doc.model.clone();
        if preselect_both {
            h.app.selected = BTreeSet::from([object, face]);
            h.app.start_solid_tool(Tool::Intersect, &h.ctx);
        } else {
            h.app.start_solid_tool(Tool::Intersect, &h.ctx);
            // Look straight down so the exposed sheet and solid are unambiguous.
            h.app.doc.camera.pitch = 1.55;
            h.app.doc.camera.target = DVec3::new(1., 1., 0.);
            h.click(h.at(DVec3::new(-0.5, 1., 1.)));
            assert_eq!(h.app.solid_first, Some(face));
            assert_eq!(h.app.doc.model, before);
            h.click(h.at(DVec3::new(1., 1., 2.)));
        }
        assert!(h.app.solid_first.is_none());
        assert_eq!(h.app.doc.history.undo_label(), Some("Intersect"));
        let result = h.app.doc.model.copy_entities(&h.app.selected);
        assert_eq!(result.faces.len(), 1);
        assert!(
            (result
                .faces
                .values()
                .map(|f| result.face_area(f))
                .sum::<f64>()
                - 4.)
                .abs()
                < 1e-6
        );
        h.app.doc.model.validate().unwrap();
        h.app.undo(&h.ctx, false);
        assert_eq!(h.app.doc.model, before);
    }
}

#[test]
fn flip_uses_keyboard_inspector_and_clickable_planes_without_detaching_objects() {
    let mut h = Harness::new();
    let (a, _) = ui_solid_pair(&mut h);
    h.app.selected = BTreeSet::from([a]);
    h.app.start_solid_tool(Tool::Flip, &h.ctx);
    let vertices = h.app.doc.model.selection_vertices(&h.app.selected);
    let before = h.app.doc.model.clone();
    let center = h.app.doc.model.selection_center(&h.app.selected).unwrap();
    h.key(Key::ArrowRight);
    for &id in &vertices {
        let (p, q) = (
            before.vertices[&id].position,
            h.app.doc.model.vertices[&id].position,
        );
        assert!((q - DVec3::new(2. * center.x - p.x, p.y, p.z)).length() < 1e-7);
    }
    assert!(h.app.doc.model.objects.contains_key(&a));
    assert_eq!(h.app.doc.history.undo_label(), Some("Flip X"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Y / green"));
    assert_eq!(h.app.doc.history.undo_label(), Some("Flip Y"));
    let point = h.at(center + DVec3::new(0., 0.6, 0.4));
    let axis = h.app.flip_plane_at(point).unwrap();
    h.click(point);
    assert_eq!(
        h.app.doc.history.undo_label(),
        Some(match axis {
            Axis::X => "Flip X",
            Axis::Y => "Flip Y",
            Axis::Z => "Flip Z",
        })
    );
    h.app.doc.model.validate().unwrap();
}
