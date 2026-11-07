use crate::natural_text::TextUi as _;
use crate::{
    icons,
    modal::{Modal, primary_button},
    renderer, storage,
    theme::*,
};
use eframe::egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};
#[cfg(test)]
use ketchup_core::History;
#[cfg(test)]
use ketchup_core::infer_move_with_snapping;
use ketchup_core::inference::AxisConstraint;
use ketchup_core::{
    Axis, AxisGuide, Camera, DVec2, DVec3, Document, Inference, Model, ObjectKind, Snap, Tool,
    selection,
};
use std::{collections::BTreeSet, time::Duration};

fn dv(p: Vec2) -> DVec2 {
    DVec2::new(p.x as f64, p.y as f64)
}
fn pv(p: DVec2) -> Vec2 {
    egui::vec2(p.x as f32, p.y as f32)
}
fn rgb([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}
/// Dark or light text, whichever reads better on an inference badge of `fill`.
fn badge_text(fill: Color32) -> Color32 {
    let luma = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luma > 160. { Color32::from_rgb(24, 24, 24) } else { Color32::WHITE }
}
#[cfg(test)]
fn axis_color(a: Axis) -> Color32 {
    match a {
        Axis::X => Color32::from_rgb(192, 94, 76),
        Axis::Y => Color32::from_rgb(97, 145, 92),
        Axis::Z => Color32::from_rgb(90, 133, 176),
    }
}
fn dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(8., 12.), Sense::hover());
    ui.painter().circle_filled(r.center(), 2.5, color);
}

#[path = "app_chrome.rs"]
mod app_chrome;
#[path = "app_drawing.rs"]
mod app_drawing;
#[path = "app_input.rs"]
mod app_input;
#[path = "app_inspector.rs"]
mod app_inspector;
#[path = "app_keyboard.rs"]
mod app_keyboard;
#[path = "app_modeling.rs"]
mod app_modeling;
#[path = "app_overlay.rs"]
mod app_overlay;
#[cfg(not(target_arch = "wasm32"))]
#[path = "app_updates.rs"]
mod app_updates;
#[path = "app_tool_set.rs"]
mod app_tool_set;
#[path = "context_menu.rs"]
mod context_menu;
#[path = "currency.rs"]
mod currency;
#[path = "display_scene.rs"]
mod display_scene;
#[path = "drawing_inference.rs"]
mod drawing_inference;
#[path = "entity_gizmo.rs"]
mod entity_gizmo;
#[path = "section_gizmo.rs"]
mod section_gizmo;
#[path = "finish_settings.rs"]
mod finish_settings;
#[path = "grid.rs"]
mod grid;
#[path = "measurement_tools.rs"]
mod measurement_tools;
#[path = "move_tool.rs"]
mod move_tool;
#[path = "rotate_tool.rs"]
mod rotate_tool;
#[path = "scale_tool.rs"]
mod scale_tool;
#[path = "precision.rs"]
mod precision;
#[path = "projects.rs"]
mod projects;
#[path = "drawing_groups.rs"]
mod drawing_groups;
#[path = "joinery.rs"]
mod joinery;
#[cfg(not(target_arch = "wasm32"))]
#[path = "viewport_benchmark.rs"]
pub(crate) mod viewport_benchmark;

#[path = "fasteners.rs"]
mod fasteners;
#[path = "sharing.rs"]
mod sharing;
#[path = "screenshot_dialog.rs"]
mod screenshot_dialog;
#[path = "searchable_selector.rs"]
mod searchable_selector;
#[path = "settings.rs"]
mod settings;
#[path = "shortcut_input.rs"]
mod shortcut_input;
#[path = "shortcuts.rs"]
mod shortcuts;
#[path = "sidebar.rs"]
mod sidebar;
pub(crate) use sidebar::{modal_size, remember_modal_size};
#[path = "solid_tools.rs"]
mod solid_tools;
#[path = "stock_settings.rs"]
mod stock_settings;
#[path = "text_tools.rs"]
mod text_tools;
#[path = "model_library.rs"]
mod model_library;
#[path = "tag_menu.rs"]
mod tag_menu;
#[path = "time_estimates.rs"]
mod time_estimates;
#[path = "viewports.rs"]
mod viewports;
use context_menu::ContextAction;
use move_tool::{CopyArray, MoveCopy};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum PaintTarget {
    #[default]
    Face,
    Group,
}

const COMPASS_SCALE: f32 = 1.3;
const COMPASS_RADIUS: f32 = 32. * COMPASS_SCALE;
const COMPASS_TIP_RADIUS: f32 = 7. * COMPASS_SCALE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompassAxis {
    axis: Axis,
    negative: bool,
}

impl CompassAxis {
    fn direction(self) -> DVec3 {
        self.axis.vector() * if self.negative { -1. } else { 1. }
    }

    fn label(self) -> &'static str {
        match (self.axis, self.negative) {
            (Axis::X, false) => "X",
            (Axis::Y, false) => "Y",
            (Axis::Z, false) => "Z",
            (Axis::X, true) => "−X",
            (Axis::Y, true) => "−Y",
            (Axis::Z, true) => "−Z",
        }
    }

    fn view(self) -> ketchup_core::StandardView {
        use ketchup_core::StandardView;
        match (self.axis, self.negative) {
            (Axis::X, false) => StandardView::Right,
            (Axis::Y, false) => StandardView::Back,
            (Axis::Z, false) => StandardView::Top,
            (Axis::X, true) => StandardView::Left,
            (Axis::Y, true) => StandardView::Front,
            (Axis::Z, true) => StandardView::Bottom,
        }
    }
}

enum GestureKind {
    Compass {
        axis: Option<CompassAxis>,
        pivot: Option<DVec3>,
        dragged: bool,
    },
    Navigate {
        pivot: Option<DVec3>,
    },
    DrawingNavigate,
    Select {
        additive: bool,
    },
    Line,
    Shape,
    Freehand {
        paused: bool,
    },
    Modeling,
    /// Press-drag-release for Push/Pull, Move and Offset; `active` once the drag started an operation.
    ModelingDrag {
        active: bool,
    },
    Solid,
    Measure,
    Paste,
    Erase {
        ids: BTreeSet<u64>,
    },
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct GeometryClipboard {
    model: Model,
    anchor: DVec3,
}
struct PastePreview {
    clipboard: GeometryClipboard,
    point: DVec3,
}
struct Gesture {
    button: egui::PointerButton,
    start: Pos2,
    last: Pos2,
    kind: GestureKind,
}

struct Modeling {
    base: Model,
    entities: BTreeSet<u64>,
    face: Option<u64>,
    anchor: DVec3,
    pixel: Pos2,
    pointer: Pos2,
    center: DVec3,
    scale_constraint: Option<AxisConstraint>,
    rotation: Option<rotate_tool::Rotation>,
    direction: DVec3,
    // Keep Push/Pull's original normal when switching or clearing axis locks.
    unlocked_direction: DVec3,
    guides: Vec<AxisGuide>,
    value: f64,
    copy: bool,
    preview: Option<Model>,
    move_snap: Option<ketchup_core::inference::MoveSnapCache>,
    snap_excluded: BTreeSet<u64>,
    move_preview: Option<ketchup_core::modeling::MovePreview>,
    preview_delta: Option<DVec3>,
    snap: Option<Snap>,
    error: Option<&'static str>,
}

struct Drawing {
    origin: DVec3,
    surface_normal: DVec3,
    on_surface: bool,
    normal: DVec3,
    u: DVec3,
    v: DVec3,
    points: Vec<DVec3>,
}

impl Drawing {
    fn set_normal(&mut self, normal: DVec3) {
        let u = (DVec3::X - normal * normal.x)
            .try_normalize()
            .unwrap_or(DVec3::Y);
        self.normal = normal;
        self.u = u;
        self.v = normal.cross(u);
    }

    fn set_lock(&mut self, lock: Option<Axis>) {
        let normal = lock.map(Axis::vector).unwrap_or(self.surface_normal);
        if normal == self.normal {
            return;
        }
        let (u, v) = (self.u, self.v);
        self.set_normal(normal);
        // Preserve a sampled stroke's size and shape when changing its plane.
        for point in &mut self.points {
            let delta = *point - self.origin;
            *point = self.origin + self.u * delta.dot(u) + self.v * delta.dot(v);
        }
    }

    fn align_to(&mut self, point: DVec3, eye: DVec3) {
        let delta = point - self.origin;
        if delta.dot(self.normal).abs() < 1e-6 {
            return;
        }
        let Some(direction) = delta.try_normalize() else {
            return;
        };
        let view = (eye - self.origin).normalize();
        // Prefer a world plane containing both points. For a vertical radius,
        // choose the plane that faces the camera rather than one seen edge-on.
        let normal = [DVec3::X, DVec3::Y, DVec3::Z]
            .into_iter()
            .filter(|normal| delta.dot(*normal).abs() < 1e-6)
            .max_by(|a, b| a.dot(view).abs().total_cmp(&b.dot(view).abs()))
            .or_else(|| {
                // A tilted target still needs an exact, planar shape. Keep the
                // normal as close to the starting orientation as possible.
                let projected =
                    self.surface_normal - direction * self.surface_normal.dot(direction);
                (projected.length_squared() > 1e-12).then(|| projected.normalize())
            })
            .or_else(|| {
                let projected = view - direction * view.dot(direction);
                (projected.length_squared() > 1e-12).then(|| projected.normalize())
            })
            .unwrap_or_else(|| direction.cross(DVec3::X).normalize());
        let normal = if normal.dot(self.normal) < 0. {
            -normal
        } else {
            normal
        };
        // Automatic inference changes the working plane, never earlier samples.
        self.set_normal(normal);
    }
}

struct ObjectCreation {
    component: bool,
    name: String,
}

pub struct KetchupApp {
    #[cfg(not(target_arch = "wasm32"))]
    viewport_benchmark: Option<viewport_benchmark::Benchmark>,
    account: crate::account::Account,
    joinery_geometry: joinery::GeometryCache,
    joinery: joinery::Preferences,
    joinery_dialog: joinery::Dialog,
    fasteners: fasteners::Preferences,
    fastener_dialog: fasteners::Dialog,
    sharing: sharing::Sharing,
    #[cfg(not(target_arch = "wasm32"))]
    updates: crate::updates::Updater,
    doc: Document,
    project: projects::Project,
    screenshot: screenshot_dialog::ScreenshotDialog,
    screenshot_gpu: Option<(eframe::wgpu::Device, eframe::wgpu::Queue)>,
    tool: Tool,
    drawing_tool: Tool,
    measurement_tool: Tool,
    solid_tool: Tool,
    solid_first: Option<u64>,
    solid_volume_cache: solid_tools::VolumeCache,
    flip_hover: Option<Axis>,
    drawing: Option<Drawing>,
    polygon_sides: usize,
    selected: BTreeSet<u64>,
    hover: Option<u64>,
    outliner_hover: BTreeSet<u64>,
    history_preview: Option<Model>,
    outliner_anchor: Option<u64>,
    outliner_filter: String,
    model_library: model_library::ModelLibrary,
    /// Two-column rail with every tool (SketchUp's Large Tool Set).
    large_tool_set: bool,
    outliner_cache: sidebar::OutlinerCache,
    focus_outliner_search: bool,
    line_start: Option<DVec3>,
    line_group: Option<u64>,
    snap: Option<Snap>,
    snap_to_grid: bool,
    snap_to_edge: bool,
    snap_to_face: bool,
    snap_to_face_center: bool,
    drawing_reference: Option<drawing_inference::DrawingReference>,
    drawing_guide: Option<AxisGuide>,
    drawing_axis_guides: Vec<AxisGuide>,
    drawing_projection: Option<(DVec3, DVec3)>,
    lock: Option<Axis>,
    second_lock: Option<Axis>,
    length_input: String,
    modeling: Option<Modeling>,
    move_copy: bool,
    move_modifier_down: bool,
    last_move_copy: Option<MoveCopy>,
    measurement: Option<measurement_tools::Measurement>,
    text_editor: Option<text_tools::TextEditor>,
    paint_color: [u8; 3],
    paint_target: PaintTarget,
    gesture: Option<Gesture>,
    section_drag: Option<section_gizmo::SectionDrag>,
    drawing_orbit_down: bool,
    drawing_orbit_pivot: Option<DVec3>,
    viewport_pointer: Option<Pos2>,
    viewport: Rect,
    viewports: viewports::Workspace,
    inspector: bool,
    inspector_width: f32,
    inspector_preferences: sidebar::InspectorPreferences,
    time_estimates: time_estimates::Settings,
    finish_product_filters: std::collections::BTreeMap<String, String>,
    stock_material_filter: String,
    wood_type_filters: std::collections::BTreeMap<String, String>,
    estimate_cache: time_estimates::EstimateCache,
    inspector_order: [sidebar::InspectorSection; sidebar::InspectorSection::COUNT],
    inspector_layout: sidebar::InspectorLayout,
    component_filter: String,
    new_tag_name: String,
    tag_rename: Option<(u64, String)>,
    help: bool,
    settings_open: bool,
    material_removal: Option<ketchup_core::MaterialId>,
    general_settings: settings::GeneralSettings,
    about: bool,
    release_notes: crate::release_notes::ReleaseNotes,
    shortcut_filter: String,
    notice: Option<(String, f64)>,
    dirty_since: Option<f64>,
    save_error: Option<String>,
    storage_recovery: bool,
    gpu_ready: bool,
    entity_cache: renderer::EntityCache,
    face_cache: renderer::FaceCache,
    scene_cache: renderer::SceneCache,
    render_model: renderer::ModelSnapshot,
    shadow_cache: crate::shadows::ShadowCache,
    section_cache: display_scene::SectionCache,
    picking_cache: crate::picking::PickingCache,
    // Rendering consumes machined/preview geometry. Input queries consume the
    // source document. Keep both sets alive instead of alternately invalidating
    // their triangulation, BVH and the shadow cache on every pointer event.
    render_face_cache: renderer::FaceCache,
    group_marker_cache: renderer::GroupMarkerCache,
    render_entity_cache: renderer::EntityCache,
    render_picking_cache: crate::picking::PickingCache,
    #[cfg(test)]
    picking_preparations: usize,
    paste_face_cache: renderer::FaceCache,
    clipboard: Option<GeometryClipboard>,
    paste_preview: Option<PastePreview>,
    context_position: Option<Pos2>,
    object_creation: Option<ObjectCreation>,
    tape_resize: Option<measurement_tools::TapeResize>,
    object_rename: Option<(u64, String)>,
}

impl KetchupApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply(&cc.egui_ctx);
        restore_preference(&cc.egui_ctx, cc.storage);
        let (doc, error) = match storage::load() {
            Ok(doc) => (doc.unwrap_or_default(), None),
            Err(e) => (Document::default(), Some(e)),
        };
        let mut app = Self::with_document(doc, error, renderer::install(cc));
        app.screenshot_gpu = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| (state.device.clone(), state.queue.clone()));
        app.restore_inspector_preferences(&cc.egui_ctx, cc.storage);
        app.release_notes.open_if_unread();
        app.account.start(&cc.egui_ctx);
        app.open_shared_startup(&cc.egui_ctx);
        #[cfg(not(target_arch = "wasm32"))]
        let app = {
            let mut app = app;
            app.updates = crate::updates::Updater::new(&cc.egui_ctx);
            match crate::updates::Resume::take(&app.doc) {
                Ok(Some(resume)) => {
                    app.project.path = resume.path;
                    app.project.modified = resume.modified;
                }
                Ok(None) => {}
                Err(error) => {
                    app.updates.error = Some(format!(
                        "Could not restore the project file location after updating: {error}"
                    ))
                }
            }
            app
        };
        #[cfg(not(target_arch = "wasm32"))]
        let app = {
            let mut app = app;
            app.viewport_benchmark = viewport_benchmark::Benchmark::from_environment(&mut app);
            app
        };
        app
    }
    fn with_document(mut doc: Document, error: Option<String>, gpu_ready: bool) -> Self {
        let selected = Self::restore_document_selection(&mut doc);
        let geometry_snapping = doc.snap_to_geometry;
        let recovery = error.is_some();
        let modified = doc.model != Model::default()
            || doc.name != Document::default().name
            || doc.camera != Camera::default()
            || !doc.snap_to_geometry
            || !doc.snap_to_axes
            || doc.display != ketchup_core::DisplaySettings::default();
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            viewport_benchmark: None,
            #[cfg(not(target_arch = "wasm32"))]
            updates: crate::updates::Updater::default(),
            account: Default::default(),
            joinery_geometry: Default::default(),
            joinery: Default::default(),
            joinery_dialog: Default::default(),
            fasteners: Default::default(),
            fastener_dialog: Default::default(),
            sharing: Default::default(),
            doc,
            project: projects::Project {
                modified,
                ..Default::default()
            },
            screenshot: screenshot_dialog::ScreenshotDialog::default(),
            screenshot_gpu: None,
            tool: Tool::Select,
            drawing_tool: Tool::Line,
            measurement_tool: Tool::Dimensions,
            solid_tool: Tool::Union,
            solid_first: None,
            solid_volume_cache: Default::default(),
            flip_hover: None,
            drawing: None,
            polygon_sides: 6,
            selected,
            hover: None,
            outliner_hover: BTreeSet::new(),
            history_preview: None,
            outliner_anchor: None,
            outliner_filter: String::new(),
            model_library: Default::default(),
            large_tool_set: false,
            outliner_cache: Default::default(),
            focus_outliner_search: false,
            line_start: None,
            line_group: None,
            snap: None,
            snap_to_grid: false,
            snap_to_edge: geometry_snapping,
            snap_to_face: geometry_snapping,
            snap_to_face_center: geometry_snapping,
            drawing_reference: None,
            drawing_guide: None,
            drawing_axis_guides: vec![],
            drawing_projection: None,
            lock: None,
            second_lock: None,
            length_input: String::new(),
            modeling: None,
            move_copy: false,
            move_modifier_down: false,
            last_move_copy: None,
            measurement: None,
            text_editor: None,
            paint_color: [195, 96, 67],
            paint_target: PaintTarget::default(),
            gesture: None,
            section_drag: None,
            drawing_orbit_down: false,
            drawing_orbit_pivot: None,
            viewport_pointer: None,
            viewport: Rect::NOTHING,
            viewports: viewports::Workspace::default(),
            inspector: true,
            inspector_width: 300.,
            inspector_preferences: sidebar::InspectorPreferences::default(),
            time_estimates: Default::default(),
            finish_product_filters: Default::default(),
            stock_material_filter: String::new(),
            wood_type_filters: Default::default(),
            estimate_cache: Default::default(),
            inspector_order: sidebar::InspectorSection::ALL,
            inspector_layout: sidebar::InspectorLayout::default(),
            component_filter: String::new(),
            new_tag_name: String::new(),
            tag_rename: None,
            help: false,
            settings_open: false,
            material_removal: None,
            general_settings: Default::default(),
            about: false,
            release_notes: crate::release_notes::ReleaseNotes::default(),
            shortcut_filter: String::new(),
            notice: error.map(|e| {
                (
                    format!("Could not restore your model: {e}. A new workspace is ready."),
                    30.,
                )
            }),
            dirty_since: None,
            save_error: None,
            storage_recovery: recovery,
            gpu_ready,
            entity_cache: renderer::EntityCache::default(),
            face_cache: renderer::FaceCache::default(),
            scene_cache: renderer::SceneCache::default(),
            render_model: renderer::ModelSnapshot::default(),
            shadow_cache: crate::shadows::ShadowCache::default(),
            section_cache: Default::default(),
            picking_cache: crate::picking::PickingCache::default(),
            render_face_cache: renderer::FaceCache::default(),
            group_marker_cache: Default::default(),
            render_entity_cache: renderer::EntityCache::default(),
            render_picking_cache: crate::picking::PickingCache::default(),
            #[cfg(test)]
            picking_preparations: 0,
            paste_face_cache: renderer::FaceCache::default(),
            clipboard: None,
            paste_preview: None,
            context_position: None,
            object_creation: None,
            tape_resize: None,
            object_rename: None,
        }
    }
    fn changed(&mut self, ctx: &egui::Context) {
        self.project.modified = true;
        self.dirty_since = Some(ctx.input(|i| i.time));
        ctx.request_repaint_after(Duration::from_millis(550));
    }
    fn say(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.notice = Some((text.into(), ctx.input(|i| i.time) + 5.));
        ctx.request_repaint_after(Duration::from_secs(5));
    }
    fn set_tool(&mut self, tool: Tool) {
        self.drawing_reference = None;
        self.drawing_guide = None;
        self.drawing_axis_guides.clear();
        self.drawing_projection = None;
        self.drawing_orbit_down = false;
        self.drawing_orbit_pivot = None;
        self.hover = None;
        self.solid_first = None;
        self.flip_hover = None;
        if tool.is_solid() {
            self.solid_tool = tool;
        }
        self.move_copy = false;
        self.last_move_copy = None;
        self.measurement = None;
        self.text_editor = None;
        self.tool = tool;
        if tool.is_drawing() {
            self.drawing_tool = tool;
        }
        if tool.is_measuring() {
            self.measurement_tool = tool;
        }
        self.drawing = None;
        if tool == Tool::Paint {
            self.inspector = true;
        }
        self.modeling = None;
        self.line_start = None;
        self.line_group = None;
        self.snap = None;
        self.lock = None;
        self.second_lock = None;
        self.length_input.clear();
        self.gesture = None;
        self.paste_preview = None;
    }
    fn enter_object(&mut self, id: u64, ctx: &egui::Context) {
        if self.doc.model.enter_object(id) {
            self.outliner_anchor = None;
            self.set_tool(Tool::Select);
            self.selected.clear();
            self.hover = None;
            self.notice = None;
            ctx.request_repaint();
        } else if self.doc.model.is_locked(id) {
            self.say(ctx, "Unlock the object before opening it");
        }
    }
    fn navigate_context(&mut self, context: Option<u64>, ctx: &egui::Context) {
        self.outliner_anchor = None;
        self.doc.model.edit_context = context;
        self.set_tool(Tool::Select);
        self.selected.clear();
        self.hover = None;
        self.notice = None;
        ctx.request_repaint();
    }
    fn close_edit_context(&mut self, ctx: &egui::Context) {
        let parent = self
            .doc
            .model
            .edit_context
            .and_then(|id| self.doc.model.parent(id));
        self.navigate_context(parent, ctx);
    }
    fn finish_context_state_edit(&mut self, label: &str, before: Model, ctx: &egui::Context) {
        if let Err(message) = self.doc.model.synchronize_edit_context() {
            self.doc.model = before;
            self.say(ctx, message);
        } else {
            self.finish_edit(label, before, ctx);
        }
    }
    fn finish_edit(&mut self, label: &str, before: Model, ctx: &egui::Context) {
        if before != self.doc.model && !self.shared_edit_allowed(ctx) {
            self.doc.model = before;
            self.say(ctx, self.shared_edit_block_reason(ctx));
            return;
        }
        if before != self.doc.model {
            self.solid_first = None;
            self.last_move_copy = None;
            self.measurement = None;
            self.modeling = None;
            let author = self.current_history_author();
            self.doc.history.record_attributed(label, before, &self.doc.model, Some(author), Some(sharing::now_timestamp()));
            self.selected.retain(|id| {
                self.doc.model.entity_in_context(*id) == Some(*id) && !self.doc.model.is_hidden(*id)
            });
            self.changed(ctx);
            #[cfg(target_arch = "wasm32")]
            self.persist();
        }
    }
    fn undo(&mut self, ctx: &egui::Context, redo: bool) {
        let current = self.doc.history.position();
        let target = if redo {
            current + 1
        } else {
            current.saturating_sub(1)
        };
        self.navigate_history(ctx, target);
    }
    fn preview_history(&mut self, target: usize) {
        self.history_preview = None;
        if target == self.doc.history.position() {
            return;
        }
        let mut history = self.doc.history.clone();
        let mut model = self.doc.model.clone();
        if history.seek(&mut model, target) {
            self.history_preview = Some(model);
        }
    }
    fn navigate_history(&mut self, ctx: &egui::Context, target: usize) {
        if !self.shared_edit_allowed(ctx) { self.say(ctx, self.shared_edit_block_reason(ctx)); return; }
        self.history_preview = None;
        self.drawing_reference = None;
        self.drawing_guide = None;
        self.drawing_axis_guides.clear();
        self.drawing_projection = None;
        self.solid_first = None;
        self.flip_hover = None;
        self.last_move_copy = None;
        self.measurement = None;
        self.drawing = None;
        self.modeling = None;
        self.gesture = None;
        self.paste_preview = None;
        self.line_start = None;
        self.line_group = None;
        self.snap = None;
        self.length_input.clear();
        let path = self.doc.model.edit_path();
        let author = self.current_history_author();
        let did = self.doc.history.seek_attributed(&mut self.doc.model, target, Some(author), Some(sharing::now_timestamp()));
        if did {
            self.outliner_anchor = None;
            self.doc.model.edit_context = path.into_iter().rev().find(|id| {
                self.doc.model.objects.contains_key(id)
                    && !self.doc.model.is_locked(*id)
                    && !self.doc.model.is_hidden(*id)
            });
            self.selected.clear();
            self.hover = None;
            self.changed(ctx);
            #[cfg(target_arch = "wasm32")]
            self.persist();
        }
    }
    fn erase_selected(&mut self, ctx: &egui::Context) {
        let before = self.doc.model.clone();
        self.doc.model.erase(&self.selected);
        self.finish_edit("Erase selection", before, ctx);
    }
    fn copy_selection(&mut self, ctx: &egui::Context) {
        if self.selected.is_empty() {
            return;
        }
        let model = self.doc.model.copy_entities(&self.selected);
        let Some((lo, hi)) = model.bounds() else {
            return;
        };
        let clipboard = GeometryClipboard {
            model,
            anchor: (lo + hi) * 0.5,
        };
        if let Ok(json) = serde_json::to_string(&clipboard) {
            ctx.copy_text(format!("ketchup-geometry-v1\n{json}"));
        }
        self.clipboard = Some(clipboard);
    }
    fn create_object(&mut self, ctx: &egui::Context, component: bool, name: &str) {
        let before = self.doc.model.clone();
        if let Some(id) = self.doc.model.make_object(&self.selected, component, name) {
            self.selected = BTreeSet::from([id]);
            self.set_tool(Tool::Select);
            self.finish_edit(
                if component {
                    "Create component"
                } else {
                    "Make group"
                },
                before,
                ctx,
            );
        }
    }
    fn paste_at(&mut self, ctx: &egui::Context, clipboard: &GeometryClipboard, offset: DVec3) {
        let before = self.doc.model.clone();
        let selected = self.doc.model.paste_entities(&clipboard.model, offset);
        if selected.is_empty() {
            self.say(
                ctx,
                "Cannot paste this geometry here; a component cannot contain its own definition",
            );
            return;
        }
        self.selected = selected;
        self.finish_edit("Paste", before, ctx);
    }
    fn request_name_input_focus(ctx: &egui::Context, id: egui::Id) {
        ctx.data_mut(|data| data.insert_temp(id.with("focus_name"), true));
    }
    fn focus_name_input(ui: &mut egui::Ui, name: &str, id: egui::Id) {
        // A new window first runs an invisible sizing pass. Wait until the field
        // is visible so that pass cannot discard the initial keyboard focus.
        if ui.is_visible()
            && ui.ctx().data_mut(|data| {
                data.remove_temp::<bool>(id.with("focus_name"))
                    .unwrap_or(false)
            })
        {
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::select_all_str(name)));
            state.store(ui.ctx(), id);
            ui.memory_mut(|memory| memory.request_focus(id));
            ui.ctx().request_repaint();
        }
    }
    fn name_input(ui: &mut egui::Ui, name: &mut String, id: egui::Id) -> egui::Response {
        // On the first visible frame, focus before TextEdit reads typed input.
        // The opening click still needs the post-registration focus below.
        if !ui.input(|input| input.pointer.any_pressed() || input.pointer.any_released()) {
            Self::focus_name_input(ui, name, id);
        }
        let response = ui.add(
            egui::TextEdit::singleline(&mut *name)
                .id(id)
                .desired_width(ui.available_width()),
        );
        // Register the widget first: egui can surrender focus while processing
        // the click that opened a previously sized dialog.
        Self::focus_name_input(ui, name, id);
        response
    }
    fn begin_object_creation(&mut self, ctx: &egui::Context, component: bool) {
        if self.selected.is_empty() || self.doc.model.selection_protected(&self.selected) {
            return;
        }
        let name = if component { "Component" } else { "Group" };
        self.object_creation = Some(ObjectCreation {
            component,
            name: name.into(),
        });
        self.gesture = None;
        Self::request_name_input_focus(ctx, egui::Id::unique("create_object_name"));
    }
    fn object_creation_window(&mut self, ctx: &egui::Context) {
        let Some(ObjectCreation {
            component,
            mut name,
        }) = self.object_creation.take()
        else {
            return;
        };
        let mut create = false;
        let mut cancel = false;
        let response = Modal::new("create_object", 360.).show(ctx, |modal| {
            modal.header(|ui| {
                ui.text_heading(if component {
                    "Create Component"
                } else {
                    "Make Group"
                });
                ui.text_weak("Give your selection a name.");
            });
            modal.body(|ui| {
                ui.text_label("Name");
                let input = Self::name_input(ui, &mut name, egui::Id::unique("create_object_name"));
                create = input.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if name.len() > 256 {
                    name.truncate(name.floor_char_boundary(256));
                }
            });
            modal.footer(|ui| {
                cancel = ui.button("Cancel").clicked();
                let valid = !name.trim().is_empty();
                create = (primary_button(ui, "Create", valid).clicked() || create) && valid;
            });
        });
        if create {
            self.create_object(ctx, component, &name);
        } else if !cancel && !response.should_close() {
            self.object_creation = Some(ObjectCreation { component, name });
        }
    }
    fn new_model(&mut self, ctx: &egui::Context) {
        let doc = Document {
            snap_to_geometry: self.doc.snap_to_geometry,
            snap_to_axes: self.doc.snap_to_axes,
            display: self.doc.display.clone(),
            camera: Camera {
                projection: self.doc.camera.projection,
                fov: self.doc.camera.fov,
                ..Camera::default()
            },
            ..Document::default()
        };
        let mut doc = doc;
        doc.display.units = self.general_settings.default_units;
        self.replace_project(doc, None, ctx);
        self.say(ctx, "New project ready");
    }
    fn fit(&mut self, ctx: &egui::Context) {
        self.doc.camera.fit(
            self.doc.model.bounds(),
            dv(self.viewport.size().max(Vec2::splat(1.))),
        );
        self.changed(ctx);
    }
    fn fit_origin(&mut self, ctx: &egui::Context) {
        self.doc.camera.target = DVec3::ZERO;
        self.changed(ctx);
    }
    fn about_window(&mut self, ctx: &egui::Context) {
        let response = Modal::new("about_ketchup", 408.).show(ctx, |modal| {
            modal.header(|ui| {
                let colors = Palette::for_ui(ui);
                ui.horizontal(|ui| {
                    let (logo, _) = ui.allocate_exact_size(egui::vec2(48., 48.), Sense::hover());
                    icons::logo(ui.painter(), logo);
                    ui.add_space(8.);
                    ui.vertical(|ui| {
                        ui.text_heading("About KetchUp");
                        ui.text_label(
                            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                .color(colors.muted),
                        );
                    });
                });
            });
            modal.body(|ui| {
                let colors = Palette::for_ui(ui);
                ui.text_label("KetchUp is a 3D modeling app for turning ideas into shapes. Draw precise geometry, shape surfaces, and organize your model with groups and components.");
                ui.add_space(8.);
                ui.text_label(
                    RichText::new("Your work is saved automatically on this device. Save a .ketchup project to keep a portable copy.")
                        .small()
                        .color(colors.muted),
                );
            });
            modal.footer(|ui| {
                if primary_button(ui, "Close", true).clicked() {
                    ui.close();
                }
            });
        });
        if response.should_close() {
            self.about = false;
        }
    }

    fn restore_document_selection(doc: &mut Document) -> BTreeSet<u64> {
        doc.selection_context = doc.selection_context.filter(|id| {
            doc.model.objects.contains_key(id)
                && !doc.model.is_locked(*id)
                && !doc.model.is_hidden(*id)
        });
        doc.model.edit_context = doc.selection_context;
        doc.selection.retain(|id| {
            doc.model.entity_in_context(*id) == Some(*id) && !doc.model.is_hidden(*id)
        });
        doc.selection.clone()
    }

    /// Keep project-specific selection out of the reusable inspector preferences.
    fn sync_selection(&mut self) -> bool {
        let desired = self.shared_selection_to_remember().unwrap_or(&self.selected);
        if self.doc.selection == *desired
            && self.doc.selection_context == self.doc.model.edit_context
        {
            return false;
        }
        let selection: BTreeSet<_> = desired
            .iter()
            .copied()
            .filter(|id| {
                self.doc.model.entity_in_context(*id) == Some(*id) && !self.doc.model.is_hidden(*id)
            })
            .collect();
        let changed = self.doc.selection != selection
            || self.doc.selection_context != self.doc.model.edit_context;
        self.doc.selection = selection;
        self.doc.selection_context = self.doc.model.edit_context;
        changed
    }

    fn persist(&mut self) {
        self.sync_selection();
        self.sync_viewport_cameras();
        match storage::save(&self.doc) {
            Ok(()) => {
                self.dirty_since = None;
                self.save_error = None;
                self.storage_recovery = false;
            }
            Err(e) => {
                self.save_error = Some(e);
                self.dirty_since = None;
            }
        }
    }
}
impl eframe::App for KetchupApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(mut benchmark) = self.viewport_benchmark.take() {
            benchmark.tick(self, &ctx, frame);
            self.viewport_benchmark = Some(benchmark);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = viewport_benchmark::scope("ui");
        // Rebuilt by visible Outliner rows; closing the inspector clears the highlight too.
        self.outliner_hover.clear();
        self.history_preview = None;
        self.account.poll(&ctx);
        self.poll_project(&ctx);
        self.poll_sharing(&ctx);
        self.sharing_before_frame(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.updates.poll(&ctx);
            if self.updates.restarting {
                ui.disable();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        self.keyboard(&ctx);
        if self.project.blocked() || self.screenshot.open {
            let opacity = ui.opacity();
            ui.disable();
            ui.set_opacity(opacity);
        }
        self.top_bar(ui);
        self.status_bar(ui);
        self.tool_rail(ui);
        let inspector_order_before = self.inspector_order;
        self.inspector_panel(ui);
        let theme_before = ctx.options(|options| options.theme_preference);
        self.viewport_panel(ui);
        if theme_before != ctx.options(|options| options.theme_preference)
            && let Some(storage) = frame.storage_mut()
        {
            save_preference(&ctx, storage);
        }
        self.object_creation_window(&ctx);
        self.tape_resize_window(&ctx);
        self.rename_object_window(&ctx);
        self.connection_sidebar_selection(&ctx);
        self.rename_viewport_window(&ctx);
        self.text_editor_window(&ctx);
        if self.help {
            self.help_window(&ctx);
        }
        if self.settings_open {
            self.settings_window(&ctx);
        }
        self.account.show(&ctx);
        self.share_dialog(&ctx);
        self.material_removal_window(&ctx);
        if self.about {
            self.about_window(&ctx);
        }
        crate::system_info::show(&ctx, frame);
        self.sync_stock_materials(&ctx);
        self.release_notes.show(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        self.update_window(&ctx);
        self.project_dialogs(&ctx);
        self.screenshot_dialog(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        Self::window_resize_handles(ui);
        self.sharing_after_frame(&ctx);
        self.persist_inspector_changes(
            &ctx,
            frame.storage_mut(),
            inspector_order_before != self.inspector_order,
        );
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            let title = format!("{} - KetchUp", self.doc.name);
            if document.title() != title {
                document.set_title(&title);
            }
        }
        if let Some(since) = self.dirty_since {
            let now = ctx.input(|i| i.time);
            if now - since >= 0.5 {
                self.persist();
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis(550));
            }
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_inspector_preferences(storage);
        if self.sync_selection() || self.dirty_since.is_some() {
            self.persist();
        }
    }
    fn on_exit(&mut self) {
        if self.sync_selection() || self.dirty_since.is_some() {
            self.persist();
        }
        let ctx = egui::Context::default();
        self.release_shared_selection(&ctx);
        #[cfg(not(target_arch = "wasm32"))]
        if self.updates.ready()
            && !self.updates.restarting
            && self.save_error.is_none()
            && !self.storage_recovery
        {
            // Preserve the file association when the pending update installs on
            // next launch, just as for the explicit Restart now action.
            if let Err(error) = crate::updates::Resume::save(
                &self.doc,
                self.project.path.clone(),
                self.project.modified,
            ) {
                eprintln!("Could not save update resume information: {error}");
            }
        }
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "app_tests.rs"]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "project_tests.rs"]
mod project_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tool_rail_tests.rs"]
mod tool_rail_tests;
