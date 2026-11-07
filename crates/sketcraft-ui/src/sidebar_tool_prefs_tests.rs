//! Tool choices, axis locks and snapping targets persist without model edits.
use super::*;

#[test]
fn section_axes_survive_immediate_reload_without_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    app.doc.display.section_axes = [true, true, false];
    app.doc.display.section_planes = true;
    app.doc.display.section_offsets = [-3.5, 8.25];
    app.doc.display.section_height = 9.;
    app.doc.display.section_cuts = true;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(!app.project.modified);
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.doc.display.section_axes, [true, true, false]);
    assert!(restored.doc.display.section_planes);
    assert_eq!(restored.doc.display.section_offsets, [-3.5, 8.25]);
    assert_eq!(restored.doc.display.section_height, 9.);
    assert!(restored.doc.display.section_cuts);
}

#[test]
fn every_active_tool_persists_immediately_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    // Select also needs to save after another tool, rather than only being
    // restored by the default for an empty preference store.
    for tool in Tool::ALL.into_iter().rev() {
        storage.flushed = false;
        app.set_tool(tool);
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed, "{tool:?}");
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.tool, tool);
        assert_eq!(
            ToolPreferences::capture(&restored),
            ToolPreferences::capture(&app)
        );
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        assert!(!restored.project.modified);
        assert!(restored.dirty_since.is_none());
    }
}

#[test]
fn two_axis_locks_save_without_model_edits_and_restore_older_preferences() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.set_tool(Tool::Line);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    for axis in [Axis::X, Axis::Y, Axis::X, Axis::Y] {
        app.toggle_axis_lock(axis, &ctx);
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!((restored.lock, restored.second_lock), (app.lock, app.second_lock));
    }
    let old: ToolPreferences = serde_json::from_str(r#"{"axis_lock":"Y"}"#).unwrap();
    old.restore(&mut app);
    assert_eq!((app.lock, app.second_lock), (Some(Axis::Y), None));
    let defaults: ToolPreferences = serde_json::from_str("{}").unwrap();
    defaults.restore(&mut app);
    assert_eq!((app.lock, app.second_lock), (None, None));
    for invalid in [
        r#"{"axis_lock":"X","second_axis_lock":"X"}"#,
        r#"{"second_axis_lock":"Y"}"#,
    ] {
        let prefs: ToolPreferences = serde_json::from_str(invalid).unwrap();
        prefs.restore(&mut app);
        assert!(app.second_lock.is_none());
    }
}

#[test]
fn grid_snapping_defaults_off_for_new_and_older_preferences() {
    for json in ["{}", r#"{"snapping":{"geometry":false,"axes":true}}"#] {
        let preferences: InspectorPreferences = serde_json::from_str(json).unwrap();
        let mut storage = SavedPreferences::default();
        eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
        let mut app = KetchupApp::with_document(Document::default(), None, true);
        assert!(!app.snap_to_grid);
        assert!(app.snap_to_edge);
        assert!(app.snap_to_face);
        app.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert!(!app.snap_to_grid);
        let geometry = preferences.snapping.as_ref().is_none_or(|s| s.geometry);
        assert_eq!(app.snap_to_edge, geometry);
        assert_eq!(app.snap_to_face, geometry);
        assert_eq!(app.snap_to_face_center, geometry);
        assert_eq!(app.doc.snap_to_geometry, geometry);
    }
}

#[test]
fn geometry_targets_restore_every_combination_without_a_model_edit() {
    for edges in [false, true] {
        for faces in [false, true] {
            for centers in [false, true] {
                let ctx = egui::Context::default();
                let mut storage = SavedPreferences::default();
                let mut app = KetchupApp::with_document(Document::default(), None, true);
                app.persist_inspector_changes(&ctx, Some(&mut storage), false);
                storage.flushed = false;
                app.snap_to_edge = edges;
                app.snap_to_face = faces;
                app.snap_to_face_center = centers;
                app.sync_geometry_snapping();
                app.persist_inspector_changes(&ctx, Some(&mut storage), false);
                assert_eq!(storage.flushed, !(edges && faces && centers));
                assert!(!app.project.modified);
                assert!(app.dirty_since.is_none());
                let mut restored = KetchupApp::with_document(Document::default(), None, true);
                restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
                assert_eq!(restored.snap_to_edge, edges);
                assert_eq!(restored.snap_to_face, faces);
                assert_eq!(restored.snap_to_face_center, centers);
                assert_eq!(restored.doc.snap_to_geometry, edges || faces || centers);
            }
        }
    }
}

#[test]
fn legacy_master_off_migrates_to_targets_that_can_be_enabled_independently() {
    let ctx = egui::Context::default();
    let preferences: InspectorPreferences = serde_json::from_str(
        r#"{"snapping":{"geometry":false,"edges":true,"faces":true,"face_centers":true}}"#,
    )
    .unwrap();
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert!(!app.snap_to_edge);
    assert!(!app.snap_to_face);
    assert!(!app.snap_to_face_center);
    app.snap_to_face_center = true;
    app.sync_geometry_snapping();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert!(!restored.snap_to_edge);
    assert!(!restored.snap_to_face);
    assert!(restored.snap_to_face_center);
    assert!(restored.doc.snap_to_geometry);
}

#[test]
fn closing_shortcuts_keeps_the_saved_search_filter() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.help = true;
    app.shortcut_filter = "Move".into();
    let _ = ctx.run_test_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |_| app.keyboard(&ctx),
    );
    assert!(!app.help);
    assert_eq!(app.shortcut_filter, "Move");
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.shortcut_filter, "Move");
}

#[test]
fn rotate_and_its_locks_restore_after_an_immediate_save_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.set_tool(Tool::Rotate);
    app.toggle_axis_lock(Axis::X, &ctx);
    app.toggle_axis_lock(Axis::Y, &ctx);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.tool, Tool::Rotate);
    assert_eq!(restored.plane_lock(), Some(Axis::Z));
    assert!(restored.modeling.is_none());
    assert_eq!(restored.general_settings.tool_shortcut(Tool::Rotate), "Q");
    let defaults: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.tools.active, Tool::Select);
    assert_eq!(defaults.tools.axis_lock, None);
}

#[test]
fn tool_menu_choices_and_options_restore_without_transient_state() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.set_tool(Tool::Polygon);
    app.set_tool(Tool::Text);
    app.set_tool(Tool::Trim);
    app.set_tool(Tool::Move);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    for sides in [3, 17, 64] {
        storage.flushed = false;
        app.polygon_sides = sides;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.polygon_sides, sides);
        assert_eq!(restored.drawing_tool, Tool::Polygon);
        assert_eq!(restored.measurement_tool, Tool::Text);
        assert_eq!(restored.solid_tool, Tool::Trim);
    }
    app.move_copy = true;
    app.paint_color = [10, 20, 30];
    app.paint_target = PaintTarget::Group;
    app.line_start = Some(DVec3::X);
    app.solid_first = Some(123);
    app.selected.insert(123);
    app.length_input = "2m".into();
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.tool, Tool::Move);
    assert!(restored.move_copy);
    assert_eq!(restored.paint_color, [10, 20, 30]);
    assert_eq!(restored.paint_target, PaintTarget::Group);
    assert!(restored.line_start.is_none());
    assert!(restored.solid_first.is_none());
    assert!(restored.selected.is_empty());
    assert!(restored.length_input.is_empty());
    assert_eq!(
        serde_json::to_value(&restored.doc).unwrap(),
        serde_json::to_value(Document::default()).unwrap()
    );

    // Turning an option back off must save just as promptly as enabling it.
    storage.flushed = false;
    app.move_copy = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert!(!restored.move_copy);
}

#[test]
fn older_tool_preferences_keep_defaults_and_invalid_options_are_normalized() {
    for json in ["{}", r#"{"tools":{}}"#] {
        let preferences: InspectorPreferences = serde_json::from_str(json).unwrap();
        let mut storage = SavedPreferences::default();
        eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
        let mut app = KetchupApp::with_document(Document::default(), None, true);
        app.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(ToolPreferences::capture(&app), ToolPreferences::default());
    }
    for (sides, expected) in [(0, 3), (usize::MAX, 64)] {
        let mut preferences = InspectorPreferences::default();
        preferences.tools = ToolPreferences {
            active: Tool::Polygon,
            drawing: Tool::Paint,
            measurement: Tool::Line,
            solid: Tool::Select,
            polygon_sides: sides,
            ..Default::default()
        };
        let mut storage = SavedPreferences::default();
        eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
        let mut app = KetchupApp::with_document(Document::default(), None, true);
        app.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(app.tool, Tool::Polygon);
        assert_eq!(app.drawing_tool, Tool::Polygon);
        assert_eq!(app.measurement_tool, Tool::Dimensions);
        assert_eq!(app.solid_tool, Tool::Union);
        assert_eq!(app.polygon_sides, expected);
    }
}

#[test]
fn paint_target_persists_immediately_and_older_preferences_default_to_face() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    assert_eq!(app.paint_target, PaintTarget::Face);
    for target in [PaintTarget::Group, PaintTarget::Face] {
        storage.flushed = false;
        app.paint_target = target;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.paint_target, target);
    }
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.paint_target, PaintTarget::Face);
}

#[test]
fn large_tool_set_saves_immediately_and_defaults_off_for_older_preferences() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for enabled in [true, false] {
        app.large_tool_set = enabled;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.large_tool_set, enabled);
    }
    let older: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(!older.large_tool_set);
}
