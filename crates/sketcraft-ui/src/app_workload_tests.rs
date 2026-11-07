//! Benchmarks, GPU snapshots and every-tool robustness sweeps.
use super::*;

#[test]
#[ignore = "Requires a GPU adapter; writes target/qa/*.png for visual inspection"]
fn gpu_renders_empty_and_modeled_workspace() {
    render_snapshots(gpu_test_state(), false);
}

#[test]
#[ignore = "Requires a GPU adapter; renders and compares every display setting independently"]
fn gpu_renders_individual_display_options() {
    render_snapshots(gpu_test_state(), true);
}

/// Repeatable CPU workload; run explicitly to compare interaction costs.
#[test]
#[ignore]
fn pointer_workload_benchmark() {
    use std::time::Instant;
    for count in [10, 100] {
        let mut h = Harness::new();
        for i in 0..count {
            let x = (i % 10) as f64 * 1.2 - 6.;
            let y = (i / 10) as f64 * 1.2 - 6.;
            let points = [
                DVec3::new(x, y, 0.),
                DVec3::new(x + 1., y, 0.),
                DVec3::new(x + 1., y + 1., 0.),
                DVec3::new(x, y + 1., 0.),
            ];
            for j in 0..4 {
                h.app
                    .doc
                    .model
                    .add_edge(points[j], points[(j + 1) % 4])
                    .unwrap();
            }
        }
        h.app
            .doc
            .camera
            .fit(h.app.doc.model.bounds(), dv(h.app.viewport.size()));
        h.app.set_tool(Tool::Line);
        let positions: Vec<_> = (0..100)
            .map(|i| {
                let t = i as f32 / 100.;
                h.app.viewport.min + h.app.viewport.size() * egui::vec2(0.1 + t * 0.8, 0.37)
            })
            .collect();
        let start = Instant::now();
        for &pos in &positions {
            h.app.update_pointer(pos);
            std::hint::black_box(h.app.snap);
        }
        let pointer = start.elapsed();
        let start = Instant::now();
        for &pos in &positions {
            std::hint::black_box(
                h.frame(vec![egui::Event::PointerMoved(pos)], egui::Modifiers::NONE),
            );
        }
        eprintln!(
            "{count} faces / {} points: pointer {:?}/event, UI {:?}/frame",
            h.app.doc.model.vertices.len(),
            pointer / 100,
            start.elapsed() / 100
        );
    }
}

/// Exercise imported topology without timing its construction or triangulation.
#[test]
#[ignore = "Explicit large-model CPU benchmark; timings depend on hardware"]
fn imported_model_workload_benchmark() {
    use std::time::Instant;
    let mut h = Harness::new();
    h.app.doc.model = crate::picking::tests::triangle_grid(4096);
    h.app.fit(&h.ctx);
    h.frame(vec![], egui::Modifiers::NONE);
    for tool in [Tool::Select, Tool::Line, Tool::Paint, Tool::Orbit] {
        h.app.set_tool(tool);
        let start = Instant::now();
        for i in 0..20 {
            let point = h.app.viewport.min
                + h.app.viewport.size() * egui::vec2(0.2 + i as f32 * 0.025, 0.47);
            std::hint::black_box(h.frame(
                vec![egui::Event::PointerMoved(point)],
                egui::Modifiers::NONE,
            ));
        }
        eprintln!(
            "4096 imported faces, {tool:?}: {:?}/frame",
            start.elapsed() / 20
        );
    }
}

#[test]
#[ignore = "Explicit axes on/off comparison on a grouped import"]
fn axes_workload_benchmark() {
    use std::time::Instant;
    let mut h = Harness::new();
    let mut json = serde_json::to_value(crate::picking::tests::triangle_grid(4096)).unwrap();
    // Imported assemblies commonly have many groups; avoid constructing them
    // with interactive operations so setup is separate from measured frames.
    for i in 0..512u64 {
        let id = 30000 + i;
        let children: Vec<_> = (i * 8..i * 8 + 8)
            .flat_map(|triangle| (4..=7).map(move |entity| triangle * 7 + entity))
            .collect();
        json["objects"][id.to_string()] = serde_json::json!({
            "id": id, "name": "Tile", "kind": "Group", "entities": children
        });
    }
    json["next_id"] = 30512.into();
    h.app.doc.model = serde_json::from_value(json).unwrap();
    h.app.doc.model.validate().unwrap();
    h.app.set_tool(Tool::Orbit);
    h.app.fit(&h.ctx);
    for axes in [false, true] {
        h.app.doc.display.axes = axes;
        h.frame(vec![], egui::Modifiers::NONE);
        let start = Instant::now();
        for _ in 0..10 {
            std::hint::black_box(h.frame(vec![], egui::Modifiers::NONE));
        }
        eprintln!(
            "4096 faces / 512 groups, axes={axes}: {:?}/frame",
            start.elapsed() / 10
        );
    }
}

#[test]
fn every_tool_survives_interrupted_gestures_resize_and_focus_loss() {
    for populated in [false, true] {
        for tool in Tool::ALL {
            let mut h = Harness::new();
            if populated {
                ui_solid_pair(&mut h);
            }
            h.app.set_tool(tool);
            let a = h.app.viewport.center();
            let b = a + egui::vec2(31., -19.);
            h.pointer(a, egui::PointerButton::Primary, true, false);
            h.frame(vec![egui::Event::PointerMoved(b)], egui::Modifiers::NONE);
            h.frame(
                vec![egui::Event::WindowFocused(false)],
                egui::Modifiers::NONE,
            );
            h.frame(vec![egui::Event::PointerGone], egui::Modifiers::NONE);
            h.pointer(b, egui::PointerButton::Primary, false, false);
            h.key(Key::Escape);
            h.frame(
                vec![egui::Event::WindowFocused(true)],
                egui::Modifiers::NONE,
            );
            h.pointer(a, egui::PointerButton::Middle, true, false);
            h.frame(vec![egui::Event::PointerMoved(b)], egui::Modifiers::SHIFT);
            h.pointer(b, egui::PointerButton::Middle, false, false);
            for size in [egui::vec2(800., 600.), egui::vec2(1440., 900.)] {
                h.size = size;
                h.frame(vec![], egui::Modifiers::NONE);
            }
            h.app.set_tool(Tool::Select);
            h.app
                .doc
                .model
                .validate()
                .unwrap_or_else(|error| panic!("{tool:?}, populated={populated}: {error}"));
            assert!(h.app.doc.camera.valid(), "{tool:?}, populated={populated}");
            assert!(h.app.gesture.is_none(), "{tool:?}, populated={populated}");
            let json = serde_json::to_string(&h.app.doc).unwrap();
            Document::from_json(&json).unwrap();
        }
    }
}

#[test]
fn display_styles_and_effects_survive_camera_changes_and_round_trip() {
    let mut h = Harness::new();
    ui_solid_pair(&mut h);
    for style in ketchup_core::FaceStyle::ALL {
        for effects in [false, true] {
            let d = &mut h.app.doc.display;
            d.face_style = style;
            d.x_ray = effects;
            d.hidden_geometry = effects;
            d.hidden_objects = effects;
            d.back_edges = effects;
            d.profiles = effects;
            d.depth_cue = effects;
            d.endpoints = effects;
            d.extensions = effects;
            d.color_by_tag = effects;
            d.section_planes = effects;
            d.section_cuts = effects;
            d.shadows = effects;
            d.fog = effects;
            for pitch in [-1.5, 0., 1.5] {
                h.app.doc.camera.pitch = pitch;
                h.frame(vec![], egui::Modifiers::NONE);
                assert!(h.app.doc.camera.valid());
            }
            let restored =
                Document::from_json(&serde_json::to_string(&h.app.doc).unwrap()).unwrap();
            assert_eq!(restored.display, h.app.doc.display);
            restored.model.validate().unwrap();
        }
    }
}

/// Measure Move separately from drawing/picking; timings exclude model creation.
#[test]
#[ignore]
fn move_workload_benchmark() {
    use std::time::Instant;
    for count in [10, 100] {
        let mut h = Harness::new();
        for i in 0..count {
            let x = (i % 10) as f64 * 1.2 - 6.;
            let y = (i / 10) as f64 * 1.2 - 6.;
            let points = [
                DVec3::new(x, y, 0.),
                DVec3::new(x + 1., y, 0.),
                DVec3::new(x + 1., y + 1., 0.),
                DVec3::new(x, y + 1., 0.),
            ];
            let mut edges = BTreeSet::new();
            for j in 0..4 {
                edges.insert(
                    h.app
                        .doc
                        .model
                        .add_edge(points[j], points[(j + 1) % 4])
                        .unwrap(),
                );
            }
            h.app.doc.model.make_object(&edges, false, "Tile").unwrap();
        }
        h.app
            .doc
            .camera
            .fit(h.app.doc.model.bounds(), dv(h.app.viewport.size()));
        h.app.set_tool(Tool::Move);
        h.app.selected = h.app.doc.model.objects.keys().take(1).copied().collect();
        h.app.begin_modeling(h.at(DVec3::new(-6., -6., 0.)), &h.ctx);
        assert!(h.app.modeling.is_some());
        h.app.modeling.as_mut().unwrap().direction = DVec3::X;
        let positions: Vec<_> = (0..40)
            .map(|i| {
                h.app.viewport.min + h.app.viewport.size() * egui::vec2(0.15 + i as f32 / 60., 0.37)
            })
            .collect();
        let edit = h.app.modeling.as_ref().unwrap();
        let start = Instant::now();
        for &pos in &positions {
            std::hint::black_box(infer_move_with_snapping(
                &edit.base,
                &h.app.doc.camera,
                dv(h.app.viewport.size()),
                dv(pos - h.app.viewport.min),
                edit.anchor,
                None,
                true,
                true,
            ));
        }
        let inference = start.elapsed() / 40;
        let start = Instant::now();
        for i in 0..40 {
            std::hint::black_box(
                KetchupApp::modeling_result(Tool::Move, edit, 0.1 + i as f64 * 0.01).unwrap(),
            );
        }
        let transform = start.elapsed() / 40;
        let start = Instant::now();
        for &pos in &positions {
            h.app.update_modeling(pos);
        }
        eprintln!("Prepared Move update: {:?}/event", start.elapsed() / 40);
        let start = Instant::now();
        for &pos in &positions {
            std::hint::black_box(
                h.frame(vec![egui::Event::PointerMoved(pos)], egui::Modifiers::NONE),
            );
        }
        eprintln!(
            "Move {count} objects / {} points: uncached inference {inference:?}, validated transform {transform:?}, UI {:?}/frame",
            h.app.doc.model.vertices.len(),
            start.elapsed() / 40
        );
    }
}
