#[cfg(test)]
use crate::test_ui::ContextExt as _;
use super::*;
use ketchup_core::{
    AngleUnit, AreaUnit, DisplaySettings, FaceStyle, LengthUnit, Projection, VolumeUnit,
};

#[path = "view_controls.rs"]
mod view_controls;

#[path = "outliner.rs"]
mod outliner;
pub(super) use outliner::OutlinerCache;

#[path = "inspector_docking.rs"]
mod docking;
pub(super) use docking::InspectorLayout;

const ORDER_PREFERENCE_KEY: &str = "ketchup.inspector-order.v1";

const PREFERENCE_KEY: &str = "ketchup.inspector.v1";
const OUTLINER_SEARCH_ID: &str = "outliner_search";

// Persist the whole settings object: new DisplaySettings fields automatically
// participate, while serde defaults keep older preferences compatible.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct InspectorPreferences {
    joinery: joinery::Preferences,
    fasteners: fasteners::Preferences,
    sharing: sharing::Preferences,
    account_open: bool,
    account_menu_open: bool,
    general: settings::GeneralSettings,
    visible: bool,
    width: f32,
    docking: Option<docking::DockLayout>,
    component_filter: String,
    outliner_filter: String,
    shortcut_filter: String,
    paint_color: [u8; 3],
    paint_target: PaintTarget,
    tools: ToolPreferences,
    snapping: Option<SnappingPreferences>,
    cutting_plan: CuttingPlanPreferences,
    time_estimates: time_estimates::Settings,
    finish_product_filters: std::collections::BTreeMap<String, String>,
    stock_material_filter: String,
    wood_type_filters: std::collections::BTreeMap<String, String>,
    last_seen_release_notes: Option<String>,
    display: Option<DisplaySettings>,
    camera: Option<CameraPreferences>,
    viewports: viewports::Preferences,
    screenshot: crate::screenshots::Settings,
    recent_projects: Vec<projects::RecentProject>,
    import_format: String,
    export_stl: bool,
    sections: std::collections::BTreeMap<String, bool>,
    section_aliases: std::collections::BTreeMap<String, String>,
    scroll_positions: std::collections::BTreeMap<String, [f32; 2]>,
    modal_sizes: std::collections::BTreeMap<String, [f32; 2]>,
}

impl Default for InspectorPreferences {
    fn default() -> Self {
        Self {
            joinery: Default::default(),
            fasteners: Default::default(),
            sharing: Default::default(),
            account_open: false,
            account_menu_open: false,
            general: Default::default(),
            visible: true,
            width: 300.,
            docking: None,
            component_filter: String::new(),
            outliner_filter: String::new(),
            shortcut_filter: String::new(),
            paint_color: [195, 96, 67],
            paint_target: PaintTarget::default(),
            tools: ToolPreferences::default(),
            snapping: None,
            cutting_plan: CuttingPlanPreferences::default(),
            time_estimates: Default::default(),
            finish_product_filters: Default::default(),
            stock_material_filter: String::new(),
            wood_type_filters: Default::default(),
            last_seen_release_notes: None,
            display: None,
            camera: None,
            viewports: viewports::Preferences::default(),
            screenshot: Default::default(),
            recent_projects: Vec::new(),
            import_format: "skp".into(),
            export_stl: false,
            sections: Default::default(),
            section_aliases: Default::default(),
            scroll_positions: Default::default(),
            modal_sizes: Default::default(),
        }
    }
}

// Remember tool choices and reusable options, never an in-progress operation or
// model-specific selection. Defaults also apply to partially saved settings.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct ToolPreferences {
    active: Tool,
    drawing: Tool,
    measurement: Tool,
    solid: Tool,
    polygon_sides: usize,
    move_copy: bool,
    axis_lock: Option<Axis>,
    second_axis_lock: Option<Axis>,
}

impl Default for ToolPreferences {
    fn default() -> Self {
        Self {
            active: Tool::Select,
            drawing: Tool::Line,
            measurement: Tool::Dimensions,
            solid: Tool::Union,
            polygon_sides: 6,
            move_copy: false,
            axis_lock: None,
            second_axis_lock: None,
        }
    }
}

impl ToolPreferences {
    fn capture(app: &KetchupApp) -> Self {
        Self {
            active: app.tool,
            drawing: app.drawing_tool,
            measurement: app.measurement_tool,
            solid: app.solid_tool,
            polygon_sides: app.polygon_sides,
            move_copy: app.move_copy,
            axis_lock: app.lock,
            second_axis_lock: app.second_lock,
        }
    }

    fn restore(&self, app: &mut KetchupApp) {
        let defaults = Self::default();
        app.drawing_tool = if self.drawing.is_drawing() {
            self.drawing
        } else {
            defaults.drawing
        };
        app.measurement_tool = if self.measurement.is_measuring() {
            self.measurement
        } else {
            defaults.measurement
        };
        app.solid_tool = if self.solid.is_solid() {
            self.solid
        } else {
            defaults.solid
        };
        app.polygon_sides = self.polygon_sides.clamp(3, 64);
        // This resets transient state without executing the tool (solid tools
        // can otherwise act immediately on selected geometry).
        app.set_tool(self.active);
        app.move_copy = self.move_copy;
        app.lock = self.axis_lock;
        app.second_lock = self.second_axis_lock
            .filter(|axis| self.axis_lock.is_some_and(|first| first != *axis));
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct SnappingPreferences {
    // Read the former master switch when migrating older saved preferences.
    // New captures store true and express the effective choices in the targets.
    geometry: bool,
    edges: bool,
    faces: bool,
    face_centers: bool,
    axes: bool,
    grid: bool,
}

impl Default for SnappingPreferences {
    fn default() -> Self {
        Self {
            geometry: true,
            edges: true,
            faces: true,
            face_centers: true,
            axes: true,
            grid: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct CuttingPlanPreferences {
    layout: crate::cutting_layout::Settings,
    selection: Option<bool>,
}

impl Default for CuttingPlanPreferences {
    fn default() -> Self {
        Self {
            layout: Default::default(),
            selection: None,
        }
    }
}

// Keep framing (target and distance) in the document, so opening another
// project never inherits the previous model's position or scale.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct CameraPreferences {
    projection: Projection,
    fov: f64,
    yaw: f64,
    pitch: f64,
}

impl Default for CameraPreferences {
    fn default() -> Self {
        Self::capture(&Camera::default())
    }
}

impl CameraPreferences {
    pub(super) fn capture(camera: &Camera) -> Self {
        Self {
            projection: camera.projection,
            fov: camera.fov,
            yaw: camera.yaw,
            pitch: camera.pitch,
        }
    }

    pub(super) fn restore(&self, camera: &mut Camera) {
        let candidate = Camera {
            projection: self.projection,
            fov: self.fov,
            yaw: self.yaw,
            pitch: self.pitch,
            ..camera.clone()
        };
        if candidate.valid() {
            *camera = candidate;
        }
    }
}

#[derive(Clone, Default)]
struct SectionMemory(
    std::collections::BTreeMap<String, bool>,
    std::collections::BTreeMap<String, egui::Id>,
    std::collections::BTreeMap<String, String>,
);

#[derive(Clone, Default)]
struct ScrollMemory {
    positions: std::collections::BTreeMap<String, [f32; 2]>,
    scopes: std::collections::BTreeMap<String, egui::Id>,
}

#[derive(Clone, Default)]
struct ModalSizes(std::collections::BTreeMap<String, [f32; 2]>);

fn modal_sizes_id() -> egui::Id {
    egui::Id::unique((PREFERENCE_KEY, "modal_sizes"))
}

pub(crate) fn modal_size(ctx: &egui::Context, key: &str) -> Option<Vec2> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ModalSizes>(modal_sizes_id())
            .0
            .get(key)
            .copied()
    })
    .filter(|size| size.iter().all(|value| value.is_finite() && *value > 0.))
    .map(|[x, y]| egui::vec2(x, y))
}

pub(crate) fn remember_modal_size(ctx: &egui::Context, key: &str, size: Vec2) {
    if size.is_finite() && size.x > 0. && size.y > 0. {
        ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<ModalSizes>(modal_sizes_id())
                .0
                .insert(key.to_owned(), [size.x, size.y]);
        });
    }
}

fn scroll_memory_id() -> egui::Id {
    egui::Id::unique((PREFERENCE_KEY, "scroll_positions"))
}

fn valid_scroll_position(position: [f32; 2]) -> [f32; 2] {
    position.map(|value| if value.is_finite() { value.max(0.) } else { 0. })
}

// Restore only when entering a new UI scope, so normal wheel input, dragging,
// and kinetic scrolling remain owned by egui. Keys are independent of layout.
pub(super) fn restored_scroll_area(
    ui: &egui::Ui,
    key: &str,
    area: egui::ScrollArea,
) -> egui::ScrollArea {
    let position = ui.ctx().data_mut(|data| {
        let memory = data.get_temp_mut_or_default::<ScrollMemory>(scroll_memory_id());
        let previous_scope = memory.scopes.insert(key.to_owned(), ui.scope_id());
        (previous_scope != Some(ui.scope_id()))
            .then(|| memory.positions.get(key).copied())
            .flatten()
    });
    if let Some(position) = position {
        area.scroll_offset(egui::vec2(position[0], position[1]))
    } else {
        area
    }
}

pub(super) fn remember_scroll(ctx: &egui::Context, key: &str, offset: Vec2) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ScrollMemory>(scroll_memory_id())
            .positions
            .insert(key.to_owned(), valid_scroll_position([offset.x, offset.y]));
    });
}

fn section_memory_id() -> egui::Id {
    egui::Id::unique(PREFERENCE_KEY)
}

// All inspector disclosures (including future ones) use these helpers, with a
// stable key independent of their position, title, and docked/floating layout.
// Editable items keep opaque disclosure keys when their label or position changes.
pub(super) fn item_section_key(ctx: &egui::Context, alias: &str) -> String {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<SectionMemory>(section_memory_id())
            .2
            .entry(alias.to_owned())
            .or_insert_with(|| format!("inspector/item/{}", ketchup_core::MaterialId::new_v4()))
            .clone()
    })
}

pub(super) fn rename_item_section(ctx: &egui::Context, from: &str, to: &str) {
    ctx.data_mut(|data| {
        let memory = data.get_temp_mut_or_default::<SectionMemory>(section_memory_id());
        if let Some(key) = memory.2.remove(from) {
            memory.2.insert(to.to_owned(), key);
        }
    });
}

pub(super) fn restore_section(ctx: &egui::Context, id: egui::Id, key: &str, default_open: bool) {
    let open = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<SectionMemory>(section_memory_id())
            .0
            .get(key)
            .copied()
    });
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, default_open);
    if let Some(open) = open {
        state.set_open(open);
    }
    state.store(ctx);
}

// CollapsingHeader owns its internal UI scope. Force the remembered value only
// when entering a different scope, then let egui handle clicks normally.
pub(super) fn restored_header_open(ui: &egui::Ui, key: &str, default_open: bool) -> Option<bool> {
    ui.ctx().data_mut(|data| {
        let memory = data.get_temp_mut_or_default::<SectionMemory>(section_memory_id());
        let previous_scope = memory.1.insert(key.to_owned(), ui.scope_id());
        (previous_scope != Some(ui.scope_id()))
            .then(|| memory.0.get(key).copied().unwrap_or(default_open))
    })
}

pub(super) fn remember_section(ctx: &egui::Context, id: egui::Id, key: &str) {
    if let Some(state) = egui::collapsing_header::CollapsingState::load(ctx, id) {
        ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<SectionMemory>(section_memory_id())
                .0
                .insert(key.to_owned(), state.is_open());
        });
    }
}

pub(super) fn set_section_open(ctx: &egui::Context, key: &str, open: bool) {
    ctx.data_mut(|data| {
        let memory = data.get_temp_mut_or_default::<SectionMemory>(section_memory_id());
        memory.0.insert(key.to_owned(), open);
        // Apply the new value even to a CollapsingHeader already in this scope.
        memory.1.remove(key);
    });
}

impl KetchupApp {
    pub(super) fn request_outliner_search(&mut self, ctx: &egui::Context) {
        self.inspector = true;
        self.focus_outliner_search = true;
        set_section_open(ctx, "outliner", true);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::unique(OUTLINER_SEARCH_ID)));
        ctx.request_repaint();
    }

    pub(super) fn restore_inspector_preferences(
        &mut self,
        ctx: &egui::Context,
        storage: Option<&dyn eframe::Storage>,
    ) {
        self.inspector_order = restore_order(storage);
        let mut preferences = storage
            .and_then(|storage| eframe::get_value::<InspectorPreferences>(storage, PREFERENCE_KEY))
            .unwrap_or_default();
        self.inspector_layout =
            InspectorLayout::restore(preferences.docking.as_ref(), &self.inspector_order);
        self.inspector_order = self.inspector_layout.order();
        for position in preferences.scroll_positions.values_mut() {
            *position = valid_scroll_position(*position);
        }
        self.sharing.preferences = preferences.sharing.clone();
        self.account.open = preferences.account_open;
        self.account.menu_open = preferences.account_menu_open;
        self.general_settings = preferences.general.clone();
        self.general_settings.normalize();
        preferences.tools.restore(self);
        self.project.recent = preferences.recent_projects.clone();
        self.project.recent.truncate(10);
        self.project.import_format =
            if ["skp", "stl", "obj", "dxf"].contains(&preferences.import_format.as_str()) {
                preferences.import_format.clone()
            } else {
                "skp".into()
            };
        self.joinery = preferences.joinery.clone();
        self.joinery.sanitize();
        self.fasteners = preferences.fasteners.clone();
        self.fasteners.sanitize();
        self.project.export_stl = preferences.export_stl;
        self.time_estimates = preferences.time_estimates.clone();
        self.time_estimates.sanitize();
        self.finish_product_filters = preferences.finish_product_filters.clone();
        self.stock_material_filter = preferences.stock_material_filter.clone();
        self.wood_type_filters = preferences.wood_type_filters.clone();
        self.inspector = preferences.visible;
        self.inspector_width = if preferences.width.is_finite() {
            preferences.width.clamp(260., 2000.)
        } else {
            300.
        };
        self.component_filter = preferences.component_filter.clone();
        self.outliner_filter = preferences.outliner_filter.clone();
        self.shortcut_filter = preferences.shortcut_filter.clone();
        self.paint_color = preferences.paint_color;
        self.paint_target = preferences.paint_target;
        self.screenshot.settings = preferences.screenshot.clone();
        self.project.cutting_plan_layout = if preferences.cutting_plan.layout.validate().is_ok() {
            preferences.cutting_plan.layout
        } else {
            Default::default()
        };
        self.project.cutting_plan_selection_preference = preferences.cutting_plan.selection;
        if let Some(snapping) = &preferences.snapping {
            self.snap_to_edge = snapping.geometry && snapping.edges;
            self.snap_to_face = snapping.geometry && snapping.faces;
            self.snap_to_face_center = snapping.geometry && snapping.face_centers;
            self.doc.snap_to_axes = snapping.axes;
        }
        self.sync_geometry_snapping();
        self.snap_to_grid = preferences
            .snapping
            .as_ref()
            .is_some_and(|snapping| snapping.grid);
        self.release_notes.last_seen_version = preferences.last_seen_release_notes.clone();
        if let Some(display) = &preferences.display
            && display.valid()
        {
            self.doc.display = display.clone();
        }
        if let Some(camera) = &preferences.camera {
            camera.restore(&mut self.doc.camera);
        }
        self.viewports = viewports::Workspace::restore(
            &preferences.viewports,
            &mut self.doc.camera,
            &self.doc.viewport_cameras,
        );
        ctx.data_mut(|data| {
            data.insert_temp(
                modal_sizes_id(),
                ModalSizes(preferences.modal_sizes.clone()),
            );
            data.insert_temp(
                scroll_memory_id(),
                ScrollMemory {
                    positions: preferences.scroll_positions.clone(),
                    scopes: Default::default(),
                },
            );
            data.insert_temp(
                section_memory_id(),
                SectionMemory(
                    preferences.sections.clone(),
                    Default::default(),
                    preferences.section_aliases.clone(),
                ),
            )
        });
        self.inspector_preferences = preferences;
        self.sync_stock_materials(ctx);
    }

    fn current_inspector_preferences(&self, ctx: &egui::Context) -> InspectorPreferences {
        InspectorPreferences {
            joinery: self.joinery.clone(),
            fasteners: self.fasteners.clone(),
            sharing: self.sharing.preferences.clone(),
            account_open: self.account.open,
            account_menu_open: self.account.menu_open,
            general: self.general_settings.clone(),
            visible: self.inspector,
            width: self.inspector_width,
            // Capture the effective layout even before the inspector first renders.
            docking: InspectorLayout::restore(
                self.inspector_layout.dock.as_ref(),
                &self.inspector_order,
            )
            .dock,
            component_filter: self.component_filter.clone(),
            outliner_filter: self.outliner_filter.clone(),
            shortcut_filter: self.shortcut_filter.clone(),
            paint_color: self.paint_color,
            paint_target: self.paint_target,
            tools: ToolPreferences::capture(self),
            snapping: Some(SnappingPreferences {
                geometry: true,
                edges: self.doc.snap_to_geometry && self.snap_to_edge,
                faces: self.doc.snap_to_geometry && self.snap_to_face,
                face_centers: self.doc.snap_to_geometry && self.snap_to_face_center,
                axes: self.doc.snap_to_axes,
                grid: self.snap_to_grid,
            }),
            cutting_plan: CuttingPlanPreferences {
                layout: self.project.cutting_plan_layout,
                selection: self.project.cutting_plan_selection_preference,
            },
            time_estimates: self.time_estimates.clone(),
            finish_product_filters: self.finish_product_filters.clone(),
            stock_material_filter: self.stock_material_filter.clone(),
            wood_type_filters: self.wood_type_filters.clone(),
            last_seen_release_notes: self.release_notes.last_seen_version.clone(),
            display: Some(self.doc.display.clone()),
            camera: Some(CameraPreferences::capture(&self.doc.camera)),
            viewports: self.viewports.capture(&self.doc.camera),
            screenshot: self.screenshot.settings.clone(),
            recent_projects: self.project.recent.clone(),
            import_format: self.project.import_format.clone(),
            export_stl: self.project.export_stl,
            section_aliases: ctx.data_mut(|data| {
                data.get_temp_mut_or_default::<SectionMemory>(section_memory_id())
                    .2
                    .clone()
            }),
            sections: ctx.data_mut(|data| {
                data.get_temp_mut_or_default::<SectionMemory>(section_memory_id())
                    .0
                    .clone()
            }),
            modal_sizes: ctx.data_mut(|data| {
                data.get_temp_mut_or_default::<ModalSizes>(modal_sizes_id())
                    .0
                    .clone()
            }),
            scroll_positions: ctx.data_mut(|data| {
                data.get_temp_mut_or_default::<ScrollMemory>(scroll_memory_id())
                    .positions
                    .clone()
            }),
        }
    }

    pub(super) fn save_inspector_preferences(&self, storage: &mut dyn eframe::Storage) {
        // Preserve unreadable settings before saving defaults over them. A
        // future migration failure must not destroy the user's catalogs.
        if let Some(raw) = storage.get_string(PREFERENCE_KEY)
            && eframe::get_value::<InspectorPreferences>(storage, PREFERENCE_KEY).is_none()
        {
            storage.set_string("ketchup.inspector.unreadable-backup.v1", raw);
        }
        eframe::set_value(storage, PREFERENCE_KEY, &self.inspector_preferences);
        save_order(&self.inspector_order, storage);
    }

    pub(super) fn persist_inspector_changes<'storage>(
        &mut self,
        ctx: &egui::Context,
        storage: Option<&mut (dyn eframe::Storage + 'storage)>,
        order_changed: bool,
    ) {
        let preferences = self.current_inspector_preferences(ctx);
        let changed = preferences != self.inspector_preferences;
        self.inspector_preferences = preferences;
        if (changed || order_changed)
            && let Some(storage) = storage
        {
            self.save_inspector_preferences(storage);
        }
        // Selection alone must survive an immediate reload, without marking
        // the model modified or waiting for the document autosave delay.
        if self.sync_selection() {
            self.persist();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum InspectorSection {
    Outliner,
    EntityInfo,
    Components,
    Tags,
    Display,
    View,
    Units,
    TimeCostEstimates,
    Materials,
    History,
    // Legacy serialized section retained only to migrate saved layouts.
    Fasteners,
}

pub(super) fn restore_order(
    storage: Option<&dyn eframe::Storage>,
) -> [InspectorSection; InspectorSection::COUNT] {
    let Some(saved) = storage.and_then(|storage| {
        eframe::get_value::<Vec<InspectorSection>>(storage, ORDER_PREFERENCE_KEY)
            // The original preference used a fixed five-element RON tuple.
            .or_else(|| {
                eframe::get_value::<[InspectorSection; 5]>(storage, ORDER_PREFERENCE_KEY)
                    .map(Vec::from)
            })
    }) else {
        return InspectorSection::ALL;
    };
    let mut order = Vec::new();
    for section in saved {
        if section==InspectorSection::Fasteners {continue;}
        if order.contains(&section) {
            return InspectorSection::ALL;
        }
        order.push(section);
    }
    // Append missing sections in introduction order, regardless of their default position.
    for section in [
        InspectorSection::Outliner,
        InspectorSection::EntityInfo,
        InspectorSection::Components,
        InspectorSection::Tags,
        InspectorSection::Display,
        InspectorSection::View,
        InspectorSection::Units,
        InspectorSection::TimeCostEstimates,
        InspectorSection::Materials,
        InspectorSection::History,
    ] {
        if !order.contains(&section) {
            order.push(section);
        }
    }
    order.try_into().unwrap_or(InspectorSection::ALL)
}

pub(super) fn save_order(
    order: &[InspectorSection; InspectorSection::COUNT],
    storage: &mut dyn eframe::Storage,
) {
    eframe::set_value(storage, ORDER_PREFERENCE_KEY, &order.to_vec());
    storage.flush();
}

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod preference_tests;

impl InspectorSection {
    pub(super) const COUNT: usize = 10;

    pub(super) const ALL: [Self; Self::COUNT] = [
        Self::EntityInfo,
        Self::Outliner,
        Self::Units,
        Self::Components,
        Self::Tags,
        Self::Display,
        Self::View,
        Self::TimeCostEstimates,
        Self::Materials,
        Self::History,
    ];

    fn default_open(self) -> bool {
        matches!(self, Self::EntityInfo | Self::Outliner)
    }

    fn title(self) -> &'static str {
        match self {
            Self::Outliner => "Outliner",
            Self::EntityInfo => "Properties",
            Self::Components => "Components",
            Self::Tags => "Tags",
            Self::Display => "Display",
            Self::View => "View",
            Self::Units => "Units",
            Self::TimeCostEstimates => "Time & Cost Estimates",
            Self::Materials => "Materials",
            Self::History => "History",
            Self::Fasteners => "Automatic Fasteners",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Outliner => "outliner",
            Self::EntityInfo => "entity_info",
            Self::Components => "components",
            Self::Tags => "tags",
            Self::Display => "display",
            Self::View => "view",
            Self::Units => "units",
            Self::TimeCostEstimates => "time_cost_estimates",
            Self::Materials => "materials",
            Self::History => "history",
            Self::Fasteners => "connections/fasteners",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

fn face_style_preview(
    painter: &egui::Painter,
    rect: Rect,
    style: FaceStyle,
    colors: Palette,
    faces: FacePalette,
) {
    let center = rect.center();
    let scale = rect.width().min(rect.height()) * 0.46;
    let point = |x, y| center + egui::vec2(x * scale, y * scale);
    let top = point(0., -1.);
    let left = point(-0.9, -0.55);
    let front = point(0., -0.1);
    let right = point(0.9, -0.55);
    let bottom_left = point(-0.9, 0.55);
    let bottom = point(0., 1.);
    let bottom_right = point(0.9, 0.55);
    let outline = Stroke::new(1.2, colors.ink);
    if style == FaceStyle::Wireframe {
        let back = point(0., 0.1);
        for end in [top, bottom_left, bottom_right] {
            painter.line_segment([back, end], Stroke::new(1., colors.muted));
        }
    } else {
        let fills = match style {
            FaceStyle::HiddenLine => [colors.paper; 3],
            FaceStyle::Monochrome => [faces.front, faces.side, faces.back],
            FaceStyle::Materials => faces.materials,
            FaceStyle::Shaded => [faces.front, faces.side, faces.shaded_side],
            FaceStyle::Wireframe => unreachable!(),
        };
        for (points, fill) in [
            ([top, right, front, left], fills[0]),
            ([left, front, bottom, bottom_left], fills[1]),
            ([front, right, bottom_right, bottom], fills[2]),
        ] {
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                fill,
                Stroke::NONE,
            ));
        }
    }
    painter.add(egui::Shape::closed_line(
        vec![top, right, bottom_right, bottom, bottom_left, left],
        outline,
    ));
    for end in [left, right, bottom] {
        painter.line_segment([front, end], outline);
    }
}

fn face_style_picker(ui: &mut egui::Ui, current: &mut FaceStyle) {
    let colors = Palette::for_ui(ui);
    let faces = FacePalette::for_ui(ui);
    let spacing = ui.spacing().item_spacing.x;
    let columns = ((ui.available_width() + spacing) / (96. + spacing))
        .floor()
        .clamp(1., FaceStyle::ALL.len() as f32) as usize;
    ui.push_id("face_style", |ui| {
        for row in FaceStyle::ALL.chunks(columns) {
            let width =
                (ui.available_width() - spacing * (row.len() - 1) as f32) / row.len() as f32;
            ui.horizontal(|ui| {
                for &style in row {
                    let selected = *current == style;
                    let mut response = ui
                        .add_sized(
                            egui::vec2(width, 100.),
                            egui::Button::new("").selected(selected).corner_radius(5),
                        )
                        .on_hover_text(style.label());
                    if response.clicked() && !selected {
                        *current = style;
                        response.mark_changed();
                    }
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::RadioButton,
                            ui.is_enabled(),
                            *current == style,
                            style.label(),
                        )
                    });
                    if ui.is_rect_visible(response.rect) {
                        let rect = response.rect;
                        face_style_preview(
                            ui.painter(),
                            Rect::from_center_size(
                                egui::pos2(rect.center().x, rect.top() + 33.),
                                egui::vec2(width - 16., 50.),
                            ),
                            style,
                            colors,
                            faces,
                        );
                        let mut label = egui::text::LayoutJob::simple(
                            style.label().to_owned(),
                            FontId::proportional(11.),
                            colors.ink,
                            width - 12.,
                        );
                        label.halign = egui::Align::Center;
                        let galley = ui.fonts_mut(|fonts| fonts.layout_job(label));
                        let pos = egui::pos2(
                            rect.center().x,
                            rect.bottom() - 21. - galley.size().y * 0.5,
                        );
                        ui.painter().galley(pos, galley, colors.ink);
                    }
                }
            });
        }
    });
}

fn inspector_icon_color(ui: &egui::Ui, id: &str) -> Color32 {
    // Keep each section's hue consistent, with lighter strokes on dark surfaces.
    let (light, dark) = match icons::section_icon(id) {
        "outliner" => ([48, 111, 173], [131, 185, 239]),
        "history" => ([117, 105, 155], [181, 169, 219]),
        "entity_info" => ([17, 129, 133], [103, 207, 211]),
        "units" => ([67, 131, 68], [143, 205, 144]),
        "time_cost_estimates" => ([183, 92, 19], [242, 170, 100]),
        "materials" | "connections/wood_joinery" => ([160, 111, 22], [230, 190, 104]),
        "time_estimates_sanding" => ([160, 111, 22], [230, 190, 104]),
        "time_estimates_finishing" | "settings/stock/finishes" => ([126, 83, 183], [188, 153, 239]),
        "time_estimates_pre_stain" => ([17, 129, 133], [103, 207, 211]),
        "time_estimates_stain" => ([183, 92, 19], [242, 170, 100]),
        "time_estimates_varnish" => ([67, 131, 68], [143, 205, 144]),
        "components" => ([126, 83, 183], [188, 153, 239]),
        "tags" => ([160, 111, 22], [230, 190, 104]),
        "display" => ([198, 76, 58], [246, 142, 125]),
        "view" | "view_camera" => ([48, 111, 173], [131, 185, 239]),
        "view_standard_views" => ([17, 129, 133], [103, 207, 211]),
        "edge_style" | "connections/fasteners" => ([88, 109, 135], [159, 181, 209]),
        "sections" => ([176, 67, 113], [237, 142, 181]),
        "shadows" => ([183, 92, 19], [242, 170, 100]),
        "fog" => ([117, 105, 155], [181, 169, 219]),
        "display_environment" => ([67, 131, 68], [143, 205, 144]),
        _ => return Palette::for_ui(ui).green,
    };
    let [r, g, b] = if ui.visuals().dark_mode { dark } else { light };
    Color32::from_rgb(r, g, b)
}

// Shared by regular inspector sections and removable sanding-grit sections.
fn inspector_header_style(ui: &mut egui::Ui, primary: bool, lone: bool) {
    let colors = Palette::for_ui(ui);
    ui.spacing_mut().button_padding = egui::vec2(10., if primary { 10. } else { 7. });
    ui.spacing_mut().indent = if lone { 34. } else { 52. };
    let visuals = ui.visuals_mut();
    visuals.collapsing_header_frame = true;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(if primary { 7 } else { 4 });
        widget.bg_stroke = Stroke::NONE;
        widget.expansion = 0.;
    }
    visuals.widgets.inactive.weak_bg_fill = if primary { colors.canvas } else { colors.paper };
    visuals.widgets.hovered.weak_bg_fill = colors.hovered;
    visuals.widgets.active.weak_bg_fill = colors.selected;
}

fn inspector_disclosure_icon(ui: &egui::Ui, center: Pos2, openness: f32, icon: &str) {
    let colors = Palette::for_ui(ui);
    icons::line_icon(
        ui.painter(),
        center + egui::vec2(24., 0.),
        icon,
        inspector_icon_color(ui, icon),
    );
    let rotation = egui::emath::Rot2::from_angle(openness * std::f32::consts::FRAC_PI_2);
    let points = [
        egui::vec2(-2., -4.),
        egui::vec2(2., 0.),
        egui::vec2(-2., 4.),
    ]
    .map(|point| center + rotation * point);
    ui.painter().add(egui::Shape::line(
        points.to_vec(),
        Stroke::new(
            1.6,
            if openness > 0. {
                colors.green
            } else {
                colors.muted
            },
        ),
    ));
}

// Keep egui's disclosure behavior, animation, and accessibility while giving
// inspector groups a shared surface and a full-width click target.
pub(super) fn inspector_section_with_remove(
    ui: &mut egui::Ui,
    title: &str,
    key: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    let colors = Palette::for_ui(ui);
    let mut remove = false;
    ui.push_id(key, |ui| {
        egui::Frame::new()
            .fill(colors.surface)
            .stroke(Stroke::new(1., colors.border))
            .corner_radius(5)
            .inner_margin(1)
            .show(ui, |ui| {
                let id = ui.make_persistent_id(key);
                restore_section(ui.ctx(), id, key, true);
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    true,
                );
                ui.set_min_width(ui.available_width());
                ui.scope(|ui| {
                    let button_style = ui.style().as_ref().clone();
                    inspector_header_style(ui, false, false);
                    let height = (ui.text_style_height(&egui::TextStyle::Button)
                        + ui.spacing().button_padding.y * 2.)
                        .max(32.);
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), height),
                        Sense::hover(),
                    );
                    let remove_rect = Rect::from_center_size(
                        egui::pos2(rect.right() - 22., rect.center().y),
                        Vec2::splat(32.),
                    );
                    let header_rect = Rect::from_min_max(
                        rect.min,
                        egui::pos2(
                            remove_rect.left() - ui.spacing().item_spacing.x,
                            rect.bottom(),
                        ),
                    );
                    let mut header = ui.interact(header_rect, id, Sense::click());
                    if header.clicked() {
                        state.toggle(ui);
                        header.mark_changed();
                    }
                    header.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::DisclosureTriangle,
                            ui.is_enabled(),
                            title,
                        )
                    });
                    ui.ctx().accesskit_node_builder(id, |node| {
                        node.set_expanded(state.is_open());
                    });
                    let visuals = ui.style().interact(&header);
                    ui.painter()
                        .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
                    let galley = egui::WidgetText::from(RichText::new(title).strong()).into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        (header_rect.width() - ui.spacing().indent).max(0.),
                        egui::TextStyle::Button,
                    );
                    ui.painter().galley(
                        egui::pos2(
                            rect.left() + ui.spacing().indent,
                            rect.center().y - galley.size().y * 0.5,
                        ),
                        galley,
                        visuals.text_color(),
                    );
                    inspector_disclosure_icon(
                        ui,
                        egui::pos2(rect.left() + 14., rect.center().y),
                        state.openness(ui.ctx()),
                        "time_estimates_sanding",
                    );
                    *ui.style_mut() = button_style;
                    // Match the section menu's 32px target and 24px painted outline.
                    let widgets = &mut ui.visuals_mut().widgets;
                    widgets.inactive.expansion = -4.;
                    widgets.hovered.expansion = -4.;
                    widgets.active.expansion = -4.;
                    widgets.open.expansion = -4.;
                    remove = ui
                        .put(
                            remove_rect,
                            egui::Button::new(RichText::new("×").color(colors.muted))
                                .min_size(remove_rect.size()),
                        )
                        .on_hover_text("Remove this grit")
                        .clicked();
                });
                state.show_body_unindented(ui, |ui| {
                    egui::Frame::new().inner_margin(10).show(ui, contents);
                });
                remember_section(ui.ctx(), id, key);
            });
    });
    ui.add_space(1.);
    remove
}

pub(super) fn inspector_section<R>(
    ui: &mut egui::Ui,
    title: &str,
    id: &'static str,
    primary: bool,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<egui::Response> {
    inspector_section_sized(ui, title, id, primary, None, None, contents)
}

// A lone docked section fills its area and has a fixed-open, draggable header.
// Its usual disclosure state is left untouched for when it rejoins a stack.
fn inspector_section_sized<R>(
    ui: &mut egui::Ui,
    title: &str,
    id: &'static str,
    primary: bool,
    fill_size: Option<Vec2>,
    sticky_viewport: Option<Rect>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<egui::Response> {
    let default_open = InspectorSection::ALL
        .into_iter()
        .find(|section| section.id() == id)
        .map_or(primary, InspectorSection::default_open);
    let body_margin = if id == "outliner" {
        egui::Margin {
            bottom: 10,
            ..egui::Margin::same(0)
        }
    } else {
        egui::Margin::same(10)
    };
    let header_action_width = if matches!(id, "materials" | "time_cost_estimates") {
        44.
    } else {
        0.
    };
    let colors = Palette::for_ui(ui);
    let icon_color = inspector_icon_color(ui, id);
    let body_style = ui.style().as_ref().clone();
    let frame = egui::Frame::new()
        .fill(colors.surface)
        .stroke(Stroke::new(1., colors.border))
        .corner_radius(if primary { 8 } else { 5 })
        .inner_margin(1)
        .shadow(if primary {
            egui::epaint::Shadow {
                offset: [0, 2],
                blur: 5,
                spread: 0,
                color: Color32::from_black_alpha(12),
            }
        } else {
            egui::epaint::Shadow::NONE
        });
    let content_size = fill_size.map(|size| (size - frame.total_margin().sum()).max(Vec2::ZERO));
    let response = frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if let Some(size) = content_size {
            ui.set_min_size(size);
        }
        ui.scope(|ui| {
            inspector_header_style(ui, primary, fill_size.is_some());
            if id == "outliner" {
                ui.spacing_mut().item_spacing.y = 0.;
            }
            if let Some(viewport) = sticky_viewport {
                let header_style = ui.style().clone();
                let height = (ui.text_style_height(&egui::TextStyle::Button)
                    + ui.spacing().button_padding.y * 2.)
                    .max(ui.spacing().interact_size.y);
                let (_, natural_rect) = ui.allocate_space(egui::vec2(ui.available_width(), height));
                let header_id = ui.make_persistent_id(id);
                let mut state = fill_size.is_none().then(|| {
                    restore_section(ui.ctx(), header_id, id, default_open);
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        header_id,
                        default_open,
                    )
                });
                let openness = state.as_ref().map_or(1., |state| state.openness(ui.ctx()));
                *ui.style_mut() = body_style;
                let body = |ui: &mut egui::Ui| {
                    egui::Frame::new().inner_margin(body_margin).show(ui, |ui| {
                        if fill_size.is_some() {
                            ui.set_min_size(ui.available_size());
                        }
                        contents(ui)
                    });
                };
                if let Some(state) = &mut state {
                    state.show_body_unindented(ui, body);
                } else {
                    body(ui);
                }
                // Constrain the pinned header to its own section, so the next
                // section pushes it out as the end of the body scrolls past.
                let top = natural_rect
                    .top()
                    .max(viewport.top())
                    .min(ui.min_rect().bottom() - height);
                let rect = natural_rect.translate(egui::vec2(0., top - natural_rect.top()));
                let layer = egui::LayerId::new(ui.layer_id().order, header_id.with("sticky"));
                ui.ctx().set_sublayer(ui.layer_id(), layer);
                let mut header_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("sticky_header")
                        .layer_id(layer)
                        .max_rect(rect),
                );
                header_ui.set_clip_rect(viewport.intersect(ui.clip_rect()));
                header_ui.set_style(header_style);
                let interaction_rect = Rect::from_min_max(
                    rect.min,
                    egui::pos2(
                        (rect.right() - header_action_width).max(rect.left()),
                        rect.bottom(),
                    ),
                );
                let mut header =
                    header_ui.interact(interaction_rect, header_id, Sense::click_and_drag());
                header.rect = rect;
                if let Some(state) = &mut state {
                    if header.clicked() {
                        state.toggle(&header_ui);
                        header.mark_changed();
                    }
                    state.store(ui.ctx());
                    remember_section(ui.ctx(), header_id, id);
                    ui.ctx().accesskit_node_builder(header_id, |node| {
                        node.set_expanded(state.is_open());
                    });
                }
                header.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        if state.is_some() {
                            egui::Role::DisclosureTriangle
                        } else {
                            egui::Role::Group
                        },
                        ui.is_enabled(),
                        title,
                    )
                });
                let visuals = header_ui.style().interact(&header);
                header_ui
                    .painter()
                    .rect_filled(rect, 7, visuals.weak_bg_fill);
                let galley = egui::WidgetText::from(RichText::new(title).strong()).into_galley(
                    &header_ui,
                    Some(egui::TextWrapMode::Truncate),
                    (rect.width() - header_ui.spacing().indent - header_action_width).max(0.),
                    egui::TextStyle::Button,
                );
                header_ui.painter().galley(
                    egui::pos2(
                        rect.left() + header_ui.spacing().indent,
                        rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    visuals.text_color(),
                );
                let center = egui::pos2(
                    rect.left() + header_ui.spacing().indent / 2. - 12.,
                    rect.center().y,
                );
                icons::line_icon(
                    header_ui.painter(),
                    egui::pos2(
                        rect.left() + header_ui.spacing().indent - 14.,
                        rect.center().y,
                    ),
                    id,
                    icon_color,
                );
                if state.is_some() {
                    let rotation =
                        egui::emath::Rot2::from_angle(openness * std::f32::consts::FRAC_PI_2);
                    let points = [
                        egui::vec2(-2., -4.),
                        egui::vec2(2., 0.),
                        egui::vec2(-2., 4.),
                    ]
                    .map(|point| center + rotation * point);
                    header_ui.painter().add(egui::Shape::line(
                        points.to_vec(),
                        Stroke::new(
                            1.6,
                            if openness > 0. {
                                colors.green
                            } else {
                                colors.muted
                            },
                        ),
                    ));
                }
                return header;
            }
            if fill_size.is_some() {
                let height = ui.text_style_height(&egui::TextStyle::Button)
                    + ui.spacing().button_padding.y * 2.;
                let (rect, header) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height),
                    Sense::click_and_drag(),
                );
                header.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Group, ui.is_enabled(), title)
                });
                let visuals = ui.style().interact(&header);
                ui.painter().rect_filled(rect, 7, visuals.weak_bg_fill);
                let galley = egui::WidgetText::from(RichText::new(title).strong()).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    (rect.width() - ui.spacing().indent - header_action_width).max(0.),
                    egui::TextStyle::Button,
                );
                ui.painter().galley(
                    egui::pos2(
                        rect.left() + ui.spacing().indent,
                        rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    visuals.text_color(),
                );
                icons::line_icon(
                    ui.painter(),
                    egui::pos2(rect.left() + ui.spacing().indent - 14., rect.center().y),
                    id,
                    icon_color,
                );
                *ui.style_mut() = body_style;
                egui::Frame::new().inner_margin(body_margin).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    contents(ui)
                });
                return header;
            }
            let open = restored_header_open(ui, id, default_open);
            let header = egui::CollapsingHeader::new(RichText::new(title).strong())
                .id_salt(id)
                .default_open(default_open)
                .open(open)
                .icon(move |ui, openness, response| {
                    let center = response.rect.center() - egui::vec2(12., 0.);
                    inspector_disclosure_icon(ui, center, openness, id);
                })
                .show_unindented(ui, |ui| {
                    *ui.style_mut() = body_style;
                    egui::Frame::new()
                        .inner_margin(body_margin)
                        .show(ui, contents)
                        .inner
                })
                .header_response;
            remember_section(ui.ctx(), header.id, id);
            header
        })
        .inner
    });
    if fill_size.is_none() {
        ui.add_space(if primary { 4. } else { 1. });
    }
    response
}

impl KetchupApp {
    pub(super) fn begin_component_placement(&mut self, id: u64, ctx: &egui::Context) {
        if !self
            .doc
            .model
            .objects
            .get(&id)
            .is_some_and(|o| matches!(o.kind, ObjectKind::Component { .. }))
        {
            return;
        }
        let mut model = self.doc.model.copy_entities(&BTreeSet::from([id]));
        // An instance placed from the library starts visible and unlocked.
        model.hidden.remove(&id);
        model.locked.remove(&id);
        model.entity_tags.remove(&id);
        let points = model.vertices.values().map(|v| v.position).chain(
            model
                .dimensions
                .values()
                .flat_map(|dimension| dimension.layout(&model)),
        );
        let mut points = points.chain(model.texts.values().flat_map(|t| t.layout(&model)));
        let Some(first) = points.next() else {
            self.say(ctx, "This component is empty");
            return;
        };
        let anchor = points.fold(first, |lo, point| lo.min(point));
        let clipboard = GeometryClipboard { model, anchor };
        self.set_tool(Tool::Select);
        self.paste_preview = Some(PastePreview {
            clipboard,
            point: self.doc.camera.target,
        });
        self.say(ctx, "Click to place the component · Escape to cancel");
    }

    fn components_contents(&mut self, ui: &mut egui::Ui) {
        let mut definitions = std::collections::BTreeMap::new();
        for object in self.doc.model.objects.values() {
            if let ObjectKind::Component { definition } = object.kind {
                let entry =
                    definitions
                        .entry(definition)
                        .or_insert((object.id, object.name.clone(), 0));
                entry.2 += 1;
            }
        }
        if definitions.is_empty() {
            crate::empty_state::EmptyState::new("No components yet",
                "Select geometry and choose Create Component from the Edit menu.").show(ui);
        } else {
            ui.add(
                egui::TextEdit::singleline(&mut self.component_filter)
                    .hint_text("Search components")
                    .desired_width(f32::INFINITY),
            );
            let query = self.component_filter.to_lowercase();
            let mut rows: Vec<_> = definitions
                .into_values()
                .filter(|(_, name, _)| name.to_lowercase().contains(&query))
                .collect();
            rows.sort_by(|a, b| {
                a.1.to_lowercase()
                    .cmp(&b.1.to_lowercase())
                    .then(a.0.cmp(&b.0))
            });
            if rows.is_empty() {
                crate::empty_state::EmptyState::new("No matching components",
                    "Try another name or clear the search to see all components.").show(ui);
            }
            for (id, name, count) in rows {
                ui.push_id(id, |ui| {
                    egui::Frame::new()
                        .fill(Palette::for_ui(ui).canvas)
                        .inner_margin(8)
                        .corner_radius(4)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.text_label(RichText::new(name).strong());
                            ui.text_label(
                                RichText::new(format!(
                                    "{count} instance{}",
                                    if count == 1 { "" } else { "s" }
                                ))
                                .small()
                                .color(Palette::for_ui(ui).muted),
                            );
                            if ui
                                .add_sized(
                                    egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                                    egui::Button::new("Place instance"),
                                )
                                .clicked()
                            {
                                self.begin_component_placement(id, ui.ctx());
                            }
                        });
                });
            }
        }
        if ui
            .add_enabled(
                !self.selected.is_empty() && !self.doc.model.selection_protected(&self.selected),
                egui::Button::new("Make Component…").min_size(egui::vec2(
                    ui.available_width(),
                    ui.spacing().interact_size.y,
                )),
            )
            .clicked()
        {
            self.context_action(ui.ctx(), ContextAction::Component);
        }
    }

    fn units_contents(&mut self, ui: &mut egui::Ui) {
        Self::unit_settings_contents(ui, &mut self.doc.display.units);
    }

    pub(super) fn unit_settings_contents(
        ui: &mut egui::Ui,
        units: &mut ketchup_core::UnitSettings,
    ) {
        fn precision_picker(
            ui: &mut egui::Ui,
            id: &'static str,
            label: &str,
            precision: &mut Option<u8>,
            default: u8,
        ) {
            let name = |digits: u8| format!("{:.*}", digits as usize, 0.);
            let current = precision
                .unwrap_or(default)
                .min(ketchup_core::units::MAX_DECIMAL_PLACES);
            crate::inspector_table::inspector_field(ui, id, label, |ui| {
                egui::ComboBox::from_id_salt(id)
                    .selected_text(name(current))
                    .width(ui.available_width())
                    .truncate()
                    .show_ui(ui, |ui| {
                        for digits in 0..=ketchup_core::units::MAX_DECIMAL_PLACES {
                            if ui
                                .selectable_label(current == digits, name(digits))
                                .clicked()
                            {
                                *precision = Some(digits);
                            }
                        }
                    })
                    .response
            });
        }

        fn picker<T: Copy + PartialEq>(
            ui: &mut egui::Ui,
            id: &'static str,
            label: &str,
            selected: &mut T,
            choices: &[T],
            name: impl Fn(T) -> &'static str,
        ) {
            let label = ui.text_label(label);
            egui::ComboBox::from_id_salt(id)
                .selected_text(name(*selected))
                .width(ui.available_width())
                .truncate()
                .show_ui(ui, |ui| {
                    for &choice in choices {
                        ui.selectable_value(selected, choice, name(choice));
                    }
                })
                .response
                .labelled_by(label.id);
        }

        picker(
            ui,
            "units_length",
            "Length",
            &mut units.length,
            LengthUnit::ALL,
            LengthUnit::label,
        );
        precision_picker(
            ui,
            "units_length_precision",
            "Length precision",
            &mut units.precision.length,
            3,
        );
        picker(
            ui,
            "units_area",
            "Area",
            &mut units.area,
            AreaUnit::ALL,
            AreaUnit::label,
        );
        precision_picker(
            ui,
            "units_area_precision",
            "Area precision",
            &mut units.precision.area,
            3,
        );
        picker(
            ui,
            "units_volume",
            "Volume",
            &mut units.volume,
            VolumeUnit::ALL,
            VolumeUnit::label,
        );
        precision_picker(
            ui,
            "units_volume_precision",
            "Volume precision",
            &mut units.precision.volume,
            3,
        );
        picker(
            ui,
            "units_angle",
            "Angle",
            &mut units.angle,
            AngleUnit::ALL,
            AngleUnit::label,
        );
        precision_picker(
            ui,
            "units_angle_precision",
            "Angle precision",
            &mut units.precision.angle,
            units.angle.default_precision(),
        );
        ui.add_space(4.);
        ui.text_label(
            RichText::new("Precision controls displayed decimal places. Lengths entered without a unit use the selected length unit.")
                .small()
                .color(Palette::for_ui(ui).muted),
        );
    }

    pub(super) fn view_menu(&mut self, ui: &mut egui::Ui) {
        let before = self.doc.display.clone();
        let d = &mut self.doc.display;
        for (value, label) in [
            (&mut d.axes, "Axes"),
            (&mut d.grid, "Grid"),
            (&mut d.guides, "Guides"),
        ] {
            Self::menu_checkbox(ui, value, label);
        }
        ui.separator();
        for (value, label) in [
            (&mut d.group_names, "Group names"),
            (&mut d.dimensions, "Dimensions"),
            (&mut d.wood_joinery, "Wood Joinery"),
        ] {
            Self::menu_checkbox(ui, value, label);
        }
        ui.separator();
        for (value, label) in [
            (&mut d.hidden_geometry, "Hidden geometry"),
            (&mut d.hidden_objects, "Hidden objects"),
        ] {
            Self::menu_checkbox(ui, value, label);
        }
        ui.separator();
        Self::menu_checkbox(ui, &mut d.x_ray, "X-Ray");
        Self::menu_checkbox(ui, &mut d.color_by_tag, "Color by tag");
        if before != self.doc.display {
            self.changed(ui.ctx());
        }
    }

    fn display_contents(&mut self, ui: &mut egui::Ui) {
        let before = self.doc.display.clone();
        let d = &mut self.doc.display;
        ui.text_label(RichText::new("Face style").strong());
        face_style_picker(ui, &mut d.face_style);
        inspector_section(ui, "Edge style", "edge_style", false, |ui| {
            for (value, label) in [
                (&mut d.edges, "Edges"),
                (&mut d.back_edges, "Back edges"),
                (&mut d.profiles, "Profiles"),
                (&mut d.depth_cue, "Depth cue"),
                (&mut d.endpoints, "Endpoints"),
                (&mut d.extensions, "Extensions"),
            ] {
                ui.checkbox(value, label);
            }
            ui.add_enabled_ui(d.profiles, |ui| {
                // Keep the label above the slider so this nested section fits
                // even at the inspector's minimum width.
                let label = ui.text_label("Profile width");
                ui.add(egui::Slider::new(&mut d.profile_width, 1. ..=8.))
                    .labelled_by(label.id);
            });
        });
        inspector_section(ui, "Shadows", "shadows", false, |ui| {
            ui.checkbox(&mut d.shadows, "Display shadows");
            ui.add_enabled_ui(d.shadows, |ui| {
                ui.checkbox(&mut d.sun_from_time, "Set sun by date and time")
                    .on_hover_text("Place the sun for a date, local solar time and latitude. The green axis points north.");
                if d.sun_from_time {
                    const MONTHS: [&str; 12] = [
                        "January", "February", "March", "April", "May", "June", "July",
                        "August", "September", "October", "November", "December",
                    ];
                    crate::inspector_table::inspector_field(ui, "sun_month", "Month", |ui| {
                        let month = usize::from(d.sun_month.clamp(1, 12)) - 1;
                        egui::ComboBox::from_id_salt("sun_month")
                            .selected_text(MONTHS[month])
                            .show_ui(ui, |ui| {
                                for (index, name) in MONTHS.iter().enumerate() {
                                    ui.selectable_value(&mut d.sun_month, index as u8 + 1, *name);
                                }
                            })
                            .response
                    });
                    let days = ketchup_core::display::days_in_month(d.sun_month.clamp(1, 12));
                    d.sun_day = d.sun_day.clamp(1, days);
                    crate::inspector_table::inspector_field(ui, "sun_day", "Day", |ui| {
                        ui.add(egui::Slider::new(&mut d.sun_day, 1..=days))
                    });
                    crate::inspector_table::inspector_field(ui, "sun_time", "Time", |ui| {
                        ui.add(
                            egui::Slider::new(&mut d.sun_minutes, 0..=24 * 60 - 1)
                                .step_by(5.)
                                .custom_formatter(|m, _| format!("{:02}:{:02}", m as u32 / 60, m as u32 % 60)),
                        )
                    });
                    crate::inspector_table::inspector_field(ui, "sun_latitude", "Latitude", |ui| {
                        ui.add(egui::Slider::new(&mut d.sun_latitude, -90. ..=90.).suffix("°"))
                    });
                    d.apply_solar_time();
                    if d.solar_elevation() < 0. {
                        ui.text_weak("The sun is below the horizon at this time, so shadows use the lowest sun.");
                    }
                }
                ui.add_enabled_ui(!d.sun_from_time, |ui| {
                    crate::inspector_table::inspector_field(ui, "sun_direction", "Direction", |ui| {
                        ui.add(egui::Slider::new(&mut d.sun_angle, 0. ..=360.).suffix("°"))
                    });
                    crate::inspector_table::inspector_field(ui, "sun_elevation", "Elevation", |ui| {
                        ui.add(egui::Slider::new(&mut d.sun_elevation, 5. ..=85.).suffix("°"))
                    });
                });
                crate::inspector_table::inspector_field(ui, "shadow_darkness", "Darkness", |ui| {
                    ui.add(egui::Slider::new(&mut d.shadow_darkness, 0. ..=1.))
                });
            });
        });
        inspector_section(ui, "Fog", "fog", false, |ui| {
            ui.checkbox(&mut d.fog, "Display fog");
            ui.add_enabled_ui(d.fog, |ui| {
                crate::inspector_table::inspector_field(ui, "fog_start", "Start", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut d.fog_start)
                            .speed(0.5)
                            .range(0. ..=1e8)
                            .suffix(" m"),
                    )
                });
                crate::inspector_table::inspector_field(ui, "fog_end", "End", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut d.fog_end)
                            .speed(0.5)
                            .range(0.1..=1e9)
                            .suffix(" m"),
                    )
                });
                d.fog_end = d.fog_end.max(d.fog_start + 0.1);
            });
            crate::inspector_table::inspector_field(
                ui,
                "fog_color",
                "Background / fog color",
                |ui| {
                    ui.spacing_mut().interact_size.x = ui.available_width();
                    crate::environment::background_control(ui, d)
                },
            );
        });
        inspector_section(ui, "Environment", "display_environment", false, |ui| {
            crate::environment::controls(ui, d);
        });
        if ui
            .add_sized(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                egui::Button::new("Reset display"),
            )
            .clicked()
        {
            *d = DisplaySettings {
                units: d.units,
                ..DisplaySettings::default()
            };
        }
        if before != self.doc.display {
            self.changed(ui.ctx());
        }
    }
}

impl KetchupApp {
    fn history_contents(&mut self, ui: &mut egui::Ui) {

        let current = self.doc.history.position();
        let labels: Vec<String> = self.doc.history.attributed_labels().map(|(label, author, timestamp)| {
            let who = author.map(|a| format!("{} ({})", a.name, if a.kind == "guest" { "Guest" } else if a.verified { "Server-verified account" } else { "Unverified" })).unwrap_or_else(|| "Unknown author".into());
            let when = timestamp.map(|t| format!(" · {t} UTC")).unwrap_or_default();
            format!("{label} · {who}{when}")
        }).collect();
        if labels.is_empty() {
            crate::empty_state::EmptyState::new("No modifications yet.",
                "Changes to your project will appear here. Make an edit to start its history.").show(ui);
            return;
        }
        ui.text_label("Hover to preview an edit. Click to restore that state.");
        ui.weak("Up to 100 edits are saved with this project. Editing an earlier state replaces later edits.");
        let latest = labels.len();
        let mut target = None;
        let mut preview = None;
        let scroll = restored_scroll_area(
            ui,
            "history_entries",
            egui::ScrollArea::vertical()
                .id_salt("history_entries")
                .max_height(360.)
                .auto_shrink([false, true]),
        )
        .show(ui, |ui| {
            for (position, label) in std::iter::once((0, "Oldest retained state".to_owned()))
                .chain(
                    labels
                        .into_iter()
                        .enumerate()
                        .map(|(index, label)| (index + 1, label)),
                )
                .rev()
            {
                let suffix = if position == current {
                    " · Current"
                } else if position > current {
                    " · Undone"
                } else {
                    ""
                };
                let response = ui
                    .push_id(position, |ui| {
                        ui.add_sized(
                            [ui.available_width(), 28.],
                            egui::Button::new(format!("{}. {label}{suffix}", latest - position))
                                .selected(position == current)
                                .wrap(),
                        )
                    })
                    .inner;
                if response.hovered() {
                    preview = Some(position);
                }
                if response.clicked() && position != current {
                    target = Some(position);
                }
            }
        });
        remember_scroll(ui.ctx(), "history_entries", scroll.state.clamped_offset());
        for action in self.doc.history.actions().iter().rev().take(5) {
            let who = action.author.as_ref().map(|a| format!("{} ({})", a.name, if a.kind == "guest" { "Guest" } else if a.verified { "Server-verified account" } else { "Unverified" })).unwrap_or_else(|| "Unknown author".into());
            ui.weak(format!("{} · {} · {} UTC", action.action, who, action.timestamp.map(|t| t.to_string()).unwrap_or_else(|| "Unknown time".into())));
        }
        if let Some(target) = target {
            self.navigate_history(ui.ctx(), target);
        } else if let Some(position) = preview {
            self.preview_history(position);
        }
    }
}

#[cfg(test)]
mod face_center_preference_tests {
    use super::*;
    #[derive(Default)]
    struct SavedPreferences(std::collections::BTreeMap<String,String>);
    impl eframe::Storage for SavedPreferences {
        fn get_string(&self,key:&str)->Option<String>{self.0.get(key).cloned()}
        fn set_string(&mut self,key:&str,value:String){self.0.insert(key.into(),value);}
        fn remove_string(&mut self,key:&str){self.0.remove(key);}
        fn flush(&mut self){}
    }

    #[test]
    fn face_center_toggle_persists_without_an_edit_and_defaults_for_older_settings() {
        let ctx=egui::Context::default();
        let mut storage=SavedPreferences::default();
        let mut app=KetchupApp::with_document(Document::default(),None,true);
        assert!(app.snap_to_face_center);
        app.snap_to_face_center=false;
        app.persist_inspector_changes(&ctx,Some(&mut storage),false);
        let mut restored=KetchupApp::with_document(Document::default(),None,true);
        restored.restore_inspector_preferences(&egui::Context::default(),Some(&storage));
        assert!(!restored.snap_to_face_center);
        let older:SnappingPreferences=serde_json::from_str(r#"{"geometry":true,"faces":false}"#).unwrap();
        assert!(older.face_centers);
    }
}
