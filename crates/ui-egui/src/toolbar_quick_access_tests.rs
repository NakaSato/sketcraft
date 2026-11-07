use crate::scenario::{Harness, handle_at, rect_of};
use serde_json::json;

fn pick(h: &mut Harness, handle: &str, item: &str) {
    h.frames(3);
    h.step(&json!({"click":{"handle": handle},"button":"right"})).unwrap();
    h.frames(3);
    assert!(h.app.viewport.context_menu.is_some(), "{handle}: the right-click opens the command's menu");
    h.step(&json!({"widget": item})).unwrap_or_else(|e| panic!("{handle}: {item}: {e}"));
    h.frames(3);
}

/// The quick-access bar (#10223) is empty and not drawn by default; a ribbon button's menu
/// adds its command after Save, Undo and Redo, a drag along the bar reorders the buttons, a
/// click starts the command, and the button's own menu removes it. The list is kept with the
/// toolbar's saved customisations.
#[test]
fn ribbon_commands_are_added_reordered_started_and_removed_from_the_quick_access_bar() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    h.frames(3);
    assert!(h.app.toolbar_custom.quick_access.is_empty(), "empty by default");
    assert!(rect_of("appbar:quick_access").is_none(), "and not drawn");

    pick(&mut h, "button:solid.extrude", "Add to Quick Access");
    pick(&mut h, "button:solid.fillet", "Add to Quick Access");
    assert_eq!(h.app.toolbar_custom.quick_access, ["solid.extrude", "solid.fillet"]);
    h.frames(2);
    let bar = rect_of("appbar:quick_access").unwrap();
    let redo = rect_of("appbar:redo").unwrap();
    assert!(bar.left() >= redo.right() && bar.center().y == redo.center().y, "after Redo: {bar:?} {redo:?}");
    let extrude = handle_at("appbar:quick:solid.extrude").unwrap();
    let fillet = handle_at("appbar:quick:solid.fillet").unwrap();
    assert!(extrude.x < fillet.x, "in the order they were added");

    // Dragged before Extrude, Fillet comes first.
    h.step(&json!({"drag": [[fillet.x, fillet.y], [extrude.x - 10.0, extrude.y]]})).unwrap();
    h.frames(3);
    assert_eq!(h.app.toolbar_custom.quick_access, ["solid.fillet", "solid.extrude"]);

    // Saved with the toolbar and read back.
    let saved = h.app.toolbar_custom.prefs();
    let mut reloaded = crate::toolbar_custom::ToolbarCustom::default();
    reloaded.load(&saved);
    assert_eq!(reloaded.quick_access, ["solid.fillet", "solid.extrude"]);
    reloaded.load(&json!({"version": 3, "quick_access": ["solid.extrude", "solid.extrude", 7, "", "no.such.command"]}));
    assert_eq!(reloaded.quick_access, ["solid.extrude", "no.such.command"], "each once; unknown ids kept but not drawn");

    // A click starts the command as its ribbon button does.
    h.step(&json!({"click":{"handle":"appbar:quick:solid.extrude"}})).unwrap();
    h.frames(2);
    assert_eq!(
        h.app.tool.as_ref().map(|t| t.cmd.to_string()).or_else(|| h.app.last_command.as_ref().map(|(id, _)| id.clone())).as_deref(),
        Some("solid.extrude")
    );
    h.step(&json!({"key":"Escape"})).unwrap();
    h.frames(2);

    // The button's own menu takes it off; the last one gone, the bar goes too.
    pick(&mut h, "appbar:quick:solid.extrude", "Remove from Quick Access");
    assert_eq!(h.app.toolbar_custom.quick_access, ["solid.fillet"]);
    pick(&mut h, "button:solid.fillet", "Remove from Quick Access");
    assert!(h.app.toolbar_custom.quick_access.is_empty());
    h.frames(2);
    assert!(rect_of("appbar:quick_access").is_none());
    assert!(h.app.session.doc.features.is_empty(), "nothing in the design changed");
}

/// A quick-access click starts exactly the pinned variant and leaves the family's remembered
/// ribbon choice alone.
#[test]
fn a_quick_access_click_on_a_family_variant_keeps_the_remembered_ribbon_choice() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.settle();
    h.app.start("sketch.rectangle.three_point");
    h.settle();
    h.step(&json!({"key":"Escape"})).unwrap();
    h.frames(2);
    assert_eq!(h.app.preferences.sketch_variants.resolve("sketch.rectangle.two_point"), "sketch.rectangle.three_point");

    h.app.toolbar_custom.quick_access = vec!["sketch.rectangle.center".to_string()];
    h.frames(3);
    h.step(&json!({"click":{"handle":"appbar:quick:sketch.rectangle.center"}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd.to_string()).as_deref(), Some("sketch.rectangle.center"));
    assert_eq!(
        h.app.preferences.sketch_variants.resolve("sketch.rectangle.two_point"),
        "sketch.rectangle.three_point",
        "the ribbon's Rectangle button still starts its remembered choice"
    );
}

#[test]
fn sketchup_large_toolset_can_be_loaded_and_cleared() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    h.frames(3);
    assert!(h.app.toolbar_custom.quick_access.is_empty());

    // Load SketchUp Large Tool Set
    crate::toolbar_custom::load_sketchup_toolset(&mut h.app);
    assert_eq!(h.app.toolbar_custom.quick_access.len(), crate::toolbar_custom::SKETCHUP_LARGE_TOOLSET.len());
    for id in crate::toolbar_custom::SKETCHUP_LARGE_TOOLSET {
        assert!(h.app.toolbar_custom.quick_access.contains(&id.to_string()), "contains {id}");
        assert!(sketcraft_engine::find_command(id).is_some(), "command {id} exists");
    }

    // Test clear
    crate::toolbar_custom::clear_quick_access(&mut h.app);
    assert!(h.app.toolbar_custom.quick_access.is_empty());
}
