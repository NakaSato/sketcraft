//! Joint surfaces, cut guides and their annotation masks.
use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "Requires a GPU adapter; compares hybrid cut-guide clipping against the full f64 reference"]
fn gpu_cut_guides_match_full_cpu_clipping_for_foreground_faces_outlines_and_dash_gaps() {
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("edges.wgsl").into()),
    });
    let pipelines = [
        PipelinePass::Faces,
        PipelinePass::EdgeDepth,
        PipelinePass::Edges,
    ]
    .map(|pass| create_pipeline(&device, &shader, wgpu::TextureFormat::Rgba8Unorm, pass));
    let size = wgpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let render = |mesh: Mesh| {
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&pipelines[0]);
            for range in &mesh.triangle_ranges {
                pass.draw_indexed(range.clone(), 0, 0..1);
            }
            let edge_start = mesh.triangle_ranges.last().map_or(0, |range| range.end);
            for pipeline in &pipelines[1..] {
                pass.set_pipeline(pipeline);
                pass.draw_indexed(edge_start..mesh.indices.len() as u32, 0, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(64),
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
        receive.recv().unwrap().unwrap();
        let pixels = readback.slice(..).get_mapped_range().unwrap().to_vec();
        readback.unmap();
        pixels
    };
    for projection in [
        sketcraft_core::Projection::Parallel,
        sketcraft_core::Projection::Perspective,
    ] {
        let camera = Camera {
            projection,
            ..Default::default()
        };
        let viewport = DVec2::splat(64.);
        let inverse = camera.matrix(viewport).inverse();
        let world = |x: f64, y: f64, z: f64| {
            inverse.project_point3(DVec3::new(x / 32. - 1., 1. - y / 32., z))
        };
        for foreground in [0.2, 0.4 - 1e-9] {
            let build = |hybrid| {
                let mut mesh = Mesh::new(&camera, viewport);
                if hybrid {
                    mesh.enable_gpu_edge_occlusion();
                }
                let rectangle = |lo: DVec2, hi: DVec2, z| {
                    let p = [
                        world(lo.x, lo.y, z),
                        world(hi.x, lo.y, z),
                        world(hi.x, hi.y, z),
                        world(lo.x, hi.y, z),
                    ];
                    [[p[0], p[1], p[2]], [p[0], p[2], p[3]]]
                };
                let stock = rectangle(DVec2::splat(6.), DVec2::splat(58.), 0.4);
                for triangle in stock {
                    mesh.world_triangle(&camera, triangle, Color32::WHITE);
                }
                for triangle in
                    rectangle(DVec2::new(25., 10.), DVec2::new(39., 54.), foreground)
                {
                    mesh.world_triangle(&camera, triangle, Color32::GRAY);
                }
                mesh.styled_line(
                    world(46., 6., 0.3),
                    world(46., 58., 0.3),
                    4.,
                    Color32::BLACK,
                    false,
                    false,
                );
                mesh.styled_line(
                    world(10., 44., 0.3),
                    world(54., 44., 0.3),
                    2.,
                    Color32::BLUE,
                    false,
                    false,
                );
                mesh.styled_line(
                    world(10., 52., 0.5),
                    world(54., 52., 0.5),
                    2.,
                    Color32::BLACK,
                    false,
                    false,
                );
                let surface = if hybrid {
                    mesh.register_joint_guide(7, world(9., 28., 0.8), world(55., 28., 0.8));
                    mesh.register_joint_guide(7, world(12., 48., 0.8), world(52., 48., 0.8));
                    mesh.cached_joint_surface(7, &stock)
                } else {
                    Arc::new(mesh.joint_surface(stock))
                };
                mesh.joint_guide(
                    world(9., 28., 0.8),
                    world(55., 28., 0.8),
                    &surface,
                    2.,
                    Color32::from_rgb(0, 100, 92),
                    true,
                );
                mesh.joint_guide(
                    world(12., 48., 0.8),
                    world(54., 15., 0.8),
                    &surface,
                    2.,
                    Color32::from_rgb(0, 100, 92),
                    true,
                );
                mesh
            };
            let reference = render(build(false));
            let hybrid = render(build(true));
            assert!(
                reference.chunks_exact(4).any(|p| p[0] == 0 && p[1] == 100),
                "draw real dotted cut guides"
            );
            assert!(
                reference
                    .chunks_exact(4)
                    .zip(hybrid.chunks_exact(4))
                    .enumerate()
                    .all(|(i, (a, b))| {
                        a.iter().zip(b).all(|(a,b)| a.abs_diff(*b) <= 2)
                // The full CPU reference masks even the zero-opacity
                // boundary of the outline's antialias fringe. GPU depth
                // keeps a guide visible at that transparent boundary;
                // the actual outline core must still match exactly.
                || ((matches!(i % 64, 43 | 48) || matches!(i / 64, 42 | 45))
                    && a == [255,255,255,255] && b[0] < b[1] && b[1] >= 100)
                    }),
                "GPU foreground occlusion must match f64 clipping: {projection:?}, {foreground}; differences={:?}",
                reference
                    .chunks_exact(4)
                    .zip(hybrid.chunks_exact(4))
                    .enumerate()
                    .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
                    .map(|(i, (a, b))| (i % 64, i / 64, a, b))
                    .take(12)
                    .collect::<Vec<_>>()
            );
        }
    }
    // Back edges use the same shader dash mask, with the original fixed
    // four-pixel dash and gap rather than the guide's fitted end pattern.
    let camera = Camera::default();
    let size = DVec2::splat(64.);
    let inverse = camera.matrix(size).inverse();
    let world = |p: DVec2| inverse.project_point3(DVec3::new(p.x / 32. - 1., 1. - p.y / 32., 0.4));
    for (a, b) in [(DVec2::new(5.17, 15.3), DVec2::new(59.1, 15.3)),
        (DVec2::new(23.2, 4.1), DVec2::new(23.2, 59.7)),
        (DVec2::new(5.3, 8.2), DVec2::new(58.2, 49.1))] {
        let color = Color32::from_rgba_unmultiplied(60, 130, 80, 160);
        let mut reference = Mesh::new(&camera, size);
        let mut optimized = Mesh::new(&camera, size);
        let (a, b) = camera.view(size).segment(world(a), world(b)).unwrap();
        let length = a.distance(b).max(1.);
        for i in 0..(length / 8.).ceil() as usize {
            reference.line(a.lerp(b, (i as f64 * 8. / length).min(1.)).extend(0.),
                a.lerp(b, ((i as f64 * 8. + 4.) / length).min(1.)).extend(0.), 1., color);
        }
        optimized.overlay_line(world(a), world(b), 1., color, true);
        assert_eq!(optimized.vertices.len(), 4);
        let expected = render(reference);
        let actual = render(optimized);
        for (i, (expected, actual)) in expected.chunks_exact(4).zip(actual.chunks_exact(4)).enumerate() {
            let point = DVec2::new((i % 64) as f64 + 0.5, (i / 64) as f64 + 0.5);
            let phase = ((point - a).dot((b-a).normalize()) / 8.).rem_euclid(1.);
            assert!(expected.iter().zip(actual).all(|(a,b)| a.abs_diff(*b) <= 3)
                || phase.min(1.-phase).min((phase-0.5).abs()) < 0.005,
                "shader overlay dash must match subdivided reference at {point:?}: {expected:?}, {actual:?}");
        }
    }

}

#[test]
fn joint_precision_masks_ignore_own_entities_and_track_foreign_owner_changes() {
    let polygon = [
        DVec2::new(0., 0.),
        DVec2::new(100., 0.),
        DVec2::new(100., 100.),
        DVec2::new(0., 100.),
    ];
    let origin = DVec3::new(0., 0., 0.4);
    let mut occlusion = Occlusion::default();
    let own = std::collections::BTreeSet::from([10]);
    occlusion.begin_frame();
    occlusion.entity = Some(10);
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.4 - 1e-9);
    assert!(
        occlusion
            .precision_candidates(&polygon, origin, DVec2::ZERO, false, &own)
            .is_empty()
    );
    let revision = occlusion.revision;
    // Identical geometry assigned to a different piece must invalidate the
    // cached identity and remain an occluder, even inside a GPU depth step.
    occlusion.begin_frame();
    occlusion.entity = Some(20);
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.4 - 1e-9);
    assert_ne!(occlusion.revision, revision);
    let candidates = occlusion.precision_candidates(&polygon, origin, DVec2::ZERO, true, &own);
    assert_eq!(candidates.len(), 2);
    assert!(
        occlusion
            .visible_surface_candidates(polygon.to_vec(), origin, DVec2::ZERO, &candidates)
            .is_empty()
    );
}

#[test]
fn joint_surface_candidates_preserve_foreground_faces_and_outlines_and_refresh_after_edits() {
    for projection in [
        sketcraft_core::Projection::Parallel,
        sketcraft_core::Projection::Perspective,
    ] {
        let camera = Camera {
            projection,
            ..Default::default()
        };
        let size = DVec2::splat(100.);
        let inverse = camera.matrix(size).inverse();
        let world = |x: f64, y: f64, z: f64| {
            inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
        };
        let p = [
            world(10., 10., 0.4),
            world(90., 10., 0.4),
            world(90., 90., 0.4),
            world(10., 90., 0.4),
        ];
        let stock = [[p[0], p[1], p[2]], [p[0], p[2], p[3]]];
        let render = |prepared: bool, dashed: bool| {
            let mut mesh = Mesh::new(&camera, size);
            // Most geometry is behind the stock; one face and one outline
            // are in front, and must still mask the complete guide width.
            for i in 0..30 {
                occluding_rectangle(
                    &mut mesh.occlusion,
                    DVec2::new(i as f64, 0.),
                    DVec2::new(i as f64 + 20., 100.),
                    0.6,
                );
            }
            occluding_rectangle(
                &mut mesh.occlusion,
                DVec2::new(45., 0.),
                DVec2::new(55., 100.),
                0.4 - 1e-9,
            );
            mesh.line(
                DVec3::new(70., 0., 0.4 - 2e-9),
                DVec3::new(70., 100., 0.4 - 2e-9),
                4.,
                Color32::BLACK,
            );
            mesh.vertices.clear();
            mesh.indices.clear();
            let surface = if prepared {
                mesh.cached_joint_surface(7, &stock)
            } else {
                Arc::new(mesh.joint_surface(stock))
            };
            if prepared {
                assert!(Arc::ptr_eq(&surface, &mesh.cached_joint_surface(7, &stock)));
                let candidates = surface.occluders.as_ref().unwrap();
                assert!(
                    candidates.iter().all(|(faces, _)| faces.len() <= 2),
                    "background geometry must be rejected once for the board"
                );
            }
            for (a, b) in [
                (world(15., 35., 0.8), world(85., 65., 0.8)),
                (world(20., 80., 0.8), world(80., 20., 0.8)),
            ] {
                mesh.joint_guide(a, b, &surface, 3., Color32::WHITE, dashed);
            }
            mesh
        };
        for dashed in [false, true] {
            let original = render(false, dashed);
            let prepared = render(true, dashed);
            // Candidate filtering preserves the exact clipping output, not
            // just the centerline or an approximate visibility threshold.
            assert_eq!(
                bytemuck::cast_slice::<Vertex, u8>(&original.vertices),
                bytemuck::cast_slice::<Vertex, u8>(&prepared.vertices)
            );
            assert_eq!(original.indices, prepared.indices);
        }
        let mut mesh = Mesh::new(&camera, size);
        let before = mesh.cached_joint_surface(7, &stock);
        occluding_rectangle(&mut mesh.occlusion, DVec2::ZERO, size, 0.4 - 1e-9);
        let after = mesh.cached_joint_surface(7, &stock);
        assert!(
            !Arc::ptr_eq(&before, &after),
            "foreground edits invalidate candidates"
        );
        mesh.joint_guide(
            world(20., 50., 0.8),
            world(80., 50., 0.8),
            &after,
            2.,
            Color32::WHITE,
            true,
        );
        assert!(mesh.vertices.is_empty());
    }
}

#[test]
fn projected_joint_surfaces_reuse_stock_and_invalidate_on_view_geometry_and_sections() {
    let camera = Camera::default();
    let size = DVec2::new(640., 480.);
    let stock = [[DVec3::ZERO, DVec3::X, DVec3::Y]];
    let mut cache = SceneCache::default();
    let mut first_frame = Mesh::cached(&camera, size, &mut cache);
    let first = first_frame.projected_joint_surface(7, &stock);
    assert!(
        Arc::ptr_eq(&first, &first_frame.projected_joint_surface(7, &stock)),
        "joints sharing a board must reuse its projected skin"
    );
    first_frame.retain(&mut cache);

    let mut next_frame = Mesh::cached(&camera, size, &mut cache);
    next_frame.set_display(&Default::default());
    next_frame.set_pixels_per_point(2.);
    assert!(
        Arc::ptr_eq(&first, &next_frame.projected_joint_surface(7, &stock)),
        "hover redraws and pixel density must not rebuild the unchanged stock projection"
    );
    let moved_stock = stock.map(|triangle| triangle.map(|p| p + DVec3::Z * 0.1));
    let moved = next_frame.projected_joint_surface(7, &moved_stock);
    assert!(!Arc::ptr_eq(&first, &moved));
    let sections = sketcraft_core::DisplaySettings {
        section_cuts: true,
        section_axes: [false, false, true],
        section_height: 0.,
        ..Default::default()
    };
    next_frame.set_display(&sections);
    let sectioned = next_frame.projected_joint_surface(7, &moved_stock);
    assert!(!Arc::ptr_eq(&moved, &sectioned));
    assert!(sectioned.triangles.is_empty());
    next_frame.retain(&mut cache);

    let mut default_display = Mesh::cached(&camera, size, &mut cache);
    let unsectioned = default_display.projected_joint_surface(7, &moved_stock);
    assert!(!Arc::ptr_eq(&sectioned, &unsectioned));
    assert!(
        !unsectioned.triangles.is_empty(),
        "default display settings must not inherit stale section clipping"
    );
    default_display.retain(&mut cache);

    let mut orbit = camera.clone();
    orbit.yaw += 0.2;
    let mut orbit_frame = Mesh::cached(&orbit, size, &mut cache);
    orbit_frame.set_display(&sections);
    let rotated = orbit_frame.projected_joint_surface(7, &moved_stock);
    assert!(!Arc::ptr_eq(&sectioned, &rotated));
    orbit_frame.retain(&mut cache);
    let mut resized = Mesh::cached(&orbit, size * 2., &mut cache);
    resized.set_display(&sections);
    assert!(!Arc::ptr_eq(
        &rotated,
        &resized.projected_joint_surface(7, &moved_stock)
    ));
    resized.retain(&mut cache);
    Mesh::cached(&orbit, size * 2., &mut cache).retain(&mut cache);
    assert!(
        cache.joint_surfaces.entries.is_empty(),
        "unused boards must not accumulate in the cache"
    );
}

#[test]
fn disabling_unused_annotation_masks_preserves_strokes_selection_and_joint_geometry() {
    let camera = Camera::default();
    let size = DVec2::splat(100.);
    let inverse = camera.matrix(size).inverse();
    let world = |x: f64, y: f64, z: f64| {
        inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
    };
    let draw = |annotations| {
        let mut mesh = Mesh::new(&camera, size);
        mesh.set_annotation_occlusion_enabled(annotations);
        mesh.line(
            DVec3::new(50., 0., 0.3),
            DVec3::new(50., 100., 0.3),
            3.,
            Color32::BLACK,
        );
        mesh.begin_selected_edges();
        mesh.surface_stroke(
            DVec3::new(10., 50., 0.35),
            DVec3::new(90., 50., 0.35),
            6.,
            Color32::RED,
            &[],
        );
        let p = [
            world(0., 0., 0.25),
            world(100., 0., 0.25),
            world(100., 100., 0.25),
            world(0., 100., 0.25),
        ];
        let surface = mesh.joint_surface([[p[0], p[1], p[2]], [p[0], p[2], p[3]]]);
        mesh.joint_guide(
            world(10., 40., 0.8),
            world(90., 40., 0.8),
            &surface,
            1.8,
            Color32::WHITE,
            true,
        );
        mesh
    };
    let mut enabled = draw(true);
    let mut disabled = draw(false);
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&enabled.vertices),
        bytemuck::cast_slice::<Vertex, u8>(&disabled.vertices)
    );
    assert_eq!(enabled.indices, disabled.indices);
    assert!(!enabled.annotation_occluders.is_empty());
    assert!(disabled.annotation_occluders.is_empty());
    assert!(
        !disabled.selection_occlusion.triangles.is_empty(),
        "ordinary edge masking must remain active"
    );
    enabled.occlude_annotations_with_edges();
    disabled.occlude_annotations_with_edges();
    assert!(!enabled.occlusion.triangles.is_empty());
    assert!(
        disabled.occlusion.triangles.is_empty(),
        "an unannotated scene must not collect unused stroke triangles"
    );
}

#[test]
fn all_lower_cut_face_edges_render_as_dotted_xray_through_their_own_board() {
    let points = [
        DVec3::new(-1., -1., -1.),
        DVec3::new(1., -1., -1.),
        DVec3::new(1., 1., -1.),
        DVec3::new(-1., 1., -1.),
        DVec3::new(-1., -1., 1.),
        DVec3::new(1., -1., 1.),
        DVec3::new(1., 1., 1.),
        DVec3::new(-1., 1., 1.),
    ];
    let triangles: Vec<_> = [
        [0, 1, 2, 3],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ]
    .into_iter()
    .flat_map(|p| {
        [
            [points[p[0]], points[p[1]], points[p[2]]],
            [points[p[0]], points[p[2]], points[p[3]]],
        ]
    })
    .collect();
    let floor = [
        DVec3::new(-0.6, -0.4, -0.5),
        DVec3::new(0.6, -0.4, -0.5),
        DVec3::new(0.6, 0.4, -0.5),
        DVec3::new(-0.6, 0.4, -0.5),
    ];
    for projection in [
        sketcraft_core::Projection::Parallel,
        sketcraft_core::Projection::Perspective,
    ] {
        let camera = Camera {
            projection,
            ..Default::default()
        };
        let mut mesh = Mesh::new(&camera, DVec2::new(800., 600.));
        let surface = mesh.joint_surface(triangles.iter().copied());
        for triangle in &surface.triangles {
            mesh.occlusion.insert(triangle.points);
        }
        for i in 0..4 {
            let start = mesh.indices.len();
            mesh.joint_guide(
                floor[i],
                floor[(i + 1) % 4],
                &surface,
                1.8,
                Color32::WHITE,
                true,
            );
            assert!(
                mesh.indices.len() > start,
                "lower cut-face edge {i} disappeared in {projection:?}"
            );
        }
        // A connector may share an exterior side plane without becoming
        // an ordinary stock rim. Its recessed contour must remain visible.
        let start = mesh.indices.len();
        mesh.joint_guide(
            DVec3::new(1., -0.4, -0.5),
            DVec3::new(1., 0.4, -0.5),
            &surface,
            1.8,
            Color32::WHITE,
            true,
        );
        assert!(
            mesh.indices.len() > start,
            "coplanarity with an outer face must not suppress cut edges"
        );
    }
}

#[test]
fn joint_visibility_work_does_not_scale_with_dash_count() {
    let camera = Camera::default();
    let size = DVec2::new(1024., 512.);
    let inverse = camera.matrix(size).inverse();
    let world = |x: f64, y: f64, z: f64| {
        inverse.project_point3(DVec3::new(x / size.x * 2. - 1., 1. - y / size.y * 2., z))
    };
    let render = |dashed| {
        let mut mesh = Mesh::new(&camera, size);
        let p = [
            world(0., 0., 0.4),
            world(size.x, 0., 0.4),
            world(size.x, size.y, 0.4),
            world(0., size.y, 0.4),
        ];
        let mut surface = mesh.joint_surface([[p[0], p[1], p[2]], [p[0], p[2], p[3]]]);
        occluding_rectangle(
            &mut mesh.occlusion,
            DVec2::new(400., 0.),
            DVec2::new(600., size.y),
            0.2,
        );
        mesh.joint_guide(
            world(10., 256., 0.8),
            world(1010., 256., 0.8),
            &mut surface,
            1.8,
            Color32::WHITE,
            dashed,
        );
        let work = (
            surface.preparation_queries,
            mesh.occlusion.polygon_queries,
            mesh.selection_occlusion.polygon_queries,
        );
        (mesh, work)
    };
    let (solid, solid_work) = render(false);
    let (dashed, dashed_work) = render(true);
    assert!(solid_work.1 > 0);
    assert_eq!(
        dashed_work, solid_work,
        "64 dashes must use the same visibility work as a continuous guide"
    );
    assert_eq!(
        dashed.indices, solid.indices,
        "dashes must not multiply GPU triangles"
    );
    assert_eq!(dashed.vertices.len(), solid.vertices.len());
    assert!(dashed.vertices.iter().all(|v| v.dash_phase >= 0.));
    let covers = |x: f64| {
        let point = DVec2::new(x / size.x * 2. - 1., 0.);
        dashed.indices.chunks_exact(3).any(|indices| {
            let p = [indices[0], indices[1], indices[2]].map(|i| {
                let p = dashed.vertices[i as usize].position;
                DVec2::new(f64::from(p[0]), f64::from(p[1]))
            });
            let denominator = (p[1] - p[0]).perp_dot(p[2] - p[0]);
            let winding = denominator.signum();
            let inside = winding != 0.
                && [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])]
                    .into_iter()
                    .all(|(a, b)| winding * (b - a).perp_dot(point - a) >= -1e-10);
            if !inside {
                return false;
            }
            let u = (point - p[0]).perp_dot(p[2] - p[0]) / denominator;
            let v = (p[1] - p[0]).perp_dot(point - p[0]) / denominator;
            let phase = [indices[0], indices[1], indices[2]]
                .map(|i| f64::from(dashed.vertices[i as usize].dash_phase));
            let phase = phase[0] + u * (phase[1] - phase[0]) + v * (phase[2] - phase[0]);
            phase < 0. || phase.rem_euclid(1.) <= 0.5
        })
    };
    for i in 0..64 {
        let dash_center = 10. + (i as f64 + 0.25) / 63.5 * 1000.;
        assert_eq!(
            covers(dash_center),
            !(400. ..600.).contains(&dash_center),
            "visible dashes must retain their phase and foreground masking"
        );
        if i < 63 {
            let gap_center = 10. + (i as f64 + 0.75) / 63.5 * 1000.;
            assert!(
                !covers(gap_center),
                "dash gaps must remain empty after clipping"
            );
        }
    }
}

#[test]
fn dashed_cut_guides_reach_both_endpoints_including_short_connectors() {
    let camera = Camera::default();
    let size = DVec2::splat(100.);
    let inverse = camera.matrix(size).inverse();
    let world = |x: f64, y: f64, z: f64| {
        inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
    };
    for length in [1., 4., 8., 13., 31., 70.] {
        let mut mesh = Mesh::new(&camera, size);
        let p = [
            world(0., 0., 0.4),
            world(100., 0., 0.4),
            world(100., 100., 0.4),
            world(0., 100., 0.4),
        ];
        let mut surface = mesh.joint_surface([[p[0], p[1], p[2]], [p[0], p[2], p[3]]]);
        mesh.joint_guide(
            world(10., 50., 0.8),
            world(10. + length, 50., 0.8),
            &mut surface,
            1.8,
            Color32::WHITE,
            true,
        );
        let xs: Vec<_> = mesh
            .vertices
            .iter()
            .map(|v| (v.position[0] + 1.) * 50.)
            .collect();
        assert!(!xs.is_empty());
        let lo = xs.iter().copied().reduce(f32::min).unwrap();
        let hi = xs.iter().copied().reduce(f32::max).unwrap();
        assert!((lo - 10.).abs() < 1e-4);
        assert!(
            (f64::from(hi) - 10. - length).abs() < 1e-4,
            "the last dash must reach the next contour segment"
        );
    }
}

#[test]
fn internal_joint_guides_project_to_the_owner_skin_and_hide_only_behind_nearer_faces() {
    for projection in [
        sketcraft_core::Projection::Parallel,
        sketcraft_core::Projection::Perspective,
    ] {
        let mut camera = Camera::default();
        camera.projection = projection;
        let size = DVec2::splat(100.);
        let inverse = camera.matrix(size).inverse();
        let world = |x: f64, y: f64, z: f64| {
            inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
        };
        let face = [
            world(10., 10., 0.6),
            world(90., 10., 0.6),
            world(50., 90., 0.6),
        ];
        for dashed in [false, true] {
            for alpha in [230, 255] {
                for occluder in [None, Some(0.5), Some(0.6), Some(0.7)] {
                    let mut mesh = Mesh::new(&camera, size);
                    if let Some(depth) = occluder {
                        occluding_rectangle(&mut mesh.occlusion, DVec2::ZERO, size, depth);
                    }
                    let back = [
                        world(10., 10., 0.85),
                        world(90., 10., 0.85),
                        world(50., 90., 0.85),
                    ];
                    let mut surface = mesh.joint_surface([face, back]);
                    for triangle in &surface.triangles {
                        mesh.occlusion.insert(triangle.points);
                    }
                    mesh.joint_guide(
                        world(30., 50., 0.8),
                        world(70., 50., 0.8),
                        &mut surface,
                        1.5,
                        Color32::from_rgba_unmultiplied(0, 100, 92, alpha),
                        dashed,
                    );
                    if occluder == Some(0.5) {
                        assert!(
                            mesh.vertices.is_empty(),
                            "hidden guides must disappear completely"
                        );
                    } else {
                        assert!(!mesh.vertices.is_empty());
                        assert!(
                            mesh.vertices
                                .iter()
                                .all(|v| (v.position[2] - 0.6).abs() < 1e-5),
                            "projection={projection:?}, dashed={dashed}, occluder={occluder:?}: depths={:?}",
                            mesh.vertices
                                .iter()
                                .map(|v| v.position[2])
                                .collect::<Vec<_>>()
                        );
                        assert!(mesh.vertices.iter().all(|v| v.color[3] == alpha));
                        mesh.normalize_parallel_depth();
                        assert!(mesh.vertices.iter().all(|v| v.position[2] > 0.));
                    }
                }
            }
        }
    }
}

#[test]
fn joint_guides_follow_sloping_faces_and_clip_the_full_width_at_foreground_edges() {
    let camera = Camera::default();
    let size = DVec2::splat(100.);
    let inverse = camera.matrix(size).inverse();
    let world = |x: f64, y: f64, z: f64| {
        inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
    };
    let projected = [
        DVec3::new(10., 10., 0.42),
        DVec3::new(90., 10., 0.42),
        DVec3::new(50., 90., 0.58),
    ];
    for dashed in [false, true] {
        let mut mesh = Mesh::new(&camera, size);
        mesh.occlusion.insert(projected);
        occluding_rectangle(&mut mesh.occlusion, DVec2::new(50., 0.), size, 0.3);
        let mut surface = mesh.joint_surface([projected.map(|p| world(p.x, p.y, p.z))]);
        mesh.joint_guide(
            world(40., 30., 0.8),
            world(60., 70., 0.8),
            &mut surface,
            6.,
            Color32::WHITE,
            dashed,
        );
        assert!(!mesh.vertices.is_empty());
        assert!(
            !mesh.clip_stroke_footprints,
            "restore the caller's clipping state"
        );
        for vertex in &mesh.vertices {
            let [x, y, z] = vertex.position;
            assert!(
                x <= 1e-5,
                "the full guide width must stop at the foreground edge"
            );
            let screen_y = (1. - y) * 50.;
            assert!(
                (z - (0.4 + screen_y * 0.002)).abs() < 1e-5,
                "the stroke width must follow its supporting surface depth"
            );
        }
    }
}

#[test]
fn stock_supported_cut_contours_cross_their_own_notches_but_hide_behind_other_pieces() {
    let camera = Camera::default();
    let size = DVec2::splat(100.);
    let inverse = camera.matrix(size).inverse();
    let world = |x: f64, y: f64, z: f64| {
        inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
    };
    for foreground in [false, true] {
        let mut mesh = Mesh::new(&camera, size);
        // The evaluated board has a notch in the middle of this contour.
        for (lo, hi) in [(20., 45.), (55., 80.)] {
            occluding_rectangle(
                &mut mesh.occlusion,
                DVec2::new(lo, 20.),
                DVec2::new(hi, 80.),
                0.6,
            );
        }
        if foreground {
            occluding_rectangle(
                &mut mesh.occlusion,
                DVec2::new(48., 20.),
                DVec2::new(52., 80.),
                0.2,
            );
        }
        let p = [
            world(20., 20., 0.4),
            world(80., 20., 0.4),
            world(80., 80., 0.4),
            world(20., 80., 0.4),
        ];
        let mut surface = mesh.joint_surface([[p[0], p[1], p[2]], [p[0], p[2], p[3]]]);
        mesh.joint_guide(
            world(30., 50., 0.8),
            world(70., 50., 0.8),
            &mut surface,
            1.8,
            Color32::WHITE,
            false,
        );
        let center = DVec2::new(0., 0.);
        let covers_center = mesh.indices.chunks_exact(3).any(|indices| {
            let p = [indices[0], indices[1], indices[2]].map(|i| {
                let p = mesh.vertices[i as usize].position;
                DVec2::new(f64::from(p[0]), f64::from(p[1]))
            });
            let winding = (p[1] - p[0]).perp_dot(p[2] - p[0]).signum();
            [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])]
                .into_iter()
                .all(|(a, b)| winding * (b - a).perp_dot(center - a) >= -1e-10)
        });
        assert_eq!(
            covers_center, !foreground,
            "a notch must not interrupt its own contour, while another foreground piece must"
        );
    }
}

#[test]
fn projected_joint_guides_stay_inside_their_owner_silhouette_and_leave_gaps_clear() {
    let camera = Camera::default();
    let size = DVec2::splat(100.);
    let inverse = camera.matrix(size).inverse();
    let world = |x: f64, y: f64, z: f64| {
        inverse.project_point3(DVec3::new(x / 50. - 1., 1. - y / 50., z))
    };
    for dashed in [false, true] {
        let mut mesh = Mesh::new(&camera, size);
        let mut triangles = Vec::new();
        // Two disconnected visible patches leave an opening in the owner.
        for (lo, hi) in [(20., 45.), (55., 80.)] {
            let p = [
                world(lo, 40., 0.4),
                world(hi, 40., 0.4),
                world(hi, 60., 0.4),
                world(lo, 60., 0.4),
            ];
            triangles.extend([[p[0], p[1], p[2]], [p[0], p[2], p[3]]]);
        }
        let mut surface = mesh.joint_surface(triangles);
        // Geometry behind the exterior but ahead of the internal cut must
        // not hide a guide projected onto the exterior.
        occluding_rectangle(&mut mesh.occlusion, DVec2::ZERO, size, 0.6);
        mesh.joint_guide(
            world(0., 50., 0.8),
            world(100., 50., 0.8),
            &mut surface,
            30.,
            Color32::WHITE,
            dashed,
        );
        assert!(!mesh.indices.is_empty());
        for triangle in mesh.indices.chunks_exact(3) {
            let points = triangle
                .iter()
                .map(|&i| mesh.vertices[i as usize].position)
                .map(|p| [(p[0] + 1.) * 50., (1. - p[1]) * 50., p[2]])
                .collect::<Vec<_>>();
            assert!(points.iter().all(|p| p[0] >= 20. - 1e-4
                && p[0] <= 80. + 1e-4
                && p[1] >= 40. - 1e-4
                && p[1] <= 60. + 1e-4
                && (p[2] - 0.4).abs() < 1e-5));
            assert!(
                points.iter().all(|p| p[0] <= 45. + 1e-4)
                    || points.iter().all(|p| p[0] >= 55. - 1e-4),
                "guides must not bridge the owner's opening"
            );
        }
    }
}
