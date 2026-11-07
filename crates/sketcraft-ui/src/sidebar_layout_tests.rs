//! Section order, disclosures, scrolling and sticky headers.
use super::*;

#[test]
fn retired_fastener_section_is_removed_without_resetting_saved_order() {
    let mut storage=SavedPreferences::default();
    let mut old=InspectorSection::ALL.to_vec();old.insert(2,InspectorSection::Fasteners);
    eframe::set_value(&mut storage,ORDER_PREFERENCE_KEY,&old);
    assert_eq!(restore_order(Some(&storage)),InspectorSection::ALL);
}

#[test]
fn connection_sidebar_sections_restore_without_model_edits() {
    for key in ["connections/wood_joinery", "connections/fasteners"] {
        for open in [false, true] {
            let ctx = egui::Context::default();
            let mut storage = SavedPreferences::default();
            let mut app = KetchupApp::with_document(Document::default(), None, true);
            set_section_open(&ctx, key, open);
            let _ = ctx.run_test_ui(egui::RawInput::default(), |ui| {
                if key == "connections/wood_joinery" {
                    app.wood_joinery_sidebar(ui);
                } else {
                    app.automatic_fasteners_sidebar(ui);
                }
            });
            app.persist_inspector_changes(&ctx, Some(&mut storage), false);
            let restored_ctx = egui::Context::default();
            let mut restored = KetchupApp::with_document(Document::default(), None, true);
            restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
            let _ = restored_ctx.run_test_ui(egui::RawInput::default(), |ui| {
                assert_eq!(restored_header_open(ui, key, true), Some(open));
            });
            assert!(!restored.project.modified);
        }
    }
}

#[test]
fn history_appends_to_old_layout_and_preferences_save_without_model_edit() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let previous: Vec<_> = InspectorSection::ALL.into_iter()
        .filter(|section| !matches!(section, InspectorSection::History | InspectorSection::Fasteners)).collect();
    eframe::set_value(&mut storage, ORDER_PREFERENCE_KEY, &previous);
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(&app.inspector_order[..previous.len()], previous.as_slice());
    assert_eq!(app.inspector_order[previous.len()], InspectorSection::History);
    assert_eq!(app.inspector_order.last(), Some(&InspectorSection::History));
    let _ = ctx.run_test_ui(egui::RawInput::default(), |ui| {
        assert_eq!(restored_header_open(ui, "history", true), Some(true));
    });
    for open in [false, true] {
        set_section_open(&ctx, "history", open);
        remember_scroll(&ctx, "history_entries", egui::vec2(0., 125.));
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(!app.project.modified);
        let restored_ctx = egui::Context::default();
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
        let _ = restored_ctx.run_test_ui(egui::RawInput::default(), |ui| {
            ui.push_id("floating_history", |ui| {
                assert_eq!(restored_header_open(ui, "history", true), Some(open));
            });
        });
        assert_eq!(restored.current_inspector_preferences(&restored_ctx)
            .scroll_positions["history_entries"], [0., 125.]);
        assert!(!restored.project.modified);
    }
}

#[test]
fn estimate_header_menu_does_not_toggle_the_disclosure() {
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> (Rect, egui::Id, bool) {
        let mut result = None;
        let _ = ctx.run_test_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_width(400.);
                let viewport = ui.clip_rect();
                let panel = inspector_section_sized(
                    ui,
                    "Time & Cost Estimates",
                    "time_cost_estimates",
                    true,
                    None,
                    Some(viewport),
                    |ui| {
                        ui.text_label("Estimate contents");
                    },
                );
                let header = panel.inner;
                let rect = Rect::from_center_size(
                    egui::pos2(header.rect.right() - 22., header.rect.center().y),
                    egui::vec2(32., 32.),
                );
                let mut action = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("test_estimate_menu")
                        .layer_id(header.layer_id)
                        .max_rect(rect),
                );
                let clicked = egui::menu::MenuButton::from_button(
                    egui::Button::new(icons::ellipsis_atom(Palette::for_ui(&action).muted))
                        .min_size(rect.size()),
                )
                .ui(&mut action, |ui| {
                    let _ = ui.button("PDF");
                })
                .0
                .clicked();
                result = Some((rect, header.id, clicked));
            },
        );
        result.unwrap()
    }
    let ctx = egui::Context::default();
    set_section_open(&ctx, "time_cost_estimates", true);
    let (rect, id, _) = frame(&ctx, vec![]);
    let pos = rect.center();
    frame(
        &ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let (_, _, clicked) = frame(
        &ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(clicked);
    assert!(
        egui::collapsing_header::CollapsingState::load(&ctx, id)
            .unwrap()
            .is_open()
    );
}

#[test]
fn outliner_search_fills_section_width_and_abuts_header() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut header = Rect::NOTHING;
    let _ = ctx.run_test_ui(egui::RawInput::default(), |ui| {
        ui.set_width(400.);
        let viewport = ui.clip_rect();
        let panel = inspector_section_sized(
            ui,
            "Outliner",
            "outliner",
            true,
            None,
            Some(viewport),
            |ui| app.outliner_contents(ui),
        );
        header = panel.inner.rect;
    });
    let search = ctx
        .read_response(egui::Id::unique(OUTLINER_SEARCH_ID))
        .unwrap()
        .rect;
    assert!((search.left() - header.left()).abs() <= 1.);
    assert!((search.right() - header.right()).abs() <= 1.);
    assert!((search.top() - header.bottom()).abs() <= 1.);
}

#[test]
fn sticky_headers_follow_their_own_area_and_keep_disclosures_persistent() {
    #[derive(Clone, Copy)]
    struct SectionFrame {
        viewport: Rect,
        section: Rect,
        header: Rect,
    }
    fn render(
        ctx: &egui::Context,
        offsets: [f32; 2],
        events: Vec<egui::Event>,
    ) -> [SectionFrame; 2] {
        let mut frames = [SectionFrame {
            viewport: Rect::NOTHING,
            section: Rect::NOTHING,
            header: Rect::NOTHING,
        }; 2];
        let _ = ctx.run_test_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(700., 400.))),
                events,
                ..Default::default()
            },
            |ui| {
                ui.columns(2, |columns| {
                    for (index, column) in columns.iter_mut().enumerate() {
                        egui::ScrollArea::vertical()
                            .id_salt(("sticky_test", index))
                            .max_height(200.)
                            .auto_shrink([false, false])
                            .vertical_scroll_offset(offsets[index])
                            .show(column, |ui| {
                                let viewport = ui.clip_rect();
                                let id = ["sticky_left", "sticky_right"][index];
                                let section = inspector_section_sized(
                                    ui,
                                    "First",
                                    id,
                                    true,
                                    None,
                                    Some(viewport),
                                    |ui| {
                                        ui.allocate_space(egui::vec2(180., 500.));
                                    },
                                );
                                frames[index] = SectionFrame {
                                    viewport,
                                    section: section.response.rect,
                                    header: section.inner.rect,
                                };
                                inspector_section_sized(
                                    ui,
                                    "Next",
                                    ["sticky_left_next", "sticky_right_next"][index],
                                    true,
                                    None,
                                    Some(viewport),
                                    |ui| {
                                        ui.allocate_space(egui::vec2(180., 300.));
                                    },
                                );
                            });
                    }
                });
            },
        );
        frames
    }

    let ctx = egui::Context::default();
    ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
    render(&ctx, [0., 0.], vec![]);
    let initial = render(&ctx, [0., 0.], vec![]);
    let scrolled = render(&ctx, [100., 0.], vec![]);
    assert_eq!(scrolled[0].header.top(), scrolled[0].viewport.top());
    assert!(scrolled[0].section.top() < scrolled[0].viewport.top());
    assert_eq!(scrolled[1].header, initial[1].header);
    let offset = initial[0].section.bottom()
        - initial[0].viewport.top()
        - initial[0].header.height() * 0.5;
    let end = render(&ctx, [offset, 0.], vec![]);
    assert!(end[0].header.top() < end[0].viewport.top());
    assert!(end[0].header.bottom() <= end[0].section.bottom());

    // Clicking the visible pinned header must update the shared disclosure
    // preference immediately, without any model edit.
    render(&ctx, [100., 0.], vec![]);
    let pinned = render(&ctx, [100., 0.], vec![])[0].header.center();
    for pressed in [true, false] {
        render(
            &ctx,
            [100., 0.],
            vec![
                egui::Event::PointerMoved(pinned),
                egui::Event::PointerButton {
                    pos: pinned,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.inspector_preferences.sections["sticky_left"]);
    assert!(app.inspector_preferences.sections["sticky_right"]);
    let restored_ctx = egui::Context::default();
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert!(!restored.inspector_preferences.sections["sticky_left"]);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
}

#[test]
fn sidebar_and_outliner_scroll_positions_save_immediately_and_restore_independently() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    let positions = [
        ("sidebar_left", egui::vec2(0., 110.)),
        ("sidebar_right", egui::vec2(0., 260.)),
        ("outliner_tree", egui::vec2(70., 0.)),
        ("outliner_geometry_rows/None", egui::vec2(0., 180.)),
        ("outliner_geometry_rows/Some(42)", egui::vec2(0., 340.)),
        (currency::RESULTS_SCROLL_KEY, egui::vec2(0., 120.)),
    ];
    let content = Vec2::splat(1500.);
    for (key, position) in positions {
        assert_eq!(scroll_frame(&ctx, key, key, position, content), position);
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed, "{key}");
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
    }

    let restored_ctx = egui::Context::default();
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    for (key, position) in positions {
        // Each area has a new egui scope, just as after an app restart or
        // when moving the inspector between docked and floating layouts.
        assert_eq!(
            scroll_frame(&restored_ctx, key, "floating", Vec2::ZERO, content),
            position
        );
        assert_eq!(
            scroll_frame(&restored_ctx, key, "docked", Vec2::ZERO, content),
            position
        );
    }
    storage.flushed = false;
    restored.persist_inspector_changes(&restored_ctx, Some(&mut storage), false);
    assert!(
        !storage.flushed,
        "unchanged positions must not save every frame"
    );
}

#[test]
fn scroll_restoration_allows_further_scrolling_and_clamps_shorter_content() {
    let ctx = egui::Context::default();
    let content = Vec2::splat(1500.);
    let key = "sidebar_right";
    remember_scroll(&ctx, key, egui::vec2(0., 400.));
    assert_eq!(
        scroll_frame(&ctx, key, "docked", Vec2::ZERO, content).y,
        400.
    );
    // Restoration must not keep forcing the saved offset in an existing
    // scope: egui must remain free to process subsequent scrolling.
    assert_eq!(
        scroll_frame(&ctx, key, "docked", egui::vec2(0., 200.), content).y,
        200.
    );
    assert_eq!(
        scroll_frame(&ctx, key, "floating", Vec2::ZERO, content).y,
        200.
    );
    let shorter = scroll_frame(
        &ctx,
        key,
        "floating",
        egui::vec2(0., 200.),
        Vec2::splat(20.),
    );
    assert_eq!(shorter, Vec2::ZERO);
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    let restored_ctx = egui::Context::default();
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(
        scroll_frame(&restored_ctx, key, "docked", Vec2::ZERO, content),
        Vec2::ZERO
    );
}

#[test]
fn scroll_positions_default_for_older_preferences_and_reject_invalid_offsets() {
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.scroll_positions.is_empty());
    assert_eq!(valid_scroll_position([f32::NAN, f32::INFINITY]), [0., 0.]);
    assert_eq!(valid_scroll_position([-15., f32::NEG_INFINITY]), [0., 0.]);
    let ctx = egui::Context::default();
    let mut preferences = InspectorPreferences::default();
    preferences
        .scroll_positions
        .insert("sidebar_left".into(), [-10., 75.]);
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, PREFERENCE_KEY, &preferences);
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(
        restored
            .current_inspector_preferences(&ctx)
            .scroll_positions["sidebar_left"],
        [0., 75.]
    );
    assert_eq!(
        scroll_frame(&ctx, "sidebar_left", "left", Vec2::ZERO, Vec2::splat(1500.)),
        egui::vec2(0., 75.)
    );
}

#[test]
fn outliner_virtualized_geometry_lists_restore_their_own_scroll_positions() {
    let ctx = egui::Context::default();
    ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
    let mut doc = Document::default();
    let mut grouped = BTreeSet::new();
    for index in 0..100 {
        let point = DVec3::Y * index as f64 * 2.;
        let edge = doc.model.add_edge(point, point + DVec3::X).unwrap();
        if index < 50 {
            grouped.insert(edge);
        }
    }
    let group = doc.model.make_object(&grouped, false, "Group").unwrap();
    let child_key = format!("outliner_geometry_rows/{:?}", Some(group));
    set_section_open(&ctx, "outliner_geometry/None", true);
    set_section_open(&ctx, &format!("outliner_object/{group}"), true);
    set_section_open(&ctx, &format!("outliner_geometry/{:?}", Some(group)), true);
    remember_scroll(&ctx, "outliner_geometry_rows/None", egui::vec2(0., 120.));
    remember_scroll(&ctx, &child_key, egui::vec2(0., 240.));
    let mut app = KetchupApp::with_document(doc.clone(), None, true);
    let render = |ctx: &egui::Context, app: &mut KetchupApp| {
        let _ = ctx.run_test_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(300., 900.))),
                ..Default::default()
            },
            |ui| app.outliner_contents(ui),
        );
    };
    render(&ctx, &mut app);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    let restored_ctx = egui::Context::default();
    restored_ctx.style_mut_of(restored_ctx.theme(), |style| style.animation_time = 0.);
    let mut restored = KetchupApp::with_document(doc, None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    render(&restored_ctx, &mut restored);
    let positions = restored
        .current_inspector_preferences(&restored_ctx)
        .scroll_positions;
    assert_eq!(positions["outliner_geometry_rows/None"], [0., 120.]);
    assert_eq!(positions[&child_key], [0., 240.]);
    assert!(restored.dirty_since.is_none());
}

#[test]
fn lone_sections_fill_the_area_stay_open_and_preserve_the_saved_disclosure() {
    fn render(
        ctx: &egui::Context,
        size: Vec2,
        fill: bool,
        events: Vec<egui::Event>,
    ) -> (bool, Rect, Rect) {
        let mut body_visible = false;
        let mut panel = Rect::NOTHING;
        let mut header = Rect::NOTHING;
        let _ = ctx.run_test_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 900.))),
                events,
                ..Default::default()
            },
            |ui| {
                let mut section_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .scope_id(egui::Id::unique("lone_section_test"))
                        .max_rect(Rect::from_min_size(egui::pos2(24., 24.), size)),
                );
                let response = inspector_section_sized(
                    &mut section_ui,
                    "Components",
                    "components",
                    true,
                    fill.then_some(size),
                    Some(Rect::from_min_size(Pos2::ZERO, size + Vec2::splat(48.))),
                    |ui| {
                        body_visible = true;
                        ui.text_label("Contents");
                    },
                );
                panel = response.response.rect;
                header = response.inner.rect;
            },
        );
        (body_visible, panel, header)
    }
    let ctx = egui::Context::default();
    set_section_open(&ctx, "components", false);
    let size = egui::vec2(320., 400.);
    assert!(!render(&ctx, size, false, vec![]).0);
    let (body_visible, panel, header) = render(&ctx, size, true, vec![]);
    assert!(body_visible);
    assert!((panel.size() - size).length() < 0.5, "{panel:?}");
    for pressed in [true, false] {
        assert!(
            render(
                &ctx,
                size,
                true,
                vec![
                    egui::Event::PointerMoved(header.center()),
                    egui::Event::PointerButton {
                        pos: header.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            )
            .0
        );
    }
    let resized = egui::vec2(260., 600.);
    let (body_visible, panel, _) = render(&ctx, resized, true, vec![]);
    assert!(body_visible);
    assert!((panel.size() - resized).length() < 0.5, "{panel:?}");
    assert!(!render(&ctx, resized, false, vec![]).0);
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let restored_ctx = egui::Context::default();
    let mut restored = KetchupApp::with_document(Document::default(), None, true);
    restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert!(!render(&restored_ctx, resized, false, vec![]).0);
    assert!(!app.project.modified);
}

#[test]
fn saved_order_survives_recreating_the_app() {
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.inspector_order.rotate_left(2);
    let order = app.inspector_order;
    eframe::App::save(&mut app, &mut storage);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    drop(app);

    let mut refreshed = KetchupApp::with_document(Document::default(), None, true);
    refreshed.inspector_order = restore_order(Some(&storage));
    assert_eq!(refreshed.inspector_order, order);
}

#[test]
fn sidebar_tab_survives_immediate_reload_without_changing_group_emphasis() {
    let ctx = egui::Context::default();
    let mut storage = SavedPreferences::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    for enabled in [false, true] {
        let _ = ctx.run_test_ui(
            egui::RawInput {
                events: [true, false]
                    .map(|pressed| egui::Event::Key {
                        key: Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .to_vec(),
                ..Default::default()
            },
            |_| app.keyboard(&ctx),
        );
        assert_eq!(app.inspector, enabled);
        assert!(app.doc.display.group_emphasis);
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert!(app.dirty_since.is_none());
        let mut restored = KetchupApp::with_document(Document::default(), None, true);
        restored.restore_inspector_preferences(&egui::Context::default(), Some(&storage));
        assert_eq!(restored.inspector, enabled);
        assert!(restored.doc.display.group_emphasis);
    }
    let old: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(old.visible);
}

#[test]
fn find_shortcut_reveals_and_focuses_outliner_and_persists_expansion() {
    for width in [1440., 800.] {
        for modifiers in [
            egui::Modifiers::CTRL,
            egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
            egui::Modifiers {
                mac_cmd: true,
                command: true,
                ..Default::default()
            },
        ] {
            let ctx = egui::Context::default();
            ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
            let mut app = KetchupApp::with_document(Document::default(), None, true);
            app.outliner_filter = "Cabinet".into();
            set_section_open(&ctx, "outliner", false);
            let screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 900.)));
            let mut other_text = String::new();
            let _ = ctx.run_test_ui(
                egui::RawInput {
                    screen_rect,
                    ..Default::default()
                },
                |ui| {
                    app.inspector_panel(ui);
                    ui.add(egui::TextEdit::singleline(&mut other_text))
                        .request_focus();
                },
            );
            assert!(ctx.egui_wants_keyboard_input());
            app.inspector = false;
            let _ = ctx.run_test_ui(
                egui::RawInput {
                    screen_rect,
                    events: vec![
                        egui::Event::ModifiersChanged(modifiers),
                        egui::Event::Key {
                            key: Key::F,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    app.keyboard(&ctx);
                    assert!(!ctx.input(|input| input.key_pressed(Key::F)));
                    app.inspector_panel(ui);
                },
            );
            // Allow newly opened windows and disclosures to finish their layout.
            for _ in 0..2 {
                let _ = ctx.run_test_ui(
                    egui::RawInput {
                        screen_rect,
                        ..Default::default()
                    },
                    |ui| {
                        app.inspector_panel(ui);
                    },
                );
            }
            assert!(app.inspector);
            assert!(!app.focus_outliner_search);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(egui::Id::unique(OUTLINER_SEARCH_ID))
            );
            assert_eq!(app.outliner_filter, "Cabinet");
            assert_eq!(app.tool, Tool::Select);

            let mut storage = SavedPreferences::default();
            app.persist_inspector_changes(&ctx, Some(&mut storage), false);
            assert!(!app.project.modified);
            let restored_ctx = egui::Context::default();
            let mut restored = KetchupApp::with_document(Document::default(), None, true);
            restored.restore_inspector_preferences(&restored_ctx, Some(&storage));
            assert!(restored.inspector);
            assert!(
                restored
                    .current_inspector_preferences(&restored_ctx)
                    .sections["outliner"]
            );
        }
    }
}

#[test]
fn unavailable_or_invalid_saved_order_uses_all_default_sections() {
    assert_eq!(restore_order(None), InspectorSection::ALL);
    let mut storage = SavedPreferences::default();
    assert_eq!(restore_order(Some(&storage)), InspectorSection::ALL);
    storage.set_string(ORDER_PREFERENCE_KEY, "corrupt preferences".into());
    assert_eq!(restore_order(Some(&storage)), InspectorSection::ALL);
    eframe::set_value(
        &mut storage,
        ORDER_PREFERENCE_KEY,
        &[InspectorSection::Tags; 5],
    );
    assert_eq!(restore_order(Some(&storage)), InspectorSection::ALL);
    eframe::set_value(
        &mut storage,
        ORDER_PREFERENCE_KEY,
        &InspectorSection::ALL[..4].to_vec(),
    );
    assert_eq!(restore_order(Some(&storage)), InspectorSection::ALL);
}

#[test]
fn sidebar_preferences_save_immediately_and_restore_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.inspector = false;
    app.inspector_width = 420.;
    app.component_filter = "Cabinet".into();
    app.paint_color = [12, 34, 56];
    let mut display = serde_json::to_value(DisplaySettings::default()).unwrap();
    // Exercise every toggle, including fields added to DisplaySettings later.
    for value in display.as_object_mut().unwrap().values_mut() {
        if let Some(enabled) = value.as_bool() {
            *value = serde_json::Value::Bool(!enabled);
        }
    }
    app.doc.display = serde_json::from_value(display).unwrap();
    app.doc.display.face_style = FaceStyle::Wireframe;
    app.doc.display.profile_width = 7.;
    app.doc.display.sun_angle = 280.;
    app.doc.display.sky_color = [3, 4, 5];
    app.doc.camera.projection = Projection::Parallel;
    app.doc.camera.fov = 65_f64.to_radians();
    app.doc
        .camera
        .set_standard_view(ketchup_core::StandardView::Bottom);
    for (key, open) in [
        ("display", false),
        ("fog", true),
        ("view", false),
        ("view_camera", true),
        ("view_standard_views", true),
        ("outliner_object/42", true),
    ] {
        let id = egui::Id::unique(key);
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(&ctx, id, false);
        state.set_open(open);
        state.store(&ctx);
        remember_section(&ctx, id, key);
    }
    let expected = app.current_inspector_preferences(&ctx);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());

    let refreshed_ctx = egui::Context::default();
    let mut refreshed = KetchupApp::with_document(Document::default(), None, true);
    refreshed.restore_inspector_preferences(&refreshed_ctx, Some(&storage));
    assert_eq!(
        refreshed.current_inspector_preferences(&refreshed_ctx),
        expected
    );
    assert!(!refreshed.project.modified);

    storage.flushed = false;
    refreshed.persist_inspector_changes(&refreshed_ctx, Some(&mut storage), false);
    assert!(
        !storage.flushed,
        "unchanged preferences must not write every frame"
    );
}

#[test]
fn bulk_expansion_overrides_existing_headers_and_restores_after_immediate_save() {
    fn render(ctx: &egui::Context, moved: bool) -> (bool, bool) {
        let mut visible = (false, false);
        let _ = ctx.run_test_ui(egui::RawInput::default(), |ui| {
            ui.push_id(if moved { "floating" } else { "docked" }, |ui| {
                let id = ui.make_persistent_id(("outliner_object", 42_u64));
                restore_section(ctx, id, "outliner_object/42", false);
                let state = egui::collapsing_header::CollapsingState::load(ctx, id).unwrap();
                visible.0 = state.is_open();
                remember_section(ctx, id, "outliner_object/42");
                let key = "outliner_geometry/Some(42)";
                let open = restored_header_open(ui, key, false);
                let header = egui::CollapsingHeader::new("Geometry (1)")
                    .id_salt(("outliner_geometry", Some(42_u64)))
                    .open(open)
                    .show(ui, |ui| {
                        visible.1 = true;
                        ui.text_label("Edge");
                    });
                remember_section(ctx, header.header_response.id, key);
            });
        });
        visible
    }

    let ctx = egui::Context::default();
    ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
    assert_eq!(render(&ctx, false), (false, false));
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    for open in [true, false] {
        for key in ["outliner_object/42", "outliner_geometry/Some(42)"] {
            set_section_open(&ctx, key, open);
        }
        // Save before another render, just as an immediate reload would.
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert_eq!(render(&ctx, false), (open, open));
        assert_eq!(render(&ctx, false), (open, open));

        let refreshed_ctx = egui::Context::default();
        refreshed_ctx.style_mut_of(refreshed_ctx.theme(), |style| style.animation_time = 0.);
        let mut refreshed = KetchupApp::with_document(Document::default(), None, true);
        refreshed.restore_inspector_preferences(&refreshed_ctx, Some(&storage));
        assert_eq!(render(&refreshed_ctx, true), (open, open));
        assert!(!refreshed.project.modified);
    }
    assert!(!app.project.modified);
    assert!(app.dirty_since.is_none());
}

#[test]
fn expanded_estimate_panels_render_all_finish_grids_without_id_clashes() {
    fn collect_text(shape: &egui::Shape, labels: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => labels.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, labels);
                }
            }
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
    ctx.options_mut(|options| options.warn_on_id_clash = true);
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.time_estimates.pre_stain.enabled = true;
    app.time_estimates.stain.enabled = true;
    app.time_estimates.varnish.enabled = true;
    for key in [
        "settings/treatment/sanding",
        "settings/treatment/finishing",
        "settings/treatment/pre_stain",
        "settings/treatment/stain",
        "settings/treatment/varnish",
    ] {
        set_section_open(&ctx, key, true);
    }
    for frame in 0..3 {
        let output = ctx.run_test_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(600., 2000.))),
                time: Some(frame as f64 / 60.),
                ..Default::default()
            },
            |ui| {
                ui.set_max_width(320.);
                time_estimates::treatment_controls(
                    ui,
                    &mut app.time_estimates,
                    true,
                    &app.general_settings.stock.finishes,
                    &mut app.finish_product_filters,
                );
            },
        );
        let mut labels = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut labels);
        }
        assert_eq!(
            labels
                .iter()
                .filter(|text| text.as_str() == "Minutes / m²")
                .count(),
            app.time_estimates.sanding_grits.len() + 3
        );
        assert!(
            !labels.iter().any(|text| text.contains("Grid ID")),
            "{labels:?}"
        );
    }
}

#[test]
fn estimator_appends_to_previously_complete_orders() {
    let previous = vec![
        InspectorSection::View,
        InspectorSection::Units,
        InspectorSection::Display,
        InspectorSection::Tags,
        InspectorSection::Components,
        InspectorSection::EntityInfo,
        InspectorSection::Outliner,
    ];
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, ORDER_PREFERENCE_KEY, &previous);
    let restored = restore_order(Some(&storage));
    assert_eq!(&restored[..previous.len()], previous.as_slice());
    assert_eq!(
        restored[restored.len() - 4],
        InspectorSection::TimeCostEstimates
    );
    assert_eq!(restored[restored.len() - 3], InspectorSection::Materials);
    assert_eq!(restored[restored.len() - 2], InspectorSection::History);
    assert_eq!(restored.last(), Some(&InspectorSection::ModelLibrary));
}

#[test]
fn original_fixed_array_order_migrates_without_losing_layout() {
    let original = [
        InspectorSection::Tags,
        InspectorSection::Display,
        InspectorSection::Outliner,
        InspectorSection::EntityInfo,
        InspectorSection::Components,
    ];
    let expected = [
        InspectorSection::Tags,
        InspectorSection::Display,
        InspectorSection::Outliner,
        InspectorSection::EntityInfo,
        InspectorSection::Components,
        InspectorSection::View,
        InspectorSection::Units,
        InspectorSection::TimeCostEstimates,
        InspectorSection::Materials,
        InspectorSection::History,
        InspectorSection::ModelLibrary,
    ];
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, ORDER_PREFERENCE_KEY, &original);
    assert_eq!(restore_order(Some(&storage)), expected);
    save_order(&expected, &mut storage);
    assert_eq!(restore_order(Some(&storage)), expected);
}

#[test]
fn inspector_disclosure_defaults_and_saved_overrides_survive_reload() {
    fn render(ctx: &egui::Context, section: InspectorSection, sticky: bool) -> bool {
        let mut visible = false;
        let _ = ctx.run_test_ui(egui::RawInput::default(), |ui| {
            ui.push_id((section.id(), sticky), |ui| {
                let viewport = ui.clip_rect();
                inspector_section_sized(
                    ui,
                    section.title(),
                    section.id(),
                    true,
                    None,
                    sticky.then_some(viewport),
                    |_| visible = true,
                );
            });
        });
        visible
    }

    for sticky in [false, true] {
        for legacy in [false, true] {
            let ctx = egui::Context::default();
            let mut storage = SavedPreferences::default();
            if legacy {
                eframe::set_value(
                    &mut storage,
                    PREFERENCE_KEY,
                    &serde_json::from_str::<InspectorPreferences>("{}").unwrap(),
                );
            }
            let mut app = KetchupApp::with_document(Document::default(), None, true);
            app.restore_inspector_preferences(&ctx, Some(&storage));
            for section in InspectorSection::ALL {
                let expected = matches!(
                    section,
                    InspectorSection::EntityInfo | InspectorSection::Outliner
                );
                assert_eq!(render(&ctx, section, sticky), expected, "{section:?}");
                set_section_open(&ctx, section.id(), !expected);
            }
            app.persist_inspector_changes(&ctx, Some(&mut storage), false);
            assert!(storage.flushed);
            assert!(!app.project.modified);
            assert!(app.dirty_since.is_none());

            let reloaded_ctx = egui::Context::default();
            let mut reloaded = KetchupApp::with_document(Document::default(), None, true);
            reloaded.restore_inspector_preferences(&reloaded_ctx, Some(&storage));
            for section in InspectorSection::ALL {
                assert_eq!(
                    render(&reloaded_ctx, section, !sticky),
                    !section.default_open(),
                    "{section:?}"
                );
            }
        }
    }
}

#[test]
fn properties_and_outliner_default_first_and_units_append_to_existing_layouts() {
    let defaults = restore_order(None);
    assert_eq!(
        &defaults[..3],
        &[
            InspectorSection::EntityInfo,
            InspectorSection::Outliner,
            InspectorSection::Units,
        ]
    );

    let previous = vec![
        InspectorSection::View,
        InspectorSection::Tags,
        InspectorSection::Display,
        InspectorSection::Outliner,
        InspectorSection::EntityInfo,
        InspectorSection::Components,
    ];
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, ORDER_PREFERENCE_KEY, &previous);
    let restored = restore_order(Some(&storage));
    assert_eq!(&restored[..previous.len()], previous.as_slice());
    assert_eq!(restored[previous.len()], InspectorSection::Units);
    assert_eq!(
        restored[restored.len() - 4],
        InspectorSection::TimeCostEstimates
    );
    assert_eq!(restored[restored.len() - 3], InspectorSection::Materials);
    assert_eq!(restored[restored.len() - 2], InspectorSection::History);
    assert_eq!(restored.last(), Some(&InspectorSection::ModelLibrary));
}

#[test]
fn grit_header_clicks_toggle_and_persist_while_remove_stays_independent() {
    const KEY: &str = "entity/treatment/sanding/grit/80";
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> (Rect, bool) {
        let mut header = Rect::NOTHING;
        let mut removed = false;
        let _ = ctx.run_test_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_width(400.);
                let origin = ui.next_widget_position() + egui::vec2(2., 2.);
                let height = (ui.text_style_height(&egui::TextStyle::Button) + 14.)
                    .max(32.);
                header = Rect::from_min_size(origin, egui::vec2(396., height));
                removed = inspector_section_with_remove(ui, "80 grit", KEY, |ui| {
                    ui.text_label("Sanding controls");
                });
            },
        );
        (header, removed)
    }
    fn click(ctx: &egui::Context, pos: Pos2) -> bool {
        frame(
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .1
    }
    let ctx = egui::Context::default();
    ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
    let (header, _) = frame(&ctx, vec![]);
    let open = |ctx: &egui::Context| {
        ctx.data_mut(|data| {
            data.get_temp::<SectionMemory>(section_memory_id())
                .unwrap()
                .0[KEY]
        })
    };
    assert!(open(&ctx));
    // Arrow, title, and blank header space each toggle exactly once.
    for x in [10., 60., 240.] {
        let pos = egui::pos2(header.left() + x, header.center().y);
        assert!(!click(&ctx, pos));
        assert!(!open(&ctx));
        assert!(!click(&ctx, pos));
        assert!(open(&ctx));
    }
    assert!(click(
        &ctx,
        egui::pos2(
            header.right() - 22.,
            header.center().y,
        ),
    ));
    assert!(open(&ctx));
    click(&ctx, header.center());
    assert!(!open(&ctx));

    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    let restored_ctx = egui::Context::default();
    app.restore_inspector_preferences(&restored_ctx, Some(&storage));
    frame(&restored_ctx, vec![]);
    assert!(!open(&restored_ctx));
}

#[test]
fn grit_disclosures_keep_identity_and_save_without_a_model_edit() {
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let alias = "entity/treatment/sanding/grit/80";
    let renamed = "entity/treatment/sanding/grit/1000";
    let key = item_section_key(&ctx, alias);
    assert_ne!(
        key,
        item_section_key(&ctx, "entity/treatment/sanding/grit/120")
    );
    assert_ne!(
        key,
        item_section_key(&ctx, "settings/treatment/sanding/grit/80")
    );
    let id = egui::Id::unique("grit_docked");
    restore_section(&ctx, id, &key, true);
    assert!(
        egui::collapsing_header::CollapsingState::load(&ctx, id)
            .unwrap()
            .is_open()
    );
    set_section_open(&ctx, &key, false);
    rename_item_section(&ctx, alias, renamed);
    assert_eq!(item_section_key(&ctx, renamed), key);

    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    assert!(storage.flushed);
    assert!(!app.project.modified);
    let restored_ctx = egui::Context::default();
    app.restore_inspector_preferences(&restored_ctx, Some(&storage));
    assert_eq!(item_section_key(&restored_ctx, renamed), key);
    let moved_id = egui::Id::unique("grit_floating");
    restore_section(&restored_ctx, moved_id, &key, true);
    assert!(
        !egui::collapsing_header::CollapsingState::load(&restored_ctx, moved_id)
            .unwrap()
            .is_open()
    );
    let older: InspectorPreferences = serde_json::from_str("{}").unwrap();
    assert!(older.section_aliases.is_empty());
}

#[test]
fn older_section_orders_keep_their_layout_and_append_new_sections() {
    let mut storage = SavedPreferences::default();
    eframe::set_value(
        &mut storage,
        ORDER_PREFERENCE_KEY,
        &vec![InspectorSection::Tags, InspectorSection::Display],
    );
    assert_eq!(
        restore_order(Some(&storage)),
        [
            InspectorSection::Tags,
            InspectorSection::Display,
            InspectorSection::Outliner,
            InspectorSection::EntityInfo,
            InspectorSection::Components,
            InspectorSection::View,
            InspectorSection::Units,
            InspectorSection::TimeCostEstimates,
            InspectorSection::Materials,
            InspectorSection::History,
            InspectorSection::ModelLibrary,
        ]
    );
}

#[test]
fn materials_append_to_existing_orders_and_save_without_model_edits() {
    let previous = vec![
        InspectorSection::View,
        InspectorSection::Tags,
        InspectorSection::Units,
        InspectorSection::Display,
        InspectorSection::Outliner,
        InspectorSection::EntityInfo,
        InspectorSection::TimeCostEstimates,
        InspectorSection::Components,
    ];
    let mut storage = SavedPreferences::default();
    eframe::set_value(&mut storage, ORDER_PREFERENCE_KEY, &previous);
    let ctx = egui::Context::default();
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    app.restore_inspector_preferences(&ctx, Some(&storage));
    assert_eq!(&app.inspector_order[..previous.len()], previous.as_slice());
    assert_eq!(
        app.inspector_order.get(previous.len()),
        Some(&InspectorSection::Materials)
    );

    // Older preferences have no disclosure entry, so Materials starts collapsed.
    let id = egui::Id::unique("materials_default");
    restore_section(
        &ctx,
        id,
        "materials",
        InspectorSection::Materials.default_open(),
    );
    assert!(
        !egui::collapsing_header::CollapsingState::load(&ctx, id)
            .unwrap()
            .is_open()
    );
    let model = app.doc.model.clone();
    for open in [false, true] {
        set_section_open(&ctx, "materials", open);
        storage.flushed = false;
        app.persist_inspector_changes(&ctx, Some(&mut storage), false);
        assert!(storage.flushed);
        assert!(!app.project.modified);
        assert_eq!(app.doc.model, model);

        let reloaded_ctx = egui::Context::default();
        let mut reloaded = KetchupApp::with_document(Document::default(), None, true);
        reloaded.restore_inspector_preferences(&reloaded_ctx, Some(&storage));
        assert_eq!(reloaded.inspector_order, app.inspector_order);
        let _ = reloaded_ctx.run_test_ui(egui::RawInput::default(), |ui| {
            ui.push_id("floating_materials", |ui| {
                assert_eq!(restored_header_open(ui, "materials", true), Some(open));
            });
        });
        assert_eq!(reloaded.doc.model, model);
        assert!(!reloaded.project.modified);
    }
}

#[test]
fn new_sections_restore_after_click_save_reload_and_layout_change() {
    fn render(ctx: &egui::Context, events: Vec<egui::Event>, moved: bool) -> (Rect, bool) {
        let mut rect = Rect::NOTHING;
        let mut body_visible = false;
        let _ = ctx.run_test_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(600., 900.))),
                events,
                ..Default::default()
            },
            |ui| {
                ui.push_id(if moved { "floating" } else { "docked" }, |ui| {
                    rect =
                        inspector_section(ui, "Future option", "future_option", false, |ui| {
                            body_visible = true;
                            ui.text_label("Future controls");
                        })
                        .inner
                        .rect;
                });
            },
        );
        (rect, body_visible)
    }
    let ctx = egui::Context::default();
    ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.);
    assert!(!render(&ctx, vec![], false).1);
    let (rect, _) = render(&ctx, vec![], false);
    for pressed in [true, false] {
        render(
            &ctx,
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            false,
        );
    }
    assert!(render(&ctx, vec![], false).1);
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let mut storage = SavedPreferences::default();
    app.persist_inspector_changes(&ctx, Some(&mut storage), false);
    let new_ctx = egui::Context::default();
    new_ctx.style_mut_of(new_ctx.theme(), |style| style.animation_time = 0.);
    let mut refreshed = KetchupApp::with_document(Document::default(), None, true);
    refreshed.restore_inspector_preferences(&new_ctx, Some(&storage));
    assert!(render(&new_ctx, vec![], true).1);
}
