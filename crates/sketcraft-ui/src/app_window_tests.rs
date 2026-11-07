//! Window chrome, toolbar, menus and native pointer batching.
use super::*;

#[test]
fn native_mouse_burst_batches_snapping_and_preserves_the_first_line_click() {
    let mut h = Harness::new();
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    h.app.set_tool(Tool::Line);
    let start = h.at(DVec3::ZERO);
    let end = h.at(DVec3::new(2., 1., 0.));
    let mut events = native_motion_burst(start - egui::vec2(30., 20.), start, 400);
    for pressed in [true, false] {
        events.push(egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    events.extend(native_motion_burst(start, end, 400));
    h.app.picking_preparations = 0;
    h.frame(events, egui::Modifiers::NONE);
    assert!(h.app.line_start.unwrap().length() < 1e-6);
    assert!(h.app.snap.unwrap().point.distance(DVec3::new(2., 1., 0.)) < 1e-6);
    assert!(h.app.doc.model.edges.is_empty());
    assert!(
        h.app.picking_preparations <= 4,
        "A motion burst must not cause a snap query per native mouse event: {}",
        h.app.picking_preparations
    );
    h.click(end);
    assert_eq!(h.app.doc.model.edges.len(), 1);
    let edge = h.app.doc.model.edges.values().next().unwrap();
    let (a, b) = h.app.doc.model.points(edge);
    assert!(a.length() < 1e-6);
    assert!(b.distance(DVec3::new(2., 1., 0.)) < 1e-6);
}

#[test]
fn selection_rectangle_tracks_native_motion_without_hover_picking() {
    let mut h = Harness::new();
    let edge = h.app.doc.model.add_edge(-DVec3::X, DVec3::X).unwrap();
    let from = h.app.viewport.min + Vec2::splat(10.);
    let to = h.app.viewport.max - Vec2::splat(10.);
    h.pointer(from, egui::PointerButton::Primary, true, false);
    h.app.hover = Some(edge);
    h.app.picking_preparations = 0;
    h.frame(native_motion_burst(from, to, 400), egui::Modifiers::NONE);
    assert_eq!(h.app.picking_preparations, 0);
    assert_eq!(h.app.gesture.as_ref().unwrap().last, to);
    assert_eq!(h.app.hover, None);
    assert!(h.app.selected.is_empty());
    h.pointer(to, egui::PointerButton::Primary, false, false);
    assert_eq!(h.app.selected, BTreeSet::from([edge]));
}

#[test]
fn native_mouse_deltas_do_not_discard_freehand_samples() {
    let mut h = Harness::new();
    h.app.doc.snap_to_geometry = false;
    h.app.doc.snap_to_axes = false;
    h.app.set_tool(Tool::Freehand);
    let start = h.app.viewport.center();
    let end = start + egui::vec2(180., -60.);
    h.pointer(start, egui::PointerButton::Primary, true, false);
    h.frame(native_motion_burst(start, end, 20), egui::Modifiers::NONE);
    assert!(h.app.drawing.as_ref().unwrap().points.len() > 5);
    h.pointer(end, egui::PointerButton::Primary, false, false);
    assert!(h.app.doc.model.edges.len() > 5);
}

#[test]
fn toolbar_window_actions_remain_accessible_at_minimum_width() {
    for width in [1440., 800.] {
        for (offset, maximized, command) in [
            (34., false, egui::ViewportCommand::Close),
            (74., false, egui::ViewportCommand::Maximized(true)),
            (74., true, egui::ViewportCommand::Maximized(false)),
            (114., false, egui::ViewportCommand::Minimized(true)),
        ] {
            let mut h = Harness::new();
            h.size.x = width;
            h.maximized = maximized;
            h.frame(vec![], egui::Modifiers::NONE);
            h.frame(vec![], egui::Modifiers::NONE);
            let output = h.click(egui::pos2(width - offset, 32.));
            assert_eq!(window_commands(&output), &[command], "width {width}");
            assert!(h.app.doc.model.edges.is_empty());
        }
    }
}

#[test]
fn toolbar_background_drags_and_double_click_toggles_maximize() {
    let mut h = Harness::new();
    let pos = egui::pos2(800., 32.);
    h.pointer(pos, egui::PointerButton::Primary, true, false);
    let output = h.frame(
        vec![egui::Event::PointerMoved(pos + egui::vec2(20., 0.))],
        egui::Modifiers::NONE,
    );
    assert!(window_commands(&output).contains(&egui::ViewportCommand::StartDrag));

    for maximized in [false, true] {
        let mut h = Harness::new();
        h.maximized = maximized;
        h.click(pos);
        let output = h.click(pos);
        assert_eq!(
            window_commands(&output),
            &[egui::ViewportCommand::Maximized(!maximized)]
        );
    }
}

#[test]
fn undecorated_window_keeps_resize_grips_and_disables_them_when_maximized() {
    let mut h = Harness::new();
    let pos = egui::pos2(1438., 898.);
    let output = h.pointer(pos, egui::PointerButton::Primary, true, false);
    assert_eq!(
        window_commands(&output),
        &[egui::ViewportCommand::BeginResize(
            egui::ResizeDirection::SouthEast
        )]
    );
    h.pointer(pos, egui::PointerButton::Primary, false, false);
    h.maximized = true;
    h.frame(vec![], egui::Modifiers::NONE);
    h.frame(vec![], egui::Modifiers::NONE);
    let output = h.pointer(pos, egui::PointerButton::Primary, true, false);
    assert!(window_commands(&output).is_empty());
}

#[test]
fn toolbar_order_and_measurement_flyout_defaults_and_remembers_the_choice() {
    let mut h = Harness::new();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let labels = [
        "Select",
        "Line",
        "Erase",
        "Orbit",
        "Push/Pull",
        "Scale",
        "Offset",
        "Move",
        "Paint",
        "Dimensions",
        "Union",
    ];
    let positions: Vec<_> = labels
        .iter()
        .map(|label| text_position(&output, label))
        .collect();
    assert!(positions.iter().all(|pos| pos.x < 76.));
    assert!(positions.windows(2).all(|pair| pair[0].y < pair[1].y));
    assert_eq!(h.app.measurement_tool, Tool::Dimensions);
    assert_eq!(h.app.solid_tool, Tool::Union);
    let rail = positions[9];
    for tool in Tool::MEASURING {
        h.click(rail);
        let menu = h.frame(vec![], egui::Modifiers::NONE);
        for option in Tool::MEASURING {
            assert_menu_icon(&menu, option.label());
        }
        h.click(menu_text_position(&menu, tool.label()));
        assert_eq!(h.app.tool, tool);
        assert_eq!(h.app.measurement_tool, tool);
        assert!(!egui::Popup::is_any_open(&h.ctx));
        h.key(Key::Space);
        assert_eq!(h.app.measurement_tool, tool);
    }
    h.key(Key::I);
    assert_eq!(h.app.measurement_tool, Tool::Dimensions);
    h.key(Key::T);
    assert_eq!(h.app.measurement_tool, Tool::TapeMeasure);
    h.key(Key::A);
    assert_eq!(h.app.measurement_tool, Tool::Protractor);
    assert!(h.app.doc.model.edges.is_empty());
}

#[test]
fn model_name_is_read_only_and_does_not_capture_tool_shortcuts() {
    let mut h = Harness::new();
    let name = h.app.doc.name.clone();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, &name));
    assert!(!h.ctx.egui_wants_keyboard_input());
    h.key(Key::L);
    assert_eq!(h.app.tool, Tool::Line);
    h.frame(
        vec![egui::Event::Text("Sketch".into())],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.doc.name, name);
    assert_eq!(h.app.tool, Tool::Line);
}

#[test]
fn closing_shortcuts_preserves_unfinished_drawing_and_blocks_file_commands() {
    let mut h = Harness::new();
    h.app.set_tool(Tool::Line);
    h.app.line_start = Some(DVec3::X);
    h.app.lock = Some(Axis::Y);
    h.app.length_input = "2m".into();
    h.key(Key::F1);
    assert!(h.app.help);
    h.frame(
        vec![egui::Event::Key {
            key: Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    assert!(h.app.project.confirm.is_none());
    assert_eq!(h.app.line_start, Some(DVec3::X));
    h.key(Key::Escape);
    assert!(!h.app.help);
    assert_eq!(h.app.line_start, Some(DVec3::X));
    assert_eq!(h.app.lock, Some(Axis::Y));
    assert_eq!(h.app.length_input, "2m");
}

#[test]
fn window_focus_loss_cancels_gesture_without_erasing_geometry() {
    let mut h = Harness::new();
    h.app.doc.model.add_edge(-DVec3::X, DVec3::X).unwrap();
    h.key(Key::E);
    let p = h.at(DVec3::ZERO);
    h.pointer(p, egui::PointerButton::Primary, true, false);
    h.frame(vec![egui::Event::PointerGone], egui::Modifiers::NONE);
    assert!(h.app.gesture.is_none());
    assert_eq!(h.app.doc.model.edges.len(), 1);
}

#[test]
fn menus_block_background_hover_and_restore_it_after_closing() {
    for context_menu in [true, false] {
        let mut h = Harness::new();
        let menu = if context_menu {
            let point = h.app.viewport.center();
            open_context_menu(&mut h, point)
        } else {
            h.click(egui::pos2(38., 169.));
            h.frame(vec![], egui::Modifiers::NONE)
        };
        let target = if context_menu {
            text_position(&menu, "Delete")
        } else {
            menu_text_position(&menu, Tool::Circle.label())
        };
        assert!(h.app.viewport.contains(target));
        let point = h
            .app
            .doc
            .camera
            .ray(dv(target - h.app.viewport.min), dv(h.app.viewport.size()))
            .plane_z(0.)
            .unwrap();
        let edge = h
            .app
            .doc
            .model
            .add_edge(point - DVec3::X, point + DVec3::X)
            .unwrap();
        h.app.picking_preparations = 0;
        h.frame(
            vec![egui::Event::PointerMoved(target)],
            egui::Modifiers::NONE,
        );
        assert_eq!(h.app.hover, None);
        assert_eq!(h.app.picking_preparations, 0);

        // Covering an existing highlight must also clear it without mouse motion.
        h.app.hover = Some(edge);
        h.app.flip_hover = Some(Axis::X);
        h.frame(vec![], egui::Modifiers::NONE);
        assert_eq!(h.app.hover, None);
        assert_eq!(h.app.flip_hover, None);

        h.key(Key::Escape);
        h.frame(vec![], egui::Modifiers::NONE);
        h.frame(
            vec![egui::Event::PointerMoved(target)],
            egui::Modifiers::NONE,
        );
        assert_eq!(h.app.hover, Some(edge));
    }
}

#[test]
fn view_sidebar_buttons_set_projection_and_all_standard_views_without_resizing() {
    let mut h = Harness::new();
    h.ctx.style_mut_of(h.ctx.theme(), |s| s.animation_time = 0.);
    h.size.y = 4000.;
    h.frame(vec![], egui::Modifiers::NONE);
    let width = h.app.viewport.right();
    for title in ["Camera", "Standard views"] {
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, title));
    }
    for (label, projection) in [
        ("Parallel", ketchup_core::Projection::Parallel),
        ("Perspective", ketchup_core::Projection::Perspective),
    ] {
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, label));
        assert_eq!(h.app.doc.camera.projection, projection);
    }
    let target = h.app.doc.camera.target;
    let distance = h.app.doc.camera.distance;
    for view in ketchup_core::StandardView::ALL {
        let output = h.frame(vec![], egui::Modifiers::NONE);
        h.click(text_position(&output, view.label()));
        assert_eq!(h.app.doc.camera.standard_view(), Some(view));
        assert_eq!(h.app.doc.camera.target, target);
        assert_eq!(h.app.doc.camera.distance, distance);
        assert!((h.app.viewport.right() - width).abs() < 0.1);
    }
    assert!(h.app.project.modified);
}

#[test]
fn generate_menu_opens_cutting_plan_scope_dialog_directly() {
    let mut h = Harness::new();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let edit = text_position(&output, "Edit");
    let generate = text_position(&output, "Generate");
    let help = text_position(&output, "Help");
    assert!(edit.x < generate.x && generate.x < help.x);
    h.click(generate);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Prepare cutting plan…"));
    assert_eq!(h.app.project.cutting_plan_selection, Some(false));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "Woodworking cutting plan");
    let output = h.click(text_position(&output, "Cancel"));
    assert!(h.app.project.cutting_plan_selection.is_none());
    assert!(h.app.project.pending.is_none());
    assert!(!h.app.project.modified);
    assert!(h.app.doc.history.undo_label().is_none());
    drop(output);
}

#[test]
fn generate_screenshot_dialog_supports_multiple_views_and_cancels_without_editing() {
    let mut h = Harness::new();
    let before = h.app.doc.clone();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Generate"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Screenshot"));
    assert!(h.app.screenshot.open);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    text_position(&output, "Generate screenshot");
    text_position(&output, "Preview viewport");
    assert!(h.app.screenshot.settings.views[1]);
    assert!(h.app.screenshot.settings.views[2]);
    assert!(!h.app.screenshot.settings.views[3]);
    h.click(text_position(&output, "Back"));
    assert!(h.app.screenshot.settings.views[3]);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "One image per view"));
    assert!(!h.app.screenshot.settings.combined);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Cancel"));
    assert!(!h.app.screenshot.open);
    assert!(h.app.screenshot.settings.views[3]);
    assert_eq!(h.app.doc, before);
    assert!(h.app.project.pending.is_none());
    assert!(h.app.doc.history.undo_label().is_none());
}

#[test]
fn system_information_opens_from_help_and_copies_the_report() {
    let mut h = Harness::new();
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Help"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(menu_text_position(&output, "System information"));
    let output = h.frame(vec![], egui::Modifiers::NONE);
    let copy = text_position(&output, "Copy to clipboard");
    let copied = h.click(copy);
    let report = copied
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text),
            _ => None,
        })
        .expect("copy button sends report to clipboard");
    assert!(report.contains(concat!("SketCraft ", env!("CARGO_PKG_VERSION"))));
    assert!(report.contains("Architecture:"));
    assert!(report.contains("Window:"));
    assert!(
        !report.contains(&h.app.doc.name),
        "diagnostics omit document names"
    );
}

#[test]
fn large_tool_set_shows_every_tool_without_flyouts_and_view_shortcuts() {
    let mut h = Harness::new();
    h.app.large_tool_set = true;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    for tool in Tool::ALL {
        let label = if tool == Tool::TapeMeasure { "Tape" } else { tool.label() };
        text_position(&output, label);
    }
    // Drawing tools are one click away instead of inside the Line flyout.
    h.click(text_position(&output, "Polygon"));
    assert_eq!(h.app.tool, Tool::Polygon);
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Rotate"));
    assert_eq!(h.app.tool, Tool::Rotate);

    let cuts = h.app.doc.display.section_cuts;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Sections"));
    assert_eq!(h.app.doc.display.section_cuts, !cuts);

    h.app.inspector = false;
    let output = h.frame(vec![], egui::Modifiers::NONE);
    h.click(text_position(&output, "Tags"));
    assert!(h.app.inspector, "Tags opens the sidebar");
}
