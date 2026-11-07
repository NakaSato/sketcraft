use super::*;

fn click_graphics(h: &mut crate::scenario::Harness, query: &str, handle: &str) {
    h.app.prefs_window.filter = query.into();
    h.frames(5);
    reveal_preference(h, handle);
    h.step(&json!({"click":{"handle":handle}})).unwrap();
    h.frames(2);
}

#[test]
fn graphics_cards_follow_manual_changes_and_preserve_non_preset_settings_in_both_themes() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"theme":theme,"language":"en","msaa":2,"dark_parts_shading":true,"frame_rate_details":true}));
        h.call("ui.prefs", json!({"open":"Graphics"}));
        let camera = h.app.cam;
        let design = h.app.session.doc.to_json();
        click_graphics(&mut h, "Quality", "view:graphics_preset:quality");
        assert_eq!(graphics_preset(&h.app.preferences), "quality");
        assert_eq!(
            (h.app.preferences.display_quality.as_str(), h.app.preferences.supersampling, h.app.preferences.supersampling_adaptive),
            ("fine", 2.0, true)
        );
        assert!(h.app.preferences.ambient_occlusion);
        assert_eq!(h.app.preferences.msaa, 2);
        assert!(h.app.preferences.dark_parts_shading && h.app.preferences.frame_rate_details);
        let quality = h.app.preferences.clone();
        click_graphics(&mut h, "Custom", "view:graphics_preset:custom");
        assert_eq!(h.app.preferences, quality, "Custom represents the mixture and sets nothing");
        click_graphics(&mut h, "Ultra", "prefs:display_quality:ultra");
        assert_eq!(h.app.preferences.display_quality, "ultra");
        assert_eq!(graphics_preset(&h.app.preferences), "custom");
        click_graphics(&mut h, "Performance", "view:graphics_preset:performance");
        assert_eq!(graphics_preset(&h.app.preferences), "performance");
        assert_eq!(
            (h.app.preferences.ambient_occlusion, h.app.preferences.supersampling, h.app.preferences.display_quality.as_str()),
            (false, 1.0, "normal")
        );
        h.call("ui.prefs", json!({"supersampling_adaptive":false}));
        assert_eq!(graphics_preset(&h.app.preferences), "performance", "Performance ignores the stored adaptive flag");
        assert_eq!(h.app.preferences.msaa, 2);
        assert!(h.app.preferences.dark_parts_shading && h.app.preferences.frame_rate_details);
        assert_eq!(h.app.cam, camera);
        assert_eq!(h.app.session.doc.to_json(), design);
        assert!(h.app.session.undo.is_empty());
    }
}

#[test]
fn graphics_tiles_and_status_chips_edit_real_values_and_cancel_in_both_themes() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"theme":theme,"language":"en"}));
        h.call("ui.prefs", json!({"open":"Graphics"}));
        let original = h.app.preferences.clone();
        let session = (h.app.ui.show_grid, h.app.ui.ground_shadow, h.app.ui.perspective);
        for (query, handle) in [("Grid", "prefs:grid"), ("Ground shadow", "prefs:ground_shadow"), ("Perspective", "prefs:perspective")] {
            click_graphics(&mut h, query, handle);
        }
        assert_eq!((h.app.ui.show_grid, h.app.ui.ground_shadow, h.app.ui.perspective), (!session.0, !session.1, !session.2));
        for (query, field) in [
            ("Ambient occlusion", "ambient_occlusion"),
            ("View angles", "show_view_angles"),
            ("Vertex markers", "vertex_markers"),
            ("Quick measurement", "quick_measure"),
            ("Frame rate", "show_frame_rate"),
            ("Analysis", "viewport_analysis_status"),
            ("Section", "viewport_section_status"),
            ("Live details", "viewport_status_details"),
            ("Fold items", "viewport_status_summary"),
        ] {
            let before = serde_json::to_value(&h.app.preferences).unwrap();
            click_graphics(&mut h, query, &format!("prefs:{field}"));
            let after = serde_json::to_value(&h.app.preferences).unwrap();
            let mut expected = before.clone();
            expected[field] = json!(!before[field].as_bool().unwrap());
            assert_eq!(after, expected, "{theme}: {field} changes precisely its own setting");
        }
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences, original);
        assert_eq!((h.app.ui.show_grid, h.app.ui.ground_shadow, h.app.ui.perspective), session);
    }
}

#[test]
fn graphics_overlay_cards_apply_persist_and_reset_without_adding_unpaged_fields() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"language":"en"}));
    h.call("ui.prefs", json!({"open":"Graphics"}));
    for (query, handle) in [
        ("View angles", "prefs:show_view_angles"),
        ("Vertex markers", "prefs:vertex_markers"),
        ("Quick measurement", "prefs:quick_measure"),
        ("Frame rate", "prefs:show_frame_rate"),
        ("Frame rate details", "prefs:frame_rate_details"),
        ("Analysis", "prefs:viewport_analysis_status"),
        ("Section", "prefs:viewport_section_status"),
        ("Live details", "prefs:viewport_status_details"),
        ("Fold items", "prefs:viewport_status_summary"),
    ] {
        click_graphics(&mut h, query, handle);
    }
    h.call("ui.prefs", json!({"button":"apply"}));
    let applied = h.app.preferences.clone();
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.show_view_angles && restored.preferences.vertex_markers && restored.preferences.quick_measure);
    assert!(restored.preferences.show_frame_rate && restored.preferences.frame_rate_details);
    assert!(!restored.preferences.viewport_analysis_status && !restored.preferences.viewport_section_status);
    assert!(!restored.preferences.viewport_status_details && !restored.preferences.viewport_status_summary);
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(!h.app.preferences.show_view_angles && !h.app.preferences.quick_measure);
    assert!(!h.app.preferences.show_frame_rate && !h.app.preferences.frame_rate_details);
    assert!(h.app.preferences.viewport_analysis_status && h.app.preferences.viewport_section_status);
    assert!(h.app.preferences.viewport_status_details && h.app.preferences.viewport_status_summary);
    assert!(h.app.preferences.vertex_markers, "the visible unpaged control remains excluded from page defaults");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences, applied, "Cancel after Apply restores the applied overlay state");
}

#[test]
fn graphics_redesign_keeps_newer_main_controls_and_their_transactions() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.prefs", json!({"theme":theme,"language":"en","open":"Graphics"}));
        let original = h.app.preferences.clone();
        for (query, handle) in [
            ("Tint new surface in a preview", "prefs:preview_tint_new_faces"),
            ("Show removed material in red", "prefs:cut_preview_tint"),
            ("Profile operations", "prefs:profile_operations"),
        ] {
            click_graphics(&mut h, query, handle);
        }
        h.app.prefs_window.filter = "Section hatch".into();
        h.frames(5);
        reveal_preference(&mut h, "dialog:Section hatch:One style");
        h.step(&json!({"click":{"handle":"dialog:Section hatch:One style"}})).unwrap();
        h.frames(2);
        assert!(!h.app.preferences.preview_tint_new_faces && !h.app.preferences.cut_preview_tint);
        assert!(h.app.preferences.profile_operations);
        assert_eq!(h.app.preferences.section_hatch, SectionHatch::One);
        h.call("ui.prefs", json!({"button":"apply"}));
        let applied = h.app.preferences.clone();
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert!(h.app.preferences.preview_tint_new_faces && h.app.preferences.cut_preview_tint);
        assert!(!h.app.preferences.profile_operations);
        assert_eq!(h.app.preferences.section_hatch, SectionHatch::ByPart);
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences, applied);
        h.call("ui.prefs", json!({"profile_operations":false}));
        assert_eq!(h.app.preferences.persistent_ribbon_pressed, original.persistent_ribbon_pressed);
    }
}

#[test]
fn graphics_style_dropdown_uses_the_real_indices_and_icon_masters() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"theme":theme,"language":"en"}));
        h.call("ui.prefs", json!({"open":"Graphics"}));
        for (index, (_, label)) in crate::navigation_bar::VISUAL_STYLES.into_iter().enumerate() {
            click_graphics(&mut h, "Visual style", "prefs:visual_style");
            h.step(&json!({"click":{"handle":format!("dialog:Visual style:{label}")}})).unwrap();
            h.frames(3);
            assert_eq!(h.app.ui.visual_style, index as u8);
            let icon = crate::navigation_bar::visual_style_icon(index as u8);
            let painted = crate::scenario::rect_of(&format!("dialog:dropdown_icon:{icon}")).unwrap();
            assert!(crate::scenario::rect_of("prefs:visual_style").unwrap().contains_rect(painted));
        }
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.ui.visual_style, 0);
    }
}

#[test]
fn graphics_style_popup_owns_escape_and_leaves_snapshot_cancel_to_the_next_escape() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1100}));
        h.call("ui.prefs", json!({"language":"en","theme":theme}));
        let before = h.app.preferences.clone();
        h.call("ui.prefs", json!({"open":"Graphics","dark_parts_shading":true}));
        let edited = h.app.preferences.clone();
        assert_ne!(before, edited);
        reveal_preference(&mut h, "prefs:visual_style");
        h.step(&json!({"click":{"handle":"prefs:visual_style"}})).unwrap();
        h.frames(2);
        let popup = "dialog:Visual style:Wireframe";
        assert!(crate::scenario::handle_at(popup).is_some());
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(h.app.prefs_window.open && h.app.prefs_window.section == PAGE_GRAPHICS);
        assert!(h.app.prefs_window.snapshot.is_some());
        assert_eq!(h.app.preferences, edited, "first Escape retains unapplied Graphics edits");
        assert!(crate::scenario::handle_at(popup).is_none());
        h.step(&json!({"key":"Enter"})).unwrap();
        h.frames(2);
        assert!(h.app.prefs_window.open && crate::scenario::handle_at(popup).is_some(), "Enter reopens the focused style trigger");
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(h.app.prefs_window.open && h.app.prefs_window.snapshot.is_some());
        assert!(crate::scenario::handle_at(popup).is_none());
        assert_eq!(h.app.preferences, edited);
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(!h.app.prefs_window.open && h.app.prefs_window.snapshot.is_none());
        assert_eq!(h.app.preferences, before, "Escape without a popup performs normal Preferences Cancel");
    }
}

#[test]
fn graphics_disabled_controls_keep_stored_values_and_msaa_waits_for_restart() {
    use egui::accesskit::Role;
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        let dir = std::env::temp_dir().join(format!("sketcraft-graphics-cards-{}-{theme}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("graphics.json");
        std::fs::write(&path, r#"{"backend":"vulkan","msaa":4}"#).unwrap();
        h.app.services.graphics_path = Some(path.to_string_lossy().into_owned());
        h.app.msaa_supported = vec![1, 2, 4];
        let active_samples = h.app.viewport.gpu.as_ref().map(|gpu| gpu.samples);
        h.call("ui.prefs", json!({"theme":theme,"language":"en","frame_rate_details":true}));
        h.call("ui.prefs", json!({"open":"Graphics"}));
        click_graphics(&mut h, "Anti-aliasing", "prefs:msaa:8");
        assert_eq!(h.app.preferences.msaa, 4, "an unavailable card cannot coerce MSAA");
        assert!(h.widget("Restart needed").is_some());
        let tree = h.output.accesskit_update.as_ref().unwrap();
        assert!(tree.nodes.iter().any(|(_, node)| node.role() == Role::RadioButton && node.label() == Some("8×") && node.is_disabled()));
        click_graphics(&mut h, "Anti-aliasing", "prefs:msaa:2");
        assert_eq!(h.app.preferences.msaa, 2);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["msaa"], 2);
        assert_eq!(saved["backend"], "vulkan");
        assert_eq!(h.app.viewport.gpu.as_ref().map(|gpu| gpu.samples), active_samples, "changing the saved level leaves the running renderer alone");
        click_graphics(&mut h, "Only when the view is still", "prefs:supersampling_adaptive");
        assert!(h.app.preferences.supersampling_adaptive, "disabled adaptive control retains its saved value");
        click_graphics(&mut h, "Frame rate details", "prefs:frame_rate_details");
        assert!(h.app.preferences.frame_rate_details, "visible disabled details control retains its saved value");
        let tree = h.output.accesskit_update.as_ref().unwrap();
        assert!(tree.nodes.iter().any(|(_, node)| node.role() == Role::CheckBox && node.label() == Some("Frame rate details") && node.is_disabled()));
        click_graphics(&mut h, "Frame rate", "prefs:show_frame_rate");
        click_graphics(&mut h, "Frame rate details", "prefs:frame_rate_details");
        assert!(!h.app.preferences.frame_rate_details, "enabling Frame rate enables its details checkbox");
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences.msaa, 4);
        assert!(h.app.preferences.frame_rate_details);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["msaa"], 4, "Cancel restores startup persistence too");
        assert_eq!(saved["backend"], "vulkan");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn graphics_page_keeps_the_untouched_radian_default_and_unpaged_reset_exclusions() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"theme":theme,"language":"en","vertex_markers":true,"grid_follows_view":true}));
        h.call("ui.prefs", json!({"open":"Graphics"}));
        for (perspective, auto_ortho) in [(false, false), (true, false), (false, true)] {
            h.app.ui.perspective = perspective;
            h.app.ui.auto_ortho_faces = auto_ortho;
            h.app.prefs_window.filter = "Field of view".into();
            h.frames(5);
            assert_eq!(h.app.preferences.perspective_fov, 0.6, "drawing a whole-degree readout never rounds the default");
        }
        for page in 0..PAGES.len() {
            defaults(&mut h.app, page);
            assert!(h.app.preferences.vertex_markers && h.app.preferences.grid_follows_view, "{page}: neither unpaged field joins resets");
        }
        assert_eq!(h.app.preferences.perspective_fov, 0.6);
    }
}

#[test]
fn viewport_preset_popup_accepts_quality_and_performance() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.inspect", json!({}));
    for handle in ["nav:view", "view:graphics_preset", "view:graphics_preset:quality"] {
        let pos = crate::scenario::handle_at(handle).unwrap();
        h.call("ui.click", json!({"x":pos.x,"y":pos.y}));
    }
    assert!(h.app.preferences.ambient_occlusion);
    for handle in ["view:graphics_preset", "view:graphics_preset:performance"] {
        let pos = crate::scenario::handle_at(handle).unwrap();
        h.call("ui.click", json!({"x":pos.x,"y":pos.y}));
    }
    assert!(!h.app.preferences.ambient_occlusion);
}

#[test]
fn quality_selector_enables_ao_and_supports_persistence_and_cancel() {
    let mut h = crate::scenario::Harness::new();
    let camera = h.app.cam;
    let design = h.app.session.doc.to_json();
    let msaa = h.app.preferences.msaa;
    h.call("ui.prefs", json!({"open":"Graphics"}));
    for handle in ["view:graphics_preset", "view:graphics_preset:quality"] {
        let pos = crate::scenario::handle_at(handle).unwrap();
        h.call("ui.click", json!({"x":pos.x,"y":pos.y}));
    }
    assert!(h.app.preferences.ambient_occlusion);
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.ambient_occlusion, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(graphics_preset(&h.app.preferences), "performance");
    assert!(!h.app.preferences.ambient_occlusion);
    h.call("engine.execute", json!({"command":"view.graphics_preset","params":{"preset":"quality"}}));
    assert!(h.app.preferences.ambient_occlusion);
    h.call("engine.execute", json!({"command":"view.graphics_preset","params":{"preset":"invalid"}}));
    assert!(h.app.preferences.ambient_occlusion);
    h.call("engine.execute", json!({"command":"view.graphics_preset","params":{"preset":"performance"}}));
    assert!(!h.app.preferences.ambient_occlusion);
    assert_eq!(graphics_preset(&h.app.preferences), "performance");
    assert_eq!(h.app.preferences.display_quality, "normal");
    assert_eq!(h.app.preferences.supersampling, 1.0);
    h.call("engine.execute", json!({"command":"view.graphics_preset","params":{"preset":"quality"}}));
    assert_eq!(graphics_preset(&h.app.preferences), "quality");
    assert_eq!(h.app.preferences.display_quality, "fine");
    assert_eq!(h.app.preferences.supersampling, 2.0);
    assert!(h.app.preferences.supersampling_adaptive);
    // The dialog is closed now: what is saved is the live state, quality included (#9730).
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    for edit in [
        json!({"display_quality":"ultra"}),
        json!({"supersampling":1.5}),
        json!({"supersampling_adaptive":false}),
        json!({"ambient_occlusion":false}),
    ] {
        h.call("engine.execute", json!({"command":"view.graphics_preset","params":{"preset":"quality"}}));
        h.call("ui.prefs", edit);
        assert_eq!(graphics_preset(&h.app.preferences), "custom");
    }
    assert_eq!(graphics_preset(&restored.preferences), "quality");
    h.call("ui.prefs", json!({"open":"Graphics"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert_eq!(graphics_preset(&h.app.preferences), "performance");
    h.call("ui.prefs", json!({"button":"ok"}));
    assert_eq!(h.app.preferences.msaa, msaa);
    assert_eq!(h.app.cam, camera);
    assert_eq!(h.app.session.doc.to_json(), design);
    assert!(h.app.session.undo.is_empty());
}
