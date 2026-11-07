//! Groups and components: context menus, clipboard, edit contexts, history.
use super::*;

#[test]
fn context_menu_preserves_multiselection_groups_and_ungroups_through_pointer_input() {
    let mut h = Harness::new();
    let a = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    let b = h
        .app
        .doc
        .model
        .add_edge(DVec3::Y * 2., DVec3::new(2., 2., 0.))
        .unwrap();
    h.app.selected = BTreeSet::from([a, b]);
    let point = h.at(DVec3::X);
    let menu = open_context_menu(&mut h, point);
    assert_eq!(h.app.selected, BTreeSet::from([a, b]));
    for label in [
        "Make Group…",
        "Create Component…",
        "Ungroup",
        "Explode",
        "Cut",
        "Copy",
        "Paste",
        "Paste in Place",
        "Delete",
        "Hide",
        "Lock",
    ] {
        text_position(&menu, label);
    }
    h.click(text_position(&menu, "Make Group…"));
    assert!(h.app.doc.model.objects.is_empty());
    assert_eq!(h.app.selected, BTreeSet::from([a, b]));
    let creation = h.app.object_creation.as_mut().unwrap();
    assert!(!creation.component);
    h.frame(
        vec![egui::Event::Text("Assembly".into())],
        egui::Modifiers::NONE,
    );
    let dialog = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&dialog, "Make Group");
    h.click(text_position(&dialog, "Create"));
    assert_eq!(h.app.doc.model.objects.len(), 1);
    let group = *h.app.selected.first().unwrap();
    assert_eq!(h.app.doc.model.objects[&group].entities.len(), 2);
    assert_eq!(h.app.doc.model.objects[&group].name, "Assembly");
    let menu = open_context_menu(&mut h, point);
    h.click(text_position(&menu, "Ungroup"));
    assert!(h.app.doc.model.objects.is_empty());
    assert_eq!(h.app.selected.len(), 2);
    assert_eq!(h.app.doc.history.undo_label(), Some("Ungroup"));
}

#[test]
fn context_menu_creates_named_component_and_explodes_it() {
    let mut h = Harness::new();
    h.app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    let point = h.at(DVec3::X);
    let menu = open_context_menu(&mut h, point);
    h.click(text_position(&menu, "Create Component…"));
    assert!(h.app.object_creation.is_some());
    assert!(h.app.doc.model.objects.is_empty());
    h.frame(
        vec![egui::Event::Text("Beam".into())],
        egui::Modifiers::NONE,
    );
    let dialog = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&dialog, "Create"));
    let id = *h.app.selected.first().unwrap();
    assert_eq!(h.app.doc.model.objects[&id].name, "Beam");
    assert!(matches!(
        h.app.doc.model.objects[&id].kind,
        ObjectKind::Component { .. }
    ));
    let menu = open_context_menu(&mut h, point);
    h.click(text_position(&menu, "Explode"));
    assert!(h.app.doc.model.objects.is_empty());
    h.app.doc.model.validate().unwrap();
}

#[test]
fn clipboard_events_support_in_place_paste_click_placement_cut_cancel_and_undo() {
    let mut h = Harness::new();
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    let id = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    h.app.selected.insert(id);
    h.frame(vec![egui::Event::Copy], egui::Modifiers::NONE);
    assert!(h.app.clipboard.is_some());
    h.frame(
        vec![egui::Event::Paste(String::new())],
        egui::Modifiers::SHIFT,
    );
    assert!(h.app.paste_preview.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 2);
    let edge = &h.app.doc.model.edges[h.app.selected.first().unwrap()];
    assert_eq!(h.app.doc.model.points(edge), (DVec3::ZERO, DVec3::X * 2.));
    h.frame(
        vec![egui::Event::Paste(String::new())],
        egui::Modifiers::NONE,
    );
    assert!(h.app.paste_preview.is_some());
    h.key(Key::Escape);
    assert!(h.app.paste_preview.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 2);
    h.frame(
        vec![egui::Event::Paste(String::new())],
        egui::Modifiers::NONE,
    );
    h.click(h.at(DVec3::new(4., 3., 0.)));
    assert!(h.app.paste_preview.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 3);
    let edge = &h.app.doc.model.edges[h.app.selected.first().unwrap()];
    let (a, b) = h.app.doc.model.points(edge);
    assert!(((a + b) * 0.5).distance(DVec3::new(4., 3., 0.)) < 1e-5);
    h.frame(vec![egui::Event::Cut], egui::Modifiers::NONE);
    assert_eq!(h.app.doc.model.edges.len(), 2);
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edges.len(), 3);
    h.app.doc.model.validate().unwrap();
}

#[test]
fn context_menu_lock_protects_delete_and_hide_can_be_reversed_from_outliner() {
    let mut h = Harness::new();
    let id = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    let point = h.at(DVec3::X);
    let menu = open_context_menu(&mut h, point);
    h.click(text_position(&menu, "Lock"));
    assert!(h.app.doc.model.locked.contains(&id));
    h.key(Key::Delete);
    h.frame(vec![egui::Event::Cut], egui::Modifiers::NONE);
    assert_eq!(h.app.doc.model.edges.len(), 1);
    let menu = open_context_menu(&mut h, point);
    h.click(text_position(&menu, "Unlock"));
    assert!(h.app.doc.model.locked.is_empty());
    let menu = open_context_menu(&mut h, point);
    h.click(text_position(&menu, "Hide"));
    assert!(h.app.selected.is_empty());
    assert!(h.app.doc.model.hidden.contains(&id));
    let tree = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&tree, "Geometry (1)"));
    h.frame(vec![], egui::Modifiers::NONE);
    let tree = h.frame(vec![], egui::Modifiers::NONE);
    let row = text_position(&tree, &format!("#{id}"));
    let menu = open_context_menu(&mut h, row);
    h.click(text_position(&menu, "Unhide"));
    assert!(h.app.doc.model.hidden.is_empty());
    h.click(point);
    assert_eq!(h.app.selected, BTreeSet::from([id]));
}

#[test]
fn system_clipboard_geometry_can_be_pasted_into_a_different_window() {
    let mut source = Harness::new();
    source
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    source.app.selected = source.app.doc.model.selectable_entities();
    let copied = source.frame(vec![egui::Event::Copy], egui::Modifiers::NONE);
    let text = copied
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    let mut destination = Harness::new();
    destination.frame(vec![egui::Event::Paste(text)], egui::Modifiers::SHIFT);
    assert_eq!(destination.app.doc.model.edges.len(), 1);
    destination.app.doc.model.validate().unwrap();
    let before = destination.app.doc.model.clone();
    destination.frame(
        vec![egui::Event::Paste("ketchup-geometry-v1\n{}".into())],
        egui::Modifiers::SHIFT,
    );
    assert_eq!(destination.app.doc.model, before);
}

#[test]
fn double_click_enters_nested_objects_and_escape_and_breadcrumb_leave_levels() {
    for component in [false, true] {
        let mut h = Harness::new();
        let edge = h
            .app
            .doc
            .model
            .add_edge(DVec3::ZERO, DVec3::X * 2.)
            .unwrap();
        h.app
            .doc
            .model
            .make_object(&BTreeSet::from([edge]), !component, "Inner")
            .unwrap();
        let outer = h
            .app
            .doc
            .model
            .make_object(&h.app.doc.model.selectable_entities(), component, "Outer")
            .unwrap();
        let inner = *h.app.doc.model.objects[&outer]
            .entities
            .iter()
            .find(|id| h.app.doc.model.objects.contains_key(id))
            .unwrap();
        double_click_object(&mut h, DVec3::X);
        assert_eq!(h.app.doc.model.edit_context, Some(outer));
        assert!(h.app.selected.is_empty());
        h.click(h.at(DVec3::X));
        assert_eq!(h.app.selected, BTreeSet::from([inner]));
        double_click_object(&mut h, DVec3::X);
        assert_eq!(h.app.doc.model.edit_path(), vec![outer, inner]);
        let frame = h.frame(vec![], egui::Modifiers::NONE);
        text_position(&frame, "Outer");
        text_position(&frame, "Inner");
        h.key(Key::Escape);
        assert_eq!(h.app.doc.model.edit_context, Some(outer));
        let frame = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&frame, "Model"));
        assert!(h.app.doc.model.edit_context.is_none());
    }
}

#[test]
fn editing_group_children_supports_line_delete_clipboard_and_undo_without_affecting_outside() {
    let mut h = Harness::new();
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    let group = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Editable")
        .unwrap();
    let outside = h
        .app
        .doc
        .model
        .add_edge(DVec3::Y * 3., DVec3::new(2., 3., 0.))
        .unwrap();
    double_click_object(&mut h, DVec3::X);
    h.key(Key::L);
    h.click(h.at(DVec3::ZERO));
    h.key(Key::ArrowUp);
    h.frame(
        vec![egui::Event::PointerMoved(h.at(DVec3::Z))],
        egui::Modifiers::NONE,
    );
    h.click(h.at(DVec3::Z));
    assert_eq!(
        h.app.doc.model.object_contents(group).unwrap().edges.len(),
        2
    );
    assert_eq!(h.app.doc.model.parent(outside), None);
    h.key(Key::Escape); // Cancel the unfinished chain first.
    assert_eq!(h.app.doc.model.edit_context, Some(group));
    h.key(Key::Space);
    h.time += 0.4;
    h.click(h.at(DVec3::Z * 0.5));
    assert_eq!(h.app.selected.len(), 1);
    assert!(!h.app.selected.contains(&group));
    h.frame(vec![egui::Event::Copy], egui::Modifiers::NONE);
    h.frame(
        vec![egui::Event::Paste(String::new())],
        egui::Modifiers::SHIFT,
    );
    assert_eq!(
        h.app.doc.model.object_contents(group).unwrap().edges.len(),
        3
    );
    h.key(Key::Delete);
    assert_eq!(
        h.app.doc.model.object_contents(group).unwrap().edges.len(),
        2
    );
    assert!(h.app.doc.model.edges.contains_key(&outside));
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edit_context, Some(group));
    assert_eq!(
        h.app.doc.model.object_contents(group).unwrap().edges.len(),
        3
    );
    h.app.doc.model.validate().unwrap();
}

#[test]
fn double_click_component_allows_push_pull_and_updates_its_other_instance() {
    let mut h = Harness::new();
    let points = [
        DVec3::ZERO,
        DVec3::X * 2.,
        DVec3::new(2., 2., 0.),
        DVec3::Y * 2.,
    ];
    for i in 0..4 {
        h.app
            .doc
            .model
            .add_edge(points[i], points[(i + 1) % 4])
            .unwrap();
    }
    let component = h
        .app
        .doc
        .model
        .make_object(&h.app.doc.model.selectable_entities(), true, "Panel")
        .unwrap();
    let copy = h.app.doc.model.copy_entities(&BTreeSet::from([component]));
    let peer = *h
        .app
        .doc
        .model
        .paste_entities(&copy, DVec3::X * 5.)
        .first()
        .unwrap();
    double_click_object(&mut h, DVec3::new(1., 1., 0.));
    assert_eq!(h.app.doc.model.edit_context, Some(component));
    h.key(Key::P);
    h.click(h.at(DVec3::new(1., 1., 0.)));
    assert!(h.app.modeling.is_some());
    h.frame(
        vec![
            egui::Event::PointerMoved(h.at(DVec3::new(1., 1., 1.))),
            egui::Event::Text("1".into()),
        ],
        egui::Modifiers::NONE,
    );
    h.key(Key::Enter);
    assert_eq!(
        h.app
            .doc
            .model
            .object_contents(component)
            .unwrap()
            .faces
            .len(),
        6
    );
    assert_eq!(
        h.app.doc.model.object_contents(peer).unwrap().faces.len(),
        6
    );
    assert!(
        h.app
            .selected
            .iter()
            .all(|id| h.app.doc.model.parent(*id) == Some(component))
    );
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model.edit_context, Some(component));
    assert_eq!(
        h.app
            .doc
            .model
            .object_contents(component)
            .unwrap()
            .faces
            .len(),
        1
    );
    assert_eq!(
        h.app.doc.model.object_contents(peer).unwrap().faces.len(),
        1
    );
    h.app.doc.model.validate().unwrap();
}

#[test]
fn locked_objects_cannot_be_opened_and_single_click_outside_closes_the_current_level() {
    let mut h = Harness::new();
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    let group = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Locked")
        .unwrap();
    h.app.doc.model.locked.insert(group);
    double_click_object(&mut h, DVec3::X);
    assert!(h.app.doc.model.edit_context.is_none());
    h.app.doc.model.locked.clear();
    double_click_object(&mut h, DVec3::X);
    assert_eq!(h.app.doc.model.edit_context, Some(group));
    h.time += 0.4;
    h.click(h.at(DVec3::Y * 4.));
    assert!(h.app.doc.model.edit_context.is_none());
}

#[test]
fn outside_click_leaves_one_nested_level_without_selecting_outside_geometry() {
    for component in [false, true] {
        let mut h = Harness::new();
        let edge = h
            .app
            .doc
            .model
            .add_edge(DVec3::ZERO, DVec3::X * 2.)
            .unwrap();
        let inner = h
            .app
            .doc
            .model
            .make_object(&BTreeSet::from([edge]), component, "Inner")
            .unwrap();
        let outer = h
            .app
            .doc
            .model
            .make_object(&BTreeSet::from([inner]), false, "Outer")
            .unwrap();
        let inner = *h.app.doc.model.objects[&outer]
            .entities
            .iter()
            .next()
            .unwrap();
        let outside_point = DVec3::Y * 4. + DVec3::X;
        h.app
            .doc
            .model
            .add_edge(DVec3::Y * 4., DVec3::Y * 4. + DVec3::X * 2.)
            .unwrap();
        double_click_object(&mut h, DVec3::X);
        double_click_object(&mut h, DVec3::X);
        assert_eq!(h.app.doc.model.edit_path(), vec![outer, inner]);
        h.time += 0.4;
        h.click(h.at(DVec3::X));
        assert_eq!(h.app.doc.model.edit_context, Some(inner));
        assert!(!h.app.selected.is_empty());
        h.time += 0.4;
        h.click(h.at(outside_point));
        assert_eq!(h.app.doc.model.edit_context, Some(outer));
        assert!(h.app.selected.is_empty());
        h.click(h.at(outside_point));
        assert!(h.app.doc.model.edit_context.is_none());
        assert!(h.app.selected.is_empty());
    }
}

#[test]
fn shift_click_and_box_selection_outside_keep_the_group_open() {
    let mut h = Harness::new();
    let edge = h
        .app
        .doc
        .model
        .add_edge(DVec3::ZERO, DVec3::X * 2.)
        .unwrap();
    let group = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Group")
        .unwrap();
    double_click_object(&mut h, DVec3::X);
    let outside = h.at(DVec3::Y * 4.);
    h.pointer(outside, egui::PointerButton::Primary, true, true);
    h.pointer(outside, egui::PointerButton::Primary, false, true);
    assert_eq!(h.app.doc.model.edit_context, Some(group));
    h.pointer(outside, egui::PointerButton::Primary, true, false);
    h.pointer(
        outside + egui::vec2(30., 30.),
        egui::PointerButton::Primary,
        false,
        false,
    );
    assert_eq!(h.app.doc.model.edit_context, Some(group));
}

#[test]
fn library_placement_creates_linked_instance_supports_cancel_and_undo() {
    let mut h = Harness::new();
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let component = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), true, "Beam")
        .unwrap();
    let before = h.app.doc.model.clone();
    h.app.begin_component_placement(component, &h.ctx);
    assert!(h.app.paste_preview.is_some());
    h.key(Key::Escape);
    assert!(h.app.paste_preview.is_none());
    assert_eq!(h.app.doc.model, before);
    h.app.begin_component_placement(component, &h.ctx);
    let at = h.at(DVec3::new(0., 3., 0.));
    h.click(at);
    assert_eq!(h.app.doc.model.objects.len(), 2);
    assert!(
        h.app
            .doc
            .model
            .objects
            .values()
            .all(|o| o.kind == before.objects[&component].kind)
    );
    h.app.doc.model.validate().unwrap();
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, before);
}

#[test]
fn history_navigation_updates_model_and_clears_transient_selection() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let initial = app.doc.model.clone();
    for y in [0., 1., 2.] {
        let before = app.doc.model.clone();
        app.doc
            .model
            .add_edge(DVec3::Y * y, DVec3::Y * y + DVec3::X)
            .unwrap();
        app.finish_edit("Draw line", before, &ctx);
    }
    let latest = app.doc.model.clone();
    app.selected
        .insert(*app.doc.model.edges.keys().next().unwrap());
    app.navigate_history(&ctx, 0);
    assert_eq!(app.doc.model, initial);
    assert!(app.selected.is_empty());
    assert!(app.project.modified);
    assert_eq!(app.doc.history.position(), 0);
    app.navigate_history(&ctx, 3);
    assert_eq!(app.doc.model, latest);
    app.undo(&ctx, false);
    assert_eq!(app.doc.history.position(), 2);
    app.undo(&ctx, true);
    assert_eq!(app.doc.model, latest);
}

#[test]
fn history_hover_preview_leaves_document_history_and_dirty_state_unchanged() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let initial = app.doc.model.clone();
    for y in [0., 1.] {
        let before = app.doc.model.clone();
        app.doc.model.add_edge(DVec3::Y * y, DVec3::Y * y + DVec3::X).unwrap();
        app.finish_edit("Draw line", before, &ctx);
    }
    app.project.modified = false;
    let document = serde_json::to_string(&app.doc).unwrap();
    let dirty_since = app.dirty_since;
    app.preview_history(0);
    assert_eq!(app.history_preview.as_ref(), Some(&initial));
    assert_eq!(serde_json::to_string(&app.doc).unwrap(), document);
    assert_eq!(app.dirty_since, dirty_since);
    assert!(!app.project.modified);
    // Hovering the current state cancels the preview.
    app.preview_history(2);
    assert!(app.history_preview.is_none());
    app.preview_history(0);
    app.navigate_history(&ctx, 0);
    assert!(app.history_preview.is_none());
    assert_eq!(app.doc.model, initial);
    assert_eq!(app.doc.history.position(), 0);
    assert!(app.project.modified);
}

#[test]
fn g_makes_a_component_from_the_selection_and_never_switches_to_polygon() {
    let mut h = Harness::new();
    h.app.doc.model.add_edge(DVec3::ZERO, DVec3::X * 2.).unwrap();
    let tool = h.app.tool;
    h.key(Key::G);
    assert!(h.app.object_creation.is_none(), "nothing selected: just a hint");
    assert_eq!(h.app.tool, tool);

    h.app.selected = h.app.doc.model.selectable_entities();
    h.key(Key::G);
    let creation = h.app.object_creation.as_ref().expect("G opens Make Component");
    assert!(creation.component);
    assert_ne!(h.app.tool, Tool::Polygon);
    let dialog = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&dialog, "Create"));
    let id = *h.app.selected.first().unwrap();
    assert!(matches!(h.app.doc.model.objects[&id].kind, ObjectKind::Component { .. }));
}
