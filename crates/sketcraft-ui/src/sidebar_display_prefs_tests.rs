//! Display, units and view preferences persist per app or per model.
use super::*;

#[test]
fn group_emphasis_survives_immediate_reload_and_project_changes() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for enabled in [false, true] {
        app.doc.display.group_emphasis = enabled;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.doc.display.group_emphasis, enabled);
        restored.new_model(&ctx);
        assert_eq!(restored.doc.display.group_emphasis, enabled);
        restored.replace_project(Document::default(), None, &ctx);
        assert_eq!(restored.doc.display.group_emphasis, enabled);
    }
    let old: DisplaySettings = serde_json::from_str("{}").unwrap();
    assert!(old.group_emphasis);
}

#[test]
fn group_names_save_without_model_edits_and_default_off() {
    let old: DisplaySettings = serde_json::from_str("{}").unwrap();
    assert!(!old.group_names);
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for enabled in [true, false] {
        app.doc.display.group_names = enabled;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.doc.display.group_names, enabled);
        restored.new_model(&ctx);
        assert_eq!(restored.doc.display.group_names, enabled);
    }
}

#[test]
fn grid_and_wood_joinery_save_without_model_edits_and_default_on() {
    let old: DisplaySettings = serde_json::from_str("{}").unwrap();
    assert!(old.grid);
    assert!(old.wood_joinery);
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for enabled in [true, false] {
        app.doc.display.grid = enabled;
        app.doc.display.wood_joinery = enabled;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.doc.display.grid, enabled);
        assert_eq!(restored.doc.display.wood_joinery, enabled);
        restored.new_model(&ctx);
        assert_eq!(restored.doc.display.grid, enabled);
        assert_eq!(restored.doc.display.wood_joinery, enabled);
    }
}

#[test]
fn shadow_antialiasing_saves_immediately_and_defaults_on() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for enabled in [false, true] {
        app.doc.display.shadow_antialiasing = enabled;
        app.general_settings.section = settings::Section::Rendering;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.doc.display.shadow_antialiasing, enabled);
        assert_eq!(restored.general_settings.section, settings::Section::Rendering);
        restored.new_model(&ctx);
        assert_eq!(restored.doc.display.shadow_antialiasing, enabled);
        // Opening a project restores that project's display settings;
        // only new projects inherit the remembered display preference.
        let mut project = Document::default();
        project.display.shadow_antialiasing = !enabled;
        restored.replace_project(project, None, &ctx);
        assert_eq!(restored.doc.display.shadow_antialiasing, !enabled);
    }
    let old: DisplaySettings = serde_json::from_str("{}").unwrap();
    assert!(old.shadow_antialiasing);
    let previous: DisplaySettings = serde_json::from_str(r#"{"antialiasing":false}"#).unwrap();
    assert!(!previous.shadow_antialiasing);
}

#[test]
fn frame_rate_diagnostics_save_immediately_and_default_off() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for enabled in [true, false] {
        app.general_settings.show_fps = enabled;
        app.general_settings.section = settings::Section::Diagnostics;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.general_settings.show_fps, enabled);
        assert_eq!(restored.general_settings.section, settings::Section::Diagnostics);
    }
    let old: settings::GeneralSettings = serde_json::from_str("{}").unwrap();
    assert!(!old.show_fps);
}

#[test]
fn shadows_and_sun_save_together_and_default_off_for_older_preferences() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    for visible in [true, false] {
        app.doc.display.shadows = visible;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.doc.display.shadows, visible);
        restored.new_model(&ctx);
        assert_eq!(restored.doc.display.shadows, visible);
    }
    let old: DisplaySettings = serde_json::from_str("{}").unwrap();
    assert!(!old.shadows);
}

#[test]
fn each_additional_option_saves_independently_without_a_model_edit() {
    let options: [(&str, fn(&mut KetchupApp)); 15] = [
        ("face center snapping", |app| app.snap_to_face_center = false),
        ("axis snapping", |app| app.doc.snap_to_axes = false),
        ("edge snapping", |app| app.snap_to_edge = false),
        ("face snapping", |app| app.snap_to_face = false),
        ("grid snapping", |app| app.snap_to_grid = true),
        ("axis lock", |app| app.lock = Some(Axis::Y)),
        ("push/pull axis lock", |app| {
            app.set_tool(Tool::PushPull);
            app.lock = Some(Axis::X);
        }),
        ("shortcut filter", |app| app.shortcut_filter = "Move".into()),
        ("stock rotation", |app| {
            app.project.cutting_plan_layout.allow_rotation = false
        }),
        ("sheet width", |app| {
            app.project.cutting_plan_layout.sheet_width_mm = 600.
        }),
        ("sheet length", |app| {
            app.project.cutting_plan_layout.sheet_length_mm = 1200.
        }),
        ("board length", |app| {
            app.project.cutting_plan_layout.board_length_mm = 3600.
        }),
        ("saw kerf", |app| {
            app.project.cutting_plan_layout.kerf_mm = 4.5
        }),
        ("whole project scope", |app| {
            app.project.cutting_plan_selection_preference = Some(false)
        }),
        ("selected parts scope", |app| {
            app.project.cutting_plan_selection_preference = Some(true)
        }),
    ];
    for (name, change) in options {
        let ctx = egui::Context::default();
        let mut storage = SavedPreferences::default();
        let mut app = KetchupApp::with_document(Document::default(), None, true);
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        storage.flushed = false;
        change(&mut app);
        let expected = app.current_inspector_preferences(&ctx);
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed, "{name}");
        assert!(!app.project.modified, "{name}");
        assert!(app.dirty_since.is_none(), "{name}");
        let restored_ctx = egui::Context::default();
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(
            restored.current_inspector_preferences(&restored_ctx),
            expected,
            "{name}"
        );
        assert!(!restored.project.modified, "{name}");
        assert!(
            restored.project.cutting_plan_selection.is_none(),
            "restoring scope must not open a modal"
        );
        storage.flushed = false;
        restored.persist_inspector_changes(&restored_ctx, Some(&mut storage), false);
        assert!(
            !storage.flushed,
            "{name}: unchanged settings must not write every frame"
        );
    }
}

#[test]
fn missing_corrupt_or_invalid_preferences_preserve_document_display() {
    let ctx = egui::Context::default();
    let mut doc = Document::default();
    doc.display.axes = false;
    let mut app = KetchupApp::with_document(doc.clone(), None, true);
    app.restore_inspector_preferences(&ctx, None);
    assert_eq!(app.doc.display, doc.display);
    let mut storage = SavedPreferences::default();
    storage.set_string(PREFERENCE_KEY, "corrupt preferences".into());
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(app.doc.display, doc.display);
    assert!(app.inspector);
    assert_eq!(app.inspector_width, 300.);

    let mut preferences = InspectorPreferences::default();
    let mut display = DisplaySettings::default();
    display.fog_end = display.fog_start;
    preferences.display = Some(display);
    preferences.width = -20.;
    eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(app.doc.display, doc.display);
    assert_eq!(app.inspector_width, 260.);

    let older: InspectorPreferences =
        serde_json::from_str(r#"{"display":{"axes":false}}"#).unwrap();
    assert!(older.visible);
    assert!(older.sections.is_empty());
    assert!(older.camera.is_none());
    assert!(older.last_seen_release_notes.is_none());
    let display = older.display.unwrap();
    assert!(!display.axes);
    assert_eq!(display.sky, DisplaySettings::default().sky);
    assert!(display.valid());
}

#[test]
fn unit_choices_and_disclosure_save_immediately_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let model = app.doc.model.clone();
    let units = ketchup_core::UnitSettings {
        length: LengthUnit::Inches,
        area: AreaUnit::SquareFeet,
        volume: VolumeUnit::Liters,
        angle: AngleUnit::Radians,
        ..Default::default()
    };
    app.doc.display.units = units;
    set_section_open(&ctx, "units", false);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert_eq!(app.doc.model, model);

    let restored_ctx = egui::Context::default();
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(restored.doc.display.units, units);
    let id = egui::Id::unique("restored_units");
    restore_section(&restored_ctx, id, "units", true);
    assert!(
        !egui::collapsing_header::CollapsingState::load(&restored_ctx, id)
            .unwrap()
            .is_open()
    );
    set_section_open(&restored_ctx, "units", true);
    restored.persist_inspector_changes(&restored_ctx, Some(&mut storage), false);
    let reloaded_ctx = egui::Context::default();
    restored.restore_inspector_preferences(&reloaded_ctx, Some(&storage));
    restore_section(&reloaded_ctx, id, "units", false);
    assert!(
        egui::collapsing_header::CollapsingState::load(&reloaded_ctx, id)
            .unwrap()
            .is_open()
    );

    let older: InspectorPreferences =
        serde_json::from_str(r#"{"display":{"axes":false}}"#).unwrap();
    assert_eq!(
        older.display.unwrap().units,
        ketchup_core::UnitSettings::default()
    );
}

#[test]
fn precision_preferences_save_without_a_model_edit_and_restore_each_control() {
    use ketchup_core::UnitPrecision;

    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let model = app.doc.model.clone();
    let mut storage = SavedPreferences::default();
    for precision in [
        UnitPrecision {
            length: Some(6),
            ..Default::default()
        },
        UnitPrecision {
            area: Some(1),
            ..Default::default()
        },
        UnitPrecision {
            volume: Some(0),
            ..Default::default()
        },
        UnitPrecision {
            angle: Some(5),
            ..Default::default()
        },
    ] {
        app.doc.display.units.precision = precision;
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert_eq!(app.doc.model, model);

        let restored_ctx = egui::Context::default();
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(restored.doc.display.units.precision, precision);
        assert!(!restored.project.modified);
    }
}

#[test]
fn view_preferences_restore_groups_without_copying_another_models_framing() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.doc.camera.projection = Projection::Parallel;
    app.doc.camera.fov = 70_f64.to_radians();
    app.doc
        .camera
        .set_standard_view(ketchup_core::StandardView::Top);
    app.doc.camera.target = DVec3::splat(500.);
    app.doc.camera.distance = 300.;
    for (key, open) in [
        ("view", false),
        ("view_camera", true),
        ("view_standard_views", true),
    ] {
        set_section_open(&ctx, key, open);
    }
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    let mut doc = Document::default();
    doc.camera.target = DVec3::new(1., 2., 3.);
    doc.camera.distance = 42.;
    let refreshed_ctx = egui::Context::default();
    let mut refreshed = KetchupApp::with_document(doc.clone(), None, true);
    refreshed.restore_inspector_preferences(&refreshed_ctx, Some(&storage));
    assert_eq!(refreshed.doc.camera.target, doc.camera.target);
    assert_eq!(refreshed.doc.camera.distance, doc.camera.distance);
    assert_eq!(
        CameraPreferences::capture(&refreshed.doc.camera),
        CameraPreferences::capture(&app.doc.camera)
    );
    let _ = refreshed_ctx.run_test_ui(egui::RawInput::default(), |ui| {
        ui.push_id("floating_view", |ui| {
            assert_eq!(restored_header_open(ui, "view", true), Some(false));
            assert_eq!(restored_header_open(ui, "view_camera", false), Some(true));
            assert_eq!(
                restored_header_open(ui, "view_standard_views", false),
                Some(true)
            );
        });
    });
    let mut preferences = refreshed.current_inspector_preferences(&refreshed_ctx);
    preferences.camera.as_mut().unwrap().fov = f64::NAN;
    // Exercise validation directly: JSON/RON need not preserve non-finite values.
    let before = refreshed.doc.camera.clone();
    preferences
        .camera
        .unwrap()
        .restore(&mut refreshed.doc.camera);
    assert_eq!(refreshed.doc.camera, before);
}

#[test]
fn new_projects_keep_preferences_and_opened_projects_use_their_own_display() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.doc.display.axes = false;
    app.doc.display.fog = true;
    app.component_filter = "Cabinet".into();
    app.doc.camera.projection = Projection::Parallel;
    app.doc.camera.fov = 60_f64.to_radians();
    app.doc
        .camera
        .set_standard_view(ketchup_core::StandardView::Top);
    app.doc.camera.target = DVec3::splat(100.);
    app.doc.camera.distance = 200.;
    let expected_camera = Camera {
        projection: app.doc.camera.projection,
        fov: app.doc.camera.fov,
        ..Camera::default()
    };
    app.new_model(&ctx);
    assert!(!app.doc.display.axes);
    assert!(app.doc.display.fog);
    assert_eq!(app.component_filter, "Cabinet");
    assert_eq!(app.doc.camera, expected_camera);
    assert!(!app.project.modified);
    app.replace_project(Document::default(), Some("opened.ketchup".into()), &ctx);
    assert_eq!(app.doc.display, DisplaySettings::default());
    assert_eq!(app.doc.camera, Camera::default());
    assert_eq!(app.component_filter, "Cabinet");
}
