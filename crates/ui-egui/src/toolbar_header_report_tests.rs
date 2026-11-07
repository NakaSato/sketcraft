use super::*;

#[test]
fn escape_closes_file_menu_on_home_and_over_a_sketch_without_cancelling_it() {
    for home in [true, false] {
        let mut h = crate::scenario::Harness::new();
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        h.app.run("sketch.line", json!({"points":[[0,0],[10,0]]})).unwrap();
        h.app.start("sketch.line");
        h.app.home.open = home;
        h.frames(3);
        let sketch = h.app.session.active_sketch;
        let before = h.app.session.doc.to_json();
        let menu_open = |h: &crate::scenario::Harness| h.ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("sc_file_menu")).unwrap_or(false));
        h.step(&json!({"click":{"handle":"appbar:file"}})).unwrap();
        assert!(menu_open(&h));
        h.app.last_escape = Some(h.ctx.input(|i| i.time));
        h.step(&json!({"key":"Escape"})).unwrap();
        assert!(!menu_open(&h), "Escape closes File on Home and in the design");
        assert_eq!(h.app.home.open, home);
        assert_eq!(h.app.session.active_sketch, sketch);
        assert!(h.app.tool.is_some(), "closing File must not cancel the sketch tool");
        assert_eq!(h.app.session.doc.to_json(), before);
        assert!(h.app.last_escape.is_none(), "closing File must not arm Finish Sketch");
    }
}

#[test]
fn escape_closes_file_menu_before_parameters_and_preserves_selection() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("parameters.change", json!({"name":"width","expression":"10 mm"})).unwrap();
    h.app.session.selection = vec![sketcraft_engine::Sel::Plane { name: "XY".into() }];
    h.step(&json!({"start":"parameters.change"})).unwrap();
    let before = (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.selection.clone());
    h.step(&json!({"click":{"handle":"appbar:file"}})).unwrap();
    assert!(h.ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("sc_file_menu")).unwrap_or(false)));
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(!h.ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("sc_file_menu")).unwrap_or(false)));
    assert!(h.app.params.is_some(), "the first Escape belongs to File");
    assert_eq!(before, (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.selection.clone()));
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(h.app.params.is_none(), "the next Escape still closes Parameters");
    assert_eq!(before, (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.selection.clone()));
}

#[test]
fn native_file_menu_preserves_application_bar_controls() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    for native in [false, true] {
        h.app.native_file_menu = native;
        h.app.integrated_titlebar = native;
        h.frames(3);
        for name in [
            "appbar:file",
            "appbar:save",
            "appbar:undo",
            "appbar:redo",
            "appbar:help",
            "titlebar:preferences",
            "titlebar:assistant",
            "titlebar:report",
        ] {
            assert!(crate::scenario::handle_at(name).is_some(), "native={native}: {name}");
        }
        let tabs = crate::scenario::rect_of("tab:design:0").unwrap();
        let assistant = crate::scenario::rect_of("titlebar:assistant").unwrap();
        assert!(tabs.right() < assistant.left(), "tabs leave room for the right controls");
        let depth = h.app.session.undo.len();
        h.step(&json!({"click":{"handle":"appbar:undo"}})).unwrap();
        assert_eq!(h.app.session.undo.len(), depth - 1);
        h.step(&json!({"click":{"handle":"appbar:redo"}})).unwrap();
        assert_eq!(h.app.session.undo.len(), depth);
        h.step(&json!({"click":{"handle":"appbar:file"}})).unwrap();
        assert!(h.widget("Save As JSON…").is_some(), "the in-window File menu still opens");
        h.step(&json!({"click":{"handle":"appbar:file"}})).unwrap();
        assert!(!h.ctx.data(|data| data.get_temp::<bool>(egui::Id::unique("sc_file_menu")).unwrap_or(false)));
        h.step(&json!({"click":{"handle":"appbar:help"}})).unwrap();
        assert!(h.app.help.menu);
        h.step(&json!({"key":"Escape"})).unwrap();
        h.step(&json!({"click":{"handle":"titlebar:assistant"}})).unwrap();
        assert!(h.app.agent_chat.open);
        h.step(&json!({"click":{"handle":"titlebar:assistant"}})).unwrap();
        assert!(!h.app.agent_chat.open);
    }
}

#[test]
fn native_menu_actions_share_undo_and_redo_history() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    assert!(!FileAction::Undo.enabled(&h.app));
    assert!(!FileAction::Redo.enabled(&h.app));
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    let depth = h.app.session.undo.len();
    assert!(FileAction::Undo.enabled(&h.app));
    FileAction::Undo.run(&mut h.app);
    assert_eq!(h.app.session.undo.len(), depth - 1);
    assert!(FileAction::Redo.enabled(&h.app));
    FileAction::Redo.run(&mut h.app);
    assert_eq!(h.app.session.undo.len(), depth);
    assert!(!FileAction::Redo.enabled(&h.app));
}

#[test]
fn native_menu_actions_share_settings_and_unsaved_quit() {
    let mut h = crate::scenario::Harness::new();
    FileAction::Settings.run(&mut h.app);
    assert!(h.app.prefs_window.open);
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    FileAction::Quit.run(&mut h.app);
    assert!(!h.app.quit_requested);
    assert!(h.app.docs.quitting.is_some());
}

#[test]
fn native_menu_actions_disable_save_on_home() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = true;
    assert!(!FileAction::Save.enabled(&h.app));
    assert!(!FileAction::SaveAs.enabled(&h.app));
    assert!(FileAction::New.enabled(&h.app));
    assert!(FileAction::Open.enabled(&h.app));
    assert!(FileAction::Settings.enabled(&h.app));
    assert!(!FileAction::Export("step").enabled(&h.app));
    assert!(!FileAction::ExportMesh("stl").enabled(&h.app));
    assert!(!FileAction::Command("document.metadata").enabled(&h.app));
    h.app.home.open = false;
    assert!(FileAction::Save.enabled(&h.app));
    assert!(FileAction::Export("step").enabled(&h.app));
}

/// File › Export STEP…, STL…, 3MF…, OBJ… and URDF… each run a command of their own, so each
/// can be given a key in Keyboard Shortcuts: the key asks where to save the file as the menu
/// item does (STL and 3MF open Save As Mesh).
#[test]
fn each_export_format_has_a_key_that_starts_it() {
    let mut h = crate::scenario::Harness::new();
    let offered = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(String, Vec<String>)>::new()));
    let (seen, dir) = (offered.clone(), h.dir.clone());
    h.app.services.pick_save = Some(Box::new(move |name, extensions| {
        seen.borrow_mut().push((name.to_string(), extensions.iter().map(|e| e.to_string()).collect()));
        Box::pin(std::future::ready(Some(dir.join(name).to_string_lossy().into_owned())))
    }));
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    h.app.session.doc_mut().name = "Bracket".into();
    for (id, key) in [("file.export_step", "Alt+Shift+F7"), ("file.export_stl", "Alt+Shift+F8")] {
        crate::keymap::bind(&mut h.app, id, key, false).unwrap();
        assert_eq!(crate::keymap::effective(&h.app, id).as_deref(), Some(key), "the File menu shows it");
    }
    crate::keymap::press(&mut h.app, "Alt+Shift+F7", false);
    h.frames(3);
    h.settle();
    assert_eq!(offered.borrow().last().cloned(), Some(("Bracket.step".to_string(), vec!["step".to_string()])));
    let step = std::fs::read(h.dir.join("Bracket.step")).unwrap();
    assert!(step.starts_with(b"ISO-10303-21;"), "a STEP file is written");

    crate::keymap::press(&mut h.app, "Alt+Shift+F8", false);
    assert_eq!(h.app.dialog.as_ref().and_then(|d| d.command()), Some("file.save_mesh"), "STL opens Save As Mesh");
    crate::dialogs::cancel(&mut h.app);
}

/// New designs are saved as `.sc`. A design opened from a `.solidcraft` file is saved back to
/// that file by Save (no rename behind the user's back); Save As offers `.sc` and leaves it.
#[test]
fn save_offers_sc_and_keeps_an_opened_solidcraft_file() {
    let mut h = crate::scenario::Harness::new();
    let offered = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(String, Vec<String>)>::new()));
    let (seen, dir) = (offered.clone(), h.dir.clone());
    h.app.services.pick_save = Some(Box::new(move |name, extensions| {
        seen.borrow_mut().push((name.to_string(), extensions.iter().map(|e| e.to_string()).collect()));
        let path = dir.join(name).to_string_lossy().into_owned();
        Box::pin(std::future::ready(Some(path)))
    }));
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    h.app.session.doc_mut().name = "Bracket".into();
    FileAction::Save.run(&mut h.app);
    h.frames(3);
    h.settle();
    assert_eq!(offered.borrow().last().map(|(n, e)| (n.as_str(), e.clone())), Some(("Bracket.sc", vec!["sc".to_string(), "json".to_string()])));
    assert!(h.dir.join("Bracket.sc").is_file(), "a new design is saved as .sc");

    // A design from an earlier release.
    let old = h.dir.join("Old.solidcraft");
    std::fs::rename(h.dir.join("Bracket.sc"), &old).unwrap();
    h.app.run("doc.open", json!({"path": old.to_string_lossy()})).unwrap();
    h.app.run("solid.box", json!({"length":1,"width":1,"height":1})).unwrap();
    let asked = offered.borrow().len();
    FileAction::Save.run(&mut h.app);
    h.frames(3);
    h.settle();
    assert_eq!(offered.borrow().len(), asked, "Save writes the opened file without asking");
    assert_eq!(h.app.session.path.as_deref(), Some(old.to_string_lossy().as_ref()));
    assert!(!h.app.session.is_dirty());
    assert_eq!(sketcraft_engine::io::read_design(&std::fs::read(&old).unwrap()).unwrap().features.len(), 2);
    assert!(!h.dir.join("Old.sc").exists() && !h.dir.join("Bracket.sc").exists(), "nothing renamed");

    FileAction::SaveAs.run(&mut h.app);
    h.frames(3);
    h.settle();
    let name = offered.borrow().last().map(|(n, _)| n.clone()).unwrap();
    assert!(name.ends_with(".sc"), "Save As offers .sc: {name}");
    assert!(h.dir.join(&name).is_file() && old.is_file(), "Save As leaves the .solidcraft file as it was");
}

/// #8329: Save As (and Save a Copy) offer the name of the file the design came from, not the
/// name stored in the design, which a copy of the file still carries; a design renamed since
/// it was opened is offered its new name.
#[test]
fn save_as_offers_the_open_files_own_name() {
    let mut h = crate::scenario::Harness::new();
    let offered = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let (seen, dir) = (offered.clone(), h.dir.clone());
    h.app.services.pick_save = Some(Box::new(move |name, _| {
        seen.borrow_mut().push(name.to_string());
        Box::pin(std::future::ready(Some(dir.join(name).to_string_lossy().into_owned())))
    }));
    let ask = |h: &mut crate::scenario::Harness, how: fn(&mut SolveApp)| {
        how(&mut h.app);
        h.frames(3);
        h.settle();
        offered.borrow().last().cloned().unwrap_or_default()
    };
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    h.app.session.doc_mut().name = "Sample Plate".into();
    assert_eq!(ask(&mut h, save_as), "Sample Plate.sc", "a new design is offered its own name");
    // The same design under another file name, as copies of a file are (the report's
    // "Sample Plate-bug.solidcraft"): it is still named "Sample Plate" inside.
    let copy = h.dir.join("Sample Plate-bug.solidcraft");
    std::fs::copy(h.dir.join("Sample Plate.sc"), &copy).unwrap();
    h.app.run("doc.open", json!({"path": copy.to_string_lossy()})).unwrap();
    assert_eq!(h.app.session.doc.name, "Sample Plate");
    assert_eq!(ask(&mut h, save_as), "Sample Plate-bug.sc");
    // Saved under that name, the file is the one Save As offers next time.
    assert_eq!(h.app.session.path.as_deref(), Some(h.dir.join("Sample Plate-bug.sc").to_string_lossy().as_ref()));
    assert_eq!(ask(&mut h, save_copy), "Sample Plate-bug copy.sc");
    h.app.run("doc.open", json!({"path": copy.to_string_lossy()})).unwrap();
    // Renamed since it was opened: the new name.
    h.app.run("component.rename", json!({"component": 0, "name": "Plate Mk2"})).unwrap();
    assert_eq!(ask(&mut h, save_as), "Plate Mk2.sc");
}

#[test]
fn native_menu_actions_are_disabled_behind_an_open_issue_report() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    h.app.home.recent = vec!["elsewhere.solidcraft".into()];
    h.app.issue_report.draft = Some(crate::issue_report::draft_for(&h.app, None, None));
    let before = (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.docs.count(), h.app.home.recent.clone());
    for action in [
        FileAction::Undo,
        FileAction::New,
        FileAction::Open,
        FileAction::OpenRecent("elsewhere.solidcraft".into()),
        FileAction::ClearRecent,
        FileAction::Command("file.insert_step"),
        FileAction::Save,
        FileAction::SaveAs,
        FileAction::SaveJson,
        FileAction::ExportMesh("stl"),
        FileAction::Export("step"),
        FileAction::Export("json"),
        FileAction::Command("document.metadata"),
        FileAction::Settings,
        FileAction::Shortcuts,
        FileAction::AgentChat,
    ] {
        assert!(!action.enabled(&h.app), "{action:?} behind the report");
        action.run(&mut h.app);
    }
    assert_eq!(before, (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.docs.count(), h.app.home.recent.clone()));
    assert!(h.app.dialog.is_none() && h.app.file_picker.is_none() && !h.app.prefs_window.open && !h.app.keymap.open && !h.app.agent_chat.open);
    // A focused report field still gets the menu's Undo and Redo.
    assert!(FileAction::Undo.menu_enabled(&h.app, true) && !FileAction::Undo.menu_enabled(&h.app, false));
    assert!(!FileAction::Save.menu_enabled(&h.app, true));
    // Quit waits as behind any modal window: it would switch to a design with unsaved changes
    // under the report.
    assert!(!FileAction::Quit.enabled(&h.app));
    FileAction::Quit.run(&mut h.app);
    assert!(h.app.docs.quitting.is_none() && h.app.docs.closing.is_none());
    // Help's windows wait too; Report an Issue… shows the report.
    for item in ["about", "shortcuts", "messages", "crashes", "connect", "updates"] {
        assert!(!crate::help::available(&h.app, item), "{item}");
        crate::help::run(&mut h.app, &h.ctx, item);
    }
    assert!(!h.app.help.about && !h.app.keymap.open && !h.app.messages.open && h.app.help.connect.is_none());
    assert!(!h.app.updater.open);
    assert!(crate::help::available(&h.app, "report") && crate::help::available(&h.app, "docs"));
    h.app.issue_report.draft = None;
    assert!(FileAction::Undo.enabled(&h.app) && FileAction::Settings.enabled(&h.app) && FileAction::Quit.enabled(&h.app));
    assert!(crate::help::available(&h.app, "shortcuts"));
}

#[test]
fn native_menu_actions_share_json_save_and_recent_cleanup() {
    let mut h = crate::scenario::Harness::new();
    let path = h.dir.join("native-menu.json");
    let output = path.clone();
    h.app.services.pick_save = Some(Box::new(move |_, _| Box::pin(std::future::ready(Some(output.to_string_lossy().into_owned())))));
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    FileAction::SaveJson.run(&mut h.app);
    h.frames(3);
    h.settle();
    let bytes = std::fs::read(path).unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&bytes).is_ok());
    assert!(!h.app.home.recent.is_empty());
    FileAction::ClearRecent.run(&mut h.app);
    assert!(h.app.home.recent.is_empty());
    assert!(h.app.home.opened_at.is_empty());
    assert!(!FileAction::ClearRecent.enabled(&h.app));
}

#[test]
fn file_menu_json_save_and_export_use_commands_and_keep_distinct_paths() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    let dir = h.dir.clone();
    h.app.services.pick_save =
        Some(Box::new(move |name, _extensions| Box::pin(std::future::ready(Some(dir.join(name).to_string_lossy().into_owned())))));
    h.frames(3);
    h.step(&json!({"click":{"handle":"appbar:folder"}})).unwrap();
    h.step(&json!({"widget":"Save As JSON…"})).unwrap();
    // Written on a worker: the window keeps drawing until it is done.
    h.settle();
    let saved = h.app.session.path.clone().unwrap();
    let bytes = std::fs::read(&saved).unwrap();
    assert!(!sketcraft_engine::io::is_design_container(&bytes));
    assert_eq!(sketcraft_engine::io::read_design(&bytes).unwrap(), *h.app.session.doc);
    h.app.session.doc_mut().name = "Exported copy".into();
    h.frames(3);
    h.step(&json!({"click":{"handle":"appbar:folder"}})).unwrap();
    h.step(&json!({"widget":"Export Design as JSON…"})).unwrap();
    assert_eq!(h.app.session.path.as_deref(), Some(saved.as_str()));
    assert!(h.dir.join("Exported copy.json").is_file());
    h.app.run("file.save", json!({})).unwrap();
    assert!(!sketcraft_engine::io::is_design_container(&std::fs::read(saved).unwrap()), "subsequent Save keeps a .json choice");
}

/// File › Export STL… and Export 3MF… both open the Save as mesh dialog with their format
/// chosen (whatever was used last), and command search finds the export by format or purpose.
#[test]
fn file_menu_has_export_stl_and_3mf_that_open_the_mesh_dialog_with_that_format() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    h.app.preferences.save_mesh_format = "3mf".into();
    for (item, format) in [("Export STL…", "stl"), ("Export 3MF…", "3mf")] {
        h.frames(3);
        h.step(&json!({"click":{"handle":"appbar:file"}})).unwrap();
        h.step(&json!({"widget": item})).unwrap();
        let Some(crate::dialogs::Kind::SaveMesh(k)) = h.app.dialog.as_ref().map(|d| &d.kind) else { panic!("{item}: no Save as mesh dialog") };
        assert_eq!(k.format_name(), format, "{item}");
        crate::dialogs::cancel(&mut h.app);
    }
    for q in ["3mf", "stl", "export 3mf", "print"] {
        let found = crate::shortcut_box::search(&h.app, q);
        assert!(found.iter().any(|r| r.0 == "file.save_mesh"), "{q}: {:?}", found.iter().map(|r| r.0).collect::<Vec<_>>());
    }
}

/// The browser build's File > Download design hands the open design to the host as a
/// `.solidcraft` container named after it; a host that saves real files shows no such item.
#[test]
fn file_menu_download_design_hands_the_container_to_the_host() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    h.app.session.doc_mut().name = "Plate".into();
    let got = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(String, Vec<u8>)>::new()));
    let sink = got.clone();
    h.app.services.download_design = Some(Box::new(move |name, bytes| {
        sink.borrow_mut().push((name.to_string(), bytes.to_vec()));
        Ok(())
    }));
    h.frames(3);
    h.step(&json!({"click":{"handle":"appbar:folder"}})).unwrap();
    h.step(&json!({"widget":"Download design"})).unwrap();
    let got = got.borrow();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "Plate.sc");
    assert!(sketcraft_engine::io::is_design_container(&got[0].1));
    assert!(h.app.session.path.is_none(), "a download is no save: the design keeps no file");
}

/// Ctrl+Shift+S opens Save As in the design workspace (it never saves over the current file),
/// and the File menu shows that shortcut next to Save As (#6558).
#[test]
fn ctrl_shift_s_opens_save_as_in_the_workspace_and_the_file_menu_shows_the_shortcut() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
    let source = h.dir.join("source.solidcraft");
    h.app.run("file.save_as", json!({"path":source})).unwrap();
    let bytes = std::fs::read(&source).unwrap();
    h.app.session.doc_mut().name = "Copy".into();
    h.frames(3);
    h.step(&json!({"key":"S","cmd":true,"shift":true})).unwrap();
    h.frames(3);
    h.settle();
    assert_eq!(h.app.session.path.as_deref(), h.dir.join("Copy.sc").to_str(), "Ctrl+Shift+S must open Save As, not save over the current file");
    assert_eq!(std::fs::read(&source).unwrap(), bytes, "Save As keeps the file the design came from");
    // The File menu shows the shortcut of the command it runs.
    let spec = sketcraft_engine::find_command("file.save_as").expect("file.save_as is a command");
    assert_eq!(spec.shortcut, Some("Ctrl+Shift+S"), "the File menu shows the shortcut of Save As");
    h.frames(3);
    h.step(&json!({"click":{"handle":"appbar:file"}})).unwrap();
    assert!(h.widget("Save As…").is_some(), "the File menu has a Save As row");
}

fn sizes(buttons: &[usize]) -> Vec<PanelSize> {
    buttons.iter().map(|&b| PanelSize { buttons: vec![ButtonSize { width: BUTTON_MIN_WIDTH, min_width: BUTTON_MIN_WIDTH }; b] }).collect()
}

fn used_with_more(plans: &[Plan]) -> f32 {
    used(plans) + if plans.iter().any(|p| p.fit == Fit::More) { MORE_ROOM } else { 0.0 }
}

#[test]
fn panels_give_up_one_button_at_a_time_then_move_to_more_and_none_is_dropped() {
    // A panel with 14 buttons and three default ones.
    let panels = sizes(&[14, 4, 2, 2]);
    let full: f32 = [14, 4, 2, 2].iter().map(|&b| panel_width(&vec![BUTTON_MIN_WIDTH; b]) + PANEL_GAP).sum();
    let all = |plans: &[Plan]| plans.iter().map(|p| (p.fit, p.shown)).collect::<Vec<_>>();
    assert_eq!(all(&fit(&panels, full)), [(Fit::Buttons, 14), (Fit::Buttons, 4), (Fit::Buttons, 2), (Fit::Buttons, 2)]);
    // A little short: the widest panel hands one button to its drop-down; the others keep all theirs.
    assert_eq!(all(&fit(&panels, full - 10.0)), [(Fit::Buttons, 13), (Fit::Buttons, 4), (Fit::Buttons, 2), (Fit::Buttons, 2)]);
    // Every panel down to one button (collapsed) fits.
    let collapsed = 4.0 * (COLLAPSED_WIDTH + PANEL_GAP);
    assert_eq!(all(&fit(&panels, collapsed)), [(Fit::Collapsed, 1); 4]);
    // Narrower: the last one goes into the » menu, and the room it frees goes back to buttons.
    let more = fit(&panels, collapsed - 1.0);
    assert_eq!(more.iter().map(|p| p.fit).collect::<Vec<_>>(), [Fit::Collapsed, Fit::Collapsed, Fit::Collapsed, Fit::More]);
    assert!(more.iter().take(3).all(|p| p.shown >= 1));
    let more = fit(&sizes(&[14, 4, 2, 2, 2, 2]), 5.0 * (COLLAPSED_WIDTH + PANEL_GAP) + MORE_ROOM - 1.0);
    assert_eq!(more.last().unwrap().fit, Fit::More);
    assert!(more.iter().any(|p| p.fit == Fit::Buttons && p.shown > 1), "overflow room restores buttons");
    // Then all of them.
    assert!(fit(&panels, 0.0).iter().all(|p| p.fit == Fit::More));
    assert_eq!(fit(&panels, f32::NAN).len(), 4);
    // No cap: every additional button keeps its width and gap.
    assert_eq!(panel_width(&[BUTTON_MIN_WIDTH; 30]) - panel_width(&[BUTTON_MIN_WIDTH; 20]), 10.0 * (BUTTON_MIN_WIDTH + BUTTON_GAP));
}

/// #2108: whatever the room, the toolbar uses it. It never leaves more than one button's width
/// blank while something is hidden, never runs past the room, and shrinks steadily as the
/// room does (so a few pixels never flip it back and forth).
#[test]
fn panels_fill_the_room_they_have_at_every_width() {
    let layouts = [
        // SOLID: CREATE, MODIFY, ASSEMBLE, CONFIGURE, CONSTRUCT, INSPECT, INSERT, SELECT.
        vec![(4, false), (4, false), (3, false), (2, false), (2, false), (2, false), (2, false), (0, false)],
        // SKETCH: labelled CREATE, then MODIFY … FINISH SKETCH.
        vec![(8, true), (2, false), (3, false), (6, false), (2, false), (2, false), (0, false), (1, false)],
        // Many pins on one panel.
        vec![(14, false), (4, false), (2, false), (2, false)],
    ];
    for layout in layouts {
        let panels: Vec<PanelSize> = layout
            .iter()
            .map(|&(buttons, labelled)| PanelSize {
                buttons: (0..buttons)
                    .map(|i| ButtonSize {
                        width: if labelled { BUTTON_MAX_WIDTH - (i % 3) as f32 * 5.0 } else { BUTTON_MIN_WIDTH },
                        min_width: BUTTON_MIN_WIDTH,
                    })
                    .collect(),
            })
            .collect();
        let mut last: Option<(f32, usize)> = None;
        for room in (0..=2000).rev().map(|r| r as f32) {
            let plans = fit(&panels, room);
            let width = used_with_more(&plans);
            let hidden = plans
                .iter()
                .zip(&panels)
                .any(|(p, s)| p.fit != Fit::Buttons || p.shown < s.buttons.len() || p.widths.iter().zip(&s.buttons).any(|(w, b)| *w < b.width));
            let more = plans.iter().filter(|p| p.fit == Fit::More).count();
            if more < panels.len() {
                assert!(width <= room + 0.01, "{layout:?} at {room}: {width} overruns");
            }
            // One button: an icon's stride, or a collapsed panel's when the next panel waits in the » menu.
            let button = if more > 0 { COLLAPSED_WIDTH + PANEL_GAP } else { BUTTON_MIN_WIDTH + BUTTON_GAP };
            if hidden && more < panels.len() {
                assert!(room - width < button, "{layout:?} at {room}: {} px blank while something is hidden", room - width);
            }
            // Fewer pixels never bring a panel back out of the » menu.
            if let Some((_, prev_more)) = last {
                assert!(more >= prev_more, "{layout:?} at {room}: a panel came back out of the » menu");
            }
            last = Some((width, more));
            // A panel shows its icons: one at the very least.
            for (p, s) in plans.iter().zip(&panels) {
                if p.fit != Fit::More && !s.buttons.is_empty() {
                    assert!(p.shown >= 1, "{layout:?} at {room}: a panel without icons");
                }
            }
        }
    }
}

#[test]
fn variable_width_buttons_narrow_before_rightmost_equal_panels_lose_buttons() {
    let panels = vec![PanelSize {
        buttons: vec![
            ButtonSize { width: 76.0, min_width: BUTTON_MIN_WIDTH },
            ButtonSize { width: 63.0, min_width: BUTTON_MIN_WIDTH },
            ButtonSize { width: 54.0, min_width: BUTTON_MIN_WIDTH },
        ],
    }];
    let full = used(&panels.iter().map(Plan::full).collect::<Vec<_>>());
    let narrow = fit(&panels, full - 1.0);
    assert_eq!(narrow[0].shown, 3);
    assert_eq!(narrow[0].widths, [75.0, 62.0, 54.0]);
    let minimum = panel_width(&[BUTTON_MIN_WIDTH; 3]) + PANEL_GAP;
    let narrow = fit(&panels, minimum);
    assert_eq!(narrow[0].shown, 3);
    assert_eq!(narrow[0].widths, [BUTTON_MIN_WIDTH; 3]);
    assert_eq!(fit(&panels, minimum - 1.0)[0].shown, 2);

    let equal = sizes(&[3, 3]);
    let full = used(&equal.iter().map(Plan::full).collect::<Vec<_>>());
    assert_eq!(fit(&equal, full - 1.0).iter().map(|p| p.shown).collect::<Vec<_>>(), [3, 2]);
}

#[test]
fn constraint_gallery_stays_fixed_and_hands_all_cells_to_the_menu_together() {
    let panels = vec![PanelSize {
        buttons: vec![
            ButtonSize { width: BUTTON_MAX_WIDTH, min_width: BUTTON_MIN_WIDTH },
            ButtonSize { width: CONSTRAINT_GRID_WIDTH, min_width: CONSTRAINT_GRID_WIDTH },
        ],
    }];
    let minimum = panel_width(&[BUTTON_MIN_WIDTH, CONSTRAINT_GRID_WIDTH]) + PANEL_GAP;
    let narrow = fit(&panels, minimum);
    assert_eq!(narrow[0].shown, 2);
    assert_eq!(narrow[0].widths, [BUTTON_MIN_WIDTH, CONSTRAINT_GRID_WIDTH]);
    let collapsed = fit(&panels, minimum - 1.0);
    assert_eq!((collapsed[0].fit, collapsed[0].shown), (Fit::Collapsed, 1));
}

fn captured_frame(h: &mut crate::scenario::Harness) {
    let token = h.app.shot_token;
    crate::issue_report::receive(&mut h.app, token, std::sync::Arc::new(egui::ColorImage::new([2, 2], vec![Color32::GREEN; 4])));
    h.frames(3);
}

#[test]
fn panel_accents_share_the_tab_mapping_but_constraints_keep_their_role() {
    for dark in [false, true] {
        for vision in crate::theme::ColorVision::ALL {
            let t = Tokens::themed(dark, vision);
            for tab in TABS.iter().copied().chain(["SKETCH", "FORM", "RENDER"]) {
                assert_eq!(panel_accent(&t, tab, "CREATE"), crate::theme::TabAccent::for_tab(&t, tab).color);
                assert_eq!(panel_accent(&t, tab, "CONSTRAIN"), t.glyph_text);
            }
        }
    }
}

#[test]
fn header_report_reserves_controls_at_normal_and_double_scale_widths() {
    for physical_width in [900.0, 1400.0] {
        for scale in [1.0, 2.0] {
            let width = physical_width / scale;
            let bar = Rect::from_min_size(egui::Pos2::ZERO, vec2(width, APP_BAR_HEIGHT));
            for captions in [0.0, crate::titlebar::WIDTH] {
                // The English label, and a longer one (Russian is about 125 px).
                for (label_width, layout) in [80.0, 125.0].into_iter().flat_map(|w| [(w, HeaderLayout::DESKTOP), (w, HeaderLayout::COMPACT)]) {
                    let (report, labeled) = report_rect(bar, captions, 354.0, label_width, 40.0, layout);
                    assert!(bar.contains_rect(report));
                    let prefs = Rect::from_center_size(pos2(report.left() - 52.0, APP_BAR_HEIGHT / 2.0), vec2(32.0, 32.0));
                    let help = Rect::from_center_size(pos2(help_x(report), APP_BAR_HEIGHT / 2.0), vec2(32.0, 32.0));
                    assert!(!report.intersects(prefs) && !report.intersects(help) && !help.intersects(prefs));
                    assert!(prefs.right() < help.left() && help.right() < report.left());
                    if layout.report == ReportButton::Icon {
                        assert!(!labeled && report.size() == vec2(32.0, 32.0));
                        continue;
                    }
                    let full = REPORT_TEXT_X + label_width + 11.0;
                    assert_eq!(
                        labeled,
                        width - captions - version_room(bar, captions, 354.0, 40.0).unwrap_or(8.0) - CLUSTER_BUTTONS - 354.0 >= 216.0 + full
                    );
                    if labeled {
                        assert!(REPORT_TEXT_X + label_width < report.width(), "{label_width}: the text overruns its button");
                    }
                }
            }
        }
    }
}

/// The 34 px bar groups brand/home, File/Save/Undo/Redo, documents, then the right cluster.
#[test]
fn title_bar_keeps_tabs_left_and_help_right() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.custom_titlebar = true;
    h.app.run("solid.box", json!({"length":10,"width":10,"height":10})).unwrap();
    h.call("ui.resize", json!({"width":1400,"height":720}));
    h.frames(3);
    for gone in ["appbar:sun", "appbar:moon", "toolbar:ai"] {
        assert!(crate::scenario::handle_at(gone).is_none(), "{gone} is no longer in the title bar");
    }
    let at = |name: &str| crate::scenario::handle_at(name).unwrap_or_else(|| panic!("{name}")).x;
    let [report, _] = h.widget("Report a bug").unwrap();
    let file = h.widget("File").expect("the File menu has a label");
    assert_eq!(file[0] as f32, at("appbar:file"));
    assert_eq!(APP_BAR_HEIGHT, 34.0);
    let rect = |name: &str| crate::scenario::rect_of(name).unwrap();
    let first = rect("titlebar:separator:brand");
    let second = rect("titlebar:separator:documents");
    assert_eq!(first.size(), vec2(1.0, 20.0));
    assert_eq!(second.size(), vec2(1.0, 20.0));
    assert_eq!(first.center().y, 17.0);
    assert_eq!(second.center().y, 17.0);
    assert!(rect("titlebar:home").right() < first.left());
    assert!(first.right() < rect("appbar:file").left());
    assert!(at("appbar:file") < at("titlebar:save") && at("titlebar:save") < at("appbar:undo") && at("appbar:undo") < at("appbar:redo"));
    assert!(rect("appbar:redo").right() < second.left());
    assert!(second.right() < rect("tab:design:0").left());
    assert!(at("tab:design:0") < at("titlebar:assistant"));
    assert!(at("titlebar:assistant") < at("titlebar:preferences"));
    for name in ["titlebar:assistant", "titlebar:preferences", "appbar:help"] {
        assert_eq!(rect(name).size(), vec2(32.0, 32.0));
        assert!(rect(name).top() >= 0.0 && rect(name).bottom() <= APP_BAR_HEIGHT);
    }
    assert!(at("titlebar:preferences") < at("appbar:help") && (at("appbar:help") as f64) < report);
    assert!(at("appbar:home") < at("appbar:file"));
    assert!(h.widget("Start page").is_some(), "the house button has an accessible name");
    h.step(&json!({"click":{"handle":"appbar:help"}})).unwrap();
    assert!(h.app.help.menu, "? opens the Help menu");
    h.step(&json!({"key":"Escape"})).unwrap();
    h.step(&json!({"click":{"handle":"appbar:home"}})).unwrap();
    assert!(h.app.home.open, "the house button shows the start page");
    let dark = h.app.ui.dark;
    h.call("ui.prefs", json!({"theme": if dark { "light" } else { "dark" }}));
    h.frames(2);
    assert_ne!(h.app.ui.dark, dark, "the theme is a preference");
}

#[test]
fn header_report_button_widens_for_a_longer_translation() {
    let width_in = |language: &str| {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.custom_titlebar = true;
        h.app.preferences.language = language.into();
        h.call("ui.resize", json!({"width":1400.0,"height":720}));
        h.frames(3);
        let center = crate::scenario::handle_at("titlebar:report").expect("the report button is drawn");
        let t = Tokens::get();
        let version = format!("v{}", crate::update::current_version());
        let version = h.ctx.fonts_mut(|f| f.layout_no_wrap(version, FontId::proportional(t.font_small), t.app_bar_text).size().x);
        // The button's right edge is fixed; its width is twice the distance to its centre.
        let bar = Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, APP_BAR_HEIGHT));
        let room = version_room(bar, crate::titlebar::WIDTH, 354.0, version).unwrap_or(8.0);
        2.0 * (1400.0 - crate::titlebar::WIDTH - room - center.x)
    };
    let (en, ru) = (width_in("en"), width_in("ru"));
    assert!((en - 114.34375).abs() < 0.05, "Plex Sans report button width: {en}");
    // "Сообщить об ошибке" needs more than the English button's 114 px.
    assert!(ru > en + 10.0, "{ru}");
}

#[test]
fn the_version_link_never_covers_the_report_button() {
    for width in [700.0, 900.0, 1400.0] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.custom_titlebar = true;
        h.call("ui.resize", json!({"width":width,"height":720}));
        h.frames(3);
        // A long version (a release candidate's) gives way to the document tabs on a narrow bar.
        let Some(version) = crate::scenario::handle_at("titlebar:version") else {
            assert!(width < 900.0, "the version is drawn as a link at {width}");
            continue;
        };
        let report = crate::scenario::handle_at("titlebar:report").expect("the report button is drawn");
        // Its click area starts where the report button ends, so a click on that button reaches the report.
        assert!(version.x > report.x, "width {width}");
    }
}

/// The desktop app shows the default header layout: search in the ribbon's tab row, a
/// labelled Report a bug, the version at the bar's right end and not in the Help menu.
#[test]
fn the_default_header_layout_is_the_desktop_one() {
    assert_eq!(HeaderLayout::default(), HeaderLayout::DESKTOP);
    assert_eq!(
        HeaderLayout::DESKTOP,
        HeaderLayout { search: SearchPlace::Ribbon, report: ReportButton::Labelled, version: VersionPlace::Header }
    );
    let mut h = crate::scenario::Harness::new();
    assert_eq!(h.app.header, HeaderLayout::DESKTOP, "a new app has the desktop layout");
    h.app.home.open = false;
    h.app.custom_titlebar = true;
    h.call("ui.resize", json!({"width":1400.0,"height":720}));
    h.frames(3);
    let rect = |name: &str| crate::scenario::rect_of(name).unwrap_or_else(|| panic!("{name}"));
    let search = rect("ribbon:search");
    assert!(search.top() >= APP_BAR_HEIGHT, "search is in the ribbon: {search:?}");
    assert_eq!(search.height(), TAB_ROW_HEIGHT - 2.0 * RIBBON_TAB_INSET);
    assert_eq!(search.width(), crate::ui_kit::CommandSearch::closed_width(&h.ctx));
    assert!(crate::scenario::rect_of("titlebar:search").is_none());
    let report = rect("titlebar:report");
    // A pre-release's longer version takes more room than RIGHT_GROUP and moves the button left.
    let t = Tokens::get();
    let version = format!("v{}", crate::update::current_version());
    let version = h.ctx.fonts_mut(|f| f.layout_no_wrap(version, FontId::proportional(t.font_small), t.app_bar_text).size().x);
    assert_eq!(report.right(), 1400.0 - crate::titlebar::WIDTH - RIGHT_GROUP.max(12.0 + version + 8.0));
    assert_eq!(report.height(), 26.0);
    assert!(report.width() > 100.0, "Report a bug carries its label: {report:?}");
    assert_eq!(rect("titlebar:preferences").center().x, report.left() - 52.0);
    assert!(crate::scenario::handle_at("titlebar:version").is_some(), "the version is in the bar");
    h.step(&json!({"click":{"handle":"appbar:help"}})).unwrap();
    assert!(h.app.help.menu);
    let version = crate::i18n::format("help.version", &[("version", crate::update::current_version())]);
    assert!(h.widget("About SolidCraft").is_some());
    assert!(h.widget(&version).is_none(), "the Help menu has no version row");
}

/// The alternative header layout: search in the bar just left of Settings, Report a bug as an
/// icon like Settings and Help, and the version as a Help menu row.
#[test]
fn a_host_can_move_search_report_and_version_in_the_header() {
    let mut h = crate::scenario::Harness::new();
    h.app.header = HeaderLayout::COMPACT;
    h.app.home.open = false;
    h.app.custom_titlebar = true;
    h.call("ui.resize", json!({"width":1400.0,"height":720}));
    h.frames(3);
    let rect = |name: &str| crate::scenario::rect_of(name).unwrap_or_else(|| panic!("{name}"));
    let (search, prefs) = (rect("titlebar:search"), rect("titlebar:preferences"));
    assert_eq!(search.right(), prefs.left() - HEADER_SEARCH_GAP, "search sits immediately left of Settings");
    assert!(search.bottom() <= APP_BAR_HEIGHT && (search.center().y - prefs.center().y).abs() < 0.5);
    assert_eq!(search.width(), crate::ui_kit::CommandSearch::closed_width(&h.ctx), "the whole field where there is room");
    assert_eq!(rect("ribbon:search"), search, "the one search field is drawn in the bar");
    // Report a bug: an icon the size of Settings and Help, still named for assistive tools.
    let report = rect("titlebar:report");
    assert_eq!(report.size(), vec2(32.0, 32.0));
    assert_eq!(report.right(), 1400.0 - crate::titlebar::WIDTH - 8.0, "no room is kept for the version");
    assert_eq!(rect("appbar:help").size(), report.size());
    assert!(h.widget("Report a bug").is_some(), "the icon keeps its accessible name");
    assert!(crate::scenario::handle_at("titlebar:version").is_none(), "the version left the bar");
    // The ribbon's tabs run to its end; the first is SketchUp's.
    let card = rect("ribbon:card");
    assert!(rect(&format!("toolbar_tab:{}", crate::workspace::SKETCHUP_TAB)).left() < card.left() + 10.0);
    // Clicking the field opens command search.
    h.step(&json!({"click":{"handle":"titlebar:search"}})).unwrap();
    assert!(h.app.ui.palette_open, "the header field opens search");
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(!h.app.ui.palette_open);
    // The version is a Help menu row, right after About.
    h.step(&json!({"click":{"handle":"appbar:help"}})).unwrap();
    assert!(h.app.help.menu);
    let version = crate::i18n::format("help.version", &[("version", crate::update::current_version())]);
    let [_, about] = h.widget("About SolidCraft").unwrap();
    let [_, row] = h.widget(&version).expect("the Help menu lists the version");
    let [_, shortcuts] = h.widget("Keyboard Shortcuts").unwrap();
    assert!(about < row && row < shortcuts, "between About and Keyboard Shortcuts");
    h.step(&json!({"widget":version})).unwrap();
    assert!(!h.app.help.menu, "choosing the row closes the menu");
    // In a narrow bar the field folds to its magnifier and still opens search.
    h.call("ui.resize", json!({"width":700.0,"height":720}));
    h.frames(3);
    let search = rect("titlebar:search");
    assert_eq!(search.width(), crate::ui_kit::CommandSearch::compact_width(26.0));
    assert_eq!(search.right(), rect("titlebar:preferences").left() - HEADER_SEARCH_GAP);
    h.step(&json!({"click":{"handle":"titlebar:search"}})).unwrap();
    assert!(h.app.ui.palette_open, "the magnifier opens search");
}

/// With native menus the bar keeps its own Settings, so search the host put in the bar sits
/// just left of it there too.
#[test]
fn search_in_the_bar_beside_native_menus() {
    let mut h = crate::scenario::Harness::new();
    h.app.header = HeaderLayout::COMPACT;
    h.app.native_file_menu = true;
    h.app.integrated_titlebar = true;
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1400.0,"height":720}));
    h.frames(3);
    let search = crate::scenario::rect_of("titlebar:search").expect("search in the bar");
    let gear = crate::scenario::rect_of("titlebar:preferences").expect("Settings in the bar");
    assert!(search.bottom() <= APP_BAR_HEIGHT && search.right() <= gear.left(), "{search:?} {gear:?}");
}

/// A release candidate's version is wider than a release's: the button moves left for it
/// instead of lying under it, and where that would crowd out the document tabs the version is
/// hidden, as on a narrow bar (#5621).
#[test]
fn a_longer_version_moves_the_report_button_instead_of_covering_it() {
    let h = crate::scenario::Harness::new();
    let t = Tokens::get();
    let measure = |text: &str| h.ctx.fonts_mut(|f| f.layout_no_wrap(text.into(), FontId::proportional(t.font_small), t.app_bar_text).size().x);
    let (release, candidate) = (measure("v0.25.1"), measure("v0.21.0-rc.1"));
    assert!(candidate > release + 15.0, "{release} {candidate}");
    for width in [700.0, 900.0, 1400.0] {
        let bar = Rect::from_min_size(egui::Pos2::ZERO, vec2(width, APP_BAR_HEIGHT));
        for captions in [0.0, crate::titlebar::WIDTH] {
            // A release keeps the button where it always was.
            assert_eq!(version_room(bar, captions, 354.0, release), Some(RIGHT_GROUP), "{width} {captions}");
            for (k, version_width) in [release, candidate, 2.0 * candidate].into_iter().enumerate() {
                let (report, _) = report_rect(bar, captions, 354.0, 80.0, version_width, HeaderLayout::DESKTOP);
                let shown = version_room(bar, captions, 354.0, version_width).is_some();
                if shown {
                    let version_left = bar.right() - 12.0 - captions - version_width;
                    assert!(report.right() + 8.0 <= version_left + 0.01, "{width} {captions} {version_width}: {report:?}");
                } else {
                    assert_eq!(report.right(), bar.right() - captions - 8.0, "{width} {captions} {version_width}: the narrow bar's margin");
                }
                if k == 1 {
                    assert_eq!(shown, width >= 900.0, "{width} {captions}: the release candidate's version");
                }
            }
        }
    }
}

#[test]
fn header_report_actual_click_opens_capture_without_changing_healthy_design() {
    for width in [450.0, 700.0, 900.0, 1400.0] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.custom_titlebar = true;
        h.app.run("solid.box", json!({"length":35,"width":12,"height":4})).unwrap();
        h.call("ui.resize", json!({"width":width,"height":720}));
        h.frames(3);
        let before = (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.revision);
        let point = h.widget("Report a bug").expect("compact and full buttons retain an accessible name");
        h.step(&json!({"click":point})).unwrap();
        assert!(h.app.issue_report.draft.is_some(), "width {width}: header click must reach the report");
        assert!(crate::issue_report::pending(&h.app));
        assert_eq!(before, (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.revision));
        captured_frame(&mut h);
        // The report card takes a few frames to reach its full height.
        h.frames(16);
        let close_label = if h.widget("Close draft").is_some() { "Close draft" } else { "Close" };
        // Existing review content scrolls vertically when the window is narrow.
        if width < 700.0 {
            let [x, y] = h.widget(close_label).unwrap();
            h.call("ui.scroll", json!({"x":x,"y":y - 60.0,"delta":-1200}));
            // Let the scroll animation finish before locating the click target.
            h.frames(60);
        }
        h.step(&json!({"widget":close_label})).unwrap();
        assert!(h.app.issue_report.draft.is_none(), "width {width}: review closes through its actual button");
        h.step(&json!({"click":{"handle":"tab:home"}})).unwrap();
        assert!(h.app.home.open, "width {width}: the Home tab remains reachable beside Report");
        assert_eq!(before, (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.revision));
    }
}

#[test]
fn header_report_preserves_new_dimension_draft_and_single_commit() {
    crate::dim_view::cancel();
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.app.run("sketch.line", json!({"points":[[0,0],[20,0]]})).unwrap();
    h.call("ui.view", json!({"view":"top"}));
    h.app.cam.distance = 80.0;
    h.app.cam.target = solidcore_record::Vec3::new(10.0, 0.0, 0.0);
    h.settle();
    h.step(&json!({"start":"sketch.dimension"})).unwrap();
    for point in [[0, 0, 0], [20, 0, 0], [10, 5, 0]] {
        h.step(&json!({"click":{"world":point}})).unwrap();
    }
    h.step(&json!({"text":"30 mm"})).unwrap();
    let before = (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.revision);
    h.step(&json!({"widget":"Report a bug"})).unwrap();
    h.frames(3);
    assert!(h.app.issue_report.draft.is_some() && crate::dim_view::editing());
    captured_frame(&mut h);
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::unique("issue_report_body")));
    h.step(&json!({"text":"Describe this dimension problem."})).unwrap();
    h.step(&json!({"key":"Enter"})).unwrap();
    assert_eq!(before, (h.app.session.doc.to_json(), h.app.session.undo.len(), h.app.session.revision));
    h.step(&json!({"widget":"Close draft"})).unwrap();
    h.frames(2);
    assert_eq!(h.ctx.memory(|m| m.focused()), Some(egui::Id::unique("sc_dim_edit")));
    h.step(&json!({"key":"Enter"})).unwrap();
    assert!(!crate::dim_view::editing());
    assert_eq!(h.app.session.undo.len(), before.1 + 1);
    assert_eq!(h.app.session.doc.param("d1").unwrap().expr, "30 mm");
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.doc.to_json(), before.0);
}
