use serde_json::json;
use sketcraft_engine::Session;

use super::*;
use crate::Services;
use crate::dialogs::apply_commands;

/// Components A (grounded) and B: a 20 x 20 x 10 box each, B 30 mm along X.
fn two_boxes() -> SketApp {
    let mut s = Session::default();
    crate::test_support::grounded_boxes(&[("A", 0), ("B", 30)], |id, p| s.execute(id, &p).unwrap());
    SketApp::new(s, Services::default())
}

fn occ(app: &SketApp, name: &str) -> u64 {
    let c = app.session.doc.find_component(name).unwrap();
    app.session.doc.occurrence_of(c).unwrap().id
}

/// As-built joints and rigid groups take components from body picks; New Component makes one
/// from bodies when bodies are picked.
#[test]
fn component_picks_become_occurrences() {
    let mut app = two_boxes();
    app.session.selection = vec![Sel::Body { name: "Body1".into() }, Sel::Body { name: "Body2".into() }];
    app.start("joint.as_built");
    let d = app.dialog.clone().unwrap();
    let c = apply_commands(&app, &d).unwrap();
    assert_eq!(c[0].1["a"], occ(&app, "A"));
    assert_eq!(c[0].1["b"], occ(&app, "B"));
    app.session.selection = vec![Sel::Body { name: "Body1".into() }, Sel::Body { name: "Body2".into() }];
    app.start("joint.rigid_group");
    let c = apply_commands(&app, app.dialog.as_ref().unwrap()).unwrap();
    assert_eq!(c[0].1["occurrences"], json!([occ(&app, "A"), occ(&app, "B")]));
    app.dialog = None;
    app.start("component.create");
    let c = apply_commands(&app, app.dialog.as_ref().unwrap()).unwrap();
    assert_eq!(c[0].0, "component.create");
}

/// A body of a further instance of a component belongs to that instance, not the first (#3857).
#[test]
fn body_of_a_further_instance_is_its_own_occurrence() {
    let mut app = two_boxes();
    let copy = app.session.execute("occurrence.copy", &json!({"component": "B", "translate": [0, 40, 0]})).unwrap()["occurrence"].as_u64().unwrap();
    let first = occ(&app, "B");
    assert_ne!(copy, first);
    let frames = app.session.doc.body_frames(&app.session.model.state());
    let of = |o: u64| frames.iter().find(|f| f.2.last() == Some(&o)).map(|f| f.0.clone()).unwrap();
    assert_eq!(occurrence_of_body(&app.session, &of(first)), Some(first));
    assert_eq!(occurrence_of_body(&app.session, &of(copy)), Some(copy));
}

/// Round edges snap to their centre; straight ones are not circles.
#[test]
fn circles_from_edge_polylines() {
    let pts: Vec<Vec3> =
        (0..=24).map(|i| f64::from(i) / 24.0 * std::f64::consts::TAU).map(|t| Vec3::new(5.0 + 3.0 * t.cos(), 2.0 + 3.0 * t.sin(), 7.0)).collect();
    let (c, n) = circle_of(&pts).unwrap();
    assert!(c.dist(Vec3::new(5.0, 2.0, 7.0)) < 1e-9 && n.cross(Vec3::Z).len() < 1e-9);
    let line: Vec<Vec3> = (0..10).map(|i| Vec3::new(f64::from(i), 0.0, 0.0)).collect();
    assert!(circle_of(&line).is_none());
}

/// A moved component's edge axis is measured in the displayed world frame.
#[test]
fn straight_edge_snap_follows_a_rotated_occurrence() {
    let mut app = two_boxes();
    let b = app.session.doc.find_component("B").unwrap();
    app.run("occurrence.move", json!({"occurrence": occ(&app, "B"), "axis": [1, 2, 3], "angle": "37 deg", "translate": [50, 20, 10]})).unwrap();
    let transform = app.session.doc.occurrence_of(b).unwrap().transform;
    let mid = sketcraft_engine::doc::apply_point(&transform, Vec3::new(30.0, 10.0, 10.0));
    let axis = sketcraft_engine::doc::apply_vector(&transform, Vec3::Y);
    let st = crate::open_meshing::session_world(&app.session);
    let mesh = st.body("Body2").unwrap().mesh();
    let index = mesh.edges.iter().position(|e| polyline_mid(e).dist(mid) < 1e-6).unwrap();
    let snap = snap_of(&app.session, &Sel::Edge { body: "Body2".into(), index, point: mid }).unwrap();
    assert!(snap.at.dist(mid) < 1e-6);
    assert!(snap.z.cross(axis).len() < 1e-9, "{snap:?}");
    assert_eq!(snap.param["occurrence"], occ(&app, "B"));
}

/// Only finite straight polylines supply an axis; reversing their samples keeps its direction.
#[test]
fn straight_edge_axes_reject_curves_and_degenerate_samples() {
    for dir in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(-2.0, 3.0, 1.0)] {
        let mut pts = vec![Vec3::ZERO, dir * 0.5, dir];
        let axis = straight_edge_axis(&pts).unwrap();
        assert!(axis.cross(dir).len() < 1e-9);
        assert!((axis.len() - 1.0).abs() < 1e-9);
        pts.reverse();
        assert_eq!(straight_edge_axis(&pts), Some(axis));
    }
    for pts in [
        vec![],
        vec![Vec3::X],
        vec![Vec3::X, Vec3::X],
        vec![Vec3::ZERO, Vec3::Y, Vec3::X],
        vec![Vec3::ZERO, Vec3::new(f64::NAN, 0.0, 0.0), Vec3::X],
        vec![Vec3::ZERO, Vec3::X * f64::INFINITY],
    ] {
        assert!(straight_edge_axis(&pts).is_none(), "{pts:?}");
    }
}

/// The Assemble ribbon must open a body picker, without executing the JSON command yet.
#[test]
fn components_from_bodies_dialog_select_all_cancel_and_ok() {
    let mut h = crate::scenario::Harness::new();
    for x in [0, 30] {
        h.app.run("solid.box", json!({"length":10,"width":10,"height":10,"corner":[x,0,0]})).unwrap();
    }
    let before = h.app.session.doc.clone();
    let names: Vec<_> = h.app.session.model.state().bodies.iter().map(|body| body.name.to_string()).collect();
    h.step(&json!({"start":"component.from_bodies"})).unwrap();
    let d = h.app.dialog.as_ref().expect("From Bodies opens a dialog");
    assert!(matches!(d.kind, Kind::Assembly(Asm::FromBodies)));
    assert!(apply_commands(&h.app, d).is_err(), "OK requires at least one body");
    assert_eq!(h.app.session.doc, before);
    h.step(&json!({"widget":crate::i18n::tr("ui.select_all")})).unwrap();
    let commands = apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).unwrap();
    assert_eq!(commands, vec![("component.from_bodies".into(), json!({"bodies":names}))]);
    if let Ok(path) = std::env::var("SKETCRAFT_COMPONENTS_DIALOG_EVIDENCE") {
        let ctx = crate::fonts::test_context();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
        let mut textures = Default::default();
        for frame in 0..6 {
            let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(bounds), ..Default::default() }, |ui| {
                h.app.logic(ui.ctx());
                h.app.ui(ui);
            });
            crate::ui_kit::test_textures_update(&mut textures, &output.textures_delta);
            output.textures_delta.clear();
            if frame == 5 {
                crate::ui_kit::test_raster(&ctx, output.shapes, output.pixels_per_point, bounds, &textures).save(&path).unwrap();
            }
        }
    }
    h.step(&json!({"widget":"Cancel"})).unwrap();
    assert!(h.app.dialog.is_none());
    assert_eq!(h.app.session.doc, before, "Cancel makes no components");
    // Existing body picks seed the same standard multi-selection input.
    h.app.session.selection = vec![Sel::Body { name: names[0].clone() }];
    h.step(&json!({"start":"component.from_bodies"})).unwrap();
    let d = h.app.dialog.as_ref().unwrap();
    assert_eq!(d.inputs[0].items.body_names(), vec![names[0].clone()]);
    h.step(&json!({"widget":crate::i18n::tr("ui.select_all")})).unwrap();
    h.step(&json!({"widget":"OK"})).unwrap();
    assert!(h.app.dialog.is_none());
    for name in &names {
        let component = h.app.session.doc.find_component(name).expect("one component per body");
        assert_eq!(h.app.session.doc.model_body_component(h.app.session.model.state().body(name).unwrap()), component);
    }
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.doc, before, "the dialog applies one undoable command");
}

/// Compute in Interference intersects and meshes each overlap on the kernel's workers. On the
/// window thread (a 1 MiB stack on Windows) the kernel ran out of stack and the app closed
/// (#11483); here the dialog's own thread has 64 KiB.
#[test]
fn interference_meshes_each_overlap_on_the_kernels_stack() {
    let mut s = Session::default();
    crate::test_support::grounded_boxes(&[("A", 0), ("B", 10)], |id, p| s.execute(id, &p).unwrap());
    let r = s.execute("inspect.clearance", &json!({"clearance": 0, "nearest": 1})).unwrap();
    assert_eq!(r["overlaps"].as_array().map(Vec::len), Some(1), "{r}");
    let st = s.world_state().unwrap();
    let meshes = std::thread::Builder::new().stack_size(64 << 10).spawn(move || overlap_meshes(&st, &r)).unwrap().join().unwrap();
    // A's 0..20 and B's 10..30 along X share 10..20.
    let b = meshes.first().map(Mesh::bounds).expect("the shared volume");
    assert_eq!(meshes.len(), 1);
    let near = |p: Vec3, q: [f64; 3]| (p.x - q[0]).abs() < 1e-6 && (p.y - q[1]).abs() < 1e-6 && (p.z - q[2]).abs() < 1e-6;
    assert!(near(b.min, [10.0, 0.0, 0.0]) && near(b.max, [20.0, 20.0, 10.0]), "{b:?}");
}

/// With the app running commands off the window thread, Compute in Interference checks on a
/// thread of its own (a slow pair froze the window until the app was closed, #11483): the
/// overlap and its shared volume arrive in a later frame, and a check of a design changed
/// meanwhile is dropped.
#[test]
fn interference_checks_off_the_window_thread_and_drops_a_check_of_a_changed_design() {
    let mut s = Session::default();
    crate::test_support::grounded_boxes(&[("A", 0), ("B", 10)], |id, p| s.execute(id, &p).unwrap());
    let mut app = SketApp::new(s, Services::default());
    app.apply_off_thread = Some(std::time::Duration::ZERO);
    app.start("inspect.interference");
    let ctx = crate::fonts::test_context();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    let state = |app: &SketApp| match app.dialog.as_ref().map(|d| &d.kind) {
        Some(Kind::Assembly(Asm::Interference { result, overlaps, check, .. })) => (result.clone(), overlaps.len(), check.is_some()),
        _ => panic!("the dialog stays open"),
    };
    // Compute as its button starts it, then frames until its reply is read.
    let compute = |app: &mut SketApp, change: bool| {
        let c = interference(app, &[], "0");
        if let Some(Kind::Assembly(Asm::Interference { result, overlaps, check, .. })) = app.dialog.as_mut().map(|d| &mut d.kind) {
            (*result, *overlaps, *check) = (None, Vec::new(), Some(c));
        }
        if change {
            app.run("solid.box", json!({"length": 5, "width": 5, "height": 5, "corner": [100, 0, 0]})).unwrap();
        }
        for _ in 0..3000 {
            let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| crate::dialogs::show(app, ui.ctx()));
            out.textures_delta.clear();
            if !state(app).2 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the check never came back");
    };
    compute(&mut app, false);
    let (result, overlaps, _) = state(&app);
    let result = result.expect("the check's reply");
    assert_eq!(result["overlaps"].as_array().map(Vec::len), Some(1), "{result}");
    assert_eq!(overlaps, 1, "the shared volume, drawn in red");
    compute(&mut app, true);
    assert_eq!(state(&app), (None, 0, false), "a check of the design before the change is dropped");
}
