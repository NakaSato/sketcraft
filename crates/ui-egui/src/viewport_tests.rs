use super::*;
use crate::navigation::*;

/// A component dragged with its Move triad is drawn as a layer: its bodies' scene is built
/// once, where the drag found them, and drawn moved by the drag's motion, the rest of the
/// model scene stays as it was (with the ground its ambient occlusion darkens put together
/// again under the dragged bodies, but in Wireframe, which has none), and a step of the drag
/// builds and moves nothing. The ground shadow follows too, from the boxes the layer knows.
/// Captured, the model scene is whole again.
#[test]
fn a_dragged_component_is_drawn_as_a_layer_built_once() {
    for wire in [false, true] {
        let mut s = sketcraft_engine::Session::default();
        s.execute("solid.box", &json!({"length": 5, "width": 5, "height": 5, "body_name": "Stays"})).unwrap();
        let first = s.execute("component.create", &json!({"name": "Part"})).unwrap()["occurrence"].as_u64().unwrap();
        s.execute("solid.box", &json!({"length": 2, "width": 3, "height": 4, "body_name": "Block", "operation": "new"})).unwrap();
        let dragged = s.execute("occurrence.copy", &json!({"occurrence": first})).unwrap()["occurrence"].as_u64().unwrap();
        s.execute("occurrence.move", &json!({"occurrence": dragged, "translate": [20, 0, 0]})).unwrap();
        let mut app = SolveApp::new(s, crate::Services::default());
        app.viewport.gpu =
            Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
        app.ui.visual_style = if wire { 2 } else { 0 };
        app.ui.ground_shadow = true;
        app.cam.pitch = 0.5;
        let whole = build_scene(&app);
        drag_layer::whole_scene_built(&mut app);
        // What the scene draws of its bodies (a body outside the active component is faded),
        // and its ground.
        let drawn = |sc: GpuScene| {
            let ground = sc.ground_ao;
            let sc = sc.merged();
            (sc.tris.len() + sc.lines.len(), sc.trans, ground)
        };
        let shadow = |app: &SolveApp| shadow_geometry(app, &crate::open_meshing::shown_world(app)).bytes;
        let mut last_shadow = shadow(&app);
        let (whole, whole_trans, _) = drawn(whole);
        let base = app.session.doc.occurrences.iter().find(|o| o.id == dragged).map(|o| o.transform).unwrap();
        let found = body_bounds(&app, &app.session.world_state().unwrap());
        let taken = |slot: &SceneSlot| slot.lock().unwrap().take();
        let mut first_step = None;
        for step in 1..4 {
            let x = 3.0 * f64::from(step);
            app.run("occurrence.move", json!({"occurrence": dragged, "base_transform": base, "translate": [x, 0, 0], "capture": false})).unwrap();
            assert!(drag_layer::follow(&mut app), "step {step}: drawn as a layer");
            let moving = drag_layer::scenes(&mut app).unwrap();
            assert_eq!(moving.transform[3][..3], [x as f32, 0.0, 0.0], "step {step}: drawn moved by the drag");
            let now_shadow = shadow(&app);
            assert!(now_shadow.len() == last_shadow.len() && now_shadow != last_shadow, "step {step}: the shadow follows");
            last_shadow = now_shadow;
            let (statics, layer) = (taken(&app.viewport.slot), taken(&moving.slot));
            match (step, statics) {
                (1, Some(statics)) => {
                    // The dragged body is the layer's, the others the model scene's.
                    let ((statics, statics_trans, statics_ground), (layer, layer_trans, layer_ground)) = (drawn(statics), drawn(layer.unwrap()));
                    assert!(statics > 0 && layer > 0 && statics + layer == whole, "{statics} + {layer} of {whole}");
                    assert_eq!(statics_trans.len() + layer_trans.len(), whole_trans.len());
                    assert!(layer_ground.is_none(), "the ground is the model scene's");
                    assert_eq!(statics_ground.is_some(), !wire, "Wireframe has no ground");
                    first_step = Some((app.viewport.key, moving.key, statics_ground));
                }
                (_, statics) => {
                    let Some((key, moving_key, first_ground)) = &first_step else { panic!("step {step}: no first step") };
                    assert!(layer.is_none() && *moving_key == moving.key, "step {step}: the dragged bodies not built again");
                    match statics {
                        // Only the ground moved.
                        Some(statics) if !wire => {
                            let (_, _, now) = drawn(statics);
                            assert!(now.is_some() && now != *first_ground, "step {step}: the ground follows");
                        }
                        None if wire => assert_eq!(app.viewport.key, *key, "step {step}"),
                        _ => panic!("step {step}: the model scene built again"),
                    }
                }
            }
            // Its mesh is not moved to draw it.
            let world = app.session.world_state().unwrap();
            let unmoved: Vec<usize> = (0..world.bodies.len()).filter(|&i| world.bodies[i].cached_mesh().is_none()).collect();
            assert_eq!(unmoved.len(), 1, "step {step}");
            // Nor its box worked out from a mesh: it is the box the drag found it in, moved.
            let known = app.viewport.pick_bounds.lock().unwrap().as_ref().is_some_and(|(st, _)| std::sync::Arc::ptr_eq(st, &world));
            assert!(known, "step {step}: the boxes come with the layer");
            let i = unmoved[0];
            let (was, now) = (found[i].1, body_bounds(&app, &world)[i].1);
            assert!(
                (now.min - was.min - Vec3::new(x, 0.0, 0.0)).len() < 1e-9 && (now.max - was.max - Vec3::new(x, 0.0, 0.0)).len() < 1e-9,
                "step {step}"
            );
        }
        // Captured: no longer a drag, and the model scene is whole again.
        app.run("component.capture_position", json!({})).unwrap();
        assert!(!drag_layer::follow(&mut app), "captured: no longer a drag");
        assert!(app.viewport.drag.is_none());
    }
}

/// A drag that comes back to where its layer found the component (the triad snapped back, or
/// dragged back to where it began) draws the same layer, unmoved. Built again with nothing
/// dragged, it would put every dragged body back in the model scene (the GPU uploading them
/// again) and take them once more at the next step.
#[test]
fn a_drag_back_where_it_began_keeps_its_layer() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 5, "width": 5, "height": 5, "body_name": "Stays"})).unwrap();
    let first = s.execute("component.create", &json!({"name": "Part"})).unwrap()["occurrence"].as_u64().unwrap();
    s.execute("solid.box", &json!({"length": 2, "width": 3, "height": 4, "body_name": "Block", "operation": "new"})).unwrap();
    let dragged = s.execute("occurrence.copy", &json!({"occurrence": first})).unwrap()["occurrence"].as_u64().unwrap();
    s.execute("occurrence.move", &json!({"occurrence": dragged, "translate": [20, 0, 0]})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    let _ = build_scene(&app);
    drag_layer::whole_scene_built(&mut app);
    let base = app.session.doc.occurrences.iter().find(|o| o.id == dragged).map(|o| o.transform).unwrap();
    let step = |app: &mut SolveApp, x: f64| {
        app.run("occurrence.move", json!({"occurrence": dragged, "base_transform": base, "translate": [x, 0, 0], "capture": false})).unwrap();
        drag_layer::follow(app).then(|| drag_layer::scenes(app)).flatten()
    };
    // A drag that has not moved anything yet is drawn as before.
    assert!(step(&mut app, 0.0).is_none(), "nothing dragged yet: no layer");
    let _ = build_scene(&app);
    drag_layer::whole_scene_built(&mut app);
    let taken = |slot: &SceneSlot| slot.lock().unwrap().take();
    let first = step(&mut app, 3.0).expect("drawn as a layer");
    assert!(taken(&first.slot).is_some_and(|sc| !sc.blocks.is_empty()), "the dragged body's scene");
    let statics = taken(&app.viewport.slot).expect("the model scene without it");
    let blocks = |sc: &GpuScene| sc.blocks.iter().map(|b| (b.key, std::sync::Arc::as_ptr(&b.bytes))).collect::<Vec<_>>();
    for x in [0.0, 6.0, 0.0, 9.0, 0.0] {
        let moving = step(&mut app, x).expect("drawn as a layer");
        assert_eq!(moving.transform[3][..3], [x as f32, 0.0, 0.0], "x {x}: drawn moved by the drag");
        assert!(moving.key == first.key && taken(&moving.slot).is_none(), "x {x}: the dragged body's scene not built again");
        // The model scene is not built again: only its ground follows the dragged body.
        let model = taken(&app.viewport.slot).expect("x {x}: the ground moved");
        assert!(blocks(&model) == blocks(&statics) && model.ground_ao != statics.ground_ao, "x {x}: the model scene not built again");
    }
}

#[test]
fn raised_bodies_cast_ground_shadows_at_the_origin() -> Result<(), Box<dyn std::error::Error>> {
    use sketcraft_engine::render::UpAxis;
    for (up, translate, coordinate) in [(UpAxis::Z, [0, 0, 13], 2), (UpAxis::Y, [0, 13, 0], 1)] {
        let mut session = sketcraft_engine::Session::default();
        session.execute("solid.box", &json!({"length": 12, "width": 12, "height": 30, "body_name": "Raised"}))?;
        session.execute("solid.move", &json!({"bodies": ["Raised"], "translate": translate}))?;
        let mut app = SolveApp::new(session, crate::Services::default());
        app.cam.up = up;
        let mut scene = GpuScene::default();
        ground_shadow(&mut scene, &app, ground_boxes(&app, &app.session.world_state()?));
        assert!(!scene.shadow.is_empty());
        for vertex in scene.shadow.as_chunks::<{ crate::gpu::TRI_SIZE }>().0 {
            let height = f32::from_le_bytes(vertex[coordinate * 4..coordinate * 4 + 4].try_into()?);
            assert!((-0.1..=0.0).contains(&height), "{up:?}: shadow must stay just below the origin ground plane, got {height}");
        }
    }
    Ok(())
}

#[test]
fn ground_shadow_keeps_legacy_placement_and_stays_below_low_bodies() -> Result<(), Box<dyn std::error::Error>> {
    use sketcraft_engine::render::UpAxis;
    for (up, coordinate) in [(UpAxis::Z, 2), (UpAxis::Y, 1)] {
        for (height, follows_model, expected) in [(13, true, 13.0), (-7, false, -7.0), (-7, true, -7.0), (0, false, 0.0)] {
            let mut session = sketcraft_engine::Session::default();
            session.execute("solid.box", &json!({"length": 12, "width": 12, "height": 30, "body_name": "Box"}))?;
            let mut translate = [0; 3];
            translate[coordinate] = height;
            session.execute("solid.move", &json!({"bodies": ["Box"], "translate": translate}))?;
            let mut app = SolveApp::new(session, crate::Services::default());
            app.cam.up = up;
            let state = app.session.world_state()?;
            let proj = projection(&app, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)));
            let model_key = scene_key(&app);
            let view_key = sketch_scene_key(&app, &proj);
            let before = shadow_geometry(&app, &state);
            crate::prefs::set(&mut app, &json!({"ground_shadow_follows_model": true}))?;
            assert_ne!(scene_key(&app), model_key, "the cached ground AO receiver is refreshed");
            assert_ne!(sketch_scene_key(&app, &proj), view_key, "the visible shadow is refreshed");
            let after = shadow_geometry(&app, &state);
            assert!(!std::sync::Arc::ptr_eq(&before.bytes, &after.bytes), "changing placement rebuilds the shadow");
            assert!(std::sync::Arc::ptr_eq(&after.bytes, &shadow_geometry(&app, &state).bytes), "unchanged placement stays cached");
            crate::prefs::set(&mut app, &json!({"ground_shadow_follows_model": follows_model}))?;
            let mut scene = GpuScene::default();
            ground_shadow(&mut scene, &app, ground_boxes(&app, &app.session.world_state()?));
            assert!(!scene.shadow.is_empty());
            for vertex in scene.shadow.as_chunks::<{ crate::gpu::TRI_SIZE }>().0 {
                let actual = f32::from_le_bytes(vertex[coordinate * 4..coordinate * 4 + 4].try_into()?);
                assert!(
                    (expected - 0.1..=expected).contains(&actual),
                    "{up:?}, follows model {follows_model}: expected {expected}, got {actual}"
                );
            }
            app.session.visibility.hidden_bodies.push("Box".into());
            let mut hidden = GpuScene::default();
            ground_shadow(&mut hidden, &app, ground_boxes(&app, &app.session.world_state()?));
            assert!(hidden.shadow.is_empty(), "hidden bodies cast no shadow");
        }
    }
    Ok(())
}

#[test]
fn ground_shadow_does_not_enter_see_through_geometry() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = sketcraft_engine::Session::default();
    session.execute("solid.box", &json!({"length": 30, "width": 30, "height": 10}))?;
    let app = SolveApp::new(session, crate::Services::default());
    let mut scene = GpuScene::default();
    ground_shadow(&mut scene, &app, ground_boxes(&app, &crate::open_meshing::session_world(&app.session)));
    assert!(scene.trans.is_empty(), "the ground shadow must not trigger order-independent transparency");
    assert_eq!(scene.shadow.len(), 48 * 5 * 3 * crate::gpu::TRI_SIZE, "all shadow rings are retained");
    let shadow_alpha = |v: &[u8]| {
        let i = u32::from_le_bytes(v.get(24..28)?.try_into().ok()?);
        scene.palette.get(crate::gpu::PaletteIndex(i)).map(|c| c[3])
    };
    assert!(scene.shadow.as_chunks::<{ crate::gpu::TRI_SIZE }>().0.iter().any(|v| shadow_alpha(v).is_some_and(|a| a > 0)));
    Ok(())
}

/// Two bodies apart get a shadow each, and the gap between them gets none of the darkest
/// middle (#10442); bodies that meet keep sharing one.
#[test]
fn ground_shadow_leaves_the_gap_between_bodies_apart() -> Result<(), Box<dyn std::error::Error>> {
    use sketcraft_engine::geom::Aabb3;
    let block = |x0: f64| Aabb3 { min: Vec3::new(x0, -30.0, 0.0), max: Vec3::new(x0 + 40.0, 30.0, 20.0) };
    let (left, right) = (block(-60.0), block(20.0));
    let groups = shadow_groups(vec![left, right]);
    assert_eq!(groups, vec![left, right], "bodies 40 apart cast a shadow each");
    let gap = Vec3::new(0.0, 0.0, 0.0);
    for g in &groups {
        let ((rx, ry), c) = (shadow_radii(g), g.center());
        let r = (((gap.x - c.x) / rx).powi(2) + ((gap.y - c.y) / ry).powi(2)).sqrt();
        assert!(r > 0.85, "the gap's middle is in a shadow's faint rim at most, not its dark middle: {r}");
    }
    // Touching, and a third body reaching both through a merged box: one shadow.
    assert_eq!(shadow_groups(vec![block(-40.0), block(0.0)]).len(), 1);
    let bridge = Aabb3 { min: Vec3::new(-25.0, 25.0, 0.0), max: Vec3::new(25.0, 40.0, 5.0) };
    assert_eq!(shadow_groups(vec![left, right, bridge]).len(), 1, "a body touching both joins them");
    // A crowd falls back to one shadow under it all.
    let crowd: Vec<_> = (0..100).map(|i| block(f64::from(i) * 100.0)).collect();
    assert_eq!(shadow_groups(crowd).len(), 1);

    let mut session = sketcraft_engine::Session::default();
    for x in [-60, 20] {
        session.execute("solid.box", &json!({"length": 40, "width": 60, "height": 20, "corner": [x, -30, 0], "operation": "new"}))?;
    }
    let app = SolveApp::new(session, crate::Services::default());
    let mut scene = GpuScene::default();
    ground_shadow(&mut scene, &app, ground_boxes(&app, &app.session.world_state()?));
    assert_eq!(scene.shadow.len(), 2 * 48 * 5 * 3 * crate::gpu::TRI_SIZE, "one ellipse under each body");
    Ok(())
}

/// The origin point hides behind bodies under the same rule as the axes and planes (#10050):
/// a cylinder standing on the origin hides the dot while the through-bodies preference is
/// off, and point, axes and planes come through together with the preference on or while a
/// dialog waits for a plane.
#[test]
fn origin_point_hides_behind_bodies_like_the_axes_and_planes() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.cylinder", &json!({"radius": 20, "height": 40})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(800., 600.));
    let proj = projection(&app, rect);
    assert!(origin_point_shown(&app), "the Origin is shown by default");
    assert!(origin_hidden_by_body(&app, &proj), "the cylinder covering the origin hides the point");
    assert!(build_highlight(&app).overlay.is_empty(), "the axes and planes are depth-tested while the point is hidden");
    // The preference brings point, axes and planes through together (#2697).
    crate::prefs::set(&mut app, &json!({"reference_geometry_through_bodies": true})).unwrap();
    assert!(!origin_hidden_by_body(&app, &proj), "with the preference the point shows through the body");
    assert!(!build_highlight(&app).overlay.is_empty(), "with the preference the axes and planes draw through");
    crate::prefs::set(&mut app, &json!({"reference_geometry_through_bodies": false})).unwrap();
    // A dialog waiting for a plane lifts all three as well (#2618).
    app.dialog = Some(crate::dialogs::Dialog::for_command(&app, "sketch.create").unwrap());
    assert!(app.picking_plane());
    assert!(!origin_hidden_by_body(&app, &proj), "while a dialog picks a plane the point shows through");
    app.dialog = None;
    // Wireframe draws no faces, so nothing hides the point, as for vertices and edges.
    app.ui.visual_style = 2;
    assert!(!origin_hidden_by_body(&app, &proj), "wireframe hides nothing");
    app.ui.visual_style = 0;
    // A body away from the origin leaves the point drawn.
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 20, "width": 20, "height": 20, "center": [200.0, 200.0, 200.0]})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    let proj = projection(&app, rect);
    assert!(!origin_hidden_by_body(&app, &proj), "a body away from the origin does not cover it");
}

#[test]
fn straight_3d_wire_is_offered_as_an_axis_but_spline_is_refused() {
    for (command, points, straight) in [
        ("sketch.line3d", json!([[10, 20, 30], [30, 20, 30]]), true),
        ("sketch.spline3d", json!([[10, 20, 30], [20, 25, 40], [30, 20, 30]]), false),
        ("sketch.spline3d", json!([[10, 20, 30], [20, 20, 30], [30, 20, 30]]), false),
    ] {
        let mut s = sketcraft_engine::Session::default();
        s.execute("sketch.create", &json!({"plane":"XZ", "offset":7})).unwrap();
        s.execute("sketch.options", &json!({"sketch_3d":true})).unwrap();
        let id = s.execute(command, &json!({"points":points})).unwrap()["curves"][0].as_str().unwrap().to_string();
        s.execute("sketch.finish", &json!({})).unwrap();
        let mut app = SolveApp::new(s, crate::Services::default());
        app.fit_view();
        let st = crate::open_meshing::session_world(&app.session);
        let wire = &st.sketches[0].sketch.wires[0];
        let at = (wire.pts[0] + wire.pts[1]) * 0.5;
        let proj = projection(&app, Rect::from_min_size(Pos2::ZERO, vec2(800., 600.)));
        let hits = pick_all(&app, &proj, proj.to_screen(at).unwrap());
        let hit = hits.iter().find(|h| matches!(h, Hit::SketchCurve { id: picked, .. } if picked == &id)).unwrap();
        let axis = crate::selection::SelInput::new("Axis", crate::selection::AXES, false);
        assert_eq!(axis.accepts(hit, None, |_, _| false).is_some(), straight, "{hit:?}");
        let selected = Sel::SketchCurve { id, sketch: None };
        assert_eq!(crate::dialogs::axis_of(&app, &selected).is_some(), straight);
    }
}

#[test]
fn plane_at_angle_uses_3d_wire_endpoints_in_model_space() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("sketch.create", &json!({"plane":"XZ", "offset":7})).unwrap();
    s.execute("sketch.options", &json!({"sketch_3d":true})).unwrap();
    let id = s.execute("sketch.line3d", &json!({"points":[[10,20,30],[30,20,30]]})).unwrap()["curves"][0].as_str().unwrap().to_string();
    s.execute("sketch.finish", &json!({})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    let selected = Sel::SketchCurve { id, sketch: None };
    let (origin, dir) = crate::dialogs::axis_of(&app, &selected).unwrap();
    assert!(origin.dist(Vec3::new(10., 20., 30.)) < 1e-9);
    assert!(dir.dist(Vec3::X) < 1e-9);
    let mut dialog = crate::dialogs::Dialog::for_command(&app, "construct.plane.at_angle").unwrap();
    dialog.inputs[0].items = vec![Sel::Plane { name: "XY".into() }];
    dialog.inputs[1].items = vec![selected];
    if let crate::dialogs::Kind::AnglePlane { angle, .. } = &mut dialog.kind {
        *angle = "90 deg".into();
    }
    for (command, params) in crate::dialogs::apply_commands(&app, &dialog).unwrap() {
        app.run(&command, params).unwrap();
    }
    let planes = sketcraft_engine::view::construction_planes(&app.session);
    let plane = &planes.last().unwrap().2;
    assert!(plane.normal().dist(-Vec3::Y) < 1e-9);
    assert!(plane.origin.dist(Vec3::new(0., 50., 10.)) < 1e-9, "{plane:?}");
}

#[test]
fn point_pick_tie_at_origin_prefers_the_drawn_point_after_deleting_coincident() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    let drawn = s.execute("sketch.point", &json!({"point": [0, 0], "id": "drawn"})).unwrap();
    let constraint = drawn["constraints"][0].as_str().unwrap();
    s.execute("sketch.delete", &json!({"entities": [constraint]})).unwrap();
    let sid = s.active_sketch.unwrap();
    assert!(s.doc.sketch(sid).unwrap().constraints.is_empty());
    let mut app = SolveApp::new(s, crate::Services::default());
    let proj = projection(&app, Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)));
    let at = proj.to_screen(Vec3::ZERO).unwrap();
    assert!(app.preferences.prefer_free_sketch_points);
    assert_eq!(sketch_point_at(&app, &proj, at).unwrap().1.as_deref(), Some("origin"), "drawing keeps its origin snap");
    let hits = pick(&app, &proj, at);
    assert!(matches!(hits.first(), Some(Hit::SketchPoint { id, .. }) if id == "drawn"), "{hits:?}");
    let all = pick_all(&app, &proj, at);
    let points: Vec<_> = all
        .iter()
        .filter_map(|h| match h {
            Hit::SketchPoint { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(points, ["drawn", "origin"], "Select Other keeps both points");
    assert!(matches!(pick_cached(&mut app, &proj, at).first(), Some(Hit::SketchPoint { id, .. }) if id == "drawn"));
    let saved = crate::prefs::set(&mut app, &json!({"prefer_free_sketch_points": false})).unwrap();
    assert_eq!(saved["prefer_free_sketch_points"], false);
    assert!(matches!(pick_cached(&mut app, &proj, at).first(), Some(Hit::SketchPoint { id, .. }) if id == "origin"));
    assert!(matches!(pick_all(&app, &proj, at).first(), Some(Hit::SketchPoint { id, .. }) if id == "origin"));
    let Some(Hit::SketchPoint { id, .. }) = hits.first() else { panic!("no point picked") };
    app.run("sketch.drag", json!({"entity": id, "from": [0, 0], "to": [10, 5]})).unwrap();
    let sk = app.session.doc.sketch(sid).unwrap();
    assert!(sk.point(sk.resolve_point("drawn").unwrap()).unwrap().dist(Vec2::new(10.0, 5.0)) < 1e-6);
    assert_eq!(sk.point(0), Some(Vec2::ZERO));
}

/// Every tangent seam Continuity Analysis reports keeps its colour while the pointer rests on
/// a face (#1247): a 60 x 40 x 20 box with its four top edges rounded (6 mm) has eight, four
/// against the sides and four against the top face. The tool lit the face under the pointer,
/// though its click takes only edges, and the face's outline covered the colours of its
/// edges. Rendered offscreen with bodies built on the CPU and evaluated on the GPU (skipped
/// without a GPU adapter); with `SKETCRAFT_CONTINUITY_TOP_SHOT` set, the pictures are
/// written there (`-cpu.png`, `-gpu.png`).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn continuity_colours_stay_on_the_edges_of_the_face_under_the_pointer() {
    use sketcraft_engine::kernel::SeamVerdict;
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 60, "width": 40, "height": 20, "body_name": "B"})).unwrap();
    s.execute("solid.fillet", &json!({"edges": [[30, 0, 20], [30, 40, 20], [0, 20, 20], [60, 20, 20]], "radius": 6})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    // The toolbar button: every seam shaded, the tool left picking edges.
    app.start("inspect.continuity");
    let tangent = |app: &SolveApp| app.session.continuity.iter().filter(|seam| seam.verdict == SeamVerdict::CurvatureJump).count();
    assert_eq!(tangent(&app), 8);
    const SIZE: u32 = 512; // rows of 256-byte multiples (the copy alignment)
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    // The pointer rests in the middle of the top face, away from its edges: the tool takes
    // only edges, so nothing there lights up over the analysis.
    let proj = projection(&app, rect);
    let middle = proj.to_screen(Vec3::new(30.0, 20.0, 20.0)).unwrap();
    let hits = pick_cached(&mut app, &proj, middle);
    assert!(matches!(hits.first(), Some(Hit::Face { .. })), "{hits:?}");
    app.viewport.hover = tool_hover(&app, &hits);
    assert_eq!(app.viewport.hover, None);
    let t = Tokens::get();
    let g1 = t.continuity[1];
    // Each tangent seam lies in one flat face (the top, or a side under its round): that
    // face's outward normal, and the seam shows where it faces the eye (the body is convex).
    let flat_normal = |e: &[Vec3]| {
        let on = |f: fn(&Vec3) -> f64, at: f64| e.iter().all(|p| (f(p) - at).abs() < 1e-6);
        [
            (on(|p| p.z, 20.0), Vec3::Z),
            (on(|p| p.y, 0.0), -Vec3::Y),
            (on(|p| p.y, 40.0), Vec3::Y),
            (on(|p| p.x, 0.0), -Vec3::X),
            (on(|p| p.x, 60.0), Vec3::X),
        ]
        .into_iter()
        .find_map(|(lies, n)| lies.then_some(n))
    };
    for tess in [false, true] {
        app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess, oit: false });
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
        let Some(img) = crate::gpu::render_offscreen(
            build_scene(&app).merged(),
            Some(build_highlight(&app)),
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            tess.then_some(sketcraft_gpu::SHADER),
            [0.0; 8],
            crate::gpu::analysis_colors(&t.analysis),
        ) else {
            eprintln!("skipping: no GPU adapter");
            return;
        };
        if let Ok(path) = std::env::var("SKETCRAFT_CONTINUITY_TOP_SHOT")
            && let Some(png) = image::RgbaImage::from_raw(SIZE, SIZE, img.clone())
        {
            png.save(path.replace(".png", if tess { "-gpu.png" } else { "-cpu.png" })).unwrap();
        }
        let lit = |x: i64, y: i64| {
            (0..SIZE as i64).contains(&x)
                && (0..SIZE as i64).contains(&y)
                && img[((y * SIZE as i64 + x) * 4) as usize..][..3].iter().zip([g1.r(), g1.g(), g1.b()]).all(|(a, b)| a.abs_diff(b) <= 12)
        };
        let mut seen = 0;
        for seam in app.session.continuity.iter().filter(|seam| seam.verdict == SeamVerdict::CurvatureJump) {
            let (a, b) = (seam.points[0], seam.points[seam.points.len() - 1]);
            let normal = flat_normal(&seam.points).unwrap_or_else(|| panic!("a tangent seam off the flat faces: {a:?}..{b:?}"));
            if normal.dot(proj.cam.eye() - a.lerp(b, 0.5)) <= 0.0 {
                continue;
            }
            seen += 1;
            // In its colour a quarter, half and three quarters along.
            for f in [0.25, 0.5, 0.75] {
                let p = a.lerp(b, f);
                let q = proj.to_screen(p).unwrap();
                let (x, y) = (q.x.round() as i64, q.y.round() as i64);
                let drawn = (-2..=2).any(|dy| (-2..=2).any(|dx| lit(x + dx, y + dy)));
                assert!(drawn, "tess {tess}: tangent seam {a:?}..{b:?} is not drawn in its colour at {p:?} ({x}, {y})");
            }
        }
        assert_eq!(seen, 6, "the four top seams and the two sides facing the eye");
    }
    // On an edge, the edge lights up: what a click measures.
    let proj = projection(&app, rect);
    let on_edge = proj.to_screen(Vec3::new(30.0, 6.0, 20.0)).unwrap();
    let hits = pick_cached(&mut app, &proj, on_edge);
    assert!(matches!(tool_hover(&app, &hits), Some(Hit::Edge { .. })), "{hits:?}");
}

/// Continuity Analysis draws each seam in its verdict's `continuity` token (#770): a box
/// with one edge rounded shows the round's two tangent seams (G1) and the creases (G0), and
/// nothing in the smooth or gap colours.
#[test]
fn continuity_seams_take_their_colours_from_the_theme() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 20, "width": 20, "height": 20})).unwrap();
    s.execute("solid.fillet", &json!({"edges": [[20, 20, 10]], "radius": 4})).unwrap();
    s.execute("inspect.continuity", &json!({})).unwrap();
    let app = SolveApp::new(s, crate::Services::default());
    let sc = build_scene(&app);
    // An overlay line is two points, a colour and a width: 32 bytes.
    let colours: Vec<[u8; 4]> = sc
        .overlay
        .as_chunks::<32>()
        .0
        .iter()
        .filter_map(|r| {
            let i = u32::from_le_bytes(r.get(24..28)?.try_into().ok()?);
            sc.palette.get(crate::gpu::PaletteIndex(i))
        })
        .collect();
    let t = Tokens::get();
    let count = |c: Color32| colours.iter().filter(|x| **x == c4(c)).count();
    let [smooth, jump, sharp, gap] = t.continuity.map(count);
    assert!(jump > 0 && sharp > 0, "tangent seams {jump}, creases {sharp} segments");
    assert_eq!((smooth, gap), (0, 0));
}

/// Isocurve Analysis (#770): a face the tool clicks shows its isoparametric curves in the
/// model scene, in the theme's accent and depth tested like edges (never drawn over the
/// model); the button again turns them off.
#[test]
fn isocurves_are_drawn_on_the_faces_and_the_button_turns_them_off() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.cylinder", json!({"radius": 10, "height": 30})).unwrap();
    h.settle();
    // A line is two points, a colour and a width: 32 bytes.
    let accent = |bytes: &[u8], sc: &GpuScene| {
        bytes
            .as_chunks::<32>()
            .0
            .iter()
            .filter_map(|r| sc.palette.get(crate::gpu::PaletteIndex(u32::from_le_bytes(r.get(24..28)?.try_into().ok()?))))
            .filter(|c| *c == c4(Tokens::get().accent))
            .count()
    };
    let before = build_scene(&h.app);
    assert_eq!(accent(&before.lines, &before), 0);
    h.step(&json!({"start": "inspect.isocurve"})).unwrap();
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("inspect.isocurve"));
    h.step(&json!({"click": {"world": [10, 0, 15]}})).unwrap();
    assert_eq!(h.app.session.isocurves.len(), 1, "{:?}", h.app.status);
    let sc = build_scene(&h.app);
    let segments: usize = h.app.session.isocurve_lines().iter().flat_map(|l| l.iter()).map(|l| l.points.len() - 1).sum();
    assert!(segments > 16, "eight curves each way: {segments} segments");
    assert_eq!(accent(&sc.lines, &sc), segments, "every segment on the model, in the accent");
    assert_eq!(accent(&sc.overlay, &sc), 0, "none over the model");
    h.step(&json!({"start": "inspect.isocurve"})).unwrap();
    assert!(h.app.session.isocurves.is_empty() && h.app.tool.is_none());
    let after = build_scene(&h.app);
    assert_eq!(accent(&after.lines, &after), 0, "the curves go with the analysis");
}

/// Isocurve Analysis on a face a boss splits in two: both pieces lie on one plane, so their
/// curves are the same lines, straight across the boss (they were spaced over each piece's
/// own extent, so the two pieces' lines missed each other).
#[test]
fn isocurves_of_a_split_face_run_on_across_the_split() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length": 60, "width": 40, "height": 30, "center": [0, 0, 15]})).unwrap();
    // Through the front face from bottom to top: the face is left in two pieces, x < -6 and
    // x > 18, wide enough for two and one of the lines (10 apart: the body with the boss is
    // 60 x 52 x 50).
    h.app.run("solid.cylinder", json!({"radius": 12, "height": 50, "base": [6, -20, 0], "operation": "join"})).unwrap();
    h.settle();
    h.app.run("inspect.isocurve", json!({"faces": [[-18, -20, 15], [24, -20, 15]]})).unwrap();
    let faces = h.app.session.isocurve_lines();
    assert_eq!(faces.len(), 2, "{:?}", h.app.status);
    // Each piece's lines up the face (x held) and across it (z held), by where they are held.
    let held = |lines: &[sketcraft_engine::kernel::IsoLine], k: usize| {
        let mut at: Vec<f64> = lines
            .iter()
            .filter_map(|l| {
                let (a, b) = (l.points.first()?, l.points.last()?);
                let (pa, pb) = ([a.x, a.y, a.z][k], [b.x, b.y, b.z][k]);
                ((pa - pb).abs() < 1e-9).then_some(pa)
            })
            .collect();
        at.sort_by(f64::total_cmp);
        at
    };
    for (k, axis) in [(0, "x"), (2, "z")] {
        let (left, right) = (held(&faces[0], k), held(&faces[1], k));
        assert!(left.len() >= 2 && !right.is_empty(), "{axis}: {left:?} {right:?}");
        let step = left[1] - left[0];
        for x in left.iter().chain(&right) {
            let rung = (x - left[0]) / step;
            assert!((rung - rung.round()).abs() < 1e-6, "{axis} = {x} is off the ladder {left:?} (step {step}); right piece {right:?}");
        }
    }
}

/// Bodies drawn from their display plans, evaluated on the GPU, look like the same bodies
/// built into triangles on the CPU: the sample design rendered both ways through the
/// viewport's real pipelines, offscreen, on this machine's GPU (skipped without one).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bodies_evaluated_on_the_gpu_look_like_bodies_built_on_the_cpu() {
    let mut s = sketcraft_engine::Session::default();
    s.run_script(&sketcraft_engine::sample::script()).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let mut render = |tess: bool| {
        app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess, oit: false });
        let scene = build_scene(&app).merged();
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
        let (planned, shader) = (scene.planned.len(), tess.then_some(sketcraft_gpu::SHADER));
        (
            planned,
            scene.tris.len(),
            crate::gpu::render_offscreen(
                scene,
                None,
                proj.vp.to_f32(),
                proj.cam.back().to_f32(),
                eye,
                SIZE,
                shader,
                [0.0; 8],
                crate::gpu::analysis_colors(&Tokens::get().analysis),
            ),
        )
    };
    let (planned, gpu_tris, gpu) = render(true);
    let (none, cpu_tris, cpu) = render(false);
    let (Some(gpu), Some(cpu)) = (gpu, cpu) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    assert!(planned > 0 && none == 0, "{planned} bodies planned");
    assert!(gpu_tris < cpu_tris, "the planned bodies' triangles are not built on the CPU ({gpu_tris} vs {cpu_tris} bytes)");
    let px = |img: &[u8], i: usize| [img[4 * i], img[4 * i + 1], img[4 * i + 2]];
    let n = (SIZE * SIZE) as usize;
    let drawn = (0..n).filter(|&i| px(&cpu, i) != [255, 255, 255]).count();
    assert!(drawn > n / 20, "the model covers the view: {drawn} pixels");
    let differ = (0..n).filter(|&i| px(&gpu, i).iter().zip(px(&cpu, i)).any(|(a, b)| a.abs_diff(b) > 24)).count();
    assert!(differ * 100 < drawn, "{differ} of {drawn} model pixels differ");
}

/// A device that cannot build the tessellation pipeline (D3D12's FXC rejected the shader
/// at startup and the app panicked) still shows the model: the failure is caught, logged,
/// and bodies are built on the CPU and drawn. A shader the device rejects stands in for FXC.
/// Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bodies_are_built_on_the_cpu_when_the_tessellation_pipeline_fails() {
    const REJECTED: &str = "
            @group(0) @binding(1) var<storage, read_write> surfaces: array<u32>;
            @compute @workgroup_size(64)
            fn main(@builtin(global_invocation_id) gid: vec3<u32>) { surfaces[gid.x] = 1u; }
        ";
    let Some(tess) = crate::gpu::offscreen_tessellates(REJECTED) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    assert!(!tess, "the rejected shader built a pipeline");
    let mut s = sketcraft_engine::Session::default();
    s.run_script(&sketcraft_engine::sample::script()).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess, oit: false });
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let scene = build_scene(&app).merged();
    assert!(scene.planned.is_empty(), "no body waits for the GPU evaluation");
    assert!(!scene.tris.is_empty(), "the bodies are meshed on the CPU");
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let Some(img) = crate::gpu::render_offscreen(
        scene,
        None,
        proj.vp.to_f32(),
        proj.cam.back().to_f32(),
        eye,
        SIZE,
        Some(REJECTED),
        [0.0; 8],
        crate::gpu::analysis_colors(&Tokens::get().analysis),
    ) else {
        return;
    };
    let n = (SIZE * SIZE) as usize;
    let drawn = img.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
    assert!(drawn > n / 20, "the model is drawn: {drawn} pixels");
}

/// Ambient occlusion while the view moves (#7165): fewer samples for a moving view, the full
/// count once it has held still for `SS_SETTLE`, and the full count whenever AO is off.
#[test]
fn ambient_occlusion_takes_fewer_samples_while_the_view_moves() {
    use sketcraft_gpu::view::{AO_SAMPLES, AO_SAMPLES_MOVING};
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    let ctx = egui::Context::default();
    let size = [800.0, 600.0];
    let view = |x: f32| {
        let mut m = [[0.0; 4]; 4];
        (0..4).for_each(|i| m[i][i] = 1.0);
        m[3][0] = x;
        m
    };
    // The samples drawn, and whether the frame asks to be drawn again by the time the view
    // will have settled (with no input to wake the window, the full count must follow).
    let at = |app: &mut SolveApp, time: f64, x: f32| {
        let mut n = 0;
        let mut out = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| n = ao_samples(app, ui.ctx(), view(x), size));
        out.textures_delta.clear();
        let delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        (n, delay <= std::time::Duration::from_secs_f64(SS_SETTLE + 0.02))
    };
    // A new context asks for its own first repaints: let it settle.
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput { time: Some(0.0), ..Default::default() }, |_| {}).textures_delta.clear();
    }
    app.preferences.ambient_occlusion = false;
    assert_eq!(at(&mut app, 0.0, 0.0), (AO_SAMPLES, false), "AO off");
    app.preferences.ambient_occlusion = true;
    assert_eq!(at(&mut app, 1.0, 1.0), (AO_SAMPLES_MOVING, true), "the view just moved");
    assert_eq!(at(&mut app, 1.1, 1.0), (AO_SAMPLES_MOVING, true), "still for less than the settling time");
    assert_eq!(at(&mut app, 1.0 + SS_SETTLE + 0.01, 1.0), (AO_SAMPLES, false), "settled");
    assert_eq!(at(&mut app, 1.4, 2.0), (AO_SAMPLES_MOVING, true), "moved again");
    app.ui.visual_style = 2;
    assert_eq!(at(&mut app, 1.45, 3.0), (AO_SAMPLES, false), "wireframe draws no AO");
}

/// Supersampling (#1288): the sample design drawn at 2× and filtered down shows the same
/// model (the same pixels covered, give or take its outline) with smoothed edges: pixels part
/// model, part background, which a single-sampled frame does not have. Skipped without a GPU
/// adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn supersampling_draws_the_same_model_with_smooth_edges() {
    let mut s = sketcraft_engine::Session::default();
    s.run_script(&sketcraft_engine::sample::script()).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render =
        |ssaa: f32| crate::gpu::render_offscreen_ssaa(build_scene(&app).merged(), proj.vp.to_f32(), proj.cam.back().to_f32(), eye, SIZE, ssaa);
    let (Some(plain), Some(super2)) = (render(1.0), render(2.0)) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let covered = |img: &[u8]| img.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
    let (a, b) = (covered(&plain), covered(&super2));
    assert!(a > 2000, "the model is drawn: {a} pixels");
    assert!((a as f64 - b as f64).abs() < a as f64 * 0.1, "same model: {a} vs {b} pixels covered");
    // Edge pixels against the white background: neither white nor any colour the plain frame
    // has, so blended. A count well above the plain frame's own.
    let palette: std::collections::HashSet<[u8; 3]> = plain.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
    let new_colours = super2.chunks(4).filter(|p| !palette.contains(&[p[0], p[1], p[2]])).count();
    assert!(new_colours > 100, "the supersampled frame blends its edges: {new_colours} new colours");
}

/// The supersampled frame stays within the device's largest texture (#869) and a memory
/// budget, and is not drawn at all when that leaves too little to gain.
#[test]
fn supersampling_keeps_within_the_device_texture_limit() {
    use crate::gpu::supersampled_size;
    assert_eq!(supersampled_size([1920.0, 1080.0], 2.0, 16384, 1), Some([3840, 2160]));
    assert_eq!(supersampled_size([1920.0, 1080.0], 1.5, 16384, 1), Some([2880, 1620]));
    // A 4096 texture limit: a 2560 px wide view gets 1.6×.
    let [w, h] = supersampled_size([2560.0, 1440.0], 2.0, 4096, 1).unwrap();
    assert!((4095..=4096).contains(&w) && (2303..=2304).contains(&h), "{w} x {h}");
    // Under 1.25× is not worth it: drawn as usual.
    assert_eq!(supersampled_size([3840.0, 2160.0], 2.0, 4096, 1), None);
    assert_eq!(supersampled_size([1920.0, 1080.0], 1.0, 16384, 1), None);
    assert_eq!(supersampled_size([0.0, 1080.0], 2.0, 16384, 1), None);
    // Under 1: drawn smaller and filtered up (the product preview past its budget, #1464).
    assert_eq!(supersampled_size([3840.0, 2160.0], 0.5, 4096, 4), Some([1920, 1080]));
    // 4× MSAA at 1440p: the memory budget caps it below 2×.
    let [w, h] = supersampled_size([2560.0, 1440.0], 2.0, 16384, 4).unwrap();
    assert!(w < 5120 && w as f32 >= 2560.0 * 1.25 && u64::from(w) * u64::from(h) * 32 <= 384 << 20, "{w} x {h}");
}

/// Ambient occlusion beside a block on a plate (#4167, #3407), seen from above with a radius
/// large on screen (a big design seen close up): the plate darkens toward the block and is
/// clear beyond the radius, with no copies of the block's outline in the darkening. The same
/// sample offsets at every pixel gave each sample its own shifted copy, so beside the block's
/// straight side every row was the same staircase. Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn ambient_occlusion_draws_no_copies_of_a_part() {
    use sketcraft_gpu::view::{AO_SAMPLES, AO_SAMPLES_MOVING};
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 300, "width": 300, "height": 4, "corner": [-150, -150, -4], "body_name": "plate"})).unwrap();
    s.execute("solid.box", &json!({"length": 30, "width": 160, "height": 16, "corner": [0, -80, 0], "operation": "new", "body_name": "block"}))
        .unwrap();
    // On one side of the view only, too far from the block to shade the rows measured below.
    s.execute("solid.box", &json!({"length": 20, "width": 20, "height": 16, "corner": [-110, 60, 0], "operation": "new", "body_name": "marker"}))
        .unwrap();
    let mut app = crate::test_support::classic_app(s);
    let (yaw, pitch) = sketcraft_engine::render::StandardView::Top.angles();
    app.cam = Camera { target: Vec3::new(-30.0, 0.0, 0.0), yaw, pitch, roll: 0.0, distance: 200.0, fov: 0.0, ..Default::default() };
    app.ui.perspective = false;
    app.ui.show_grid = false;
    app.set_origin_shown(false);
    app.ui.ground_shadow = false;
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    const SIZE: u32 = 512;
    const RADIUS: f32 = 45.0;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], 0.0];
    let render = |radius: f32, samples: u32| {
        crate::gpu::render_offscreen_ao(build_scene(&app).merged(), proj.vp.to_f32(), proj.cam.back().to_f32(), eye, SIZE, radius, samples)
    };
    let Some(off) = render(0.0, AO_SAMPLES) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let (w, r) = (f64::from(SIZE), scene_radius(&app));
    let at = |p: Vec3| proj.cam.to_screen(p, w, w, r).unwrap();
    // The block's left side on screen, the radius in pixels, and rows beside the middle of
    // the side, far enough from its ends that every sample's copy spans them all.
    let edge = at(Vec3::new(0.0, 0.0, 16.0)).0;
    let reach = (at(Vec3::new(f64::from(RADIUS), 0.0, 0.0)).0 - at(Vec3::ZERO).0).abs();
    let (a, b) = (at(Vec3::new(-10.0, 10.0, 0.0)).1.round() as usize, at(Vec3::new(-10.0, -10.0, 0.0)).1.round() as usize);
    let (y0, y1) = (a.min(b), a.max(b));
    assert!(reach > 100.0 && y1 > y0 + 20, "{reach} px, rows {y0}..{y1}");
    let lum = |img: &[u8], x: usize, y: usize| img[(y * SIZE as usize + x) * 4..][..3].iter().map(|&c| f64::from(c)).sum::<f64>();
    // The still picture's samples, and the fewer drawn while the view moves (#7165).
    let mut beside = Vec::new();
    let mut pictures = Vec::new();
    for samples in [AO_SAMPLES, AO_SAMPLES_MOVING] {
        let Some(on) = render(RADIUS, samples) else { return };
        let on = &on;
        // The plate's visibility in each column: its brightness with AO over its brightness
        // without, averaged over the rows.
        let column = |x: usize| (y0..y1).map(|y| lum(on, x, y) / lum(&off, x, y).max(1.0)).sum::<f64>() / (y1 - y0) as f64;
        // From 30 px beyond the radius up to 4 px from the block (its outline).
        let from = (edge - reach - 30.0) as usize;
        let profile: Vec<f64> = (from..=(edge - 4.0) as usize).map(column).collect();
        let (clear, shaded) = (&profile[..20], &profile[30..]);
        let step = shaded.windows(2).map(|p| (p[1] - p[0]).abs()).fold(0.0, f64::max);
        eprintln!(
            "{samples} samples: plate {} without AO; edge {edge:.1} px, radius {reach:.1} px, rows {y0}..{y1}; clear {:.4}..{:.4}; next to the block {:.4}; largest step {step:.4}",
            lum(&off, from, y0) / 3.0,
            clear.iter().copied().fold(f64::INFINITY, f64::min),
            clear.iter().copied().fold(0.0, f64::max),
            profile[profile.len() - 1],
        );
        assert!(profile[profile.len() - 1] < 0.98, "{samples} samples: the plate darkens beside the block: {:.4}", profile[profile.len() - 1]);
        assert!(clear.iter().all(|v| (v - 1.0).abs() < 0.004), "{samples} samples: the plate is clear beyond the radius: {clear:?}");
        assert!(step < 0.005, "{samples} samples: no copies of the block's outline: a step of {step:.4} between neighbouring columns");
        beside.push(profile[profile.len() - 1]);
        pictures.push(on.clone());
    }
    // The shader takes the count it is given (the moving frame is a different picture), and
    // spreads it over the same hemisphere: both darken the contact alike.
    assert_ne!(pictures[0], pictures[1], "8 samples draw another picture than 32");
    assert!((beside[0] - beside[1]).abs() < 0.02, "the same darkening beside the block at 32 and 8 samples: {beside:?}");
    // No grain (#9327): where the plate darkens, neighbouring pixels agree at either count.
    let (from, to) = ((edge - reach) as usize, (edge - 4.0) as usize);
    for picture in &pictures {
        let grain = (y0..y1).flat_map(|y| (from..to).map(move |x| (lum(picture, x, y) - lum(picture, x + 1, y)).abs() / 3.0)).fold(0.0, f64::max);
        eprintln!("grain {grain:.2}");
        assert!(grain <= 2.0, "neighbouring pixels differ by {grain:.2} of 255");
    }
    for picture in &pictures {
        // Blurred by depth: the block's top right beside the edge stays as clear as without AO.
        for x in (edge + 1.0) as usize..=(edge + 3.0) as usize {
            let y = (y0 + y1) / 2;
            assert!((lum(picture, x, y) - lum(&off, x, y)).abs() <= 6.0, "the plate's shade crosses onto the block's top at {x}");
        }
        // The map is read the right way up: the plate darkens beside the marker, not at its mirror image.
        let shade = |p: Vec3| {
            let (x, y, _) = at(p);
            lum(picture, x as usize, y as usize) / lum(&off, x as usize, y as usize).max(1.0)
        };
        let (beside, mirror) = (shade(Vec3::new(-100.0, 58.0, 0.0)), shade(Vec3::new(-100.0, -58.0, 0.0)));
        assert!(beside < 0.99 && (mirror - 1.0).abs() < 0.004, "beside the marker {beside:.4}, at its mirror image {mirror:.4}");
    }
    // Off, nothing is read from the released maps: the plate is as bright beside the block as away from it.
    assert!((lum(&off, to, y0) - lum(&off, from, y0)).abs() <= 3.0, "AO off darkens nothing");
}

/// Faded and see-through bodies are drawn from their plans where the renderer blends them
/// order-independently, and look as they did built on the CPU: a see-through box beside a
/// part faded outside the active component, drawn both ways. Without order-independent
/// transparency they stay on the CPU (the sorted fallback needs their triangles). Skipped
/// without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn faded_and_see_through_bodies_are_drawn_from_their_plans_with_oit() {
    let mut s = sketcraft_engine::Session::default();
    let part = s.execute("component.create", &json!({"name": "Part"})).unwrap()["component"].as_u64().unwrap();
    s.execute("solid.box", &json!({"length": 30, "width": 30, "height": 30, "corner": [0, 0, 0], "operation": "new", "body_name": "Glass"}))
        .unwrap();
    s.execute("appearance.assign", &json!({"bodies": ["Glass"], "color": "#3366cc", "opacity": 0.5})).unwrap();
    s.execute("solid.box", &json!({"length": 20, "width": 20, "height": 20, "corner": [40, 0, 0], "operation": "new", "body_name": "Inside"}))
        .unwrap();
    s.execute("component.activate", &json!({"component": 0})).unwrap();
    s.execute("solid.box", &json!({"length": 20, "width": 20, "height": 20, "corner": [0, 40, 0], "operation": "new", "body_name": "Faded"}))
        .unwrap();
    s.execute("component.activate", &json!({"component": part})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.ui.ground_shadow = false;
    app.fit_view();
    const SIZE: u32 = 192;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let target =
        |tess: bool, oit: bool| Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess, oit });
    let classes = |scene: &GpuScene| {
        let mut c: Vec<_> = scene.planned.iter().map(|b| format!("{:?}", b.class)).collect();
        c.sort();
        c
    };
    app.viewport.gpu = target(true, false);
    assert_eq!(classes(&build_scene(&app)), ["Opaque"], "without OIT only the opaque body is drawn from its plan");
    app.viewport.gpu = target(true, true);
    let planned = build_scene(&app);
    assert_eq!(classes(&planned), ["Faded", "Glass", "Opaque"], "with OIT every body is");
    let built: usize = planned.blocks.iter().map(|b| b.bytes.vertices.len()).sum();
    assert_eq!(built, 0, "and none is built on the CPU");
    // Hovered, a faded body drawn from its plan lights through the plan's triangles.
    let faded = app.session.world_state().unwrap().bodies.iter().find(|b| b.name.starts_with("Faded")).map(|b| b.name.clone()).unwrap();
    app.viewport.hover_bodies = vec![faded];
    assert_eq!(build_highlight(&app).hover_planned.len(), 1, "the faded body's plan is lit");
    app.viewport.hover_bodies.clear();
    app.viewport.gpu = target(false, false);
    let cpu = build_scene(&app);
    let render = |scene: GpuScene, tess: bool| {
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
        crate::gpu::render_offscreen(
            scene,
            None,
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            tess.then_some(sketcraft_gpu::SHADER),
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    let (Some(from_plans), Some(from_cpu)) = (render(planned, true), render(cpu, false)) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let differ = from_plans.chunks(4).zip(from_cpu.chunks(4)).filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 6)).count();
    let shown = from_cpu.chunks(4).filter(|p| p[..3] != from_cpu[..3]).count();
    assert!(shown > 2000, "the bodies are in view ({shown} px)");
    assert!(differ * 100 < shown, "{differ} of {shown} px differ between the plan and CPU pictures");
}

/// A hovered face lights solidly over a body drawn from its display plan on the GPU: the
/// highlight uses the plan's triangles, not the reference mesh's other triangulation of the
/// same face, which fought the model at equal depth and shimmered in wedges. Every face of a
/// box with a pocket and a round boss, hovered, changes the same pixels as on the CPU path (where model and
/// highlight share the reference mesh). Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_hovered_face_lights_whole_over_a_body_drawn_from_its_plan() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 40, "height": 40, "corner": [0, 0, 0], "operation": "new"})).unwrap();
    s.execute("solid.box", &json!({"length": 16, "width": 16, "height": 20, "corner": [12, 12, 20], "operation": "cut"})).unwrap();
    s.execute("solid.cylinder", &json!({"base": [33, 33, 40], "diameter": 10, "height": 10, "operation": "join"})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let body = app.session.world_state().unwrap().bodies[0].clone();
    let faces = body.body.face_count();
    let mut render = |tess: bool, hover: Option<usize>| {
        app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess, oit: false });
        app.viewport.hover = hover.map(|index| Hit::Face { body: body.name.clone(), index, point: Vec3::ZERO });
        let (scene, highlight) = (build_scene(&app), build_highlight(&app).merged());
        assert_eq!(!scene.planned.is_empty(), tess, "the box is drawn from its plan exactly on the GPU path");
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
        let shader = tess.then_some(sketcraft_gpu::SHADER);
        crate::gpu::render_offscreen(
            scene,
            Some(highlight),
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            shader,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    let lit = |plain: &[u8], hovered: &[u8]| -> Vec<bool> { plain.chunks(4).zip(hovered.chunks(4)).map(|(a, b)| a != b).collect() };
    let (Some(gpu_plain), Some(cpu_plain)) = (render(true, None), render(false, None)) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let mut checked = 0;
    for f in 0..faces {
        let (Some(gpu), Some(cpu)) = (render(true, Some(f)), render(false, Some(f))) else { return };
        let (on_gpu, on_cpu) = (lit(&gpu_plain, &gpu), lit(&cpu_plain, &cpu));
        let area = on_cpu.iter().filter(|x| **x).count();
        if area < 400 {
            continue; // facing away or hidden
        }
        // Inside the face, two pixels in from its outline: a curved face's outline follows
        // its own facets, which the plan and the reference mesh cut differently.
        let n = SIZE as i32;
        let inside = |i: usize| {
            let (x, y) = (i as i32 % n, i as i32 / n);
            (-2..=2).all(|dy| (-2..=2).all(|dx| on_cpu.get(((y + dy).clamp(0, n - 1) * n + (x + dx).clamp(0, n - 1)) as usize) == Some(&true)))
        };
        let missed = (0..on_cpu.len()).filter(|&i| inside(i) && !on_gpu[i]).count();
        let lights = missed * 200 < area;
        // A failure keeps its pictures: this failed once in a loaded full-suite run on Metal
        // and has not been reproduced since (#1383). The mask is red where the face does not
        // light on the GPU, grey over the rest of the face as the CPU lights it.
        let kept = if lights {
            String::new()
        } else {
            let mask: Vec<u8> = (0..on_cpu.len())
                .flat_map(|i| match (inside(i) && !on_gpu[i], on_cpu[i]) {
                    (true, _) => [255, 0, 0, 255],
                    (false, true) => [128, 128, 128, 255],
                    (false, false) => [255, 255, 255, 255],
                })
                .collect();
            let changed = on_gpu.iter().filter(|x| **x).count();
            let pictures =
                [("gpu_plain", &gpu_plain), ("gpu_hovered", &gpu), ("cpu_plain", &cpu_plain), ("cpu_hovered", &cpu), ("missed", &mask)];
            format!("; {changed} pixels change on the GPU; {}", keep_failed_pictures(&format!("hover-face-{f}"), &pictures, SIZE))
        };
        assert!(lights, "face {f}: {missed} of its {area} hovered pixels do not light (the highlight fights the model){kept}");
        checked += 1;
    }
    assert!(checked >= 3, "{checked} faces seen lit");
}

/// Saves a failing pixel test's pictures (straight RGBA, `size` × `size`) as PNGs in a folder
/// of their own under the temporary folder, and says where.
#[cfg(not(target_arch = "wasm32"))]
fn keep_failed_pictures(name: &str, pictures: &[(&str, &Vec<u8>)], size: u32) -> String {
    let dir = std::env::temp_dir().join(format!("sketcraft-{name}-{}", std::process::id()));
    let saved = std::fs::create_dir_all(&dir).is_ok()
        && pictures.iter().all(|(file, rgba)| {
            image::RgbaImage::from_raw(size, size, rgba.to_vec()).is_some_and(|img| img.save(dir.join(format!("{file}.png"))).is_ok())
        });
    if saved { format!("pictures in {}", dir.display()) } else { format!("the pictures could not be saved in {}", dir.display()) }
}

/// The highlight scene as the viewport hands it over keeps its feedback (a hovered face, a
/// body copied on the CPU) in a block of its own, kept while the highlight is unchanged. That
/// block is drawn: the picture is the same as with the feedback merged into the scene's own
/// triangles. Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_highlight_scenes_own_block_is_drawn() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20, "corner": [0, 0, 0], "operation": "new"})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    const SIZE: u32 = 128;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let body = app.session.world_state().unwrap().bodies[0].clone();
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let model = build_scene(&app);
    let render = |highlight: GpuScene| {
        crate::gpu::render_offscreen(
            model.clone(),
            Some(highlight),
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    let Some(plain) = render(GpuScene::default()) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let mut lit = 0;
    for index in 0..body.body.face_count() {
        app.viewport.hover = Some(Hit::Face { body: body.name.clone(), index, point: Vec3::ZERO });
        let highlight = build_highlight(&app);
        assert!(highlight.blocks.iter().any(|b| !b.bytes.vertices.is_empty()), "the face's feedback is in the highlight's block");
        let (Some(as_handed), Some(merged)) = (render(highlight.clone()), render(highlight.merged())) else { return };
        assert!(as_handed == merged, "face {index}: the highlight's block is not drawn as its merged triangles are");
        lit += usize::from(as_handed != plain);
    }
    assert!(lit >= 3, "{lit} faces light up");
}

/// An origin plane lying on a body's face is tinted evenly over it (#1739: a box built at the
/// origin, seen from below in orthographic; the XY plane's square z-fought with the bottom
/// face in stripes). Faces are pushed back by their slope (`FACE_BIAS`), the planes are not.
/// Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn an_origin_plane_on_a_face_is_tinted_evenly() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 10, "corner": [0, 0, 0]})).unwrap();
    let mut app = crate::test_support::classic_app(s);
    app.cam = Camera {
        target: Vec3::new(16.904, 10.024, 3.661),
        yaw: -0.32,
        pitch: -0.9652,
        roll: 0.0,
        distance: 98.557,
        fov: 0.0,
        ..Default::default()
    };
    app.ui.perspective = false;
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let mut render = |origin: bool| {
        app.set_origin_shown(origin);
        app.viewport.gpu =
            Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
        let (scene, highlight) = (build_scene(&app), build_highlight(&app).merged());
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let img = crate::gpu::render_offscreen(
            scene,
            Some(highlight),
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            [e[0], e[1], e[2], 0.0],
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        );
        (img, proj)
    };
    let (Some(with), proj) = render(true) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let (Some(without), _) = render(false) else { return };
    app.set_origin_shown(true);
    let xy = origin_planes(&app).into_iter().find(|(n, _, _)| *n == "XY").map(|(_, _, q)| q).unwrap();
    let (w, h, r) = (f64::from(SIZE), f64::from(SIZE), scene_radius(&app));
    let (mut samples, mut plain) = (0, Vec::new());
    // Points inside the square (away from its outline), where it lies on the bottom face.
    for i in 1..12 {
        for j in 1..12 {
            let (u, v) = (f64::from(i) / 12.0, f64::from(j) / 12.0);
            let p = xy[0] + (xy[1] - xy[0]) * u + (xy[3] - xy[0]) * v;
            if !(p.x > 0.5 && p.x < 39.5 && p.y > 0.5 && p.y < 29.5) {
                continue;
            }
            let Some((x, y, _)) = proj.cam.to_screen(p, w, h, r) else { continue };
            let at = (y as usize * SIZE as usize + x as usize) * 4;
            samples += 1;
            if with.get(at..at + 4) == without.get(at..at + 4) {
                plain.push((x as u32, y as u32));
            }
        }
    }
    assert!(samples > 40, "{samples} points of the square on the face");
    assert!(
        plain.is_empty(),
        "{} of {samples} points of the XY plane's square do not show over the face: {:?}",
        plain.len(),
        &plain[..plain.len().min(6)]
    );
}

/// While a dimension's value is typed only the open sketch, drawn at that value, loses its
/// hover highlight: another sketch's curve is drawn where it is, so its hover still shows.
#[test]
fn a_typed_value_keeps_the_hover_of_another_sketch() {
    crate::dim_view::cancel();
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    let line = h.app.run("sketch.line", json!({"points":[[0,30],[40,30]]})).unwrap()["curves"][0].as_str().unwrap().to_string();
    let other = h.app.session.active_sketch.unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    let own = h.app.run("sketch.line", json!({"points":[[0,0],[40,0]]})).unwrap()["curves"][0].as_str().unwrap().to_string();
    let open = h.app.session.active_sketch.unwrap();
    h.call("ui.view", json!({"view":"top"}));
    h.app.cam.distance = 80.0;
    h.app.cam.target = Vec3::new(20.0, 15.0, 0.0);
    h.settle();
    h.step(&json!({"start":"sketch.dimension"})).unwrap();
    h.step(&json!({"click":{"world":[20,0,0]}})).unwrap();
    h.step(&json!({"text":"60"})).unwrap();
    h.settle();
    assert!(crate::dim_view::preview_solve(&h.app).is_some(), "the value is typed");
    let proj = display_proj(&h.app);
    let drawn = |h: &crate::scenario::Harness| {
        let ctx = crate::fonts::test_context();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            hover_highlight(&h.app, &ui.ctx().layer_painter(egui::LayerId::background()), &proj);
        });
        output.textures_delta.clear();
        !output.shapes.is_empty()
    };
    h.app.viewport.hover = Some(Hit::SketchCurve { sketch: other, id: line, straight: true, point: Vec3::ZERO });
    assert!(drawn(&h), "the other sketch's curve is lit under the pointer");
    h.app.viewport.hover = Some(Hit::SketchCurve { sketch: open, id: own, straight: true, point: Vec3::ZERO });
    assert!(!drawn(&h), "the open sketch's design curve is not");
    crate::dim_view::cancel();
}

#[test]
fn construction_drawing_and_hover_overlays_preserve_visible_gaps() {
    fn length(shape: &Shape) -> f32 {
        match shape {
            Shape::LineSegment { points, .. } => points[0].distance(points[1]),
            Shape::Path(path) => path.points.windows(2).map(|w| w[0].distance(w[1])).sum(),
            Shape::Vec(shapes) => shapes.iter().map(length).sum(),
            _ => 0.0,
        }
    }
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.call("ui.view", json!({"view":"top"}));
    h.settle();
    let proj = display_proj(&h.app);
    let a = Vec2::ZERO;
    let b = Vec2::new(20.0, 8.0);
    let whole = proj.to_screen(Vec3::new(b.x, b.y, 0.0)).unwrap().distance(proj.to_screen(Vec3::ZERO).unwrap());
    let mut draw = crate::tools::Tool::for_command("sketch.line").unwrap();
    draw.pts.push((a, None));
    draw.hover = Some((b, None));
    for mode in [0, 1, 2] {
        crate::sketch_palette::set_mode(&mut h.app, mode);
        let ctx = crate::fonts::test_context();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::background());
            crate::tools::preview(&h.app, &draw, &painter, &proj);
        });
        output.textures_delta.clear();
        let preview: f32 = output.shapes.iter().map(|s| length(&s.shape)).sum();
        let r = h.app.run("sketch.line", json!({"points":[[0,0],[20,8]]})).unwrap();
        let id = r["curves"][0].as_str().unwrap().to_string();
        h.app.viewport.hover = Some(Hit::SketchCurve { sketch: h.app.session.active_sketch.unwrap(), id, straight: true, point: Vec3::ZERO });
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            hover_highlight(&h.app, &ui.ctx().layer_painter(egui::LayerId::background()), &proj);
        });
        output.textures_delta.clear();
        let hover: f32 = output.shapes.iter().map(|s| length(&s.shape)).sum();
        assert!((preview - hover).abs() < 0.01, "mode {mode}: preview {preview}, hover {hover}");
        if mode == 0 {
            assert!((preview - whole).abs() < 0.01);
        } else {
            assert!(preview > whole * 0.25 && preview < whole * 0.85, "mode {mode} covers {preview} of {whole}");
        }
        h.app.run("edit.undo", json!({})).unwrap();
    }
    crate::sketch_palette::set_mode(&mut h.app, 0);
}

/// Headless pictures of the view (`ui.render`, the agent's view image) are drawn in the
/// window's theme (#974) and colour vision mode (#778). The light theme's tokens are the engine's own colours, so a
/// light picture in Off is unchanged; otherwise each part takes the token the window draws it
/// with. Sketch states follow what the window does with them (`sketch_color`) and the axes the
/// mode's axis colours, so the two can't drift apart.
#[test]
fn pictures_are_drawn_in_the_windows_theme() {
    use crate::theme::ColorVision;
    use sketcraft_engine::view::Palette;
    assert_eq!(scene_palette(false, ColorVision::Off), Palette::DEFAULT, "the light theme is the engine's colours");
    let rgb = |c: Color32| Rgb(c.r(), c.g(), c.b());
    let (p, t) = (scene_palette(true, ColorVision::Off), Tokens::dark());
    assert_eq!(p.background, (rgb(t.viewport_top), rgb(t.viewport_bottom)));
    assert_eq!(
        [p.grid, p.grid_major, p.edge, p.body, p.body_selected, p.cap],
        [t.grid, t.grid_major, t.body_edge, t.body, t.body_selected, t.section_cap].map(rgb)
    );
    assert_ne!(p.background, Palette::DEFAULT.background);
    for (dark, mode) in [false, true].into_iter().flat_map(|d| ColorVision::ALL.map(|m| (d, m))) {
        let (p, t) = (scene_palette(dark, mode), Tokens::themed(dark, mode));
        assert_eq!(p.axes, t.axes().map(rgb), "dark {dark}, {mode:?}: the axes");
        assert_eq!(p.analysis, t.analysis, "dark {dark}, {mode:?}: the surface analysis");
        for (tag, cpu) in [
            (colors::SKETCH, p.sketch),
            (colors::SKETCH_FIXED, p.sketch_fixed),
            (colors::SKETCH_DONE, p.sketch_done),
            (colors::SKETCH_LOCKED, p.sketch_locked),
            (colors::SKETCH_PROJECTED, p.sketch_projected),
            (colors::SKETCH_LOST, p.sketch_lost),
            (colors::SKETCH_CONSTRUCTION, p.sketch_construction),
            (colors::SKETCH_CENTERLINE, p.sketch_centerline),
        ] {
            let [r, g, b, _] = sketch_color(&t, tag);
            assert_eq!(cpu, Rgb(r, g, b), "dark {dark}, {mode:?}: the window draws {tag:?} in another colour");
        }
    }
}

/// The view's picture follows a theme change: its background is the window's.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_view_picture_follows_the_theme() {
    if !crate::gpu::test_picture_renderer() {
        return;
    }
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.app.cam = sketcraft_engine::view::home_camera(&h.app.session).unwrap();
    for dark in [false, true, false] {
        h.app.ui.dark = dark;
        let img = crate::control::view_rgba(&h.app, 160, 120).unwrap();
        // The top row's most common colour: the background's top.
        let mut counts = std::collections::HashMap::new();
        for px in img.as_chunks::<4>().0.iter().take(160) {
            *counts.entry([px[0], px[1], px[2]]).or_insert(0) += 1;
        }
        let px = counts.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c).unwrap_or_default();
        let want = if dark { Tokens::dark() } else { Tokens::light() }.viewport_top;
        let near = px.iter().zip([want.r(), want.g(), want.b()]).all(|(a, b)| a.abs_diff(b) <= 2);
        assert!(near, "dark {dark}: top row {px:?}, the window's background {want:?}");
    }
}

/// Changing only the 3D view's dark or light (#4146) rebuilds the view's scene and highlights
/// in its colours, as a change of the window's theme does.
#[test]
fn changing_only_the_views_dark_or_light_rebuilds_its_scene() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.call("ui.prefs", json!({"theme": "dark"}));
    h.frames(2);
    let before = (scene_key(&h.app), highlight_geometry_key(&h.app));
    h.call("ui.prefs", json!({"viewport_theme": "light"}));
    h.frames(2);
    assert!(h.app.ui.dark, "the window stays dark");
    assert_ne!(scene_key(&h.app), before.0, "the scene is rebuilt");
    assert_ne!(highlight_geometry_key(&h.app), before.1, "and so are the highlights");
}

/// A section's cut outlines are drawn onto the view, so a light view in a dark window draws
/// them in the light side's text colour (#4146): the window's would be pale on the light view.
#[test]
fn a_sections_outlines_take_the_views_text_colour() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.app.run("inspect.section", json!({"plane": "XZ", "at": 15})).unwrap();
    h.call("ui.prefs", json!({"theme": "dark", "viewport_theme": "light"}));
    h.frames(2);
    let scene = build_scene(&h.app);
    let colours: Vec<[u8; 4]> = scene
        .lines
        .as_chunks::<32>()
        .0
        .iter()
        .filter_map(|l| {
            let i = u32::from_le_bytes(l.get(24..28)?.try_into().ok()?);
            scene.palette.get(crate::gpu::PaletteIndex(i))
        })
        .collect();
    let (view, window) = (Tokens::light().text.gamma_multiply(0.9), Tokens::dark().text.gamma_multiply(0.9));
    assert!(colours.contains(&c4(view)), "the outlines in the view's text colour");
    assert!(!colours.contains(&c4(window)), "none in the window's");
}

/// The view's picture is cut by the section the viewport shows: an open Section Analysis
/// dialog's preview, a sketch's Slice.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_view_picture_uses_section_preview_and_sketch_slice() {
    fn pixels(h: &mut crate::scenario::Harness, cam: Camera) -> Vec<[u8; 4]> {
        h.app.cam = cam;
        let img = crate::control::view_rgba(&h.app, 320, 240).unwrap();
        img.as_chunks::<4>().0.to_vec()
    }
    if !crate::gpu::test_picture_renderer() {
        return;
    }
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":40,"width":30,"height":20})).unwrap();
    let cam = sketcraft_engine::view::home_camera(&h.app.session).unwrap();
    let whole = pixels(&mut h, cam);
    let before = h.app.session.doc.clone();
    let undo = h.app.session.undo.len();
    h.app.session.selection = vec![Sel::Plane { name: "XZ".into() }];
    h.app.start("inspect.section");
    let Some(crate::dialogs::Kind::Section { offset, .. }) = h.app.dialog.as_mut().map(|d| &mut d.kind) else { panic!("section dialog") };
    *offset = "15 mm".into();
    assert!(h.app.session.section.is_none());
    let preview = pixels(&mut h, cam);
    assert!(whole.iter().zip(&preview).filter(|(a, b)| a != b).count() > 100, "the picture ignored the uncommitted section preview");
    assert!(std::sync::Arc::ptr_eq(&before, &h.app.session.doc));
    assert_eq!(h.app.session.undo.len(), undo);
    h.app.run("inspect.section", json!({"plane":"XZ","offset":15})).unwrap();
    h.app.dialog = None;
    assert_eq!(preview, pixels(&mut h, cam), "preview and committed section must draw identically");

    h.app.session.section = None;
    h.app.run("sketch.create", json!({"plane":{"origin":[0,0,10],"x_dir":[1,0,0],"y_dir":[0,1,0]}})).unwrap();
    let whole = pixels(&mut h, cam);
    h.app.run("sketch.options", json!({"slice":true})).unwrap();
    assert!(h.app.session.section.is_none());
    let sliced = pixels(&mut h, cam);
    assert!(whole.iter().zip(&sliced).filter(|(a, b)| a != b).count() > 100, "the picture ignored the active Sketch Slice");
    h.app.run("sketch.options", json!({"slice":false})).unwrap();
    assert_eq!(whole, pixels(&mut h, cam), "disabling Slice restores the whole-body pixels");
}

#[test]
fn timeline_modifiers_highlight_on_hover_and_click() {
    for command in ["solid.fillet", "solid.chamfer"] {
        let mut h = crate::scenario::Harness::new();
        h.app.set_origin_shown(false);
        let base = h.app.run("solid.box", json!({"length":20,"width":20,"height":20})).unwrap()["feature"].as_u64().unwrap();
        let id = h.app.run(command, json!({"edges":[[0,0,10]],"radius":2,"distance":2,"name":"Blend"})).unwrap()["feature"].as_u64().unwrap();
        h.app.run("solid.box", json!({"length":5,"width":5,"height":5,"corner":[40,0,0]})).unwrap();
        h.frames(2);
        let geometry = feature_geometry(&h.app, id);
        assert_eq!(geometry.len(), 1, "{command}: {geometry:?}");
        let Sel::Face { body, index, .. } = &geometry[0] else { panic!("a modifier must highlight its face") };
        let st = h.app.session.world_state().unwrap();
        let m = st.body(body).unwrap().mesh();
        let triangles = m.tri_face.iter().filter(|f| **f as usize == *index).count();
        assert!(triangles > 0 && triangles < m.triangles.len());
        let p = crate::scenario::handle_at("timeline:Blend").unwrap();
        h.step(&json!({"move":[p.x,p.y]})).unwrap();
        assert_eq!(h.app.viewport.hover_feature, Some(id));
        assert_eq!(build_highlight(&h.app).merged().tris.len(), triangles * 3 * crate::gpu::TRI_SIZE);
        assert!(!build_highlight(&h.app).merged().lines.is_empty());
        h.step(&json!({"click":[p.x,p.y]})).unwrap();
        assert_eq!(h.app.session.selection, vec![Sel::Feature { id }]);
        // Moving away removes hover but leaves the selected blend highlighted.
        h.step(&json!({"move":[5,5]})).unwrap();
        assert_eq!(h.app.viewport.hover_feature, None);
        assert_eq!(build_highlight(&h.app).merged().tris.len(), triangles * 3 * crate::gpu::TRI_SIZE);
        let bounds = selection_bounds(&h.app).unwrap();
        assert!(bounds.max.x <= 2.01 && bounds.max.y <= 2.01 && bounds.max.z > 19.99, "{bounds:?}");
        // Creator features retain whole-body feedback, while hidden bodies stay hidden.
        assert_eq!(feature_geometry(&h.app, base), vec![Sel::Body { name: body.clone() }]);
        h.app.session.visibility.hidden_bodies.push(body.clone());
        assert!(feature_geometry(&h.app, id).is_empty());
        assert!(selection_bounds(&h.app).is_none());
        assert!(build_highlight(&h.app).merged().tris.is_empty());
    }
}

/// A sketch picked in the timeline or the Browser lights all its curves in the selection
/// colour, also while it is hidden (#7156); nothing is lit once it is no longer picked.
#[test]
fn a_picked_sketch_lights_its_curves_shown_or_hidden() {
    let mut h = crate::scenario::Harness::new();
    h.app.set_origin_shown(false);
    h.app.run("solid.box", json!({"length":20,"width":20,"height":20})).unwrap();
    h.app.run("sketch.create", json!({"plane":"XY","name":"Outline"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0":[30,0],"p1":[40,10]})).unwrap();
    h.app.run("sketch.circle.center", json!({"center":[35,5],"radius":3,"construction":true})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    let id = h.app.session.doc.find_feature("Outline").unwrap().id;
    h.frames(2);
    let sel_edge = c4(Tokens::get().sel_edge);
    // The selection-coloured strokes of the highlight, drawn over the model (a line holds its
    // colour's palette index).
    let lit = |app: &SolveApp| {
        let scene = build_highlight(app).merged();
        scene
            .overlay
            .as_chunks::<{ crate::gpu::LINE_SIZE }>()
            .0
            .iter()
            .filter(|line| {
                let index = line.get(24..28).and_then(|b| b.try_into().ok()).map_or(0, u32::from_le_bytes);
                scene.palette.get(crate::gpu::PaletteIndex(index)) == Some(sel_edge)
            })
            .count()
    };
    assert_eq!(lit(&h.app), 0, "nothing is picked");
    let p = crate::scenario::handle_at("timeline:Outline").unwrap();
    h.step(&json!({"click":[p.x,p.y]})).unwrap();
    assert_eq!(h.app.session.selection, vec![Sel::Feature { id }]);
    let shown = lit(&h.app);
    // Four sides, the circle's many segments and its dashes.
    assert!(shown > 20, "the picked sketch's curves are lit: {shown}");
    h.app.session.visibility.hidden_sketches.push(id);
    assert!(!visible_sketches(&h.app).contains(&id));
    assert_eq!(lit(&h.app), shown, "a hidden sketch is lit too");
    h.app.session.selection = vec![Sel::Feature { id: h.app.session.doc.features[0].id }];
    assert_eq!(lit(&h.app), 0, "another feature picked");
    h.app.session.selection.clear();
    assert_eq!(lit(&h.app), 0, "nothing is picked");
}

#[test]
fn history_marker_shows_only_its_sketch_without_changing_visibility() -> Result<(), Box<dyn std::error::Error>> {
    for explicitly_hidden in [false, true] {
        let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        app.ui.show_grid = false;
        let mut sketches = Vec::new();
        for (name, x) in [("First", 0), ("Second", 30)] {
            app.run("sketch.create", json!({"plane":"XY","name":name}))?;
            app.run("sketch.rectangle.two_point", json!({"p0":[x,0],"p1":[x+10,10]}))?;
            app.run("sketch.finish", json!({}))?;
            let id = app.session.doc.find_feature(name).ok_or("missing sketch")?.id;
            sketches.push(id);
            app.run("solid.extrude", json!({"sketch":name,"distance":5,"body_name":name}))?;
        }
        if explicitly_hidden {
            app.session.visibility.hidden_sketches.clone_from(&sketches);
        }
        let stored = app.session.visibility.clone();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(640., 480.));
        for (position, expected) in [(0, None), (1, sketches.first().copied()), (2, None), (3, sketches.get(1).copied()), (4, None)] {
            app.run("timeline.roll_to", json!({"position":position}))?;
            assert_eq!(visible_sketches(&app), expected.into_iter().collect::<Vec<_>>(), "position {position}, hidden {explicitly_hidden}");
            let scene = build_sketch_scene(&app, &projection(&app, rect)).merged();
            assert_eq!(!scene.lines.is_empty(), expected.is_some(), "sketch lines at position {position}");
            assert_eq!(app.session.visibility, stored, "stepping must preserve stored visibility");
        }
    }
    Ok(())
}

#[test]
fn timeline_modifier_highlight_follows_edits_and_rollback() {
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("solid.box", json!({"length":20,"width":20,"height":20})).unwrap();
    let id = app.run("solid.fillet", json!({"edges":[[0,0,10]],"radius":2})).unwrap()["feature"].as_u64().unwrap();
    // A later chamfer changes the same body without consuming the fillet.
    app.run("solid.chamfer", json!({"edges":[[20,20,10]],"distance":1})).unwrap();
    app.run("solid.move", json!({"bodies":["Body1"],"translate":[30,0,0]})).unwrap();
    app.run("body.rename", json!({"body":"Body1","name":"Moved"})).unwrap();
    app.session.selection = vec![Sel::Feature { id }];
    let bounds = selection_bounds(&app).unwrap();
    assert!(bounds.min.x >= 29.99 && bounds.max.x <= 32.01, "{bounds:?}");
    assert!(feature_geometry(&app, id).iter().all(|x| matches!(x, Sel::Face { body, .. } if body == "Moved")));
    app.run("timeline.roll_to", json!({"position":1})).unwrap();
    assert!(feature_geometry(&app, id).is_empty());
    app.run("timeline.roll_to", json!({})).unwrap();
    assert!(!feature_geometry(&app, id).is_empty());
    app.run("timeline.suppress", json!({"feature":id})).unwrap();
    assert!(feature_geometry(&app, id).is_empty());
}

#[test]
fn timeline_modifier_highlight_follows_component_instances() {
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("component.create", json!({"name":"Part","grounded":false})).unwrap();
    app.run("solid.box", json!({"length":20,"width":20,"height":20})).unwrap();
    let id = app.run("solid.fillet", json!({"edges":[[0,0,10]],"radius":2})).unwrap()["feature"].as_u64().unwrap();
    app.run("occurrence.move", json!({"occurrence":"Part:1","translate":[30,0,0]})).unwrap();
    app.run("occurrence.copy", json!({"component":"Part","translate":[0,40,0]})).unwrap();
    let geometry = feature_geometry(&app, id);
    assert_eq!(geometry.len(), 2, "both occurrences retain the fillet's face: {geometry:?}");
    app.session.selection = vec![Sel::Feature { id }];
    let bounds = selection_bounds(&app).unwrap();
    assert!(bounds.min.x >= 29.99 && bounds.max.x <= 32.01 && bounds.max.y >= 41.99, "{bounds:?}");
}

/// The parts the instancing tests place: a box (flat triangles, its vertices shared by
/// position and each fragment lit with its triangle's normal) and a cylinder (smooth, its
/// vertices shared by position and normal).
const PARTS: [(&str, &str); 2] = [("solid.box", r#"{"length":20,"width":10,"height":5}"#), ("solid.cylinder", r#"{"radius":6,"height":12}"#)];

/// A design with a part (`command` with `params`, see [`PARTS`]) placed three times (one
/// placement turned), drawn on a device that builds bodies on the CPU.
fn three_placed_parts((command, params): (&str, &str)) -> SolveApp {
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::default());
    app.ui.ground_shadow = false;
    app.set_origin_shown(false);
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    app.run("component.create", json!({"name":"Part"})).unwrap();
    app.run(command, serde_json::from_str(params).unwrap()).unwrap();
    app.run("occurrence.copy", json!({"component":"Part","translate":[40,0,0]})).unwrap();
    let turned = app.run("occurrence.copy", json!({"component":"Part","translate":[0,40,0]})).unwrap()["occurrence"].clone();
    app.run("occurrence.move", json!({"occurrence":turned,"axis":[1,1,0],"angle":"30 deg"})).unwrap();
    // With the part active, its other placements are drawn faded.
    app.run("component.activate", json!({"component":"root"})).unwrap();
    app
}

/// A part placed by several occurrences is drawn from one copy of its triangles, once per
/// placement: one instanced mesh with a placement for each occurrence, whose triangles land
/// where each occurrence's own moved mesh has them, and no moved copy is made to draw them.
#[test]
fn a_part_placed_many_times_is_drawn_from_one_copy_of_its_triangles() {
    for (part, flat) in PARTS.into_iter().zip([true, false]) {
        placed_three_times_is_drawn_from_one_copy(part, flat);
    }
}

fn placed_three_times_is_drawn_from_one_copy(part: (&str, &str), flat: bool) {
    let app = three_placed_parts(part);
    let st = crate::open_meshing::session_world(&app.session);
    assert_eq!(st.bodies.len(), 3);
    eprintln!("DBG before {}", st.bodies.iter().filter(|b| b.cached_mesh().is_some()).count());
    let scene = build_scene(&app);
    assert_eq!(scene.instanced.len(), 1, "one shared mesh");
    assert_eq!(scene.instanced[0].count(), 3, "a placement per occurrence");
    let shared = &scene.instanced[0].tris;
    assert_eq!(shared.flat, flat, "{part:?}");
    let corners = shared.indices.len();
    let vertices = shared.vertices.len() / crate::gpu::SHARED_SIZE;
    assert!(vertices * 2 < corners, "{part:?}: {vertices} vertices shared by {corners} corners");
    assert!(scene.blocks.iter().all(|b| b.bytes.vertices.is_empty()), "no body's triangles built where it stands");
    let moved = st.bodies.iter().filter(|b| b.cached_mesh().is_some()).count();
    assert!(moved <= 1, "{moved} bodies made a moved mesh to be drawn");
    let merged = scene.merged();
    // Each placement's triangles where its own mesh has them, with its normals.
    let vertex = |b: &[u8]| -> [f32; 6] { std::array::from_fn(|i| f32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]])) };
    let mut drawn: Vec<[f32; 6]> = merged.tris.as_chunks::<{ crate::gpu::TRI_SIZE }>().0.iter().map(|v| vertex(v)).collect();
    let mut expected: Vec<[f32; 6]> = Vec::new();
    for b in &st.bodies {
        let m = b.mesh();
        for i in m.triangles.iter().flatten().map(|i| *i as usize) {
            let (p, n) = (m.positions[i].to_f32(), m.normals[i].to_f32());
            expected.push([p[0], p[1], p[2], n[0], n[1], n[2]]);
        }
    }
    let order = |a: &[f32; 6], b: &[f32; 6]| a.iter().map(|x| (x * 1e3).round() as i64).cmp(b.iter().map(|x| (x * 1e3).round() as i64));
    drawn.sort_by(order);
    expected.sort_by(order);
    assert_eq!(drawn.len(), expected.len());
    for (d, e) in drawn.iter().zip(&expected) {
        assert!(d.iter().zip(e).all(|(x, y)| (x - y).abs() < 1e-3), "{d:?} vs {e:?}");
    }
    // Each placement's edges are drawn where it stands.
    let one = st.bodies[0].mesh();
    let segments: usize = one
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| one.edge_pickable(*i) && one.shaded_edge_visible(*i))
        .map(|(_, e)| e.len().saturating_sub(1))
        .sum();
    // Vertex markers are lines from a point to itself; count the edges' segments.
    let edge_lines = merged.lines.as_chunks::<{ crate::gpu::LINE_SIZE }>().0.iter().filter(|r| r[..12] != r[12..24]).count();
    assert_eq!(edge_lines, 3 * segments);
}

#[test]
fn imported_mesh_edges_draw_in_each_style_without_brep_picks() {
    let h = crate::scenario::Harness::new();
    let path = h.dir.join("box.3mf");
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("solid.box", json!({"length":20,"width":20,"height":10})).unwrap();
    app.run("file.save_mesh", json!({"path":path})).unwrap();
    app.run("file.new", json!({})).unwrap();
    let imported = app.run("file.insert_mesh", json!({"path":path})).unwrap();
    app.run("component.activate", json!({"component":"root"})).unwrap();
    app.run("occurrence.copy", json!({"component":imported["component"],"translate":[40,0,0]})).unwrap();
    app.ui.show_grid = false;
    app.set_origin_shown(false);
    app.ui.ground_shadow = false;
    app.fit_view();
    for style in 0..4 {
        app.ui.visual_style = style;
        let scene = build_scene(&app).merged();
        // Twelve edges per placed box, in the saved mesh's pieces: no side longer than a fifth
        // of the 30 mm diagonal, so the eight 20 mm edges in four and the four 10 mm ones in
        // two, 40 segments.
        let lines = if style == 1 { 0 } else { 2 * 40 * crate::gpu::LINE_SIZE };
        assert_eq!(scene.lines.len(), lines, "style {style}: twelve edges per placed box");
        assert_eq!(scene.hidden.len(), if style == 3 { lines } else { 0 });
        assert_eq!(scene.tris.is_empty(), style == 2, "wireframe draws only the edges");
    }
    let proj = projection(&app, Rect::from_min_size(Pos2::ZERO, vec2(800., 600.)));
    for body in &crate::open_meshing::session_world(&app.session).bodies {
        let mesh = body.mesh();
        assert_eq!(mesh.edges.len(), 12, "the placed mesh keeps its outlines");
        for (i, edge) in mesh.edges.iter().enumerate() {
            assert!(!mesh.edge_pickable(i), "placements keep display-only edge semantics");
            for point in [edge[0], (edge[0] + edge[1]) * 0.5] {
                let hits = pick_all(&app, &proj, proj.to_screen(point).unwrap());
                assert!(!hits.iter().any(|h| matches!(h, Hit::Edge { .. } | Hit::Vertex { .. })), "{hits:?}");
            }
        }
    }
}

#[test]
fn mesh_outline_hover_cache_follows_measure_and_preserves_modeling_picks() {
    let h = crate::scenario::Harness::new();
    let path = h.dir.join("box.stl");
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("solid.box", json!({"length":20,"width":30,"height":10})).unwrap();
    app.run("file.save_mesh", json!({"path":path})).unwrap();
    app.run("file.new", json!({})).unwrap();
    app.run("file.insert_mesh", json!({"path":path})).unwrap();
    app.set_origin_shown(false);
    app.fit_view();
    let proj = projection(&app, Rect::from_min_size(Pos2::ZERO, vec2(800., 600.)));
    let pos = proj.to_screen(Vec3::new(20., 0., 5.)).unwrap();
    let measure = crate::dialogs::Dialog::for_command(&app, "inspect.measure").unwrap();
    let mut fillet = crate::dialogs::Dialog::for_command(&app, "solid.fillet").unwrap();
    // Identical input masks must not make the two dialogs share cached geometry hits.
    fillet.inputs[0].accept = measure.inputs[0].accept;
    app.dialog = Some(fillet.clone());
    assert!(!pick_cached(&mut app, &proj, pos).iter().any(|h| matches!(h, Hit::Edge { .. })));
    app.dialog = Some(measure);
    assert!(matches!(pick_cached(&mut app, &proj, pos).first(), Some(Hit::Edge { .. })));
    let hits = pick_all(&app, &proj, pos);
    assert!(hits.iter().any(|h| matches!(h, Hit::Edge { .. })), "Select Other also offers the measured outline");
    app.dialog = Some(fillet);
    assert!(!pick_cached(&mut app, &proj, pos).iter().any(|h| matches!(h, Hit::Edge { .. })));
    app.dialog = None;
    let hits = pick_cached(&mut app, &proj, pos);
    assert!(!hits.iter().any(|h| matches!(h, Hit::Edge { .. } | Hit::Vertex { .. })));
    assert!(crate::open_meshing::session_world(&app.session).bodies[0].mesh().display_only_edges);
}

/// Manual GPU benchmark: 25 placements of the mesh `SKETCRAFT_BENCH_MESH` (an STL, OBJ or
/// 3MF) drawn offscreen at 3600 × 2190 px (divided by `SKETCRAFT_BENCH_DIV`) with 4× MSAA
/// while the camera turns, each frame waited for: the placements' own triangles (`merged`, as
/// before instancing) and as instances. Prints the median frame time of each.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "manual: SKETCRAFT_BENCH_MESH=part.stl cargo test --release -p sketcraft-ui-egui --lib -- --ignored instanced_scene_gpu_benchmark --nocapture"]
fn instanced_scene_gpu_benchmark() {
    use egui_wgpu::wgpu;
    let Some(path) = std::env::var_os("SKETCRAFT_BENCH_MESH") else { return };
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.ground_shadow = false;
    app.set_origin_shown(false);
    let component = app.run("file.insert_mesh", json!({"path": path.to_string_lossy()})).unwrap()["component"].clone();
    app.run("component.activate", json!({"component":"root"})).unwrap();
    let size = crate::open_meshing::session_world(&app.session).bodies[0].mesh_bounds().size();
    let step = size.x.max(size.y) * 1.3;
    for k in 1..25 {
        app.run("occurrence.copy", json!({"component":component,"translate":[f64::from(k % 5) * step, f64::from(k / 5) * step, 0.0]})).unwrap();
    }
    // `SKETCRAFT_BENCH_OPACITY=0.5`: every placement see-through.
    if let Some(opacity) = std::env::var("SKETCRAFT_BENCH_OPACITY").ok().and_then(|v| v.parse::<f64>().ok()) {
        app.run("appearance.assign", json!({"components": [component.to_string()], "color": "#b4b8bc", "opacity": opacity})).unwrap();
    }
    let Some((_, device, queue)) = crate::gpu::offscreen_device() else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let scale: u32 = std::env::var("SKETCRAFT_BENCH_DIV").ok().and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let (w, h, samples) = (3600u32 / scale, 2190u32 / scale, 4u32);
    let depth = sketcraft_gpu::depth_format(&device, true);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    app.viewport.gpu = Some(GpuTarget { format, depth: Some(depth), samples, tess: false, oit: false });
    app.fit_view();
    let texture = |format, samples| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bench"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    let (color, resolve, depth_view) = (texture(format, samples), texture(format, 1), texture(depth, samples));
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32));
    let t0 = std::time::Instant::now();
    let scene = build_scene(&app);
    let t1 = std::time::Instant::now();
    drop(build_scene(&app));
    eprintln!("scene built in {:.0} ms, again in {:.0} ms", (t1 - t0).as_secs_f64() * 1e3, t1.elapsed().as_secs_f64() * 1e3);
    let proj = projection(&app, rect);
    let st = crate::open_meshing::session_world(&app.session);
    let seen = st.bodies.iter().filter(|b| proj.to_screen(b.mesh_bounds().center()).is_some_and(|p| rect.contains(p))).count();
    eprintln!("{w} x {h} px: {seen} of {} placements in view", st.bodies.len());
    // `SKETCRAFT_BENCH_CLOSE=1`: close up on the middle placement, most of the others out of view.
    if std::env::var("SKETCRAFT_BENCH_CLOSE").is_ok_and(|v| v == "1") {
        let middle = st.bodies.iter().map(|b| b.mesh_bounds()).nth(12).unwrap_or_default();
        app.cam.target = middle.center();
        app.cam.distance = middle.diagonal() * 1.5;
        let proj = projection(&app, rect);
        let seen = st.bodies.iter().filter(|b| proj.to_screen(b.mesh_bounds().center()).is_some_and(|p| rect.contains(p))).count();
        eprintln!("close up: {seen} of {} placements' centres in view", st.bodies.len());
    }
    let yaw = app.cam.yaw;
    // The levels of detail, made off the window thread meanwhile.
    let t = std::time::Instant::now();
    while app.viewport.mesh_lods.busy() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let lod = build_scene(&app);
    eprintln!(
        "levels of detail made in {:.1} s: {:?}",
        t.elapsed().as_secs_f64(),
        lod.instanced.iter().map(|m| m.levels.iter().map(|(t, e)| (t.indices.len() / 3, *e)).collect::<Vec<_>>()).collect::<Vec<_>>()
    );
    // `SKETCRAFT_BENCH_PNG=dir`: the view drawn from each placement's own triangles, in full
    // as instances, and from the levels (`SKETCRAFT_BENCH_PNG_ONLY=1`: then stop).
    if let Some(dir) = std::env::var_os("SKETCRAFT_BENCH_PNG") {
        const SIDE: u32 = 1024;
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIDE as f32, SIDE as f32));
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
        for (name, scene) in [("own", scene.clone().merged()), ("full", scene.clone()), ("lod", lod.clone())] {
            if let Some(rgba) = crate::gpu::render_offscreen(
                scene,
                None,
                proj.vp.to_f32(),
                proj.cam.back().to_f32(),
                eye,
                SIDE,
                None,
                [0.0; 8],
                crate::gpu::analysis_colors(&Tokens::get().analysis),
            ) && let Some(png) = sketcraft_engine::render::png(SIDE as usize, SIDE as usize, rgba)
            {
                let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), png);
            }
        }
    }
    if std::env::var_os("SKETCRAFT_BENCH_PNG_ONLY").is_some() {
        return;
    }
    for (label, scene) in [("own triangles", scene.clone().merged()), ("instanced", scene), ("instanced, levels of detail", lod)] {
        let mut renderer = crate::gpu::Renderer::new(&device, GpuTarget { format, depth: Some(depth), samples, tess: false, oit: false }, None);
        let slot: SceneSlot = std::sync::Arc::new(std::sync::Mutex::new(Some(scene)));
        let none = || std::sync::Arc::new(std::sync::Mutex::new(None));
        let (mut times, mut prepared) = (Vec::new(), Vec::new());
        for k in 0..40 {
            app.cam.yaw = yaw + f64::from(k) * 0.05;
            let proj = projection(&app, rect);
            let e = proj.cam.eye().to_f32();
            let frame = Frame {
                product_lights: None,
                product: false,
                upload_error: None,
                key: 1,
                slot: slot.clone(),
                sketch_key: 0,
                sketch_slot: none(),
                hl_key: 0,
                hl_slot: none(),
                pv_key: 0,
                pv_slot: none(),
                pv_hide: false,
                preview_bounds: None,
                view_proj: proj.vp.to_f32(),
                back: proj.cam.back().to_f32(),
                size_px: [w as f32, h as f32],
                clip: [0.0; 4],
                cap: [0.0; 4],
                images: Vec::new(),
                clouds: Vec::new(),
                analysis: [0.0; 8],
                analysis_colors: crate::gpu::analysis_colors(&Tokens::get().analysis),
                access: None,
                outline: true,
                eye: [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }],
                ao_radius: 0.0,
                ao_samples: sketcraft_gpu::view::AO_SAMPLES,
                sky: [0.0; 4],
                back_face: [0.0; 4],
                ssaa: 1.0,
                hover_tint: [0.0; 3],
                selection_tint: [0.0; 3],
                hatch: Default::default(),
                moving: None,
            };
            let t0 = std::time::Instant::now();
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.prepare(&device, &queue, &mut encoder, &frame);
            if k >= 5 {
                prepared.push(t0.elapsed().as_secs_f64() * 1e3);
            }
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("bench"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &color,
                            depth_slice: None,
                            resolve_target: Some(&resolve),
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Discard },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(sketcraft_gpu::view::far_depth(sketcraft_gpu::view::reversed_depth(Some(depth)))),
                                store: wgpu::StoreOp::Discard,
                            }),
                            stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    })
                    .forget_lifetime();
                renderer.paint(&frame, &mut pass, [0.0, 0.0, w as f32, h as f32]);
            }
            queue.submit([encoder.finish()]);
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            if k >= 5 {
                times.push(t0.elapsed().as_secs_f64() * 1e3);
            }
        }
        times.sort_by(f64::total_cmp);
        prepared.sort_by(f64::total_cmp);
        eprintln!(
            "{label}: median {:.1} ms a frame (min {:.1}, max {:.1}), of which {:.1} ms preparing on the CPU",
            times[times.len() / 2],
            times[0],
            times[times.len() - 1],
            prepared[prepared.len() / 2]
        );
    }
}

#[test]
fn small_reference_meshes_reuse_display_triangles_when_their_occurrence_moves() {
    use sketcraft_engine::doc::{Document, FeatureKind, MeshData};
    let mut doc = Document::default();
    let component = doc.add_component(Some("Part"), 0).unwrap();
    let occurrence = doc.occurrences.iter_mut().find(|o| o.component == component).unwrap();
    occurrence.grounded = false;
    let occurrence = occurrence.id;
    let meshes = vec![MeshData {
        name: "Small mesh".into(),
        positions: [0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0].into(),
        triangles: [0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3].into(),
        color: Some([0.8, 0.3, 0.1]),
    }];
    let id = doc.add_feature(FeatureKind::MeshImport { file: "small.stl".into(), meshes }, Some("Mesh")).unwrap();
    doc.feature_mut(id).unwrap().component = component;
    let mut app = SolveApp::new(sketcraft_engine::Session::new(doc), crate::Services::default());
    app.ui.ground_shadow = false;
    app.ui.visual_style = 1;
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    let first = build_scene(&app);
    assert_eq!(first.instanced.len(), 1, "a single reference mesh below the large-mesh threshold shares its triangles");
    assert!(first.blocks.iter().all(|b| b.bytes.vertices.is_empty()));
    app.run("occurrence.move", json!({"occurrence":occurrence, "translate":[7,11,13], "axis":[1,1,0], "angle":"30 deg"})).unwrap();
    let moved = build_scene(&app);
    assert_eq!(moved.instanced.len(), 1);
    assert!(std::sync::Arc::ptr_eq(&first.instanced[0].tris, &moved.instanced[0].tris));
    assert!(moved.blocks.iter().all(|b| b.bytes.vertices.is_empty()));
    let expected = app.session.world_state().unwrap().bodies[0].mesh();
    let merged = moved.merged();
    assert_ne!(first.merged().tris, merged.tris, "the placement changed");
    let actual: Vec<[f32; 3]> = merged
        .tris
        .as_chunks::<{ crate::gpu::TRI_SIZE }>()
        .0
        .iter()
        .map(|v| std::array::from_fn(|i| f32::from_le_bytes(v[4 * i..4 * i + 4].try_into().unwrap())))
        .collect();
    let expected: Vec<[f32; 3]> = expected.triangles.iter().flatten().map(|i| expected.positions[*i as usize].to_f32()).collect();
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(a.iter().zip(b).all(|(a, b)| (*a - b).abs() < 1e-4));
    }
}

fn single_reference_grid(n: u32) -> SolveApp {
    use sketcraft_engine::doc::{Document, FeatureKind, MeshData};
    let mut doc = Document::default();
    let positions = (0..=n).flat_map(|y| (0..=n).flat_map(move |x| [f64::from(x), f64::from(y), 0.0])).collect();
    let triangles = (0..n)
        .flat_map(|y| {
            (0..n).flat_map(move |x| {
                let a = y * (n + 1) + x;
                [a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]
            })
        })
        .collect();
    let meshes = vec![MeshData { name: "Grid".into(), positions, triangles, color: None }];
    doc.add_feature(FeatureKind::MeshImport { file: "grid.stl".into(), meshes }, Some("Grid")).unwrap();
    let mut app = SolveApp::new(sketcraft_engine::Session::new(doc), crate::Services::default());
    app.ui.ground_shadow = false;
    app.ui.visual_style = 1;
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    app
}

#[test]
fn a_single_reference_mesh_below_the_former_instance_threshold_keeps_full_detail() {
    let app = single_reference_grid(100);
    let scene = build_scene(&app);
    let instance = scene.instanced.first().unwrap();
    assert_eq!(scene.instanced.len(), 1);
    assert_eq!(instance.tris.indices.len(), 20_000 * 3);
    assert!(instance.levels.is_empty());
    assert!(!app.viewport.mesh_lods.busy(), "no simplification job is started");
    assert_eq!(app.viewport.mesh_lods.generation(), 0);
}

#[test]
fn a_small_translucent_reference_mesh_keeps_the_sorted_fallback_without_oit() {
    let mut app = single_reference_grid(2);
    app.run("appearance.assign", json!({"bodies":["Grid"],"color":"#3366cc","opacity":0.4})).unwrap();
    let sorted = build_scene(&app);
    assert!(sorted.instanced.is_empty(), "the non-OIT path sorts individual glass triangles");
    assert!(!sorted.merged().glass.is_empty());
    app.viewport.gpu.as_mut().unwrap().oit = true;
    let instanced = build_scene(&app);
    assert_eq!(instanced.instanced.len(), 1);
    assert_eq!(instanced.instanced[0].look, crate::gpu::InstanceLook::Glass);
    app.run("appearance.assign", json!({"bodies":["Grid"],"color":"#3366cc","opacity":1.0})).unwrap();
    app.run("component.create", json!({"name":"Editing"})).unwrap();
    app.viewport.gpu.as_mut().unwrap().oit = false;
    let faded = build_scene(&app);
    assert!(faded.instanced.is_empty(), "faded reference meshes retain the old non-OIT block ordering");
    assert!(!faded.merged().trans.is_empty());
    app.viewport.gpu.as_mut().unwrap().oit = true;
    let instanced = build_scene(&app);
    assert_eq!(instanced.instanced.len(), 1);
    assert_eq!(instanced.instanced[0].look, crate::gpu::InstanceLook::Faded);
}

/// A large mesh's shared vertices made ahead (by the background mesher after a file is
/// opened or inserted) are the ones its first scene draws: that frame welds nothing.
#[test]
fn a_large_meshs_shared_vertices_are_made_ahead_of_its_first_scene() {
    use sketcraft_engine::doc::{Document, FeatureKind, MeshData};
    // A flat grid of 160 × 160 squares: 51 200 triangles, each corner its own vertex.
    let n = 160u32;
    let corners: Vec<[u32; 2]> =
        (0..n).flat_map(|j| (0..n).flat_map(move |i| [[i, j], [i + 1, j], [i + 1, j + 1], [i, j], [i + 1, j + 1], [i, j + 1]])).collect();
    let mut doc = Document::default();
    let meshes = vec![MeshData {
        name: "Grid".into(),
        positions: corners.iter().flat_map(|[x, y]| [f64::from(*x), f64::from(*y), 0.0]).collect(),
        triangles: (0..u32::try_from(corners.len()).unwrap()).collect(),
        color: None,
    }];
    doc.add_feature(FeatureKind::MeshImport { file: "grid.stl".into(), meshes }, Some("Grid")).unwrap();
    let mut app = SolveApp::new(sketcraft_engine::Session::new(doc), crate::Services::default());
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    let (mesh, _) = crate::open_meshing::session_world(&app.session).bodies[0].mesh_in_place();
    assert!(mesh.triangles.len() >= INSTANCE_ALONE);
    warm_shared_tris(&mesh);
    let warmed = WARMED.lock().unwrap().iter().find(|(m, _)| std::sync::Arc::ptr_eq(m, &mesh)).map(|(_, t)| t.clone()).unwrap();
    assert_eq!(warmed.vertices.len() / crate::gpu::SHARED_SIZE, 161 * 161, "the grid's corners shared");
    let scene = build_scene(&app);
    assert_eq!(scene.instanced.len(), 1, "a large mesh alone is drawn from shared vertices");
    assert!(std::sync::Arc::ptr_eq(&scene.instanced[0].tris, &warmed), "the ones made ahead");
    assert!(!WARMED.lock().unwrap().iter().any(|(m, _)| std::sync::Arc::ptr_eq(m, &mesh)), "taken");
}

/// A design with one imported mesh of about 100 000 triangles (a sphere of radius 50, each
/// corner its own point, as an STL is read), drawn on a device that builds bodies on the CPU.
fn large_scan() -> SolveApp {
    use sketcraft_engine::doc::{Document, FeatureKind, MeshData};
    let n = 160u32;
    let at = |i: u32, j: u32| {
        let (t, p) = (std::f64::consts::PI * f64::from(i) / f64::from(n), std::f64::consts::TAU * f64::from(j) / f64::from(2 * n));
        let (st, sp, cp) =
            (if i == 0 || i == n { 0.0 } else { t.sin() }, if j == 2 * n { 0.0 } else { p.sin() }, if j == 2 * n { 1.0 } else { p.cos() });
        [50.0 * st * cp, 50.0 * st * sp, 50.0 * t.cos()]
    };
    let corners: Vec<[f64; 3]> = (0..n)
        .flat_map(|i| (0..2 * n).flat_map(move |j| [at(i, j), at(i + 1, j + 1), at(i, j + 1), at(i, j), at(i + 1, j), at(i + 1, j + 1)]))
        .collect();
    let mut doc = Document::default();
    let meshes = vec![MeshData {
        name: "Scan".into(),
        positions: corners.iter().flatten().copied().collect(),
        triangles: (0..u32::try_from(corners.len()).unwrap()).collect(),
        color: None,
    }];
    doc.add_feature(FeatureKind::MeshImport { file: "scan.stl".into(), meshes }, Some("Scan")).unwrap();
    let mut app = SolveApp::new(sketcraft_engine::Session::new(doc), crate::Services::default());
    app.ui.ground_shadow = false;
    app.set_origin_shown(false);
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    app
}

/// A large mesh is drawn in full until its levels of detail are made off the window thread;
/// then the scene is built again with them, and seen small, a coarse level draws it as the
/// full triangles do (within a fraction of a pixel). Skipped without a GPU adapter (the
/// levels are checked anyway).
#[test]
fn a_large_mesh_seen_small_is_drawn_from_its_levels_of_detail() {
    let mut app = large_scan();
    let (mesh, _) = crate::open_meshing::session_world(&app.session).bodies[0].mesh_in_place();
    assert!(mesh.triangles.len() >= INSTANCE_ALONE);
    let key = scene_key(&app);
    let first = build_scene(&app);
    assert_eq!(first.instanced.len(), 1);
    assert!(first.instanced[0].levels.is_empty(), "drawn in full meanwhile");
    let began = std::time::Instant::now();
    while app.viewport.mesh_lods.busy() {
        assert!(began.elapsed() < std::time::Duration::from_secs(120), "the levels are made");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_ne!(scene_key(&app), key, "the scene is built again when they are");
    let scene = build_scene(&app);
    let levels = &scene.instanced[0].levels;
    assert!(levels.len() >= 2, "{} levels", levels.len());
    // Small on screen: the sphere about 30 px across.
    app.fit_view();
    app.cam.distance *= 4.0;
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render = |scene: GpuScene| {
        crate::gpu::render_offscreen(
            scene,
            None,
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    // The level the placement is drawn from there.
    let rows = [[1.0f32, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
    let scale = crate::gpu::placement_scale(&rows, &scene.instanced[0].tris.bounds, &proj.vp.to_f32(), [SIZE as f32; 2]);
    let errors: Vec<f32> = std::iter::once(0.0).chain(levels.iter().map(|(_, e)| *e)).collect();
    assert!(crate::gpu::level_for(&errors, scale) >= 1, "a coarse level: scale {scale} px/mm, errors {errors:?}");
    let (Some(lod), Some(full)) = (render(scene.clone()), render(scene.merged())) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    let drawn = full.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
    assert!(drawn > 300, "the sphere is drawn: {drawn} pixels");
    let differ = lod.chunks(4).zip(full.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 24)).count();
    assert!(differ * 20 < drawn, "{differ} of {drawn} pixels differ");
}

/// See-through and faded placements of a part are drawn as instances too, with their alpha:
/// glass with an appearance's opacity, faded context with another component active. None
/// of their triangles go to the glass sorted triangle by triangle, and they look as their
/// own triangles did. GPU pictures skipped without an adapter.
#[test]
fn see_through_and_faded_placements_are_drawn_as_instances() {
    use crate::gpu::InstanceLook;
    let mut app = three_placed_parts(PARTS[0]);
    app.run("appearance.assign", json!({"components": ["Part"], "color": "#3366cc", "opacity": 0.4})).unwrap();
    let glass = build_scene(&app);
    assert_eq!(glass.instanced.len(), 1);
    assert_eq!((glass.instanced[0].look, glass.instanced[0].count()), (InstanceLook::Glass, 3));
    let alpha = |p: &[u8]| {
        let i = u32::from_le_bytes(p.get(48..52)?.try_into().ok()?);
        glass.palette.get(crate::gpu::PaletteIndex(i)).map(|c| c[3])
    };
    assert!(glass.instanced[0].instances.as_chunks::<{ crate::gpu::INSTANCE_SIZE }>().0.iter().all(|p| alpha(p) == Some(102)), "alpha 0.4");
    assert!(glass.glass.is_empty(), "no triangle sorted one by one");
    assert!(!glass.clone().merged().glass.is_empty());
    // One placement active (as right after the part is made): the others faded.
    let mut context = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    context.viewport.gpu = app.viewport.gpu;
    context.ui.ground_shadow = false;
    context.set_origin_shown(false);
    context.run("component.create", json!({"name":"Part"})).unwrap();
    context.run(PARTS[0].0, serde_json::from_str(PARTS[0].1).unwrap()).unwrap();
    context.run("occurrence.copy", json!({"component":"Part","translate":[40,0,0]})).unwrap();
    context.run("occurrence.copy", json!({"component":"Part","translate":[0,40,0]})).unwrap();
    let faded = build_scene(&context);
    let looks: Vec<_> = faded.instanced.iter().map(|m| (m.look, m.count())).collect();
    assert!(looks.contains(&(InstanceLook::Faded, 2)), "the other placements faded: {looks:?}");
    assert!(faded.trans.is_empty());
    for (name, scene) in [("glass", glass), ("faded", faded)] {
        app.fit_view();
        const SIZE: u32 = 256;
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
        let proj = projection(&app, rect);
        let e = proj.cam.eye().to_f32();
        let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
        let render = |scene: GpuScene| {
            crate::gpu::render_offscreen(
                scene,
                None,
                proj.vp.to_f32(),
                proj.cam.back().to_f32(),
                eye,
                SIZE,
                None,
                [0.0; 8],
                crate::gpu::analysis_colors(&Tokens::get().analysis),
            )
        };
        let (Some(instanced), Some(own)) = (render(scene.clone()), render(scene.merged())) else {
            eprintln!("skipping: no GPU adapter");
            return;
        };
        let drawn = own.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
        assert!(drawn > 1000, "{name}: drawn {drawn}");
        let differ = instanced.chunks(4).zip(own.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 24)).count();
        // A pixel here and there along the boxes' edges, where moving the corners on the GPU
        // rounds them differently.
        assert!(differ * 50 < drawn, "{name}: {differ} of {drawn} pixels differ");
    }
}

/// Each placement of a part is picked where it stands, the turned one too, from the part's
/// shared mesh: the face under the cursor names that placement's body, at the point the
/// cursor is over, and no placement makes a moved copy of the mesh to be picked.
#[test]
fn each_placement_is_picked_where_it_stands_from_the_shared_mesh() {
    let mut app = three_placed_parts(PARTS[0]);
    app.fit_view();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
    let proj = projection(&app, rect);
    let st = crate::open_meshing::session_world(&app.session);
    // The top face's centre of each placement, from its moved box corners.
    let tops: Vec<(String, Vec3)> = st
        .bodies
        .iter()
        .map(|b| {
            let (mesh, m) = b.mesh_in_place();
            let local = mesh.bounds();
            (
                b.name.clone(),
                sketcraft_engine::doc::apply_point(
                    &m,
                    Vec3::new((local.min.x + local.max.x) / 2.0, (local.min.y + local.max.y) / 2.0, local.max.z),
                ),
            )
        })
        .collect();
    for (name, top) in &tops {
        let at = proj.to_screen(*top).unwrap();
        let hits = pick(&app, &proj, at);
        let face = hits.iter().find_map(|h| match h {
            Hit::Face { body, point, .. } => Some((body.clone(), *point)),
            _ => None,
        });
        let (body, point) = face.unwrap_or_else(|| panic!("no face of {name} under {at:?}: {hits:?}"));
        assert_eq!(&body, name);
        assert!(point.dist(*top) < 0.05, "{point:?} vs {top:?}");
    }
    assert!(
        st.bodies.iter().all(|b| b.cached_mesh().is_none() || b.mesh_in_place().1 == sketcraft_engine::doc::IDENTITY),
        "a moved copy was made to pick"
    );
}

/// A part drawn as instances looks the same as its placements' own triangles drawn one by
/// one. Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn instances_draw_like_their_placements_own_triangles() {
    for part in PARTS {
        if !placed_three_times_draws_like_its_own_triangles(part) {
            return;
        }
    }
}

/// `false` without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
fn placed_three_times_draws_like_its_own_triangles(part: (&str, &str)) -> bool {
    let mut app = three_placed_parts(part);
    app.fit_view();
    const SIZE: u32 = 256;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render = |scene: GpuScene| {
        crate::gpu::render_offscreen(
            scene,
            None,
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    let scene = build_scene(&app);
    assert!(!scene.instanced.is_empty());
    let (Some(instanced), Some(one_by_one)) = (render(scene.clone()), render(scene.clone().merged())) else {
        eprintln!("skipping: no GPU adapter");
        return false;
    };
    let drawn = instanced.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
    assert!(drawn > (SIZE * SIZE) as usize / 50, "the parts are drawn: {drawn} pixels");
    let differ = instanced.chunks(4).zip(one_by_one.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 24)).count();
    assert!(differ * 200 < drawn, "{part:?}: {differ} of {drawn} pixels differ");
    // Close up on the turned placement, the others out of view: what is in view is all drawn.
    let turned = crate::open_meshing::session_world(&app.session).bodies[2].mesh_bounds();
    app.cam.target = turned.center();
    app.cam.distance = turned.diagonal() * 0.75;
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render = |scene: GpuScene| {
        crate::gpu::render_offscreen(
            scene,
            None,
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    let (Some(instanced), Some(one_by_one)) = (render(scene.clone()), render(scene.merged())) else { return false };
    let drawn = instanced.chunks(4).filter(|p| p[..3] != [255, 255, 255]).count();
    assert!(drawn > (SIZE * SIZE) as usize / 4, "{part:?}: close up, the part fills the view: {drawn} pixels");
    let differ = instanced.chunks(4).zip(one_by_one.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 24)).count();
    assert!(differ * 200 < drawn, "{part:?} close up: {differ} of {drawn} pixels differ");
    true
}

#[test]
fn ray_segment_closest_takes_the_point_under_the_ray() {
    let (o, d) = (Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, -1.0));
    let x = |a: Vec3, b: Vec3| ray_segment_closest(o, d, a, b);
    assert!(x(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, -4.0)).dist(Vec3::new(0.0, 0.0, -2.0)) < 1e-9);
    // Beyond an end: the end.
    assert!(x(Vec3::new(1.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0)).dist(Vec3::new(1.0, 0.0, 0.0)) < 1e-9);
    // Parallel, degenerate and non-finite input: the start, no panic.
    assert_eq!(x(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 5.0)), Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(x(Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)), Vec3::new(2.0, 0.0, 0.0));
    let nan = Vec3::new(f64::NAN, 0.0, 0.0);
    let _ = x(nan, Vec3::ZERO);
}

/// Hovering anywhere along a visible edge between two faces picks that edge, not a face
/// (the edge used to pick only where its start or middle was as near as the face).
#[test]
fn edge_between_two_faces_picks_along_its_whole_length() {
    for style in [0u8, 2] {
        let ctx = crate::fonts::test_context();
        let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        app.run("solid.box", json!({"length":200,"width":120,"height":160})).unwrap();
        app.set_origin_shown(false);
        app.ui.visual_style = style;
        app.cam.yaw = 0.7;
        app.cam.pitch = 0.5;
        app.cam.target = Vec3::new(100.0, 60.0, 80.0);
        app.cam.distance = 500.0;
        hover_frame(&mut app, &ctx, vec![]);
        let proj = projection(&app, app.viewport.rect.unwrap());
        let st = app.session.world_state().unwrap();
        let m = st.body("Body1").unwrap().mesh();
        let mut checked = 0;
        for (ei, e) in m.edges.iter().enumerate() {
            let (Some(a), Some(b)) = (e.first().copied(), e.last().copied()) else { continue };
            for k in 1..10 {
                let p = a.lerp(b, k as f64 / 10.0);
                let Some(sp) = proj.to_screen(p) else { continue };
                let (o, d) = proj.ray(sp);
                // Only edges in front at that point (shaded); every edge in wireframe.
                let depth = (p - o).dot(d);
                if style != 2 && m.raycast(o, d).is_some_and(|(t, _)| t < depth - 1e-3) {
                    continue;
                }
                let hits = pick(&app, &proj, sp);
                let first = candidate(&app, &hits).map(|x| x.1);
                assert!(matches!(first, Some(Sel::Edge { index, .. }) if index == ei), "style {style}: edge {ei} at {k}/10 picked {first:?}");
                checked += 1;
            }
        }
        assert!(checked > 50, "style {style}: only {checked} samples");
    }
}

fn hover_frame(app: &mut SolveApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let selected = app.highlighted();
    let mut output = ctx.run_ui(
        egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() },
        |ui| {
            show(app, ui);
        },
    );
    // This headless frame checks interactions without applying textures to a renderer.
    output.textures_delta.clear();
    // Click selection is applied after rendering; hover itself must already be current.
    if app.viewport.gpu.is_some() && selected == app.highlighted() {
        assert_eq!(app.viewport.hl_key, highlight_key(app), "GPU highlight must use this frame's hover");
    }
}

fn hover_app(gpu: bool) -> SolveApp {
    let mut s = sketcraft_engine::Session::default();
    s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    s.execute("sketch.point", &json!({"point": [0, 0]})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    if gpu {
        // The callback is queued by egui; no device is needed to inspect scene updates.
        app.viewport.gpu =
            Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    }
    app
}

fn space(pressed: bool) -> egui::Event {
    egui::Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::default() }
}

/// Holding Space is a momentary pan: the drag moves the view rather than the geometry under the
/// pointer, in every preset. The point is over the sketch's own point, so without Space this
/// drag would drag it (and the undo stack would grow).
#[test]
fn holding_space_pans_instead_of_dragging_geometry() {
    for preset in crate::prefs::NAV_PRESETS.iter().map(|p| p.id) {
        let ctx = crate::fonts::test_context();
        let mut app = hover_app(false);
        app.preferences.nav = preset.into();
        app.set_origin_shown(false);
        hover_frame(&mut app, &ctx, vec![]);
        let point = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
        let before = app.cam;
        let undo = app.session.undo.len();
        let press = |pressed: bool, at: Pos2| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        hover_frame(&mut app, &ctx, vec![space(true)]);
        hover_frame(&mut app, &ctx, vec![space(true), egui::Event::PointerMoved(point), press(true, point)]);
        hover_frame(&mut app, &ctx, vec![space(true), egui::Event::PointerMoved(point + vec2(40.0, 25.0))]);
        assert_ne!(app.cam, before, "{preset}: holding Space must pan the view");
        assert_eq!(app.session.undo.len(), undo, "{preset}: holding Space must not edit the design");
        assert!(app.viewport.point_drag.is_none(), "{preset}: holding Space must not drag geometry");
        hover_frame(&mut app, &ctx, vec![press(false, point + vec2(40.0, 25.0)), space(false)]);
    }
}

#[test]
fn ctrl_left_drag_from_empty_space_orbits_in_every_preset() {
    for preset in crate::prefs::NAV_PRESETS.iter().map(|p| p.id) {
        for action in ["orbit", "tool", "point", "click"] {
            let ctx = crate::fonts::test_context();
            let mut app = hover_app(false);
            app.preferences.nav = preset.into();
            app.set_origin_shown(false);
            hover_frame(&mut app, &ctx, vec![]);
            let point = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
            let (start, end) = match action {
                "point" => (point, point + vec2(30.0, 20.0)),
                "click" => (point, point),
                _ => (point - vec2(60.0, 60.0), point - vec2(20.0, 30.0)),
            };
            if action == "tool" {
                app.start("sketch.line");
            }
            let before = app.cam;
            let undo = app.session.undo.len();
            let modifiers = egui::Modifiers { ctrl: true, command: !cfg!(target_os = "macos"), ..Default::default() };
            let frame = |app: &mut SolveApp, mut events: Vec<egui::Event>| {
                events.insert(0, egui::Event::ModifiersChanged(modifiers));
                hover_frame(app, &ctx, events);
            };
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers },
                ],
            );
            frame(&mut app, vec![egui::Event::PointerMoved(end)]);
            frame(&mut app, vec![]);
            match action {
                "orbit" | "tool" => {
                    assert_ne!(app.cam, before, "{preset}: Ctrl+left drag on empty space orbits ({action})");
                    assert!(app.viewport.boxsel.is_none(), "{preset}: no box selection while orbiting");
                }
                "point" => {
                    assert_eq!(app.cam, before, "{preset}: Ctrl+drag on sketch geometry is not an orbit");
                    assert!(app.viewport.point_drag.is_some(), "{preset}: Ctrl+drag still drags the point");
                }
                _ => assert_eq!(app.cam, before, "{preset}: a Ctrl+click does not orbit"),
            }
            frame(&mut app, vec![egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers }]);
            frame(&mut app, vec![]);
            match action {
                "click" => assert!(!app.session.selection.is_empty(), "{preset}: Ctrl+click still selects"),
                "tool" => assert_eq!(app.session.undo.len(), undo, "{preset}: the orbit drag adds no geometry"),
                "orbit" => assert!(app.session.selection.is_empty(), "{preset}: the end of the orbit selects nothing"),
                _ => {}
            }
        }
    }
}

#[test]
fn openscad_left_drag_preserves_editing_and_selection() {
    for action in ["point", "box", "tool"] {
        let ctx = crate::fonts::test_context();
        let mut app = hover_app(false);
        app.preferences.nav = crate::prefs::OPENSCAD_NAV.into();
        app.set_origin_shown(false);
        hover_frame(&mut app, &ctx, vec![]);
        let point = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
        let (start, end) =
            if action == "point" { (point, point + vec2(30.0, 20.0)) } else { (point - vec2(40.0, 40.0), point + vec2(40.0, 40.0)) };
        if action == "tool" {
            app.start("sketch.line");
            assert!(app.tool.is_some());
        }
        let before = app.cam;
        let undo = app.session.undo.len();
        let modifiers = egui::Modifiers { shift: action == "box", ..Default::default() };
        let frame = |app: &mut SolveApp, mut events: Vec<egui::Event>| {
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
            let mut out = ctx.run_ui(
                egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() },
                |ui| show(app, ui),
            );
            out.textures_delta.clear();
        };
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers },
            ],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        frame(&mut app, vec![]);
        assert_eq!(app.cam, before, "{action} drag must not orbit");
        if action == "point" {
            assert!(app.viewport.point_drag.is_some());
            assert_eq!(app.session.undo.len(), undo + 1);
        } else if action == "box" {
            assert!(app.viewport.boxsel.is_some());
        } else {
            assert!(app.tool.is_some());
            assert!(app.viewport.primary_nav.is_none());
        }
        frame(&mut app, vec![egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers }]);
        if action == "box" {
            assert!(!app.session.selection.is_empty());
        }
    }
}

#[test]
fn right_click_that_moves_a_fraction_of_a_point_opens_the_menu_without_orbiting() {
    // A Magic Mouse right-click as logged on macOS (#5517): the pointer creeps 0.1 pt while the
    // button is down, in its own frames or in the release frame.
    use egui::{Event, PointerButton::Secondary};
    let p = pos2(400.0, 300.0);
    let (a, b) = (p + vec2(0.0, -0.02), p + vec2(0.0, -0.1));
    let button = |pos, pressed| Event::PointerButton { pos, button: Secondary, pressed, modifiers: egui::Modifiers::NONE };
    let clicks = [
        vec![vec![Event::PointerMoved(p), button(p, true)], vec![Event::PointerMoved(a)], vec![Event::PointerMoved(b)], vec![button(b, false)]],
        vec![vec![Event::PointerMoved(p), button(p, true)], vec![Event::PointerMoved(b), button(b, false)]],
    ];
    let drag = vec![
        vec![Event::PointerMoved(p), button(p, true)],
        vec![Event::PointerMoved(p + vec2(15.0, 10.0))],
        vec![Event::PointerMoved(p + vec2(30.0, 20.0))],
        vec![button(p + vec2(30.0, 20.0), false)],
    ];
    for nav in [crate::prefs::NAV_PRESETS[0].id, crate::prefs::TOUCHPAD_NAV] {
        for (frames, is_click) in clicks.iter().map(|f| (f, true)).chain([(&drag, false)]) {
            let ctx = crate::fonts::test_context();
            let mut app = hover_app(false);
            app.preferences.nav = nav.into();
            hover_frame(&mut app, &ctx, vec![]);
            let before = app.cam;
            for (k, events) in frames.iter().enumerate() {
                let raw = egui::RawInput {
                    time: Some(1.0 + k as f64 * 0.016),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                    events: events.clone(),
                    ..Default::default()
                };
                ctx.run_ui(raw, |ui| show(&mut app, ui)).textures_delta.clear();
            }
            if is_click {
                assert!(app.viewport.context_menu.is_some(), "{nav}: a right-click opens the menu");
                assert_eq!(app.cam, before, "{nav}: a right-click does not orbit");
            } else {
                assert!(app.viewport.context_menu.is_none(), "{nav}: a right drag opens no menu");
                assert_ne!(app.cam.yaw, before.yaw, "{nav}: a right drag still orbits");
            }
        }
    }
}

#[test]
fn openscad_empty_space_drags_navigate_without_editing_or_box_selecting() {
    for (button, shift) in [
        (egui::PointerButton::Primary, false),
        (egui::PointerButton::Secondary, false),
        (egui::PointerButton::Middle, false),
        (egui::PointerButton::Secondary, true),
        (egui::PointerButton::Middle, true),
    ] {
        let ctx = crate::fonts::test_context();
        let mut app = hover_app(false);
        app.preferences.nav = crate::prefs::OPENSCAD_NAV.into();
        hover_frame(&mut app, &ctx, vec![]);
        let point = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
        let p = if button == egui::PointerButton::Primary { point + vec2(80.0, 80.0) } else { point };
        let before = app.cam;
        let undo = app.session.undo.len();
        let modifiers = egui::Modifiers { shift, ..Default::default() };
        let frame = |app: &mut SolveApp, mut events: Vec<egui::Event>| {
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
            ctx.run_ui(
                egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() },
                |ui| show(app, ui),
            )
            .textures_delta
            .clear();
        };
        frame(&mut app, vec![egui::Event::PointerMoved(p), egui::Event::PointerButton { pos: p, button, pressed: true, modifiers }]);
        let end = p + vec2(30.0, 20.0);
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        frame(&mut app, vec![]);
        assert!(app.viewport.hover.is_none());
        assert!(app.viewport.boxsel.is_none() && app.viewport.point_drag.is_none());
        frame(&mut app, vec![egui::Event::PointerButton { pos: end, button, pressed: false, modifiers }]);
        assert_eq!(app.session.undo.len(), undo);
        assert!(app.session.selection.is_empty());
        assert!(app.viewport.context_menu.is_none());
        if button == egui::PointerButton::Primary {
            assert_ne!(app.cam.yaw, before.yaw);
            assert_eq!(app.cam.target, before.target);
            assert_eq!(app.cam.distance, before.distance);
        } else if shift {
            assert_ne!(app.cam.distance, before.distance);
            assert_eq!(app.cam.target, before.target);
            assert_eq!(app.cam.yaw, before.yaw);
        } else {
            assert_ne!(app.cam.target, before.target);
            assert_eq!(app.cam.distance, before.distance);
            assert_eq!(app.cam.yaw, before.yaw);
        }
    }
}

#[test]
fn touchpad_modifier_motion_navigates_without_buttons_or_document_changes() {
    for alt in [false, true] {
        let ctx = crate::fonts::test_context();
        let mut app = hover_app(false);
        app.preferences.nav = "touchpad".into();
        let p = pos2(350.0, 300.0);
        hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
        let before = app.cam;
        let doc = app.session.doc.to_json();
        let undo = app.session.undo.len();
        let log = app.session.log.len();
        let mods = egui::Modifiers { shift: !alt, alt, ..Default::default() };
        hover_frame(&mut app, &ctx, vec![egui::Event::ModifiersChanged(mods)]);
        hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(30.0, 20.0))]);
        assert!(!ctx.input(|i| i.pointer.any_down()));
        if alt {
            assert!((app.cam.yaw - before.yaw - 0.24).abs() < 1e-6);
            assert_eq!(app.cam.target, before.target);
        } else {
            assert_ne!(app.cam.target, before.target);
            assert_eq!(app.cam.yaw, before.yaw);
        }
        assert_eq!(app.cam.distance, before.distance);
        let after = app.cam;
        for _ in 0..3 {
            hover_frame(&mut app, &ctx, vec![]);
            assert_eq!(app.cam, after, "no drift between pointer events");
            assert!(app.viewport.hover.is_none());
        }
        hover_frame(&mut app, &ctx, vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE), egui::Event::PointerMoved(p)]);
        assert_eq!(app.cam, after, "release ends modifier-only navigation");
        assert_eq!(app.session.doc.to_json(), doc);
        assert_eq!(app.session.undo.len(), undo);
        assert_eq!(app.session.log.len(), log, "continuous motion must not flood the command log");
        assert!(app.session.selection.is_empty());
        assert!(app.viewport.boxsel.is_none() && app.viewport.point_drag.is_none());
    }
}

#[test]
fn touchpad_motion_with_focused_tool_fields_preserves_editing_and_pointer_ownership() {
    for command in ["solid.move", "solid.extrude"] {
        for canvas in [true, false] {
            for alt in [true, false] {
                let mut h = crate::scenario::Harness::new();
                h.app.preferences.nav = "touchpad".into();
                h.app.preferences.value_boxes_in_view = canvas;
                if command == "solid.move" {
                    h.app.run("solid.box", json!({"length":30,"width":20,"height":10})).unwrap();
                    h.app.session.selection = vec![Sel::Body { name: "Body1".into() }];
                } else {
                    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
                    h.app.run("sketch.rectangle.two_point", json!({"p0":[0,0],"p1":[40,30]})).unwrap();
                    h.app.run("sketch.finish", json!({})).unwrap();
                    let sketch = h.app.session.doc.features[0].id;
                    h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
                }
                h.step(&json!({"start":command})).unwrap();
                h.frames(6);
                let focus = h.ctx.memory(|m| m.focused()).expect("the tool's value field has focus");
                assert!(h.ctx.egui_wants_keyboard_input());
                let doc = h.app.session.doc.to_json();
                let undo = h.app.session.undo.len();
                let values = format!("{:?}", h.app.dialog.as_ref().unwrap().kind);
                h.call("ui.move", json!({"x":600,"y":350}));
                let before = h.app.cam;
                h.call("ui.move", json!({"x":600,"y":350,"alt":alt,"shift":!alt}));
                h.call("ui.move", json!({"x":660,"y":390,"alt":alt,"shift":!alt}));
                if alt {
                    assert!((h.app.cam.yaw - before.yaw - 0.48).abs() < 1e-6, "{command}, canvas={canvas}: a focused field must allow orbit");
                    assert_eq!(h.app.cam.target, before.target);
                } else {
                    assert_ne!(h.app.cam.target, before.target, "{command}, canvas={canvas}: a focused field must allow pan");
                    assert_eq!(h.app.cam.yaw, before.yaw);
                }
                assert_eq!(h.app.cam.distance, before.distance);
                assert_eq!(h.ctx.memory(|m| m.focused()), Some(focus), "navigation must keep the field focused");

                // The field owns pointer motion over itself, even while its keyboard focus
                // no longer blocks navigation elsewhere. Leaving it must rearm without a jump.
                let at = h.ctx.read_response(focus).unwrap().rect.center();
                let moved = h.app.cam;
                h.call("ui.move", json!({"x":at.x,"y":at.y,"alt":alt,"shift":!alt}));
                h.call("ui.move", json!({"x":at.x+1.0,"y":at.y,"alt":alt,"shift":!alt}));
                assert_eq!(h.app.cam, moved, "pointer motion over a field must stay with the field");
                h.call("ui.move", json!({"x":600,"y":350,"alt":alt,"shift":!alt}));
                assert_eq!(h.app.cam, moved, "returning to the viewport must not jump");
                h.call("ui.move", json!({"x":605,"y":350,"alt":alt,"shift":!alt}));
                assert_ne!(h.app.cam, moved, "navigation resumes after leaving the field");
                let resumed = h.app.cam;
                h.call("ui.move", json!({"x":600,"y":350}));
                h.frames(3);
                assert_eq!(h.app.cam, resumed, "releasing the modifier ends navigation");
                assert_eq!(format!("{:?}", h.app.dialog.as_ref().unwrap().kind), values);
                assert_eq!(h.app.session.doc.to_json(), doc);
                assert_eq!(h.app.session.undo.len(), undo);
                assert_eq!(h.ctx.memory(|m| m.focused()), Some(focus));

                h.call("ui.key", json!({"key":"A","cmd":true}));
                h.call("ui.text", json!({"text":"7"}));
                assert_eq!(h.app.dialog.as_mut().unwrap().primary().unwrap().2.as_str(), "7", "typing still reaches the focused field");
                assert_eq!(h.app.cam, resumed);
            }
        }
    }
}

#[test]
fn touchpad_motion_uses_the_effective_perspective_projection() {
    let mut app = hover_app(false);
    app.ui.perspective = true;
    app.cam = Camera { yaw: 0.0, pitch: 0.0, distance: 100.0, ..Default::default() };
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
    let pivot = Vec3::new(15.0, -50.0, 0.0);
    let before = projection(&app, rect).to_screen(pivot).unwrap();
    camera_motion(&mut app, rect, json!({"motion":"orbit","delta":[20,10],"pivot":pivot}));
    let after = projection(&app, rect).to_screen(pivot).unwrap();
    assert!(before.distance(after) < 1e-3, "rendered pivot drifted from {before:?} to {after:?}");
    assert_eq!(app.cam.fov, 0.0, "the projection toggle remains UI state, not a stored-camera mutation");
}

#[test]
fn touchpad_motion_does_not_move_the_camera_under_a_canvas_triad() {
    let ctx = crate::fonts::test_context();
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length":20,"width":20,"height":20})).unwrap();
    s.selection = vec![Sel::Body { name: "Body1".into() }];
    let mut app = SolveApp::new(s, crate::Services::default());
    app.preferences.nav = "touchpad".into();
    app.start("solid.move");
    app.dialog.as_mut().unwrap().focus = false;
    let p = pos2(100.0, 100.0);
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    let triad = crate::dialogs_move::triad(&app, app.dialog.as_ref().unwrap()).unwrap();
    let handle = projection(&app, app.viewport.rect.unwrap()).to_screen(triad.center).unwrap();
    let mods = egui::Modifiers { alt: true, ..Default::default() };
    hover_frame(&mut app, &ctx, vec![egui::Event::ModifiersChanged(mods)]);
    assert!(!ctx.egui_wants_keyboard_input());
    let before = app.cam;
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(handle)]);
    assert_eq!(app.cam, before, "a triad handle owns its input before the camera changes");
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
    assert_eq!(app.cam, before, "leaving the handle rearms without a jump");
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(5.0, 0.0))]);
    assert_ne!(app.cam.yaw, before.yaw, "the exposed viewport remains navigable with Move open");
}

#[test]
fn touchpad_motion_preserves_input_ownership_and_rearms_without_jumping() {
    let ctx = crate::fonts::test_context();
    let mut app = hover_app(false);
    // The classic view's navigation bar (the SketchUp screen has none).
    crate::test_support::classic_view(&mut app);
    app.preferences.nav = "touchpad".into();
    app.preferences.orbit_around = crate::prefs::OrbitAround::Cursor;
    let p = pos2(350.0, 300.0);
    let alt = egui::Modifiers { alt: true, ..Default::default() };
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
    hover_frame(&mut app, &ctx, vec![egui::Event::ModifiersChanged(alt)]);
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(10.0, 0.0))]);
    let pivot = app.viewport.orbit_pivot;
    assert!(pivot.is_some());
    hover_frame(&mut app, &ctx, vec![]);
    assert_eq!(app.viewport.orbit_pivot, pivot, "pauses keep the same orbit pivot");
    let before = app.cam;
    let bar = app.viewport.layout.as_ref().unwrap().navigation.center();
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(bar)]);
    assert_eq!(app.cam, before, "the navigation bar owns its input");
    assert!(app.viewport.orbit_pivot.is_none());
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
    assert_eq!(app.cam, before, "reentry must not apply the distance from an overlay");
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(5.0, 0.0))]);
    assert_ne!(app.cam.yaw, before.yaw);
    let before = app.cam;
    for mods in [
        egui::Modifiers::NONE,
        egui::Modifiers { shift: true, alt: true, ..Default::default() },
        egui::Modifiers { shift: true, ctrl: true, ..Default::default() },
        egui::Modifiers { alt: true, command: true, mac_cmd: true, ..Default::default() },
    ] {
        hover_frame(&mut app, &ctx, vec![egui::Event::ModifiersChanged(mods), egui::Event::PointerMoved(p)]);
        assert_eq!(app.cam, before, "unbound combinations must not navigate");
    }
    let shift = egui::Modifiers { shift: true, ..Default::default() };
    hover_frame(&mut app, &ctx, vec![egui::Event::ModifiersChanged(shift), egui::Event::PointerMoved(p)]);
    hover_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: shift }],
    );
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(30.0, 20.0))]);
    assert_eq!(app.cam, before, "Shift+left drag must still box-select, not pan");
    assert!(app.viewport.boxsel.is_some());
    hover_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton { pos: p + vec2(30.0, 20.0), button: egui::PointerButton::Primary, pressed: false, modifiers: shift }],
    );
    assert_eq!(app.cam, before, "the release frame still belongs to selection");
}

#[test]
fn touchpad_focus_and_preset_changes_discard_old_motion() {
    let ctx = crate::fonts::test_context();
    let mut app = hover_app(false);
    app.preferences.nav = "touchpad".into();
    let alt = egui::Modifiers { alt: true, ..Default::default() };
    let frame = |app: &mut SolveApp, focused: bool, p: Pos2| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                focused,
                events: vec![egui::Event::ModifiersChanged(alt), egui::Event::PointerMoved(p)],
                ..Default::default()
            },
            |ui| show(app, ui),
        );
        out.textures_delta.clear();
    };
    frame(&mut app, true, pos2(300.0, 300.0));
    frame(&mut app, true, pos2(310.0, 300.0));
    let before = app.cam;
    frame(&mut app, false, pos2(500.0, 400.0));
    frame(&mut app, true, pos2(350.0, 300.0));
    assert_eq!(app.cam, before, "focus loss and regain must not jump");
    frame(&mut app, true, pos2(360.0, 300.0));
    assert!((app.cam.yaw - before.yaw - 0.08).abs() < 1e-6);
    app.preferences.nav = "fusion".into();
    let before = app.cam;
    frame(&mut app, true, pos2(450.0, 300.0));
    assert_eq!(app.cam, before);
    app.preferences.nav = "touchpad".into();
    frame(&mut app, true, pos2(500.0, 300.0));
    assert_eq!(app.cam, before, "changing presets must rearm before moving");
    app.viewport.spin = true;
    frame(&mut app, true, pos2(510.0, 300.0));
    assert!(!app.viewport.spin, "pointer-only navigation stops continuous orbit");
    assert!((app.cam.yaw - before.yaw - 0.08).abs() < 1e-6);
}

#[test]
fn touchpad_vertical_trackpad_scroll_zooms_without_overwriting_saved_pan() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.nav = "touchpad".into();
    h.app.preferences.smooth_wheel_zoom = false;
    h.app.preferences.zoom_to_cursor = false;
    h.call("ui.move", json!({"x":800,"y":500}));
    let before = h.app.cam;
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            modifiers: egui::Modifiers::NONE,
            phase,
        });
        h.frame();
    }
    assert!((h.app.cam.distance - before.distance * (-0.18_f64).exp()).abs() < 1e-6);
    assert_eq!(h.app.cam.target, before.target);
    assert!(h.app.preferences.trackpad_pan);
    let after = h.app.cam;
    h.frames(60);
    assert_eq!(h.app.cam, after);
    h.app.preferences.nav = "fusion".into();
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            modifiers: egui::Modifiers::NONE,
            phase,
        });
        h.frame();
    }
    assert_eq!(h.app.cam.distance, after.distance);
    assert_ne!(h.app.cam.target, after.target);
}

#[test]
fn hover_stays_off_between_navigation_events_and_returns_on_release() {
    for gpu in [false, true] {
        for button in [egui::PointerButton::Middle, egui::PointerButton::Secondary, egui::PointerButton::Primary] {
            let ctx = crate::fonts::test_context();
            let mut app = hover_app(gpu);
            if button == egui::PointerButton::Primary {
                app.viewport.nav = Some(NavMode::Orbit);
            }
            hover_frame(&mut app, &ctx, vec![]);
            let p = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
            hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
            assert!(app.viewport.hover.is_some());
            hover_frame(&mut app, &ctx, vec![egui::Event::PointerButton { pos: p, button, pressed: true, modifiers: Default::default() }]);
            // The lit mode owns drags, not a press held still (which may be opening Select
            // Other, #8864): the highlight stays until the press drags.
            let lit = button == egui::PointerButton::Primary;
            assert_eq!(app.viewport.hover.is_some(), lit, "hover on a {button:?} press");
            if lit {
                hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(1.0, 0.0))]);
                assert!(app.viewport.hover.is_some(), "a press within click distance is still");
            }
            let offsets = if lit { [20.0, 21.0, 22.0] } else { [1.0, 2.0, 3.0] };
            for offset in offsets {
                hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p + vec2(offset, 0.0))]);
                assert!(app.viewport.hover.is_none(), "hover while moving");
                for _ in 0..3 {
                    hover_frame(&mut app, &ctx, vec![]);
                    assert!(app.viewport.hover.is_none(), "hover between movement events");
                }
            }
            hover_frame(&mut app, &ctx, vec![egui::Event::PointerButton { pos: p, button, pressed: false, modifiers: Default::default() }]);
            assert!(app.viewport.hover.is_some(), "hover must return on release");
        }
    }
}

fn device_face_app(gpu: bool) -> SolveApp {
    let mut app = hover_app(gpu);
    app.session = sketcraft_engine::Session::default();
    app.session.execute("solid.box", &json!({"length":40,"width":40,"height":40,"corner":[-20,-20,-20]})).unwrap();
    app.set_origin_shown(false);
    app.cam.target = Vec3::ZERO;
    app.cam.distance = 110.0;
    app.cam.yaw = 0.5;
    app.cam.pitch = 0.5;
    app
}

fn device_hover_frame(app: &mut SolveApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    use crate::space_mouse::{DeviceMotion, Motion};
    let before = app.cam;
    app.viewport.device_motion = Some(DeviceMotion { motion: Motion { translation: [0.0; 3], rotation: [0.0, 0.0, 0.1] }, seconds: 0.016 });
    hover_frame(app, ctx, events);
    assert_ne!(app.cam, before, "the device must keep moving the camera");
    assert!(app.viewport.device_motion.is_none());
}

#[test]
fn midpoint_picking_cache_follows_tool_lifecycle() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.app.run("sketch.line", json!({"points":[[10,10],[30,10]],"infer":false})).unwrap();
    h.call("ui.view", json!({"view":"top"}));
    h.frames(3);
    let proj = projection(&h.app, h.app.viewport.rect.unwrap());
    let at = proj.to_screen(Vec3::new(20.0, 10.0, 0.0)).unwrap();
    let is_mid = |hits: Vec<Hit>| matches!(hits.first(), Some(Hit::SketchMidpoint { id, .. }) if id == "l1");
    assert!(is_mid(pick_cached(&mut h.app, &proj, at)));
    h.app.tool = crate::tools::Tool::for_command("sketch.line");
    assert!(!is_mid(pick_cached(&mut h.app, &proj, at)), "starting a tool invalidates idle hover");
    h.app.tool = None;
    assert!(is_mid(pick_cached(&mut h.app, &proj, at)), "ending a tool restores MID at the same pixel");
    assert!(!is_mid(pick_for_tool(&h.app, &proj, at)), "a temporarily taken tool still picks its operands");
}

#[test]
fn device_navigation_keeps_face_hover_current_for_a_stationary_or_moving_pointer() {
    for gpu in [false, true] {
        let ctx = crate::fonts::test_context();
        let mut app = device_face_app(gpu);
        hover_frame(&mut app, &ctx, vec![]);
        let p = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
        hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
        for offset in [0.0, 15.0, -15.0, 0.0] {
            let at = p + vec2(offset, 0.0);
            let mut key = app.viewport.last_pick.as_ref().unwrap().0;
            for events in [vec![egui::Event::PointerMoved(at)], vec![], vec![]] {
                device_hover_frame(&mut app, &ctx, events);
                let proj = projection(&app, app.viewport.rect.unwrap());
                let (expected, _) = candidate(&app, &pick(&app, &proj, at)).unwrap();
                assert!(matches!(expected, Hit::Face { .. }));
                assert_eq!(app.viewport.hover.as_ref(), Some(&expected), "hover must use the current camera and pointer");
                let next = app.viewport.last_pick.as_ref().unwrap().0;
                assert_ne!(next, key, "a moving camera must invalidate cached picking");
                key = next;
            }
        }
        hover_frame(&mut app, &ctx, vec![]);
        assert!(matches!(app.viewport.hover, Some(Hit::Face { .. })));
    }
}

#[test]
fn device_navigation_allows_face_clicks_and_preserves_modifier_selection() {
    for gpu in [false, true] {
        for (modifiers, add) in [(egui::Modifiers::NONE, false), (egui::Modifiers::SHIFT, true), (egui::Modifiers::COMMAND, true)] {
            let ctx = crate::fonts::test_context();
            let mut app = device_face_app(gpu);
            let existing = Sel::Body { name: "Body1".into() };
            app.session.selection = vec![existing.clone()];
            hover_frame(&mut app, &ctx, vec![]);
            let p = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
            device_hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p - vec2(15.0, 0.0))]);
            assert_eq!(app.session.selection, vec![existing.clone()], "device motion alone must retain selection");
            let at = p + vec2(15.0, 0.0);
            device_hover_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers },
                ],
            );
            device_hover_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers }],
            );
            let proj = projection(&app, app.viewport.rect.unwrap());
            let (_, selected) = candidate(&app, &pick(&app, &proj, at)).unwrap();
            assert!(matches!(selected, Sel::Face { .. }));
            let expected = if add { vec![existing, selected] } else { vec![selected] };
            assert_eq!(app.session.selection, expected, "the click must select the face under the current camera");
        }
    }
}

/// Two-handed use: while a 3D mouse turns the view, the pointer still preselects what is
/// under it in the view as it is now, and a click picks it (it neither does nothing nor
/// clears the selection).
#[test]
fn device_navigation_keeps_hover_and_click_picking() {
    use crate::space_mouse::{DeviceMotion, Motion};
    for gpu in [false, true] {
        let ctx = crate::fonts::test_context();
        let mut s = sketcraft_engine::Session::default();
        s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20})).unwrap();
        let mut app = SolveApp::new(s, crate::Services::default());
        app.set_origin_shown(false);
        if gpu {
            app.viewport.gpu =
                Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
        }
        app.cam.target = Vec3::new(20.0, 15.0, 10.0);
        hover_frame(&mut app, &ctx, vec![]);
        // The middle of the top face.
        let p = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::new(20.0, 15.0, 20.0)).unwrap();
        hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
        assert!(matches!(app.viewport.hover, Some(Hit::Face { .. })), "{:?}", app.viewport.hover);
        let turn = || Some(DeviceMotion { motion: Motion { translation: [0.0; 3], rotation: [0.0, 0.0, 0.1] }, seconds: 0.016 });
        let mut pick = app.viewport.last_pick.as_ref().unwrap().0;
        for _ in 0..3 {
            let before = app.cam;
            app.viewport.device_motion = turn();
            hover_frame(&mut app, &ctx, vec![]);
            assert_ne!(app.cam, before, "the device turns the view");
            assert!(matches!(app.viewport.hover, Some(Hit::Face { .. })), "hover stays on while the device moves the view");
            let now = app.viewport.last_pick.as_ref().unwrap().0;
            assert_ne!(now, pick, "the pick follows the turned view");
            pick = now;
        }
        // A click while the device keeps turning the view selects the face under the pointer.
        let button =
            |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        app.viewport.device_motion = turn();
        hover_frame(&mut app, &ctx, vec![button(true)]);
        app.viewport.device_motion = turn();
        hover_frame(&mut app, &ctx, vec![button(false)]);
        assert!(matches!(app.session.selection.as_slice(), [Sel::Face { .. }]), "gpu {gpu}: {:?}", app.session.selection);
    }
}

#[test]
fn hover_stays_off_during_spin_and_animated_views() {
    let ctx = crate::fonts::test_context();
    let mut app = hover_app(true);
    hover_frame(&mut app, &ctx, vec![]);
    let p = projection(&app, app.viewport.rect.unwrap()).to_screen(Vec3::ZERO).unwrap();
    hover_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
    assert!(app.viewport.hover.is_some());
    app.viewport.spin = true;
    for _ in 0..3 {
        hover_frame(&mut app, &ctx, vec![]);
        assert!(app.viewport.hover.is_none());
    }
    app.viewport.spin = false;
    hover_frame(&mut app, &ctx, vec![]);
    assert!(app.viewport.hover.is_some());
    app.cam_anim = Some(sketcraft_engine::render::CameraAnim::new(app.cam, app.cam, 0.0));
    for _ in 0..3 {
        hover_frame(&mut app, &ctx, vec![]);
        assert!(app.viewport.hover.is_none());
    }
    app.cam_anim = None;
    hover_frame(&mut app, &ctx, vec![]);
    assert!(app.viewport.hover.is_some());
}

/// After a close-up refines a body (#1320), the model draws it from the refined plan (on the
/// GPU) or its mesh (on the CPU), its edges come from that mesh too, and a hovered face lights
/// exactly triangles the model draws, never another triangulation (#1064).
#[test]
fn a_refined_body_highlights_on_its_drawn_triangles() {
    use sketcraft_engine::doc::display_quality::current;
    let tris_of = |bytes: &[u8]| -> Vec<[u32; 9]> {
        bytes
            .chunks(3 * crate::gpu::TRI_SIZE)
            .map(|t| {
                let mut v: Vec<[u32; 3]> = t
                    .chunks(crate::gpu::TRI_SIZE)
                    .map(|c| std::array::from_fn(|i| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap())))
                    .collect();
                v.sort_unstable();
                std::array::from_fn(|i| v[i / 3][i % 3])
            })
            .collect()
    };
    let mesh_tris = |m: &sketcraft_engine::geom::Mesh| -> std::collections::HashSet<[u32; 9]> {
        let mut sc = GpuScene::default();
        for f in 0..m.tri_face.iter().map(|f| *f as usize + 1).max().unwrap_or(0) {
            face_tris(&mut sc, m, f, [0; 4], true);
        }
        tris_of(&sc.tris).into_iter().collect()
    };
    for tess in [true, false] {
        let mut app = cylinder_app();
        app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess, oit: false });
        let body = app.session.world_state().unwrap().bodies[0].clone();
        let before = build_scene(&app).merged();
        let key = scene_key(&app);
        assert!(app.viewport.lod.lock().unwrap().force(&body, current(), 2));
        assert_ne!(scene_key(&app), key, "a refinement rebuilds the scene");
        let (plan, mesh) = refined(&app, &body).unwrap();
        assert!(mesh.triangles.len() > body.plan_mesh().unwrap().triangles.len() * 2);
        let scene = build_scene(&app).merged();
        if tess {
            assert!(std::sync::Arc::ptr_eq(&scene.planned[0].plan, &plan), "faces drawn from the refined plan");
        } else {
            assert_eq!(scene.tris.len(), mesh.triangles.len() * 3 * crate::gpu::TRI_SIZE, "faces built from the refined mesh");
            assert!(scene.tris.len() > before.tris.len());
        }
        // Edges (lines) from the refined mesh: finer polylines.
        assert!(scene.lines.len() > before.lines.len(), "{} vs {}", scene.lines.len(), before.lines.len());
        let drawn = mesh_tris(&mesh);
        let mut lit_faces = 0;
        for index in 0..body.body.face_count() {
            app.viewport.hover = Some(Hit::Face { body: body.name.clone(), index, point: Vec3::ZERO });
            let lit = tris_of(&build_highlight(&app).merged().tris);
            assert!(!lit.is_empty(), "face {index} lights");
            assert!(lit.iter().all(|t| drawn.contains(t)), "face {index}: every lit triangle is a drawn one");
            lit_faces += 1;
        }
        assert!(lit_faces >= 3, "{lit_faces} faces");
    }
}

fn cylinder_app() -> SolveApp {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.cylinder", &json!({"radius": 10, "height": 20})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.set_origin_shown(false);
    app
}

/// A Ø20 × 20 cylinder from a circle drawn as two half arcs, extruded: the arcs on one
/// circle sweep one full-turn wall with its seam edge, and each rim is one closed edge, as
/// SMLib's own cylinder has (a rim or wall in pieces now comes only from an import).
fn arcs_cylinder_app() -> SolveApp {
    let mut s = sketcraft_engine::Session::default();
    s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    s.execute("sketch.arc.three_point", &json!({"start": [10, 0], "end": [-10, 0], "through": [0, 10]})).unwrap();
    s.execute("sketch.arc.three_point", &json!({"start": [-10, 0], "end": [10, 0], "through": [0, -10]})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    s.execute("solid.extrude", &json!({"distance": 20})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.set_origin_shown(false);
    app
}

/// Moving the pointer across a face keeps its highlight (a scanned mesh is one face of
/// millions of triangles): only another face, edge or plane builds it again.
#[test]
fn a_face_highlight_is_kept_while_the_pointer_moves_across_the_face() {
    let mut app = cylinder_app();
    let hover = |index: usize, x: f64| Hit::Face { body: "Body1".into(), index, point: Vec3::new(x, 0.0, 5.0) };
    app.viewport.hover = Some(hover(0, 10.0));
    let key = highlight_key(&app);
    let before = build_highlight(&app).merged();
    assert!(!before.tris.is_empty(), "the face is lit");
    app.viewport.hover = Some(hover(0, 9.0));
    assert_eq!(highlight_key(&app), key, "the same face, another point");
    assert_eq!(build_highlight(&app).merged().tris, before.tris);
    app.viewport.hover = Some(hover(1, 9.0));
    assert_ne!(highlight_key(&app), key, "another face");
    app.viewport.hover = Some(Hit::Edge { body: "Body1".into(), index: 0, mid: Vec3::ZERO, point: Vec3::ZERO });
    let edge = highlight_key(&app);
    app.viewport.hover = Some(Hit::Edge { body: "Body1".into(), index: 0, mid: Vec3::X, point: Vec3::X });
    assert_ne!(highlight_key(&app), edge, "an edge's middle picks which part of it is lit");
    app.viewport.hover = Some(Hit::Plane { name: "XY".into(), point: Vec3::ZERO });
    let plane = highlight_key(&app);
    app.viewport.hover = Some(Hit::Plane { name: "XY".into(), point: Vec3::X });
    assert_eq!(highlight_key(&app), plane, "the same plane, another point");
}

#[test]
fn body_highlight_buffers_are_reused_when_the_camera_orbits_or_pans() {
    let mut app = cylinder_app();
    app.set_origin_shown(true);
    app.viewport.hover_bodies = vec!["Body1".into()];
    let key = highlight_key(&app);
    let before = build_highlight(&app).merged();
    assert!(!before.tris.is_empty());
    app.cam.orbit(25.0, 10.0);
    app.cam.roll_by(0.3);
    app.cam.pan(20.0, -15.0, 600.0);
    assert_eq!(highlight_key(&app), key, "world-space feedback must stay resident during navigation");
    let after = build_highlight(&app).merged();
    assert_eq!(before.tris, after.tris);
    assert_eq!(before.lines, after.lines);
    app.cam.zoom_at(1.2, None);
    assert_ne!(highlight_key(&app), key, "reference geometry follows the view's scale");

    let mut app = hover_app(true);
    let sketch = app.session.active_sketch.unwrap();
    app.run("sketch.circle.center", json!({"center":[0,0],"radius":5})).unwrap();
    app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
    let key = highlight_key(&app);
    let before = build_highlight(&app).merged();
    assert!(!before.trans.is_empty());
    app.cam.orbit(25.0, 10.0);
    assert_ne!(highlight_key(&app), key, "profile feedback still follows the projected curves and lift toward the eye");
    assert_ne!(before.trans, build_highlight(&app).merged().trans);
}

#[test]
fn world_highlight_block_survives_zoom_and_refreshes_when_bodies_are_hidden() {
    let mut app = cylinder_app();
    // The classic view's origin planes (the SketchUp screen shows the axes alone).
    crate::test_support::classic_view(&mut app);
    app.set_origin_shown(true);
    app.viewport.hover_bodies = vec!["Body1".into()];
    let before = build_highlight(&app);
    let bytes = before.blocks[0].bytes.clone();
    assert!(!bytes.vertices.is_empty());
    app.cam.zoom_at(4.0, None);
    let after = build_highlight(&app);
    assert!(std::sync::Arc::ptr_eq(&bytes, &after.blocks[0].bytes), "zoom must not regenerate or upload body feedback");
    assert_ne!(before.guide_trans, after.guide_trans, "reference planes still resize with the view");
    app.session.visibility.hidden_bodies.push("Body1".into());
    let hidden = build_highlight(&app);
    assert!(!std::sync::Arc::ptr_eq(&bytes, &hidden.blocks[0].bytes));
    assert!(hidden.blocks[0].bytes.vertices.is_empty(), "hidden bodies must lose their feedback");
    app.session.visibility.hidden_bodies.clear();
    let shown = build_highlight(&app);
    assert_eq!(shown.blocks[0].bytes.vertices, bytes.vertices);
}

#[test]
fn whole_body_highlight_preserves_each_triangle_and_selected_face_once() {
    let mut app = cylinder_app();
    let body = app.session.world_state().unwrap().bodies[0].clone();
    let expected = shown_mesh(&app, &body, &None).triangles.len() * 3 * crate::gpu::TRI_SIZE;
    app.viewport.hover_bodies = vec![body.name.clone()];
    for selected in
        [vec![], vec![Sel::Face { body: body.name.clone(), index: 0, point: Vec3::ZERO }], vec![Sel::Body { name: body.name.clone() }]]
    {
        app.session.selection = selected;
        assert_eq!(build_highlight(&app).merged().tris.len(), expected, "whole-body feedback must neither omit nor duplicate selected faces");
    }
}

/// Every body selected (Select All on an assembly) looks each body up by name in a map, not
/// along the world's bodies: the map finds the same body a scan finds for every name (the
/// first one, were a name held twice) and none for a name no body has, and the highlight
/// lights every selected body once, with a selected face and edge as before.
#[test]
fn selected_bodies_found_by_name_are_the_bodies_a_scan_finds() {
    let mut app = cylinder_app();
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: true, oit: false });
    app.run("solid.box", json!({"length": 4, "width": 4, "height": 4, "center": [40, 0, 2], "operation": "new", "name": "Seed"})).unwrap();
    app.run("solid.pattern.rectangular", json!({"features": ["Seed"], "dir1": [1, 0, 0], "count1": 6, "spacing1": 6, "name": "Row"})).unwrap();
    let st = app.session.world_state().unwrap();
    assert_eq!(st.bodies.len(), 7);
    let mut by_name = BodiesByName::new(&st);
    for b in &st.bodies {
        let found = by_name.get(&b.name).expect("every body is found by its name");
        assert!(std::ptr::eq(found, st.body(&b.name).unwrap()), "{}: the body a scan finds", b.name);
    }
    assert!(by_name.get("No such body").is_none());
    let mut twice = (*st).clone();
    let mut copy = twice.bodies[1].clone();
    copy.name = twice.bodies[0].name.clone();
    twice.bodies.push(copy);
    let mut by_name = BodiesByName::new(&twice);
    assert!(std::ptr::eq(by_name.get(&twice.bodies[0].name).unwrap(), &twice.bodies[0]), "a name held twice finds the first body");
    let names: Vec<String> = st.bodies.iter().map(|b| b.name.clone()).collect();
    let first = st.bodies[0].clone();
    drop(st);
    let mut selection: Vec<Sel> = names.iter().map(|name| Sel::Body { name: name.clone() }).collect();
    selection.push(Sel::Face { body: first.name.clone(), index: 0, point: Vec3::ZERO });
    selection.push(Sel::Edge { body: first.name.clone(), index: 0, point: Vec3::ZERO });
    app.session.selection = selection;
    let lit = build_highlight(&app);
    assert_eq!(lit.selected_planned.len() + lit.selected_blocks.len(), names.len(), "every selected body is lit once");
    let merged = lit.merged();
    assert!(!merged.tris.is_empty(), "the selected face is lit");
    assert!(!merged.lines.is_empty() || !merged.overlay.is_empty() || !merged.hidden.is_empty(), "the selected edge is lit");
}

/// A whole highlighted body is lit from the model's own triangles, not by copying every one
/// of them onto the CPU (a whole assembly lit that way took most of a second a frame): from
/// its display plan where the model draws one, else from its vertices in the model scene's
/// blocks. See-through bodies keep the CPU feedback, which colours them.
#[test]
fn whole_body_highlights_reuse_the_models_own_triangles() {
    let mut app = cylinder_app();
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: true, oit: false });
    let body = crate::open_meshing::session_world(&app.session).bodies[0].clone();
    let plan = body.display_plan().expect("the test body is drawn from a plan");
    app.viewport.hover_bodies = vec![body.name.clone()];
    let hovered = build_highlight(&app);
    assert_eq!(hovered.hover_planned.len(), 1, "the whole hovered body is lit from its plan");
    assert!(std::sync::Arc::ptr_eq(&hovered.hover_planned[0], &plan));
    assert_eq!(hovered.tris.len(), 0);
    assert!(hovered.blocks.iter().all(|b| b.bytes.vertices.is_empty()), "no copy of the body's triangles");
    app.viewport.hover_bodies.clear();
    app.session.selection = vec![Sel::Body { name: body.name.clone() }];
    let selected = build_highlight(&app);
    assert_eq!(selected.selected_planned.len(), 1, "a selected whole body is lit from its plan too");
    assert_eq!(selected.tris.len(), 0);
    assert!(selected.blocks.iter().all(|b| b.bytes.vertices.is_empty()));
    // A body the model builds on the CPU (here: no GPU plans) is lit from its own vertices
    // in the model scene's blocks.
    app.session.selection.clear();
    app.viewport.hover_bodies.clear();
    app.viewport.gpu = None;
    build_scene(&app);
    app.viewport.hover_bodies = vec![body.name.clone()];
    let built = build_highlight(&app);
    assert!(built.hover_planned.is_empty());
    assert_eq!(built.hover_blocks.len(), 1, "a CPU-built body is lit from the model's block");
    assert_eq!(built.tris.len(), 0);
    assert!(built.blocks.iter().all(|b| b.bytes.vertices.is_empty()), "no copy of the body's triangles");
    // Two occurrences of one component are drawn as placements of its shared triangles: the
    // highlight draws those placements again, in its colour.
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.cylinder", &json!({"radius": 10, "height": 20, "body_name": "Sleeve"})).unwrap();
    let made = s.execute("component.from_bodies", &json!({"bodies": ["Sleeve"]})).unwrap();
    let part = made["components"][0]["component"].as_u64().unwrap();
    s.execute("occurrence.copy", &json!({"component": part, "translate": [40, 0, 0]})).unwrap();
    let mut assembly = SolveApp::new(s, crate::Services::default());
    assembly.set_origin_shown(false);
    let names: Vec<String> = crate::open_meshing::session_world(&assembly.session).bodies.iter().map(|b| b.name.clone()).collect();
    assert_eq!(names.len(), 2, "two placements of the part");
    build_scene(&assembly);
    assembly.viewport.hover_bodies = names;
    let instanced = build_highlight(&assembly);
    assert_eq!(instanced.hover_instanced.len(), 1, "the part's one shared mesh");
    assert_eq!(instanced.hover_instanced[0].count(), 2, "both of its placements");
    assert_eq!(instanced.tris.len(), 0);
    assert!(instanced.blocks.iter().all(|b| b.bytes.vertices.is_empty()), "no copy of the placements");
    // A see-through body is not in the block's opaque runs: its own vertices in the block are
    // drawn again, recoloured in the highlight's palette, and still no triangle is copied.
    app.viewport.hover_bodies.clear();
    app.run("appearance.assign", json!({"bodies": [body.name.clone()], "color": [200, 200, 200], "opacity": 0.5})).unwrap();
    build_scene(&app);
    app.viewport.hover_bodies = vec![body.name.clone()];
    let see_through = build_highlight(&app);
    assert!(see_through.hover_planned.is_empty(), "a see-through body is not drawn from a plan");
    assert_eq!(see_through.recoloured.len(), 1, "it is lit from the model's own vertices");
    assert!(!see_through.recoloured[0].vertices.is_empty());
    assert!(see_through.blocks.iter().all(|b| b.bytes.vertices.is_empty()), "no copy of its triangles");
}

/// A whole faded or see-through body lit from the model's own vertices, recoloured in the
/// highlight's palette, looks exactly as the copy of its triangles it replaces did: hovered,
/// and selected. Skipped without a GPU adapter.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn faded_and_see_through_bodies_light_as_their_copies_did() {
    const SIZE: u32 = 128;
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 30, "width": 20, "height": 10, "corner": [0, 0, 0], "body_name": "Plate"})).unwrap();
    s.execute("solid.cylinder", &json!({"base": [45, 10, 0], "diameter": 12, "height": 25, "body_name": "Post"})).unwrap();
    s.execute("component.from_bodies", &json!({"bodies": ["Post"]})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32));
    let proj = projection(&app, rect);
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render = |model: GpuScene, highlight: GpuScene| {
        crate::gpu::render_offscreen(
            model,
            Some(highlight),
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&Tokens::get().analysis),
        )
    };
    for case in ["see-through", "faded"] {
        match case {
            "see-through" => drop(app.run("appearance.assign", json!({"bodies": ["Plate"], "color": "#88aacc", "opacity": 0.4})).unwrap()),
            _ => {
                app.run("appearance.assign", json!({"bodies": ["Plate"]})).unwrap();
                app.run("component.activate", json!({"component": "Post"})).unwrap();
            }
        }
        for selected in [false, true] {
            app.viewport.hover_bodies = if selected { Vec::new() } else { vec!["Plate".into()] };
            app.session.selection = if selected { vec![Sel::Body { name: "Plate".into() }] } else { Vec::new() };
            let model = build_scene(&app);
            let highlight = build_highlight(&app);
            assert_eq!(highlight.recoloured.len(), 1, "{case}: the plate is lit from the model's vertices");
            let highlight_handed = highlight.clone();
            // The copy it replaces: every triangle, lit, in the colour the copy was drawn in.
            let t = Tokens::get();
            let colour = if selected {
                c4(t.body_selected)
            } else {
                let c = body_color(&app.session.doc, &app_body(&app, "Plate")).unwrap_or(t.body);
                let k = t.hover_face_lift;
                [c.r().saturating_add(k), c.g().saturating_add(k), c.b().saturating_add(k), 255]
            };
            // The same highlight (its edges and widgets), the plate copied instead of recoloured.
            let mut copy = highlight.clone().merged();
            copy.recoloured.clear();
            let mesh = drawn_mesh(&app, &app_body(&app, "Plate"));
            mesh_tris(&mut copy, &mesh, colour, true, |_| true);
            let (Some(lit), Some(copied)) = (render(model.clone(), highlight_handed), render(model, copy)) else {
                eprintln!("skipping: no GPU adapter");
                return;
            };
            assert!(lit == copied, "{case}, selected {selected}: the recoloured body does not look as its copy did");
        }
        app.session.selection.clear();
    }
}

fn app_body(app: &SolveApp, name: &str) -> sketcraft_engine::doc::ModelBody {
    app.session.world_state().unwrap().body(name).cloned().unwrap()
}

#[test]
fn body_input_hover_lights_the_whole_body_and_refreshes_for_face_inputs() {
    let mut app = cylinder_app();
    app.set_origin_shown(false);
    let body = crate::open_meshing::session_world(&app.session).bodies[0].clone();
    let hit = Hit::Face { body: body.name.clone(), index: 0, point: Vec3::ZERO };
    app.start("file.save_mesh");
    app.viewport.hover = Some(hit.clone());
    let expected = shown_mesh(&app, &body, &None).triangles.len() * 3 * crate::gpu::TRI_SIZE;
    assert_eq!(hovered_body(&app), Some(body.name.clone()));
    assert_eq!(build_highlight(&app).merged().tris.len(), expected);
    let body_key = highlight_key(&app);
    for accept in [crate::selection::FACES, crate::selection::FACES | crate::selection::BODIES] {
        app.dialog.as_mut().unwrap().inputs[0].accept = accept;
        assert_eq!(hovered_body(&app), None, "face inputs must keep face feedback");
        assert_ne!(highlight_key(&app), body_key, "a changed input must invalidate cached body feedback");
        assert!(build_highlight(&app).merged().tris.len() < expected);
    }
    app.dialog.as_mut().unwrap().inputs[0].accept = crate::selection::BODIES;
    let (_, sel) = candidate(&app, std::slice::from_ref(&hit)).unwrap();
    dialog_pick(&mut app, &hit, sel);
    assert!(app.highlighted().contains(&Sel::Body { name: body.name.clone() }));
    assert_eq!(build_highlight(&app).merged().tris.len(), expected, "hover must not duplicate the selected body's faces");
}

#[test]
fn smooth_wall_display_preserves_wireframe_and_edge_selection() {
    let mut s = sketcraft_engine::Session::default();
    let sketch = s.execute("sketch.create", &json!({"plane":"XY"})).unwrap()["sketch"].as_u64().unwrap();
    // A circle r10 drawn as eight cubic sketch splines, each the same eighth-arc turned by 45
    // degrees, so every two neighbours meet with one tangent and one curvature (G2).
    let step = std::f64::consts::TAU / 8.;
    let k = 4. / 3. * (step / 4.).tan() * 10.;
    for i in 0..8 {
        let (a, b) = (i as f64 * step, (i + 1) as f64 * step);
        let p0 = [10. * a.cos(), 10. * a.sin()];
        let p3 = if i == 7 { [10., 0.] } else { [10. * b.cos(), 10. * b.sin()] };
        let p1 = [p0[0] - k * a.sin(), p0[1] + k * a.cos()];
        let p2 = [p3[0] + k * b.sin(), p3[1] - k * b.cos()];
        s.execute("sketch.spline.control_point", &json!({"points":[p0,p1,p2,p3]})).unwrap();
    }
    s.execute("solid.extrude", &json!({"sketch":sketch,"distance":3})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.ui.ground_shadow = false;
    let st = app.session.world_state().unwrap();
    let b = st.bodies.first().unwrap();
    let mesh = b.mesh();
    // Each spline extrudes as one wall: eight walls and two caps. Edges: the eight-piece loop
    // on each cap and one upright edge where each two walls meet: 24.
    assert_eq!(mesh.edges.len(), 24);
    // The walls are distinct faces meeting with one curvature (G2) along the eight upright
    // edges, so the shaded view draws none of them.
    assert_eq!(mesh.smooth_edges.iter().filter(|f| **f).count(), 8);
    let smooth = mesh.smooth_edges.iter().position(|f| *f).unwrap();
    let expected = mesh.edges.iter().enumerate().filter(|(i, _)| mesh.shaded_edge_visible(*i)).map(|(_, e)| e.len() - 1).sum::<usize>();
    // Vertex markers are lines from a point to itself; count the edges' segments.
    let edge_lines = |bytes: &[u8]| bytes.as_chunks::<{ crate::gpu::LINE_SIZE }>().0.iter().filter(|r| r[..12] != r[12..24]).count();
    let shaded = build_scene(&app).merged();
    assert_eq!(edge_lines(&shaded.lines), expected);
    assert!(!shaded.tris.is_empty());
    app.ui.visual_style = 2;
    let wire = build_scene(&app).merged();
    // The wireframe draws every edge but seams, the smooth wall joins among them: they are
    // edges between faces, hidden only when shaded.
    let expected_wire = mesh.edges.iter().enumerate().filter(|(i, _)| mesh.edge_visible(*i)).map(|(_, e)| e.len() - 1).sum::<usize>();
    assert_eq!(edge_lines(&wire.lines), expected_wire);
    assert!(expected_wire > expected);
    assert!(wire.tris.is_empty());
    app.ui.visual_style = 0;
    app.session.selection = vec![Sel::Edge { body: b.name.clone(), index: smooth, point: polyline_mid(&mesh.edges[smooth]) }];
    assert!(!build_highlight(&app).merged().lines.is_empty());
    let mut without_display_mask = (*mesh).clone();
    without_display_mask.smooth_edges.clear();
    assert_eq!(mesh.surface_patch(mesh.edge_faces[smooth][0] as usize), without_display_mask.surface_patch(mesh.edge_faces[smooth][0] as usize));
    let cam = sketcraft_engine::view::home_camera(&app.session).unwrap();
    let cpu = sketcraft_engine::view::scene(&app.session, &cam).unwrap();
    let actual_edges = cpu.lines.iter().filter(|line| line.color == sketcraft_engine::view::colors::EDGE).count();
    assert_eq!(actual_edges, mesh.edges.iter().enumerate().filter(|(i, _)| mesh.shaded_edge_visible(*i)).count());
    if let Ok(directory) = sketcraft_engine::geom::env::var("SMOOTH_SCREENSHOTS").ok_or(()) {
        std::fs::create_dir_all(&directory).unwrap();
        for (name, old_edges, wireframe) in [("after", false, false), ("before", true, false), ("wireframe", true, true)] {
            let mut scene = sketcraft_engine::view::scene(&app.session, &cam).unwrap();
            scene.lines.retain(|line| line.color == sketcraft_engine::view::colors::EDGE);
            if old_edges {
                scene.lines.clear();
                for (i, edge) in mesh.edges.iter().enumerate() {
                    if mesh.edge_pickable(i) {
                        scene.lines.push(sketcraft_engine::render::SceneLine {
                            points: edge.clone(),
                            color: sketcraft_engine::view::colors::EDGE,
                            alpha: 1.0,
                            width: 1.2,
                            on_top: false,
                        });
                    }
                }
            }
            if wireframe {
                scene.meshes.clear();
            }
            assert!(crate::gpu::test_picture_renderer(), "SMOOTH_SCREENSHOTS needs a GPU");
            let png = sketcraft_engine::capture::png(&scene, &cam, 1200, 800).unwrap();
            std::fs::write(std::path::Path::new(&directory).join(format!("smooth-walls-{name}.png")), png.bytes).unwrap();
        }
    }
}

#[test]
fn certified_small_crease_overrides_old_seam_fit_for_picking_and_patches() {
    let mut s = sketcraft_engine::Session::default();
    let sketch = s.execute("sketch.create", &json!({"plane":"XY"})).unwrap()["sketch"].as_u64().unwrap();
    let step = std::f64::consts::TAU / 8.;
    let k = 4. / 3. * (step / 4.).tan() * 10.;
    for i in 0..8 {
        let a = i as f64 * step;
        let b = (i + 1) as f64 * step;
        let p0 = [10. * a.cos(), 10. * a.sin()];
        let p3 = if i == 7 { [10., 0.] } else { [10. * b.cos(), 10. * b.sin()] };
        let mut p1 = [p0[0] - k * a.sin(), p0[1] + k * a.cos()];
        let mut p2 = [p3[0] + k * b.sin(), p3[1] - k * b.cos()];
        if i == 0 {
            p1[0] += k * 1e-6;
            p2[0] += k * 1e-6 * b.cos();
            p2[1] += k * 1e-6 * b.sin();
        }
        s.execute("sketch.spline.control_point", &json!({"points":[p0,p1,p2,p3]})).unwrap();
    }
    s.execute("solid.extrude", &json!({"sketch":sketch,"distance":3})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    let st = app.session.world_state().unwrap();
    let body = st.bodies.first().unwrap();
    let mesh = body.mesh();
    let crease = mesh
        .edges
        .iter()
        .position(|line| {
            line.first().zip(line.last()).is_some_and(|(a, b)| {
                (a.x - 10.).abs() < 1e-8 && a.y.abs() < 1e-8 && (b.x - 10.).abs() < 1e-8 && b.y.abs() < 1e-8 && (a.z - b.z).abs() > 2.9
            })
        })
        .unwrap();
    // Truck's coarse seam fit took this crease for a seam between pieces of one surface;
    // the seam flags now compare the exact surfaces (`topo::seam_flags`). Either way the
    // certified crease is sharp, drawn in wireframe and picked. Its kink of a millionth is
    // too close to tangent to show in the shading (`smooth_edges::Join`: smooth and a
    // measured crease at once), so the shaded view leaves it out.
    assert!(mesh.sharp_edges[crease]);
    assert!(mesh.edge_visible(crease) && !mesh.shaded_edge_visible(crease));
    assert!(mesh.edge_pickable(crease));
    assert!(mesh.edge_faces[crease].iter().any(|f| mesh.surface_patch(*f as usize).len() == 1));
    let mid = polyline_mid(&mesh.edges[crease]);
    app.cam = app.cam.looking_from(Vec3::new(30., 0., 1.5));
    app.cam.target = Vec3::new(0., 0., 1.5);
    for perspective in [false, true] {
        app.ui.perspective = perspective;
        let proj = projection(&app, Rect::from_min_size(pos2(0., 0.), vec2(800., 600.)));
        let hits = pick(&app, &proj, proj.to_screen(mid).unwrap());
        assert!(hits.iter().any(|hit| matches!(hit,Hit::Edge{index,..} if *index==crease)), "{hits:?}");
    }
}

#[test]
fn circular_items_measure_as_whole_rims_and_surfaces() {
    let mut app = arcs_cylinder_app();
    let st = app.session.world_state().unwrap();
    let b = &st.bodies[0];
    let m = b.mesh();
    let edges: Vec<Sel> = m
        .edges
        .iter()
        .enumerate()
        .filter(|(_, p)| p.len() > 3 && p.iter().all(|q| (q.z - 20.0).abs() < 1e-6))
        .map(|(index, p)| Sel::Edge { body: b.name.clone(), index, point: polyline_mid(p) })
        .collect();
    let result = app.session.measure_items(&edges);
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert!((result["items"][0]["length_mm"].as_f64().unwrap() - 20.0 * std::f64::consts::PI).abs() < 0.2, "{result}");
    app.session.selection = edges;
    let d = crate::dialogs::Dialog::for_command(&app, "inspect.measure").unwrap();
    assert_eq!(d.items().len(), 1);

    let faces = m.edge_faces.iter().zip(&m.seams).find(|(_, seam)| **seam).unwrap().0;
    assert_eq!(faces.len(), 1, "the seam is inside the one wall");
    let wall = Sel::Face { body: b.name.clone(), index: faces[0] as usize, point: Vec3::new(10.0, 0.0, 10.0) };
    let wall_pieces = grouped_selection(&app.session, &wall);
    assert_eq!(wall_pieces, std::slice::from_ref(&wall));
    let result = app.session.measure_items(&wall_pieces);
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert!((result["items"][0]["area_mm2"].as_f64().unwrap() - 400.0 * std::f64::consts::PI).abs() < 10.0, "{result}");
    let mut d = crate::dialogs::Dialog::for_command(&app, "inspect.measure").unwrap();
    d.pick(&app.session, wall.clone());
    assert_eq!(d.items().len(), 2, "a rim and a wall are two measurement targets");
    d.pick(&app.session, Sel::Face { body: b.name.clone(), index: faces[0] as usize, point: Vec3::new(-10.0, 0.0, 10.0) });
    assert_eq!(d.items().len(), 1, "a pick across the seam toggles the same wall");
}

#[test]
fn colour_analyses_show_a_legend() {
    // #973: Draft, Accessibility and Curvature Map say what their colours mean; Zebra and no
    // analysis draw no legend.
    let mut h = crate::scenario::Harness::new();
    // Keep the existing corner legend available through the HUD compatibility setting.
    h.app.preferences.viewport_analysis_status = false;
    h.app.home.open = false;
    h.app.run("solid.box", json!({"length":20,"width":20,"height":20})).unwrap();
    let legend = |h: &mut crate::scenario::Harness| {
        h.frames(2);
        crate::scenario::handle_at("analysis:legend").is_some()
    };
    assert!(!legend(&mut h));
    for (cmd, shown) in [("inspect.draft", true), ("inspect.accessibility", true), ("inspect.curvature_map", true), ("inspect.zebra", false)] {
        h.app.session.analysis = None;
        h.app.run(cmd, json!({})).unwrap();
        assert!(h.app.session.analysis.is_some(), "{cmd}");
        assert_eq!(legend(&mut h), shown, "{cmd}");
    }
    h.app.session.analysis = None;
    assert!(!legend(&mut h));
}

#[test]
fn a_sketch_slice_cuts_at_the_sketch_while_a_section_is_on() {
    // #10705: with a Section Analysis on, turning Slice on in a sketch did nothing.
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":40,"width":30,"height":20,"body_name":"Block"})).unwrap();
    h.app.run("construct.plane.offset", json!({"base":"XY","offset":10,"name":"Mid"})).unwrap();
    h.app.run("sketch.create", json!({"plane":"Mid"})).unwrap();
    h.app.run("inspect.section", json!({"plane":"XZ","offset":15,"hatch":false})).unwrap();
    h.frames(2);
    let across_y = |h: &crate::scenario::Harness| section_plane(&h.app).is_some_and(|(o, n)| n.y.abs() > 0.99 && (o.y.abs() - 15.0).abs() < 1e-9);
    assert!(across_y(&h));
    assert!(!section_hatch(&h.app));
    h.app.run("sketch.options", json!({"slice":true})).unwrap();
    h.frames(2);
    let (o, n) = section_plane(&h.app).expect("the slice");
    assert!(n.z.abs() > 0.99 && (o.z - 10.0).abs() < 1e-9, "{o:?} {n:?}");
    assert!(section_hatch(&h.app), "a slice is hatched as it is without a section");
    // Slice off, or the sketch closed: the section is back.
    h.app.run("sketch.options", json!({"slice":false})).unwrap();
    assert!(across_y(&h));
    h.app.run("sketch.options", json!({"slice":true})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.frames(2);
    assert!(across_y(&h));
}

#[test]
fn a_section_leaves_the_origin_planes_and_axes_whole() {
    // #1229: the origin planes and axes are reference geometry; the section plane cuts the
    // model, not them, so they go to the lists the GPU draws without the section clip.
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":20,"width":20,"height":20,"corner":[-10,-10,-10]})).unwrap();
    h.app.run("inspect.section", json!({"plane": "XZ", "offset": 0})).unwrap();
    h.app.set_origin_shown(true);
    h.frames(2);
    assert!(section_plane(&h.app).is_some());
    let sc = build_highlight(&h.app).merged();
    let planes = origin_planes(&h.app).len();
    assert!(planes > 0);
    assert_eq!(sc.guide_trans.len(), planes * 6 * crate::gpu::TRI_SIZE);
    assert!(sc.trans.is_empty(), "no origin plane is drawn where the section clips");
    // Each origin axis: a solid part and dashes beyond, plus each plane's four edges.
    assert!(sc.guide_lines.len() >= (planes * 4 + 3) * crate::gpu::LINE_SIZE);
    assert!(sc.xray.is_empty(), "depth-tested unless the preference is on");
    // Shown through bodies (Preferences), they are drawn over everything, also unclipped.
    h.app.preferences.reference_geometry_through_bodies = true;
    let sc = build_highlight(&h.app).merged();
    assert_eq!(sc.xray.len(), planes * 6 * crate::gpu::TRI_SIZE);
    assert!(sc.trans.is_empty() && sc.guide_trans.is_empty() && sc.guide_lines.is_empty());
    assert!(sc.overlay.len() >= (planes * 4 + 3) * crate::gpu::LINE_SIZE);
}

#[test]
fn circular_rim_hover_click_and_dialog_pick_take_the_whole_rim() {
    let mut app = arcs_cylinder_app();
    let st = app.session.world_state().unwrap();
    let b = &st.bodies[0];
    let m = b.mesh();
    let top = m.edges.iter().position(|e| e.len() > 3 && e.iter().all(|p| (p.z - 20.0).abs() < 1e-6)).unwrap();
    // The rim is one closed edge round the whole circle.
    assert_eq!(m.circular_rim(top), vec![top]);
    let rim = &m.edges[top];
    assert!(rim.first().zip(rim.last()).is_some_and(|(a, b)| a.dist(*b) < 1e-9));
    assert!(rim.iter().all(|p| (p.x.hypot(p.y) - 10.0).abs() < 1e-6));
    let line_count = rim.len() - 1;
    let mid = polyline_mid(rim);
    app.viewport.hover = Some(Hit::Edge { body: b.name.clone(), index: top, mid, point: mid });
    assert_eq!(build_highlight(&app).merged().lines.len(), 2 * line_count * crate::gpu::LINE_SIZE);
    let item = hit_sel(app.viewport.hover.as_ref().unwrap()).unwrap();
    select(&mut app, Some(item.clone()), false, false);
    assert_eq!(app.session.selection, std::slice::from_ref(&item));
    assert_eq!(build_highlight(&app).merged().lines.len(), 2 * line_count * crate::gpu::LINE_SIZE);
    // Ctrl/Shift clicking the rim deselects it.
    select(&mut app, Some(item.clone()), true, true);
    assert!(app.session.selection.is_empty());
    let mut d = crate::dialogs::Dialog::for_command(&app, "solid.chamfer").unwrap();
    if let crate::dialogs::Kind::Fillet { chain, .. } = &mut d.kind {
        *chain = false;
    }
    d.pick(&app.session, item.clone());
    assert_eq!(d.items().len(), 1);
    d.pick(&app.session, item);
    assert!(d.items().is_empty());
    // Screen-space picking works from either side of the rim as the model turns.
    for at in [rim[rim.len() / 4], rim[3 * rim.len() / 4]] {
        app.cam = app.cam.looking_from(Vec3::new(at.x, at.y, 20.0));
        app.cam.target = Vec3::new(0.0, 0.0, 10.0);
        for perspective in [false, true] {
            app.ui.perspective = perspective;
            let proj = projection(&app, Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)));
            let hits = pick(&app, &proj, proj.to_screen(at).unwrap());
            let (_, sel) = candidate(&app, &hits).unwrap();
            assert!(matches!(sel, Sel::Edge { index, .. } if index == top), "{hits:?}");
            assert_eq!(grouped_selection(&app.session, &sel).len(), 1);
        }
    }
}

#[test]
fn curved_face_hover_selection_and_dialogs_cross_the_seam() {
    let mut app = arcs_cylinder_app();
    let st = app.session.world_state().unwrap();
    let b = &st.bodies[0];
    let m = b.mesh();
    let seam = m.seams.iter().position(|seam| *seam).unwrap();
    let faces = &m.edge_faces[seam];
    assert_eq!(faces.len(), 1, "the seam is inside the one wall");
    let wall = faces[0];
    let index = wall as usize;
    assert_eq!(m.surface_patch(index), vec![index]);
    let triangle_count = m.tri_face.iter().filter(|f| **f == wall).count();
    // The wall's triangles nearest the seam on either side of it, by their centres' turn
    // from the seam round the axis.
    let at = m.edges[seam][0];
    let turn = |p: Vec3| (at.x * p.y - at.y * p.x).atan2(at.x * p.x + at.y * p.y);
    let centres: Vec<Vec3> = m
        .triangles
        .iter()
        .zip(&m.tri_face)
        .filter(|(_, f)| **f == wall)
        .filter_map(|(t, _)| m.tri(t).map(|[a, c, d]| (a + c + d) / 3.0))
        .collect();
    let side =
        |sign: f64| centres.iter().copied().filter(|p| turn(*p) * sign > 0.0).min_by(|p, q| turn(*p).abs().total_cmp(&turn(*q).abs())).unwrap();
    let sides = [side(1.0), side(-1.0)];
    assert!(sides.iter().all(|p| turn(*p).abs() < 0.5), "{sides:?}");
    for (k, &point) in sides.iter().enumerate() {
        app.viewport.hover = Some(Hit::Face { body: b.name.clone(), index, point });
        assert_eq!(build_highlight(&app).merged().tris.len(), triangle_count * 3 * crate::gpu::TRI_SIZE);
        let item = hit_sel(app.viewport.hover.as_ref().unwrap()).unwrap();
        select(&mut app, Some(item.clone()), false, false);
        assert_eq!(app.session.selection, std::slice::from_ref(&item), "keep the original tool placement point");
        assert_eq!(build_highlight(&app).merged().tris.len(), triangle_count * 3 * crate::gpu::TRI_SIZE);
        let mut dialog = crate::dialogs::Dialog::for_command(&app, "solid.chamfer").unwrap();
        assert_eq!(dialog.items().len(), 1);
        dialog.pick(&app.session, item.clone());
        assert!(dialog.items().is_empty());
        dialog.pick(&app.session, item);
        assert_eq!(dialog.items().len(), 1);
        // A point on the other side of the seam still toggles the whole surface off.
        select(&mut app, Some(Sel::Face { body: b.name.clone(), index, point: sides[1 - k] }), true, true);
        assert!(app.session.selection.is_empty());
    }
    // Every face is its own patch: the planar caps, the wall, a box's faces.
    for i in 0..b.body.face_count() {
        assert_eq!(m.surface_patch(i), vec![i]);
    }
    app.session.execute("solid.box", &json!({"length": 10, "width": 10, "height": 10})).unwrap();
    let st = app.session.world_state().unwrap();
    let box_body = st.bodies.last().unwrap();
    for i in 0..box_body.body.face_count() {
        assert_eq!(box_body.mesh().surface_patch(i), vec![i]);
    }
}

#[test]
fn immediate_wheel_zoom_applies_normalized_input_once() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.smooth_wheel_zoom = false;
    h.app.preferences.trackpad_pan = false;
    h.app.preferences.zoom_to_cursor = false;
    h.call("ui.move", json!({"x":800,"y":500}));
    let before = h.app.cam;
    let line = h.ctx.options(|o| o.input_options.line_scroll_speed);
    for (unit, amount, points) in
        [(egui::MouseWheelUnit::Point, 120.0, 120.0), (egui::MouseWheelUnit::Line, 1.0, line), (egui::MouseWheelUnit::Page, 0.1, 100.0)]
    {
        let height = h.app.cam.half_height();
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit,
            delta: vec2(0.0, amount),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.frame();
        let expected = height * (-f64::from(points) * 0.0015).exp();
        assert!((h.app.cam.half_height() - expected).abs() < 1e-6);
        let after = h.app.cam;
        h.frames(30);
        assert_eq!(h.app.cam, after, "idle frames must not replay wheel input");
    }
    assert_eq!(h.app.cam.target, before.target);
    h.app.preferences.zoom_reverse = true;
    let height = h.app.cam.half_height();
    h.call("ui.scroll", json!({"x":800,"y":500,"delta":120}));
    assert!((h.app.cam.half_height() - height * 0.18_f64.exp()).abs() < 1e-6);
}

#[test]
fn wheel_zoom_setting_changes_do_not_replay_smoothing() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.zoom_to_cursor = false;
    h.app.preferences.trackpad_pan = false;
    h.call("ui.move", json!({"x":800,"y":500}));
    h.app.synthetic.push(egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 120.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.frame();
    assert!(h.ctx.input(|i| i.is_scrolling()));
    h.app.preferences.smooth_wheel_zoom = false;
    let stopped = h.app.cam;
    h.frame();
    assert_eq!(h.app.cam, stopped);
    h.app.preferences.smooth_wheel_zoom = true;
    h.frames(90);
    assert_eq!(h.app.cam, stopped, "re-enabling must not replay the old smoothing tail");
    assert!(!h.app.viewport.wheel_zoom_raw_until_idle);
    h.app.preferences.smooth_wheel_zoom = false;
    for _ in 0..3 {
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.frame();
    }
    assert!((h.app.cam.half_height() - stopped.half_height() * (-0.54_f64).exp()).abs() < 1e-6);
    let after = h.app.cam;
    h.frames(90);
    assert_eq!(h.app.cam, after);
}

#[test]
fn trackpad_zoom_modifier_zooms_without_pan_or_tail() {
    use crate::prefs::TrackpadZoomModifier as Z;
    use egui::{Modifiers as M, MouseWheelUnit as U, TouchPhase as P};
    for preset in ["fusion", crate::prefs::TOUCHPAD_NAV] {
        for (chosen, held, zooms) in [
            (Z::Cmd, M::COMMAND, true),
            (Z::Off, M::COMMAND, false),
            (Z::Alt, M::ALT, true),
            (Z::Alt, M::COMMAND, false),
            (Z::Ctrl, M::CTRL, true),
        ] {
            for bracket in [true, false] {
                for reverse in [false, true] {
                    // Run the same input with the given choice; a key that is not chosen
                    // must leave main's own handling (Cmd/Ctrl + wheel is a pinch, #8480) unchanged.
                    let run = |choice| {
                        let mut h = crate::scenario::Harness::new();
                        h.app.preferences.nav = preset.into();
                        h.app.preferences.trackpad_zoom_modifier = choice;
                        h.app.preferences.zoom_reverse = reverse;
                        h.app.preferences.zoom_to_cursor = false;
                        h.call("ui.move", json!({"x":800,"y":500}));
                        let before = h.app.cam;
                        let event = |phase| egui::Event::MouseWheel {
                            source: egui::MouseWheelSource::Unknown,
                            unit: U::Point,
                            delta: vec2(60.0, 120.0),
                            modifiers: held,
                            phase,
                        };
                        if bracket {
                            h.app.synthetic.push(event(P::Start));
                            h.frame();
                        }
                        h.app.synthetic.push(event(P::Move));
                        h.frame();
                        if bracket {
                            h.app.synthetic.push(event(P::End));
                            h.frame();
                        }
                        (h, before)
                    };
                    let (mut h, before) = run(chosen);
                    if zooms {
                        let expected = before.half_height() * (-0.18 * crate::prefs::zoom_sign(&h.app)).exp();
                        assert!((h.app.cam.half_height() - expected).abs() < 1e-6, "{preset}, {chosen:?}, {held:?}");
                        assert_eq!(h.app.cam.target, before.target);
                        assert_eq!(h.app.cam.back(), before.back());
                    } else {
                        assert_eq!(h.app.cam, run(Z::Off).0.app.cam, "{preset}, {chosen:?}, {held:?}: unchosen keys keep the default handling");
                    }
                    let after = h.app.cam;
                    h.frames(30);
                    assert_eq!(h.app.cam, after, "modifier scroll must not drift");
                }
            }
        }
    }
}

#[test]
fn trackpad_zoom_modifier_ignores_horizontal_only_scroll() {
    for chosen in [crate::prefs::TrackpadZoomModifier::Cmd, crate::prefs::TrackpadZoomModifier::Alt] {
        let mut h = crate::scenario::Harness::new();
        h.app.preferences.trackpad_zoom_modifier = chosen;
        h.call("ui.move", json!({"x":800,"y":500}));
        let before = h.app.cam;
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
            h.app.synthetic.push(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(120.0, 0.0),
                modifiers: chosen.modifiers().unwrap(),
                phase,
            });
            h.frame();
        }
        h.frames(30);
        assert_eq!(h.app.cam, before, "{chosen:?}: horizontal scroll must be ignored");
    }
}

#[test]
fn trackpad_zoom_modifier_matches_wheel_speed_and_pointer_anchor() {
    for preset in ["fusion", crate::prefs::TOUCHPAD_NAV] {
        for unit in [egui::MouseWheelUnit::Point, egui::MouseWheelUnit::Line] {
            let mut h = crate::scenario::Harness::new();
            h.app.preferences.nav = preset.into();
            h.app.preferences.smooth_wheel_zoom = false;
            h.app.preferences.trackpad_zoom_modifier = crate::prefs::TrackpadZoomModifier::Cmd;
            h.call("engine.execute", json!({"command":"solid.box","params":{"length":40,"width":30,"height":20}}));
            h.call("ui.camera", json!({"fit":true}));
            h.call("ui.move", json!({"x":800,"y":500}));
            let before = h.app.cam;
            let event = |modifiers| egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit,
                delta: vec2(0.0, if unit == egui::MouseWheelUnit::Point { 120.0 } else { 3.0 }),
                modifiers,
                phase: egui::TouchPhase::Move,
            };
            h.app.synthetic.push(event(egui::Modifiers::NONE));
            h.frame();
            let wheel = h.app.cam;
            h.frames(30);
            h.app.cam = before;
            h.step(&json!({"shot":"trackpad-zoom-modifier-before"})).unwrap();
            h.app.synthetic.push(event(egui::Modifiers::COMMAND));
            h.frame();
            assert_eq!(h.app.cam, wheel, "{preset}, {unit:?}: same speed and cursor anchor as wheel");
            assert_ne!(h.app.cam.target, before.target, "off-centre zoom must preserve its pointer anchor");
            h.step(&json!({"shot":"trackpad-zoom-modifier-after"})).unwrap();
        }
    }
}

#[test]
fn trackpad_zoom_modifier_rejects_nonfinite_scroll() {
    for preset in ["fusion", crate::prefs::TOUCHPAD_NAV] {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut h = crate::scenario::Harness::new();
            h.app.preferences.nav = preset.into();
            h.call("ui.move", json!({"x":800,"y":500}));
            let before = h.app.cam;
            for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
                h.app.synthetic.push(egui::Event::MouseWheel {
                    source: egui::MouseWheelSource::Unknown,
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, invalid),
                    modifiers: egui::Modifiers::COMMAND,
                    phase,
                });
                h.frame();
            }
            h.frames(10);
            assert!(h.app.cam.is_valid());
            assert_eq!(h.app.cam, before);
        }
    }
}

#[test]
fn trackpad_two_finger_scroll_pans_without_zooming() {
    let mut h = crate::scenario::Harness::new();
    assert!(h.app.preferences.trackpad_pan, "two-finger pan is the default");
    h.app.preferences.zoom_to_cursor = false;
    h.call("ui.move", json!({"x":800,"y":500}));
    let gesture = |phase| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(60.0, -40.0),
        modifiers: egui::Modifiers::NONE,
        phase,
    };
    // A bracketed trackpad gesture pans (measured macOS Start…End bracketing).
    let before = h.app.cam;
    h.app.synthetic.push(gesture(egui::TouchPhase::Start));
    h.frame();
    h.app.synthetic.push(gesture(egui::TouchPhase::Move));
    h.frame();
    assert_eq!(h.app.cam.half_height(), before.half_height(), "trackpad scroll must not zoom");
    assert_ne!(h.app.cam.target, before.target, "trackpad scroll must pan");
    h.app.synthetic.push(gesture(egui::TouchPhase::End));
    h.frame();
    let after = h.app.cam;
    h.frames(30);
    assert_eq!(h.app.cam, after, "pan must not drift once input stops");
    // Bare moves are a mouse: vertical zooms…
    h.app.preferences.smooth_wheel_zoom = false;
    h.app.synthetic.push(egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 120.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.frame();
    assert!((h.app.cam.half_height() - after.half_height() * (-0.18_f64).exp()).abs() < 1e-6);
    // …and horizontal pans without zooming (thumb wheel).
    let zoomed = h.app.cam;
    h.app.synthetic.push(egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(40.0, 0.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.frame();
    assert_eq!(h.app.cam.half_height(), zoomed.half_height());
    assert_ne!(h.app.cam.target, zoomed.target);
    // Opting out restores the legacy wheel behaviour for all point input.
    h.app.preferences.trackpad_pan = false;
    h.app.synthetic.push(egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 120.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.frame();
    assert!((h.app.cam.half_height() - zoomed.half_height() * (-0.18_f64).exp()).abs() < 1e-6);
    // Legacy mode also zooms bracketed input instead of panning it.
    let legacy = h.app.cam.half_height();
    let legacy_target = h.app.cam.target;
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            modifiers: egui::Modifiers::NONE,
            phase,
        });
        h.frame();
    }
    assert!((h.app.cam.half_height() - legacy * (-0.18_f64).exp()).abs() < 1e-6);
    assert_eq!(h.app.cam.target, legacy_target);
    // The thumb wheel pans sideways even with panning off: it is a mouse.
    let h0 = h.app.cam.half_height();
    h.app.synthetic.push(egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(40.0, 0.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.frame();
    assert_eq!(h.app.cam.half_height(), h0);
    assert_ne!(h.app.cam.target, legacy_target);
}

#[test]
fn fusion_shift_two_finger_scroll_orbits_without_pan_zoom_or_drift() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.nav = "fusion".into();
    h.app.preferences.orbit_around = crate::prefs::OrbitAround::ViewCentre;
    h.call("engine.execute", json!({"command":"solid.box","params":{"length":40,"width":30,"height":20}}));
    h.call("ui.camera", json!({"fit":true}));
    h.call("ui.move", json!({"x":800,"y":500}));
    h.step(&json!({"shot":"fusion-trackpad-before"})).unwrap();
    let before = h.app.cam;
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(60.0, -40.0),
            modifiers: egui::Modifiers::SHIFT,
            phase,
        });
        h.frame();
    }
    assert_ne!(h.app.cam.back(), before.back(), "Shift + two fingers must orbit");
    assert_eq!(h.app.cam.target, before.target, "orbit must not pan");
    assert_eq!(h.app.cam.half_height(), before.half_height(), "orbit must not zoom");
    let after = h.app.cam;
    h.frames(90);
    assert_eq!(h.app.cam, after, "orbit must not drift once input stops");
    h.step(&json!({"shot":"fusion-trackpad-orbit"})).unwrap();
    for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(60.0, -40.0),
            modifiers: egui::Modifiers::NONE,
            phase,
        });
        h.frame();
    }
    assert_eq!(h.app.cam.back(), after.back(), "unmodified two fingers must not orbit");
    assert_ne!(h.app.cam.target, after.target, "unmodified two fingers must pan");
    h.step(&json!({"shot":"fusion-trackpad-pan"})).unwrap();
}

/// #8410: with the trackpad orbit reversed, Shift with two fingers orbits exactly the other way
/// on both axes, and a plain two-finger pan is the same as without it.
#[test]
fn reversed_trackpad_orbit_turns_the_other_way_and_leaves_pan_alone() {
    let step = |orbit_reverse: bool, modifiers: egui::Modifiers| {
        let mut h = crate::scenario::Harness::new();
        h.app.preferences.nav = "fusion".into();
        h.app.preferences.orbit_around = crate::prefs::OrbitAround::ViewCentre;
        h.app.preferences.orbit_reverse = orbit_reverse;
        h.call("ui.move", json!({"x":800,"y":500}));
        let gesture = |phase| egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(20.0, -10.0),
            modifiers,
            phase,
        };
        h.app.synthetic.push(gesture(egui::TouchPhase::Start));
        h.frame();
        let before = h.app.cam;
        h.app.synthetic.push(gesture(egui::TouchPhase::Move));
        h.frame();
        (before, h.app.cam)
    };
    let (before, after) = step(true, egui::Modifiers::SHIFT);
    let mut expected = before;
    expected.orbit(-20.0, 10.0);
    assert_eq!(after, expected, "a reversed orbit turns the other way");
    assert_eq!(step(true, egui::Modifiers::NONE), step(false, egui::Modifiers::NONE), "a pan does not change");
}

#[test]
fn fusion_trackpad_uses_each_events_modifiers_and_can_switch_mid_gesture() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.nav = "fusion".into();
    h.app.preferences.orbit_around = crate::prefs::OrbitAround::ViewCentre;
    h.call("ui.move", json!({"x":800,"y":500}));
    let gesture = |phase, modifiers| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(20.0, -10.0),
        modifiers,
        phase,
    };
    h.app.synthetic.push(gesture(egui::TouchPhase::Start, egui::Modifiers::NONE));
    h.frame();
    for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT, egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
        let before = h.app.cam;
        h.app.synthetic.push(gesture(egui::TouchPhase::Move, modifiers));
        h.frame();
        assert_eq!(h.app.cam.half_height(), before.half_height());
        if modifiers.shift {
            let mut expected = before;
            expected.orbit(20.0, -10.0);
            assert_eq!(h.app.cam, expected, "Shift must orbit continuously, without snapping");
        } else {
            assert_eq!(h.app.cam.back(), before.back());
            assert_ne!(h.app.cam.target, before.target);
        }
    }
    let before = h.app.cam;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))),
        time: Some(h.ctx.input(|input| input.time) + 1.0 / 60.0),
        focused: true,
        events: vec![gesture(egui::TouchPhase::Move, egui::Modifiers::NONE), gesture(egui::TouchPhase::Move, egui::Modifiers::SHIFT)],
        ..Default::default()
    };
    let mut output = h.ctx.run_ui(raw, |ui| {
        h.app.logic(ui.ctx());
        h.app.ui(ui);
    });
    output.textures_delta.clear();
    assert_ne!(h.app.cam.target, before.target, "unmodified events in the same frame still pan");
    assert_ne!(h.app.cam.back(), before.back(), "Shift events in the same frame still orbit");
    h.app.synthetic.push(gesture(egui::TouchPhase::Cancel, egui::Modifiers::SHIFT));
    h.frame();
    assert!(!h.app.viewport.trackpad_gesture);
    assert!(h.app.viewport.orbit_pivot.is_none());
    assert!(h.app.viewport.orbit_from.is_none());
    let after = h.app.cam;
    h.frames(90);
    assert_eq!(h.app.cam, after);
}

/// #7327: macOS sends the inertia after the fingers lift as its own scroll bracket, right after
/// the fingers' one, with the modifiers held at the time. Releasing Shift while an orbit coasts
/// (or pressing it while a pan coasts) ends the coast instead of carrying it into the other mode.
#[test]
fn fusion_trackpad_inertia_stops_when_shift_changes() {
    use egui::{Modifiers, TouchPhase::*};
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.nav = "fusion".into();
    h.app.preferences.orbit_around = crate::prefs::OrbitAround::ViewCentre;
    h.call("ui.move", json!({"x":800,"y":500}));
    let gesture = |phase, modifiers| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(20.0, -10.0),
        modifiers,
        phase,
    };
    let play = |h: &mut crate::scenario::Harness, phase, modifiers| {
        h.app.synthetic.push(gesture(phase, modifiers));
        h.frame();
    };
    for (held, other) in [(Modifiers::SHIFT, Modifiers::NONE), (Modifiers::NONE, Modifiers::SHIFT)] {
        // The fingers swipe and lift; the coast starts in the next frame and keeps their mode.
        for phase in [Start, Move, Move, End, Start, Move] {
            play(&mut h, phase, held);
        }
        let coasting = h.app.cam;
        play(&mut h, Move, held);
        assert_ne!(h.app.cam, coasting, "{held:?}: the coast keeps moving while the modifiers stay");
        // The modifiers change: the rest of the coast is dropped, even when they change back.
        let stopped = h.app.cam;
        for modifiers in [other, other, held, other] {
            play(&mut h, Move, modifiers);
            assert_eq!(h.app.cam, stopped, "{held:?}: the coast must end, not pan or orbit");
        }
        play(&mut h, End, other);
        h.frames(30);
        assert_eq!(h.app.cam, stopped);
    }
    // Fingers touching during a coast are a new swipe: it moves at once and still switches
    // between pan and orbit with Shift while the fingers stay down.
    for phase in [Start, Move, Move, End, Start, Move] {
        play(&mut h, phase, Modifiers::SHIFT);
    }
    play(&mut h, End, Modifiers::SHIFT);
    play(&mut h, Start, Modifiers::SHIFT);
    for modifiers in [Modifiers::SHIFT, Modifiers::NONE, Modifiers::SHIFT] {
        let before = h.app.cam;
        play(&mut h, Move, modifiers);
        if modifiers.shift {
            let mut expected = before;
            expected.orbit(20.0, -10.0);
            assert_eq!(h.app.cam, expected, "a new swipe orbits with Shift");
        } else {
            assert_eq!(h.app.cam.back(), before.back());
            assert_ne!(h.app.cam.target, before.target, "a new swipe pans without Shift");
        }
    }
    play(&mut h, End, Modifiers::NONE);
    // A swipe after a pause is a new swipe too, whatever the last one did.
    h.frames(30);
    play(&mut h, Start, Modifiers::NONE);
    let before = h.app.cam;
    play(&mut h, Move, Modifiers::NONE);
    assert_ne!(h.app.cam.target, before.target, "a later swipe pans");
    play(&mut h, End, Modifiers::NONE);
}

#[test]
fn shift_trackpad_orbit_preserves_other_presets_and_legacy_scroll() {
    for preset in crate::prefs::NAV_PRESETS.iter().map(|preset| preset.id) {
        for trackpad_pan in [true, false] {
            let mut h = crate::scenario::Harness::new();
            h.app.preferences.nav = preset.into();
            h.app.preferences.trackpad_pan = trackpad_pan;
            h.app.preferences.zoom_to_cursor = false;
            h.call("ui.move", json!({"x":800,"y":500}));
            let before = h.app.cam;
            for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
                h.app.synthetic.push(egui::Event::MouseWheel {
                    source: egui::MouseWheelSource::Unknown,
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(20.0, -10.0),
                    modifiers: egui::Modifiers::SHIFT,
                    phase,
                });
                h.frame();
            }
            if preset == "fusion" && trackpad_pan {
                assert_ne!(h.app.cam.back(), before.back());
            } else {
                assert_eq!(h.app.cam.back(), before.back(), "{preset}, pan={trackpad_pan}: must not orbit");
                if h.app.preferences.trackpad_pans() {
                    assert_ne!(h.app.cam.target, before.target, "{preset}: Shift still pans");
                    assert_eq!(h.app.cam.half_height(), before.half_height());
                }
            }
        }
    }
}

#[test]
fn fusion_shift_trackpad_respects_orbit_centre_and_rejects_invalid_motion() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.nav = "fusion".into();
    h.app.preferences.orbit_around = crate::prefs::OrbitAround::Origin;
    h.app.cam.target = Vec3::new(30.0, 20.0, 10.0);
    h.call("ui.move", json!({"x":800,"y":500}));
    let before = h.app.cam;
    let gesture = |phase, delta| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta,
        modifiers: egui::Modifiers::SHIFT,
        phase,
    };
    h.app.synthetic.push(gesture(egui::TouchPhase::Start, egui::Vec2::ZERO));
    h.frame();
    h.app.synthetic.push(gesture(egui::TouchPhase::Move, vec2(20.0, -10.0)));
    h.frame();
    let mut expected = before;
    expected.orbit_about(20.0, -10.0, Vec3::ZERO);
    assert_eq!(h.app.cam, expected);
    for delta in [vec2(f32::NAN, 1.0), vec2(1.0, f32::INFINITY)] {
        h.app.synthetic.push(gesture(egui::TouchPhase::Move, delta));
        h.frame();
        assert_eq!(h.app.cam, expected);
    }
    h.app.synthetic.push(gesture(egui::TouchPhase::End, egui::Vec2::ZERO));
    h.frame();
    assert!(h.app.viewport.orbit_from.is_none());
}

#[test]
fn pinch_gesture_zooms_and_ignores_noise() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.smooth_wheel_zoom = false;
    h.app.preferences.zoom_to_cursor = false;
    h.call("ui.move", json!({"x":800,"y":500}));
    let height = h.app.cam.half_height();
    let target = h.app.cam.target;
    h.app.synthetic.push(egui::Event::Zoom(1.2));
    h.frame();
    let zoomed = h.app.cam.half_height();
    assert!(((zoomed - height / 1.2) / (height / 1.2)).abs() < 1e-6, "spread must zoom in");
    assert_eq!(h.app.cam.target, target, "centre zoom keeps the target");
    h.app.preferences.zoom_reverse = true;
    let height = h.app.cam.half_height();
    h.app.synthetic.push(egui::Event::Zoom(1.2));
    h.frame();
    let flipped = h.app.cam.half_height();
    assert!(((flipped - height * 1.2) / (height * 1.2)).abs() < 1e-6, "reverse flips the pinch direction");
    h.app.synthetic.push(egui::Event::Zoom(f32::NAN));
    h.app.synthetic.push(egui::Event::Zoom(0.0));
    h.app.synthetic.push(egui::Event::Zoom(1.0));
    let stopped = h.app.cam;
    h.frame();
    assert_eq!(h.app.cam, stopped, "noise must leave the camera still");
}

/// A two-finger pan, a pinch and the wheel over a value box anchored to the model (Extrude's
/// distance) move the view as they do beside it (#10061): the box scrolls nothing, and a
/// gesture crossing it stopped. The box keeps its value. Dialogs keep theirs (below).
#[test]
fn trackpad_gestures_and_the_wheel_over_a_value_box_move_the_view() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [40, 30]})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    let sketch = h.app.session.doc.features[0].id;
    h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
    h.step(&json!({"start": "solid.extrude"})).unwrap();
    h.frames(6);
    let over =
        |h: &crate::scenario::Harness| h.ctx.read_response(egui::Id::unique("sc_canvas_value")).expect("Extrude's distance box").rect.center();
    let distance = |h: &crate::scenario::Harness| format!("{:?}", h.app.dialog.as_ref().map(|d| &d.kind));
    let typed = distance(&h);
    let at = over(&h);
    h.call("ui.move", json!({"x": at.x, "y": at.y}));
    h.frames(2);
    assert!(h.ctx.layer_id_at(at).is_some_and(crate::ui_kit::container::is_canvas), "the pointer is on the box");
    let gesture = |phase| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(30.0, -20.0),
        modifiers: egui::Modifiers::NONE,
        phase,
    };
    let before = h.app.cam;
    h.app.synthetic.push(gesture(egui::TouchPhase::Start));
    h.frame();
    h.app.synthetic.push(gesture(egui::TouchPhase::Move));
    h.frame();
    assert_ne!(h.app.cam.target, before.target, "a two-finger pan over the box pans");
    assert_eq!(h.app.cam.half_height(), before.half_height());
    h.app.synthetic.push(gesture(egui::TouchPhase::End));
    h.frames(30);
    let at = over(&h);
    h.call("ui.move", json!({"x": at.x, "y": at.y}));
    h.frames(2);
    let panned = h.app.cam;
    h.app.synthetic.push(egui::Event::Zoom(1.2));
    h.frame();
    assert!(h.app.cam.half_height() < panned.half_height(), "a pinch over the box zooms");
    let at = over(&h);
    h.call("ui.move", json!({"x": at.x, "y": at.y}));
    h.frames(2);
    let pinched = h.app.cam;
    h.app.preferences.smooth_wheel_zoom = false;
    h.call("ui.scroll", json!({"x": at.x, "y": at.y, "delta": 120.0}));
    h.frames(2);
    assert_ne!(h.app.cam.half_height(), pinched.half_height(), "the wheel over the box zooms");
    assert_eq!(distance(&h), typed, "the box kept its value");
    assert!(h.app.dialog.is_some());
}

/// #10128: a Shift + trackpad orbit must not halt when the spinning model moves a floating
/// value box (Extrude's distance) under the cursor. The box follows the geometry, so mid-orbit
/// it slides under a pointer that never moved; the orbit ignores it and keeps turning, frame
/// after frame, while the pointer is over the box.
#[test]
fn orbit_ignores_floating_field_under_cursor() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [40, 30]})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    let sketch = h.app.session.doc.features[0].id;
    h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
    h.step(&json!({"start": "solid.extrude"})).unwrap();
    h.frames(6);
    let over =
        |h: &crate::scenario::Harness| h.ctx.read_response(egui::Id::unique("sc_canvas_value")).expect("Extrude's distance box").rect.center();
    let typed = format!("{:?}", h.app.dialog.as_ref().map(|d| &d.kind));
    // The orbit is in progress and the box slides under the cursor: the pointer sits on the
    // box every frame, as it does when the spinning model carries the box to the pointer.
    let at = over(&h);
    h.call("ui.move", json!({"x": at.x, "y": at.y}));
    h.frames(2);
    assert!(h.ctx.layer_id_at(at).is_some_and(crate::ui_kit::container::is_canvas), "the pointer is on the box");
    let orbit = |phase| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(24.0, -16.0),
        modifiers: egui::Modifiers::SHIFT,
        phase,
    };
    h.app.synthetic.push(orbit(egui::TouchPhase::Start));
    h.frame();
    let mut last = h.app.cam;
    for _ in 0..4 {
        h.app.synthetic.push(orbit(egui::TouchPhase::Move));
        h.frame();
        let at = over(&h);
        h.call("ui.move", json!({"x": at.x, "y": at.y}));
        assert_ne!((h.app.cam.yaw, h.app.cam.pitch), (last.yaw, last.pitch), "the orbit keeps turning while the box is under the cursor");
        last = h.app.cam;
    }
    h.app.synthetic.push(orbit(egui::TouchPhase::End));
    h.frames(3);
    assert_eq!(format!("{:?}", h.app.dialog.as_ref().map(|d| &d.kind)), typed, "the box kept its value");
    assert!(h.app.dialog.is_some());
}

#[test]
fn trackpad_gestures_over_dialogs_do_not_move_the_view() {
    let ctx = crate::fonts::test_context();
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert!(app.preferences.trackpad_pan);
    crate::help::run(&mut app, &ctx, "shortcuts");
    let mut time = 0.0;
    let pan = |phase, modifiers| egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(40.0, -20.0),
        modifiers,
        phase,
    };
    let mut frame = |app: &mut SolveApp, pointer: Option<Pos2>, events: Vec<egui::Event>| {
        time += 1.0 / 60.0;
        let mut input = events;
        if let Some(p) = pointer {
            input.insert(0, egui::Event::PointerMoved(p));
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0))),
                time: Some(time),
                events: input,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| show(app, ui));
                crate::prefs::show(app, ui.ctx());
            },
        );
        output.textures_delta.clear();
    };
    for _ in 0..4 {
        frame(&mut app, None, Vec::new());
    }
    let window = ctx.memory(|m| m.area_rect(egui::Id::unique("sc_prefs"))).unwrap();
    let over = window.center();
    let height = app.cam.half_height();
    let target = app.cam.target;
    let direction = app.cam.back();
    // A full gesture plus pinch over the dialog stays in the popup.
    for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
        for events in [
            vec![pan(egui::TouchPhase::Start, modifiers)],
            vec![pan(egui::TouchPhase::Move, modifiers), egui::Event::Zoom(1.2)],
            vec![pan(egui::TouchPhase::End, modifiers)],
        ] {
            frame(&mut app, Some(over), events);
            assert_eq!(app.cam.half_height(), height, "trackpad input over a dialog must stay in the popup");
            assert_eq!(app.cam.target, target);
            assert_eq!(app.cam.back(), direction, "Shift + trackpad over a dialog must not orbit");
        }
    }
    let exposed = pos2(40.0, 700.0);
    assert!(!window.contains(exposed));
    frame(&mut app, Some(exposed), Vec::new());
    frame(&mut app, Some(exposed), vec![pan(egui::TouchPhase::Start, egui::Modifiers::NONE)]);
    frame(&mut app, Some(exposed), vec![pan(egui::TouchPhase::Move, egui::Modifiers::NONE)]);
    assert_eq!(app.cam.half_height(), height);
    assert_ne!(app.cam.target, target, "the exposed viewport still pans");
    frame(&mut app, Some(exposed), vec![egui::Event::Zoom(1.2)]);
    assert_ne!(app.cam.half_height(), height, "the exposed viewport still pinch-zooms");
    frame(&mut app, Some(exposed), vec![pan(egui::TouchPhase::End, egui::Modifiers::NONE)]);
}

#[test]
fn shortcuts_window_wheel_does_not_zoom_the_viewport() {
    for smooth in [true, false] {
        let ctx = crate::fonts::test_context();
        let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        app.preferences.smooth_wheel_zoom = smooth;
        app.preferences.trackpad_pan = false;
        crate::help::run(&mut app, &ctx, "shortcuts");
        let mut time = 0.0;
        let mut frame = |app: &mut SolveApp, pointer: Option<Pos2>, wheel: f32| {
            time += 1.0 / 60.0;
            let mut events = Vec::new();
            if let Some(p) = pointer {
                events.push(egui::Event::PointerMoved(p));
            }
            if wheel != 0.0 {
                events.push(egui::Event::MouseWheel {
                    source: egui::MouseWheelSource::Unknown,
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, wheel),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                });
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| show(app, ui));
                    crate::prefs::show(app, ui.ctx());
                },
            );
            output.textures_delta.clear();
        };
        // Let egui establish the floating window's size and hit-test widgets.
        for _ in 0..4 {
            frame(&mut app, None, 0.0);
        }
        let window = ctx.memory(|m| m.area_rect(egui::Id::unique("sc_prefs"))).unwrap();
        let list = window.center() + vec2(140.0, 0.0);
        let height = app.cam.half_height();
        let target = app.cam.target;
        frame(&mut app, Some(list), 0.0);
        for n in 0..12 {
            frame(&mut app, Some(list), if n == 0 { -120.0 } else { 0.0 });
            assert_eq!(app.cam.half_height(), height, "wheel input over shortcuts must stay in the popup");
            assert_eq!(app.cam.target, target);
        }
        // The window's non-scrolling sidebar must also shield the model.
        frame(&mut app, Some(window.left_center() + vec2(30.0, 0.0)), -120.0);
        assert_eq!(app.cam.half_height(), height);
        // An exposed part of the viewport still accepts wheel navigation with the popup open.
        let exposed = pos2(40.0, 700.0);
        assert!(!window.contains(exposed));
        frame(&mut app, Some(exposed), 0.0);
        frame(&mut app, Some(exposed), -120.0);
        assert_ne!(app.cam.half_height(), height);
        // Closing the popup returns its former area to the viewport.
        app.prefs_window.open = false;
        for _ in 0..12 {
            frame(&mut app, Some(list), 0.0);
        }
        let height = app.cam.half_height();
        frame(&mut app, Some(list), -120.0);
        assert_ne!(app.cam.half_height(), height);
    }
}

#[test]
fn wheel_by_a_dialog_list_or_panel_edge_leaves_the_model_alone() {
    // #678: scrolling the Physical Material list, a drop-down that hangs over the model,
    // zoomed the model too. egui's hit test counts the model as under the pointer within a
    // few points of a window's, a list's or the viewport's own edge, so near those edges
    // the same wheel turn reached both.
    use crate::scenario::{Harness, handle_at};
    // One wheel turn at `p`, then frames until egui's smoothing is spent: did the camera move?
    let wheel = |h: &mut Harness, p: Pos2, delta: f32| {
        let before = h.app.cam;
        h.call("ui.scroll", json!({"x": p.x, "y": p.y, "delta": delta}));
        h.frames(20);
        h.app.cam != before
    };
    // A trackpad pinch, then a two-finger scroll (which pans the model): did the camera move?
    let gestures = |h: &mut Harness, p: Pos2| {
        let before = h.app.cam;
        h.call("ui.move", json!({"x": p.x, "y": p.y}));
        h.app.synthetic.push(egui::Event::Zoom(1.2));
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::Move, egui::TouchPhase::End] {
            let unit = egui::MouseWheelUnit::Point;
            h.app.synthetic.push(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit,
                delta: vec2(30.0, -20.0),
                modifiers: egui::Modifiers::NONE,
                phase,
            });
        }
        h.settle();
        h.frames(20);
        h.app.cam != before
    };
    for (layout, smooth) in [
        (crate::shell::LayoutMode::Floating, true),
        (crate::shell::LayoutMode::Floating, false),
        (crate::shell::LayoutMode::Docked, true),
        (crate::shell::LayoutMode::Docked, false),
    ] {
        let mut h = Harness::new();
        h.call("ui.set", json!({"layout":layout}));
        h.app.preferences.smooth_wheel_zoom = smooth;
        h.app.preferences.zoom_to_cursor = false;
        h.call("engine.execute", json!({"command": "solid.box", "params": {"length": 40, "width": 30, "height": 20}}));
        h.call("ui.start", json!({"command": "material.assign"}));
        let field = handle_at("dialog:Material").expect("the dialog's material field");
        let field = json!({"x": field.x, "y": field.y});
        h.call("ui.move", field.clone());
        h.call("ui.click", field.clone());
        let item = "dialog:Material:Steel";
        let first = handle_at(item).expect("the open material list");
        let list = h
            .ctx
            .memory(|m| {
                let popups = m.areas().visible_layer_ids().into_iter().filter(|l| l.order == egui::Order::Foreground);
                popups.filter_map(|l| m.area_rect(l.id)).find(|r| r.contains(first))
            })
            .expect("the list's popup");
        let dialog = h.app.viewport.dialog_rect.expect("the dialog");
        let vp = h.app.viewport.rect.expect("the viewport");
        // The dense header, hint and footer change which item first hangs beyond
        // the card. Keep the original reproduction over the exposed model.
        let (item, first) = crate::scenario::handles("dialog:Material:")
            .into_iter()
            .find(|(_, point)| !dialog.contains(*point) && list.shrink(2.0).contains(*point) && vp.contains(*point))
            .expect("a visible list item over the model");
        assert!(!dialog.contains(first) && vp.contains(first), "the list must hang over the model: {first:?} {dialog:?}");

        // The wheel scrolls the list and only the list, in its middle and by its edges.
        assert!(!wheel(&mut h, pos2(list.center().x, first.y), -60.0), "smooth {smooth}: the wheel over the list zoomed the model");
        let moved = handle_at(&item).expect("the list stays open");
        assert!(moved.y < first.y - 1.0, "the wheel must scroll the list: {first:?} -> {moved:?}");
        for p in [pos2(list.left() + 2.0, list.center().y), pos2(list.right() - 2.0, list.center().y), pos2(list.center().x, list.bottom() - 2.0)]
        {
            assert!(!wheel(&mut h, p, 60.0), "smooth {smooth}: the wheel at {p:?} by the list's edge ({list:?}) zoomed the model");
            assert!(!gestures(&mut h, p), "a pinch or two-finger scroll at {p:?} by the list's edge ({list:?}) moved the model");
            assert!(h.app.viewport.mouse.is_none(), "the model must not see the pointer at {p:?} over the list");
        }

        // With the list closed: by the dialog's edges, and over the browser and the timeline
        // next to the model.
        h.call("ui.move", field.clone());
        h.call("ui.click", field);
        assert!(handle_at(&item).is_none(), "the list must close");
        // Floating cards sit inside the full viewport. Probe their actual edges; the
        // content anchor also reserves gaps and space below the short browser card.
        // Docked panels end at the viewport boundary instead.
        let (browser_edge, mid, history_edge) = if layout == crate::shell::LayoutMode::Floating {
            let browser = h.app.shell.browser.expect("the floating browser card");
            let history = h.app.shell.history.expect("the floating history card");
            (browser.right(), browser.center().y, history.top())
        } else {
            (vp.left(), vp.center().y, vp.bottom())
        };
        let x = vp.center().x;
        for p in [
            pos2(dialog.left() + 2.0, dialog.center().y),
            pos2(dialog.center().x, dialog.bottom() - 2.0),
            pos2(browser_edge - 2.0, mid),
            pos2(x, history_edge + 2.0),
        ] {
            assert!(!wheel(&mut h, p, 60.0), "smooth {smooth}: the wheel at {p:?} off the model ({vp:?}, dialog {dialog:?}) zoomed it");
            assert!(h.app.viewport.mouse.is_none(), "the model must not see the pointer at {p:?}");
        }
        // The model itself still zooms right next to them.
        for p in [pos2(dialog.left() - 2.0, dialog.center().y), pos2(browser_edge + 2.0, mid), pos2(x, history_edge - 2.0)] {
            assert!(wheel(&mut h, p, 60.0), "smooth {smooth}: the wheel at {p:?} on the model ({vp:?}) must zoom it");
            assert_eq!(h.app.viewport.mouse, Some(p));
            assert!(gestures(&mut h, p), "a pinch and a two-finger scroll at {p:?} on the model must move it");
        }
    }
}

#[test]
fn floating_dialog_blocks_viewport_hover_and_clicks() {
    let mut h = crate::scenario::Harness::new();
    h.call("engine.execute", json!({"command": "solid.box", "params": {"length": 40, "width": 30, "height": 20}}));
    h.call("ui.view", json!({"view": "fit"}));
    h.app.ui.pick_bodies = true;
    let p = h.app.viewport.rect.unwrap().center();
    h.call("ui.move", json!({"x": p.x, "y": p.y}));
    assert!(h.app.viewport.hover.is_some(), "the test point must hover over the model");
    h.call("ui.click", json!({"x": p.x, "y": p.y}));
    assert!(!h.app.session.selection.is_empty(), "the uncovered model must be selectable");
    h.app.session.selection.clear();

    h.call("ui.prefs", json!({"open": "Navigation"}));
    let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique("sc_prefs"))).unwrap();
    assert!(window.contains(p), "the dialog must cover the model at the test point");
    h.call("ui.move", json!({"x": p.x, "y": p.y}));
    assert!(h.app.viewport.hover.is_none(), "the dialog must block model highlights");
    assert!(h.app.viewport.mouse.is_none());
    h.call("ui.click", json!({"x": p.x, "y": p.y}));
    assert!(h.app.session.selection.is_empty(), "clicks in the dialog must not select the model");

    h.call("ui.prefs", json!({"button": "cancel"}));
    h.call("ui.move", json!({"x": p.x, "y": p.y}));
    assert!(h.app.viewport.hover.is_some(), "closing the dialog must restore model highlights");
    h.call("ui.click", json!({"x": p.x, "y": p.y}));
    assert!(!h.app.session.selection.is_empty());
}

#[test]
fn floating_dialog_blocks_axis_view_highlights_and_clicks() {
    for mode in [crate::shell::LayoutMode::Floating, crate::shell::LayoutMode::Docked] {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.set", json!({"layout":mode}));
        h.call("ui.resize", json!({"width": 800, "height": 700}));
        let p = crate::scenario::handle_at("axis_view:z").unwrap();
        h.call("ui.move", json!({"x": p.x, "y": p.y}));
        assert_eq!(crate::scenario::count_of("axis_view_highlights"), Some(1.0));
        h.call("ui.prefs", json!({"open": "Navigation"}));
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique("sc_prefs"))).unwrap();
        assert!(window.contains(p), "the floating dialog must cover the axis target");
        let camera = serde_json::to_value(h.app.cam).unwrap();
        h.call("ui.move", json!({"x": p.x, "y": p.y}));
        assert_eq!(crate::scenario::count_of("axis_view_highlights"), Some(0.0));
        h.call("ui.click", json!({"x": p.x, "y": p.y}));
        assert_eq!(serde_json::to_value(h.app.cam).unwrap(), camera, "dialog clicks must not turn the axis view");
        h.call("ui.drag", json!({"x0": p.x, "y0": p.y, "x1": p.x + 20.0, "y1": p.y, "steps": 8}));
        assert_eq!(serde_json::to_value(h.app.cam).unwrap(), camera, "dialog drags must not orbit the axis gizmo");
        assert!(h.app.viewport.axis_drag.is_none());
        h.call("ui.prefs", json!({"button": "cancel"}));
        h.call("ui.move", json!({"x": p.x, "y": p.y}));
        assert_eq!(crate::scenario::count_of("axis_view_highlights"), Some(1.0));
        h.call("ui.click", json!({"x": p.x, "y": p.y}));
        assert_ne!(serde_json::to_value(h.app.cam).unwrap(), camera);
    }
}

/// #10091: an axis tip turns the view in place by default; with Preferences › Navigation
/// "Fit the model after a navigator view change" on it also frames the whole model, as Home does.
#[test]
fn axis_tips_frame_the_model_when_the_navigation_preference_asks() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"navigator_style":"axes", "look_at_anim":false}));
    h.app.run("solid.box", json!({"corner":[100,0,0],"length":20,"width":20,"height":20})).unwrap();
    h.settle();
    let undo = h.app.session.undo.len();
    let away = |h: &mut crate::scenario::Harness| {
        h.call("ui.view", json!({"view":"iso"}));
        h.app.cam.pan(300.0, 200.0, 500.0);
        h.app.cam.distance *= 4.0;
        h.frames(2);
        h.app.cam
    };
    let from = away(&mut h);
    let p = crate::scenario::handle_at("axis_view:-y").unwrap();
    h.call("ui.click", json!({"x":p.x,"y":p.y}));
    h.frames(2);
    assert!(h.app.cam.back().dist(-Vec3::Y) < 1e-6);
    assert_eq!(h.app.cam.target, from.target, "off by default: the target stays");
    assert_eq!(h.app.cam.distance, from.distance, "off by default: the zoom stays");
    h.call("ui.prefs", json!({"navigator_fit":true}));
    let from = away(&mut h);
    let p = crate::scenario::handle_at("axis_view:-y").unwrap();
    h.call("ui.click", json!({"x":p.x,"y":p.y}));
    h.frames(2);
    assert!(h.app.cam.back().dist(-Vec3::Y) < 1e-6);
    assert!(h.app.cam.target.dist(Vec3::new(110.0, 10.0, 10.0)) < 1e-6, "looks at the middle of the model, {:?}", h.app.cam.target);
    assert!(h.app.cam.distance < from.distance / 2.0, "{} vs {}", h.app.cam.distance, from.distance);
    // Home, then an axis: the zoom and the centre stay.
    let home = crate::scenario::rect_of("axis_view:home").unwrap().center();
    h.call("ui.click", json!({"x":home.x,"y":home.y}));
    h.frames(2);
    let framed = h.app.cam;
    for axis in ["axis_view:-y", "axis_view:z"] {
        let p = crate::scenario::handle_at(axis).unwrap();
        h.call("ui.click", json!({"x":p.x,"y":p.y}));
        h.frames(2);
        assert!((h.app.cam.distance - framed.distance).abs() < 1e-6, "{axis}: {} vs Home's {}", h.app.cam.distance, framed.distance);
        assert!(h.app.cam.target.dist(framed.target) < 1e-6, "{axis}: {:?} vs Home's {:?}", h.app.cam.target, framed.target);
    }
    h.call("ui.prefs", json!({"open":"Navigation"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(!h.app.preferences.navigator_fit, "Navigation defaults restore the in-place turn");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.session.undo.len(), undo, "a view change edits nothing");
}

#[test]
fn view_axis_drag_owns_navigation_and_keeps_the_view_centre() {
    for mode in [crate::shell::LayoutMode::Floating, crate::shell::LayoutMode::Docked] {
        for nav in [None, Some(NavMode::Orbit), Some(NavMode::Pan)] {
            let mut h = crate::scenario::Harness::new();
            h.call("ui.set", json!({"layout":mode}));
            h.call("ui.view", json!({"view": "front"}));
            h.app.viewport.nav = nav;
            let before = h.app.cam;
            let p = crate::scenario::handle_at("axis_view:-y").unwrap();
            h.call("ui.drag", json!({"x0": p.x, "y0": p.y, "x1": p.x + 48.0, "y1": p.y, "steps": 8}));
            assert!((h.app.cam.yaw - 48.0 * 0.008).abs() < 1e-6, "{nav:?}: axis drag must orbit exactly once");
            assert_eq!(h.app.cam.target, before.target, "{nav:?}: axis drag must not pan");
            assert_eq!(h.app.cam.distance, before.distance);
            assert_eq!(h.app.cam.pitch, before.pitch);
            assert!(h.app.viewport.axis_drag.is_none(), "release clears the axis gesture");
            assert!(h.app.viewport.boxsel.is_none());
        }
    }
}

#[test]
fn cosmetic_threads_use_depth_tested_lines_and_keep_selection_and_wireframe() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.cylinder", &json!({"radius":3,"height":10})).unwrap();
    // A pick in the middle of a face free at both ends names the end the thread starts from.
    s.execute("solid.thread", &json!({"face":[3,0,5],"designation":"M6","length":5,"from":[3,0,0]})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    let count = |sc: GpuScene| {
        sc.merged()
            .lines
            .as_chunks::<{ crate::gpu::LINE_SIZE }>()
            .0
            .iter()
            .filter(|line| line.get(28..32) == Some(0.85f32.to_le_bytes().as_slice()))
            .count()
    };
    assert_eq!(count(build_scene(&app)), 0, "markings are off by default");
    let off_key = scene_key(&app);
    app.preferences.cosmetic_threads = true;
    assert_ne!(scene_key(&app), off_key, "toggling invalidates the scene without a camera move");
    assert!(count(build_scene(&app)) > 20);
    assert!(crate::control::view_scene(&app, Default::default()).unwrap().lines.iter().any(|l| l.width == 0.85));
    app.preferences.cosmetic_threads = false;
    assert_eq!(count(build_scene(&app)), 0);
    assert!(!crate::control::view_scene(&app, Default::default()).unwrap().lines.iter().any(|l| l.width == 0.85));
    app.preferences.cosmetic_threads = true;
    app.session.selection = vec![Sel::Face { body: "Body1".into(), index: 0, point: Vec3::new(3., 0., 5.) }];
    let selection = app.session.selection.clone();
    assert!(count(build_scene(&app)) > 20);
    assert!(!build_highlight(&app).merged().tris.is_empty());
    assert_eq!(app.session.selection, selection);
    app.ui.visual_style = 2;
    assert_eq!(count(build_scene(&app)), 0);
    app.ui.visual_style = 0;
    app.session.visibility.hidden_bodies.push("Body1".into());
    assert_eq!(count(build_scene(&app)), 0);
    app.session.visibility.hidden_bodies.clear();
    app.session.doc_mut().appearances.bodies.insert("Body1".into(), sketcraft_engine::doc::appearance::Look::custom([150, 150, 150], 0.5));
    assert_eq!(count(build_scene(&app)), 0, "transparent bodies omit cosmetic strokes");
    app.session.doc_mut().appearances.bodies.clear();
    app.session.execute("component.create", &json!({"name":"Other active component"})).unwrap();
    assert_eq!(count(build_scene(&app)), 0, "inactive bodies are faded and unmarked");
}

#[test]
fn cosmetic_threads_preference_controls_internal_and_external_markings_without_editing_the_design() {
    for internal in [false, true] {
        let mut s = sketcraft_engine::Session::default();
        let face = if internal {
            s.execute("solid.box", &json!({"length":20,"width":20,"height":10})).unwrap();
            s.execute("solid.hole", &json!({"position":[10,10,10],"direction":[0,0,-1],"diameter":5})).unwrap();
            json!([12.5, 10, 9])
        } else {
            s.execute("solid.cylinder", &json!({"radius":3,"height":10})).unwrap();
            json!([3, 0, 9])
        };
        s.execute("solid.thread", &json!({"face":face,"designation":"M6","length":4})).unwrap();
        let mut app = SolveApp::new(s, crate::Services::default());
        let design = app.session.doc.to_json();
        let camera = app.cam;
        let count = |app: &SolveApp| {
            build_scene(app)
                .merged()
                .lines
                .as_chunks::<{ crate::gpu::LINE_SIZE }>()
                .0
                .iter()
                .filter(|line| line.get(28..32) == Some(0.85f32.to_le_bytes().as_slice()))
                .count()
        };
        let key = scene_key(&app);
        assert_eq!(count(&app), 0);
        crate::prefs::set(&mut app, &json!({"cosmetic_threads":true})).unwrap();
        assert_ne!(scene_key(&app), key);
        assert!(count(&app) > 20, "internal={internal}");
        assert!(crate::control::view_scene(&app, Default::default()).unwrap().lines.iter().any(|l| l.width == 0.85));
        crate::prefs::set(&mut app, &json!({"cosmetic_threads":false})).unwrap();
        assert_eq!(scene_key(&app), key);
        assert_eq!(count(&app), 0);
        assert_eq!(app.session.doc.to_json(), design);
        assert_eq!(app.cam, camera);
    }
}

#[test]
fn cosmetic_thread_scene_is_bounded_for_many_occurrences_and_cached_across_camera_moves() {
    let mut s = sketcraft_engine::Session::default();
    let c = s.execute("component.create", &json!({"name":"Threaded pegs"})).unwrap();
    s.execute("solid.cylinder", &json!({"radius":3,"height":20})).unwrap();
    s.execute("solid.thread", &json!({"face":[3,0,10],"designation":"M6"})).unwrap();
    for i in 1..96 {
        s.execute("occurrence.copy", &json!({"component":c["component"],"translate":[i*8,0,0]})).unwrap();
    }
    s.execute("component.activate", &json!({"component":0})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    let started = std::time::Instant::now();
    app.preferences.cosmetic_threads = true;
    let scene = build_scene(&app).merged();
    let strokes = scene
        .lines
        .as_chunks::<{ crate::gpu::LINE_SIZE }>()
        .0
        .iter()
        .filter(|line| line.get(28..32) == Some(0.85f32.to_le_bytes().as_slice()))
        .count();
    eprintln!("96 occurrence viewport scene: {strokes} cosmetic strokes, {:?}", started.elapsed());
    assert!(strokes > 1000 && strokes <= sketcraft_engine::cosmetic_threads::MAX_SCENE_SEGMENTS);
    let marked_occurrences: std::collections::HashSet<i32> = scene
        .lines
        .as_chunks::<{ crate::gpu::LINE_SIZE }>()
        .0
        .iter()
        .filter(|line| line.get(28..32) == Some(0.85f32.to_le_bytes().as_slice()))
        .filter_map(|line| line.get(0..4).and_then(|bytes| bytes.try_into().ok()).map(f32::from_le_bytes))
        .map(|x| (x / 8.0).round() as i32)
        .collect();
    assert_eq!(marked_occurrences.len(), 96, "schematic density shares strokes across all 96 pegs");
    let key = scene_key(&app);
    app.cam.distance *= 1.5;
    app.cam.yaw += 0.2;
    assert_eq!(scene_key(&app), key, "ordinary camera changes reuse the scene; LOD changes have their own generation");
}

#[test]
fn section_outlines_follow_the_cut() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20})).unwrap();
    let st = s.model.state();
    let m = st.bodies[0].mesh();
    let (o, n) = (Vec3::new(0.0, 15.0, 0.0), Vec3::new(0.0, -1.0, 0.0));
    let segs = sketcraft_engine::view::section_segments(&m, o, n);
    assert!(segs.len() >= 4, "{segs:?}");
    // On the plane (a hair to the kept side) and around the 40 x 20 cut.
    let mut len = 0.0;
    for (a, b) in &segs {
        assert!((a.y - 15.0).abs() < 0.01 && (b.y - 15.0).abs() < 0.01);
        len += a.dist(*b);
    }
    assert!((len - 120.0).abs() < 0.5, "outline length {len}");
    // A plane that misses the body cuts nothing.
    assert!(sketcraft_engine::view::section_segments(&m, Vec3::new(0.0, 99.0, 0.0), n).is_empty());
}

/// Tilting a section keeps each body's section index: one serves every normal, so a tilt
/// drag does not rebuild it every frame (#10373).
#[test]
fn tilting_the_section_keeps_its_index() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20})).unwrap();
    let o = Vec3::new(0.0, 15.0, 0.0);
    s.section = Some((o, Vec3::new(0.0, -1.0, 0.0)));
    let mut app = SolveApp::new(s, crate::Services::default());
    let indexes =
        |app: &SolveApp| app.viewport.section_index.lock().unwrap().values().map(|(_, i)| std::sync::Arc::as_ptr(i) as usize).collect::<Vec<_>>();
    build_scene(&app);
    let first = indexes(&app);
    assert_eq!(first.len(), 1);
    for k in 1..4 {
        let a = (10.0 * k as f64).to_radians();
        app.session.section = Some((o, Vec3::new(0.0, -a.cos(), a.sin())));
        build_scene(&app);
        assert_eq!(indexes(&app), first, "tilted {k}");
    }
}

#[test]
fn selection_shows_where_the_model_hides_it() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    assert!(build_highlight(&app).merged().hidden.is_empty());
    // The back face: its outline is drawn (dimmed) where the box hides it.
    app.session.selection = vec![Sel::Face { body: "Body1".into(), index: 3, point: Vec3::new(20.0, 30.0, 10.0) }];
    assert!(!build_highlight(&app).merged().hidden.is_empty());
}

#[test]
fn axis_view_top_bottom_choose_nearest_square_heading() {
    for (from, to) in [
        (0.0, 0.0),
        (30.0, 0.0),
        (-80.0, -90.0),
        (100.0, 90.0),
        (170.0, 180.0),
        (400.0, 360.0),
        (-400.0, -360.0),
        (-45.0, 0.0),
        (45.0, 0.0),
        (135.0, 90.0),
        (-135.0, -90.0),
        (44.99, 0.0),
        (45.01, 90.0),
        (-44.99, 0.0),
        (-45.01, -90.0),
    ] {
        let got = nearest_quarter_turn(f64::to_radians(from)).to_degrees();
        assert!((got - to).abs() < 1e-9, "{from}° → {got}°, expected {to}°");
    }
    assert_eq!(nearest_quarter_turn(f64::NAN), 0.0);
    assert_eq!(nearest_quarter_turn(f64::INFINITY), 0.0);
}

#[test]
fn shift_orbit_accumulates_and_returns_to_free_rotation() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.view", json!({"view":"front"}));
    let rect = h.app.viewport.rect.unwrap();
    let before = h.app.cam;
    for _ in 0..100 {
        orbit_drag(&mut h.app, rect, 2.0, 0.0, None, true, false);
    }
    assert!((h.app.cam.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert_eq!(h.app.cam.target, before.target);
    assert_eq!(h.app.cam.distance, before.distance);
    orbit_drag(&mut h.app, rect, 0.0, 0.0, None, false, false);
    h.frames(2);
    assert!((h.app.cam.yaw - 1.6).abs() < 1e-6);
    assert!(h.app.viewport.orbit_free.is_none());
}

#[test]
fn axis_snap_selects_all_directions_in_both_up_frames() {
    assert_eq!(snap_directions().count(), 26);
    for up in [sketcraft_engine::render::UpAxis::Z, sketcraft_engine::render::UpAxis::Y] {
        for d in snap_directions() {
            let mut cam = Camera { up, ..Camera::default() };
            cam = cam.looking_from(cam.from_frame(d + Vec3::new(0.05, 0.03, 0.02)));
            let to = snapped_axis_camera(cam);
            assert!(to.back().dist(cam.from_frame(d)) < 1e-6);
            assert_eq!(to.target, cam.target);
            assert_eq!(to.distance, cam.distance);
            assert_eq!(to.fov, cam.fov);
        }
    }
}

#[test]
fn camera_resets_drop_the_unsnapped_orbit() {
    for reset in 0..3 {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length":10,"width":10,"height":10})).unwrap();
        let rect = h.app.viewport.rect.unwrap();
        orbit_drag(&mut h.app, rect, 120.0, 40.0, None, true, false);
        assert!(h.app.viewport.orbit_free.is_some());
        match reset {
            0 => h.app.fit_view(),
            1 => h.app.set_up_axis(sketcraft_engine::render::UpAxis::Y),
            _ => crate::documents::new_design(&mut h.app),
        }
        assert!(h.app.viewport.orbit_free.is_none());
    }
}

#[test]
fn axis_snap_is_applied_at_once() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.view", json!({"view":"front"}));
    let rect = h.app.viewport.rect.unwrap();
    orbit_drag(&mut h.app, rect, 120.0, 0.0, None, true, false);
    assert!((h.app.cam.yaw - std::f64::consts::FRAC_PI_4).abs() < 1e-6, "the snapped view shows in the same frame");
}

/// The origin pivot holds through a Shift drag that snaps to an axis: the snapped
/// view turns without letting the origin slide on screen.
#[test]
fn snapping_keeps_the_origin_still() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.orbit_around = crate::prefs::OrbitAround::Origin;
    h.app.run("solid.box", json!({"length":40,"width":40,"height":40})).unwrap();
    h.app.fit_view();
    let rect = h.app.viewport.rect.unwrap();
    let at = rect.center() + vec2(50.0, -20.0);
    orbit_drag(&mut h.app, rect, 0.0, 0.0, Some(at), true, true);
    let before = projection(&h.app, rect).to_screen(Vec3::ZERO).unwrap();
    for _ in 0..30 {
        orbit_drag(&mut h.app, rect, 4.0, 2.0, Some(at), true, true);
        let after = projection(&h.app, rect).to_screen(Vec3::ZERO).unwrap();
        assert!(after.distance(before) < 2.0, "{before:?} -> {after:?}");
    }
}

/// An orbit drag that starts on the model turns about the point under the cursor, which
/// stays on its pixel, in perspective as in parallel views. The perspective view used to
/// correct with a pan at the target's depth, so the point slid away and the view swung
/// about the camera instead.
#[test]
fn cursor_orbit_keeps_the_picked_point_on_its_pixel() {
    for perspective in [false, true] {
        for offset in [vec2(30.0, 20.0), vec2(-60.0, 10.0), vec2(120.0, -40.0)] {
            let mut h = crate::scenario::Harness::new();
            h.app.preferences.orbit_around = crate::prefs::OrbitAround::Cursor;
            h.app.ui.perspective = perspective;
            h.app.run("solid.box", json!({"length":80,"width":50,"height":20})).unwrap();
            h.app.fit_view();
            h.frames(2); // Fit resolves against the measured floating shell before the drag.
            let rect = h.app.viewport.rect.unwrap();
            let at = rect.center() + offset;
            let proj = projection(&h.app, rect);
            assert!(pick(&h.app, &proj, at).iter().any(|hit| matches!(hit, Hit::Face { .. })), "the drag starts on the box");
            let pivot = crate::prefs::pivot_under(&h.app, &proj, at);
            // Near an edge the pick snaps the pivot onto it, a few pixels from the cursor:
            // the pivot keeps the pixel it was picked on, wherever that is.
            let start = proj.to_screen(pivot).unwrap();
            assert!(start.distance(at) < 8.0, "perspective={perspective}: picked under the cursor: {at:?} -> {start:?}");
            let before = h.app.cam;
            h.call("ui.drag", json!({"x0":at.x,"y0":at.y,"x1":at.x+150.0,"y1":at.y+60.0,"button":"middle","shift":true,"steps":20}));
            assert!((h.app.cam.yaw - before.yaw).abs() > 0.5, "the view turned");
            let after = projection(&h.app, rect).to_screen(pivot).unwrap();
            assert!(after.distance(start) < 0.5, "perspective={perspective}: {start:?} -> {after:?}");
        }
    }
}

/// An erratic orbit drag (big jumps, reversals, the pointer over the panels and back) keeps
/// the point picked when it began on its pixel the whole way, and coming back to where it
/// began brings back the view it began with (up to the rounding of the pointer's f32 steps).
#[test]
fn an_erratic_cursor_orbit_keeps_its_pivot_and_comes_back() {
    for perspective in [false, true] {
        let mut h = crate::scenario::Harness::new();
        h.app.preferences.orbit_around = crate::prefs::OrbitAround::Cursor;
        h.app.ui.perspective = perspective;
        h.app.run("solid.box", json!({"length":80,"width":50,"height":20})).unwrap();
        h.app.fit_view();
        h.frames(2);
        let rect = h.app.viewport.rect.unwrap();
        let at = rect.center() + vec2(30.0, 20.0);
        let pivot = crate::prefs::pivot_under(&h.app, &projection(&h.app, rect), at);
        let start = h.app.cam;
        let shift = egui::Modifiers { shift: true, ..Default::default() };
        let send = |h: &mut crate::scenario::Harness, e: egui::Event| {
            h.app.synthetic.push(e);
            h.frame();
        };
        send(&mut h, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        send(&mut h, egui::Event::PointerMoved(at));
        send(&mut h, egui::Event::PointerButton { pos: at, button: egui::PointerButton::Middle, pressed: true, modifiers: shift });
        let mut seed: u32 = 12345;
        let mut next = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f32::from((seed >> 16) as u16) / 65535.0
        };
        for i in 0..300 {
            // Mostly small moves, now and then a big jump, some of them over the panels.
            let p = if i % 17 == 0 { pos2(next() * 1600.0, next() * 1000.0) } else { at + vec2((next() - 0.5) * 900.0, (next() - 0.5) * 200.0) };
            send(&mut h, egui::Event::PointerMoved(p));
            assert_eq!(h.app.viewport.orbit_pivot, Some(pivot), "move {i}: the pivot is picked once, when the drag begins");
            let s = projection(&h.app, rect).to_screen(pivot).unwrap();
            assert!(s.distance(at) < 1.0, "perspective={perspective}, move {i} to {p:?}: the pivot moved to {s:?}");
        }
        send(&mut h, egui::Event::PointerMoved(at));
        let s = projection(&h.app, rect).to_screen(pivot).unwrap();
        assert!(s.distance(at) < 1.0, "{s:?}");
        let back = h.app.cam;
        assert!(
            (back.yaw - start.yaw).rem_euclid(std::f64::consts::TAU).min((start.yaw - back.yaw).rem_euclid(std::f64::consts::TAU)) < 1e-6,
            "{start:?} -> {back:?}"
        );
        assert!((back.pitch - start.pitch).abs() < 1e-6, "{start:?} -> {back:?}");
        assert!((back.target - start.target).len() < 1e-4, "back where it began: {start:?} -> {back:?}");
        send(&mut h, egui::Event::PointerButton { pos: at, button: egui::PointerButton::Middle, pressed: false, modifiers: shift });
    }
}

#[test]
fn shift_orbit_keeps_cursor_pivot_and_default_middle_pan() {
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.orbit_around = crate::prefs::OrbitAround::Cursor;
    h.app.run("solid.box", json!({"length":40,"width":40,"height":40})).unwrap();
    h.app.fit_view();
    let rect = h.app.viewport.rect.unwrap();
    let at = rect.center() + vec2(50.0, -20.0);
    orbit_drag(&mut h.app, rect, 0.0, 0.0, Some(at), false, true);
    let pivot = h.app.viewport.orbit_pivot.unwrap();
    let before = projection(&h.app, rect).to_screen(pivot).unwrap();
    for _ in 0..20 {
        orbit_drag(&mut h.app, rect, 4.0, 2.0, Some(at), true, true);
        let after = projection(&h.app, rect).to_screen(pivot).unwrap();
        assert!(after.distance(before) < 2.0);
    }
    h.app.viewport.orbit_free = None;
    h.app.viewport.orbit_pivot = None;
    h.call("ui.view", json!({"view":"front"}));
    let before = h.app.cam;
    h.call("ui.drag", json!({"x0":800,"y0":500,"x1":1020,"y1":500,"button":"middle","shift":true,"steps":40}));
    assert!((h.app.cam.yaw - 1.76).abs() < 1e-6, "before={before:?}, after={:?}", h.app.cam);
    h.call("ui.view", json!({"view":"front"}));
    h.call("ui.drag", json!({"x0":800,"y0":500,"x1":850,"y1":520,"button":"middle"}));
    assert_eq!(h.app.cam.yaw, before.yaw);
    assert_eq!(h.app.cam.pitch, before.pitch);
    assert_ne!(h.app.cam.target, before.target);
}

#[test]
fn preset_shift_middle_orbit_is_free_but_shift_right_orbit_snaps() {
    for preset in ["fusion", "inventor"] {
        let mut h = crate::scenario::Harness::new();
        h.app.preferences.nav = preset.into();
        h.call("ui.view", json!({"view":"front"}));
        let before = h.app.cam;
        h.call("ui.drag", json!({"x0":800,"y0":500,"x1":850,"y1":520,"button":"middle","shift":true,"steps":20}));
        let mut expected = before;
        expected.orbit(50.0, 20.0);
        assert!((h.app.cam.yaw - expected.yaw).abs() < 1e-6, "{preset}: {:?}", h.app.cam);
        assert!((h.app.cam.pitch - expected.pitch).abs() < 1e-6, "{preset}: {:?}", h.app.cam);
        assert_eq!(h.app.cam.target, before.target);
        assert_eq!(h.app.cam.distance, before.distance);
        h.call("ui.view", json!({"view":"front"}));
        h.call("ui.drag", json!({"x0":800,"y0":500,"x1":1020,"y1":500,"button":"right","shift":true,"steps":40}));
        assert!((h.app.cam.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-6, "{preset}: {:?}", h.app.cam);
    }
}

/// Shift let go partway through a Shift+middle orbit (Fusion): the rest of the drag still
/// orbits (#9557), and Shift pressed partway through a middle pan still pans. With
/// `nav_drag_keeps_mode` off, the keys held decide on every frame, as before.
#[test]
fn a_middle_drag_keeps_its_mode_when_shift_changes() {
    let shift = egui::Modifiers { shift: true, ..Default::default() };
    let send = |h: &mut crate::scenario::Harness, e: egui::Event| {
        h.app.synthetic.push(e);
        h.frame();
    };
    for keeps in [true, false] {
        for (first, then) in [(shift, egui::Modifiers::NONE), (egui::Modifiers::NONE, shift)] {
            let mut h = crate::scenario::Harness::new();
            h.app.preferences.nav_drag_keeps_mode = keeps;
            h.call("ui.view", json!({"view":"front"}));
            let start = h.app.cam;
            let at = h.app.viewport.rect.unwrap().center();
            send(&mut h, egui::Event::ModifiersChanged(first));
            send(&mut h, egui::Event::PointerMoved(at));
            send(&mut h, egui::Event::PointerButton { pos: at, button: egui::PointerButton::Middle, pressed: true, modifiers: first });
            for i in 1..=10 {
                send(&mut h, egui::Event::PointerMoved(at + vec2(4.0 * i as f32, 0.0)));
            }
            let half = h.app.cam;
            send(&mut h, egui::Event::ModifiersChanged(then));
            for i in 11..=20 {
                send(&mut h, egui::Event::PointerMoved(at + vec2(4.0 * i as f32, 0.0)));
            }
            send(
                &mut h,
                egui::Event::PointerButton { pos: at + vec2(80.0, 0.0), button: egui::PointerButton::Middle, pressed: false, modifiers: then },
            );
            let end = h.app.cam;
            // Fusion: Shift+middle orbits, middle alone pans.
            let case = format!("keeps={keeps}, Shift {} partway", if first.shift { "let go" } else { "pressed" });
            let moved = |a: Camera, b: Camera, orbit: bool| {
                if orbit {
                    assert_ne!(a.yaw, b.yaw, "{case}: orbits");
                    assert_eq!(a.target, b.target, "{case}: doesn't pan");
                } else {
                    assert_eq!(a.yaw, b.yaw, "{case}: doesn't orbit");
                    assert_ne!(a.target, b.target, "{case}: pans");
                }
            };
            moved(start, half, first.shift);
            moved(half, end, if keeps { first.shift } else { then.shift });
        }
    }
}

#[test]
fn navigation_cube_repeated_face_click_keeps_view_and_drag_snaps() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"navigator_style":"cube"}));
    h.run(&json!([
        {"call":"ui.view","params":{"view":"home"}},
        {"click":{"handle":"axis_view:z"}},
        {"expect":{"camera":{"pitch_deg":90,"yaw_deg":0}}},
        {"click":{"handle":"axis_view:z"}},
        {"expect":{"camera":{"pitch_deg":90,"yaw_deg":0}}},
        {"call":"ui.view","params":{"view":"front"}}
    ]))
    .unwrap();
    let c = axis_center(&h.app, h.app.viewport.rect.unwrap());
    h.call("ui.drag", json!({"x0":c.x,"y0":c.y,"x1":c.x+220.0,"y1":c.y,"shift":true,"steps":40}));
    assert!((h.app.cam.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert!(h.app.viewport.axis_drag.is_none());
    assert!(h.app.viewport.orbit_free.is_none());
    assert!(h.app.viewport.boxsel.is_none());
}

#[test]
fn construction_point_is_pickable_only_while_visible() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("construct.point.vertex", &json!({"point":{"point":[20,15,10]},"name":"Point1"})).unwrap();
    s.execute("construct.axis.two_points", &json!({"a":{"point":[-20,0,10]},"b":{"point":[-20,0,30]},"name":"Axis1"})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    let proj = projection(&app, Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0)));
    let pos = proj.to_screen(Vec3::new(20.0, 15.0, 10.0)).unwrap();
    let is_point =
        |hit: &Hit| hit_sel(hit).is_some_and(|sel| serde_json::to_value(sel).unwrap() == json!({"type":"construction_point","name":"Point1"}));
    for prefer_free in [false, true] {
        app.preferences.prefer_free_sketch_points = prefer_free;
        assert!(pick(&app, &proj, pos).iter().any(is_point));
        assert!(pick_all(&app, &proj, pos).iter().any(is_point));
    }
    let axis_pos = proj.to_screen(Vec3::new(-20.0, 0.0, 20.0)).unwrap();
    let axis = Hit::Axis { name: "Axis1".into() };
    assert!(pick(&app, &proj, axis_pos).contains(&axis));
    assert_eq!(
        crate::selection::SelInput::new("Axis", crate::selection::AXES, false).accepts(&axis, None, |_, _| false),
        Some(Sel::Axis { name: "Axis1".into() })
    );
    app.session.visibility.hidden_origin.extend(["Point1".into(), "Axis1".into()]);
    assert!(!pick(&app, &proj, axis_pos).contains(&axis));
    assert!(!pick(&app, &proj, pos).iter().any(is_point));
    assert!(!pick_all(&app, &proj, pos).iter().any(is_point));
}

#[test]
fn construction_axes_span_the_model_and_points_show() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20})).unwrap();
    s.execute("construct.axis.two_points", &json!({"a": {"vertex": [0, 0, 20]}, "b": {"vertex": [40, 30, 20]}})).unwrap();
    s.execute("construct.point.vertex", &json!({"point": {"vertex": [40, 0, 20]}})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    let axes = construction_axis_segments(&app);
    assert_eq!(axes.len(), 1);
    let (name, a, b) = axes[0].clone();
    // The segment runs through both points and past them.
    let d = (b - a).normalized().unwrap();
    for p in [Vec3::new(0.0, 0.0, 20.0), Vec3::new(40.0, 30.0, 20.0)] {
        let t = (p - a).dot(d);
        assert!(t > 0.0 && t < a.dist(b) && (a + d * t).dist(p) < 1e-6, "{p:?} not on {a:?}..{b:?}");
    }
    assert_eq!(construction_points(&app).len(), 1);
    // It is an axis for dialogs (pattern directions, revolve axes), and hiding it hides it.
    let (o, dir) = crate::dialogs::axis_of(&app, &Sel::Axis { name: name.clone() }).unwrap();
    assert!(o.dist(Vec3::new(0.0, 0.0, 20.0)) < 1e-6 && dir.dist(d) < 1e-6);
    app.session.visibility.hidden_origin.push(name);
    assert!(construction_axis_segments(&app).is_empty());
}

/// Draft Analysis on the GPU takes its colours from the tokens: the sample design through the
/// viewport's real pipeline, offscreen (skipped without a GPU). In Off the faces that pass are
/// green; in Protan they are the red-green table's blue, and the negative faces are striped.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn draft_analysis_on_the_gpu_draws_the_modes_colours_and_stripes() {
    use crate::theme::ColorVision;
    let mut s = sketcraft_engine::Session::default();
    s.run_script(&sketcraft_engine::sample::script()).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.fit_view();
    app.run("inspect.draft", json!({"pull": [-1, -1, 0]})).unwrap();
    app.viewport.gpu = Some(GpuTarget { format: egui_wgpu::wgpu::TextureFormat::Rgba8Unorm, depth: None, samples: 1, tess: false, oit: false });
    const SIZE: u32 = 256;
    let proj = projection(&app, Rect::from_min_size(Pos2::ZERO, vec2(SIZE as f32, SIZE as f32)));
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render = |c: &crate::theme::AnalysisColors| {
        crate::gpu::render_offscreen(
            build_scene(&app),
            None,
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            analysis_uniform(&app),
            crate::gpu::analysis_colors(c),
        )
    };
    let off = Tokens::themed(false, ColorVision::Off).analysis;
    let protan = Tokens::themed(false, ColorVision::Protan).analysis;
    let flat = crate::theme::AnalysisColors { hatch: false, ..protan };
    let (Some(a), Some(b), Some(c)) = (render(&off), render(&protan), render(&flat)) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    // Pixels of a colour under any light: the same hue, so the same ratios between channels.
    let like = |img: &[u8], c: [f32; 3]| {
        img.as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                let m = c.iter().copied().fold(0.0, f32::max);
                let k = f32::from(p[0].max(p[1]).max(p[2])) / 255.0 / m;
                k > 0.3 && (0..3).all(|i| (f32::from(p[i]) / 255.0 - c[i] * k).abs() < 0.04)
            })
            .count()
    };
    let n = (SIZE * SIZE) as usize;
    assert!(like(&a, off.draft[0]) > n / 50 && like(&a, protan.draft[0]) == 0, "Off: green passing faces");
    assert!(like(&b, protan.draft[0]) > n / 50 && like(&b, off.draft[0]) == 0, "Protan: blue passing faces");
    // The stripes: with them on, some negative pixels are darker, and nothing else changes.
    let negative = |p: &[u8]| like(p, protan.draft[2]) == 1;
    let changed: Vec<usize> = (0..n).filter(|&i| b[4 * i..4 * i + 3] != c[4 * i..4 * i + 3]).collect();
    let neg = (0..n).filter(|&i| negative(&c[4 * i..4 * i + 4])).count();
    assert!(!changed.is_empty() && changed.len() * 2 < neg, "{} striped of {neg} negative pixels", changed.len());
    assert!(changed.iter().all(|&i| negative(&c[4 * i..4 * i + 4])), "only negative faces are striped");
    assert!(changed.iter().all(|&i| b[4 * i..4 * i + 3].iter().zip(&c[4 * i..4 * i + 3]).all(|(x, y)| x <= y)), "stripes are darker");
}

/// The view's picture draws in the colour vision mode: a fixed sketch line in the light theme
/// takes Deutan's value, and Off's is gone.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_view_picture_draws_in_the_colour_vision_mode() {
    use crate::theme::ColorVision;
    if !crate::gpu::test_picture_renderer() {
        return;
    }
    let mut h = crate::scenario::Harness::new();
    h.app.ui.dark = false;
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    let made = h.app.run("sketch.line", json!({"points": [[-30, -20], [30, 20]]})).unwrap();
    h.app.run("sketch.constraint.fix", json!({"entity": made["created"]["curves"][0]})).unwrap();
    let mut cam = sketcraft_engine::view::home_camera(&h.app.session).unwrap();
    cam.set_view(sketcraft_engine::render::StandardView::Top);
    h.app.cam = cam;
    let count = |h: &crate::scenario::Harness, c: Color32| {
        let img = crate::control::view_rgba(&h.app, 320, 240).unwrap();
        img.as_chunks::<4>().0.iter().filter(|p| p[..3].iter().zip([c.r(), c.g(), c.b()]).all(|(a, b)| a.abs_diff(b) <= 8)).count()
    };
    let (off, deutan) = (Tokens::light().sketch_locked, Tokens::themed(false, ColorVision::Deutan).sketch_locked);
    let (a, b) = (count(&h, off), count(&h, deutan));
    assert!(a > 20 && b == 0, "Off: {a} fixed-line pixels, {b} in Deutan's colour");
    h.app.preferences.color_vision = ColorVision::Deutan;
    let (a, b) = (count(&h, off), count(&h, deutan));
    assert!(a == 0 && b > 20, "Deutan: {a} fixed-line pixels in Off's colour, {b} in Deutan's");
}

/// A mode's table reaches the headless picture's palette: the sketch states and axes it
/// changes, and nothing else (the light theme in Off is the engine's own colours).
#[test]
fn picture_palette_follows_the_colour_vision_mode() {
    use crate::theme::ColorVision;
    let rgb = |c: Color32| Rgb(c.r(), c.g(), c.b());
    let off = scene_palette(false, ColorVision::Off);
    let rg = scene_palette(false, ColorVision::Protan);
    let t = Tokens::themed(false, ColorVision::Protan);
    assert_eq!((rg.sketch_projected, rg.sketch_locked), (rgb(t.sketch_projected), rgb(t.sketch_locked)));
    assert_ne!(rg.sketch_projected, off.sketch_projected, "the red-green table changes projected lines in light");
    assert_eq!((rg.axes[0], rg.axes[1], rg.axes[2]), (rgb(t.axis_x), rgb(t.axis_y), off.axes[2]));
    assert_eq!((rg.sketch, rg.background, rg.grid, rg.body), (off.sketch, off.background, off.grid, off.body));
    let by = scene_palette(false, ColorVision::Tritan);
    assert_eq!(by.sketch_locked, rgb(Tokens::themed(false, ColorVision::Tritan).sketch_locked));
    assert_ne!(by.sketch_locked, off.sketch_locked);
    assert_eq!(by.sketch_projected, off.sketch_projected);
}

/// The mode is on from the next frame (theme, widgets, caches, `ui.render`), and Cancel puts
/// it back.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn colour_vision_mode_reaches_the_frame_and_the_view_caches() {
    use crate::theme::ColorVision;
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.language = "en".into();
    h.app.home.open = false;
    let keys = |app: &SolveApp| (scene_key(app), highlight_key(app));
    h.call("ui.prefs", json!({"open": "Interface"}));
    crate::prefs::reveal_preference(&mut h, "prefs:color_vision");
    assert!(h.widget("Color vision").is_some() && h.widget("Off").is_some() && h.widget("Deutan").is_some());
    let before = keys(&h.app);
    h.step(&json!({"click":{"handle":"dialog:Mode:Deutan"}})).unwrap();
    h.frame();
    assert_eq!(crate::theme::vision(), ColorVision::Deutan);
    assert_eq!(Tokens::get(), Tokens::themed(crate::theme::is_dark(), ColorVision::Deutan));
    let after = keys(&h.app);
    assert!(before.0 != after.0 && before.1 != after.1, "both caches are keyed on the mode");
    h.call("ui.prefs", json!({"button": "cancel"}));
    h.frame();
    assert_eq!((h.app.preferences.color_vision, crate::theme::vision()), (ColorVision::Off, ColorVision::Off));
    assert_eq!(keys(&h.app), before);
    // `ui.render` shows the mode too: a fixed sketch line in Deutan's value (in the light
    // theme, where the red-green table changes it; the render follows the theme).
    if !crate::gpu::test_picture_renderer() {
        return;
    }
    h.call("ui.prefs", json!({"theme": "light"}));
    h.frame();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    let made = h.app.run("sketch.line", json!({"points": [[-30, -20], [30, 20]]})).unwrap();
    h.app.run("sketch.constraint.fix", json!({"entity": made["created"]["curves"][0]})).unwrap();
    h.call("ui.view", json!({"view": "top"}));
    let path = std::env::temp_dir().join(format!("sc_colour_vision_render_{}.png", std::process::id()));
    let near = |h: &mut crate::scenario::Harness, c: Color32| {
        let r = h.call("ui.render", json!({"path": path.to_string_lossy(), "width": 400, "height": 300}));
        assert_eq!(r["ok"], true, "{r}");
        let img = image::open(&path).unwrap().to_rgb8();
        img.pixels().filter(|p| p.0.iter().zip([c.r(), c.g(), c.b()]).all(|(a, b)| a.abs_diff(b) <= 8)).count()
    };
    let (off, deutan) = (Tokens::light().sketch_locked, Tokens::themed(false, ColorVision::Deutan).sketch_locked);
    let (a, b) = (near(&mut h, off), near(&mut h, deutan));
    assert!(a > 20 && b == 0, "Off: {a} fixed-line pixels, {b} in Deutan's colour");
    h.call("ui.prefs", json!({"color_vision": "deutan"}));
    h.frame();
    let (a, b) = (near(&mut h, off), near(&mut h, deutan));
    h.call("ui.prefs", json!({"color_vision": "off"}));
    h.frame();
    let _ = std::fs::remove_file(&path);
    assert!(a == 0 && b > 20, "Deutan: {a} fixed-line pixels in Off's colour, {b} in Deutan's");
}

/// #7058: an imported SVG leaves a sketch with regions and no body. A marquee over it must
/// take those regions, as a click on one does: the box selection only looked for bodies, and
/// the regions of a body-less design were out of reach.
#[test]
fn a_marquee_over_an_imported_svg_sketch_selects_its_regions() {
    let mut h = crate::scenario::Harness::new();
    let svg = r#"<svg width="40mm" height="40mm" viewBox="0 0 40 40"><path d="M0 0 H40 V40 H0 Z"/></svg>"#;
    let imported = h.app.run("sketch.insert_svg", json!({"text": svg})).unwrap();
    let sketch = imported["sketch"].as_u64().unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    assert!(crate::open_meshing::session_world(&h.app.session).bodies.is_empty(), "the import leaves a sketch, not a body");
    h.call("ui.view", json!({"view": "top"}));
    let st = crate::open_meshing::session_world(&h.app.session);
    let ss = st.sketch(sketch).unwrap();
    assert_eq!(ss.profiles.len(), 1, "the imported square is one region");
    let (low, high) = ss.sketch.bounds().unwrap();
    let a = ss.plane.to_world(Vec2::new(low.x - 5.0, low.y - 5.0));
    let b = ss.plane.to_world(Vec2::new(high.x + 5.0, high.y + 5.0));
    h.step(&json!({"drag": [{"world": [a.x, a.y, a.z]}, {"world": [b.x, b.y, b.z]}]})).unwrap();
    assert_eq!(h.app.session.selection, vec![Sel::Profile { sketch, index: 0 }], "the marquee takes the imported region");
}

/// #7058: the reporter drags with Shift held to add the second region of the import to the
/// selection, so that both are extruded at once.
#[test]
fn a_shift_marquee_adds_the_other_region_of_an_imported_svg_sketch() {
    let mut h = crate::scenario::Harness::new();
    let svg = r#"<svg width="80mm" height="40mm" viewBox="0 0 80 40"><path d="M0 0 H30 V30 H0 Z"/><path d="M50 0 H80 V30 H50 Z"/></svg>"#;
    let imported = h.app.run("sketch.insert_svg", json!({"text": svg})).unwrap();
    let sketch = imported["sketch"].as_u64().unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.call("ui.view", json!({"view": "top"}));
    h.call("ui.view", json!({"view": "fit"}));
    let st = crate::open_meshing::session_world(&h.app.session);
    let ss = st.sketch(sketch).unwrap();
    assert_eq!(ss.profiles.len(), 2, "the imported squares are two regions");
    // The world box of each region, a little outside its edges.
    let mut boxes = Vec::new();
    for p in ss.profiles.iter() {
        let mut low = Vec2::new(f64::INFINITY, f64::INFINITY);
        let mut high = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for seg in p.region.outer.segs.iter() {
            for q in [seg.start(), seg.end()] {
                low.x = low.x.min(q.x);
                low.y = low.y.min(q.y);
                high.x = high.x.max(q.x);
                high.y = high.y.max(q.y);
            }
        }
        boxes.push((ss.plane.to_world(Vec2::new(low.x - 2.0, low.y - 2.0)), ss.plane.to_world(Vec2::new(high.x + 2.0, high.y + 2.0))));
    }
    let (a, b) = boxes[0];
    h.step(&json!({"drag": [{"world": [a.x, a.y, a.z]}, {"world": [b.x, b.y, b.z]}]})).unwrap();
    assert_eq!(h.app.session.selection, vec![Sel::Profile { sketch, index: 0 }], "the marquee takes the first region");
    let (c, d) = boxes[1];
    // The drag runs from the box's top-left corner to its bottom-right one: a release on the
    // navigator compass in the view's top-right corner drops the rubber band before it
    // selects, so the second region is taken by a drag that ends over the model.
    h.step(&json!({"drag": [{"world": [c.x, d.y, c.z]}, {"world": [d.x, c.y, d.z]}], "shift": true})).unwrap();
    assert_eq!(
        h.app.session.selection,
        vec![Sel::Profile { sketch, index: 0 }, Sel::Profile { sketch, index: 1 }],
        "the shift marquee adds the second region"
    );
}

/// #7058: the regions are what a body-less design has to select, but a design that has a body
/// keeps the old behaviour: the marquee takes the bodies, not the regions.
#[test]
fn a_marquee_in_a_design_with_a_body_takes_only_the_bodies() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length":20,"width":20,"height":20})).unwrap();
    let svg = r#"<svg width="40mm" height="40mm" viewBox="0 0 40 40"><path d="M0 0 H40 V40 H0 Z"/></svg>"#;
    let imported = h.app.run("sketch.insert_svg", json!({"text": svg})).unwrap();
    let sketch = imported["sketch"].as_u64().unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.settle();
    assert_eq!(crate::open_meshing::session_world(&h.app.session).bodies.len(), 1, "the design has a body and a sketch");
    assert!(!crate::open_meshing::session_world(&h.app.session).bodies[0].mesh().edges.is_empty(), "the body is meshed");
    h.call("ui.view", json!({"view": "top"}));
    h.settle();
    h.call("ui.view", json!({"view": "fit"}));
    h.settle();
    let st = crate::open_meshing::session_world(&h.app.session);
    let ss = st.sketch(sketch).unwrap();
    let (low, high) = ss.sketch.bounds().unwrap();
    // The screen box of the body and of the region: the marquee covers both.
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for b in &st.bodies {
        let m = b.mesh();
        for t in m.triangles.iter() {
            if let Some(tr) = m.tri(t) {
                for p in tr {
                    lo[0] = lo[0].min(p.x);
                    lo[1] = lo[1].min(p.y);
                    lo[2] = lo[2].min(p.z);
                    hi[0] = hi[0].max(p.x);
                    hi[1] = hi[1].max(p.y);
                    hi[2] = hi[2].max(p.z);
                }
            }
        }
    }
    let mut points = Vec::new();
    for x in [lo[0], hi[0]] {
        for y in [lo[1], hi[1]] {
            for z in [lo[2], hi[2]] {
                points.push(Vec3::new(x, y, z));
            }
        }
    }
    for x in [low.x, high.x] {
        for y in [low.y, high.y] {
            points.push(ss.plane.to_world(Vec2::new(x, y)));
        }
    }
    let mut screen_lo = [f64::INFINITY; 2];
    let mut screen_hi = [f64::NEG_INFINITY; 2];
    for p in &points {
        let s = h.at(&json!({"world": [p.x, p.y, p.z]})).unwrap();
        screen_lo[0] = screen_lo[0].min(s[0]);
        screen_lo[1] = screen_lo[1].min(s[1]);
        screen_hi[0] = screen_hi[0].max(s[0]);
        screen_hi[1] = screen_hi[1].max(s[1]);
    }
    h.step(&json!({"drag": [[screen_lo[0] - 5.0, screen_lo[1] - 5.0], [screen_hi[0] + 5.0, screen_hi[1] + 5.0]]})).unwrap();
    let taken = h.app.session.selection.clone();
    assert!(!taken.is_empty(), "the marquee takes the body it covers");
    assert!(taken.iter().all(|s| matches!(s, Sel::Body { .. })), "a design with a body takes the bodies alone, not the regions: {taken:?}");
}

/// #7058: a mask that asks for edges or faces never takes the regions: the regions are
/// neither. The Patch dialog's boundary input takes profiles or edges, so the marquee must
/// leave it the edges alone.
#[test]
fn a_marquee_in_a_dialog_that_asks_for_edges_takes_no_region() {
    let mut h = crate::scenario::Harness::new();
    let svg = r#"<svg width="40mm" height="40mm" viewBox="0 0 40 40"><path d="M0 0 H40 V40 H0 Z"/></svg>"#;
    let imported = h.app.run("sketch.insert_svg", json!({"text": svg})).unwrap();
    let sketch = imported["sketch"].as_u64().unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.call("ui.view", json!({"view": "top"}));
    // The Patch dialog takes its boundary from profiles or edges.
    h.step(&json!({"start": "surface.patch"})).unwrap();
    assert!(h.app.dialog.is_some(), "the patch dialog is open");
    let st = crate::open_meshing::session_world(&h.app.session);
    let ss = st.sketch(sketch).unwrap();
    let (low, high) = ss.sketch.bounds().unwrap();
    let a = ss.plane.to_world(Vec2::new(low.x - 5.0, low.y - 5.0));
    let b = ss.plane.to_world(Vec2::new(high.x + 5.0, high.y + 5.0));
    h.step(&json!({"drag": [{"world": [a.x, a.y, a.z]}, {"world": [b.x, b.y, b.z]}]})).unwrap();
    let items = h.app.dialog.as_ref().unwrap().inputs[0].items.clone();
    assert!(!items.iter().any(|s| matches!(s, Sel::Profile { .. })), "a mask that asks for edges takes no region: {items:?}");
}

/// #4376: with the Lasso selection shape, a drag in a sketch draws a freehand loop and takes
/// the curves wholly inside it. An L shaped loop leaves out a line in its notch and a line
/// that cuts across the notch, which no rectangle could; Shift adds to the selection.
#[test]
fn a_lasso_in_a_sketch_takes_the_curves_wholly_inside_it() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    let line = |h: &mut crate::scenario::Harness, a: [f64; 2], b: [f64; 2]| {
        let made = h.app.run("sketch.line", json!({"points": [a, b]})).unwrap();
        made["created"]["curves"][0].as_str().unwrap().to_string()
    };
    let bottom = line(&mut h, [25.0, 25.0], [75.0, 25.0]);
    let left = line(&mut h, [25.0, 35.0], [25.0, 75.0]);
    let notch = line(&mut h, [60.0, 60.0], [75.0, 75.0]);
    let across = line(&mut h, [30.0, 75.0], [75.0, 30.0]);
    let far = line(&mut h, [120.0, 25.0], [120.0, 75.0]);
    h.call("ui.view", json!({"view": "top"}));
    h.call("ui.view", json!({"view": "fit"}));
    let r = h.call("ui.set", json!({"lassoSelect": true}));
    assert_eq!(r["ok"], true, "{r}");
    let loop_at = |pts: &[[f64; 2]]| pts.iter().map(|p| json!({"world": [p[0], p[1], 0.0]})).collect::<Vec<_>>();
    let curves = |h: &crate::scenario::Harness| {
        h.app
            .session
            .selection
            .iter()
            .filter_map(|s| if let Sel::SketchCurve { id, .. } = s { Some(id.clone()) } else { None })
            .collect::<Vec<_>>()
    };
    let l = [[15.0, 15.0], [85.0, 15.0], [85.0, 45.0], [45.0, 45.0], [45.0, 85.0], [15.0, 85.0], [15.0, 20.0]];
    h.step(&json!({"drag": loop_at(&l), "steps": 6})).unwrap();
    assert!(h.app.viewport.boxsel.is_none(), "the release ends the lasso");
    assert_eq!(curves(&h), vec![bottom.clone(), left.clone()], "not {notch}, {across} or {far}");
    // Shift adds the far line with a second loop; drawn the other way round, it is the same.
    let around_far = [[110.0, 15.0], [110.0, 85.0], [130.0, 85.0], [130.0, 15.0], [112.0, 15.0]];
    h.step(&json!({"drag": loop_at(&around_far), "steps": 6, "shift": true})).unwrap();
    assert_eq!(curves(&h), vec![bottom, left, far]);
    // Back to the rectangle: a plain drag is a box again.
    h.call("ui.set", json!({"lassoSelect": false}));
    h.step(&json!({"drag": loop_at(&[[15.0, 15.0], [85.0, 85.0]])})).unwrap();
    assert_eq!(curves(&h).len(), 4, "the window box takes all four lines inside it: {:?}", curves(&h));
}
