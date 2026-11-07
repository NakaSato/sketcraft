//! Move-copy previews, arrays and modifiers.
use super::*;

#[test]
fn move_copy_modifier_toggles_live_preview_on_windows_and_mac() {
    for mac in [false, true] {
        let mut h = Harness::new();
        let before = begin_move_copy(&mut h, mac);
        let modifier = if mac {
            egui::Modifiers::ALT
        } else {
            egui::Modifiers::CTRL
        };
        let preview = h.app.modeling.as_ref().unwrap().preview.as_ref().unwrap();
        assert_eq!(preview.edges.len(), 2);
        assert_eq!(h.app.doc.model, before);
        h.frame(vec![], modifier);
        assert!(!h.app.move_copy);
        assert_eq!(
            h.app
                .modeling
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
                .edges
                .len(),
            1
        );
        h.frame(vec![], modifier);
        assert!(
            !h.app.move_copy,
            "holding the modifier must not toggle repeatedly"
        );
        h.frame(vec![], egui::Modifiers::NONE);
        h.frame(vec![], modifier);
        h.frame(vec![], egui::Modifiers::NONE);
        modeling_number(&mut h, "6");
        assert_copy_positions(&h, &[0., 6.]);
        assert_eq!(
            h.app.doc.model.edges[before.edges.keys().next().unwrap()],
            *before.edges.values().next().unwrap()
        );
        assert_eq!(h.app.selected.len(), 1);
        assert!(h.app.selected.iter().all(|id| !before.contains_entity(id)));
        assert_eq!(h.app.doc.history.undo_label(), Some("Move copy"));
        h.app.undo(&h.ctx, false);
        assert_eq!(h.app.doc.model, before);
        h.app.undo(&h.ctx, true);
        assert_copy_positions(&h, &[0., 6.]);
    }
}

#[test]
fn move_copy_arrays_divide_repeat_revise_and_undo_as_one_operation() {
    let mut h = Harness::new();
    let before = begin_move_copy(&mut h, false);
    let end = h.at(DVec3::X * 6.);
    h.click(end);
    assert_copy_positions(&h, &[0., 6.]);
    for (input, positions) in [
        ("/3", vec![0., 2., 4., 6.]),
        ("/2", vec![0., 3., 6.]),
        ("3x", vec![0., 6., 12., 18.]),
        ("x2", vec![0., 6., 12.]),
        ("3/", vec![0., 2., 4., 6.]),
        ("2*", vec![0., 6., 12.]),
        ("*3", vec![0., 6., 12., 18.]),
        ("/1", vec![0., 6.]),
    ] {
        modeling_number(&mut h, input);
        assert_copy_positions(&h, &positions);
        assert!(h.app.length_input.is_empty());
    }
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
    assert_eq!(h.app.doc.history.undo_label(), None);
    h.app.undo(&h.ctx, true);
    assert_copy_positions(&h, &[0., 6.]);
    assert!(h.app.last_move_copy.is_none());
}

#[test]
fn move_copy_invalid_arrays_are_atomic_and_escape_keeps_placed_copies() {
    let mut h = Harness::new();
    begin_move_copy(&mut h, false);
    modeling_number(&mut h, "6");
    let placed = h.app.doc.model.clone();
    for input in ["/0", "/-2", "/2.5", "/1001", "/", "2x3", "1e3x", "6"] {
        h.app.length_input.clear();
        modeling_number(&mut h, input);
        assert_eq!(h.app.doc.model, placed, "invalid input {input}");
        assert!(h.app.last_move_copy.is_some());
    }
    h.key(Key::Escape);
    assert_eq!(h.app.doc.model, placed);
    assert!(h.app.last_move_copy.is_none());
    assert!(!h.app.move_copy);
    assert!(h.app.length_input.is_empty());
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edges.len(), 1);
}

#[test]
fn move_copy_cancel_tool_switch_and_other_edits_clear_array_state() {
    let mut h = Harness::new();
    let before = begin_move_copy(&mut h, false);
    h.key(Key::Escape);
    assert_eq!(h.app.doc.model, before);
    assert!(h.app.modeling.is_none());
    assert!(!h.app.move_copy);
    assert_eq!(h.app.doc.history.undo_label(), None);

    let start = h.at(DVec3::ZERO);
    h.click(start);
    h.key(Key::ArrowRight);
    let end = h.at(DVec3::X * 6.);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    h.frame(vec![], egui::Modifiers::CTRL);
    h.frame(vec![], egui::Modifiers::NONE);
    modeling_number(&mut h, "6");
    assert!(h.app.last_move_copy.is_some());
    h.key(Key::Delete);
    assert!(h.app.last_move_copy.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 1);
    h.key(Key::Space);
    assert!(!h.app.move_copy);
}

#[test]
fn move_copy_uses_option_on_mac_and_preserves_clipboard_shortcuts() {
    let mut h = Harness::new();
    h.ctx.set_os(egui::os::OperatingSystem::Mac);
    h.key(Key::M);
    for modifier in [egui::Modifiers::CTRL, egui::Modifiers::MAC_CMD] {
        h.frame(vec![], modifier);
        assert!(!h.app.move_copy);
        h.frame(vec![], egui::Modifiers::NONE);
    }
    h.ctx.set_os(egui::os::OperatingSystem::Windows);
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::Y).unwrap();
    h.app.selected.insert(edge);
    h.frame(vec![egui::Event::Copy], egui::Modifiers::CTRL);
    assert!(!h.app.move_copy);
    assert!(h.app.clipboard.is_some());
}

#[test]
fn move_copy_arrays_preserve_nested_objects_colors_and_component_links() {
    for component in [false, true] {
        let mut h = Harness::new();
        let face = modeling_rectangle(&mut h);
        h.app.doc.model.face_colors.insert(face, [12, 34, 56]);
        let inner = h
            .app
            .doc
            .model
            .make_object(&h.app.doc.model.selectable_entities(), false, "Inner")
            .unwrap();
        let outer = h
            .app
            .doc
            .model
            .make_object(&BTreeSet::from([inner]), component, "Outer")
            .unwrap();
        let before = h.app.doc.model.clone();
        let kind = before.objects[&outer].kind.clone();
        h.app.selected = BTreeSet::from([outer]);
        h.key(Key::M);
        h.frame(vec![], egui::Modifiers::CTRL);
        h.frame(vec![], egui::Modifiers::NONE);
        let anchor = h.at(DVec3::ZERO);
        h.click(anchor);
        h.key(Key::ArrowUp);
        let end = h.at(DVec3::Z * 6.);
        h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
        modeling_number(&mut h, "6");
        modeling_number(&mut h, "/3");
        assert_eq!(h.app.doc.model.root_entities().len(), 4);
        assert_eq!(h.app.doc.model.objects.len(), 8);
        assert_eq!(h.app.doc.model.faces.len(), 4);
        assert!(
            h.app
                .doc
                .model
                .face_colors
                .values()
                .all(|color| *color == [12, 34, 56])
        );
        assert_eq!(h.app.doc.model.face_colors.len(), 4);
        let mut heights = Vec::new();
        for root in h.app.doc.model.root_entities() {
            assert_eq!(h.app.doc.model.objects[&root].kind, kind);
            let children = &h.app.doc.model.objects[&root].entities;
            assert_eq!(children.len(), 1);
            assert!(
                h.app
                    .doc
                    .model
                    .objects
                    .contains_key(children.first().unwrap())
            );
            heights.push(
                h.app
                    .doc
                    .model
                    .selection_center(&BTreeSet::from([root]))
                    .unwrap()
                    .z,
            );
        }
        heights.sort_by(f64::total_cmp);
        assert_eq!(heights, vec![0., 2., 4., 6.]);
        h.app.doc.model.validate().unwrap();
        h.app.undo(&h.ctx, false);
        assert_eq!(h.app.doc.model, before);
    }
}

#[test]
fn move_copy_arrays_inside_component_stay_in_context_and_update_peers() {
    let mut h = Harness::new();
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::Y).unwrap();
    let component = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), true, "Assembly")
        .unwrap();
    let source = h.app.doc.model.copy_entities(&BTreeSet::from([component]));
    let peer = *h
        .app
        .doc
        .model
        .paste_entities(&source, DVec3::Y * 4.)
        .first()
        .unwrap();
    assert!(h.app.doc.model.enter_object(component));
    h.app.selected = h.app.doc.model.selectable_entities();
    let before = h.app.doc.model.clone();
    h.key(Key::M);
    let anchor = h.at(DVec3::ZERO);
    h.click(anchor);
    h.key(Key::ArrowRight);
    let end = h.at(DVec3::X * 6.);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    h.frame(vec![], egui::Modifiers::CTRL);
    h.frame(vec![], egui::Modifiers::NONE);
    modeling_number(&mut h, "6");
    modeling_number(&mut h, "/3");
    assert_eq!(h.app.doc.model.edit_context, Some(component));
    assert_eq!(h.app.doc.model.objects[&component].entities.len(), 4);
    assert_eq!(h.app.doc.model.objects[&peer].entities.len(), 4);
    assert!(
        h.app
            .selected
            .iter()
            .all(|id| h.app.doc.model.parent(*id) == Some(component))
    );
    assert_eq!(h.app.doc.model.root_entities().len(), 2);
    h.app.doc.model.validate().unwrap();
    h.key(Key::Escape);
    assert_eq!(h.app.doc.model.edit_context, Some(component));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
}

#[test]
fn move_copy_array_can_be_entered_in_the_measurements_field() {
    let mut h = Harness::new();
    begin_move_copy(&mut h, false);
    modeling_number(&mut h, "6");
    h.click(egui::pos2(h.size.x - 90., h.size.y - 21.));
    assert!(
        h.ctx.egui_wants_keyboard_input(),
        "click should focus the Measurements field"
    );
    modeling_number(&mut h, "/3");
    assert_copy_positions(&h, &[0., 2., 4., 6.]);
    assert!(h.app.length_input.is_empty());
}
