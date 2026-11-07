use super::*;

/// Home hides the design: its Undo and Redo buttons, the File menu's Insert, Save
/// and Export, and CAD keyboard shortcuts leave it alone. File-view Save shortcuts
/// have their own regression coverage; viewport actions work again after returning.
#[test]
fn design_actions_are_off_on_home() {
    // A design with history both ways, a selection and unsaved changes, behind Home.
    let on_home = || {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Main"})).unwrap();
        h.app.run("solid.box", json!({"length": 5, "width": 5, "height": 5, "corner": [20, 0, 0]})).unwrap();
        h.app.run("edit.undo", json!({})).unwrap();
        h.app.session.selection = vec![sketcraft_engine::Sel::Body { name: "Main".into() }];
        let rev = h.app.session.revision;
        h.app.home.show(rev);
        h.frames(2);
        h
    };
    let mut h = on_home();
    assert!(!design_shown(&h.app));
    // Everything an action could change: the design, its history and file, files written,
    // the selection and visibility, and whether it left Home or opened a tool, dialog or the command box.
    let state = |h: &crate::scenario::Harness| {
        let files = std::fs::read_dir(&h.dir).map_or(0, |d| d.count());
        let doc = serde_json::to_value(&*h.app.session.doc).unwrap();
        let busy = (h.app.tool.is_some(), h.app.dialog.is_some(), h.app.ui.palette_open);
        (
            doc,
            h.app.session.undo.len(),
            h.app.session.redo.len(),
            h.app.session.path.clone(),
            files,
            h.app.session.selection.clone(),
            h.app.session.visibility.hidden_bodies.clone(),
            h.app.home.open,
            busy,
        )
    };
    let click = |h: &mut crate::scenario::Harness, handle: &str| {
        let at = crate::scenario::handle_at(handle).unwrap();
        h.call("ui.click", json!({"x": at.x, "y": at.y}));
    };
    let mut actions: Vec<(String, Box<dyn Fn(&mut crate::scenario::Harness)>)> = Vec::new();
    for icon in ["undo", "redo"] {
        actions.push((format!("{icon} button"), Box::new(move |h| click(h, &format!("appbar:{icon}")))));
    }
    for (key, cmd) in [("Z", true), ("Y", true), ("Delete", false), ("V", false), ("E", false), ("S", false)] {
        let name = if cmd { format!("Ctrl+{key}") } else { key.to_string() };
        actions.push((name, Box::new(move |h| drop(h.call("ui.key", json!({"key": key, "cmd": cmd}))))));
    }
    for item in ["Insert STEP…", "Insert Mesh…", "Save", "Save As…", "Export STL…", "Export STEP…", "Export 3MF…", "Export OBJ…", "Export URDF…"]
    {
        actions.push((
            format!("File › {item}"),
            Box::new(move |h| {
                h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("sc_file_menu"), false));
                click(h, "appbar:folder");
                // A same-named File navigation action is now present: target the menu row explicitly.
                if let Some(at) = crate::scenario::handle_at(&format!("menu:{item}")) {
                    h.call("ui.click", json!({"x":at.x,"y":at.y}));
                }
            }),
        ));
    }
    // Each action on its own design, so one cannot hide another.
    let mut acted = Vec::new();
    for (name, act) in &actions {
        let mut h = on_home();
        let before = state(&h);
        act(&mut h);
        h.settle();
        if state(&h) != before {
            acted.push(name.clone());
        }
    }
    assert!(acted.is_empty(), "these acted on the hidden design from Home: {acted:?}");

    h.app.home.open = false;
    h.frames(2);
    let at = crate::scenario::handle_at("appbar:redo").unwrap();
    h.call("ui.click", json!({"x": at.x, "y": at.y}));
    assert_eq!(h.app.session.redo.len(), 0, "Redo works once Home is closed");
    h.call("ui.key", json!({"key": "Z", "cmd": true}));
    h.settle();
    assert_eq!(h.app.session.redo.len(), 1, "Ctrl+Z works once Home is closed");
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("sc_file_menu"), false));
    click(&mut h, "appbar:file");
    h.step(&json!({"widget": "Save"})).unwrap();
    h.settle();
    assert!(h.app.session.path.is_some() && !h.app.session.is_dirty(), "the design saves once Home is closed");
}

/// File › Export URDF… writes the robot description and its meshes next to the picked file.
#[test]
fn file_menu_exports_a_urdf() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("solid.box", json!({"length": 10, "width": 10, "height": 10})).unwrap();
    h.frames(2);
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("sc_file_menu"), false));
    let at = crate::scenario::handle_at("appbar:folder").unwrap();
    h.call("ui.click", json!({"x": at.x, "y": at.y}));
    let _ = h.step(&json!({"widget": "Export URDF…"}));
    h.settle();
    let files: Vec<String> = std::fs::read_dir(&h.dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(files.iter().any(|f| f.ends_with(".urdf")) && files.iter().any(|f| f.ends_with("-base.stl")), "{files:?}");
}

/// The disabled buttons' tooltip is translated in every catalog (not English fallback).
#[test]
fn needs_design_tooltip_is_translated() {
    let english = crate::i18n::Locale::for_language("en").text("ui.needs_design");
    assert!(english.contains("{command}"));
    for catalog in crate::i18n::LOCALES.iter().filter(|c| !matches!(c.id, "en" | "en-GB")) {
        let text = crate::i18n::Locale::for_language(catalog.id).text("ui.needs_design");
        assert!(text != english && text.contains("{command}"), "{} must translate ui.needs_design", catalog.id);
    }
}
