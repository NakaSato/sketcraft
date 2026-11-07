//! Shadow masks, subdivisions and antialiasing.
use super::*;

#[test]
fn shadow_antialiasing_leaves_faces_strokes_and_grid_unchanged() {
    let build = |enabled| {
        let camera = Camera::default();
        let mut mesh = Mesh::new(&camera, DVec2::splat(100.));
        mesh.display.shadow_antialiasing = enabled;
        mesh.world_triangle(&camera, [DVec3::ZERO, DVec3::X, DVec3::Z], Color32::RED);
        mesh.line(
            DVec3::new(8., 32., 0.5),
            DVec3::new(56., 32., 0.5),
            2.,
            Color32::BLACK,
        );
        mesh.background_grid_line(
            [DVec2::new(8., 32.), DVec2::new(56., 32.)],
            [1., 1.],
            true,
            Color32::GRAY,
        );
        mesh.into_callback()
    };
    let enabled = build(true);
    let disabled = build(false);
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&enabled.vertices),
        bytemuck::cast_slice::<Vertex, u8>(&disabled.vertices)
    );
    assert_eq!(enabled.indices, disabled.indices);
    assert_eq!(enabled.triangle_ranges, disabled.triangle_ranges);
    assert!(enabled.shadow_range.is_empty());
    assert!(disabled.shadow_range.is_empty());
}

#[test]
fn shadow_antialiasing_preserves_receiver_silhouette() {
    let camera = Camera::default();
    let size = DVec2::new(1000., 800.);
    let triangle = [
        DVec3::new(-2., -2., 0.),
        DVec3::new(2., -2., 0.),
        DVec3::new(0., 2., 2.),
    ];
    let shadow = crate::shadows::FaceShadow {
        lit: vec![],
        shaded: vec![triangle],
    };
    let mut actual = Mesh::new(&camera, size);
    actual.shadow_face(&[triangle], &shadow, Color32::WHITE, Color32::GRAY);
    let mut original = Mesh::new(&camera, size);
    original.world_triangle(&camera, triangle, Color32::WHITE);
    assert!(!actual.vertices.is_empty());
    assert_eq!(actual.indices, original.indices);
    assert_eq!(actual.triangle_ranges, original.triangle_ranges);
    for (a, b) in actual.vertices.iter().zip(&original.vertices) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color, b.color);
        assert_eq!(a.shadow_color, Color32::GRAY.to_array());
    }
    let callback = actual.into_callback();
    assert!(!callback.shadow_range.is_empty());
    assert_eq!(callback.shadow_range.start, original.indices.len() as u32);
}

#[test]
fn shadow_mask_partitions_include_lit_occluders_at_their_original_depth() {
    for projection in [
        sketcraft_core::Projection::Perspective,
        sketcraft_core::Projection::Parallel,
    ] {
        let mut camera = Camera {
            projection,
            ..Default::default()
        };
        camera.set_standard_view(sketcraft_core::StandardView::Iso);
        let [a, b, c] = [
            DVec3::new(-2., -2., 0.),
            DVec3::new(2., -2., 0.),
            DVec3::new(0., 2., 0.),
        ];
        let midpoint = (a + b) * 0.5;
        let shadow = crate::shadows::FaceShadow {
            lit: vec![[a, midpoint, c]],
            shaded: vec![[midpoint, b, c]],
        };
        let mut mesh = Mesh::new(&camera, DVec2::new(640., 480.));
        mesh.display.shadow_antialiasing = true;
        mesh.shadow_face(&[[a, b, c]], &shadow, Color32::WHITE, Color32::GRAY);
        assert_eq!(mesh.shadow_vertices.len(), 6);
        for (vertices, (triangle, coverage)) in mesh.shadow_vertices.chunks_exact(3).zip([
            (shadow.lit[0], [0, 0, 0, 255]),
            (shadow.shaded[0], [255; 4]),
        ]) {
            for (vertex, point) in vertices.iter().zip(triangle) {
                let projected = mesh.view.project_depth(point).unwrap();
                assert_eq!(
                    vertex.position[2], projected.z as f32,
                    "mask depth must not be offset"
                );
                assert_eq!(
                    vertex.color, coverage,
                    "lit pieces must also occlude rear shadows"
                );
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "Requires a GPU adapter; verifies only shadow pixels change through the shadow mask resolve"]
fn shadow_antialiasing_smooths_shadows_without_smoothing_silhouettes() {
    use egui_wgpu::CallbackTrait;
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Shadow mask backend: {:?}", adapter.get_info().backend);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let mut renderer = egui_wgpu::Renderer::new(
            &device,
            format,
            egui_wgpu::RendererOptions {
                depth_stencil_format: Some(wgpu::TextureFormat::Depth32Float),
                ..Default::default()
            },
        );
        install_resources(&device, format, &mut renderer);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [64, 64],
            pixels_per_point: 1.,
        };
        let make_callback = |enabled| {
            let mut vertices = [
                [-0.8, -0.8, 0.5],
                [0.8, -0.8, 0.5],
                [0.8, 0.8, 0.5],
                [-0.8, 0.8, 0.5],
            ]
            .map(|position| Vertex {
                dash_phase: -1.,
                position,
                color: [255, 0, 0, 255],
                lateral: 0.,
                shadow_color: if enabled { [0, 0, 0, 255] } else { [0; 4] },
            })
            .to_vec();
            vertices.extend(
                [
                    [-0.5, -0.5, 0.5_f32.next_down()],
                    [0.6, -0.3, 0.5_f32.next_down()],
                    [0.1, 0.6, 0.5_f32.next_down()],
                ]
                .map(|position| Vertex {
                    dash_phase: -1.,
                    position,
                    color: if enabled { [255; 4] } else { [0, 0, 0, 255] },
                    lateral: 0.,
                    shadow_color: [0; 4],
                }),
            );
            SceneCallback {
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6],
                triangle_ranges: vec![0..if enabled { 6 } else { 9 }],
                shadow_range: if enabled { 6..9 } else { 9..9 },
                buffer_pool: None,
                gpu_pool: Default::default(),
                gpu_buffers: OnceLock::new(),
                antialiasing: enabled,
                target_size: [64, 64],
                antialiased: OnceLock::new(),
            }
        };
        let hard = make_callback(false);
        let smooth = make_callback(true);
        let other_pane = make_callback(true);
        // The mask must not compare a partition against its own receiver,
        // even if independent projection rounds it behind the original.
        let mut rounded = make_callback(true);
        for vertex in &mut rounded.vertices[4..] {
            vertex.position[2] = 0.5_f32.next_up();
        }
        let mut hidden = make_callback(true);
        // A separate, unshadowed foreground face still hides rear masks.
        for vertex in &mut hidden.vertices[..4] {
            vertex.shadow_color = [0; 4];
        }
        for vertex in &mut hidden.vertices[4..] {
            vertex.position[2] = 0.51;
        }
        // Exercise actual orthographic depth normalization with thousands
        // of fragments on a sloped receiver, rather than a flat overlay.
        let mut fragmented_mesh = Mesh::new(
            &Camera {
                projection: sketcraft_core::Projection::Parallel,
                ..Default::default()
            },
            DVec2::splat(64.),
        );
        fragmented_mesh.display.shadow_antialiasing = true;
        let receiver = make_callback(true);
        fragmented_mesh.vertices = receiver.vertices[..4].to_vec();
        fragmented_mesh.indices = vec![0, 1, 2, 0, 2, 3];
        fragmented_mesh.triangle_ranges = vec![0..6];
        let depth =
            |x: f32, y: f32| (1e-7 + (0.19 * f64::from(x) + 0.13 * f64::from(y)) * 1e-8) as f32;
        for vertex in &mut fragmented_mesh.vertices {
            vertex.position[2] = depth(vertex.position[0], vertex.position[1]);
        }
        for x in 0..48 {
            for y in 0..48 {
                let x0 = -0.8 + 1.6 * x as f32 / 48.;
                let x1 = -0.8 + 1.6 * (x + 1) as f32 / 48.;
                let y0 = -0.8 + 1.6 * y as f32 / 48.;
                let y1 = -0.8 + 1.6 * (y + 1) as f32 / 48.;
                for [x, y] in [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]] {
                    fragmented_mesh.shadow_vertices.push(Vertex {
                        dash_phase: -1.,
                        position: [x, y, depth(x, y)],
                        color: [255; 4],
                        lateral: 0.,
                        shadow_color: [0; 4],
                    });
                }
            }
        }
        fragmented_mesh.normalize_parallel_depth();
        let fragmented = fragmented_mesh.into_callback();
        // A lit receiver partition must erase rear shadow coverage even
        // when it is drawn after that coverage in the same mask pass.
        let mut lit_foreground = make_callback(true);
        for index in [0, 1, 2, 0, 2, 3] {
            let mut vertex = lit_foreground.vertices[index];
            vertex.position[2] = 0.49;
            vertex.color = [0, 0, 0, 255];
            vertex.shadow_color = [0; 4];
            lit_foreground
                .indices
                .push(lit_foreground.vertices.len() as u32);
            lit_foreground.vertices.push(vertex);
        }
        lit_foreground.shadow_range.end = lit_foreground.indices.len() as u32;
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 64,
                    height: 64,
                    depth_or_array_layers: 1,
                },
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
        // Repeated frames also exercise the retained resolve texture.
        let mut baseline_silhouette = None;
        for (callback, visible_shadow) in [
            (&hard, true),
            (&smooth, true),
            (&other_pane, true),
            (&smooth, true),
            (&rounded, true),
            (&hidden, false),
            (&fragmented, true),
            (&lit_foreground, false),
        ] {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256 * 64,
                mapped_at_creation: false,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            callback.prepare(
                &device,
                &queue,
                &screen,
                &mut encoder,
                &mut renderer.callback_resources,
            );
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
                        stencil_ops: None,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                    }),
                    ..Default::default()
                });
                let viewport = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64., 64.));
                callback.paint(
                    egui::PaintCallbackInfo {
                        viewport,
                        clip_rect: viewport,
                        pixels_per_point: 1.,
                        screen_size_px: [64, 64],
                    },
                    &mut pass.forget_lifetime(),
                    &renderer.callback_resources,
                );
            }
            encoder.copy_texture_to_buffer(
                color.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: 64,
                    height: 64,
                    depth_or_array_layers: 1,
                },
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
            if visible_shadow {
                assert!(
                    mapped.chunks_exact(4).any(|pixel| pixel[0] == 0),
                    "coplanar shadow must render, including after a rounding step"
                );
            }
            let center = &mapped[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4];
            assert!(
                if visible_shadow {
                    center[0] < 8
                } else {
                    center[0] > 248
                },
                "coplanar rounding must not erase shadows or expose hidden ones: {center:?}"
            );
            if std::ptr::eq(callback, &fragmented) {
                for y in 10..54 {
                    for x in 10..54 {
                        assert!(
                            mapped[(y * 64 + x) * 4] < 8,
                            "fragmented isometric shadow must have no lit holes at {x},{y}"
                        );
                    }
                }
            }
            let silhouette: Vec<_> = mapped.chunks_exact(4).map(|p| p[1]).collect();
            if let Some(baseline) = &baseline_silhouette {
                assert_eq!(&silhouette, baseline);
            } else {
                baseline_silhouette = Some(silhouette);
            }
            assert_eq!(
                mapped
                    .chunks_exact(4)
                    .any(|pixel| pixel[0] > 0 && pixel[0] < 255),
                callback.antialiasing && visible_shadow,
                "AA must resolve partial shadow coverage"
            );
        }
        assert_ne!(
            smooth.antialiased.get().unwrap().bind_group,
            other_pane.antialiased.get().unwrap().bind_group
        );
    }
}

#[test]
fn shadow_subdivisions_preserve_draw_geometry_and_original_occluders() {
    let camera = Camera::default();
    let size = DVec2::new(1000., 800.);
    let [a, b, c] = [
        DVec3::new(-2., -2., 0.),
        DVec3::new(2., -2., 0.),
        DVec3::new(0., 2., 2.),
    ];
    let ab = (a + b) * 0.5;
    let bc = (b + c) * 0.5;
    let ca = (c + a) * 0.5;
    let shadow = crate::shadows::FaceShadow {
        lit: vec![[a, ab, ca], [ab, b, bc], [ca, bc, c]],
        shaded: vec![[ab, bc, ca]],
    };
    for section in [false, true] {
        for alpha in [64, 255] {
            let display = sketcraft_core::DisplaySettings {
                section_cuts: section,
                section_height: 0.75,
                shadow_antialiasing: false,
                fog: true,
                ..Default::default()
            };
            let color = Color32::from_rgba_unmultiplied(200, 200, 200, alpha);
            let shaded = Color32::from_rgba_unmultiplied(100, 100, 100, alpha);
            let mut actual = Mesh::new(&camera, size);
            actual.set_display(&display);
            actual.shadow_face(&[[a, b, c]], &shadow, color, shaded);
            let mut expected = Mesh::new(&camera, size);
            expected.set_display(&display);
            for &triangle in &shadow.lit {
                expected.world_triangle(&camera, triangle, color);
            }
            for &triangle in &shadow.shaded {
                expected.world_triangle(&camera, triangle, shaded);
            }
            assert_eq!(
                bytemuck::cast_slice::<Vertex, u8>(&actual.vertices),
                bytemuck::cast_slice::<Vertex, u8>(&expected.vertices)
            );
            assert_eq!(actual.indices, expected.indices);
            assert_eq!(actual.triangle_ranges, expected.triangle_ranges);
            let mut original = Mesh::new(&camera, size);
            original.set_display(&display);
            original.world_triangle(&camera, [a, b, c], color);
            let points = |mesh: &Mesh| {
                mesh.occlusion
                    .triangles
                    .iter()
                    .map(|t| t.points)
                    .collect::<Vec<_>>()
            };
            assert_eq!(points(&actual), points(&original));
            if alpha == 255 {
                assert!(actual.occlusion.triangles.len() < expected.occlusion.triangles.len());
            } else {
                assert!(actual.occlusion.triangles.is_empty());
            }
        }
    }
}
