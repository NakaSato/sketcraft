use super::*;
use crate::scenario::{self, Harness};

fn harness() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":2400,"height":1000}));
    h.app.session.execute("solid.box", &json!({"length":10,"width":10,"height":10})).unwrap();
    h.app.ui.tab = "INSPECT".into();
    for id in PERSISTENT_COMMANDS {
        crate::toolbar_custom::pin(&mut h.app, "INSPECT", id).unwrap();
    }
    h.frames(3);
    h
}

fn shapes(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(v) => {
            for s in v {
                shapes(s, out);
            }
        }
        s => out.push(s.clone()),
    }
}

fn button_shapes(h: &Harness, id: &str) -> Vec<egui::Shape> {
    let r = scenario::rect_of(&format!("button:{id}")).unwrap();
    let mut all = Vec::new();
    for s in &h.shapes {
        shapes(&s.shape, &mut all);
    }
    all.into_iter().filter(|s| r.intersects(s.visual_bounding_rect())).collect()
}

fn assert_paint(h: &Harness, id: &str, selected: bool) {
    assert_presentation(h, id, selected, true);
}

fn assert_presentation(h: &Harness, id: &str, selected: bool, check_fill: bool) {
    let r = scenario::rect_of(&format!("button:{id}")).unwrap();
    let t = Tokens::get();
    let shapes = button_shapes(h, id);
    if check_fill {
        assert_eq!(
            shapes.iter().any(|s| matches!(s, egui::Shape::Rect(s) if s.rect == r && s.fill == t.selected_bg)),
            selected,
            "{id} selected fill"
        );
    }
    assert_eq!(shapes.iter().any(|s| matches!(s, egui::Shape::LineSegment { points, stroke } if stroke.width == 2.0 && stroke.color == t.accent && points[0].y == r.bottom() - t.space_4)), selected, "{id} underline");
    if h.app.preferences.ribbon_labels {
        let text = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(s) if r.contains(s.pos) => Some(s),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {id} label"));
        let font = &text.galley.job.sections[0].format.font_id;
        assert_eq!(*font, if selected { crate::fonts::semibold(t.font_body) } else { FontId::proportional(t.font_body) });
        assert_eq!(text.galley.job.sections[0].format.color, if selected { t.selected_ink } else { t.text });
    }
}

fn tap(h: &mut Harness, id: &str) {
    let at = scenario::handle_at(&format!("button:{id}")).unwrap();
    let before = h.app.session.status_items();
    h.frame_with(crate::test_support::pointer(at, true));
    assert_eq!(h.app.session.status_items(), before, "pointer-down is not a persistent activation");
    for command in PERSISTENT_COMMANDS {
        // Existing pointer-down paint is transient and may use the same palette fill.
        // The underline, actual weight and published identities distinguish persistence.
        assert_presentation(h, command, before.iter().any(|item| item.command == *command), *command != id);
        assert_eq!(scenario::handle_at(&format!("ribbon:pressed:{command}")).is_some(), before.iter().any(|item| item.command == *command));
    }
    h.frame_with(crate::test_support::pointer(at, false));
}

#[test]
fn persistent_pressed_and_search_pulse_keep_independent_effect_and_paint_authority() {
    let mut h = harness();
    for enabled in [true, false] {
        h.call("ui.prefs", json!({"persistent_ribbon_pressed":enabled,"ribbon_reveal":true}));
        h.app.sbox.reveal = Some(crate::shortcut_box::Reveal {
            id: "inspect.zebra",
            since: h.ctx.input(|input| input.time) - crate::shortcut_box::REVEAL_PULSE / 4.0,
            preview: false,
            back: "INSPECT".into(),
        });
        h.frame();
        let zebra = scenario::rect_of("button:inspect.zebra").unwrap();
        assert_eq!(scenario::rect_of("ribbon:reveal"), Some(zebra));
        assert!(h.app.session.status_items().is_empty(), "a search pulse does not activate an effect");
        assert_presentation(&h, "inspect.zebra", false, false);
        h.app.session.execute("inspect.zebra", &json!({})).unwrap();
        h.frame();
        assert_presentation(&h, "inspect.zebra", enabled, enabled);
        let accent = crate::theme::TabAccent::for_tab(&Tokens::get(), "INSPECT");
        assert!(button_shapes(&h, "inspect.zebra").iter().any(|shape| matches!(shape, egui::Shape::Rect(rect) if rect.rect == zebra && rect.stroke.width == 2.0 && rect.stroke.color == accent.color.gamma_multiply(h.app.sbox.reveal.as_ref().unwrap().strength(h.ctx.input(|input| input.time), h.ctx.global_style().animation_time > 0.0)))), "the upstream pulse remains painted above the selected background");
        for (id, active) in [("inspect.draft", true), ("inspect.draft", false)] {
            let at = scenario::handle_at(&format!("button:{id}")).unwrap();
            h.frame_with(crate::test_support::pointer(at, true));
            h.frame_with(crate::test_support::pointer(at, false));
            assert_presentation(&h, "inspect.zebra", false, false);
            assert_eq!(h.app.session.status_items().iter().any(|item| item.command == id), active);
            assert_paint(&h, id, active && enabled);
            assert_eq!(scenario::rect_of("ribbon:reveal"), Some(zebra));
        }
        assert!(h.app.session.status_items().is_empty());
        h.app.sbox.reveal = None;
        h.frame();
        assert!(scenario::rect_of("ribbon:reveal").is_none());
    }
}

fn open_status_grid(h: &mut Harness, panel: &str) {
    h.app.preferences.ribbon_icon_grid = true;
    h.app.preferences.ribbon_menus_skip_pinned = false;
    h.frames(3);
    h.step(&json!({"click":{"handle":format!("panel:INSPECT:{panel}")}})).unwrap();
    // Exact token colours are checked after the native Area fade, not during its opacity ramp.
    let settle_frames = (h.ctx.global_style().animation_time * 60.0).ceil() as usize + 2;
    h.frames(settle_frames.max(3));
    assert!(scenario::rect_of("toolbar_dropdown").is_some());
}

fn close_status_grid(h: &mut Harness, panel: &str) {
    // Use the real header toggle so closing a menu never supplies an idle effect Escape.
    h.step(&json!({"click":{"handle":format!("panel:INSPECT:{panel}")}})).unwrap();
    h.frames(3);
    assert!(scenario::rect_of("toolbar_dropdown").is_none());
}

fn status_grid_rect(_h: &Harness, id: &str) -> Rect {
    let at = scenario::handle_at(&format!("dropdown:{id}")).unwrap();
    Rect::from_center_size(at, vec2(GRID_CELL, GRID_CELL)).shrink(2.0)
}

fn assert_status_grid(h: &Harness, id: &str, selected: bool, toggled: Option<egui::accesskit::Toggled>) {
    let rect = status_grid_rect(h, id);
    let t = Tokens::get();
    let mut drawn = Vec::new();
    for shape in &h.shapes {
        shapes(&shape.shape, &mut drawn);
    }
    assert_eq!(
        drawn.iter().any(|shape| matches!(shape, egui::Shape::Rect(s) if s.rect == rect && s.fill == t.selected_bg)),
        selected,
        "{id} grid fill"
    );
    assert_eq!(drawn.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke } if stroke.width == 2.0 && stroke.color == t.accent && points[0].y == rect.bottom() - t.space_4 && rect.contains(points[0]))), selected, "{id} grid underline");
    let command = sketcraft_engine::find_command(id).unwrap();
    let label = if toggled.is_some() { persistent_button_name(&h.app, command) } else { h.app.tr_label(command.label).to_string() };
    let node = h
        .output
        .accesskit_update
        .as_ref()
        .and_then(|tree| {
            tree.nodes.iter().find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button
                    && node.label() == Some(label.as_str())
                    && node.bounds().is_some_and(|b| rect.contains(pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
            })
        })
        .map(|(_, node)| node)
        .expect("the actual grid button, distinguished from a same-named pinned ribbon button by bounds");
    assert_eq!(node.toggled(), toggled, "{id} grid accessibility");
}

#[test]
fn persistent_grid_deferred_paint_retains_partial_and_offscreen_scroll_clips() {
    let mut saw_partial = false;
    let mut saw_offscreen = false;
    for offset in [0.0, 24.0, 96.0] {
        let mut h = harness();
        h.app.session.execute("inspect.section", &json!({"plane":"XY","at":4})).unwrap();
        h.app.session.execute("inspect.zebra", &json!({})).unwrap();
        let effects = h.app.session.status_items();
        let ctx = h.ctx.clone();
        let mut expected = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 240.0))), focused: true, ..Default::default() },
            |ui| {
                expected.clear(); // Context layout passes never accumulate stale paint expectations.
                // Leave visible space above the scroll viewport, where a broadened clip leaks.
                ui.add_space(80.0);
                let mut paints = Vec::new();
                egui::ScrollArea::vertical()
                    .id_salt("persistent_grid_clip_regression")
                    .auto_shrink([false, false])
                    .max_height(64.0)
                    .min_scrolled_height(64.0)
                    .vertical_scroll_offset(offset)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for id in ["inspect.section", "inspect.zebra", "inspect.draft"] {
                            let command = sketcraft_engine::find_command(id).unwrap();
                            let (rect, _) = ui.allocate_exact_size(vec2(GRID_CELL, GRID_CELL), Sense::hover());
                            let clip = ui.clip_rect().intersect(rect);
                            let _ = dropdown_cell(&h.app, ui, command, rect, false, &mut paints);
                            if id != "inspect.draft" {
                                expected.push((id, rect.shrink(2.0), clip));
                            }
                        }
                    });
                // This is deliberately the larger parent Ui, matching the real post-dispatch call.
                paint_persistent_ribbon(&h.app, ui, paints, "INSPECT", "ribbon_grid");
            },
        );
        // Drawn outside an app frame, so nothing uploads the cells' icon textures (as
        // `test_support::frame`).
        output.textures_delta.clear();
        assert_eq!(h.app.session.status_items(), effects, "rendering real active cells never changes their effects");
        for (id, cell, original_clip) in expected {
            let mut backgrounds = Vec::new();
            for clipped in &output.shapes {
                let mut drawn = Vec::new();
                shapes(&clipped.shape, &mut drawn);
                if drawn.iter().any(|shape| matches!(shape, egui::Shape::Rect(rect) if rect.rect == cell && rect.fill == Tokens::get().selected_bg)) {
                    backgrounds.push(clipped.clip_rect);
                }
            }
            if original_clip.intersect(cell).is_positive() {
                assert!(!backgrounds.is_empty(), "{id} actually paints its visible selected fragment at offset {offset}");
                assert!(
                    backgrounds.iter().all(|clip| *clip == original_clip),
                    "{id} retains its original cell/scroll clip, not the parent clip, at offset {offset}"
                );
                saw_partial |= !original_clip.contains_rect(cell);
            } else {
                assert!(
                    backgrounds.iter().all(|clip| !clip.intersect(cell).is_positive()),
                    "offscreen {id} must not paint into the space above its scroll viewport"
                );
                saw_offscreen = true;
            }
        }
    }
    assert!(saw_partial, "the native ScrollArea must actually partly clip an active cell");
    assert!(saw_offscreen, "the native ScrollArea must actually scroll an active cell out of view");
}

#[test]
fn persistent_grid_uses_actual_section_and_analysis_with_independent_compatibility_preferences() {
    for section in [false, true] {
        for ribbon in [false, true] {
            let mut h = harness();
            h.app.preferences.viewport_analysis_status = false;
            h.app.preferences.viewport_section_status = section;
            h.app.preferences.persistent_ribbon_pressed = ribbon;
            h.app.session.execute("inspect.zebra", &json!({"stripes":14})).unwrap();
            h.app.session.execute("inspect.section", &json!({"plane":"XY","at":4})).unwrap();
            let effects = h.app.session.status_items();
            open_status_grid(&mut h, "SECTION");
            assert_status_grid(&h, "inspect.section", section && ribbon, (section && ribbon).then_some(egui::accesskit::Toggled::True));
            assert_eq!(h.app.session.status_items(), effects);
            close_status_grid(&mut h, "SECTION");
            open_status_grid(&mut h, "SURFACE QUALITY");
            assert_status_grid(&h, "inspect.zebra", ribbon, ribbon.then_some(egui::accesskit::Toggled::True));
            assert_eq!(h.app.session.status_items(), effects, "presentation preferences and menu navigation never mutate effects");
            close_status_grid(&mut h, "SURFACE QUALITY");
        }
    }
}

#[test]
fn persistent_grid_tracks_all_six_analysis_replacements_queries_and_failed_activation() {
    let mut h = harness();
    h.app.session.execute("inspect.section", &json!({"plane":"YZ","at":3})).unwrap();
    let cut = h.app.session.section;
    for id in PERSISTENT_COMMANDS.iter().copied().filter(|id| *id != "inspect.section") {
        h.app.session.execute(id, &json!({})).unwrap();
        let before = h.app.session.status_items();
        h.app.session.execute("view.status_items", &json!({})).unwrap();
        assert!(h.app.session.execute("inspect.draft", &json!({"pull":[0,0,0]})).is_err());
        assert_eq!(h.app.session.status_items(), before);
        let panel = if matches!(id, "inspect.draft" | "inspect.accessibility") { "CHECK" } else { "SURFACE QUALITY" };
        open_status_grid(&mut h, panel);
        assert_status_grid(&h, id, true, Some(egui::accesskit::Toggled::True));
        assert_eq!(h.app.session.section, cut, "analysis replacement leaves Section independent");
        close_status_grid(&mut h, panel);
    }
}

#[test]
fn persistent_grid_keeps_native_keyboard_focus_and_active_tooltip() {
    let mut h = harness();
    h.app.session.execute("inspect.section", &json!({"plane":"XY","at":4})).unwrap();
    open_status_grid(&mut h, "SECTION");
    h.step(&json!({"key":"ArrowRight"})).unwrap();
    h.frames(3);
    assert!(scenario::rect_of("ribbon_grid:focus:inspect.section").is_some());
    assert_status_grid(&h, "inspect.section", true, Some(egui::accesskit::Toggled::True));
    h.step(&json!({"move":{"handle":"dropdown:inspect.section"}})).unwrap();
    h.frames(100);
    let command = sketcraft_engine::find_command("inspect.section").unwrap();
    let hint = persistent_hint(&h.app, command, true).unwrap();
    let mut drawn = Vec::new();
    for shape in &h.shapes {
        shapes(&shape.shape, &mut drawn);
    }
    assert!(
        drawn.iter().any(|shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == hint)),
        "the actual grid uses the same native turn-off tooltip"
    );
    assert!(h.app.session.section.is_some());
}

#[test]
fn persistent_grid_clear_uses_post_dispatch_session_without_stale_paint_or_cross_effect_clear() {
    for (id, panel) in [("inspect.section", "SECTION"), ("inspect.zebra", "SURFACE QUALITY")] {
        let mut h = harness();
        h.app.session.execute("inspect.zebra", &json!({})).unwrap();
        h.app.session.execute("inspect.section", &json!({"plane":"XY","at":4})).unwrap();
        open_status_grid(&mut h, panel);
        assert_status_grid(&h, id, true, Some(egui::accesskit::Toggled::True));
        let rect = status_grid_rect(&h, id);
        let at = scenario::handle_at(&format!("dropdown:{id}")).unwrap();
        let before = h.app.session.status_items();
        h.frame_with(crate::test_support::pointer(at, true));
        assert_eq!(h.app.session.status_items(), before, "pointer down is not a command activation");
        h.frame_with(crate::test_support::pointer(at, false));
        // Check the release frame, with no settling frame: both paint paths resolve after dispatch.
        assert_paint(&h, id, false);
        let mut drawn = Vec::new();
        for shape in &h.shapes {
            shapes(&shape.shape, &mut drawn);
        }
        let t = Tokens::get();
        // The pointer still hovers this cell: native hover may share the selected fill.
        // Persistence is distinguished by the underline, identity and native toggled state.
        assert!(
            drawn.iter().any(|shape| matches!(shape, egui::Shape::Rect(s) if s.rect == rect && s.fill == t.accent_soft)),
            "{id} retains its normal hovered fill on release"
        );
        assert!(!drawn.iter().any(|shape| matches!(shape, egui::Shape::LineSegment { points, stroke } if stroke.width == 2.0 && stroke.color == t.accent && points[0].y == rect.bottom() - t.space_4 && rect.contains(points[0]))), "{id} has no persistent underline on release");
        assert!(scenario::rect_of(&format!("ribbon_grid:pressed:{id}")).is_none(), "{id} has no published pressed identity on release");
        let command = sketcraft_engine::find_command(id).unwrap();
        let label = persistent_button_name(&h.app, command);
        let node = h
            .output
            .accesskit_update
            .as_ref()
            .and_then(|tree| {
                tree.nodes.iter().find(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button
                        && node.label() == Some(label.as_str())
                        && node.bounds().is_some_and(|b| rect.contains(pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
                })
            })
            .map(|(_, node)| node)
            .expect("the actual cleared grid button, distinguished from its pinned ribbon button by bounds");
        assert_eq!(node.toggled(), Some(egui::accesskit::Toggled::False), "{id} grid accessibility is off on the release frame");
        assert_eq!(h.app.session.section.is_some(), id != "inspect.section");
        assert_eq!(h.app.session.analysis.is_some(), id != "inspect.zebra");
    }
}

#[test]
fn section_persistent_ribbon_uses_independent_status_preference_and_active_toggle() {
    let mut h = harness();
    h.app.session.execute("inspect.zebra", &json!({})).unwrap();
    h.app.session.execute("inspect.section", &json!({"plane":"XY","at":4})).unwrap();
    h.frame();
    assert_paint(&h, "inspect.section", true);
    assert_paint(&h, "inspect.zebra", true);
    h.app.preferences.viewport_analysis_status = false;
    h.frame();
    assert_paint(&h, "inspect.section", true);
    h.app.preferences.viewport_section_status = false;
    h.frame();
    assert_paint(&h, "inspect.section", false);
    assert!(persistent_hint(&h.app, sketcraft_engine::find_command("inspect.section").unwrap(), true).is_none());
    assert!(h.app.session.section.is_some());
    h.app.preferences.viewport_section_status = true;
    h.app.preferences.persistent_ribbon_pressed = false;
    h.frame();
    assert_paint(&h, "inspect.section", false);
    h.app.preferences.persistent_ribbon_pressed = true;
    h.frames(2);
    tap(&mut h, "inspect.section");
    assert!(h.app.session.section.is_none() && h.app.session.analysis.is_some());
    assert_paint(&h, "inspect.section", false);
    assert_paint(&h, "inspect.zebra", true);
}

#[test]
fn persistent_commands_derive_all_seven_programmatic_effects_and_clear_without_residue() {
    let mut h = harness();
    for id in PERSISTENT_COMMANDS {
        assert!(!persistent_pressed(&h.app, id));
        h.app.session.execute(id, &if *id == "inspect.section" { json!({"plane":"XY"}) } else { json!({}) }).unwrap();
        h.frame(); // Exactly the first output following the engine mutation.
        assert_eq!(h.app.session.status_items().iter().map(|s| s.command.as_str()).collect::<Vec<_>>(), [*id]);
        for c in PERSISTENT_COMMANDS {
            assert_paint(&h, c, c == id);
        }
        let before = h.app.session.status_items();
        let revision = h.app.session.revision;
        h.frame();
        assert_eq!(h.app.session.status_items(), before);
        assert_eq!(h.app.session.revision, revision);
        h.app.session.execute(id, &json!({"clear":true})).unwrap();
        h.frame();
        assert_paint(&h, id, false);
    }
    for id in ["inspect.minimum_radius", "solid.extrude"] {
        assert!(!persistent_command(id));
    }
    h.app.session.execute("inspect.zebra", &json!({})).unwrap();
    let before = h.app.session.status_items();
    assert!(h.app.session.execute("inspect.draft", &json!({"pull":[0,0,0]})).is_err());
    h.frame();
    assert_eq!(h.app.session.status_items(), before);
    assert_paint(&h, "inspect.zebra", true);
}

#[test]
fn persistent_replacement_and_toggle_are_correct_in_the_click_release_frame_both_directions() {
    for labels in [true, false] {
        let mut h = harness();
        h.app.preferences.ribbon_labels = labels;
        crate::toolbar_custom::reorder(&mut h.app, "INSPECT", "inspect.draft", "SURFACE QUALITY", Some("inspect.zebra"));
        h.frames(2);
        for id in ["inspect.zebra", "inspect.draft", "inspect.zebra", "inspect.continuity", "inspect.environment_map"] {
            tap(&mut h, id); // No settling frame after release.
            assert_eq!(h.app.session.status_items().iter().map(|s| s.command.as_str()).collect::<Vec<_>>(), [id]);
            for c in PERSISTENT_COMMANDS {
                assert_paint(&h, c, *c == id);
            }
            let c = sketcraft_engine::find_command(id).unwrap();
            assert_eq!(
                persistent_hint(&h.app, c, true),
                Some(format!("Turn off {} · Esc", h.app.tr_label(crate::workspace::ribbon_button_label(c))))
            );
        }
        tap(&mut h, "inspect.environment_map");
        assert!(h.app.session.status_items().is_empty());
        assert_paint(&h, "inspect.environment_map", false);
        tap(&mut h, "inspect.continuity");
        tap(&mut h, "inspect.continuity");
        assert!(h.app.session.status_items().is_empty());
        assert_paint(&h, "inspect.continuity", false);
    }
}

#[test]
fn persistent_presentation_preference_preserves_old_help_and_legacy_two_effects() {
    let mut h = harness();
    h.call("ui.prefs", json!({"single_surface_analysis":false}));
    h.app.session.execute("inspect.zebra", &json!({})).unwrap();
    h.app.session.execute("inspect.continuity", &json!({})).unwrap();
    h.frame();
    assert_eq!(h.app.session.status_items().len(), 2);
    assert_paint(&h, "inspect.zebra", true);
    assert_paint(&h, "inspect.continuity", true);
    let before = h.app.session.status_items();
    h.app.preferences.persistent_ribbon_pressed = false;
    h.frame();
    for id in PERSISTENT_COMMANDS {
        assert_paint(&h, id, false);
    }
    assert_eq!(h.app.session.status_items(), before);
    let label = h.app.tr_label(sketcraft_engine::find_command("inspect.zebra").unwrap().info(&h.app.session).label);
    let old_node = h
        .output
        .accesskit_update
        .as_ref()
        .and_then(|tree| tree.nodes.iter().find(|(_, n)| n.label() == Some(label)))
        .map(|(_, n)| n)
        .expect("old neutral accessible button label");
    assert_eq!(old_node.toggled(), None, "the old presentation makes no false inactive-effect announcement");
    assert!(persistent_hint(&h.app, sketcraft_engine::find_command("inspect.zebra").unwrap(), true).is_none());
    h.app.preferences.persistent_ribbon_pressed = true;
    h.frame();
    assert_paint(&h, "inspect.zebra", true);
    assert_paint(&h, "inspect.continuity", true);
    h.app.last_command = Some(("inspect.draft".into(), "Draft".into()));
    h.frame();
    assert_paint(&h, "inspect.draft", false);
    assert!(persistent_hint(&h.app, sketcraft_engine::find_command("inspect.zebra").unwrap(), false).is_none());
}

#[test]
fn persistent_selected_shapes_and_real_semibold_survive_every_colour_vision_mode() {
    let mut h = harness();
    h.app.session.execute("inspect.zebra", &json!({})).unwrap();
    for dark in [false, true] {
        for vision in ["off", "protan", "deutan", "tritan"] {
            h.call("ui.prefs", json!({"theme":if dark { "dark" } else { "light" },"color_vision":vision}));
            h.frame();
            assert_paint(&h, "inspect.zebra", true);
            assert_paint(&h, "inspect.curvature_map", false);
        }
    }
    let painter = h.ctx.layer_painter(egui::LayerId::background());
    let plain = ribbon_button_galley_weight(&painter, "Zebra", 150.0, Tokens::get().text, false);
    let heavy = ribbon_button_galley_weight(&painter, "Zebra", 150.0, Tokens::get().text, true);
    assert!((plain.size().x - heavy.size().x).abs() > 0.1, "actual 600-weight face has different glyph metrics");
}

#[test]
fn persistent_keyboard_focus_uses_native_response_and_clears_with_space() {
    let mut h = harness();
    h.app.session.execute("inspect.zebra", &json!({})).unwrap();
    h.frame();
    let at = scenario::handle_at("button:inspect.zebra").unwrap();
    // A pointer press establishes the real existing response's keyboard focus.
    h.frame_with(crate::test_support::pointer(at, true));
    h.frame_with(crate::test_support::pointer(at, false));
    let id = h.ctx.memory(|m| m.focused()).expect("native ribbon response is focusable");
    assert!(h.app.session.status_items().is_empty());
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.frame_with(crate::test_support::key(egui::Key::Space));
    assert_eq!(h.app.session.status_items().len(), 1);
    assert_paint(&h, "inspect.zebra", true);
    assert!(scenario::rect_of("ribbon:focus:inspect.zebra").is_some());
    let node = h
        .output
        .accesskit_update
        .as_ref()
        .and_then(|tree| tree.nodes.iter().find(|(_, n)| n.label() == Some("Zebra")))
        .map(|(_, n)| n)
        .expect("accessible native Zebra button");
    assert_eq!(node.role(), egui::accesskit::Role::Button);
    assert_eq!(node.toggled(), Some(egui::accesskit::Toggled::True));
}

#[test]
fn persistent_tooltip_is_the_exact_translated_visible_label_and_old_help_is_reachable() {
    let mut h = harness();
    h.app.session.execute("inspect.zebra", &json!({})).unwrap();
    for language in ["en", "de", "ja"] {
        h.call("ui.prefs", json!({"language":language}));
        let command = sketcraft_engine::find_command("inspect.zebra").unwrap();
        let name = h.app.tr_label(crate::workspace::ribbon_button_label(command));
        let expected = crate::i18n::Locale::for_language(language).format("ui.viewport_status_clear_hint", &[("name", name)]);
        assert_eq!(persistent_hint(&h.app, command, true), Some(expected.clone()));
        let at = scenario::handle_at("button:inspect.zebra").unwrap();
        h.frame_with(vec![egui::Event::PointerMoved(at)]);
        h.frames(100);
        let mut all = Vec::new();
        for shape in &h.shapes {
            shapes(&shape.shape, &mut all);
        }
        assert!(all.iter().any(|s| matches!(s, egui::Shape::Text(t) if t.galley.text() == expected)), "actual tooltip {expected}");
    }
    h.app.preferences.persistent_ribbon_pressed = false;
    h.frame_with(vec![egui::Event::PointerMoved(egui::Pos2::ZERO)]);
    h.frame_with(vec![egui::Event::PointerMoved(scenario::handle_at("button:inspect.zebra").unwrap())]);
    h.frames(100);
    assert!(persistent_hint(&h.app, sketcraft_engine::find_command("inspect.zebra").unwrap(), true).is_none());
    assert_paint(&h, "inspect.zebra", false);
}

#[test]
fn persistent_dropdown_clear_keeps_native_pin_and_command_routes() {
    let mut h = harness();
    h.app.session.execute("inspect.environment_map", &json!({})).unwrap();
    h.frame();
    h.step(&json!({"click":{"handle":"panel:INSPECT:SURFACE QUALITY"}})).unwrap();
    assert!(scenario::handle_at("dropdown:inspect.environment_map").is_some());
    h.step(&json!({"click":{"handle":"dropdown:inspect.environment_map"}})).unwrap();
    assert!(h.app.session.status_items().is_empty());
    assert_paint(&h, "inspect.environment_map", false);
}

#[test]
fn persistent_registration_preserves_live_continuity_tool_without_a_status_item() {
    let mut h = harness();
    h.app.start("inspect.continuity");
    assert_eq!(h.app.tool.as_ref().map(|tool| tool.cmd), Some("inspect.continuity"));
    h.app.session.execute("inspect.continuity", &json!({"clear":true})).unwrap();
    h.frame();
    assert!(h.app.session.status_items().is_empty());
    assert_eq!(scenario::count_of("ribbon:pressed"), Some(0.0));
    assert_presentation(&h, "inspect.continuity", false, false);
    let r = scenario::rect_of("button:inspect.continuity").unwrap();
    let accent = crate::theme::TabAccent::for_tab(&Tokens::get(), "INSPECT");
    assert!(
        button_shapes(&h, "inspect.continuity")
            .iter()
            .any(|s| matches!(s, egui::Shape::Rect(s) if s.rect == r && s.fill == accent.soft && s.stroke.color == accent.color)),
        "the real live tool retains its transient indication"
    );
    h.app.session.execute("inspect.continuity", &json!({})).unwrap();
    h.frame();
    assert_paint(&h, "inspect.continuity", true);
    // A dialog's last-command fallback must not invent an analysis tool.
    h.app.tool = None;
    h.app.start("solid.box");
    assert!(h.app.dialog.is_some());
    h.app.ui.tab = "INSPECT".into();
    h.app.last_command = Some(("inspect.zebra".into(), "Zebra".into()));
    h.frame();
    assert_paint(&h, "inspect.zebra", false);
    let r = scenario::rect_of("button:inspect.zebra").unwrap();
    assert!(
        !button_shapes(&h, "inspect.zebra")
            .iter()
            .any(|s| matches!(s, egui::Shape::Rect(s) if s.rect == r && s.fill == accent.soft && s.stroke.color == accent.color)),
        "last_command is not a live analysis tool"
    );
}

#[test]
fn persistent_click_clear_and_replacement_remove_old_tool_paint_in_the_release_frame() {
    fn no_tool_paint(h: &Harness) {
        let r = scenario::rect_of("button:inspect.continuity").unwrap();
        let accent = crate::theme::TabAccent::for_tab(&Tokens::get(), "INSPECT");
        assert!(
            !button_shapes(h, "inspect.continuity")
                .iter()
                .any(|s| matches!(s, egui::Shape::Rect(s) if s.rect == r && s.fill == accent.soft && s.stroke.color == accent.color)),
            "no captured old live-tool fill/border after dispatch"
        );
        assert!(scenario::handle_at("ribbon:pressed:inspect.continuity").is_none());
        assert_paint(h, "inspect.continuity", false);
    }
    for labels in [true, false] {
        let mut h = harness();
        h.app.preferences.ribbon_labels = labels;
        crate::toolbar_custom::reorder(&mut h.app, "INSPECT", "inspect.continuity", "SURFACE QUALITY", Some("inspect.environment_map"));
        h.frames(2);
        tap(&mut h, "inspect.continuity");
        assert!(h.app.tool.is_some());
        tap(&mut h, "inspect.continuity");
        assert!(h.app.tool.is_none() && h.app.session.status_items().is_empty());
        no_tool_paint(&h);
        tap(&mut h, "inspect.continuity");
        tap(&mut h, "inspect.environment_map");
        assert!(h.app.tool.is_none());
        assert_eq!(h.app.session.status_items()[0].command, "inspect.environment_map");
        no_tool_paint(&h);
        assert_paint(&h, "inspect.environment_map", true);
    }
}
