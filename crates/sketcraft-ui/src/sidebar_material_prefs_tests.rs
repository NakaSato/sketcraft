//! Materials, finishes, currency, joinery, fasteners and time estimates persist.
use super::*;

#[test]
fn joinery_preferences_save_without_model_edit_and_restore_legacy_defaults() {
    let defaults: InspectorPreferences=serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.joinery,joinery::Preferences::default());
    let display:DisplaySettings=serde_json::from_str("{}").unwrap();
    assert!(display.joinery_preview);
    let ctx=egui::Context::default();
    let mut storage=SavedPreferences::default();
    let mut app=SketCraftApp::with_document(Document::default(),None,true);
    app.joinery.settings.kind=sketcraft_core::joinery::JointKind::Dovetail;
    app.joinery.settings.depth_axis=2;
    app.joinery.settings.width_axis=0;
    app.joinery.settings.width_ratio=0.6;
    app.joinery.settings.height_ratio=0.7;
    app.joinery.settings.fingers=6;
    app.joinery.settings.dovetails=3;
    app.joinery.settings.dovetail_width=Some(0.02);
    app.joinery.settings.clearance=0.0002;
    app.joinery.settings.taper_degrees=12.;
    app.joinery.open=true;
    app.joinery.reverse=true;
    app.doc.display.joinery_preview=false;
    app.persist_inspector_changes(&ctx,Some(&mut storage),false);
    let mut restored=SketCraftApp::with_document(Document::default(),None,true);
    restored.restore_inspector_preferences(&egui::Context::default(),Some(&storage));
    assert_eq!(restored.joinery,app.joinery);
    assert!(!restored.doc.display.joinery_preview);
    assert!(restored.joinery_dialog.members.is_none());
    assert!(!restored.project.modified);
}

#[test]
fn fastener_preferences_save_immediately_without_model_edit() {
    let defaults: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.fasteners, fasteners::Preferences::default());
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.fasteners.settings.pattern = sketcraft_core::connections::Pattern::Grid;
    app.fasteners.settings.spacing = 0.05;
    app.fasteners.open = true;
    app.fasteners.reverse = true;
    app.doc.display.fastener_face_handles = true;
    app.doc.display.fasteners = false;
    set_section_open(&ctx, "connections/fasteners", true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let restored_ctx = egui::Context::default();
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(restored.fasteners, app.fasteners);
    assert!(restored.doc.display.fastener_face_handles);
    assert!(!restored.doc.display.fasteners);
    assert!(!restored.project.modified);
    let saved: InspectorPreferences = eframe::get_value(&storage, PREFERENCE_KEY).unwrap();
    assert_eq!(saved.sections.get("connections/fasteners"), Some(&true));
}

#[test]
fn currency_changes_save_immediately_and_apply_after_switching_projects() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.general.stock.currency, currency::DEFAULT);

    for code in ["CAD", "EUR", "JPY"] {
        app.general_settings.stock.currency = code.into();
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());

        let restored_ctx = egui::Context::default();
        let mut restored = SketCraftApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(restored.general_settings.stock.currency, code);
        restored.replace_project(Document::default(), None, &restored_ctx);
        assert_eq!(restored.general_settings.stock.currency, code);
        assert!(!restored.project.modified);
    }
}

#[test]
fn currency_search_saves_immediately_and_restores_without_model_edits() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.general.stock.currency_filter.is_empty());

    for query in ["Canadian", "JPY", ""] {
        app.general_settings.stock.currency_filter = query.into();
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());

        let restored_ctx = egui::Context::default();
        let mut restored = SketCraftApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(restored.general_settings.stock.currency_filter, query);
        assert_eq!(restored.general_settings.stock.currency, currency::DEFAULT);
        restored.replace_project(Document::default(), None, &restored_ctx);
        assert_eq!(restored.general_settings.stock.currency_filter, query);
        assert!(!restored.project.modified);
    }
}

#[test]
fn wood_types_and_searches_save_immediately_without_model_edits() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let model = app.doc.model.clone();
    let mut storage = SavedPreferences::default();
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.wood_type_filters.is_empty());
    let stock = stock_settings::Stock::default();
    let id = stock.id;
    let key = format!("settings/stock/wood/{id}/type");
    app.general_settings.stock.wood.push(stock);
    for (kind, query) in [
        (stock_settings::WoodType::Plywood, "ply"),
        (stock_settings::WoodType::Mdf, "MDF"),
        (stock_settings::WoodType::Unspecified, ""),
    ] {
        app.general_settings.stock.wood[0].wood_type = kind;
        app.wood_type_filters.insert(key.clone(), query.into());
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert_eq!(app.doc.model, model);
        assert!(!app.project.modified);
        let restored_ctx = egui::Context::default();
        let mut restored = SketCraftApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(restored.general_settings.stock.wood[0].wood_type, kind);
        assert_eq!(restored.wood_type_filters[&key], query);
        restored.replace_project(Document::default(), None, &restored_ctx);
        assert_eq!(restored.general_settings.stock.wood[0].wood_type, kind);
        assert_eq!(restored.wood_type_filters[&key], query);
        assert!(restored.doc.model.stock_materials.is_empty());
    }
}

#[test]
fn stock_material_search_saves_immediately_without_model_edits() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let model = app.doc.model.clone();
    let mut storage = SavedPreferences::default();
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.stock_material_filter.is_empty());

    for query in ["Oak 19", "project material", ""] {
        app.stock_material_filter = query.into();
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert_eq!(app.doc.model, model);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());

        let restored_ctx = egui::Context::default();
        let mut restored = SketCraftApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(restored.stock_material_filter, query);
        restored.replace_project(Document::default(), None, &restored_ctx);
        assert_eq!(restored.stock_material_filter, query);
        assert!(restored.doc.model.stock_materials.is_empty());
        assert!(!restored.project.modified);
    }
}

#[test]
fn legacy_currency_upgrade_is_saved_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut old = InspectorPreferences::default();
    old.general.stock.currency = "$".into();
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &old);
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(app.general_settings.stock.currency, "USD");
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.general_settings.stock.currency, "USD");
}

#[test]
fn finishing_products_and_disclosure_save_immediately_without_model_edits() {
    use finish_settings::{ContainerUnit, Finish, FinishType};

    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.general_settings.section = settings::Section::StockPricing;
    app.general_settings.stock.finishes = vec![
        Finish {
            name: "Pre-stain conditioner".into(),
            finish_type: FinishType::PreStain,
            container_size: 946.,
            container_unit: ContainerUnit::Milliliters,
            price: 18.75,
            ..Default::default()
        },
        Finish {
            name: "Clear varnish".into(),
            finish_type: FinishType::Varnish,
            container_size: 1.,
            container_unit: ContainerUnit::UsQuarts,
            price: 25.50,
            ..Default::default()
        },
    ];
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);

    // Editing existing products and closing the section are preferences,
    // so neither operation needs a document edit to be written to storage.
    app.general_settings.stock.finishes[1].finish_type = FinishType::Paint;
    app.general_settings.stock.finishes[1].name = "White paint".into();
    app.general_settings.stock.finishes[1].price = 30.;
    app.general_settings.stock.finishes[1].coverage_m2_per_container = Some(12.5);
    set_section_open(&ctx, "settings/stock/finishes", false);
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());

    let restored_ctx = egui::Context::default();
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(restored.general_settings, app.general_settings);
    let _ = restored_ctx.run_test_ui(Default::default(), |ui| {
        assert_eq!(
            restored_header_open(ui, "settings/stock/finishes", true),
            Some(false)
        );
    });

    // Catalog edits and section expansion also survive a different project.
    restored.replace_project(Document::default(), None, &restored_ctx);
    assert_eq!(
        restored.general_settings.stock.finishes,
        app.general_settings.stock.finishes
    );
    assert!(restored.doc.model.stock_materials.is_empty());
    restored.general_settings.stock.finishes.remove(0);
    set_section_open(&restored_ctx, "settings/stock/finishes", true);
    storage.flushed = false;
    restored.persist_inspector_changes(&restored_ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!restored.project.modified);
    let final_ctx = egui::Context::default();
    app.restore_inspector_preferences(&final_ctx, Some(&storage));
    assert_eq!(
        app.general_settings.stock.finishes,
        restored.general_settings.stock.finishes
    );
    let _ = final_ctx.run_test_ui(Default::default(), |ui| {
        assert_eq!(
            restored_header_open(ui, "settings/stock/finishes", false),
            Some(true)
        );
    });
}

#[test]
fn material_project_usage_persists_across_restarts() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let stock = stock_settings::Stock::default();
    let id = stock.id;
    app.general_settings.stock.wood.push(stock);
    let edge = app.doc.model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let object = app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Board")
        .unwrap();
    app.doc
        .model
        .objects
        .get_mut(&object)
        .unwrap()
        .stock_material_id = Some(id);
    app.doc.name = "Desk".into();
    app.project.path = Some("desk.ketchup".into());
    app.sync_stock_materials(&ctx);
    let project_id = app.doc.project_id;
    app.project.path = Some("desk-copy.ketchup".into());
    app.sync_stock_materials(&ctx);
    assert_eq!(
        app.general_settings.stock.projects.len(),
        2,
        "Save As retains the original file's last-known usage"
    );
    assert_eq!(app.doc.project_id, project_id);
    app.replace_project(Document::default(), None, &ctx);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(
        restored.general_settings.stock.projects,
        app.general_settings.stock.projects
    );
    assert!(
        restored
            .general_settings
            .stock
            .projects
            .values()
            .all(|usage| usage.materials.get(&id) == Some(&1))
    );
}

#[test]
fn old_catalog_ids_are_saved_immediately_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut old = InspectorPreferences::default();
    old.general.stock.wood.push(stock_settings::Stock {
        id: sketcraft_core::MaterialId::nil(),
        ..Default::default()
    });
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &old);
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    let id = app.general_settings.stock.wood[0].id;
    assert!(!id.is_nil());
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert_eq!(restored.general_settings.stock.wood[0].id, id);
}

#[test]
fn removing_stock_persists_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    app.general_settings
        .stock
        .wood
        .push(stock_settings::Stock::default());
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    app.general_settings.stock.wood.clear();
    storage.flushed = false;
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
    assert!(restored.general_settings.stock.wood.is_empty());
}

#[test]
fn time_estimate_controls_save_immediately_and_restore_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let model = app.doc.model.clone();
    app.general_settings.section = settings::Section::SurfaceTreatment;
    app.general_settings.stock.finishes =
        (0..3).map(|_| finish_settings::Finish::default()).collect();
    for (i, step) in app.time_estimates.sanding.iter_mut().enumerate() {
        step.enabled = i % 2 == 0;
        step.minutes_per_m2 = 7. + i as f64;
    }
    app.time_estimates.sanding_after_varnish = vec![false, true, false, true, false, true];
    for (step, product) in [
        &mut app.time_estimates.pre_stain,
        &mut app.time_estimates.stain,
        &mut app.time_estimates.varnish,
    ]
    .into_iter()
    .zip(&app.general_settings.stock.finishes)
    {
        step.enabled = true;
        step.product_id = Some(product.id);
        step.minutes_per_m2 = 12.5;
        step.coats = 2;
        step.drying_hours = 3.25;
        step.final_drying_hours = Some(12.5);
    }
    set_section_open(&ctx, "time_cost_estimates", false);
    let subsections = [
        ("settings/treatment/sanding", false),
        ("entity/treatment/sanding", true),
        ("settings/treatment/finishing", true),
        ("entity/treatment/finishing", false),
        ("time_estimates_pre_stain", false),
        ("time_estimates_stain", true),
        ("time_estimates_varnish", false),
        ("time_estimates_material_cost", true),
    ];
    for (key, open) in subsections {
        set_section_open(&ctx, key, open);
    }
    app.inspector_order.rotate_right(1);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert_eq!(app.doc.model, model);
    let restored_ctx = egui::Context::default();
    let mut restored = SketCraftApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(restored.time_estimates, app.time_estimates);
    assert_eq!(
        restored.general_settings.stock.finishes,
        app.general_settings.stock.finishes
    );
    assert_eq!(
        restored.general_settings.section,
        settings::Section::SurfaceTreatment
    );
    assert_eq!(restored.inspector_order, app.inspector_order);
    assert!(restored.selected.is_empty());
    assert!(!restored.project.modified);
    let _ = restored_ctx.run_test_ui(egui::RawInput::default(), |ui| {
        ui.push_id("floating_estimator", |ui| {
            assert_eq!(
                restored_header_open(ui, "time_cost_estimates", true),
                Some(false)
            );
            for (key, open) in subsections {
                assert_eq!(restored_header_open(ui, key, false), Some(open), "{key}");
            }
        });
    });
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.time_estimates, time_estimates::Settings::default());
}

#[test]
fn finishing_product_searches_save_immediately_without_model_edits() {
    let ctx = egui::Context::default();
    let mut app = SketCraftApp::with_document(Document::default(), None, true);
    let model = app.doc.model.clone();
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.finish_product_filters.is_empty());
    for key in [
        "settings/treatment/pre_stain",
        "settings/treatment/stain",
        "settings/treatment/varnish",
        "entity/treatment/pre_stain",
        "entity/treatment/stain",
        "entity/treatment/varnish",
    ] {
        app.finish_product_filters
            .insert(key.into(), format!("Search for {key}"));
    }
    let mut storage = SavedPreferences::default();
    for query in ["oak stain", ""] {
        app.finish_product_filters
            .insert("entity/treatment/stain".into(), query.into());
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert_eq!(app.doc.model, model);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let restored_ctx = egui::Context::default();
        let mut restored = SketCraftApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        assert_eq!(restored.finish_product_filters, app.finish_product_filters);
        restored.replace_project(Document::default(), None, &restored_ctx);
        assert_eq!(restored.finish_product_filters, app.finish_product_filters);
        assert!(!restored.project.modified);
    }
}
