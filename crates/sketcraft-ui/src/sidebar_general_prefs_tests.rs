//! General, account, sharing, export and other app preferences persist.
use super::*;

#[test]
fn recent_projects_save_immediately_without_model_edit() {
    let defaults: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(defaults.recent_projects.is_empty());
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.project.path = Some("recent.ketchup".into());
    app.remember_recent_project(None);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.project.recent, app.project.recent);
    assert!(!restored.project.modified);
}

#[test]
fn sharing_preferences_restore_without_model_edits_and_default_for_old_settings() {
    let defaults: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(!defaults.sharing.open);
    assert!(defaults.sharing.invitation.is_empty());
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.sharing.preferences.open = true;
    app.sharing.preferences.invitation = "a".repeat(48);
    set_section_open(&ctx, "sharing_notifications", true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let restored_ctx = egui::Context::default();
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(restored.sharing.preferences, app.sharing.preferences);
    let saved: InspectorPreferences = eframe::get_value(&storage, PREFERENCE_KEY).unwrap();
    assert_eq!(saved.sections.get("sharing_notifications"), Some(&true));
    assert!(!restored.project.modified);
}

#[test]
fn account_preferences_restore_without_a_model_edit() {
    let defaults: InspectorPreferences = serde_json::from_str(r#"{"account_server":"https://old.example"}"#).unwrap();
    assert!(!defaults.account_open);
    assert!(!defaults.account_menu_open);
    assert!(!serde_json::to_string(&defaults).unwrap().contains("account_server"));
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.account.open = true;
    app.account.menu_open = true;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&ctx, Some(&storage));
    assert!(!storage.get_string(PREFERENCE_KEY).unwrap().contains("account_server"));
    assert!(restored.account.open);
    assert!(restored.account.menu_open);
    assert!(!restored.project.modified);
}

#[test]
fn modal_sizes_survive_reload_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    remember_modal_size(&ctx, "general_settings", egui::vec2(820., 560.));
    remember_modal_size(&ctx, "another_dialog", egui::vec2(480., 320.));
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
    let restored_ctx = egui::Context::default();
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(
        modal_size(&restored_ctx, "general_settings"),
        Some(egui::vec2(820., 560.))
    );
    assert_eq!(
        modal_size(&restored_ctx, "another_dialog"),
        Some(egui::vec2(480., 320.))
    );
    assert_eq!(modal_size(&restored_ctx, "unknown"), None);
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.modal_sizes.is_empty());
    let legacy: InspectorPreferences = serde_json::from_str(
        r#"{"modal_positions":{"general_settings":[120.0,80.0]}}"#,
    )
    .unwrap();
    assert!(
        serde_json::to_value(legacy)
            .unwrap()
            .get("modal_positions")
            .is_none()
    );
}

#[test]
fn exchange_choices_survive_reload_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    app.project.import_format = "dxf".into();
    app.project.export_stl = true;
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.project.import_format, "dxf");
    assert!(restored.project.export_stl);
    for format in ["3mf", "glb", "gltf", "any"] {
        app.project.import_format = format.into();
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.project.import_format, format);
    }
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.import_format, "skp");
    assert!(!old.export_stl);
    let mut invalid = old;
    invalid.import_format = "unsupported".into();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &invalid);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.project.import_format, "skp");
}

#[test]
fn legacy_ron_sanding_arrays_keep_the_rest_of_the_preferences() {
    #[derive(serde::Serialize)]
    struct LegacyTreatment {
        sanding: [time_estimates::Step; 6],
        sanding_after_varnish: [bool; 6],
    }
    #[derive(serde::Serialize)]
    struct LegacyPreferences {
        general: settings::GeneralSettings,
        time_estimates: LegacyTreatment,
    }
    let mut general = settings::GeneralSettings::default();
    general.stock.currency = "MXN".into();
    let old = LegacyPreferences {
        general,
        time_estimates: LegacyTreatment {
            sanding: std::array::from_fn(|_| time_estimates::Step::default()),
            sanding_after_varnish: [false; 6],
        },
    };
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &old);
    let restored = eframe::get_value::<InspectorPreferences>(&storage, PREFERENCE_KEY).unwrap();
    assert_eq!(restored.general.stock.currency, "MXN");
    assert_eq!(restored.time_estimates.sanding.len(), 6);
    assert_eq!(
        restored.time_estimates.sanding_after_varnish,
        vec![false; 6]
    );
}

#[test]
fn older_and_invalid_export_preferences_keep_safe_defaults() {
    let ctx = egui::Context::default();
    for json in [
        "{}",
        r#"{"cutting_plan":{"layout":{"kerf_mm":5.0}}}"#,
        r#"{"cutting_plan":{"group_stock":false,"layout":{"kerf_mm":4.5}}}"#,
    ] {
        let preferences: InspectorPreferences = serde_json::from_str(json).unwrap();
        assert!(preferences.snapping.is_none());
        assert!(preferences.shortcut_filter.is_empty());
        assert!(preferences.cutting_plan.selection.is_none());
        assert!(preferences.cutting_plan.layout.validate().is_ok());
        let mut storage = SavedPreferences::default();
        eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
        let mut doc = Document::default();
        doc.snap_to_geometry = false;
        doc.snap_to_axes = false;
        let mut restored = SketCraftApp::with_document(doc, None, true);
        restored.restore_inspector_preferences(&ctx, Some(&storage));
        assert!(!restored.doc.snap_to_geometry);
        assert!(!restored.doc.snap_to_axes);
        assert_eq!(
            restored.project.cutting_plan_layout,
            preferences.cutting_plan.layout
        );
    }
    let mut preferences = InspectorPreferences::default();
    preferences.cutting_plan.layout.sheet_width_mm = -1.;
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(
        restored.project.cutting_plan_layout,
        crate::cutting_layout::Settings::default()
    );
}

#[test]
fn screenshot_options_persist_immediately_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let settings = crate::screenshots::Settings {
        views: [true, false, false, true, true, false, false, false],
        combined: false,
        paper: crate::screenshots::Paper::Tabloid,
        landscape: false,
        format: crate::screenshots::Format::Pdf,
        dpi: 300,
    };
    app.screenshot.settings = settings.clone();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(!app.project.modified);
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.screenshot.settings, settings);
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.screenshot, crate::screenshots::Settings::default());
}

#[test]
fn outliner_search_persists_immediately_and_defaults_to_empty() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.outliner_filter = "Handle".into();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);

    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.outliner_filter, "Handle");
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.outliner_filter.is_empty());

    restored.outliner_filter.clear();
    restored.persist_inspector_changes(&ctx, Some(&mut storage), false);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert!(app.outliner_filter.is_empty());
}

#[test]
fn general_settings_save_immediately_and_restore_without_model_edits() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.general_settings.section = settings::Section::StockPricing;
    app.general_settings.orbit_speed = 2.;
    app.general_settings.pan_speed = 0.5;
    app.general_settings.zoom_speed = 3.;
    app.general_settings.reverse_zoom = true;
    app.general_settings.default_units.length = LengthUnit::Inches;
    app.general_settings.tool_keys.swap(1, 2);
    app.general_settings.stock.currency = "CAD".into();
    app.general_settings.stock.units = LengthUnit::Inches;
    app.general_settings.stock.wood.push(stock_settings::Stock {
        id: sketcraft_core::MaterialId::new_v4(),
        name: "Pine board".into(),
        thickness_mm: 19.,
        width_mm: 89.,
        length_mm: 2438.4,
        price: 8.25,
        wood_type: Default::default(),
    });
    set_section_open(&ctx, "settings/stock/wood", false);
    app.doc.display.grid_auto_size = false;
    app.doc.display.grid_spacing = 0.0254;
    app.doc.display.grid_infinite = false;
    app.doc.display.grid_max_length = 30.48;
    app.doc.display.axis_colors = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    app.doc.display.inference_color = [10, 11, 12];
    app.doc.display.midpoint_color = [13, 14, 15];
    app.doc.display.endpoint_color = [16, 17, 18];
    app.doc.display.on_edge_color = [19, 20, 21];
    app.doc.display.on_face_color = [22, 23, 24];
    let expected = app.current_inspector_preferences(&ctx);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
    let restored_ctx = egui::Context::default();
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(
        restored.current_inspector_preferences(&restored_ctx),
        expected
    );
    restored.new_model(&restored_ctx);
    assert_eq!(restored.doc.display.units.length, LengthUnit::Inches);
    let mut opened = Document::default();
    opened.display.units.length = LengthUnit::Millimeters;
    restored.replace_project(opened, None, &restored_ctx);
    assert_eq!(restored.doc.display.units.length, LengthUnit::Millimeters);
    assert_eq!(
        restored.general_settings.default_units.length,
        LengthUnit::Inches
    );
}

#[test]
fn older_general_preferences_keep_defaults() {
    let preferences: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(preferences.general, settings::GeneralSettings::default());
}

#[test]
fn viewed_release_notes_save_immediately_and_restore_for_other_projects() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    // Establish saved preferences first, so only the read state changes.
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    storage.flushed = false;
    assert!(app.release_notes.has_unread());
    app.release_notes.open_if_unread();
    assert!(app.release_notes.open);
    let _ = ctx.run_test_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 700.))),
            ..Default::default()
        },
        |_| app.release_notes.show(&ctx),
    );
    assert!(!app.release_notes.has_unread());
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());

    let mut other_doc = Document::default();
    other_doc.name = "Another project".into();
    let mut refreshed = SketCraftApp::with_document(other_doc, None, true);
    let refreshed_ctx = egui::Context::default();
    refreshed.restore_inspector_preferences(&refreshed_ctx, Some(&storage));
    refreshed.release_notes.open_if_unread();
    assert!(!refreshed.release_notes.has_unread());
    assert!(!refreshed.release_notes.open);
    assert_eq!(refreshed.doc.name, "Another project");
    storage.flushed = false;
    refreshed.persist_inspector_changes(&refreshed_ctx, Some(&mut storage), false);
    assert!(!storage.flushed);

    // An older saved read version must allow the next release to open.
    let mut older = refreshed.current_inspector_preferences(&refreshed_ctx);
    older.last_seen_release_notes = Some("0.0.0".into());
    eframe::set_value(&mut storage, PREFERENCE_KEY, &older);
    let mut updated = SketCraftApp::with_document(Document::default(), None, true);
    updated.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    updated.release_notes.open_if_unread();
    assert!(updated.release_notes.open);
    assert!(updated.release_notes.has_unread());
}

#[test]
fn model_library_folder_search_and_format_save_immediately_and_rescan_after_restore() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("truck.glb"), b"glTF").unwrap();
    std::fs::write(folder.path().join("canopy.skp"), b"skp").unwrap();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.model_library.prefs = super::super::model_library::Preferences {
        folder: folder.path().to_string_lossy().into_owned(),
        search: "truck".into(),
        format: "glb".into(),
    };
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);

    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.model_library.prefs, app.model_library.prefs);
    assert!(restored.model_library.scanned.is_none(), "the restored folder rescans when shown");
    restored.model_library.rescan();
    let matches = restored.model_library.matches();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].relative, "truck.glb");

    let older: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(older.model_library, Default::default());
}
