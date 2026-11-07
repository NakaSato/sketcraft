use super::*;
use crate::test_ui::ContextExt as _;
use eframe::App;

#[path = "axis_lock_tests.rs"]
mod axis_locks;
#[path = "rotate_tool_tests.rs"]
mod rotate_tool;
#[path = "scale_tool_tests.rs"]
mod scale_tool;
#[path = "offset_tool_tests.rs"]
mod offset_tool;
#[path = "modeling_anchor_tests.rs"]
mod modeling_anchors;

#[path = "compass_tests.rs"]
mod compass;
#[path = "context_menu_tests.rs"]
mod context_menus;
#[path = "drawing_inference_tests.rs"]
mod drawing_inference;
#[path = "feature_audit_tests.rs"]
mod feature_audit;
#[path = "feature_suggestion_tests.rs"]
mod feature_suggestion;
#[path = "grid_tests.rs"]
mod grid_snapping;
#[path = "outliner_drag_tests.rs"]
mod outliner_drag;
#[path = "precision_tests.rs"]
mod precision_input;
#[path = "self_snap_tests.rs"]
mod self_snapping;
#[path = "release_notes_tests.rs"]
mod release_notes;
#[path = "rendering_tests.rs"]
mod rendering;
#[path = "text_tools_tests.rs"]
mod text_tools;
#[path = "app_workload_tests.rs"]
mod workload_tests;
#[path = "app_move_copy_tests.rs"]
mod move_copy_tests;
#[path = "app_solid_tools_tests.rs"]
mod solid_tools_tests;
#[path = "app_erase_tests.rs"]
mod erase_tests;
#[path = "app_navigation_tests.rs"]
mod navigation_tests;
#[path = "app_window_tests.rs"]
mod window_tests;
#[path = "app_drawing_tests.rs"]
mod drawing_tests;
#[path = "app_modeling_tests.rs"]
mod modeling_tests;
#[path = "app_objects_tests.rs"]
mod objects_tests;
#[path = "app_measurement_tests.rs"]
mod measurement_tests;
#[path = "app_inspector_tests.rs"]
mod inspector_tests;

struct Harness {
    ctx: egui::Context,
    app: KetchupApp,
    time: f64,
    size: Vec2,
    maximized: bool,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        apply(&ctx);
        // Keep UI interaction fixtures settled and all picker choices visible.
        ctx.style_mut_of(ctx.theme(), |style| {
            style.animation_time = 0.;
            style.spacing.combo_height = 1000.;
        });
        let mut harness = Self {
            ctx,
            app: KetchupApp::with_document(Document::default(), None, true),
            time: 0.,
            size: egui::vec2(1440., 900.),
            maximized: false,
        };
        // Interaction tests use an explicit, expanded legacy workspace so
        // control access and cached row positions do not depend on defaults.
        // Fresh-layout defaults and restoration have separate preference tests.
        use sidebar::InspectorSection::*;
        let order = [Outliner, EntityInfo, Units, Components, Tags, Display, View,
            TimeCostEstimates, Materials, History];
        harness.app.inspector_order = order;
        for key in ["outliner", "entity_info", "units", "components", "tags",
            "display", "view", "time_cost_estimates", "materials", "history"] {
            sidebar::set_section_open(&harness.ctx, key, true);
        }
        harness.frame(vec![], egui::Modifiers::NONE);
        harness.frame(vec![], egui::Modifiers::NONE);
        harness
    }
    fn frame(
        &mut self,
        mut events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        self.time += 0.01;
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .maximized = Some(self.maximized);
        let app = &mut self.app;
        let mut output = self
            .ctx
            .run_test_ui(input, |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()));
        output.textures_delta.clear();
        output
    }
    fn pointer(
        &mut self,
        pos: Pos2,
        button: egui::PointerButton,
        pressed: bool,
        shift: bool,
    ) -> egui::FullOutput {
        let modifiers = egui::Modifiers {
            shift,
            ..Default::default()
        };
        self.pointer_with_modifiers(pos, button, pressed, modifiers)
    }
    fn pointer_with_modifiers(
        &mut self,
        pos: Pos2,
        button: egui::PointerButton,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        self.frame(
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                },
            ],
            modifiers,
        )
    }
    fn click(&mut self, pos: Pos2) -> egui::FullOutput {
        self.pointer(pos, egui::PointerButton::Primary, true, false);
        self.pointer(pos, egui::PointerButton::Primary, false, false)
    }
    fn at(&self, p: DVec3) -> Pos2 {
        self.app.viewport.min
            + pv(self
                .app
                .doc
                .camera
                .project(p, dv(self.app.viewport.size()))
                .unwrap())
    }
    fn key(&mut self, key: Key) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![event(true)], egui::Modifiers::NONE);
        self.frame(vec![event(false)], egui::Modifiers::NONE);
    }
}

fn window_commands(output: &egui::FullOutput) -> &[egui::ViewportCommand] {
    &output.viewport_output[&egui::ViewportId::ROOT].commands
}

fn native_motion_burst(from: Pos2, to: Pos2, count: usize) -> Vec<egui::Event> {
    (1..=count)
        .flat_map(|i| {
            [
                egui::Event::PointerMoved(from.lerp(to, i as f32 / count as f32)),
                egui::Event::MouseMoved(egui::vec2(1., -1.)),
            ]
        })
        .collect()
}

fn gpu_test_state() -> eframe::egui_wgpu::RenderState {
    use eframe::{egui_wgpu, wgpu};
    use std::sync::Arc;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU adapter");
    eprintln!("GPU test adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let renderer = egui_wgpu::Renderer::new(
        &device,
        format,
        egui_wgpu::RendererOptions {
            depth_stencil_format: Some(wgpu::TextureFormat::Depth32Float),
            ..Default::default()
        },
    );
    egui_wgpu::RenderState {
        adapter,
        available_adapters: vec![],
        instance,
        device,
        queue,
        target_format: format,
        renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
    }
}

fn render_snapshots(state: eframe::egui_wgpu::RenderState, audit: bool) {
    use eframe::wgpu;
    let ctx = egui::Context::default();
    apply(&ctx);
    let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
    cc.wgpu_render_state = Some(state.clone());
    assert!(renderer::install(&cc));
    let mut app = KetchupApp::with_document(Document::default(), None, true);
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa");
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut time = 0.;
    let snapshots: Vec<String> = if audit {
        feature_audit::render_cases()
            .into_iter()
            .map(|c| c.name)
            .collect()
    } else {
        [
            "empty",
            "modeled",
            "face-selected",
            "axis-preview",
            "push-pull-preview",
            "painted",
            "context-menu",
            "component-dialog",
            "paste-preview",
            "group-edit",
            "nested-edit",
            "move-alignment",
            "rectangle-preview",
            "drawing-menu",
            "measure-tape",
            "measure-angle",
            "dimensions",
            "measurement-menu",
            "solid-menu",
            "solid-union",
            "solid-split",
            "flip-planes",
            "sidebar",
            "display-section",
            "display-xray",
            "compact",
            "axis-in-front",
            "axis-behind",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    };
    let mut previous_audit: Option<(String, Vec<u8>)> = None;
    for snapshot in snapshots {
        let snapshot = snapshot.as_str();
        if audit {
            app = feature_audit::render_workspace(snapshot);
        }
        if matches!(snapshot, "axis-in-front" | "axis-behind") {
            app = KetchupApp::with_document(Document::default(), None, true);
            app.doc.display.grid = false;
            app.doc.camera.target = DVec3::Y * 3.;
            let toward_eye = (app.doc.camera.eye() - app.doc.camera.target).normalize();
            let right = DVec3::Z.cross(toward_eye).normalize();
            let up = toward_eye.cross(right);
            // Cover the same screen point with a face either behind or in
            // front of the Y axis. Both occlusion directions must work.
            let center = app.doc.camera.target
                + toward_eye * if snapshot == "axis-in-front" { -1. } else { 1. };
            let corners = [
                center - right - up,
                center + right - up,
                center + right + up,
                center - right + up,
            ];
            for i in 0..4 {
                app.doc
                    .model
                    .add_edge(corners[i], corners[(i + 1) % 4])
                    .unwrap();
            }
        }
        if snapshot == "modeled" {
            app.doc.name = "Courtyard study".into();
            let corners = [
                DVec3::new(-3., -2., 0.),
                DVec3::new(3., -2., 0.),
                DVec3::new(3., 2., 0.),
                DVec3::new(-3., 2., 0.),
            ];
            for i in 0..4 {
                let a = corners[i];
                let b = corners[(i + 1) % 4];
                app.doc.model.add_edge(a, b).unwrap();
                app.doc
                    .model
                    .add_edge(a + DVec3::Z * 3., b + DVec3::Z * 3.)
                    .unwrap();
                app.doc.model.add_edge(a, a + DVec3::Z * 3.).unwrap();
            }
            let ridge_a = DVec3::new(-3., 0., 5.);
            let ridge_b = DVec3::new(3., 0., 5.);
            app.doc.model.add_edge(ridge_a, ridge_b).unwrap();
            for (i, ridge) in [(0, ridge_a), (3, ridge_a), (1, ridge_b), (2, ridge_b)] {
                app.doc
                    .model
                    .add_edge(corners[i] + DVec3::Z * 3., ridge)
                    .unwrap();
            }
            app.selected
                .insert(*app.doc.model.edges.keys().next().unwrap());
            app.doc.camera.target = DVec3::new(0., 0., 2.);
            app.doc.camera.distance = 19.;
        }
        if snapshot == "face-selected" {
            let point = app
                .doc
                .camera
                .project(DVec3::new(0., -2., 1.5), dv(app.viewport.size()))
                .unwrap();
            let (id, _, _) = app
                .doc
                .model
                .nearest_face(app.doc.camera.ray(point, dv(app.viewport.size())))
                .unwrap();
            app.selected = BTreeSet::from([id]);
        }
        if snapshot == "axis-preview" {
            app.set_tool(Tool::Line);
            app.selected.clear();
            let start = DVec3::new(3., -2., 3.);
            app.line_start = Some(start);
            let cursor = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(start + DVec3::Z * 1.8, dv(app.viewport.size()))
                    .unwrap())
                + egui::vec2(2., 0.);
            app.update_pointer(cursor);
            assert_eq!(app.preview_alignment(), Some(Axis::Z));
        }
        if snapshot == "push-pull-preview" {
            app.doc = Document::default();
            let corners = [
                DVec3::new(-2., -2., 0.),
                DVec3::new(2., -2., 0.),
                DVec3::new(2., 2., 0.),
                DVec3::new(-2., 2., 0.),
            ];
            for i in 0..4 {
                app.doc
                    .model
                    .add_edge(corners[i], corners[(i + 1) % 4])
                    .unwrap();
            }
            app.set_tool(Tool::PushPull);
            app.selected.clear();
            let at = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(DVec3::ZERO, dv(app.viewport.size()))
                    .unwrap());
            app.begin_modeling(at, &ctx);
            let end = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(DVec3::Z * 2., dv(app.viewport.size()))
                    .unwrap());
            app.update_modeling(end);
            assert_eq!(
                app.modeling
                    .as_ref()
                    .unwrap()
                    .preview
                    .as_ref()
                    .unwrap()
                    .faces
                    .len(),
                6
            );
        }
        if snapshot == "painted" {
            let preview = app.modeling.take().unwrap().preview.unwrap();
            app.doc.model = preview;
            app.set_tool(Tool::Paint);
            app.selected.clear();
            for id in app.doc.model.faces.keys().copied().collect::<Vec<_>>() {
                app.doc.model.paint_face(id, [195, 96, 67]).unwrap();
            }
        }
        if snapshot == "context-menu" {
            app.set_tool(Tool::Select);
            app.selected = app.doc.model.selectable_entities();
            app.create_object(&ctx, false, "House");
        }
        if snapshot == "component-dialog" {
            egui::Popup::close_all(&ctx);
            app.object_creation = Some(ObjectCreation {
                component: true,
                name: "House component".into(),
            });
        }
        if snapshot == "paste-preview" {
            app.object_creation = None;
            app.context_action(&ctx, ContextAction::Copy);
            app.context_action(&ctx, ContextAction::Paste);
        }
        if snapshot == "group-edit" {
            app.doc = Document::default();
            app.doc.name = "Nested assemblies".into();
            app.set_tool(Tool::Select);
            let corners = [
                DVec3::new(-2., -2., 0.),
                DVec3::new(2., -2., 0.),
                DVec3::new(2., 2., 0.),
                DVec3::new(-2., 2., 0.),
            ];
            for i in 0..4 {
                app.doc
                    .model
                    .add_edge(corners[i], corners[(i + 1) % 4])
                    .unwrap();
            }
            let panel = app
                .doc
                .model
                .make_object(&app.doc.model.selectable_entities(), false, "Panel")
                .unwrap();
            let assembly = app
                .doc
                .model
                .make_object(&BTreeSet::from([panel]), true, "Assembly")
                .unwrap();
            let copy = app.doc.model.copy_entities(&BTreeSet::from([assembly]));
            app.doc.model.paste_entities(&copy, DVec3::X * 6.);
            app.doc.camera.target = DVec3::X * 2.;
            app.doc.camera.distance = 21.;
            app.enter_object(assembly, &ctx);
        }
        if snapshot == "nested-edit" {
            let assembly = app.doc.model.edit_context.unwrap();
            let panel = *app.doc.model.objects[&assembly]
                .entities
                .iter()
                .find(|id| app.doc.model.objects.contains_key(id))
                .unwrap();
            app.enter_object(panel, &ctx);
            let face = *app
                .doc
                .model
                .object_contents(panel)
                .unwrap()
                .faces
                .keys()
                .next()
                .unwrap();
            app.selected = BTreeSet::from([face]);
        }
        if snapshot == "move-alignment" {
            app.doc = Document::default();
            app.doc.camera.target = DVec3::new(0., 1., 2.);
            let start = DVec3::new(-4., -3., 2.);
            let moving = app
                .doc
                .model
                .add_edge(start, start + DVec3::X * 0.5)
                .unwrap();
            for reference in [DVec3::new(0., 4., 2.), DVec3::new(3., 0., 2.)] {
                app.doc
                    .model
                    .add_edge(reference, reference + DVec3::new(0.31, 0.27, 0.19))
                    .unwrap();
            }
            app.selected = BTreeSet::from([moving]);
            app.set_tool(Tool::Move);
            let anchor = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(start, dv(app.viewport.size()))
                    .unwrap());
            app.begin_modeling(anchor, &ctx);
        }
        if snapshot == "rectangle-preview" {
            app.doc = Document::default();
            app.set_tool(Tool::Rectangle);
            app.notice = None;
            app.selected.clear();
            let start = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(DVec3::new(-2., -1.5, 0.), dv(app.viewport.size()))
                    .unwrap());
            app.begin_drawing(start);
            let end = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(DVec3::new(2., 1.5, 0.), dv(app.viewport.size()))
                    .unwrap());
            app.update_pointer(end);
        }
        if snapshot == "drawing-menu" {
            app.drawing = None;
        }
        if snapshot == "measurement-menu" {
            egui::Popup::close_all(&ctx);
            app.set_tool(Tool::Dimensions);
        }
        if snapshot.starts_with("measure-") || snapshot == "dimensions" {
            egui::Popup::close_all(&ctx);
            app.doc = Document::default();
            app.doc.name = "Measured study".into();
            app.doc.camera.target = DVec3::new(2., 1., 0.);
            app.inspector = true;
            app.notice = None;
            app.selected.clear();
            let points = [
                DVec3::ZERO,
                DVec3::X * 4.,
                DVec3::new(4., 3., 0.),
                DVec3::Y * 3.,
            ];
            for i in 0..4 {
                app.doc
                    .model
                    .add_edge(points[i], points[(i + 1) % 4])
                    .unwrap();
            }
            let at = |point| {
                app.viewport.min
                    + pv(app
                        .doc
                        .camera
                        .project(point, dv(app.viewport.size()))
                        .unwrap())
            };
            let clicks = if snapshot == "measure-tape" {
                vec![at(points[0]), at(points[2])]
            } else {
                vec![at(points[0]), at(points[1]), at(points[3])]
            };
            if snapshot == "dimensions" {
                app.set_tool(Tool::Dimensions);
                let id = app
                    .doc
                    .model
                    .add_dimension(points[0], points[1], DVec3::new(0., -1.2, 0.))
                    .unwrap();
                app.doc
                    .model
                    .add_dimension(points[1], points[2], DVec3::X * 1.2)
                    .unwrap();
                app.selected = BTreeSet::from([id]);
            } else {
                app.set_tool(if snapshot == "measure-tape" {
                    Tool::TapeMeasure
                } else {
                    Tool::Protractor
                });
                for click in clicks {
                    app.measurement_click(click, &ctx);
                }
            }
        }
        if snapshot.starts_with("solid-") || snapshot == "flip-planes" {
            egui::Popup::close_all(&ctx);
            app.doc = Document::default();
            app.doc.name = "Solid tools study".into();
            app.doc.camera.target = DVec3::new(2., 1., 1.);
            app.doc.camera.distance = 16.;
            app.inspector = true;
            app.notice = None;
            let a = ui_solid_box(&mut app.doc.model, DVec3::ZERO, DVec3::new(3., 3., 2.));
            let b = ui_solid_box(
                &mut app.doc.model,
                DVec3::new(1.5, -1., 0.),
                DVec3::new(4.5, 2., 3.),
            );
            for (id, color) in [(a, [190, 85, 62]), (b, [84, 133, 160])] {
                let faces: Vec<_> = app
                    .doc
                    .model
                    .expand_entities(&BTreeSet::from([id]))
                    .into_iter()
                    .filter(|id| app.doc.model.faces.contains_key(id))
                    .collect();
                for face in faces {
                    app.doc.model.paint_face(face, color).unwrap();
                }
            }
            app.selected = BTreeSet::from([a, b]);
            if snapshot == "solid-menu" {
                app.set_tool(Tool::Union);
            } else if snapshot == "flip-planes" {
                app.selected = BTreeSet::from([a]);
                app.start_solid_tool(Tool::Flip, &ctx);
                app.flip_hover = Some(Axis::X);
            } else {
                app.start_solid_tool(
                    if snapshot == "solid-split" {
                        Tool::Split
                    } else {
                        Tool::Union
                    },
                    &ctx,
                );
                if snapshot == "solid-split" {
                    for id in app.selected.clone() {
                        let offset = match app.doc.model.objects[&id].name.as_str() {
                            "Split A" => DVec3::new(-1., 0., 0.),
                            "Split B" => DVec3::new(1., 0., 0.),
                            _ => DVec3::new(0., -1., 0.),
                        };
                        app.doc
                            .model
                            .move_selection(&BTreeSet::from([id]), offset)
                            .unwrap();
                    }
                }
                assert_eq!(
                    app.selected.len(),
                    if snapshot == "solid-split" { 3 } else { 1 }
                );
                app.selected.clear();
            }
        }
        if matches!(snapshot, "sidebar" | "display-section" | "display-xray") {
            egui::Popup::close_all(&ctx);
            app.set_tool(Tool::Select);
            app.inspector = true;
            app.selected.clear();
            app.notice = None;
            if snapshot == "sidebar" {
                app.doc = Document::default();
                app.doc.name = "Component collection".into();
                let object = ui_solid_box(&mut app.doc.model, DVec3::ZERO, DVec3::new(2., 2., 3.));
                let component = app
                    .doc
                    .model
                    .make_object(&BTreeSet::from([object]), true, "Cabinet")
                    .unwrap();
                let copy = app.doc.model.copy_entities(&BTreeSet::from([component]));
                let second = app.doc.model.paste_entities(&copy, DVec3::X * 4.);
                let tag = app.doc.model.add_tag("Furniture").unwrap();
                app.doc
                    .model
                    .assign_tag(&BTreeSet::from([component]), Some(tag))
                    .unwrap();
                app.doc.model.assign_tag(&second, Some(tag)).unwrap();
                app.doc.model.add_tag("Architecture").unwrap();
                app.doc.camera.target = DVec3::new(3., 1., 1.);
                app.doc.camera.distance = 16.;
            }
            app.doc.display.shadows = true;
            app.doc.display.section_planes = snapshot == "display-section";
            app.doc.display.section_cuts = snapshot == "display-section";
            app.doc.display.section_height = 1.5;
            app.doc.display.x_ray = snapshot == "display-xray";
            app.doc.display.back_edges = snapshot == "display-xray";
        }
        if snapshot == "compact" {
            app.set_tool(Tool::Select);
            app.inspector = false;
        }
        let (width, height) = if snapshot == "compact" {
            (900_u32, 600_u32)
        } else {
            (1440_u32, 900_u32)
        };
        let device = &state.device;
        let queue = &state.queue;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("KetchUp QA screenshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: state.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("QA depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());
        for frame in 0..4 {
            time += 0.1;
            app.dirty_since = None; // Snapshot edits must not write a user's saved model.
            let events = if snapshot == "drawing-menu" && frame < 2 {
                let point = egui::pos2(38., 169.);
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 0,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else if matches!(snapshot, "solid-menu" | "measurement-menu") && frame < 2 {
                let point = egui::pos2(38., if snapshot == "solid-menu" { 781. } else { 713. });
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 0,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else if matches!(snapshot, "solid-union" | "solid-split" | "flip-planes") {
                vec![egui::Event::PointerMoved(egui::pos2(1100., 90.))]
            } else if snapshot == "context-menu" && frame < 2 {
                let point = app.viewport.min
                    + pv(app
                        .doc
                        .camera
                        .project(DVec3::new(0., -2., 1.5), dv(app.viewport.size()))
                        .unwrap());
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Secondary,
                        pressed: frame == 0,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else if snapshot == "paste-preview" {
                let point = app.viewport.min
                    + pv(app
                        .doc
                        .camera
                        .project(DVec3::new(6., 0., 0.), dv(app.viewport.size()))
                        .unwrap());
                vec![egui::Event::PointerMoved(point)]
            } else if snapshot == "move-alignment" {
                let cursor = app.viewport.min
                    + pv(app
                        .doc
                        .camera
                        .project(DVec3::new(3., 4., 2.), dv(app.viewport.size()))
                        .unwrap())
                    + egui::vec2(2., -2.);
                vec![egui::Event::PointerMoved(cursor)]
            } else {
                vec![]
            };
            let cpu_start = std::time::Instant::now();
            let mut output = ctx.run_test_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(width as f32, height as f32),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
            if audit && frame == 3 {
                eprintln!(
                    "DISPLAY_CPU {snapshot} {:.3} ms",
                    cpu_start.elapsed().as_secs_f64() * 1000.
                );
            }
            let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
            let screen = eframe::egui_wgpu::ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: output.pixels_per_point,
            };
            let mut renderer = state.renderer.write();
            for (id, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    renderer.update_texture(device, queue, *id, delta);
                }
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            let buffers = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("QA scene"),
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
            }
            queue.submit(buffers.into_iter().chain([encoder.finish()]));
            for id in &output.textures_delta.free {
                renderer.free_texture(id);
            }
            output.textures_delta.clear();
        }
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("QA readback"),
            size: stride as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap()
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        receive.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let rgba: Vec<u8> = mapped
            .chunks(stride as usize)
            .flat_map(|row| row[..width as usize * 4].iter().copied())
            .collect();
        if audit {
            let (pair, index) = snapshot.rsplit_once('-').unwrap();
            if index == "0" {
                previous_audit = Some((pair.into(), rgba.clone()));
            } else {
                let (previous_pair, before) = previous_audit.take().unwrap();
                assert_eq!(previous_pair, pair);
                let mut changed = 0;
                for y in app.viewport.top().ceil() as usize..app.viewport.bottom().floor() as usize
                {
                    for x in
                        app.viewport.left().ceil() as usize..app.viewport.right().floor() as usize
                    {
                        let offset = (y * width as usize + x) * 4;
                        changed +=
                            usize::from(rgba[offset..offset + 3] != before[offset..offset + 3]);
                    }
                }
                if !fasteners::AVAILABLE && matches!(pair,
                    "audit-fasteners" | "audit-fastener_preview" | "audit-fastener_face_handles"
                ) {
                    // Retained project settings must have no visual effect while
                    // Automatic Fasteners is temporarily unavailable.
                    assert_eq!(changed, 0, "{pair} exposed disabled fastener visuals");
                } else {
                    assert!(changed > 0, "{pair} did not change any viewport pixels");
                }
                eprintln!("DISPLAY_RENDER {pair} changed={changed}");
            }
        }
        if snapshot == "face-selected" {
            let point = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(DVec3::new(0., -2., 1.5), dv(app.viewport.size()))
                    .unwrap());
            let offset = (point.y as usize * width as usize + point.x as usize) * 4;
            assert_eq!(
                &rgba[offset..offset + 3],
                // The selected highlight is shaded by the view's key light.
                &[192, 159, 142],
                "Selected face must render a filled interior"
            );
        }
        if matches!(snapshot, "axis-in-front" | "axis-behind") {
            let point = app.viewport.min
                + pv(app
                    .doc
                    .camera
                    .project(DVec3::Y * 3., dv(app.viewport.size()))
                    .unwrap());
            let green_axis = (-2..=2).any(|dy| {
                (-2..=2).any(|dx| {
                    let x = point.x.round() as i32 + dx;
                    let y = point.y.round() as i32 + dy;
                    let offset = (y as usize * width as usize + x as usize) * 4;
                    let [r, g, b] =
                        [rgba[offset], rgba[offset + 1], rgba[offset + 2]].map(i32::from);
                    g > r + 12 && g > b + 12
                })
            });
            assert_eq!(
                green_axis,
                snapshot == "axis-in-front",
                "{snapshot}: Y axis must respect face depth"
            );
        }
        assert!(
            rgba.chunks(4)
                .any(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100),
            "UI should contain visible dark content"
        );
        let file = std::fs::File::create(out_dir.join(format!("{snapshot}.png"))).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
        drop(mapped);
        buffer.unmap();
    }
}

fn dropdown_position(output: &egui::FullOutput) -> Pos2 {
    fn locate(shape: &egui::Shape) -> Option<Pos2> {
        match shape {
            egui::Shape::Path(path)
                if path.closed && path.points.len() == 3
                    && path.points[1] - path.points[0] == egui::vec2(8., 0.)
                    && path.points[2] - path.points[0] == egui::vec2(4., 4.) =>
            {
                Some(path.points[0] + egui::vec2(4., 2.))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(locate),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|shape| locate(&shape.shape))
        .expect("Missing vector dropdown indicator")
}

fn text_position(output: &egui::FullOutput, label: &str) -> Pos2 {
    fn locate(shape: &egui::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| locate(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, label))
        .unwrap_or_else(|| panic!("Missing UI text: {label}"))
}

fn menu_text_position(output: &egui::FullOutput, label: &str) -> Pos2 {
    fn locate(shape: &egui::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().rev().find_map(|shape| locate(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|shape| locate(&shape.shape, label))
        .unwrap_or_else(|| panic!("Missing submenu text: {label}"))
}

fn assert_menu_icon(output: &egui::FullOutput, label: &str) {
    let center = menu_text_position(output, label);
    let text = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.job.text == label
                    && (text.pos.y - center.y).abs() < 16.
                    && text.pos.x > 76. =>
            {
                Some(text)
            }
            _ => None,
        })
        .expect("Menu label must be painted");
    let icon_rect =
        Rect::from_center_size(egui::pos2(text.pos.x - 20., center.y), egui::vec2(30., 30.));
    assert!(
        output.shapes.iter().any(|shape| {
            !matches!(shape.shape, egui::Shape::Text(_) | egui::Shape::Noop)
                && icon_rect.contains_rect(shape.shape.visual_bounding_rect())
                && shape.shape.visual_bounding_rect().is_positive()
        }),
        "Missing icon beside {label}"
    );
}

fn open_context_menu(h: &mut Harness, point: Pos2) -> egui::FullOutput {
    // Let egui retire the hit-test layer of a previously closed popup/window.
    h.frame(vec![], egui::Modifiers::NONE);
    h.frame(vec![], egui::Modifiers::NONE);
    h.pointer(point, egui::PointerButton::Secondary, true, false);
    h.pointer(point, egui::PointerButton::Secondary, false, false);
    h.frame(vec![], egui::Modifiers::NONE)
}

fn modeling_rectangle(h: &mut Harness) -> u64 {
    let points = [
        DVec3::ZERO,
        DVec3::new(4., 0., 0.),
        DVec3::new(4., 4., 0.),
        DVec3::new(0., 4., 0.),
    ];
    for i in 0..4 {
        h.app
            .doc
            .model
            .add_edge(points[i], points[(i + 1) % 4])
            .unwrap();
    }
    *h.app.doc.model.faces.keys().next().unwrap()
}

fn modeling_number(h: &mut Harness, value: &str) {
    h.frame(vec![egui::Event::Text(value.into())], egui::Modifiers::NONE);
    h.key(Key::Enter);
}

fn double_click_object(h: &mut Harness, world: DVec3) {
    h.time += 0.4;
    let point = h.at(world);
    h.click(point);
    h.click(point);
    h.frame(vec![], egui::Modifiers::NONE);
    h.frame(vec![], egui::Modifiers::NONE);
}

fn begin_move_copy(h: &mut Harness, mac: bool) -> Model {
    h.ctx.set_os(if mac {
        egui::os::OperatingSystem::Mac
    } else {
        egui::os::OperatingSystem::Windows
    });
    let edge = h.app.doc.model.add_edge(DVec3::ZERO, DVec3::Y).unwrap();
    h.app.selected = BTreeSet::from([edge]);
    let before = h.app.doc.model.clone();
    h.key(Key::M);
    let start = h.at(DVec3::ZERO);
    h.click(start);
    h.key(Key::ArrowRight);
    let end = h.at(DVec3::X * 6.);
    h.frame(vec![egui::Event::PointerMoved(end)], egui::Modifiers::NONE);
    let modifier = if mac {
        egui::Modifiers::ALT
    } else {
        egui::Modifiers::CTRL
    };
    h.frame(vec![], modifier);
    assert!(h.app.move_copy);
    assert!(h.app.modeling.as_ref().unwrap().copy);
    h.frame(vec![], egui::Modifiers::NONE);
    assert!(
        h.app.move_copy,
        "releasing the modifier must keep Copy enabled"
    );
    before
}

fn assert_copy_positions(h: &Harness, expected: &[f64]) {
    let mut positions: Vec<_> = h
        .app
        .doc
        .model
        .edges
        .values()
        .map(|e| h.app.doc.model.vertices[&e.a].position.x)
        .collect();
    positions.sort_by(f64::total_cmp);
    assert_eq!(positions.len(), expected.len());
    for (actual, expected) in positions.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }
    h.app.doc.model.validate().unwrap();
}

fn ui_solid_box(model: &mut Model, min: DVec3, max: DVec3) -> u64 {
    let mut body = Model::default();
    let corners = [
        min,
        DVec3::new(max.x, min.y, min.z),
        DVec3::new(max.x, max.y, min.z),
        DVec3::new(min.x, max.y, min.z),
    ];
    for i in 0..4 {
        body.add_edge(corners[i], corners[(i + 1) % 4]).unwrap();
    }
    let face = *body.faces.keys().next().unwrap();
    let normal = ketchup_core::faces::plane(&body, &body.faces[&face].vertices)
        .unwrap()
        .normal;
    body.push_pull(face, (max.z - min.z) / normal.z).unwrap();
    let entities = model.paste_entities(&body, DVec3::ZERO);
    model.make_object(&entities, false, "Solid box").unwrap()
}

fn properties_output(h: &mut Harness) -> egui::FullOutput {
    h.ctx.run_test_ui(egui::RawInput::default(), |ui| {
        h.app.entity_info_contents(ui);
    })
}

fn properties_outline(model: &mut Model, points: &[DVec3]) -> u64 {
    let before: BTreeSet<_> = model.faces.keys().copied().collect();
    for i in 0..points.len() {
        match model.add_edge(points[i], points[(i + 1) % points.len()]) {
            Ok(_) | Err("That edge already exists") => {}
            Err(error) => panic!("Could not create properties fixture: {error}"),
        }
    }
    *model.faces.keys().find(|id| !before.contains(id)).unwrap()
}

fn ui_solid_pair(h: &mut Harness) -> (u64, u64) {
    let a = ui_solid_box(&mut h.app.doc.model, DVec3::ZERO, DVec3::splat(2.));
    let b = ui_solid_box(
        &mut h.app.doc.model,
        DVec3::new(1., 0., 0.),
        DVec3::new(3., 2., 2.),
    );
    (a, b)
}

