use crate::scenario::{Harness, rect_of};
use serde_json::json;

/// A second click on the button that opened a drop-down closes it (#1522): the workspace
/// switcher's menu, and a panel's command list.
#[test]
fn a_second_click_on_its_button_closes_a_drop_down() {
    let mut h = Harness::new();
    h.frames(3);
    let ws = rect_of("workspace_switcher").unwrap().center();
    h.step(&json!({"click": [ws.x, ws.y]})).unwrap();
    assert!(h.app.viewport.context_menu.is_some(), "the first click opens the workspace menu");
    h.step(&json!({"click": [ws.x, ws.y]})).unwrap();
    h.frames(2);
    assert!(h.app.viewport.context_menu.is_none(), "the second click closes it instead of reopening it");
    h.step(&json!({"click": [ws.x, ws.y]})).unwrap();
    assert!(h.app.viewport.context_menu.is_some(), "and a third opens it again");
    h.step(&json!({"key": "Escape"})).unwrap();

    h.step(&json!({"click": {"handle": "panel:MODEL:CREATE"}})).unwrap();
    h.frames(2);
    assert!(rect_of("toolbar_dropdown").is_some(), "the first click opens the panel's list");
    h.step(&json!({"click": {"handle": "panel:MODEL:CREATE"}})).unwrap();
    h.frames(2);
    assert!(rect_of("toolbar_dropdown").is_none(), "the second click closes it");
}

#[test]
fn ribbon_panel_menus_follow_hover_only_after_click_and_only_when_enabled() {
    for follow_hover in [false, true] {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1000}));
        let prefs = h.call("ui.prefs", json!({"ribbon_menus_follow_hover":follow_hover}));
        assert_eq!(prefs["result"]["ribbon_menus_follow_hover"], follow_hover);
        let create = "panel:MODEL:CREATE";
        let modify = "panel:MODEL:MODIFY";
        let move_to = |h: &mut Harness, handle| {
            h.step(&json!({"move":{"handle":handle}})).unwrap();
            h.frames(3);
        };
        move_to(&mut h, create);
        assert!(rect_of("toolbar_dropdown").is_none(), "hover alone never opens a menu");
        h.step(&json!({"click":{"handle":create}})).unwrap();
        h.frames(3);
        let create_menu = rect_of("toolbar_dropdown").expect("Create menu");
        move_to(&mut h, "dropdown:solid.box");
        assert_eq!(rect_of("toolbar_dropdown"), Some(create_menu), "moving into the menu keeps it open");

        move_to(&mut h, modify);
        if follow_hover {
            let modify_menu = rect_of("toolbar_dropdown").expect("Modify menu");
            assert!(modify_menu.left() > create_menu.left(), "hover switches to the panel on the right");
            move_to(&mut h, create);
            assert_eq!(rect_of("toolbar_dropdown"), Some(create_menu), "hover also switches to the left");
        } else {
            assert_eq!(rect_of("toolbar_dropdown"), Some(create_menu), "click-only mode ignores sibling hover");
        }
        let outside = h.app.viewport.rect.unwrap().right_bottom() - egui::vec2(40.0, 60.0);
        h.step(&json!({"move":[outside.x,outside.y]})).unwrap();
        h.frames(3);
        assert_eq!(rect_of("toolbar_dropdown"), (!follow_hover).then_some(create_menu), "only hover mode closes on leaving");
        if follow_hover {
            move_to(&mut h, modify);
            assert!(rect_of("toolbar_dropdown").is_none(), "returning to the ribbon still needs a click");
        }
    }
}

/// A click on the lit button of the sketch tool in use ends it, so clicks in the view select
/// again and Ctrl+click adds lines to the selection (#10151); a constraint in the ribbon's
/// grid ends the same way. With the preference off, the click starts the tool again.
#[test]
fn a_second_click_on_a_running_tools_button_ends_it() {
    use sketcraft_engine::Sel;
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.set_origin_shown(false);
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0":[-20,-20],"p1":[20,20]})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0":[-10,-10],"p1":[10,10]})).unwrap();
    h.app.cam.distance = 100.0;
    h.settle();
    let grid = format!("ribbon:constraint:{}", crate::workspace::CONSTRAINT_GALLERY[0]);
    for handle in ["button:sketch.line", "button:sketch.dimension", grid.as_str()] {
        h.step(&json!({"click": {"handle": handle}})).unwrap();
        let running = h.app.tool.as_ref().map(|t| t.cmd);
        assert!(running.is_some(), "{handle}: the first click starts the tool");
        h.frames(30);
        h.step(&json!({"click": {"handle": handle}})).unwrap();
        assert!(h.app.tool.is_none(), "{handle}: the second click ends {running:?}");
        h.frames(30);
    }
    // With no tool running, clicks select, and Ctrl+click adds lines of both rectangles.
    for (i, p) in [[-15.0, 20.0], [20.0, 15.0], [-5.0, 10.0], [10.0, 5.0]].iter().enumerate() {
        let at = h.call("ui.at", json!({"sketch": p}))["result"].clone();
        h.step(&json!({"click": at, "ctrl": i > 0})).unwrap();
    }
    let curves = h.app.session.selection.iter().filter(|s| matches!(s, Sel::SketchCurve { .. })).count();
    assert_eq!(curves, 4, "{:?}", h.app.session.selection);
    // The old way, behind the preference: the click starts the tool again.
    h.app.preferences.ribbon_click_ends_tool = false;
    h.step(&json!({"click": {"handle": "button:sketch.line"}})).unwrap();
    h.frames(30);
    h.step(&json!({"click": {"handle": "button:sketch.line"}})).unwrap();
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.line"));
}

/// A tool pinned into the Finish Sketch group, the one at the ribbon's right edge, shows
/// pressed while it runs like any other tool button; only Finish and Cancel never do, not
/// even under a press.
#[test]
fn a_tool_pinned_into_the_finish_group_shows_pressed_while_it_runs() {
    fn flat(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| flat(s, out)),
            s => out.push(s.clone()),
        }
    }
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    crate::toolbar_custom::reorder(&mut h.app, "SKETCH", "sketch.ellipse", "FINISH SKETCH", None);
    h.settle();
    h.step(&json!({"click": {"handle": "button:sketch.ellipse"}})).unwrap();
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.ellipse"), "the click starts the tool");
    h.frames(3);
    let soft = crate::theme::TabAccent::for_tab(&crate::theme::Tokens::get(), "SKETCH").soft;
    let pressed = |h: &Harness, id: &str| {
        let r = rect_of(&format!("button:{id}")).unwrap();
        let mut all = Vec::new();
        h.shapes.iter().for_each(|s| flat(&s.shape, &mut all));
        all.iter().any(|s| matches!(s, egui::Shape::Rect(s) if s.rect == r && s.fill == soft))
    };
    assert!(pressed(&h, "sketch.ellipse"), "the running tool's button is pressed");
    // A press lights any other button while the pointer is down; Finish, Cancel and Finish
    // with AutoConstrain (a button only once pinned) stay plain. None runs as a tool or
    // leaves a dialog, so a press is how they would light.
    for exit in ["sketch.finish", "sketch.cancel", "sketch.finish_auto_constrain"] {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        crate::toolbar_custom::pin(&mut h.app, "SKETCH", exit).unwrap();
        h.settle();
        let at = crate::scenario::handle_at(&format!("button:{exit}")).unwrap();
        h.frame_with(crate::test_support::pointer(at, true));
        assert!(!pressed(&h, exit), "a press on {exit} does not show it pressed");
    }
}

/// Ribbon Tabs Only folds the ribbon to its tab row (#10112) in both layouts: a tab's click
/// opens its panels over the view, a command starting or the pointer leaving closes them, and
/// the command again keeps the whole ribbon shown.
#[test]
fn ribbon_tabs_only_folds_to_the_tab_row_and_a_tab_opens_over_the_view() {
    for layout in ["compact", "floating"] {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.app.run("view.layout", json!({ "mode": layout })).unwrap();
        h.frames(3);
        assert!(!h.app.preferences.ribbon_tabs_only, "off by default");
        let whole = rect_of("toolbar").unwrap();
        let whole_view = h.app.viewport.rect.unwrap();
        assert!(rect_of("ribbon:folded").is_none());

        h.app.start("view.ribbon_tabs_only");
        h.frames(3);
        assert!(h.app.preferences.ribbon_tabs_only, "{layout}: the command folds the ribbon");
        let folded = rect_of("ribbon:folded").unwrap();
        assert!(folded.height() <= super::TAB_ROW_HEIGHT + 1.0, "{layout}: only the tab row is left: {folded:?}");
        assert!(rect_of("panel:MODEL:CREATE").is_none(), "{layout}: no panels while folded");
        if layout == "compact" {
            assert!(h.app.viewport.rect.unwrap().top() < whole_view.top() - 50.0, "{layout}: the view takes the panels' room");
        }

        // A tab's click opens its panels over the view, as tall as the whole ribbon.
        let tab = rect_of("toolbar_tab:SURFACE").unwrap().center();
        h.step(&json!({"click": [tab.x, tab.y]})).unwrap();
        h.frames(2);
        assert_eq!(h.app.ui.tab, "SURFACE");
        assert!(h.app.ui.ribbon_open, "{layout}: the tab's panels open");
        let open = rect_of("ribbon:open").unwrap();
        assert!((open.height() - whole.height()).abs() <= 2.0 * crate::shell::SHELL_GAP + 1.0, "{layout}: {open:?} vs {whole:?}");
        assert!(rect_of("panel:SURFACE:CREATE").is_some(), "{layout}: the panels are drawn");
        if layout == "compact" {
            let top = h.app.viewport.rect.unwrap().top();
            assert!((top - folded.bottom()).abs() <= 2.0, "{layout}: over the view, not pushing it down: {top} {folded:?}");
        }

        // The pointer leaving the panels closes them.
        let away = h.app.viewport.rect.unwrap().center() + egui::vec2(0.0, 150.0);
        h.step(&json!({"move": [away.x, away.y]})).unwrap();
        h.frames(2);
        assert!(!h.app.ui.ribbon_open, "{layout}: leaving closes the panels");
        assert!(rect_of("ribbon:open").is_none());

        // A command started from them closes them too.
        h.step(&json!({"click": [tab.x, tab.y]})).unwrap();
        h.frames(2);
        assert!(h.app.ui.ribbon_open);
        h.app.start("view.grid");
        h.frames(2);
        assert!(!h.app.ui.ribbon_open, "{layout}: a command closes the panels");

        // The command again keeps the whole ribbon shown.
        h.app.ui.tab = "SURFACE".into();
        h.app.start("view.ribbon_tabs_only");
        h.frames(3);
        assert!(!h.app.preferences.ribbon_tabs_only);
        assert_eq!(h.app.ui.tab, "SURFACE", "{layout}: the ribbon keeps its tab");
        assert!(rect_of("ribbon:folded").is_none());
        assert_eq!(rect_of("toolbar").unwrap().height(), whole.height(), "{layout}");
        assert!(h.app.run("view.ribbon_tabs_only", json!({"on": true})).is_ok());
        assert!(h.app.preferences.ribbon_tabs_only, "{layout}: `on` sets it");
        assert!(h.app.session.undo.is_empty(), "{layout}: a window setting, not a design change");
    }
}

/// Switching tabs with a panel's drop-down open forgets that drop-down, so the pointer leaving
/// the overlay still closes it (#10112).
#[test]
fn ribbon_tabs_only_closes_after_a_tab_switch_with_a_panel_menu_open() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.frames(3);
    h.app.start("view.ribbon_tabs_only");
    h.frames(3);
    let tab = rect_of("toolbar_tab:MODEL").unwrap().center();
    h.step(&json!({"click": [tab.x, tab.y]})).unwrap();
    h.frames(2);
    h.step(&json!({"click": {"handle": "panel:MODEL:MODIFY"}})).unwrap();
    h.frames(2);
    let key = egui::Id::unique("sc_panel_open");
    assert!(h.app.ui.ribbon_open);
    assert!(h.ctx.data(|d| d.get_temp::<egui::Id>(key)).is_some(), "the panel menu is open");
    let other = rect_of("toolbar_tab:SURFACE").unwrap().center();
    h.step(&json!({"click": [other.x, other.y]})).unwrap();
    h.frames(2);
    assert_eq!(h.app.ui.tab, "SURFACE");
    assert!(h.app.ui.ribbon_open, "the other tab's panels are shown");
    assert!(h.ctx.data(|d| d.get_temp::<egui::Id>(key)).is_none(), "the old menu is forgotten");
    let away = h.app.viewport.rect.unwrap().center() + egui::vec2(0.0, 150.0);
    h.step(&json!({"move": [away.x, away.y]})).unwrap();
    h.frames(2);
    assert!(!h.app.ui.ribbon_open, "leaving still closes the overlay");
}

/// A drop-down row's help tooltip names the key the command runs from now, not its default.
#[test]
fn a_drop_down_row_tooltip_names_a_rebound_key() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.frames(3);
    crate::keymap::bind(&mut h.app, "solid.shell", "Shift+J", false).unwrap();
    h.step(&json!({"click": {"handle": "panel:MODEL:MODIFY"}})).unwrap();
    h.frames(2);
    h.step(&json!({"move": {"handle": "dropdown:solid.shell"}})).unwrap();
    h.frames(40);
    let shell = h.app.tr_label("Shell").to_string();
    assert!(h.widget(&format!("{shell}  (Shift+J)")).is_some(), "the tooltip shows the new key");
}
