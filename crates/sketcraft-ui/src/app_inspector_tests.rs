//! Inspector sidebar, materials and finishing editors.
use super::*;

#[test]
fn sidebar_sections_collapse_and_display_controls_change_saved_settings() {
    let mut h = Harness::new();
    // Make all sections accessible in the viewport without scrolling.
    h.size.y = 2600.;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    for title in ["Components", "Tags", "Display"] {
        text_position(&output, title);
    }
    h.click(text_position(&output, "Properties"));
    for _ in 0..30 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "View"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(menu_text_position(&output, "Axes"));
    h.key(Key::Escape);
    assert!(!h.app.doc.display.axes);
    assert!(h.app.project.modified);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let display_y = text_position(&output, "Display").y;
    h.click(text_position(&output, "Components"));
    for _ in 0..30 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    // The next section moves up when Components is collapsed.
    assert!(text_position(&output, "Display").y < display_y - 30.);
    h.click(text_position(&output, "Components"));
    for _ in 0..30 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "No components yet");
    let saved = Document::from_json(&serde_json::to_string(&h.app.doc).unwrap()).unwrap();
    assert!(!saved.display.axes);
}

#[test]
fn sidebar_headers_dock_and_rejoin_in_memory_and_keep_their_disclosure_state() {
    use sidebar::InspectorSection;

    let mut h = Harness::new();
    h.ctx.style_mut_of(h.ctx.theme(), |s| s.animation_time = 0.);
    h.size.y = 3000.;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Components"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let from = text_position(&output, "Components");
    let to = text_position(&output, "Outliner") - egui::vec2(0., 12.);
    h.pointer(from, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(to)], egui::Modifiers::NONE);
    h.pointer(to, egui::PointerButton::Primary, false, false);
    assert_eq!(h.app.inspector_order[0], InspectorSection::Components);
    assert!(!h.app.project.modified);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    assert!(text_position(&output, "Components").y < text_position(&output, "Outliner").y);
    // A section alone in its area opens automatically and cannot collapse.
    h.click(text_position(&output, "Components"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "No components yet");

    let from = text_position(&output, "Components");
    let mut to = text_position(&output, "Display") + egui::vec2(0., 8.);
    to.x = h.size.x - h.app.inspector_width * 0.5;
    h.pointer(from, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(to)], egui::Modifiers::NONE);
    h.pointer(to, egui::PointerButton::Primary, false, false);
    assert_eq!(
        h.app.inspector_order,
        [
            InspectorSection::Outliner,
            InspectorSection::EntityInfo,
            InspectorSection::Units,
            InspectorSection::Tags,
            InspectorSection::Components,
            InspectorSection::Display,
            InspectorSection::View,
            InspectorSection::TimeCostEstimates,
            InspectorSection::Materials,
            InspectorSection::History,
            InspectorSection::ModelLibrary,
        ]
    );
    let order = h.app.inspector_order;
    h.app.inspector = false;
    h.frame(vec![], egui::Modifiers::NONE);
    h.app.inspector = true;
    h.app.new_model(&h.ctx);
    h.frame(vec![], egui::Modifiers::NONE);
    assert_eq!(h.app.inspector_order, order);
    let fresh = KetchupApp::with_document(Document::default(), None, true);
    assert_eq!(fresh.inspector_order, InspectorSection::ALL);
}

#[test]
fn sidebar_drag_moves_the_entire_panel_and_snaps_its_neighbors_and_drop() {
    let mut h = Harness::new();
    h.size.y = 2000.;
    h.ctx
        .style_mut_of(h.ctx.theme(), |s| s.animation_time = 0.2);
    for _ in 0..30 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let from = text_position(&output, "Components");
    let body_offset = text_position(&output, "No components yet") - from;
    let neighbor_start = text_position(&output, "Tags").y;
    let mut to = text_position(&output, "Display") + egui::vec2(0., 8.);
    to.x = h.size.x - h.app.inspector_width * 0.5;
    let viewport = h.app.viewport;
    let order = h.app.inspector_order;
    h.pointer(from, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(to)], egui::Modifiers::NONE);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    assert!(text_position(&output, "Components").distance(to) < 1.);
    assert!((text_position(&output, "No components yet") - to - body_offset).length() < 1.);
    let neighbor_first = text_position(&output, "Tags").y;
    for _ in 0..8 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let neighbor_middle = text_position(&output, "Tags").y;
    for _ in 0..25 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let neighbor_end = text_position(&output, "Tags").y;
    assert!(neighbor_first < neighbor_start - 1.);
    assert!((neighbor_middle - neighbor_first).abs() < 0.01);
    assert!((neighbor_end - neighbor_first).abs() < 0.01);
    assert!(text_position(&output, "Components").distance(to) < 1.);
    assert_eq!(h.app.inspector_order, order);
    assert_eq!(h.app.viewport, viewport);
    h.pointer(to, egui::PointerButton::Primary, false, false);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let dropping = text_position(&output, "Components");
    for _ in 0..30 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let dropped = text_position(&output, "Components");
    assert!(dropping.distance(dropped) < 0.01);
    assert!(dropped.y > text_position(&output, "Tags").y);
    assert!((dropped.x - from.x).abs() < 1.);
    assert!(!h.app.project.modified);
}

#[test]
fn sidebar_header_drag_cancels_on_escape_or_release_outside_the_inspector() {
    let mut h = Harness::new();
    h.size.y = 2000.;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let from = text_position(&output, "Components");
    let to = text_position(&output, "Outliner") - egui::vec2(0., 50.);
    let order = h.app.inspector_order;
    h.pointer(from, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerMoved(to)], egui::Modifiers::NONE);
    h.key(Key::Escape);
    h.pointer(to, egui::PointerButton::Primary, false, false);
    assert_eq!(h.app.inspector_order, order);

    for _ in 0..30 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let from = text_position(&output, "Components");
    let outside = h.app.viewport.center();
    h.pointer(from, egui::PointerButton::Primary, true, false);
    h.frame(
        vec![egui::Event::PointerMoved(outside)],
        egui::Modifiers::NONE,
    );
    assert!(egui::DragAndDrop::has_payload_of_type::<
        sidebar::InspectorSection,
    >(&h.ctx));
    h.pointer(outside, egui::PointerButton::Primary, false, false);
    assert_eq!(h.app.inspector_order, order);
}

#[test]
fn expanding_edge_style_keeps_the_inspector_width_stable() {
    let mut h = Harness::new();
    h.ctx.style_mut_of(h.ctx.theme(), |s| s.animation_time = 0.);
    h.size.y = 2600.;
    for _ in 0..3 {
        h.frame(vec![], egui::Modifiers::NONE);
    }
    let right = h.app.viewport.right();
    for _ in 0..2 {
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, "Edge style"));
        for _ in 0..5 {
            h.frame(vec![], egui::Modifiers::NONE);
            assert!((h.app.viewport.right() - right).abs() < 0.1);
        }
    }
}

#[test]
fn materials_section_assigns_catalog_material_as_an_undoable_document_edit() {
    let mut h = Harness::new();
    h.size.y = 4000.;
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let object = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Board")
        .unwrap();
    h.app.selected = BTreeSet::from([object]);
    h.app
        .general_settings
        .stock
        .wood
        .push(stock_settings::Stock {
            id: ketchup_core::MaterialId::new_v4(),
            name: "Test oak".into(),
            thickness_mm: 19.,
            width_mm: 89.,
            length_mm: 2440.,
            price: 10.,
            wood_type: Default::default(),
        });
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Properties"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    assert!(text_position(&output, "Materials").y < text_position(&output, "Stock material").y);
    for label in ["Sanding", "Finishing", "Revert to defaults"] {
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == label)
        }));
    }
    h.click(text_position(&output, "Unassigned"));
    h.frame(vec![], egui::Modifiers::NONE);
    let before_search = h.app.doc.model.clone();
    h.frame(
        vec![egui::Event::Text("TeSt oak".into())],
        egui::Modifiers::NONE,
    );
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "Test oak (19 × 89 × 2440 mm)");
    assert_eq!(h.app.doc.model, before_search);
    assert_eq!(h.app.stock_material_filter, "TeSt oak");
    h.key(Key::Enter);
    let material_id = h.app.general_settings.stock.wood[0].id;
    let assigned = ketchup_core::StockMaterial {
        name: "Test oak".into(),
        dimensions: [190, 890, 24400],
    };
    assert_eq!(
        h.app.doc.model.objects[&object].stock_material_id,
        Some(material_id)
    );
    assert!(h.app.doc.model.objects[&object].stock_material.is_none());
    assert_eq!(h.app.doc.model.stock_materials[&material_id], assigned);
    assert!(h.app.project.modified);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    assert!(
        h.app.selected.contains(&object),
        "Selection after assigning material: {:?}",
        h.app.selected
    );
    assert!(text_position(&output, "Stock material").y < text_position(&output, "Sanding").y);
    assert!(text_position(&output, "Sanding").y < text_position(&output, "Finishing").y);
    text_position(&output, "Revert to defaults");
    h.click(text_position(&output, "Test oak"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Unassigned"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    for label in ["Sanding", "Finishing", "Revert to defaults"] {
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == label)
        }));
    }
    // History navigation clears the selection; reselect the restored object.
    h.app.undo(&h.ctx, false);
    h.app.selected = BTreeSet::from([object]);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "Sanding");
    text_position(&output, "Finishing");
    h.app.general_settings.stock.wood[0].name = "Renamed oak".into();
    h.app.general_settings.stock.wood[0].width_mm = 140.;
    h.app.sync_stock_materials(&h.ctx);
    assert_eq!(
        h.app.doc.model.objects[&object].stock_material_id,
        Some(material_id)
    );
    assert_eq!(
        h.app.doc.model.stock_materials[&material_id].name,
        "Renamed oak"
    );
    assert_eq!(
        h.app.doc.model.stock_materials[&material_id].dimensions[1],
        1400
    );
    h.app.general_settings.stock.wood.clear();
    h.app.sync_stock_materials(&h.ctx);
    assert_eq!(
        h.app.doc.model.stock_materials[&material_id].name,
        "Renamed oak"
    );
    h.app.undo(&h.ctx, false);
    assert!(h.app.doc.model.objects[&object].stock_material_id.is_none());
    h.app.undo(&h.ctx, true);
    assert_eq!(
        h.app.doc.model.objects[&object].stock_material_id,
        Some(material_id)
    );
    assert_eq!(
        h.app.doc.model.stock_materials[&material_id].name,
        "Renamed oak"
    );
    assert_eq!(
        h.app.doc.model.stock_materials[&material_id].dimensions[1],
        1400
    );
}

#[test]
fn shared_finishing_editor_selects_products_for_defaults_and_objects() {
    for defaults in [true, false] {
        let mut h = Harness::new();
        h.size.y = 4000.;
        let product = finish_settings::Finish {
            name: "Golden oak".into(),
            ..Default::default()
        };
        let product_id = product.id;
        h.app.general_settings.stock.finishes = vec![
            product,
            finish_settings::Finish {
                name: "Clear topcoat".into(),
                finish_type: finish_settings::FinishType::Varnish,
                ..Default::default()
            },
        ];
        h.app.time_estimates.stain.enabled = true;
        let object = if defaults {
            h.app.general_settings.section = settings::Section::SurfaceTreatment;
            h.app.settings_open = true;
            None
        } else {
            let stock = stock_settings::Stock::default();
            let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
            let id = h
                .app
                .doc
                .model
                .make_object(&BTreeSet::from([edge]), false, "Board")
                .unwrap();
            h.app
                .doc
                .model
                .stock_materials
                .insert(stock.id, stock.definition());
            h.app
                .doc
                .model
                .objects
                .get_mut(&id)
                .unwrap()
                .stock_material_id = Some(stock.id);
            h.app.general_settings.stock.wood.push(stock);
            h.app.selected = BTreeSet::from([id]);
            let output = h.frame(vec![], egui::Modifiers::NONE);
            h.click(text_position(&output, "Properties"));
            Some(id)
        };
        // Let the newly opened settings modal complete its sizing pass.
        h.frame(vec![], egui::Modifiers::NONE);
        let before = h.app.doc.model.clone();
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, "Finishing"));
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, "Stain"));
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, "No product"));
        h.frame(vec![], egui::Modifiers::NONE);
        h.frame(
            vec![egui::Event::Text("GoLdEn STAIN".into())],
            egui::Modifiers::NONE,
        );
        let output = h.frame(vec![], egui::Modifiers::NONE);
        text_position(&output, "Golden oak");
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Clear topcoat")
        }));
        assert_eq!(
            h.app.doc.model, before,
            "Searching must not edit the document"
        );
        assert_eq!(h.app.time_estimates.stain.product_id, None);
        let filter_key = if defaults {
            "settings/treatment/stain"
        } else {
            "entity/treatment/stain"
        };
        assert_eq!(h.app.finish_product_filters[filter_key], "GoLdEn STAIN");
        h.key(Key::Enter);

        if let Some(id) = object {
            assert_eq!(
                h.app.doc.model.objects[&id]
                    .surface_treatment
                    .stain
                    .product_id,
                Some(product_id)
            );
            assert!(h.app.doc.model.objects[&id].surface_treatment_customized);
            assert_eq!(h.app.time_estimates.stain.product_id, None);
            assert!(h.app.project.modified);
            let restored =
                Document::from_json(&serde_json::to_string(&h.app.doc).unwrap()).unwrap();
            assert_eq!(
                restored.model.objects[&id]
                    .surface_treatment
                    .stain
                    .product_id,
                Some(product_id)
            );
            h.app.undo(&h.ctx, false);
            assert_eq!(h.app.doc.model, before);
            h.app.undo(&h.ctx, true);
            assert_eq!(
                h.app.doc.model.objects[&id]
                    .surface_treatment
                    .stain
                    .product_id,
                Some(product_id)
            );
            h.app.replace_project(Document::default(), None, &h.ctx);
            assert!(h.app.doc.model.objects.is_empty());
            assert_eq!(h.app.time_estimates.stain.product_id, None);
            h.app.replace_project(restored, None, &h.ctx);
            h.app.selected = BTreeSet::from([id]);
        } else {
            assert_eq!(h.app.time_estimates.stain.product_id, Some(product_id));
            assert_eq!(h.app.doc.model, before);
            assert!(!h.app.project.modified);
        }

        // The selection follows the catalog ID through renames and removals.
        h.app.general_settings.stock.finishes[0].name = "Renamed stain".into();
        let output = h.frame(vec![], egui::Modifiers::NONE);
        text_position(&output, "Renamed stain");
        h.app.general_settings.stock.finishes.clear();
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, "Unavailable product"));
        let output = h.frame(vec![], egui::Modifiers::NONE);
        text_position(&output, "No products match your search.");
        h.click(text_position(&output, "Clear"));
        assert!(h.app.finish_product_filters[filter_key].is_empty());
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, "No product"));
        if let Some(id) = object {
            assert_eq!(
                h.app.doc.model.objects[&id]
                    .surface_treatment
                    .stain
                    .product_id,
                None
            );
        } else {
            assert_eq!(h.app.time_estimates.stain.product_id, None);
            assert!(!h.app.project.modified);
        }
    }
}

#[test]
fn material_removal_warns_about_other_projects_and_requires_confirmation() {
    let mut h = Harness::new();
    let stock = stock_settings::Stock::default();
    let id = stock.id;
    h.app.general_settings.stock.wood.push(stock.clone());
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let object = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Board")
        .unwrap();
    h.app
        .doc
        .model
        .objects
        .get_mut(&object)
        .unwrap()
        .stock_material_id = Some(id);
    h.app.doc.name = "Cabinet".into();
    h.app.sync_stock_materials(&h.ctx);
    let cabinet = h.app.doc.clone();
    h.app.replace_project(Document::default(), None, &h.ctx);
    h.app.settings_open = true;
    h.app.general_settings.section = settings::Section::StockPricing;
    h.app.material_removal = Some(id);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "Cabinet · 1 assigned object");
    h.click(text_position(&output, "Cancel"));
    assert!(h.app.material_removal.is_none());
    assert!(h.app.settings_open);
    assert_eq!(h.app.general_settings.stock.wood.len(), 1);
    h.app.material_removal = Some(id);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Remove material"));
    assert!(h.app.material_removal.is_none());
    assert!(h.app.general_settings.stock.wood.is_empty());
    h.app.replace_project(cabinet, None, &h.ctx);
    assert_eq!(h.app.doc.model.objects[&object].stock_material_id, Some(id));
    assert_eq!(h.app.doc.model.stock_materials[&id], stock.definition());
}

#[test]
fn materials_revert_button_clears_only_selected_treatment_overrides_and_is_undoable() {
    let mut h = Harness::new();
    h.size.y = 4000.;
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let object = h
        .app
        .doc
        .model
        .make_object(&BTreeSet::from([edge]), false, "Board")
        .unwrap();
    let stock = stock_settings::Stock::default();
    h.app
        .doc
        .model
        .stock_materials
        .insert(stock.id, stock.definition());
    h.app.general_settings.stock.wood.push(stock.clone());
    let board = h.app.doc.model.objects.get_mut(&object).unwrap();
    board.stock_material_id = Some(stock.id);
    board.surface_treatment_customized = true;
    board.surface_treatment.stain.enabled = true;
    h.app.selected = BTreeSet::from([object]);
    h.frame(vec![], egui::Modifiers::NONE); // Register the project's material usage first.
    let defaults = h.app.time_estimates.clone();
    let catalog = h.app.general_settings.stock.clone();
    let customized = h.app.doc.model.clone();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Properties"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    assert!(text_position(&output, "Revert to defaults").y > text_position(&output, "Finishing").y);
    assert!(
        text_position(&output, "Apply to all children").y
            < text_position(&output, "Revert to defaults").y
    );
    h.click(text_position(&output, "Revert to defaults"));
    assert!(
        h.app.doc.model.objects[&object]
            .surface_treatment_override()
            .is_none()
    );
    assert_eq!(
        h.app.doc.model.objects[&object].stock_material_id,
        Some(stock.id)
    );
    assert_eq!(h.app.time_estimates, defaults);
    assert_eq!(h.app.general_settings.stock, catalog);
    assert!(h.app.project.modified);
    // The absence of an override survives saving and follows future Settings defaults.
    let saved = serde_json::to_string(&h.app.doc).unwrap();
    let restored: Document = serde_json::from_str(&saved).unwrap();
    assert!(
        restored.model.objects[&object]
            .surface_treatment_override()
            .is_none()
    );
    h.app.undo(&h.ctx, false);
    assert_eq!(h.app.doc.model, customized);
}
