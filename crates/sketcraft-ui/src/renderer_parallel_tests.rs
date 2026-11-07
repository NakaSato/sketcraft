//! Parallel batching and shared caches match the serial results.
#[test]
fn group_marker_geometry_is_shared_across_views_and_replaced_after_model_changes() {
    use super::*;
    let mut model = Model::default();
    model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    model.add_edge(DVec3::X, DVec3::X + DVec3::Y).unwrap();
    let group = model.make_object(&model.selectable_entities(), false, "Group").unwrap();
    let mut cache = GroupMarkerCache::default();
    let first = cache.update(&model, 1);
    let point = first.iter().find(|marker| marker.id == group).unwrap().point;
    assert!(Arc::ptr_eq(&first, &cache.update(&model, 1)));
    model.move_selection(&std::collections::BTreeSet::from([group]), DVec3::X * 2.).unwrap();
    let moved = cache.update(&model, 2);
    let marker = moved.iter().find(|marker| marker.id == group).unwrap();
    assert!(marker.point.distance(point + DVec3::X * 2.) < 1e-10);
    assert_eq!(marker.owners, model.expand_entities(&std::collections::BTreeSet::from([group])));
    assert!(cache.update(&Model::default(), 3).is_empty());
}

#[test]
fn narrow_annotation_queries_cover_actual_tessellation_and_antialiasing() {
    use super::*;
    for pixels in [1., 2., 2.5] {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(pixels);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            for (a, b) in [(DVec2::new(10., 15.), DVec2::new(400., 330.)),
                (DVec2::new(10., 15.), DVec2::new(10., 330.)),
                (DVec2::new(10., 15.), DVec2::new(400., 15.))] {
                let footprint = annotation_stroke_footprint(a, b, 0.75 + 2. / f64::from(pixels));
                let winding = (footprint[1] - footprint[0]).perp_dot(footprint[2] - footprint[1]).signum();
                assert!(winding > 0., "annotation clipping expects positive winding");
                let shape = egui::Shape::line_segment([egui::pos2(a.x as f32, a.y as f32),
                    egui::pos2(b.x as f32, b.y as f32)], egui::Stroke::new(1.5, Color32::WHITE));
                let primitives = ui.ctx().tessellate(vec![egui::epaint::ClippedShape::new(Rect::EVERYTHING, shape)], pixels);
                assert!(!primitives.is_empty());
                for primitive in primitives {
                    if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        for vertex in mesh.vertices {
                            let p = DVec2::new(f64::from(vertex.pos.x), f64::from(vertex.pos.y));
                            assert!((0..4).all(|i| winding * (footprint[(i+1)%4] - footprint[i])
                                .perp_dot(p - footprint[i]) >= -1e-5), "stroke vertex outside query: {p:?}");
                        }
                    }
                }
                if (b-a).min_element() > 0. {
                    assert!(polygon_area(&[footprint]) < (b.x-a.x)*(b.y-a.y)*0.15);
                }
            }
        });
    }
}

#[test]
fn screen_ordered_outline_hierarchy_matches_general_visibility_queries() {
    use super::*;
    let mut general = Occlusion::default();
    let mut outlines = Occlusion { outline_order: true, ..Default::default() };
    for i in 0..512 {
        let lo = DVec2::new((i * 37 % 100) as f64, (i * 53 % 100) as f64);
        let hi = lo + DVec2::new(3., 0.7);
        let depth = 0.2 + (i % 7) as f64 * 0.01;
        occluding_rectangle(&mut general, lo, hi, depth);
        occluding_rectangle(&mut outlines, lo, hi, depth);
    }
    for i in 0..100 {
        let a = DVec3::new(0., i as f64 + 0.1, 0.7);
        let b = DVec3::new(104., 100. - i as f64, 0.7);
        let expected = general.visible_intervals(a, b);
        let actual = outlines.visible_intervals(a, b);
        assert_eq!(expected.len(), actual.len());
        for (a, b) in expected.iter().zip(&actual) {
            assert!((a.0 - b.0).abs() < 1e-10 && (a.1 - b.1).abs() < 1e-10);
        }
    }
    let polygon = vec![DVec2::ZERO, DVec2::new(104., 0.), DVec2::splat(104.), DVec2::new(0., 104.)];
    let area = |regions: Vec<Vec<DVec2>>| regions.iter().map(|p| {
        (1..p.len()-1).map(|i| (p[i]-p[0]).perp_dot(p[i+1]-p[0]).abs()*0.5).sum::<f64>()
    }).sum::<f64>();
    assert!((area(general.visible_polygons(polygon.clone(), 0.7)) - area(outlines.visible_polygons(polygon, 0.7))).abs() < 1e-7);
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn ordinary_parallel_edges_preserve_the_completed_outline_hierarchy_between_frames() {
    use super::*;
    let camera = Camera::default();
    let size = DVec2::splat(512.);
    let inverse = camera.matrix(size).inverse();
    let world = |x, y| inverse.project_point3(DVec3::new(x / 256. - 1., 1. - y / 256., 0.4));
    let edges: Vec<_> = (0..256).map(|i| EdgeDraw {
        id: i, key: (0, 1), points: [world(10., i as f64 * 1.8), world(500., i as f64 * 1.8)],
        color: Color32::BLACK, width: 1.8, visible: true, back: false, overlay: false,
    }).collect();
    let mut mesh = Mesh::new(&camera, size);
    mesh.draw_edges(&edges, true, true);
    mesh.selection_occlusion.prepare_queries();
    assert!(!mesh.selection_occlusion.nodes.is_empty());
    let hierarchy = Arc::clone(&mesh.selection_occlusion.nodes);
    let revision = mesh.selection_occlusion.revision;
    mesh.selection_occlusion.begin_frame();
    mesh.draw_edges(&edges, true, true);
    mesh.selection_occlusion.prepare_queries();
    assert_eq!(mesh.selection_occlusion.revision, revision);
    assert!(Arc::ptr_eq(&hierarchy, &mesh.selection_occlusion.nodes),
        "collecting ordinary edges must not truncate/rebuild the existing hierarchy");
}

#[test]
fn shared_polygon_splits_match_independent_half_plane_clipping() {
    use super::*;
    let polygon = vec![DVec2::new(-2., -1.), DVec2::new(0., -2.),
        DVec2::new(3., -1.), DVec2::new(2., 2.), DVec2::new(-1., 3.)];
    for i in 0..2000 {
        let angle = i as f64 * 0.37;
        let normal = DVec2::new(angle.cos(), angle.sin());
        let offset = (i % 41) as f64 * 0.2 - 4.;
        let side = |p: DVec2| normal.dot(p) + offset;
        let (inside, outside) = split_screen_polygon(&polygon, side);
        assert_eq!(inside, clip_polygon(&polygon, side));
        assert_eq!(outside, clip_polygon(&polygon, |p| -side(p)));
    }
    for offset in [-2. - 1e-14, -2., -2. + 1e-14, 1., 3.] {
        let side = |p: DVec2| p.x + offset;
        let (inside, outside) = split_screen_polygon(&polygon, side);
        assert_eq!(inside, clip_polygon(&polygon, side));
        assert_eq!(outside, clip_polygon(&polygon, |p| -side(p)));
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn parallel_joint_guides_match_serial_clipping_dashes_and_annotation_masks() {
    use super::*;
    let camera = Camera::default();
    let size = DVec2::splat(512.);
    let inverse = camera.matrix(size).inverse();
    let world = |x, y, z| inverse.project_point3(DVec3::new(x / 256. - 1., 1. - y / 256., z));
    let stock = [
        [world(10., 10., 0.4), world(500., 10., 0.4), world(500., 500., 0.4)],
        [world(10., 10., 0.4), world(500., 500., 0.4), world(10., 500., 0.4)],
    ];
    let lines: Vec<_> = (0..256).map(|i| {
        let y = 15. + f64::from(i) * 1.8;
        [world(15., y, 0.6), world(495., 510. - y, 0.6)]
    }).collect();
    let build = |parallel: bool| {
        let mut mesh = Mesh::new(&camera, size);
        mesh.enable_gpu_edge_occlusion();
        for triangle in stock { mesh.world_triangle(&camera, triangle, Color32::WHITE); }
        mesh.world_triangle(&camera, [world(200., 0., 0.4 - 1e-9), world(260., 512., 0.4 - 1e-9), world(330., 0., 0.4 - 1e-9)], Color32::GRAY);
        mesh.styled_line(world(0., 240., 0.4 - 1e-9), world(512., 240., 0.4 - 1e-9), 2., Color32::BLACK, false, false);
        for [a, b] in &lines { mesh.register_joint_guide(1, *a, *b); }
        let surface = mesh.cached_joint_surface_for_entities(1, &stock, &Default::default());
        let paint = |mesh: &mut Mesh, &[a, b]: &[DVec3; 2]| {
            mesh.joint_guide(a, b, &surface, 1.8, Color32::GREEN, true);
        };
        if parallel { mesh.paint_overlays(&lines, 64, paint); }
        else { for line in &lines { paint(&mut mesh, line); } }
        mesh
    };
    let serial = build(false);
    let parallel = build(true);
    assert_eq!(serial.indices, parallel.indices);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&serial.vertices), bytemuck::cast_slice::<_, u8>(&parallel.vertices));
    assert_eq!(serial.annotation_occluders, parallel.annotation_occluders);
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn parallel_face_batches_match_serial_mesh_depth_and_shadow_coverage() {
    use super::*;
    let camera = Camera::default();
    let size = DVec2::splat(512.);
    let inverse = camera.matrix(size).inverse();
    let world = |x, y, z| inverse.project_point3(DVec3::new(x / 256. - 1., 1. - y / 256., z));
    let mut model = Model::default();
    for i in 0..96 {
        let x = 10. + f64::from(i % 12) * 40.;
        let y = 10. + f64::from(i / 12) * 60.;
        let points = [world(x, y, 0.4), world(x + 30., y, 0.4), world(x, y + 40., 0.4)];
        for j in 0..3 { model.add_edge(points[j], points[(j + 1) % 3]).unwrap(); }
    }
    let mut cache = FaceCache::default();
    cache.update(&model);
    assert!(model.faces.len() >= 64);
    let shadows: Vec<_> = model.faces.keys().map(|id| {
        let [a, b, c] = cache.get(*id).triangles[0];
        let mid = (b + c) * 0.5;
        crate::shadows::FaceShadow { lit: vec![[a, b, mid]], shaded: vec![[a, mid, c]] }
    }).collect();
    for antialias in [false, true] {
        let build = |parallel: bool| {
            let mut mesh = Mesh::new(&camera, size);
            mesh.display.fog = true;
            mesh.display.shadow_antialiasing = antialias;
            // An existing prefix checks both vertex and index offsets.
            mesh.world_triangle(&camera, [world(0., 0., 0.8), world(512., 0., 0.8), world(0., 512., 0.8)], Color32::WHITE);
            let faces: Vec<_> = model.faces.values().zip(&shadows).enumerate().map(|(i, (face, shadow))| FaceDraw {
                face, cached: cache.get(face.id), color: Color32::from_rgb(100, 160, 200),
                shadow: (i % 3 != 0).then_some((shadow, Color32::DARK_GRAY)),
            }).collect();
            if parallel { mesh.draw_faces(&faces); }
            else { for face in &faces { mesh.draw_faces(std::slice::from_ref(face)); } }
            mesh
        };
        let serial = build(false);
        let parallel = build(true);
        assert_eq!(serial.indices, parallel.indices);
        assert_eq!(serial.triangle_ranges, parallel.triangle_ranges);
        assert_eq!(bytemuck::cast_slice::<_, u8>(&serial.vertices), bytemuck::cast_slice::<_, u8>(&parallel.vertices));
        assert_eq!(bytemuck::cast_slice::<_, u8>(&serial.shadow_vertices), bytemuck::cast_slice::<_, u8>(&parallel.shadow_vertices));
        assert_eq!(serial.occlusion.triangles.iter().map(|t| (t.entity, t.points)).collect::<Vec<_>>(),
            parallel.occlusion.triangles.iter().map(|t| (t.entity, t.points)).collect::<Vec<_>>());
        assert!(serial.edge_surfaces.iter().all(|(key, surfaces)| {
            let other = &parallel.edge_surfaces[key];
            surfaces.iter().map(|s| (s.gradient, s.inward)).eq(other.iter().map(|s| (s.gradient, s.inward)))
        }));
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn parallel_edge_batches_match_serial_geometry_and_outline_masks() {
    use super::*;
    let camera = Camera::default();
    let size = DVec2::splat(512.);
    let inverse = camera.matrix(size).inverse();
    let world = |x, y, z| inverse.project_point3(DVec3::new(x / 256. - 1., 1. - y / 256., z));
    for gpu in [false, true] {
        let build = |parallel: bool| {
            let mut mesh = Mesh::new(&camera, size);
            if gpu { mesh.enable_gpu_edge_occlusion(); }
            mesh.set_entity(Some(9000));
            mesh.world_triangle(&camera, [world(200., 30., 0.3), world(330., 460., 0.3), world(400., 40., 0.3)], Color32::WHITE);
            for selected in [false, true] {
                if selected { mesh.begin_selected_edges(); }
                let edges: Vec<_> = (0..288).map(|i| {
                    let y = 20. + f64::from(i) * 1.5;
                    EdgeDraw { id: i as u64 + u64::from(selected) * 1000,
                        key: (0, 1), points: [world(10., y, 0.4), world(490., 500. - y, 0.4)],
                        color: if selected { Color32::RED } else { Color32::BLACK },
                        width: if selected { 3. } else { 1.8 },
                        back: i % 7 == 0, visible: i % 11 != 0, overlay: i % 13 == 0 }
                }).collect();
                if parallel { mesh.draw_edges(&edges, true, true); }
                else { for edge in &edges { mesh.draw_edges(std::slice::from_ref(edge), true, true); } }
            }
            mesh
        };
        let serial = build(false);
        let parallel = build(true);
        assert_eq!(serial.indices, parallel.indices);
        assert_eq!(bytemuck::cast_slice::<_, u8>(&serial.vertices), bytemuck::cast_slice::<_, u8>(&parallel.vertices));
        assert_eq!(serial.annotation_occluders, parallel.annotation_occluders);
        assert_eq!(serial.selection_occlusion.triangles.iter().map(|t| (t.entity, t.points)).collect::<Vec<_>>(),
            parallel.selection_occlusion.triangles.iter().map(|t| (t.entity, t.points)).collect::<Vec<_>>());
    }
}
