//! Retained meshes, GPU buffers, stroke batching and the face cache.
use super::*;

#[test]
fn retained_mesh_reuses_identical_output_and_keeps_old_frames_alive() {
    let camera = Camera::default();
    let size = DVec2::new(640., 480.);
    let mut cache = SceneCache::default();
    let build = |cache: &mut SceneCache, color| {
        let mut mesh = Mesh::cached(&camera, size, cache);
        mesh.world_triangle(&camera, [DVec3::ZERO, DVec3::X, DVec3::Y], color);
        mesh.retain(cache)
    };
    let first = build(&mut cache, Color32::WHITE);
    let same = build(&mut cache, Color32::WHITE);
    assert!(Arc::ptr_eq(&first, &same));
    assert_eq!(cache.buffers.lock().unwrap().len(), 1);
    let changed = build(&mut cache, Color32::RED);
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(first.vertices[0].color, Color32::WHITE.to_array());
    assert_eq!(changed.vertices[0].color, Color32::RED.to_array());
    assert!(Arc::ptr_eq(cache.retained.as_ref().unwrap(), &changed));
    // Retained frames keep their allocations until their last reference ends.
    assert!(cache.buffers.lock().unwrap().is_empty());
    drop(first);
    drop(same);
    assert_eq!(cache.buffers.lock().unwrap().len(), 1);
    assert!(!changed.vertices.is_empty());
}

#[test]
fn retained_mesh_compares_indices_draw_ranges_and_normalized_parallel_depth() {
    let camera = Camera {
        projection: sketcraft_core::Projection::Parallel,
        ..Default::default()
    };
    let size = DVec2::new(640., 480.);
    let mut cache = SceneCache::default();
    let build = |cache: &mut SceneCache| {
        let mut mesh = Mesh::cached(&camera, size, cache);
        mesh.world_triangle(&camera, [DVec3::ZERO, DVec3::X, DVec3::Y], Color32::WHITE);
        mesh
    };
    let first = build(&mut cache).retain(&mut cache);
    let same = build(&mut cache).retain(&mut cache);
    assert!(Arc::ptr_eq(&first, &same));
    let mut mesh = build(&mut cache);
    mesh.indices.swap(0, 1);
    let reordered = mesh.retain(&mut cache);
    assert!(!Arc::ptr_eq(&first, &reordered));
    let mut mesh = build(&mut cache);
    mesh.triangle_ranges.clear();
    let strokes = mesh.retain(&mut cache);
    assert!(!Arc::ptr_eq(&first, &strokes));
}

#[test]
fn callback_batches_interleaved_faces_and_strokes_in_stable_pass_order() {
    let camera = Camera::default();
    let mut mesh = Mesh::new(&camera, DVec2::new(1000., 800.));
    // Use distinct indices to prove both pass ordering and completeness.
    mesh.indices = (0..27).collect();
    mesh.triangle_ranges = vec![6..12, 18..21, 24..27];
    let callback = mesh.into_callback();
    assert_eq!(callback.triangle_ranges, vec![0..12]);
    assert_eq!(
        callback.indices,
        [
            6, 7, 8, 9, 10, 11, 18, 19, 20, 24, 25, 26, 0, 1, 2, 3, 4, 5, 12, 13, 14, 15, 16,
            17, 21, 22, 23,
        ]
    );
    for ranges in [vec![], vec![0..27], vec![6..12]] {
        let mut mesh = Mesh::new(&camera, DVec2::new(1000., 800.));
        mesh.indices = (0..27).collect();
        mesh.triangle_ranges = ranges.clone();
        let expected_faces: Vec<_> = ranges.iter().flat_map(|range| range.clone()).collect();
        let expected_edges: Vec<_> = (0..27)
            .filter(|i| !ranges.iter().any(|r| r.contains(i)))
            .collect();
        let callback = mesh.into_callback();
        let face_end = callback
            .triangle_ranges
            .first()
            .map_or(0, |range| range.end as usize);
        assert_eq!(&callback.indices[..face_end], expected_faces);
        assert_eq!(&callback.indices[face_end..], expected_edges);
    }
}

#[test]
fn adjacency_survives_vertex_movement_and_refreshes_after_topology_edits() {
    let mut model = Model::default();
    for (a, b) in [
        (DVec3::ZERO, DVec3::X),
        (DVec3::X, DVec3::Y),
        (DVec3::Y, DVec3::ZERO),
    ] {
        model.add_edge(a, b).unwrap();
    }
    let face = model.faces.values().next().unwrap().clone();
    let (a, b) = (face.vertices[0], face.vertices[1]);
    let mut cache = FaceCache::default();
    cache.update(&model);
    assert_eq!(cache.adjacent_faces(a, b), &[face.id]);
    for vertex in model.vertices.values_mut() {
        vertex.position.z += 1.;
    }
    cache.update(&model);
    assert_eq!(cache.adjacent_faces(a, b), &[face.id]);
    assert_eq!(cache.topology_builds, 1);
    model.faces.clear();
    cache.update(&model);
    assert!(cache.adjacent_faces(a, b).is_empty());
    model.faces.insert(face.id, face.clone());
    cache.update(&model);
    assert_eq!(cache.adjacent_faces(b, a), &[face.id]);
    assert_eq!(cache.topology_builds, 3);
}

#[test]
fn parallel_faces_remain_distinct_after_webgl_depth_conversion() {
    let camera = Camera {
        projection: sketcraft_core::Projection::Parallel,
        ..Default::default()
    };
    let mut mesh = Mesh::new(&camera, DVec2::new(640., 480.));
    let depths = [0., 1e-7_f32, 1.01e-7_f32, 1.];
    let webgl_depth = |z: f32| (z * 2. - 1.) * 0.5 + 0.5;
    assert_eq!(webgl_depth(depths[1]), webgl_depth(depths[2]));
    mesh.vertices = depths
        .into_iter()
        .map(|z| Vertex {
            dash_phase: -1.,
            position: [0., 0., z],
            color: Color32::WHITE.to_array(),
            lateral: 0.,
            shadow_color: [0; 4],
        })
        .collect();
    mesh.normalize_parallel_depth();
    let mapped: Vec<_> = mesh.vertices.iter().map(|v| v.position[2]).collect();
    assert_eq!(mapped[0], 0., "overlays stay in front");
    assert_eq!(mapped[3], 1., "background lines stay behind geometry");
    assert!(webgl_depth(mapped[1]) < webgl_depth(mapped[2]));
    assert!(mapped[1] > 0. && mapped[2] < 1.);

    let mut perspective = Mesh::new(&Camera::default(), DVec2::new(640., 480.));
    perspective.vertices = mesh.vertices;
    perspective.normalize_parallel_depth();
    assert_eq!(
        perspective
            .vertices
            .iter()
            .map(|v| v.position[2])
            .collect::<Vec<_>>(),
        mapped
    );
}

#[test]
fn indexed_strokes_preserve_coverage_with_compact_vertices() {
    assert_eq!(std::mem::size_of::<Vertex>(), 28);
    assert_eq!(std::mem::offset_of!(Vertex, shadow_color), 20);
    assert_eq!(std::mem::offset_of!(Vertex, dash_phase), 24);
    assert_eq!(std::mem::offset_of!(Vertex, color), 12);
    assert_eq!(std::mem::offset_of!(Vertex, lateral), 16);
    let size = DVec2::new(1000., 800.);
    for shared in [false, true] {
        for angle in [0_f64, 0.3, std::f64::consts::FRAC_PI_4, 1.57, 2.4] {
            let mut mesh = Mesh::new(&Camera::default(), size);
            let a = DVec2::new(500., 400.);
            let b = a + DVec2::new(angle.cos(), angle.sin()) * 100.;
            let surfaces: &[EdgeSurface] = if shared {
                &[
                    EdgeSurface::unbounded(DVec2::new(0.002, 0.)),
                    EdgeSurface::unbounded(DVec2::new(-0.003, 0.)),
                ]
            } else {
                &[]
            };
            mesh.surface_stroke(a.extend(0.5), b.extend(0.5), 3., Color32::BLACK, surfaces);
            assert_eq!(mesh.vertices.len(), if shared { 6 } else { 4 });
            assert_eq!(mesh.indices.len(), if shared { 12 } else { 6 });
            let screen = |index: u32| {
                let p = mesh.vertices[index as usize].position;
                DVec2::new(
                    (f64::from(p[0]) + 1.) * size.x * 0.5,
                    (1. - f64::from(p[1])) * size.y * 0.5,
                )
            };
            let area: f64 = mesh
                .indices
                .chunks_exact(3)
                .map(|t| {
                    let [a, b, c] = [screen(t[0]), screen(t[1]), screen(t[2])];
                    let area = (b - a).perp_dot(c - a) * 0.5;
                    assert!(area > 0., "indexed triangles must preserve stroke winding");
                    area
                })
                .sum();
            let expected = 100. * (3. + angle.sin().abs() + angle.cos().abs());
            assert!(
                (area - expected).abs() < 0.01,
                "indexed strips must cover the full stroke"
            );
        }
    }
}

#[test]
fn mesh_buffers_are_reused_after_the_last_frame_reference_is_released() {
    let mut cache = SceneCache::default();
    let camera = Camera::default();
    let size = DVec2::new(1000., 800.);
    let mut mesh = Mesh::cached(&camera, size, &mut cache);
    mesh.vertices.reserve(1000);
    mesh.indices.reserve(1000);
    mesh.triangle_ranges.reserve(16);
    mesh.line(
        DVec3::new(10., 10., 0.5),
        DVec3::new(90., 10., 0.5),
        1.8,
        Color32::BLACK,
    );
    let vertices = mesh.vertices.as_ptr();
    let indices = mesh.indices.as_ptr();
    let callback = Arc::new(SceneCallback {
        shadow_range: mesh.indices.len() as u32..mesh.indices.len() as u32,
        vertices: mesh.vertices,
        indices: mesh.indices,
        triangle_ranges: mesh.triangle_ranges,
        buffer_pool: mesh.buffer_pool,
        gpu_pool: mesh.gpu_pool,
        gpu_buffers: OnceLock::new(),
        antialiasing: false,
        target_size: [64, 64],
        antialiased: OnceLock::new(),
    });
    let retained = Arc::clone(&callback);
    drop(callback);
    assert!(cache.buffers.lock().unwrap().is_empty());
    drop(retained);
    assert_eq!(cache.buffers.lock().unwrap().len(), 1);
    let next = Mesh::cached(&camera, size, &mut cache);
    assert_eq!(next.vertices.as_ptr(), vertices);
    assert_eq!(next.indices.as_ptr(), indices);
    assert!(
        next.vertices.is_empty() && next.indices.is_empty() && next.triangle_ranges.is_empty()
    );
    assert!(next.vertices.capacity() >= 1000 && next.indices.capacity() >= 1000);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "Requires a GPU adapter; verifies simultaneous viewport uploads and buffer recycling"]
fn simultaneous_viewports_own_distinct_gpu_buffers_until_released() {
    use egui_wgpu::CallbackTrait;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut renderer = egui_wgpu::Renderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        egui_wgpu::RendererOptions {
            depth_stencil_format: Some(wgpu::TextureFormat::Depth32Float),
            ..Default::default()
        },
    );
    install_resources(&device, wgpu::TextureFormat::Rgba8Unorm, &mut renderer);
    let gpu_pool = GpuBufferPool::default();
    let callback = || SceneCallback {
        shadow_range: 1..1,
        vertices: vec![Vertex {
            dash_phase: -1.,
            position: [0., 0., 0.5],
            color: [255; 4],
            lateral: 0.,
            shadow_color: [0; 4],
        }],
        indices: vec![0],
        triangle_ranges: vec![],
        buffer_pool: None,
        gpu_pool: gpu_pool.clone(),
        gpu_buffers: OnceLock::new(),
        antialiasing: false,
        target_size: [64, 64],
        antialiased: OnceLock::new(),
    };
    let first = callback();
    let second = callback();
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [1000, 700],
        pixels_per_point: 1.,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    first.prepare(
        &device,
        &queue,
        &screen,
        &mut encoder,
        &mut renderer.callback_resources,
    );
    second.prepare(
        &device,
        &queue,
        &screen,
        &mut encoder,
        &mut renderer.callback_resources,
    );
    let original = first.gpu_buffers.get().unwrap().vertices.clone();
    assert_ne!(original, second.gpu_buffers.get().unwrap().vertices);
    assert_ne!(
        first.gpu_buffers.get().unwrap().indices,
        second.gpu_buffers.get().unwrap().indices
    );
    assert!(gpu_pool.lock().unwrap().is_empty());
    drop(first);
    assert_eq!(gpu_pool.lock().unwrap().len(), 1);
    let third = callback();
    third.prepare(
        &device,
        &queue,
        &screen,
        &mut encoder,
        &mut renderer.callback_resources,
    );
    assert_eq!(original, third.gpu_buffers.get().unwrap().vertices);
    assert_ne!(
        third.gpu_buffers.get().unwrap().vertices,
        second.gpu_buffers.get().unwrap().vertices
    );
}

#[test]
fn face_batches_exclude_interleaved_strokes_and_clipped_faces() {
    let camera = Camera::default();
    let mut mesh = Mesh::new(&camera, DVec2::new(1000., 800.));
    let line = |mesh: &mut Mesh| {
        mesh.line(
            DVec3::new(50., 50., 0.5),
            DVec3::new(100., 100., 0.5),
            1.8,
            Color32::BLACK,
        )
    };
    let triangle = [DVec3::ZERO, DVec3::X, DVec3::Y];
    line(&mut mesh);
    mesh.world_triangle(&camera, triangle, Color32::WHITE);
    mesh.world_triangle(&camera, triangle, Color32::WHITE);
    line(&mut mesh);
    mesh.world_triangle(&camera, triangle, Color32::WHITE);
    mesh.set_display(&sketcraft_core::DisplaySettings {
        section_cuts: true,
        section_height: -1.,
        ..Default::default()
    });
    mesh.world_triangle(&camera, triangle, Color32::WHITE);
    assert_eq!(mesh.triangle_ranges, vec![6..12, 18..21]);
    assert_eq!(mesh.vertices.len(), 17);
    assert_eq!(mesh.indices.len(), 21);
}

#[test]
fn face_cache_reuses_meshes_and_tracks_geometry_holes_and_undo() {
    let mut model = Model::default();
    for (lo, hi) in [(0., 4.), (1., 3.)] {
        let points = [
            DVec3::new(lo, lo, 0.),
            DVec3::new(hi, lo, 0.),
            DVec3::new(hi, hi, 0.),
            DVec3::new(lo, hi, 0.),
        ];
        for i in 0..4 {
            model.add_edge(points[i], points[(i + 1) % 4]).unwrap();
        }
    }
    let original = model.clone();
    let outer = model
        .faces
        .values()
        .find(|f| !f.holes.is_empty())
        .unwrap()
        .id;
    let mut cache = FaceCache::default();
    cache.update(&model);
    let mesh = cache.get(outer).triangles.as_ptr();
    model.face_colors.insert(outer, [100, 120, 140]);
    model.hidden.insert(outer);
    cache.update(&model);
    assert_eq!(mesh, cache.get(outer).triangles.as_ptr());
    for vertex in model.vertices.values_mut() {
        vertex.position.x *= 2.;
    }
    cache.update(&model);
    assert_eq!(
        cache.get(outer).triangles,
        model.face_triangles(&model.faces[&outer])
    );
    assert_ne!(
        cache.get(outer).triangles,
        original.face_triangles(&original.faces[&outer])
    );
    model.faces.get_mut(&outer).unwrap().holes.clear();
    cache.update(&model);
    assert_eq!(
        cache.get(outer).triangles,
        model.face_triangles(&model.faces[&outer])
    );
    cache.update(&original);
    assert_eq!(
        cache.get(outer).triangles,
        original.face_triangles(&original.faces[&outer])
    );
    cache.update(&Model::default());
    assert!(cache.faces.is_empty());
}
