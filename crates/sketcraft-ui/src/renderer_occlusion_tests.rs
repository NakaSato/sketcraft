//! Edge, selection, annotation and label occlusion against faces.
use super::*;

#[test]
fn annotations_share_outline_masks_without_changing_line_or_label_visibility() {
    let draw = |shared| {
        let mut mesh = Mesh::new(&Camera::default(), DVec2::splat(100.));
        if shared {
            mesh.enable_gpu_edge_occlusion();
        }
        mesh.line(
            DVec3::new(50., 0., 0.3),
            DVec3::new(50., 100., 0.3),
            3.,
            Color32::BLACK,
        );
        mesh.begin_selected_edges();
        mesh.surface_stroke(
            DVec3::new(0., 65., 0.4),
            DVec3::new(100., 65., 0.4),
            4.,
            Color32::RED,
            &[],
        );
        mesh.occlude_annotations_with_edges();
        mesh
    };
    let mut reference = draw(false);
    let mut shared = draw(true);
    assert!(
        shared.occlusion.triangles.len() < reference.occlusion.triangles.len(),
        "do not duplicate ordinary outlines into the face hierarchy"
    );
    for depth in [0.2, 0.3 + 1e-10, 0.6] {
        let (a, b) = (DVec3::new(10., 50., depth), DVec3::new(90., 50., depth));
        let expected = reference.visible_annotation_line(a, b);
        let actual = shared.visible_annotation_line(a, b);
        assert_eq!(expected.len(), actual.len());
        for ((a, b), (c, d)) in expected.into_iter().zip(actual) {
            assert!(a.distance(c) < 1e-8 && b.distance(d) < 1e-8);
        }
        let polygon = vec![
            DVec2::new(20., 20.),
            DVec2::new(80., 20.),
            DVec2::new(80., 80.),
            DVec2::new(20., 80.),
        ];
        let expected = reference.visible_annotation_polygons(polygon.clone(), a, DVec2::ZERO);
        let actual = shared.visible_annotation_polygons(polygon, a, DVec2::ZERO);
        assert!((polygon_area(&expected) - polygon_area(&actual)).abs() < 1e-7);
    }
}

#[test]
fn gpu_edge_visibility_preserves_precision_and_invalidates_cached_intervals_when_modes_change()
{
    let mut occlusion = Occlusion::default();
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.3);
    let (a, b) = (DVec3::new(10., 50., 0.5), DVec3::new(90., 50., 0.5));
    assert!(occlusion.visible_intervals(a, b).is_empty());
    occlusion.line_cursor = 0;
    let visible = occlusion.visible_intervals_with_guard(a, b, Some(1e-6));
    assert_eq!(
        visible,
        vec![(0., 1.)],
        "distant faces are resolved by GPU depth"
    );
    occlusion.line_cursor = 0;
    assert!(
        occlusion.visible_intervals(a, b).is_empty(),
        "selected edges retain f64 clipping"
    );
    occlusion.line_cursor = 0;
    assert!(
        occlusion
            .visible_intervals_with_guard(a, b, Some(0.3))
            .is_empty(),
        "a steep stroke footprint still requires CPU clipping"
    );
    let (a, b) = (
        DVec3::new(10., 50., 0.3 + 1e-9),
        DVec3::new(90., 50., 0.3 + 1e-9),
    );
    assert!(
        occlusion
            .visible_intervals_with_guard(a, b, Some(1e-6))
            .is_empty(),
        "foreground differences lost in GPU depth must still hide the edge"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "Requires a GPU adapter; verifies incident face depth outside its silhouette"]
fn rear_face_boundary_does_not_hide_foreground_selection_outside_the_face() {
    use wgpu::util::DeviceExt;
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("edges.wgsl").into()),
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipelines = [PipelinePass::EdgeDepth, PipelinePass::Edges]
        .map(|pass| create_pipeline(&device, &shader, format, pass));
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
        format,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());

    let camera = Camera::default();
    let viewport = DVec2::splat(64.);
    let inverse = camera.matrix(viewport).inverse();
    let unproject = |p: DVec3| {
        let q = inverse * glam::DVec4::new(p.x / 32. - 1., 1. - p.y / 32., p.z, 1.);
        q.truncate() / q.w
    };
    // A rear triangle exists only to the RIGHT of its vertical boundary.
    // Its depth slopes toward the eye if extrapolated LEFT, outside the face.
    let points = [
        DVec3::new(32., 8., 0.6),
        DVec3::new(34., 8., 0.9),
        DVec3::new(32., 56., 0.6),
    ]
    .map(unproject);
    let mut model = Model::default();
    let boundary = model.add_edge(points[0], points[2]).unwrap();
    model.add_edge(points[2], points[1]).unwrap();
    model.add_edge(points[1], points[0]).unwrap();
    let mut faces = FaceCache::default();
    faces.update(&model);
    let mut mesh = Mesh::new(&camera, viewport);
    for face in model.faces.values() {
        mesh.face_edges(face, faces.get(face.id));
        for &triangle in &faces.get(face.id).triangles {
            mesh.world_triangle(&camera, triangle, Color32::WHITE);
        }
    }
    assert_eq!(model.faces.len(), 1, "exercise a real incident face");
    mesh.styled_edge(
        &model.edges[&boundary],
        &model,
        1.8,
        Color32::BLACK,
        false,
        false,
    );
    mesh.begin_selected_edges();
    mesh.line(
        DVec3::new(8., 32., 0.55),
        DVec3::new(56., 32., 0.55),
        3.,
        Color32::RED,
    );
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
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
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
        let faces = create_pipeline(&device, &shader, format, PipelinePass::Faces);
        pass.set_pipeline(&faces);
        for range in &mesh.triangle_ranges {
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
        let edge_start = mesh.triangle_ranges.last().map_or(0, |range| range.end);
        for pipeline in &pipelines {
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
    let mapped = readback.slice(..).get_mapped_range().unwrap();

    let sample = &mapped[(32 * 256 + 31 * 4)..(32 * 256 + 31 * 4 + 4)];
    assert_eq!(
        sample,
        [255, 0, 0, 255],
        "rear boundary must not advance beyond its face silhouette"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "Requires a GPU adapter; verifies edge occlusion and overlay pixels"]
fn crossing_selection_edges_respect_depth_in_both_draw_orders() {
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("edges.wgsl").into()),
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipelines = [PipelinePass::EdgeDepth, PipelinePass::Edges]
        .map(|pass| create_pipeline(&device, &shader, format, pass));
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
        format,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let color_view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    for (reverse, hybrid) in [false, true].into_iter().flat_map(|reverse| [false, true].map(|hybrid| (reverse, hybrid))) {
        // Depth zero is used by intentional x-ray/wireframe overlays.
        for rear_depth in [0.7, 0.] {
            let mut mesh = Mesh::new(&Camera::default(), DVec2::splat(64.));
            if hybrid { mesh.enable_gpu_edge_occlusion(); }
            let strokes = [
                (
                    DVec3::new(8., 32., 0.3),
                    DVec3::new(56., 32., 0.3),
                    1.8,
                    Color32::BLACK,
                ),
                (
                    DVec3::new(32., 8., rear_depth),
                    DVec3::new(32., 56., rear_depth),
                    3.,
                    Color32::RED,
                ),
            ];
            for index in if reverse { [1, 0] } else { [0, 1] } {
                let (a, b, width, color) = strokes[index];
                mesh.line(a, b, width, color);
            }
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
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256 * 64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
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
                for pipeline in &pipelines {
                    pass.set_pipeline(pipeline);
                    pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
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
            let mapped = readback.slice(..).get_mapped_range().unwrap();
            let pixel = |x: usize, y: usize| &mapped[(y * 256 + x * 4)..(y * 256 + x * 4 + 4)];
            if rear_depth > 0. {
                let crossing = pixel(32, 32);
                assert!(crossing[0] < 64, "near edge wins, reverse={reverse}");
                assert_eq!(crossing[0], crossing[1]);
                assert_eq!(crossing[1], crossing[2]);
            } else if !reverse {
                assert_eq!(pixel(32, 32), [255, 0, 0, 255], "overlay stays visible");
            }
            assert_eq!(
                pixel(32, 16),
                [255, 0, 0, 255],
                "selection color is preserved"
            );
            assert!(pixel(33, 16)[1] > 0, "antialiased fringe still blends");
        }
    }
}

#[test]
fn wide_diagonal_selection_clipping_respects_depth_across_its_entire_footprint() {
    for rear_depth in [0.6, 0.3, 0.2] {
        let mut mesh = Mesh::new(&Camera::default(), DVec2::splat(100.));
        mesh.line(
            DVec3::new(50., 0., 0.3),
            DVec3::new(50., 100., 0.3),
            2.,
            Color32::WHITE,
        );
        mesh.begin_selected_edges();
        let start = mesh.indices.len();
        mesh.surface_stroke(
            DVec3::new(20., 20., rear_depth),
            DVec3::new(80., 80., rear_depth),
            6.,
            Color32::RED,
            &[],
        );
        assert!(mesh.indices.len() > start);
        let mut crosses_ordinary_outline = false;
        for triangle in mesh.indices[start..].chunks_exact(3) {
            let xs = [triangle[0], triangle[1], triangle[2]]
                .map(|i| (f64::from(mesh.vertices[i as usize].position[0]) + 1.) * 50.);
            let crosses =
                xs.iter().any(|&x| x < 48.5 - 1e-5) && xs.iter().any(|&x| x > 51.5 + 1e-5);
            crosses_ordinary_outline |= crosses;
            if rear_depth > 0.3 {
                assert!(
                    xs.iter().all(|&x| x <= 48.5 + 1e-5)
                        || xs.iter().all(|&x| x >= 51.5 - 1e-5),
                    "rear selection triangle crosses the foreground stroke: {xs:?}"
                );
            }
        }
        if rear_depth <= 0.3 {
            assert!(
                crosses_ordinary_outline,
                "visible selection was clipped at depth {rear_depth}"
            );
        }
    }
}

#[test]
fn edge_occlusion_preserves_depth_differences_lost_by_gpu_rounding() {
    let mut mesh = Mesh::new(&Camera::default(), DVec2::splat(100.));
    let near = 0.5_f64;
    let rear = near + 1e-10;
    assert_eq!(near as f32, rear as f32);
    mesh.line(
        DVec3::new(50., 10., near),
        DVec3::new(50., 90., near),
        1.8,
        Color32::BLACK,
    );
    mesh.occlude_annotations_with_edges();
    let spans = mesh
        .occlusion
        .visible_intervals(DVec3::new(10., 50., rear), DVec3::new(90., 50., rear));
    assert_eq!(spans.len(), 2);
    assert!(spans[0].1 < 0.5 && spans[1].0 > 0.5);
}

#[test]
fn foreground_edges_clip_annotation_strokes_and_label_footprints() {
    let mut mesh = Mesh::new(&Camera::default(), DVec2::splat(100.));
    mesh.line(
        DVec3::new(50., 10., 0.3),
        DVec3::new(50., 90., 0.3),
        2.,
        Color32::BLACK,
    );
    mesh.occlude_annotations_with_edges();
    let a = DVec3::new(10., 50., 0.6);
    let b = DVec3::new(90., 50., 0.6);
    let lines = mesh.visible_annotation_line(a, b);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].1.x < 49. && lines[1].0.x > 51.);
    assert_eq!(
        mesh.visible_annotation_line(a.truncate().extend(0.2), b.truncate().extend(0.2)),
        vec![(a.truncate().extend(0.2), b.truncate().extend(0.2))]
    );
    let polygon = vec![
        DVec2::new(40., 40.),
        DVec2::new(60., 40.),
        DVec2::new(60., 60.),
        DVec2::new(40., 60.),
    ];
    let visible = mesh.occlusion.visible_polygons(polygon, 0.6);
    assert!((polygon_area(&visible) - 340.).abs() < 1e-7);
}

#[test]
fn dimension_lines_and_ticks_hide_only_behind_foreground_faces() {
    for projection in [
        ketchup_core::Projection::Perspective,
        ketchup_core::Projection::Parallel,
    ] {
        let camera = Camera {
            projection,
            ..Camera::default()
        };
        let size = DVec2::new(1000., 800.);
        let a = camera.target - camera.right() * 2.;
        let b = camera.target + camera.right() * 2.;
        let mut mesh = Mesh::new(&camera, size);
        let center = mesh.view.project_depth(camera.target).unwrap();
        let pa = mesh.view.project(a).unwrap();
        let pb = mesh.view.project(b).unwrap();
        let lo = center.truncate() - DVec2::new(10., 20.);
        let hi = center.truncate() + DVec2::new(10., 20.);
        occluding_rectangle(&mut mesh.occlusion, lo, hi, center.z - 0.001);
        let lines = mesh.annotation_line(a, b);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].0.truncate().distance(pa) < 1e-8);
        assert!((lines[0].1.x - lo.x).abs() < 1e-8);
        assert!((lines[1].0.x - hi.x).abs() < 1e-8);
        assert!(lines[1].1.truncate().distance(pb) < 1e-8);
        assert!(
            mesh.annotation_tick(camera.target, DVec2::new(3., 4.))
                .is_empty()
        );

        // Moving the covering face behind the dimension restores every part.
        mesh.occlusion = Occlusion::default();
        occluding_rectangle(&mut mesh.occlusion, lo, hi, center.z + 0.001);
        let lines = mesh.annotation_line(a, b);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].0.truncate().distance(pa) < 1e-8
                && lines[0].1.truncate().distance(pb) < 1e-8
        );
        assert_eq!(
            mesh.annotation_tick(camera.target, DVec2::new(3., 4.))
                .len(),
            1
        );
        mesh.display.section_cuts = true;
        mesh.display.section_height = -1.;
        assert!(mesh.annotation_line(a, b).is_empty());
        assert!(
            mesh.annotation_tick(camera.target, DVec2::new(3., 4.))
                .is_empty()
        );
    }
}

#[test]
fn dimension_stroke_width_and_antialiasing_do_not_bleed_across_silhouettes() {
    for pixels_per_point in [1., 2., 2.5] {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(pixels_per_point);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let viewport = Rect::from_min_size(egui::pos2(100., 50.), egui::vec2(100., 100.));
            // The first centerline is wholly outside the face. Its width
            // crosses the silhouette; the second stroke's end cap crosses it.
            for (a, b, face_lo, face_depth) in [
                (DVec3::new(49.75, 20., 0.5), DVec3::new(49.75, 80., 0.5), DVec2::new(50., 0.), 0.4),
                (DVec3::new(10., 50., 0.5), DVec3::new(50., 50., 0.5), DVec2::new(50., 0.), 0.4),
                // A perspective stroke's interpolated depth crosses the face.
                (DVec3::new(20., 50., 0.3), DVec3::new(80., 50., 0.7), DVec2::ZERO, 0.5),
            ] {
                let mut mesh = Mesh::new(&Camera::default(), DVec2::splat(100.));
                occluding_rectangle(&mut mesh.occlusion, face_lo, DVec2::splat(100.), face_depth);
                let shapes = mesh.annotation_stroke(ui.painter(), viewport, a, b, egui::Stroke::new(1.5, Color32::WHITE));
                assert!(!shapes.is_empty());
                let mut triangle_count = 0;
                for shape in shapes {
                    let egui::Shape::Mesh(stroke) = shape else { panic!("a partially covered stroke must clip its full mesh") };
                    triangle_count += stroke.indices.len() / 3;
                    assert!(stroke.vertices.iter().all(|v| v.pos.x <= viewport.min.x + 50.0001), "stroke width or antialiasing leaked behind the face at scale {pixels_per_point}: {a:?} to {b:?}, vertices {:?}", stroke.vertices);
                }
                assert!(triangle_count > 0, "preserve the exposed part of the stroke");
                mesh.occlusion = Occlusion::default();
                occluding_rectangle(&mut mesh.occlusion, face_lo, DVec2::splat(100.), 0.9);
                let shapes = mesh.annotation_stroke(ui.painter(), viewport, a, b, egui::Stroke::new(1.5, Color32::WHITE));
                assert!(matches!(shapes.as_slice(), [egui::Shape::LineSegment { .. }]));
            }
        });
        output.textures_delta.clear();
    }
}

#[test]
fn coplanar_occluders_do_not_fragment_visible_strokes() {
    let mut occlusion = Occlusion::default();
    let plane = |x, y| DVec3::new(x, y, 0.4 + x * 0.001);
    for i in 0..128 {
        let offset = f64::from(i) * 0.1;
        occlusion.insert([
            plane(0., offset),
            plane(100., offset),
            plane(50., 100. + offset),
        ]);
    }
    let polygon = vec![
        DVec2::new(40., 40.),
        DVec2::new(60., 40.),
        DVec2::new(60., 60.),
        DVec2::new(40., 60.),
    ];
    let visible = occlusion.visible_surface_polygons(
        polygon.clone(),
        plane(0., 0.),
        DVec2::new(0.001, 0.),
    );
    assert_eq!(visible, vec![polygon]);
}

#[test]
fn dimension_label_footprints_follow_silhouettes_and_sloping_depth() {
    let polygon = vec![
        DVec2::new(10., 10.),
        DVec2::new(90., 10.),
        DVec2::new(90., 90.),
        DVec2::new(10., 90.),
    ];
    for reversed in [false, true] {
        for (near, far, area) in [
            (0.4, 0.4, 0.),
            (0.6, 0.6, 6400.),
            (0.5, 0.5, 6400.),
            (0.4, 0.6, 3200.),
        ] {
            let mut occlusion = Occlusion::default();
            let points = [
                DVec3::new(0., 0., near),
                DVec3::new(100., 0., far),
                DVec3::new(100., 100., far),
                DVec3::new(0., 100., near),
            ];
            for mut triangle in [
                [points[0], points[1], points[2]],
                [points[0], points[2], points[3]],
            ] {
                if reversed {
                    triangle.swap(0, 2);
                }
                occlusion.insert(triangle);
            }
            let visible = occlusion.visible_polygons(polygon.clone(), 0.5);
            assert!((polygon_area(&visible) - area).abs() < 1e-7);
        }
    }
    let mut occlusion = Occlusion::default();
    occluding_rectangle(
        &mut occlusion,
        DVec2::new(40., 0.),
        DVec2::new(60., 100.),
        0.4,
    );
    assert!(
        (polygon_area(&occlusion.visible_polygons(polygon.clone(), 0.5)) - 4800.).abs() < 1e-7
    );
    // Hidden or removed faces must stop clipping labels on the next frame.
    occlusion.begin_frame();
    assert_eq!(
        occlusion.visible_polygons(polygon.clone(), 0.5),
        vec![polygon]
    );
}

#[test]
fn partially_occluded_labels_preserve_texture_coordinates_and_viewport_offset() {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let viewport = Rect::from_min_size(egui::pos2(100., 50.), egui::vec2(1000., 800.));
        let mut mesh = Mesh::new(&Camera::default(), DVec2::new(1000., 800.));
        let anchor = DVec3::ZERO;
        let center = mesh.view.project_depth(anchor).unwrap();
        let at = viewport.min + egui::vec2(center.x as f32, center.y as f32);
        let bounds = Rect::from_center_size(at, egui::vec2(80., 20.));
        let mut label = egui::Mesh::default();
        label.add_rect_with_uv(
            bounds,
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
            Color32::WHITE,
        );
        occluding_rectangle(
            &mut mesh.occlusion,
            center.truncate() - DVec2::new(100., 100.),
            center.truncate() + DVec2::new(0., 100.),
            center.z - 0.001,
        );
        let shapes = mesh.annotation_label(
            ui.painter(),
            viewport,
            anchor,
            bounds,
            egui::Shape::mesh(label),
        );
        assert!(!shapes.is_empty());
        let mut area = 0.;
        for shape in shapes {
            let egui::Shape::Mesh(label) = shape else {
                panic!("clipped label must be a mesh")
            };
            for v in &label.vertices {
                assert!(v.pos.x >= at.x - 1e-4);
                assert!((v.uv.x - (v.pos.x - bounds.left()) / bounds.width()).abs() < 1e-5);
                assert!((v.uv.y - (v.pos.y - bounds.top()) / bounds.height()).abs() < 1e-5);
                assert_eq!(v.color, Color32::WHITE);
            }
            for indices in label.indices.chunks_exact(3) {
                let [a, b, c] = [indices[0], indices[1], indices[2]]
                    .map(|i| label.vertices[i as usize].pos);
                area += f64::from((b - a).rot90().dot(c - a).abs()) * 0.5;
            }
        }
        assert!((area - 800.).abs() < 1e-3);

        let galley = ui.painter().layout_no_wrap(
            "123.45 m".into(),
            egui::FontId::monospace(12.),
            Color32::WHITE,
        );
        let original_vertices = galley.num_vertices;
        let text_at = at - galley.size() * 0.5;
        let bounds = Rect::from_min_size(text_at, galley.size()).expand(4.);
        let shapes = mesh.annotation_label(
            ui.painter(),
            viewport,
            anchor,
            bounds,
            egui::Shape::galley(text_at, Arc::clone(&galley), Color32::WHITE),
        );
        let [egui::Shape::Text(text)] = shapes.as_slice() else {
            panic!("keep clipped glyphs in a galley until final tessellation")
        };
        assert_eq!(
            galley.num_vertices, original_vertices,
            "never mutate cached font layouts"
        );
        assert!(text.galley.num_indices > 0);
        assert!(
            text.galley
                .rows
                .iter()
                .flat_map(|r| &r.visuals.mesh.vertices)
                .any(|v| v.uv.x > 1.)
        );
        use egui::emath::GuiRounding as _;
        let text_pos = text.pos.round_to_pixels(ctx.pixels_per_point());
        for row in &text.galley.rows {
            for v in &row.visuals.mesh.vertices {
                assert!(text_pos.x + row.pos.x + v.pos.x >= at.x - 1e-4);
            }
        }
    });
    output.textures_delta.clear();
}

#[test]
fn hover_frames_reuse_visibility_and_changed_views_refit_the_hierarchy() {
    let mut occlusion = Occlusion::default();
    let a = DVec3::new(10., 50., 0.5);
    let b = DVec3::new(90., 50., 0.5);
    occlusion.begin_frame();
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.4);
    assert!(occlusion.visible_intervals(a, b).is_empty());
    let revision = occlusion.revision;
    assert_eq!(occlusion.hierarchy_builds, 1);

    for _ in 0..10 {
        occlusion.begin_frame();
        occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.4);
        assert!(occlusion.visible_intervals(a, b).is_empty());
        assert_eq!(
            occlusion.query_work,
            (0, 0),
            "hover must not traverse faces again"
        );
        assert_eq!(occlusion.revision, revision);
    }
    occlusion.begin_frame();
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.6);
    assert_eq!(occlusion.visible_intervals(a, b), vec![(0., 1.)]);
    assert_ne!(occlusion.revision, revision);
    assert_eq!(occlusion.hierarchy_builds, 1, "same topology should refit");
    assert_eq!(occlusion.nodes[0].bounds.lo.z, 0.6);

    // Removing faces or changing an opaque fill to a translucent fill
    // must discard the previous frame's occluders and cached intervals.
    occlusion.begin_frame();
    assert_eq!(occlusion.visible_intervals(a, b), vec![(0., 1.)]);
    assert!(occlusion.triangles.is_empty());
}

#[test]
fn occlusion_hierarchy_prunes_large_models_and_stacked_faces() {
    let mut grid = Occlusion::default();
    for y in 0..64 {
        for x in 0..64 {
            let lo = DVec2::new(f64::from(x * 16), f64::from(y * 16));
            occluding_rectangle(&mut grid, lo, lo + DVec2::splat(8.), 0.4);
        }
    }
    assert!(
        grid.visible_intervals(DVec3::new(514., 514., 0.8), DVec3::new(518., 514., 0.8))
            .is_empty()
    );
    assert!(
        grid.query_work.0 < 64,
        "visited {} hierarchy nodes",
        grid.query_work.0
    );
    assert!(
        grid.query_work.1 <= 8,
        "tested {} triangles",
        grid.query_work.1
    );
    // Fully overlapping layers used to create an enormous per-edge
    // candidate set even when the nearest face hid the entire edge.
    let mut layers = Occlusion::default();
    for layer in 0..2048 {
        occluding_rectangle(
            &mut layers,
            DVec2::ZERO,
            DVec2::splat(100.),
            0.4 + f64::from(layer) * 0.0001,
        );
    }
    assert!(
        layers
            .visible_intervals(DVec3::new(10., 50., 0.9), DVec3::new(90., 50., 0.9))
            .is_empty()
    );
    assert!(layers.query_work.0 < 64);
    assert!(layers.query_work.1 <= 8);
}

#[test]
fn occlusion_hierarchy_matches_exhaustive_clipping_and_updates_for_new_faces() {
    let mut state = 42_u64;
    let mut random = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as f64 / f64::from(u32::MAX)
    };
    let mut occlusion = Occlusion::default();
    for _ in 0..100 {
        let points = std::array::from_fn(|_| {
            DVec3::new(random() * 800., random() * 600., 0.4 + random() * 0.2)
        });
        occlusion.insert(points);
    }
    for _ in 0..100 {
        let a = DVec3::new(random() * 800., random() * 600., 0.4 + random() * 0.2);
        let b = DVec3::new(random() * 800., random() * 600., 0.4 + random() * 0.2);
        let mut hidden: Vec<_> = occlusion
            .triangles
            .iter()
            .filter_map(|t| t.hidden_interval(a, b))
            .collect();
        hidden.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut expected = Vec::new();
        let mut start: f64 = 0.;
        for (lo, hi) in hidden {
            if lo > start {
                expected.push((start, lo));
            }
            start = start.max(hi);
        }
        if start < 1. {
            expected.push((start, 1.));
        }
        let actual = occlusion.visible_intervals(a, b);
        assert_eq!(actual.len(), expected.len());
        for ((a, b), (c, d)) in actual.into_iter().zip(expected) {
            assert!((a - c).abs() < 1e-12 && (b - d).abs() < 1e-12);
        }
    }
    assert!(!occlusion.nodes.is_empty());
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::new(800., 600.), 0.1);
    assert!(occlusion.nodes.is_empty());
    assert!(
        occlusion
            .visible_intervals(DVec3::new(10., 50., 0.9), DVec3::new(90., 50., 0.9))
            .is_empty()
    );
}

#[test]
fn hidden_centerlines_are_removed_even_within_one_gpu_depth_step() {
    let mut occlusion = Occlusion::default();
    occluding_rectangle(&mut occlusion, DVec2::ZERO, DVec2::splat(100.), 0.999);
    let a = DVec3::new(10., 50., 0.999 + 1e-9);
    let b = DVec3::new(90., 50., a.z);
    assert_eq!(a.z as f32, 0.999_f32);
    assert!(occlusion.visible_intervals(a, b).is_empty());
    // Coplanar outlines and edges in front of a face remain visible.
    for depth in [0.999, 0.998] {
        assert_eq!(
            occlusion.visible_intervals(a.truncate().extend(depth), b.truncate().extend(depth)),
            vec![(0., 1.)]
        );
    }
}

#[test]
fn centerline_clipping_keeps_exposed_ends_and_real_gaps() {
    let mut occlusion = Occlusion::default();
    for x in [100., 300.] {
        occluding_rectangle(
            &mut occlusion,
            DVec2::new(x, 0.),
            DVec2::new(x + 100., 100.),
            0.4,
        );
    }
    assert_eq!(
        occlusion.visible_intervals(DVec3::new(0., 50., 0.5), DVec3::new(500., 50., 0.5)),
        vec![(0., 0.2), (0.4, 0.6), (0.8, 1.)]
    );
}

#[test]
fn centerline_clipping_handles_both_windings_and_depth_crossings() {
    for reverse in [false, true] {
        let mut occlusion = Occlusion::default();
        let mut points = [
            DVec3::new(0., 0., 0.5),
            DVec3::new(200., 0., 0.5),
            DVec3::new(0., 200., 0.5),
        ];
        if reverse {
            points.swap(0, 1);
        }
        occlusion.insert(points);
        let visible =
            occlusion.visible_intervals(DVec3::new(10., 50., 0.4), DVec3::new(90., 50., 0.6));
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].0, 0.);
        assert!((visible[0].1 - 0.5).abs() < 1e-12);
    }
}

#[test]
fn grazing_surface_adjustments_cannot_resurrect_a_hidden_edge() {
    let camera = Camera::default();
    let size = DVec2::new(1000., 800.);
    let toward_eye = (camera.eye() - camera.target).normalize();
    let right = DVec3::Z.cross(toward_eye).normalize();
    let up = toward_eye.cross(right);
    let mut mesh = Mesh::new(&camera, size);
    mesh.world_triangle(
        &camera,
        [
            camera.target - right * 2. - up * 2.,
            camera.target + right * 2. - up * 2.,
            camera.target + up * 2.,
        ],
        Color32::WHITE,
    );
    let count = mesh.vertices.len();
    for width in [1.8, 3., 8.] {
        mesh.surface_line(
            camera.target - right * 0.2 - toward_eye * 0.01,
            camera.target + right * 0.2 - toward_eye * 0.01,
            width,
            Color32::BLACK,
            false,
            true,
            &[
                EdgeSurface::unbounded(DVec2::new(0., 0.1)),
                EdgeSurface::unbounded(DVec2::new(0., -0.1)),
            ],
        );
        assert_eq!(mesh.vertices.len(), count);
    }
    // Transparent fills must not participate in opaque occlusion.
    let mut translucent = Mesh::new(&camera, size);
    translucent.world_triangle(
        &camera,
        [
            camera.target - right - up,
            camera.target + right - up,
            camera.target + up,
        ],
        Color32::from_rgba_unmultiplied(255, 255, 255, 64),
    );
    assert!(translucent.occlusion.triangles.is_empty());
}

#[test]
fn sloping_faces_do_not_give_unrelated_hidden_edges_a_depth_allowance() {
    let gradient = depth_gradient([
        DVec3::new(0., 0., 0.4),
        DVec3::new(100., 0., 0.6),
        DVec3::new(0., 100., 0.5),
    ])
    .unwrap();
    assert!((gradient - DVec2::new(0.002, 0.001)).length() < 1e-12);
    for offset in [DVec2::new(-3., -2.), DVec2::new(3., 2.)] {
        let surface_depth = (0.5 + gradient.dot(offset)) as f32;
        assert!(
            stroke_depth(0.5, offset, &[EdgeSurface::unbounded(gradient)]) <= surface_depth
        );
        // A free edge just behind the surface must stay hidden. Previously
        // the face's slope bias exposed it for every stroke width/DPI.
        assert!(stroke_depth(f64::from(surface_depth) + 0.0001, offset, &[]) > surface_depth);
    }
}

#[test]
fn shared_crease_keeps_center_depth_for_wide_and_high_dpi_strokes() {
    for scale in [1., 2., 3.] {
        for width in [1.8, 3., 8.] {
            let mut mesh = Mesh::new(&Camera::default(), DVec2::new(1000., 800.));
            mesh.set_pixels_per_point(scale);
            mesh.surface_stroke(
                DVec3::new(500., 300., 0.5),
                DVec3::new(500., 500., 0.5),
                width,
                Color32::BLACK,
                &[
                    EdgeSurface::unbounded(DVec2::new(0.002, 0.)),
                    EdgeSurface::unbounded(DVec2::new(-0.003, 0.)),
                ],
            );
            let seam: Vec<_> = mesh.vertices.iter().filter(|v| v.lateral == 0.).collect();
            assert_eq!(seam.len(), 2);
            for v in seam {
                assert_eq!(v.position[0], 0.);
                assert_eq!(v.position[2], 0.5_f32.next_down());
            }
            assert!(
                mesh.vertices
                    .iter()
                    .filter(|v| v.lateral != 0.)
                    .all(|v| v.position[2] < 0.499)
            );
        }
    }
}

#[test]
fn edge_allowance_is_local_to_incident_visible_faces_and_includes_holes() {
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
    let mut cache = FaceCache::default();
    cache.update(&model);
    let face = model.faces.values().find(|f| !f.holes.is_empty()).unwrap();
    let mut mesh = Mesh::new(&Camera::default(), DVec2::new(1000., 800.));
    mesh.face_edges(face, cache.get(face.id));
    assert_eq!(mesh.edge_surfaces.len(), 8);
    for ring in std::iter::once(&face.vertices).chain(face.holes.iter()) {
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            assert_eq!(mesh.edge_surfaces[&(a.min(b), a.max(b))].len(), 1);
            let surface = mesh.edge_surfaces[&(a.min(b), a.max(b))][0];
            let midpoint = mesh
                .view
                .project((model.vertices[&a].position + model.vertices[&b].position) * 0.5)
                .unwrap();
            assert!(
                cache
                    .get(face.id)
                    .hit(mesh.view.ray(midpoint + surface.inward * 0.25))
                    .is_some(),
                "face-side allowance must point into material, including at holes"
            );
            assert!(
                cache
                    .get(face.id)
                    .hit(mesh.view.ray(midpoint - surface.inward * 0.25))
                    .is_none(),
                "silhouette-side allowance must not extrapolate into empty space"
            );
        }
    }
    assert!(depth_gradient([DVec3::ZERO, DVec3::X, DVec3::X * 2.]).is_none());
}

#[test]
fn stroke_width_is_constant_across_angles_and_display_scales() {
    let size = DVec2::new(1000., 800.);
    for scale in [1., 1.25, 2., 3.] {
        for angle in [0_f64, 0.3, std::f64::consts::FRAC_PI_4, 1.57, 2.4] {
            let mut mesh = Mesh::new(&Camera::default(), size);
            mesh.set_pixels_per_point(scale);
            let a = DVec2::new(500., 400.);
            let b = a + DVec2::new(angle.cos(), angle.sin()) * 100.;
            mesh.line(a.extend(0.5), b.extend(0.5), 1.8, Color32::BLACK);
            let screen = |v: &Vertex| {
                DVec2::new(
                    (f64::from(v.position[0]) + 1.) * size.x * 0.5,
                    (1. - f64::from(v.position[1])) * size.y * 0.5,
                )
            };
            let outer_width = screen(&mesh.vertices[0]).distance(screen(&mesh.vertices[1]));
            let fringe = (angle.sin().abs() + angle.cos().abs()) / f64::from(scale);
            assert!((outer_width - fringe - 1.8).abs() < 1e-4);
        }
    }
}
