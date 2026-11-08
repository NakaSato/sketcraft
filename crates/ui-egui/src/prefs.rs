//! Grouped preferences composed with the shared UI kit. Changes apply immediately; Cancel
//! restores the opening snapshot, Apply keeps editing, and Defaults affects the current page.

use crate::ui_kit::BackingTooltipExt as _;
use sketcraft_engine::geom::prelude::*;
use std::collections::BTreeMap;

use egui::{Modifiers, Pos2};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sketcore_record::Vec3;
pub use sketcraft_engine::doc::display_units::{format_length, length_number, length_precision, unit_scale};
use sketcraft_engine::render::Camera;

/// The key the apps keep the preferences under in eframe's storage.
pub const STORAGE_KEY: &str = "sketcraft.prefs";
/// The key earlier releases used. Compatibility shim (docs/compatibility.md): read when the
/// current key is missing, never written.
pub const LEGACY_STORAGE_KEY: &str = "solvecraft.prefs";

/// The stored preferences, from the current key or else the legacy one.
pub fn stored(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get(STORAGE_KEY).or_else(|| get(LEGACY_STORAGE_KEY))
}

mod design;
mod general;
mod graphics;
mod interface;
mod navbar;
mod navigation;
mod sketch;
mod theme_picker;
mod tools;

use crate::SketApp;
use crate::theme::Tokens;
use crate::tool_modifiers::ModifierCombo;
use crate::ui_kit;
use crate::viewport::NavMode;

pub const UNITS: &[&str] = &["mm", "cm", "m", "in", "ft"];
pub const OPENSCAD_NAV: &str = "openscad";
pub const TOUCHPAD_NAV: &str = "touchpad";
/// A mouse button that can drag the camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavButton {
    Left,
    Middle,
    Right,
}

/// Modifier keys a binding needs held (others may be held too).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NavKeys {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl NavKeys {
    const NONE: NavKeys = NavKeys { shift: false, ctrl: false, alt: false };
    const SHIFT: NavKeys = NavKeys { shift: true, ctrl: false, alt: false };
    const CTRL: NavKeys = NavKeys { shift: false, ctrl: true, alt: false };
    const ALT: NavKeys = NavKeys { shift: false, ctrl: false, alt: true };
    const SHIFT_ALT: NavKeys = NavKeys { shift: true, ctrl: false, alt: true };

    /// All of these keys are held (Ctrl is Cmd on macOS).
    fn held(self, m: Modifiers) -> bool {
        (!self.shift || m.shift) && (!self.ctrl || m.ctrl || m.command) && (!self.alt || m.alt)
    }
}

/// One camera binding: dragging `button` with `keys` held does `mode`.
#[derive(Clone, Copy, Debug)]
pub struct NavBinding {
    pub button: NavButton,
    pub keys: NavKeys,
    pub mode: NavMode,
}

const fn bind(button: NavButton, keys: NavKeys, mode: NavMode) -> NavBinding {
    NavBinding { button, keys, mode }
}

/// A mouse mapping: its bindings, most specific first (the first one whose button is dragged
/// with all its keys held wins).
pub struct NavPreset {
    pub id: &'static str,
    pub label: &'static str,
    /// What the mouse does, for the Preferences page.
    pub help: &'static str,
    /// Choosing the preset turns the wheel the other way.
    pub reverse_zoom: bool,
    /// Dragging down zooms in instead of out (independent of the wheel preference).
    pub reverse_drag_zoom: bool,
    /// Choosing the preset sets what a middle double-click does (`None` leaves it as it is).
    pub middle_double_click: Option<&'static str>,
    /// Choosing the preset sets "Right drag orbits" (`None` leaves it as it is).
    pub right_drag_orbits: Option<bool>,
    pub bindings: &'static [NavBinding],
}

impl NavPreset {
    /// What dragging `button` with `mods` held does, if anything.
    pub fn mode(&self, button: NavButton, mods: Modifiers) -> Option<NavMode> {
        self.bindings.iter().find(|b| b.button == button && b.keys.held(mods)).map(|b| b.mode)
    }
}

use NavButton::{Left, Middle, Right};
use NavMode::{Orbit, Pan, Zoom};

/// The mouse mappings. The first is the default.
pub const NAV_PRESETS: &[NavPreset] = &[
    NavPreset {
        id: "fusion",
        label: "Fusion",
        help: "Middle drag pans, Shift+middle orbits, right drag orbits",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[bind(Middle, NavKeys::SHIFT, Orbit), bind(Middle, NavKeys::NONE, Pan), bind(Right, NavKeys::NONE, Orbit)],
    },
    NavPreset {
        id: "solidworks",
        label: "SolidWorks",
        help: "Middle drag orbits, Ctrl+middle pans, Shift+middle zooms, right drag orbits",
        reverse_zoom: true,
        reverse_drag_zoom: false,
        // Picked here, the right button is left to the context menu and mouse gestures (#6492).
        middle_double_click: Some("fit"),
        right_drag_orbits: Some(false),
        bindings: &[
            bind(Middle, NavKeys::CTRL, Pan),
            bind(Middle, NavKeys::SHIFT, Zoom),
            bind(Middle, NavKeys::NONE, Orbit),
            bind(Right, NavKeys::NONE, Orbit),
        ],
    },
    // Public Creo Parametric dynamic-viewing bindings; no product code or assets are used.
    // https://support.ptc.com/help/creo/creo_pma/r12/usascii/fundamentals/fundamentals/About_Dynamic_Viewing.html
    NavPreset {
        id: "creo",
        label: "Creo",
        help: "Middle drag orbits, Shift+middle pans, Ctrl+middle drag vertically zooms (down to zoom in); scroll the wheel to zoom",
        reverse_zoom: false,
        reverse_drag_zoom: true,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[bind(Middle, NavKeys::CTRL, Zoom), bind(Middle, NavKeys::SHIFT, Pan), bind(Middle, NavKeys::NONE, Orbit)],
    },
    NavPreset {
        id: "nx",
        label: "NX",
        // NX's CAD1 scheme drags middle to rotate, middle + left to zoom and middle + right to
        // pan; the bindings here hold one button at a time, so the two-button drags are the
        // middle drag with Ctrl (zoom) and Shift (pan).
        help: "Middle drag orbits, Shift+middle pans (NX's middle + right), Ctrl+middle zooms (NX's middle + left); right drag leaves the camera",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: Some("fit"),
        right_drag_orbits: Some(false),
        bindings: &[bind(Middle, NavKeys::SHIFT, Pan), bind(Middle, NavKeys::CTRL, Zoom), bind(Middle, NavKeys::NONE, Orbit)],
    },
    NavPreset {
        id: "inventor",
        label: "Inventor",
        help: "Middle drag pans, Shift+middle orbits, Ctrl+middle zooms, right drag orbits",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[
            bind(Middle, NavKeys::SHIFT, Orbit),
            bind(Middle, NavKeys::CTRL, Zoom),
            bind(Middle, NavKeys::NONE, Pan),
            bind(Right, NavKeys::NONE, Orbit),
        ],
    },
    NavPreset {
        id: "alias",
        label: "Alias",
        help: "Shift+Alt+left drag orbits, Shift+Alt+middle pans, Shift+Alt+right zooms; middle drag also pans and right drag orbits",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[
            bind(Left, NavKeys::SHIFT_ALT, Orbit),
            bind(Middle, NavKeys::SHIFT_ALT, Pan),
            bind(Right, NavKeys::SHIFT_ALT, Zoom),
            bind(Middle, NavKeys::CTRL, Zoom),
            bind(Middle, NavKeys::NONE, Pan),
            bind(Right, NavKeys::NONE, Orbit),
        ],
    },
    NavPreset {
        id: "maya",
        label: "Maya",
        help: "Alt+left drag orbits, Alt+middle pans, Alt+right zooms; middle drag also pans",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[
            bind(Left, NavKeys::ALT, Orbit),
            bind(Middle, NavKeys::ALT, Pan),
            bind(Right, NavKeys::ALT, Zoom),
            bind(Middle, NavKeys::NONE, Pan),
        ],
    },
    NavPreset {
        id: "blender",
        label: "Blender",
        help: "Middle drag orbits, Shift+middle pans, Ctrl+middle zooms; right drag leaves the camera alone",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[bind(Middle, NavKeys::SHIFT, Pan), bind(Middle, NavKeys::CTRL, Zoom), bind(Middle, NavKeys::NONE, Orbit)],
    },
    NavPreset {
        id: "tinkercad",
        label: "Tinkercad",
        help: "Right drag orbits, middle or Shift+right drag pans",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[bind(Right, NavKeys::SHIFT, Pan), bind(Right, NavKeys::NONE, Orbit), bind(Middle, NavKeys::NONE, Pan)],
    },
    NavPreset {
        id: OPENSCAD_NAV,
        label: "OpenSCAD",
        help: "Left drag on empty space orbits, Shift+left drag box-selects, sketch geometry drags edit; right or middle drag pans, Shift+right or Shift+middle drag zooms",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[
            bind(Left, NavKeys::NONE, Orbit),
            bind(Right, NavKeys::SHIFT, Zoom),
            bind(Middle, NavKeys::SHIFT, Zoom),
            bind(Right, NavKeys::NONE, Pan),
            bind(Middle, NavKeys::NONE, Pan),
        ],
    },
    NavPreset {
        id: "hfss",
        label: "HFSS",
        help: "Alt+left drag pans, Shift+Alt+left zooms, Ctrl+left drag on empty space orbits",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        // Ctrl+left orbit is the drag from empty space every preset has: as a binding it would
        // also take a Ctrl+click on geometry, which adds to the selection.
        bindings: &[bind(Left, NavKeys::SHIFT_ALT, Zoom), bind(Left, NavKeys::ALT, Pan)],
    },
    NavPreset {
        id: TOUCHPAD_NAV,
        label: "Touchpad",
        help: "Hold Shift and move the pointer to pan; hold Alt/Option and move it to orbit. No mouse button is needed. Vertical scroll zooms.",
        reverse_zoom: false,
        reverse_drag_zoom: false,
        middle_double_click: None,
        right_drag_orbits: None,
        bindings: &[bind(Middle, NavKeys::NONE, Pan), bind(Right, NavKeys::NONE, Orbit)],
    },
];

impl NavPreset {
    /// Button-free motion has exact modifiers, unlike the additive drag bindings.
    pub fn motion_mode(&self, mods: Modifiers) -> Option<NavMode> {
        if self.id != TOUCHPAD_NAV || mods.ctrl || mods.command || mods.mac_cmd {
            return None;
        }
        match (mods.shift, mods.alt) {
            (true, false) => Some(Pan),
            (false, true) => Some(Orbit),
            _ => None,
        }
    }

    pub(crate) fn translated_label(&self) -> &str {
        match self.id {
            "creo" => crate::i18n::tr("nav.creo.label"),
            TOUCHPAD_NAV => crate::i18n::tr("nav.touchpad.label"),
            _ => crate::i18n::tr_label(self.label),
        }
    }

    pub(crate) fn translated_help(&self) -> &str {
        if self.id == "creo" { crate::i18n::tr("nav.creo.help") } else { crate::i18n::tr_label(self.help) }
    }
}

impl Prefs {
    /// Touchpad scroll zooms without overwriting the user's preference for other presets.
    pub fn trackpad_pans(&self) -> bool {
        self.nav != TOUCHPAD_NAV && self.trackpad_pan
    }
}

/// The preset with this id (the default for an unknown one).
pub fn nav_preset(id: &str) -> &'static NavPreset {
    NAV_PRESETS.iter().find(|p| p.id == id).unwrap_or(&NAV_PRESETS[0])
}

/// Picks `preset` the way the Preferences page does: its bindings, wheel direction and, when the
/// preset names them, middle double-click and "Right drag orbits".
pub fn choose_nav_preset(prefs: &mut Prefs, preset: &NavPreset) {
    prefs.nav = preset.id.into();
    prefs.zoom_reverse = preset.reverse_zoom;
    if let Some(double_click) = preset.middle_double_click {
        prefs.middle_double_click = double_click.into();
    }
    if let Some(orbits) = preset.right_drag_orbits {
        prefs.right_drag_orbits = orbits;
    }
}
/// Stable control-channel names; presentation labels may be shorter.
pub const PAGES: &[&str] =
    &["General", "Appearance", "Layout", "Graphics", "Design", "Sketch", "Tools", "Navigation", "Shortcuts", "Assistant", "Plugins"];
pub const PAGE_GENERAL: usize = 0;
pub const PAGE_APPEARANCE: usize = 1;
pub const PAGE_LAYOUT: usize = 2;
pub const PAGE_GRAPHICS: usize = 3;
pub const PAGE_DESIGN: usize = 4;
pub const PAGE_SKETCH: usize = 5;
pub const PAGE_TOOLS: usize = 6;
pub const PAGE_NAVIGATION: usize = 7;
pub const PAGE_SHORTCUTS: usize = 8;
pub const PAGE_ASSISTANT: usize = 9;
pub const PAGE_PLUGINS: usize = 10;
/// What Preferences accepts for `parameter_list_rows`: `parameters.complete` answers at most 200.
pub const PARAMETER_LIST_ROWS: std::ops::RangeInclusive<u16> = 5..=200;
const WINDOW_PAGES: &[ui_kit::WindowPage<SketApp>] = &[
    ui_kit::WindowPage {
        icon: "prefs_general",
        label: "ui.general",
        group: None,
        handle: "prefs:page:General",
        scroll: true,
        draw: window_page::<PAGE_GENERAL>,
    },
    ui_kit::WindowPage {
        icon: "prefs_appearance",
        label: "prefs.structure.appearance",
        group: Some("prefs.structure.look"),
        handle: "prefs:page:Appearance",
        scroll: true,
        draw: window_page::<PAGE_APPEARANCE>,
    },
    ui_kit::WindowPage {
        icon: "prefs_layout",
        label: "prefs.structure.layout",
        group: None,
        handle: "prefs:page:Layout",
        scroll: true,
        draw: window_page::<PAGE_LAYOUT>,
    },
    ui_kit::WindowPage {
        icon: "prefs_graphics",
        label: "ui.graphics",
        group: None,
        handle: "prefs:page:Graphics",
        scroll: true,
        draw: window_page::<PAGE_GRAPHICS>,
    },
    ui_kit::WindowPage {
        icon: "prefs_design",
        label: "ui.design",
        group: Some("prefs.structure.work"),
        handle: "prefs:page:Design",
        scroll: true,
        draw: window_page::<PAGE_DESIGN>,
    },
    ui_kit::WindowPage {
        icon: "prefs_sketch",
        label: "prefs.structure.sketch",
        group: None,
        handle: "prefs:page:Sketch",
        scroll: true,
        draw: window_page::<PAGE_SKETCH>,
    },
    ui_kit::WindowPage {
        icon: "prefs_tools",
        label: "prefs.structure.tools",
        group: None,
        handle: "prefs:page:Tools",
        scroll: true,
        draw: window_page::<PAGE_TOOLS>,
    },
    ui_kit::WindowPage {
        icon: "prefs_navigation",
        label: "ui.navigation",
        group: Some("prefs.input"),
        handle: "prefs:page:Navigation",
        scroll: true,
        draw: window_page::<PAGE_NAVIGATION>,
    },
    ui_kit::WindowPage {
        icon: "prefs_shortcuts",
        label: "prefs.shortcuts",
        group: None,
        handle: "prefs:page:Shortcuts",
        scroll: false,
        draw: window_page::<PAGE_SHORTCUTS>,
    },
    ui_kit::WindowPage {
        icon: "prefs_assistant",
        label: "prefs.assistant",
        group: Some("prefs.extend"),
        handle: "prefs:page:Assistant",
        scroll: true,
        draw: window_page::<PAGE_ASSISTANT>,
    },
    ui_kit::WindowPage {
        icon: "prefs_plugins",
        label: "ui.plugins",
        group: None,
        handle: "prefs:page:Plugins",
        scroll: true,
        draw: window_page::<PAGE_PLUGINS>,
    },
];

/// Former page names remain usable through both ui.prefs and scenario handles.
const PAGE_ALIASES: &[(&str, usize)] = &[
    ("AI", PAGE_ASSISTANT),
    ("Interface", PAGE_APPEARANCE),
    ("Interface/navigation bar", PAGE_LAYOUT),
    ("Keyboard Shortcuts", PAGE_SHORTCUTS),
    ("Selection", PAGE_SKETCH),
    ("Sketch & selection", PAGE_SKETCH),
    ("Sketch and selection", PAGE_SKETCH),
];

fn page_path(index: usize) -> String {
    let Some(page) = WINDOW_PAGES.get(index) else { return String::new() };
    let group = WINDOW_PAGES.get(..=index).and_then(|pages| pages.iter().rev().find_map(|page| page.group));
    match group {
        Some(group) => format!("{} > {}", crate::i18n::tr(group), crate::i18n::tr(page.label)),
        None => crate::i18n::tr(page.label).to_owned(),
    }
}

fn search_header(ui: &mut egui::Ui, app: &mut SketApp) {
    let typed = drive_search(ui, app);
    let field = ui_kit::preferences_search_field(ui, egui::Id::unique("prefs_search"), &mut app.prefs_window.filter, crate::i18n::tr("prefs.search"));
    if typed {
        place_search_cursor(ui.ctx(), egui::Id::unique("prefs_search"), app.prefs_window.filter.chars().count());
    }
    crate::scenario::publish_handle("prefs:search", field.rect.center());
    if field.has_focus() {
        crate::prefs_search::note_key_sink(ui.ctx());
    }
    settle_search(app);
    crate::prefs_search::set_reveal(ui.ctx(), app.prefs_window.reveal.clone());
    crate::prefs_search::set_force_scroll(ui.ctx(), !app.prefs_window.filter.trim().is_empty());
}

/// Keys for the search field. Shortcut capture keeps the key for itself.
fn drive_search(ui: &mut egui::Ui, app: &mut SketApp) -> bool {
    let ctx = ui.ctx().clone();
    if crate::keymap::capturing(app) {
        return false;
    }
    let id = egui::Id::unique("prefs_search");
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
    let sink = crate::prefs_search::key_sink(&ctx);
    let mut typed = false;
    if !sink {
        let modifiers = ctx.input(|input| input.modifiers);
        if !modifiers.command && !modifiers.ctrl && !modifiers.alt {
            let mut chars = String::new();
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    let egui::Event::Text(text) = event else { return true };
                    if text.chars().any(char::is_control) {
                        return true;
                    }
                    chars.push_str(text);
                    false
                });
            });
            if !chars.is_empty() {
                app.prefs_window.filter.push_str(&chars);
                ctx.memory_mut(|memory| memory.request_focus(id));
                typed = true;
            }
        }
    }
    let query = app.prefs_window.filter.trim().to_lowercase();
    if query.is_empty() {
        return typed;
    }
    let (nav_query, cursor, hits) = crate::prefs_search::nav(&ctx);
    let fresh = nav_query == query;
    let cursor = if fresh { cursor.min(hits.len().saturating_sub(1)) } else { 0 };
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) && fresh && !hits.is_empty() {
        crate::prefs_search::set_cursor(&ctx, (cursor + 1).min(hits.len() - 1));
    } else if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) && fresh && !hits.is_empty() {
        crate::prefs_search::set_cursor(&ctx, cursor.saturating_sub(1));
    } else if fresh && !hits.is_empty() && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        if let Some(hit) = hits.get(cursor) {
            open_result(app, hit.page, &hit.label);
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
    } else {
        crate::prefs_search::set_cursor(&ctx, cursor);
    }
    if !app.prefs_window.filter.trim().is_empty() && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.prefs_window.filter.clear();
        ctx.memory_mut(|memory| memory.surrender_focus(id));
    }
    typed
}

fn place_search_cursor(ctx: &egui::Context, id: egui::Id, chars: usize) {
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else { return };
    let cursor = egui::text::CCursor::new(chars);
    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(cursor)));
    state.store(ctx, id);
}

fn open_result(app: &mut SketApp, page: usize, label: &str) {
    if page == PAGE_SHORTCUTS {
        crate::keymap::reveal(app);
    }
    if page == PAGE_PLUGINS {
        app.prefs_window.plugin_query.clear();
    }
    app.prefs_window.filter.clear();
    app.prefs_window.search_from = None;
    app.prefs_window.section = page;
    app.prefs_window.land_on = Some(page);
    app.prefs_window.reveal = Some(crate::prefs_search::reveal_for(page, label));
}

/// An empty field returns to the page that was up when the search started.
fn settle_search(app: &mut SketApp) {
    if app.prefs_window.filter.trim().is_empty() {
        if app.prefs_window.reveal.is_none()
            && let Some(page) = app.prefs_window.search_from.take()
        {
            app.prefs_window.section = page;
            app.prefs_window.land_on = Some(page);
        }
    } else if app.prefs_window.search_from.is_none() {
        app.prefs_window.search_from = Some(app.prefs_window.section);
    }
}

/// A page tab leaves search and shows that page on its own.
fn clear_search(app: &mut SketApp) {
    app.prefs_window.filter.clear();
    app.prefs_window.search_from = None;
    app.prefs_window.reveal = None;
}

fn window_page<const INDEX: usize>(ui: &mut egui::Ui, app: &mut SketApp) {
    for (alias, index) in PAGE_ALIASES {
        if let Some(page) = WINDOW_PAGES.get(*index)
            && let Some(rect) = crate::scenario::rect_of(page.handle)
        {
            crate::scenario::publish_target(&format!("prefs:page:{alias}"), rect);
        }
    }
    if let Some(page) = app.prefs_window.land_on.take() {
        if app.prefs_window.section != page {
            crate::keymap::stop_capture(app);
            app.keymap.read_only = false;
        }
        app.prefs_window.section = page;
    } else if app.prefs_window.section != INDEX {
        crate::keymap::stop_capture(app);
        app.keymap.read_only = false;
        app.prefs_window.section = INDEX;
    }
    page(app, ui, &Tokens::get());
}

pub const THEMES: &[(&str, &str)] = &[("dark", "Dark"), ("light", "Light"), ("system", "System")];
pub const MSAA: [u32; 4] = [1, 2, 4, 8];
/// Supersampling factors (#1288): off, 1.5× and 2× the viewport's pixels each way.
pub const SUPERSAMPLING: [f32; 3] = [1.0, 1.5, 2.0];

/// Original view navigators, independent of the mouse navigation preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigatorStyle {
    /// Also what a navigator this version doesn't know reads as (`lenient`).
    #[default]
    Axes,
    Cube,
    /// A body of the user's choice, drawn the cube's size ([`crate::view_gizmo`]); the cube
    /// until one is chosen.
    Custom,
}

impl NavigatorStyle {
    pub const ALL: [Self; 3] = [Self::Axes, Self::Cube, Self::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Axes => "Signed axes",
            Self::Cube => "Cube",
            Self::Custom => "Custom",
        }
    }
}

/// How the custom view gizmo colours its body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigatorTexture {
    /// The navigation cube's colours. Also what a value this version doesn't know reads as
    /// (`lenient`).
    #[default]
    Themed,
    /// The body's own appearance, as the viewport shows it.
    Modeled,
}

/// How the History at the left lists sketches (Preferences: "Sketches", #6489). The timeline
/// along the bottom always keeps the creation order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySketches {
    /// Every sketch is a row of its own, in creation order (how it always was). Also what a
    /// value this version doesn't know reads as (`lenient`).
    #[default]
    InOrder,
    /// A sketch a feature is built from is listed one level in, under that feature.
    UnderFeature,
}

/// How a section hatches the faces it cuts (Preferences: "Section hatch").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionHatch {
    /// Each body in its own hatch (`sketcraft_render::hatch`): 45 or 135 degrees in one of the
    /// theme's hatch colours, touching bodies never alike. Also what a value this version doesn't
    /// know reads as (`lenient`).
    #[default]
    ByPart,
    /// Every cut face in the cap colour at 45 degrees (how it was before by part).
    One,
}

/// What the interface calls the origin planes (Preferences › Interface). Only the names shown
/// change: the planes' ids (`XY`, `XZ`, `YZ`), commands, scripts and saved designs keep theirs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginPlaneNames {
    /// XY, XZ and YZ. Also what a naming this version doesn't know reads as (`lenient`).
    #[default]
    Xyz,
    /// Top for XY, Front for XZ and Right for YZ (Z up), the engine's aliases for them.
    TopFrontRight,
}

impl OriginPlaneNames {
    pub const ALL: [Self; 2] = [Self::Xyz, Self::TopFrontRight];
    /// The origin planes' ids, in the order the Preferences list them.
    pub const PLANES: [&'static str; 3] = ["XY", "XZ", "YZ"];

    /// The name shown for the plane `name`: an origin plane in this naming, in the interface's
    /// language; any other plane (a construction plane) by its own name.
    pub fn label(self, name: &str) -> &str {
        match (self, name) {
            (Self::TopFrontRight, "XY") => crate::i18n::tr("ui.top"),
            (Self::TopFrontRight, "XZ") => crate::i18n::tr("ui.front"),
            (Self::TopFrontRight, "YZ") => crate::i18n::tr("ui.right"),
            _ => name,
        }
    }
}

/// Where a sketch dimension's value is typed (Preferences › Sketch, "Dimension values", #7246).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionInput {
    /// In a box on the dimension (also what a value this version doesn't know reads as).
    #[default]
    View,
    /// In a small window at the dimension, with arrows that step the value (`dim_view::window`).
    Window,
}

/// The lengths (millimetres) and angles (degrees) the dimension window's arrows may step by.
pub const DIMENSION_STEP_MM: std::ops::RangeInclusive<f64> = 0.001..=1000.0;
pub const DIMENSION_STEP_DEG: std::ops::RangeInclusive<f64> = 0.001..=180.0;

/// What an orbit turns around (Preferences: "Orbit around").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbitAround {
    /// The world origin: the view turns about it and keeps it still on screen.
    Origin,
    /// The view's centre: what Pan moves and Fit frames (the default, as it always was).
    #[default]
    ViewCentre,
    /// The model point under the cursor when the drag began.
    Cursor,
}

/// The sole modifier that turns vertical scroll into wheel-style zoom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackpadZoomModifier {
    Cmd,
    Alt,
    Ctrl,
    /// The default: scroll keeps its usual meaning until a key is chosen.
    #[default]
    Off,
}

impl TrackpadZoomModifier {
    pub const ALL: [Self; 4] = [Self::Cmd, Self::Alt, Self::Ctrl, Self::Off];

    pub fn modifiers(self) -> Option<egui::Modifiers> {
        match self {
            Self::Cmd => Some(egui::Modifiers::COMMAND),
            Self::Alt => Some(egui::Modifiers::ALT),
            Self::Ctrl => Some(egui::Modifiers::CTRL),
            Self::Off => None,
        }
    }

    pub fn label_id(self, platform: crate::tool_modifiers::Platform) -> &'static str {
        match self {
            Self::Cmd if platform == crate::tool_modifiers::Platform::Mac => "prefs.trackpad_zoom_modifier.cmd",
            Self::Alt => "prefs.trackpad_zoom_modifier.alt",
            Self::Cmd | Self::Ctrl => "prefs.trackpad_zoom_modifier.ctrl",
            Self::Off => "prefs.trackpad_zoom_modifier.off",
        }
    }
}

/// Where the view goes when a sketch ends (Preferences: "When a sketch ends", #7038).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchExitView {
    /// Back to the view from before the sketch opened.
    Previous,
    /// The view stays where it is (the default since #4909).
    #[default]
    Keep,
    /// Back to the view from before the sketch, unless the sketch view was turned: a view only
    /// panned or zoomed goes back, a turned one stays.
    UnlessRotated,
}

impl SketchExitView {
    pub const ALL: [Self; 3] = [Self::Previous, Self::Keep, Self::UnlessRotated];

    /// The message id of the choice's label.
    pub fn label_id(self) -> &'static str {
        match self {
            Self::Previous => "prefs.sketch_exit_view.previous",
            Self::Keep => "prefs.sketch_exit_view.keep",
            Self::UnlessRotated => "prefs.sketch_exit_view.unless_rotated",
        }
    }

    /// The view to turn to when a sketch ends with the camera at `now`, or `None` to leave it.
    /// `before` is the view from before the sketch opened and `facing` the one it turned to then.
    /// Going back keeps `now`'s projection, so a perspective switch made in the sketch stays.
    pub fn view_after(self, before: Camera, facing: Camera, now: Camera) -> Option<Camera> {
        let back = Camera { fov: now.fov, ..before };
        match self {
            Self::Previous => Some(back),
            Self::Keep => None,
            Self::UnlessRotated => same_orientation(facing, now).then_some(back),
        }
    }
}

/// Whether two cameras look the same way: pans and zooms move a camera without turning it.
/// Half a degree absorbs rounding in the animation that turned the view to face the sketch.
fn same_orientation(a: Camera, b: Camera) -> bool {
    let near = 0.5_f64.to_radians().cos();
    let ((ar, au, ab), (br, bu, bb)) = (a.basis(), b.basis());
    ar.dot(br) > near && au.dot(bu) > near && ab.dot(bb) > near
}

/// How the offer to stand a file up is made when its header names no program known to save Y up
/// (Preferences: "For files from unknown programs", #7717). A file from SolidWorks is asked in the
/// dialog (only the note when it already stands on Z, unless this is Ask); SketCraft's own files
/// are never offered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandUpUnknown {
    /// The "Stand this part up?" dialog for every file from another program, without looking at
    /// how it rests, as before #7717 (a SolidWorks file too).
    Ask,
    /// No offer: the context menu's Turn Up Axis still stands the file up.
    Off,
    /// A small note over the view, only when the file seems to lie on its back, that goes by
    /// itself and never blocks input (the default). A saved value this version doesn't know reads
    /// as this.
    #[default]
    #[serde(other)]
    Note,
}

impl StandUpUnknown {
    pub const ALL: [Self; 3] = [Self::Ask, Self::Note, Self::Off];

    /// The choice as `ui.prefs` and the saved preferences spell it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Note => "note",
            Self::Off => "off",
        }
    }

    /// The message id of the choice's label.
    pub fn label_id(self) -> &'static str {
        match self {
            Self::Ask => "prefs.stand_up_unknown.ask",
            Self::Note => "prefs.stand_up_unknown.note",
            Self::Off => "prefs.stand_up_unknown.off",
        }
    }
}

/// The default largest STEP/import file, in MB (`sketcraft_io::MAX_STEP_BYTES`).
pub const DEFAULT_STEP_MB: u64 = sketcraft_engine::io::limits::FileLimits::DEFAULT.step / sketcraft_engine::io::limits::MB;
/// The default largest design file, in MB (`sketcraft_io::MAX_DESIGN_BYTES`).
pub const DEFAULT_DESIGN_MB: u64 = sketcraft_engine::io::limits::FileLimits::DEFAULT.design / sketcraft_engine::io::limits::MB;

/// The file size limits the preferences ask for (Preferences › General › Largest file to open),
/// brought into range.
pub fn file_limits(p: &Prefs) -> sketcraft_engine::io::limits::FileLimits {
    sketcraft_engine::io::limits::FileLimits::from_mb(p.max_step_file_mb, p.max_design_file_mb).clamped()
}

/// Every read from now on uses the preferred file size limits (unless `--max-file-size` gave
/// this run its own). The browser keeps its defaults.
pub fn apply_file_limits(p: &Prefs) {
    if sketcraft_engine::io::limits::ADJUSTABLE
        && let Err(e) = sketcraft_engine::io::limits::set_preferred(file_limits(p))
    {
        log::warn!("preferences: file size limits: {e}");
    }
}

/// Perspective view's default field of view (about 34°), in radians.
const PERSPECTIVE_FOV: f64 = 0.6;

/// The field of view Perspective view draws with: the preference, or the default when a saved
/// value is outside the accepted range (a hand edit).
pub fn perspective_fov(p: &Prefs) -> f64 {
    if accepted::perspective_fov(p) { p.perspective_fov } else { PERSPECTIVE_FOV }
}

/// Perspective view keeps the target's size when its angle changes (#5364): the distance scales by
/// tan(old/2) / tan(new/2), so the angle changes the depth, not the zoom. Called every frame and
/// after `ui.prefs`, so every way the angle changes (the slider, Cancel, Restore Defaults, a script)
/// goes through it. A parallel view keeps the last perspective angle, and while Render is open the
/// camera is Render's own, so the change waits until the design view is back.
pub(crate) fn follow_view_fov(app: &mut SketApp) {
    let fov = app.view_fov();
    if fov <= 0.0 || crate::render_workspace::active(app) {
        return;
    }
    let old = std::mem::replace(&mut app.prefs_window.drawn_fov, fov);
    if old > 0.0 && old != fov {
        let k = (old * 0.5).tan() / (fov * 0.5).tan();
        app.cam.distance *= k;
        if let Some(anim) = &mut app.cam_anim {
            anim.from.distance *= k;
            anim.to.distance *= k;
        }
    }
}

/// The browser sidebar's first width, and the width [`Prefs::default`] restores; the panel
/// itself clamps what the user drags to its own range.
pub const DEFAULT_BROWSER_WIDTH: f32 = 250.0;

/// The browser's first share of the column it shares with History under it (#7371): the equal
/// split; the column clamps what the user drags so both cards keep their header and two rows.
pub const DEFAULT_BROWSER_SHARE: f32 = 0.5;

/// When a component's bodies sit in a Bodies folder of their own in the browser (#4217): each
/// folder is one more level that pushes deep assemblies right. Only folders inside components
/// fold; the design's own Bodies folder, and a folder hidden with its eye (its eye is the way
/// back), always stay (`browser::bodies_folded`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodiesFolders {
    Always,
    /// A component with one body and no groups lists the body straight under it. Also what a
    /// value this version doesn't know reads as (`lenient`).
    #[default]
    Several,
    /// Components list their bodies and groups straight under them.
    Never,
}

impl BodiesFolders {
    pub const ALL: [Self; 3] = [Self::Always, Self::Several, Self::Never];

    /// Its name in the Preferences (a message id).
    pub fn label_id(self) -> &'static str {
        match self {
            Self::Always => "prefs.bodies_folders_always",
            Self::Several => "prefs.bodies_folders_several",
            Self::Never => "prefs.bodies_folders_never",
        }
    }
}

// The stored fields, their defaults, page resets and simple form controls share this table.
// Omitted pages keep fields such as grid_follows_view and added agents when restoring defaults.
// Accepted control-channel ranges may differ from slider ranges (look_at_speed).
macro_rules! preference_settings {
    ($( $(#[$doc:meta])* $field:ident: $ty:ty = $default:expr $(, $page:expr)? $(, accept($accepted:expr))? $(=> $control:ident($($args:tt)*))? ;)*) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct Prefs { $($(#[$doc])* pub $field: $ty,)* }

        impl Default for Prefs {
            fn default() -> Self { Self { $($field: $default,)* } }
        }

        /// Every stored field and the page whose Defaults resets it (`None`: no page).
        pub(crate) const FIELD_PAGES: &[(&str, Option<usize>)] = &[$((stringify!($field), preference_settings!(@page $($page)?)),)*];

        fn reset_preferences(p: &mut Prefs, page: usize) {
            let d = Prefs::default();
            $($(if page == $page { p.$field = d.$field; })?)*
        }

        mod accepted {
            use super::*;
            $($(pub(super) fn $field(p: &Prefs) -> bool { ($accepted).contains(&p.$field) })?)*
        }

        mod setting {
            use super::*;
            $($(pub(super) fn $field(app: &mut SketApp, ui: &mut egui::Ui) -> egui::Response {
                preference_settings!(@control app, ui, $field, $control($($args)*))
            })?)*
        }
    };
    (@page) => { None };
    (@page $page:expr) => { Some($page) };
    (@control $app:ident, $ui:ident, $field:ident, check($label:literal $(, $help:literal)?)) => {
        ui_kit::checkbox($ui, crate::i18n::tr_label($label), &mut $app.preferences.$field)
            $(.backing_on_hover_text(crate::i18n::tr_label($help)))?
    };
    (@control $app:ident, $ui:ident, $field:ident, check_tr($label:literal)) => {
        ui_kit::checkbox($ui, crate::i18n::tr($label), &mut $app.preferences.$field)
    };
    (@control $app:ident, $ui:ident, $field:ident, check_id($label:literal)) => {
        ui_kit::checkbox($ui, $app.tr($label), &mut $app.preferences.$field)
    };
    (@control $app:ident, $ui:ident, $field:ident, check_id_help($label:literal, $help:literal)) => {
        ui_kit::checkbox($ui, $app.tr($label), &mut $app.preferences.$field).on_hover_text($app.tr($help))
    };
    (@control $app:ident, $ui:ident, $field:ident, number($label:literal, $range:expr)) => {
        ui_kit::row($ui, $app.tr($label), |ui| {
            ui_kit::number_field(ui, &mut $app.preferences.$field, $range, 1.0, "")
        })
    };
    (@control $app:ident, $ui:ident, $field:ident, enabled_slider($enabled:ident, $label:literal, $range:expr, $step:expr, $format:expr)) => {
        ui_kit::row($ui, $app.tr_label($label), |ui| {
            ui.add_enabled_ui($app.preferences.$enabled, |ui| {
                ui_kit::slider(ui, &mut $app.preferences.$field, $range, ui_kit::Slider { step: Some($step), format: $format, ..Default::default() })
            }).inner
        })
    };
    (@control $app:ident, $ui:ident, $field:ident, slider($label:literal, $range:expr, $step:expr, $format:expr, $rect:literal)) => {
        ui_kit::row($ui, $app.tr($label), |ui| {
            let response = ui_kit::slider(ui, &mut $app.preferences.$field, $range, ui_kit::Slider { step: Some($step), format: $format, ..Default::default() });
            crate::scenario::publish_rect($rect, response.rect);
            response
        })
    };
}

preference_settings! {
    /// Units of new designs.
    default_units: String = "mm".into(), PAGE_DESIGN;
    /// Mouse mapping: `fusion`, `solidworks` or `inventor`.
    nav: String = "fusion".into(), PAGE_NAVIGATION;
    /// Opt in to rotation through both poles with roll, without the Shift axis-snap spring.
    /// Independent of mouse bindings; false also preserves existing saved presets' behavior.
    free_orbit: bool = false, PAGE_NAVIGATION;
    /// With free orbit, mouse drags turn sideways about the world's up axis (a turntable without
    /// the pole limit) instead of the screen's.
    free_orbit_upright: bool = false, PAGE_NAVIGATION => check_tr("nav.free_orbit_upright.label");
    /// Off: a right drag that the mouse mapping would orbit leaves the camera alone (#6492).
    right_drag_orbits: bool = true, PAGE_NAVIGATION => check_tr("nav.right_drag_orbits.label");
    /// A middle or right drag keeps the mode it started with until its button is released, as a
    /// left drag with the mapping's keys does: letting go of Shift partway through a Shift+middle
    /// orbit doesn't turn it into a pan (#9557). Off: the keys held decide on every frame, as before.
    nav_drag_keeps_mode: bool = true, PAGE_NAVIGATION => check_tr("nav.drag_keeps_mode.label");
    /// An orbit drag that reaches the edge of the view moves the cursor to the opposite edge and
    /// keeps turning; false stops the drag at the edge of the screen, as before.
    orbit_cursor_wrap: bool = true, PAGE_NAVIGATION => check_tr("nav.orbit_cursor_wrap.label");
    /// View orientation widget: original signed axes (default) or compact cube.
    navigator_style: NavigatorStyle = NavigatorStyle::default(), PAGE_NAVIGATION;
    /// The custom view gizmo's colours: the cube's (themed) or the body's own (modeled).
    navigator_texture: NavigatorTexture = NavigatorTexture::default(), PAGE_NAVIGATION;
    /// The custom view gizmo has the cube's orbit and roll arrows around it.
    navigator_arrows: bool = true, PAGE_NAVIGATION;
    /// The custom view gizmo has the cube's Home button beside it.
    navigator_home: bool = true, PAGE_NAVIGATION;
    /// Show the radial ring and enable hold/flick gestures for viewport context menus.
    radial_context_menu: bool = false, PAGE_NAVIGATION => check("Radial context menu");
    /// Work on the model directly in the viewport (pull a face to extrude or cut); off by default.
    direct_interaction: bool = false, PAGE_TOOLS;
    /// Mouse gestures: a ring of tools on the right button.
    mouse_gestures: crate::mouse_gesture::Config = Default::default(), PAGE_NAVIGATION;
    /// The wheel zooms out when turned forward.
    zoom_reverse: bool = false, PAGE_NAVIGATION => check("Reverse zoom direction");
    /// What an orbit turns around.
    orbit_around: OrbitAround = OrbitAround::default(), PAGE_NAVIGATION;
    /// The cursor preference as releases before the origin option saved it (`true` = the point
    /// under the cursor). [`migrate`] folds it into `orbit_around` when saved preferences are
    /// read, so an existing choice survives the upgrade; this version only writes `false`.
    orbit_cursor: bool = false, PAGE_NAVIGATION;
    /// Anti-aliasing samples (applies on restart).
    msaa: u32 = 4, PAGE_GRAPHICS;
    /// Share shading and Continuity; false preserves their independent legacy behavior.
    single_surface_analysis: bool = true, PAGE_GRAPHICS => check_id("prefs.graphics_page.single_surface_analysis");
    /// Linux: the window system the app opens with at its next start (#3083): `auto` (XWayland
    /// whenever there is an X display, native Wayland on niri), `wayland` or `x11`.
    window_system: String = "auto".into(), PAGE_GRAPHICS;
    /// Exact graphics adapter name; empty selects automatically on the next start.
    graphics_adapter: String = String::new(), PAGE_GRAPHICS;
    /// Soft contact shading in the GPU viewport; optional and applied immediately.
    ambient_occlusion: bool = false, PAGE_GRAPHICS;
    /// Restore the shadow plane at the lowest visible body, including above the origin (#9838).
    ground_shadow_follows_model: bool = false, PAGE_GRAPHICS => check_id("prefs.ground_shadow_follows_model");
    /// Shade the viewport in linear light with a sky fill, so dark parts keep their shape
    /// (#7974). Off by default: the viewport shades as it always has.
    dark_parts_shading: bool = false, PAGE_GRAPHICS => check_id("prefs.dark_parts_shading");
    /// Optional schematic markings on threaded cylindrical walls; off for new and older preferences.
    cosmetic_threads: bool = false, PAGE_GRAPHICS;
    /// A section hatches each cut body its own way (the default), or every one alike, in the cap
    /// colour at 45 degrees, as before.
    section_hatch: SectionHatch = SectionHatch::ByPart, PAGE_GRAPHICS;
    /// The grid turns to the upright origin plane facing a side view (#1470). Off by default: the
    /// grid stays on the ground plane (XY) whatever the view.
    grid_follows_view: bool = false => check_id("ui.grid_follows_view");
    /// Draw reference geometry such as inactive sketches and datum planes through shaded bodies.
    /// Off by default to preserve the depth-tested viewport behaviour.
    reference_geometry_through_bodies: bool = false, PAGE_GRAPHICS => check_id("prefs.graphics_page.reference_geometry_through_bodies");
    /// A small dot at every vertex wherever edges are drawn (#7834). Off by default: the viewport
    /// draws edges only, as it always has.
    vertex_markers: bool = false;
    /// A preview of a feature that changes a body draws the surface it adds in the preview
    /// colour, working out face by face which surface that is. Off: the body is drawn as itself
    /// and nothing is worked out (`preview::Colors::tint_new`).
    preview_tint_new_faces: bool = true, PAGE_GRAPHICS => check_id("prefs.preview_tint_new_faces");
    /// A preview that removes material shows the removed shape as a red see-through ghost. Off:
    /// the ghost is fully transparent and only its outline is drawn (`preview::Colors::cut_tint`).
    cut_preview_tint: bool = true, PAGE_GRAPHICS => check_id("prefs.cut_preview_tint");
    /// Restore the previous platform-specific fixed Render preview geometry budget.
    render_previous_geometry_budget: bool = false, PAGE_GRAPHICS => check_id("prefs.render_previous_geometry_budget");
    /// Restore Raster for new Render setups, fixed CPU sampling and its mode selector.
    /// Refit on entry and keep shot activation camera-neutral, with the previous preview
    /// framing, navigation, picking and selection overlays. Disable automatic live feedback
    /// and automatic result opening; retain the previous result/cancel viewing behavior.
    /// GPU sample edits round to powers of two.
    render_legacy_image_defaults: bool = false, PAGE_GRAPHICS => check_id("prefs.render_legacy_image_defaults");
    /// How finely the viewport tessellates curved faces and edges: `coarse`, `normal`, `fine` or
    /// `ultra` (#1287). Only the display plan the viewport draws; never the design or exports.
    display_quality: String = "normal".into(), PAGE_GRAPHICS;
    /// Supersampling (#1288): the view drawn at this many times its pixels each way and filtered
    /// down (1, 1.5 or 2; 1 is off).
    supersampling: f32 = 1.0, PAGE_GRAPHICS;
    /// Supersample only while the view holds still (full speed while orbiting).
    supersampling_adaptive: bool = true, PAGE_GRAPHICS => check_tr("prefs.graphics_page.supersampling_adaptive");
    /// Show the frame rate in the view's corner (the view then redraws every frame).
    show_frame_rate: bool = false, PAGE_GRAPHICS;
    /// Record where the time goes (SMLib's calls, kernel operations, features, commands, imports)
    /// for `diagnostics.profile` (docs/perf.md). Off by default: nothing is recorded.
    profile_operations: bool = false, PAGE_GRAPHICS => check_tr("prefs.profile_operations");
    /// With the frame rate: a frame-time graph, a histogram and what the GPU drew (#2548).
    frame_rate_details: bool = false, PAGE_GRAPHICS;
    /// Draw a wider rim under finished sketch curves to improve contrast on shaded faces.
    sketch_halo: bool = false, PAGE_GRAPHICS => check("Halo under sketch curves");
    /// Show the view's pitch, yaw and roll and its projection under the orientation gizmo (#6032).
    /// Off by default; View ▸ View angles and `view.angle_readout` turn it on.
    show_view_angles: bool = false, PAGE_GRAPHICS;
    /// Cast directional solar shadows based on sun position (time of day and season).
    solar_shadows: bool = true, PAGE_GRAPHICS => check_tr("prefs.solar_shadows");
    /// Solar time of day in decimal hours (0.0..24.0; default 14.0 = 2:00 PM).
    solar_time_hours: f64 = 14.0, PAGE_GRAPHICS, accept(0.0..=24.0);
    /// Solar day of year (1..365; default 172 = June 21, Summer Solstice).
    solar_day_of_year: u32 = 172, PAGE_GRAPHICS, accept(1..=365);
    /// Solar latitude in degrees (-90.0..90.0; default 37.77 = San Francisco / Mid-latitude).
    solar_latitude_deg: f64 = 37.77, PAGE_GRAPHICS, accept(-90.0..=90.0);
    /// Show world X/Y/Z component dimensions beside the total distance in Measure.
    measure_axis_distances: bool = true, PAGE_TOOLS;
    /// Perspective view's vertical field of view in radians (#5364), 15° to 90°. The view reads a
    /// saved value outside that range as the default ([`perspective_fov`]).
    perspective_fov: f64 = PERSPECTIVE_FOV, PAGE_GRAPHICS, accept(15f64.to_radians()..=90f64.to_radians());
    /// Decimals shown for lengths and angles.
    length_decimals: u8 = 2, PAGE_DESIGN;
    angle_decimals: u8 = 1, PAGE_DESIGN;
    /// The most parameters Down arrow lists in an empty expression box (#9246): fewer keeps the
    /// list light in a large design.
    parameter_list_rows: u16 = 50, PAGE_DESIGN;
    /// Up/Down and the wheel over a focused value field step its plain number (`value_step`,
    /// #2320). Off by default: the keys move the text cursor and the wheel scrolls, as before.
    step_values: bool = false, PAGE_DESIGN => check_id("ui.step_values");
    /// When SMLib's full check runs while features are computed (`diagnostics.validation`):
    /// `each_feature` (the default) checks the bodies each feature keeps once; `each_step`, as
    /// before, checks after every sweep, boolean and blend; `off` (experimental) never does.
    validation: sketcraft_engine::kernel::Validation = sketcraft_engine::kernel::Validation::EachFeature, PAGE_DESIGN;
    /// A command's value (Extrude's Distance, Fillet's Radius…) in a box beside its handle in the
    /// view (`canvas::boxes_shown`). Off: it is typed in the command window only, which takes the
    /// keyboard after a pick instead, and nothing covers the model (#9479).
    value_boxes_in_view: bool = true, PAGE_DESIGN => check_id("prefs.value_boxes_in_view");
    /// The Parameters dialog applies an expression as soon as its field is left (#9279). Off: the
    /// edits wait in the dialog until Apply or OK sets them together, in one rebuild and one undo
    /// step. Its checkbox is in the dialog itself.
    parameters_apply_live: bool = true;
    /// A design coming back to its tab reads again the linked designs whose files changed since
    /// it last read them. Off: they wait for Update Links, as before.
    update_changed_links: bool = true, PAGE_DESIGN => check_id("settings.update_changed_links");
    /// A double-click on a component, in the browser or on its face in the view, opens a linked
    /// one's design with the assembly around it (`edit_in_place`) and activates any other. Off:
    /// a double-click in the browser activates every component and one in the view does nothing,
    /// as before.
    double_click_components: bool = true, PAGE_DESIGN => check_id("settings.double_click_components");
    /// Insert Design lists the designs open in other tabs, each inserted with one click, then
    /// Browse… for a file (#8570). Off: it opens the file picker at once, as before.
    insert_lists_open_designs: bool = true, PAGE_DESIGN => check_id("settings.insert_lists_open_designs");
    /// Reopen the saved designs that were open when the app was last closed (#5542).
    reopen_designs: bool = true, PAGE_GENERAL;
    /// Those designs' files, in tab order, as the last quit left them.
    open_designs: Vec<String> = Vec::new(), PAGE_GENERAL;
    /// Web app (#10220): keep each open design's unsaved changes current in the browser's
    /// private storage, so closing or reloading the tab doesn't ask through the browser's own
    /// "leave site?" box; the next visit offers them back. Off: the browser asks, as before.
    keep_unsaved_in_browser: bool = false, PAGE_GENERAL;
    keep_unsaved_also_asks: bool = true, PAGE_GENERAL;
    /// The largest STEP or import file opened, in MB (#4086): from the default (512) up to 16 GB.
    /// `--max-file-size` overrides it for one run. The browser keeps the default.
    max_step_file_mb: u64 = DEFAULT_STEP_MB, PAGE_GENERAL;
    /// The largest design file opened or saved, in MB (#4086): from the default (256) up to 16 GB.
    max_design_file_mb: u64 = DEFAULT_DESIGN_MB, PAGE_GENERAL;
    /// Explicit assembly Move uses joint handles; off keeps the previous Move panel.
    assembly_move_handles: bool = true, PAGE_TOOLS => check_id("joint_move.use_handles");
    /// Degrees per step near the assembly Move ring.
    joint_rotation_coarse: f64 = 10.0, PAGE_TOOLS;
    /// Degrees per step after pulling the cursor outside the precision guide.
    joint_rotation_fine: f64 = 0.1, PAGE_TOOLS;
    /// `dark`, `light` or `system` (follow the operating system), or a theme file's id: a
    /// built-in one (`gruvbox-dark`, …) or one of `custom_themes` (#2255).
    theme: String = "dark".into(), PAGE_APPEARANCE;
    /// A picture behind the model in place of the view's background; empty for none.
    background_image: String = String::new(), PAGE_APPEARANCE;
    /// Theme used when the operating system is light; unknown ids resolve to Light.
    theme_day: String = "light".into(), PAGE_APPEARANCE;
    /// Theme used when the operating system is dark; unknown ids resolve to Dark.
    theme_night: String = "dark".into(), PAGE_APPEARANCE;
    /// The 3D view's colours: the theme's own, or the Light or Dark theme's whatever the window
    /// uses (#4146).
    viewport_theme: crate::theme::ViewportTheme = crate::theme::ViewportTheme::Theme, PAGE_APPEARANCE;
    /// The 3D view's background, one colour for the whole application (#7688), or `None` (the
    /// default) for the theme's gradient.
    viewport_background: Option<[u8; 3]> = None, PAGE_APPEARANCE;
    /// The application bar's logo in the theme's accent instead of the theme's logo colours.
    logo_accent: bool = false, PAGE_APPEARANCE;
    /// Imported theme files (`theme_files`), kept here so they also work in the browser.
    #[serde(deserialize_with = "themes_that_read")]
    custom_themes: Vec<crate::theme_files::ThemeFile> = Vec::new();
    /// Desktop: a `sketcraft-theme/1` file another tool writes, followed as the theme and
    /// reloaded when it changes (#7573). Empty (the default): no file is followed.
    theme_follow: String = String::new(), PAGE_APPEARANCE;
    /// Per-theme overrides. Raw JSON retains entries from newer or hand-edited settings.
    theme_options: BTreeMap<String, BTreeMap<String, serde_json::Value>> = BTreeMap::new(), PAGE_APPEARANCE;
    /// Interface language: a registered locale ID, or `system` to follow the system's language
    /// on every start (#10138). A saved language keeps winning, so existing users don't change.
    language: String = "system".into(), 0;
    /// Shared workspace composition; system windows and popups retain their purpose.
    layout: crate::shell::LayoutMode = Default::default(), PAGE_LAYOUT;
    /// Ordered view controls and their appearance.
    #[serde(default)]
    navigation_bar: crate::navigation_bar::NavigationBar = Default::default(), PAGE_LAYOUT;
    /// Show command names beneath ribbon icons, independently of workspace layout.
    ribbon_labels: bool = true, PAGE_LAYOUT;
    /// Show engine-derived persistent selection in the ribbon; off retains neutral buttons.
    persistent_ribbon_pressed: bool = true, PAGE_LAYOUT;
    viewport_analysis_status: bool = true, PAGE_GRAPHICS;
    viewport_section_status: bool = true, PAGE_GRAPHICS;
    viewport_status_details: bool = true, PAGE_GRAPHICS;
    viewport_status_summary: bool = true, PAGE_GRAPHICS;
    /// Keep the SKETCH tab in the ribbon, right after MODEL, while no sketch is open; its drawing
    /// tools then start a sketch first (#7904).
    sketch_tab_always: bool = false, PAGE_LAYOUT;
    /// The model screen as SketchUp's (`sketchup_screen`): a sky over a ground, long red, green
    /// and blue axes (solid one way, dotted the other) and no origin planes. Off: the classic view.
    sketchup_screen: bool = true, PAGE_LAYOUT;
    /// Each design tab keeps its open dialog or tool, and its toolbar tab, while another design is
    /// shown, and has them back when it is shown again (#9360). Off by default: a switch cancels
    /// them and shows the MODEL tab.
    keep_work_on_tab_switch: bool = false, PAGE_LAYOUT;
    /// Hovering a result in Search commands shows the command's place in the ribbon: its tab,
    /// its button or its row in a section's drop-down (#7287). A command started from the search
    /// blinks there, and a running command that sits in a drop-down marks its section label. Off
    /// by default: the ribbon stays as it was.
    ribbon_reveal: bool = false, PAGE_LAYOUT;
    /// Fold the ribbon to its tab row, so the view has the room its panels took (#10112). A tab's
    /// click shows its panels over the view until a command starts or the pointer leaves them.
    /// Off by default: the whole ribbon shows. `view.ribbon_tabs_only` switches it.
    ribbon_tabs_only: bool = false, PAGE_LAYOUT;
    /// A double-click on a design tab renames the design in the tab; off, it maximizes the window
    /// as the rest of the title bar does, as before.
    tab_double_click_renames: bool = true, PAGE_LAYOUT;
    /// Where a dialog's OK and Cancel sit: under its inputs (the default) or in its title row (#6708).
    dialog_buttons: crate::ui_kit::DialogButtons = Default::default(), PAGE_LAYOUT;
    /// Where notices and offers stack: the view's bottom-right corner (the default) or the bottom
    /// centre, as before #7893.
    toast_place: crate::viewport::ToastPlace = Default::default(), PAGE_LAYOUT;
    /// Toolbar tabs hidden by the person. An empty list keeps every existing tab visible.
    hidden_toolbar_tabs: Vec<String> = crate::workspace::HIDDEN_BY_DEFAULT.iter().map(|t| t.to_string()).collect(), PAGE_LAYOUT;
    /// Show the ribbon's panel drop-downs as a grid of large icons, their names in the tooltips,
    /// instead of a list of small icons with names (#4739).
    ribbon_icon_grid: bool = false, PAGE_LAYOUT;
    /// Offer the CAD-only tools SketchUp has no match for (Fillet, Shell, Hole, Loft, ...;
    /// `workspace::CAD_ONLY`) in the ribbon, the command search, menus and shortcuts. Off by
    /// default; on gives the old ribbon. Features already in a design stay editable either way.
    show_cad_tools: bool = false, PAGE_LAYOUT;
    /// After a ribbon panel menu is open, hover another panel to switch to its menu.
    ribbon_menus_follow_hover: bool = false, PAGE_LAYOUT;
    /// Leave the commands the ribbon shows as buttons (pinned ones) out of its panel drop-downs,
    /// which then list only the rest (#4739).
    ribbon_menus_skip_pinned: bool = false, PAGE_LAYOUT;
    /// Interface scale (1 = 100 %).
    ui_scale: f32 = 1.0, PAGE_APPEARANCE, accept(0.5..=2.5);
    /// Interface font: an installed font family (or a font file), or empty for the built-in one
    /// (#1290). Menus, dialogs and labels use it; glyphs it lacks fall back as before.
    ui_font: String = String::new(), PAGE_APPEARANCE;
    /// While a dialog or a sketch is open, OK and Cancel in the view's top right corner, with the
    /// axis view moved down below them (#6709). Off by default.
    view_confirm_corner: bool = false, PAGE_LAYOUT;
    /// A thin bar along the window's bottom edge: what is being edited, the open sketch's state
    /// and the design's units (#7243). Off by default.
    status_bar: bool = false, PAGE_LAYOUT;
    /// The selection's length, area or distance in the view's bottom right corner (#7000). Off by
    /// default.
    quick_measure: bool = false, PAGE_GRAPHICS;
    /// Put the feature history to the left of the browser instead of below the viewport.
    timeline_left: bool = false, PAGE_LAYOUT;
    /// Measured outer width; the contents react to this width without a names mode.
    timeline_left_width: f32 = 130.0, PAGE_LAYOUT;
    /// Share the browser column, vertically, instead of a separate left column.
    timeline_under_browser: bool = false, PAGE_LAYOUT;
    /// Compact history: the browser lists each component's features in history order with the
    /// marker, and neither timeline is shown.
    history_in_browser: bool = false, PAGE_LAYOUT;
    /// Fold the shared-column history to its header.
    timeline_collapsed: bool = false, PAGE_LAYOUT;
    /// The browser's share of the column's height when History sits under it, which the user
    /// drags at the seam between the two cards (#7371).
    browser_share: f32 = DEFAULT_BROWSER_SHARE, PAGE_LAYOUT;
    /// Whether the browser beside the vertical history is collapsed to its rail.
    timeline_left_browser_collapsed: bool = false, PAGE_LAYOUT;
    /// Whether the History at the left lists a sketch under the feature built from it.
    history_sketches: HistorySketches = HistorySketches::default(), PAGE_LAYOUT;
    /// A Play History button in the History controls, between Previous and Next (#3688). Off
    /// gives the four controls History had before; right-clicking the controls toggles it too.
    history_play_button: bool = true, PAGE_LAYOUT;
    /// On, two or more joints show as one button with their count in the History, and a click
    /// shows each one (#8884). Off, the default, shows each joint as the History always has.
    history_group_joints: bool = false, PAGE_LAYOUT;
    /// On, the History bar folds long runs of one component's features into chips and starts
    /// feature folders folded, and a STEP insert that brings more than one item goes into a
    /// `Group #N` folder (#8885). Off, the default, shows each feature, starts folders open and
    /// makes no folder, as the bar always has.
    history_bar_groups: bool = false, PAGE_LAYOUT;
    /// The vertical history (left, or under the browser) starts with the active component's Bodies
    /// folder and the Origin, as the browser shows them (#6490). The bottom strip is unchanged.
    timeline_origin_bodies: bool = false, PAGE_LAYOUT;
    /// Width of the browser sidebar, which
    /// the user drags wider or narrower at its right edge.
    browser_width: f32 = DEFAULT_BROWSER_WIDTH, PAGE_LAYOUT;
    /// Render shell preference, independent of the document and Design history.
    render_shots_strip: bool = true;
    render_inspector_side: String = "right".into();
    render_inspector_width: f32 = 305.0;
    render_browser_side: String = "left".into();
    render_browser_width: f32 = 266.0;
    /// Resource identities and history belong to the user, never the design.
    render_library_favorites: std::collections::BTreeSet<(String, String)> = Default::default();
    render_library_recent: std::collections::VecDeque<(String, String)> = Default::default();
    /// Draw the parts browser on a translucent card over the 3D view instead of a column of its
    /// own, so the workspace keeps the whole window width.
    browser_float: bool = false, PAGE_LAYOUT;
    /// The browser card folded to its header with the chevron in that header, kept from one run
    /// to the next like the toolbar's pins (#8692).
    browser_collapsed: bool = false, PAGE_LAYOUT;
    /// Whether the browser card is always shown or kept out of sight until the pointer comes to
    /// it, as the navigation bar's "Hide until hovered" does (#8692).
    browser_show: crate::browser::BrowserShow = Default::default(), PAGE_LAYOUT;
    /// Where command dialogs open: at the view's edge, or in the left column in place of the
    /// browser and the History there (#6707).
    dialog_place: crate::dialog_column::DialogPlace = Default::default(), PAGE_LAYOUT;
    /// The names the browser, dialogs and lists show for the origin planes (#6491).
    origin_plane_names: OriginPlaneNames = OriginPlaneNames::default(), PAGE_DESIGN;
    /// When components' bodies sit in a Bodies folder in the browser (#4217).
    browser_bodies_folders: BodiesFolders = BodiesFolders::Several, PAGE_LAYOUT;
    /// Each tree row's eye between its fold arrow and its radio, as before #8718, instead of in a
    /// column at the rows' right end. Off by default.
    browser_eye_left: bool = false, PAGE_LAYOUT;
    /// Every component placement in the browser starts open, as before #9274, instead of closed
    /// until the user opens it. Off by default.
    browser_components_open: bool = false, PAGE_LAYOUT;
    /// Folders in the Browser start closed, and placements too whatever "Components start open"
    /// says, and they open only on request (#8768): with their arrow, Expand All or Find in
    /// Browser, which opens just the rows down to its body and closes again the ones the last find
    /// opened. A body picked in the 3D view leaves the Browser as it is, and only the body Find in
    /// Browser found has the rows above it marked, so a large assembly's placements are not named
    /// again after each edit. Off by default: folders start open, placements as "Components start
    /// open" says, and a pick in the 3D view opens the Browser down to its body (#3005), as before.
    browser_rows_start_closed: bool = false, PAGE_LAYOUT;
    /// The wheel zooms toward the point under the cursor (else toward the view's centre).
    zoom_to_cursor: bool = true, PAGE_NAVIGATION => check("Zoom to the cursor", "The wheel zooms toward the point under the cursor (off: toward the view's center)");
    /// Speed multiplier for wheel zoom.
    wheel_zoom_speed: f32 = 1.0, PAGE_NAVIGATION, accept(0.25..=4.0) => slider("ui.wheel_zoom_speed", 0.25..=4.0, 0.05, |v| format!("{v:.2}×"), "prefs:wheel_zoom_speed");
    /// Opt-in surface anchoring for drag-zoom. The wheel
    /// and pinch retain their independent `zoom_to_cursor` preference.
    zoom_stop_at_model: bool = false, PAGE_NAVIGATION => check_id("prefs.zoom_stop_at_model");
    /// Spread discrete wheel input across frames (off: apply each event immediately).
    smooth_wheel_zoom: bool = true, PAGE_NAVIGATION => check("Smooth wheel zoom", "Turn off to apply each wheel event immediately, without a smoothing tail");
    /// Two-finger trackpad scroll pans (off: it zooms like the wheel); pinch always zooms.
    trackpad_pan: bool = true, PAGE_NAVIGATION;
    /// Hold this modifier alone to zoom with vertical wheel or two-finger scroll.
    trackpad_zoom_modifier: TrackpadZoomModifier = TrackpadZoomModifier::default(), PAGE_NAVIGATION;
    /// Horizontal trackpad and thumb-wheel scrolling pans the view the other way (#2007).
    pan_reverse_x: bool = false, PAGE_NAVIGATION => check("ui.reverse_horizontal_pan_direction", "Horizontal two-finger scrolling and the thumb wheel pan the other way");
    /// Vertical trackpad scrolling pans the view the other way (#5240).
    pan_reverse_y: bool = false, PAGE_NAVIGATION => check("ui.reverse_vertical_pan_direction", "Vertical two-finger scrolling pans the other way");
    /// Shift with two-finger trackpad scrolling orbits the view the other way, both ways at once,
    /// as natural scrolling turns it (#8410).
    orbit_reverse: bool = false, PAGE_NAVIGATION => check("ui.reverse_trackpad_orbit_direction", "Shift with two-finger scrolling orbits the other way");
    /// View changes (axis view, Look At, Home) animate; `look_at_speed` scales how fast.
    look_at_anim: bool = true, PAGE_NAVIGATION => check("Animate view changes", "ui.look_at_anim_desc");
    look_at_speed: f32 = 1.0, PAGE_NAVIGATION, accept(0.25..=4.0) => enabled_slider(look_at_anim, "Animation speed", 0.5..=3.0, 0.25, |v| format!("{v:.2}×"));
    /// Default modelling orientation: `z` (Z up) or `y` (Y up) (#3).
    up_axis: String = "z".into(), PAGE_DESIGN;
    /// After a STEP or IGES file opens, offer to stand a Y-up part up (`stand_up`).
    offer_stand_up: bool = true, PAGE_DESIGN => check_tr("ui.offer_to_stand_up_y_up_files");
    /// How the offer is made for a file whose header names no program known to save Y up: a note
    /// over the view (the default), the dialog, or nothing (#7717).
    stand_up_unknown: StandUpUnknown = StandUpUnknown::default(), PAGE_DESIGN;
    /// Dragging Extrude's distance arrow stops at flat faces parallel to the profile over it and
    /// outlines them (`extrude_face_snap`, #2398). Off: the drag moves in round steps only.
    extrude_arrow_face_stops: bool = true, PAGE_TOOLS => check_id("prefs.extrude_arrow_face_stops");
    /// Extrude: with a profile picked, a face, vertex or parallel plane picked off its plane
    /// switches the extent to To Object (off: picks only add profiles, as before). A click into the
    /// Profiles box makes the next picks profiles again.
    extrude_auto_to_object: bool = true, PAGE_TOOLS => check_tr("ui.extrude_auto_to_object");
    /// Extrude turns the view to isometric each time it opens, so the depth can be seen (#9176).
    /// Off (the default): the view stays as it is.
    extrude_view_isometric: bool = false, PAGE_TOOLS => check_id("prefs.extrude_view_isometric");
    /// With `extrude_view_isometric`: when Extrude closes (OK or Cancel) the view goes back to
    /// where it was before it turned (#9176). Off (the default): it stays isometric.
    extrude_view_return: bool = false, PAGE_TOOLS => check_id("prefs.extrude_view_return");
    /// Feature windows choose Join, New Body or Cut from what their material touches (`op_auto`,
    /// #6901). Off: they keep the operation they open with (Extrude its own Join or Cut), as before.
    auto_operation: bool = true, PAGE_TOOLS => check_id("prefs.auto_operation");
    /// Reuse a command's safe values and options after it succeeds, until the app closes. Off by default: each dialog opens with its usual defaults.
    remember_dialog_values: bool = false, PAGE_TOOLS => check_id_help("prefs.remember_dialog_values", "prefs.remember_dialog_values.help");
    /// Placing a Single hole snaps to the middle of the face and the centres of its round edges
    /// (`hole_place`). Off by default: a hole goes where it is clicked.
    hole_snaps: bool = false, PAGE_TOOLS => check("Snap to face and edge centres");
    /// A placed Single hole has a handle that drags it, and a pick elsewhere on a face that has a
    /// hole moves the hole there (`hole_place`). Off by default: a second pick on its face takes
    /// the hole out, and a hole is moved by taking it out and picking again.
    hole_moves: bool = false, PAGE_TOOLS => check("Move holes by clicking or dragging");
    /// The browser's Origin folder starts hidden in designs that don't keep its eye themselves.
    hide_origin: bool = false, PAGE_DESIGN => check_tr("ui.hide_origin_by_default");
    /// Handles on the preview of a feature placed on a face (Rest, Boss, Snap Fit): a marker that
    /// drags it across the model and a ring that turns it. Off by default: the feature is then placed
    /// by its click only, as before, and its Position input takes faces only.
    placement_handles: bool = false, PAGE_TOOLS => check_tr("ui.placement_handles");
    /// The files a design names (linked designs, font files) may be on any network location, for
    /// organisations that keep their designs on file servers. Off (the default): only on the
    /// servers in `network_hosts`, or one the user allows when asked (`network_prompt`).
    network_locations: bool = false, PAGE_GENERAL => check_tr("prefs.network_locations");
    /// The servers the files a design names may be on ("Always allow this server").
    network_hosts: Vec<String> = Vec::new(), PAGE_GENERAL;
    /// What a middle-button double-click in the view does: `home`, `fit` or `off`.
    middle_double_click: String = "home".into(), PAGE_NAVIGATION;
    /// A view chosen on the navigator (a cube face, corner or arrow, an axis tip) also frames the
    /// whole model, as Home does (#10091). Off by default: the view turns in place, keeping its
    /// target and zoom.
    navigator_fit: bool = false, PAGE_NAVIGATION;
    /// The view cube's directional arrows orbit the camera the way the arrow points, as in
    /// Fusion and Inventor (`invert_navigator_arrows`, #10254). Off by default: an arrow rotates
    /// the cube toward the side it sits on, so the arrow on the left swings the camera left.
    invert_navigator_arrows: bool = false, PAGE_NAVIGATION;
    /// The view cube's two curved arrows roll the view the other way: the clockwise arrow rolls
    /// counterclockwise and the counterclockwise arrow rolls clockwise (`invert_curved_roll_arrows`,
    /// #10483). Off by default: each arrow rolls the view the way it curls.
    invert_curved_roll_arrows: bool = false, PAGE_NAVIGATION;
    /// How much of the view the model fills when Home, Fit, an opened design or a navigator
    /// view change frames it (0.1 to 1; #10091): 1, the default, fills the room between the
    /// panels, the compass and the navigation bar, as before.
    home_fit_size: f32 = 1.0, PAGE_NAVIGATION, accept(0.1..=1.0);
    /// Colour vision mode (#778): `off`, `protan`, `deutan` or `tritan`.
    color_vision: crate::theme::ColorVision = crate::theme::ColorVision::Off, PAGE_APPEARANCE;
    /// Check for updates at start and once a day (docs/updates.md).
    check_updates: bool = true, 0;
    /// When the last update check ran, and the versions skipped or put off (`update::Memo`).
    update: crate::update::Memo = crate::update::Memo::default();
    /// The variant last started in each split tool family (Rectangle, Circle, Arc, Slot, Polygon, Line,
    /// Spline): the family's button and shortcut start it again (`sketch_families`).
    sketch_variants: crate::sketch_families::Choices = crate::sketch_families::Choices::default();
    /// Push/Pull's last distance in millimetres (positive pulls out of the face, negative pushes
    /// in), from its window or a `solid.press_pull` run: with Push/Pull open, a double-click on a
    /// flat face repeats it (`press_pull_repeat`). `None` (the default, and what older settings
    /// load as) until Push/Pull is used; no page reset discards it.
    press_pull_distance: Option<f64> = None;
    /// Personal Hole parameter combinations, shared by every design in this profile.
    /// No page reset discards this user-authored library.
    hole_presets: crate::hole_presets::Library = crate::hole_presets::Library::default();
    /// Save as mesh: the last format (`stl` or `3mf`). Older versions also kept a "send to slicer"
    /// choice here (`save_mesh_send`, `save_mesh_slicer`); a settings file that still has them loads,
    /// and they are ignored.
    save_mesh_format: String = "stl".into();
    /// Hidden edges in the current-view DXF export (false: omit, true: dashed).
    view_dxf_dashed: bool = false;
    /// Model-space chord tolerance for current-view DXF curves, in millimetres.
    view_dxf_tolerance: f64 = 0.01;
    /// Send to Slicer: the last slicer used (an installed slicer's id or a program path).
    send_slicer: String = String::new();
    /// Slicers that can load a file into the window that is already open do so (default on).
    slicer_single_instance: bool = true, 0;
    /// How strongly a print face stands out in the viewport (#8429): the share of the section
    /// colour in its tint, 0 to 1; its outline follows. 0 draws no highlight.
    print_face_highlight: f32 = 0.2, PAGE_GENERAL, accept(0.0..=1.0);
    /// The line about sending again replacing the slicer's file has been shown.
    slicer_resend_hint_seen: bool = false;
    /// The user has agreed to Open with EasyPrint uploading a model to Printables' servers.
    easyprint_consent: bool = false;
    /// Save as mesh: the last quality (`standard`, `fine` or `finest`, #4747).
    save_mesh_quality: String = "standard".into();
    /// The programs the user added to the slicer list, newest first.
    custom_slicers: Vec<String> = Vec::new();
    /// AI features (docs/agent-chat.md): the master switch. Off: no AI button, shortcut, menu
    /// entry or panel, and nothing is looked for or started.
    ai_features: bool = true, PAGE_ASSISTANT;
    /// The Assistant button in the application bar (the shortcut and View menu entry stay).
    ai_toolbar_button: bool = true, PAGE_ASSISTANT;
    /// An agent's slow commands over the control channel (MCP) compute off the window thread,
    /// so the window stays live while they run (#1989, [`crate::control_job`]). Off: they run on
    /// the window thread, and the window waits for each, as before (the default).
    agent_off_thread: bool = false, PAGE_ASSISTANT;
    /// Let AI agents connect: the localhost control channel (MCP) opens on the default port at
    /// start-up, unless `--control`/`SKETCRAFT_CONTROL_PORT` name one. Read at start-up.
    agent_control: bool = true, PAGE_ASSISTANT;
    /// Add SketCraft's MCP server to the AI apps installed on this computer at start-up
    /// ([`crate::ai_connect`]); off takes the entries out again.
    connect_ai_apps: bool = true, PAGE_ASSISTANT;
    /// Agents the user added (the installed ones are found by themselves). API keys are never
    /// here: they live in the system keychain.
    agents: Vec<crate::agent_chat::agents::AgentConfig> = Vec::new();
    /// The agent a new conversation starts with (an agent's id; empty: the first one available).
    agent_default: String = String::new();
    /// The agent the chat last started (an agent's id; empty: none yet).
    agent_last: String = String::new();
    /// When SketCraft starts: connect the agent started last (`agent_last`, else the one a new
    /// conversation starts with), and open the panel (#7009). Off by default: an agent starts
    /// only when a message is sent or Connect is pressed, and the panel opens when asked for.
    agent_connect_at_start: bool = false, PAGE_ASSISTANT;
    agent_chat_open_at_start: bool = false, PAGE_ASSISTANT;
    /// How much the agent may do without asking (`ask`, `edits`, `all`), and per agent.
    ai_permissions: crate::agent_chat::agents::Mode = crate::agent_chat::agents::Mode::Edits, PAGE_ASSISTANT;
    agent_permissions: std::collections::BTreeMap<String, crate::agent_chat::agents::Mode> = Default::default(), PAGE_ASSISTANT;
    /// The model last chosen in the chat, per agent: put back when the agent offers it again.
    agent_models: std::collections::BTreeMap<String, String> = Default::default();
    /// "Always allow" answers: per agent, the tools it may use without asking.
    agent_always_allow: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    /// The side the panel docks on (`left` or `right`), its width, and whether it is folded to a strip.
    agent_chat_side: String = "right".into(), PAGE_ASSISTANT;
    agent_chat_width: f32 = 380.0, PAGE_ASSISTANT;
    agent_chat_minimized: bool = false, PAGE_ASSISTANT;
    /// Whether the chat is in its own window instead of the side panel, and that window's place
    /// and size as the system last reported them (`[x, y, width, height]`, points; `x` can be
    /// negative or past the main screen when the window sits on another one).
    agent_chat_popped: bool = false, PAGE_ASSISTANT;
    agent_chat_window: Option<[f32; 4]> = None, PAGE_ASSISTANT;
    /// Changes the agent may make for one message; 0 is no limit.
    agent_chat_turn_limit: u32 = crate::agent_chat::gate::DEFAULT_TURN_LIMIT, PAGE_ASSISTANT;
    /// Custom instructions (#8220): text the agent chat adds to its system note when a session
    /// starts (`agent_chat::acp::system_note`). No page reset discards this user-authored text.
    agent_instructions: String = String::new();
    /// Compact output for agents connected through the MCP server (`sketcraft-cli mcp`, docs/mcp.md):
    /// `inspect_design` summarises sketches and tool JSON is not pretty-printed. Off: as always.
    /// The server asks once per agent session (`mcp.options`), so a change applies to the next one.
    mcp_compact: bool = false, PAGE_ASSISTANT;

    // ---- Sketch / selection modifier bindings (classic defaults; alternate mapping is opt-in) ----
    /// Modifier that suppresses sketch auto-snap while drawing or dragging a sketch point.
    /// Default: Alt/Option. Set to Ctrl/Cmd for the alternate mapping.
    suppress_sketch_snap: ModifierCombo = ModifierCombo::CTRL_ALT, PAGE_SKETCH;
    /// Temporary horizontal/vertical guides from hovered centres and edge midpoints.
    sketch_tracking_guides: bool = true, PAGE_SKETCH;
    /// While a drawing tool waits for a point, hovered sketch points (ends, corners, free
    /// points) start tracking guides too (#9105). Off: only centres and midpoints, as before.
    sketch_tracking_points: bool = true, PAGE_SKETCH;
    /// Snap a drawing point to where the two most recent centre or midpoint guides cross.
    /// Off: the guides stay visual and the cursor does not move (#8694).
    sketch_recent_snap: bool = false, PAGE_SKETCH;
    /// Drawing lines a point up with the sketch's points on both axes at once, keeping the
    /// horizontal or vertical snap to the previous point, and the click adds the alignments
    /// (#7828). Off by default: only with the previous point, one axis at a time, as before.
    sketch_align_points: bool = false, PAGE_SKETCH;
    /// Open Project's options before picking; off restores its previous direct-pick workflow.
    project_options_dialog: bool = true, PAGE_SKETCH;
    /// Whether Alt/Option also frees gizmo / canvas manipulator arrows (Primary always does).
    manipulator_free_alt: bool = true, PAGE_SKETCH;
    /// A sketch's icon in History and the browser takes the colour of its solve state, and its
    /// tooltip names the state (#7245). Off: every sketch has the same icon, as before.
    sketch_state_icons: bool = true, PAGE_SKETCH;
    /// Show the construction-line family in Sketch CREATE; off restores the earlier menu.
    sketch_construction_lines: bool = true, PAGE_SKETCH;
    /// Use the only straight construction line in the preselection as Mirror's reference.
    sketch_mirror_auto_line: bool = true, PAGE_SKETCH;
    /// A sketch tool's shortcut pressed again before the first point steps to the tool's next
    /// creation mode (#2459). Off (the default): the key starts the tool again, as it always has.
    shortcut_cycles_modes: bool = false, PAGE_SKETCH;
    /// A click on the ribbon button of the sketch tool that is running ends the tool, so clicks
    /// in the view select again (#10151). Off: the click starts the tool again, as before.
    ribbon_click_ends_tool: bool = true, PAGE_SKETCH;
    /// Opening or editing a sketch turns the view to face it (the default). Off: the view stays as
    /// it is, and Look At faces the sketch when asked.
    sketch_open_look_at: bool = true, PAGE_SKETCH;
    /// A sketch started on a face of a body in another component than the active one activates
    /// that component first, so the sketch belongs to it (`sketch.create` `activate_part`,
    /// #10215). Off (the default): the sketch belongs to the active component, as before.
    sketch_face_activates_part: bool = false, PAGE_SKETCH;
    /// Automatically split coplanar faces when sketch curves are drawn on a body face (SketchUp direct modeling behavior).
    sketch_auto_split_face: bool = true, PAGE_SKETCH;
    /// While a sketch is open, only sketch keys and keys that work everywhere run; a model
    /// command's key (E, F) does nothing. Off (the default): a model key pressed in a sketch
    /// runs its command, and one that builds a feature finishes the sketch first (#2815).
    sketch_keys_only: bool = false, PAGE_SHORTCUTS;
    /// Where the view goes when a sketch ends (#7038). Keep (the default) leaves it as it is.
    sketch_exit_view: SketchExitView = SketchExitView::default(), PAGE_SKETCH;
    /// A shape drawn in a sketch gets all its dimensions at once: the typed values, and every
    /// other size the shape has at its size as drawn (`sketch.draw.dimensioned` with `as_drawn`).
    /// Shapes `sketcraft_engine::dimension_roles` doesn't list are drawn as before.
    sketch_auto_dimensions: bool = false, PAGE_SKETCH;
    /// With `sketch_auto_dimensions`: new shapes are also placed from the sketch origin until they
    /// are fully constrained (`sketch.draw.dimensioned` with `locate`). A shape already anchored to
    /// the origin or to placed geometry gets nothing more.
    sketch_auto_locate: bool = false, PAGE_SKETCH;
    /// A centre-point arc runs the way the cursor goes round its centre from the start (#6645).
    /// Off: it always runs counter-clockwise from the start, as it did before.
    center_arc_follows_cursor: bool = true, PAGE_SKETCH;
    /// In the Line tool, a press, a drag and the release draw one line from the press to the
    /// release and end there (#7247); clicks still draw a chain. Off (the default): a drag is a
    /// click that moved on the way, placing a point where it is let go (#2448).
    line_drag_draws_one: bool = false, PAGE_SKETCH;
    /// In the Line tool, leaving the chain's last point and resting back on it switches to a
    /// tangent arc, and the same again back to lines (#9489). Off (the default): resting there
    /// does nothing, and tangent arcs are the Arc tool's.
    line_hover_tangent_arc: bool = false, PAGE_SKETCH;
    /// Allow ordinary edge picks through faces in Shaded with hidden edges.
    /// Off preserves the visible-only picking used in v0.30.1.
    select_hidden_edges: bool = false, PAGE_SKETCH;
    /// Include hidden geometry and a small pointer neighbourhood in Select Other.
    select_other_hidden: bool = true, PAGE_SKETCH;
    /// Where a dimension's value is typed: in a box on the dimension (the default), or in a small
    /// window at it whose arrows step the value (#7246, `dim_view::window`).
    dimension_input: DimensionInput = DimensionInput::View, PAGE_SKETCH;
    /// What the dimension window's arrows, Up/Down and the wheel add to a length (millimetres,
    /// converted to the unit the value is written in) and to an angle (degrees).
    dimension_step_mm: f64 = 1.0, PAGE_SKETCH, accept(DIMENSION_STEP_MM);
    dimension_step_deg: f64 = 1.0, PAGE_SKETCH, accept(DIMENSION_STEP_DEG);
    /// The first length dimension of a sketch with nothing sized yet scales the whole drawing to
    /// its value (#1365). Off: it sizes only what it measures (#4109).
    first_dimension_scales: bool = true, PAGE_SKETCH;
    /// When true: Shift+click adds without removing; Primary toggles.
    /// When false (default): Shift and Primary both toggle.
    selection_shift_add_only: bool = false, PAGE_SKETCH;
    /// Prefer movable sketch points on exact selection ties; off keeps point insertion order.
    prefer_free_sketch_points: bool = true, PAGE_SKETCH;
    /// While the open sketch's constraints conflict, the curves and points a failing constraint
    /// holds, and its glyph or dimension, are drawn in `sketch_conflict` (#7244). Off: only the
    /// glyphs and dimensions are red, in `error`, as before.
    sketch_conflict_colour: bool = true, PAGE_SKETCH;
    /// Hide constraint glyphs when the sketch is zoomed out so far that a millimetre of the plane
    /// is smaller than `sketch_tools::MIN_GLYPH_PX_PER_MM` on screen (#8980) and the sketch spans
    /// less than `sketch_tools::MIN_GLYPH_SPAN_PX` (#10518). Off: glyphs stay at a fixed screen
    /// size at every zoom, as before.
    hide_constraint_glyphs_zoomed_out: bool = true, PAGE_SKETCH;
    /// The Ellipse tool draws the major and minor axes as construction lines with the ellipse, so
    /// its half-axes and axes can be dimensioned. Off: the ellipse alone, and D on it gives its
    /// minor radius, labelled R, as before.
    ellipse_axes: bool = true, PAGE_SKETCH;
    /// Drive Joints shows the moved parts' old place as a ghost (its Show ghost box, #5905).
    drive_joints_ghost: bool = true, PAGE_TOOLS => check_tr("prefs.design_tools.ghost");
    /// While Move's triad drags a component drawn as a layer of its own, what is under the pointer
    /// is still picked and highlighted, as before #7273. Off by default: such a drag picks nothing,
    /// so a step neither meshes the dragged bodies where they stand nor looks up the Browser row a
    /// highlight follows (`drag_layer::holds_pointer`).
    move_drag_highlights: bool = false, PAGE_TOOLS => check_id("prefs.move_drag_highlights");
    /// Plugin ids turned off (Preferences › Plugins). An installed plugin whose id is absent is on.
    disabled_plugins: std::collections::BTreeSet<String> = std::collections::BTreeSet::new(), PAGE_PLUGINS;
}

/// What a middle-button double-click in the view does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiddleDoubleClick {
    /// Go Home (the iso view, framing what is shown).
    Home,
    /// Frame what is shown, keeping the orientation.
    Fit,
    Off,
}

impl MiddleDoubleClick {
    pub const ALL: [(MiddleDoubleClick, &'static str, &'static str); 3] =
        [(MiddleDoubleClick::Home, "home", "Home"), (MiddleDoubleClick::Fit, "fit", "Fit"), (MiddleDoubleClick::Off, "off", "Off")];

    /// The preference's setting (Home for an unknown one).
    pub fn of(app: &SketApp) -> MiddleDoubleClick {
        Self::parse(&app.preferences.middle_double_click).unwrap_or(MiddleDoubleClick::Home)
    }

    pub fn parse(id: &str) -> Option<MiddleDoubleClick> {
        Self::ALL.iter().find(|(_, i, _)| *i == id).map(|(m, _, _)| *m)
    }

    pub fn label(self) -> &'static str {
        Self::ALL.iter().find(|(m, _, _)| *m == self).map_or("Home", |(_, _, l)| l)
    }
}

/// The preferences window.
#[derive(Default)]
pub struct PrefsWindow {
    navbar: navbar::State,
    navbar_scroll: bool,
    pub open: bool,
    /// The page shown (index into [`PAGES`]).
    pub section: usize,
    /// Everything as it was when the dialog opened (or at the last Apply), for Cancel.
    snapshot: Option<Snapshot>,
    /// The up axis preference the view follows now (applied when it changes, and at startup).
    applied_up: Option<String>,
    /// The Profile operations preference the profiler last followed: applied when it changes,
    /// so `diagnostics.profile` (an agent) and SKETCRAFT_PROFILE can switch it in between.
    applied_profile: Option<bool>,
    /// The appearance last asked of the native window ([`sync_window_theme`]).
    window_theme: Option<egui::SystemTheme>,
    /// Last observed OS tone, independent of the selected theme's tone.
    system_theme_dark: Option<bool>,
    /// Import callbacks fill the same slot as a card click; never persisted in preferences.
    theme_picker_night: bool,
    /// The perspective angle the view was last drawn with ([`follow_view_fov`]); 0 before the first.
    drawn_fov: f64,
    /// The search text of the interface font list.
    font_filter: String,
    /// Search text on the Plugins page.
    pub(crate) plugin_query: String,
    /// The followed theme file's state (#7573): its last good theme and why a read failed.
    pub(crate) follow: crate::theme_files::Follow,
    /// Filters every page. Empty leaves the selected page as it is.
    pub filter: String,
    /// The page on screen when the current search started. Clearing the field returns there.
    search_from: Option<usize>,
    /// Set when a result is chosen, so the page draw does not put the previous tab back.
    land_on: Option<usize>,
    /// The row to scroll to after a result is chosen.
    reveal: Option<crate::prefs_search::Reveal>,
    /// Where the theme Import tile was drawn last frame: a file dropped there is a theme (#8226).
    import_tile: Option<egui::Rect>,
}

/// Following a theme file needs the file system: the desktop app only (#7573).
const FOLLOW_AVAILABLE: bool = cfg!(not(target_arch = "wasm32"));

/// Follow `path` as the theme for this run (the `--theme-file` launch flag), without saving it.
pub fn follow_theme_file_for_run(app: &mut SketApp, path: &str) -> Result<(), String> {
    if !FOLLOW_AVAILABLE {
        return Err(FOLLOW_UNAVAILABLE.into());
    }
    let path = crate::theme_files::follow_path(path)?;
    app.prefs_window.follow.launch = (!path.is_empty()).then_some(path);
    Ok(())
}

const FOLLOW_UNAVAILABLE: &str = "`theme_follow` is not available in the browser: it follows a file on this computer; import the theme instead";

/// Stop following a theme file: the preference and this run's launch flag. The chosen theme
/// takes over again, with its own dark or light.
fn stop_following(app: &mut SketApp) {
    let following = !app.prefs_window.follow.path(&app.preferences.theme_follow).is_empty();
    app.preferences.theme_follow.clear();
    app.prefs_window.follow.launch = None;
    if following {
        let theme = app.preferences.theme.clone();
        select_theme(app, &theme);
    }
}

/// Look at the followed theme file (#7573): every second while the app runs and whenever the
/// window gains the focus. A new failure is reported once in the status bar; a recovery too.
fn follow_theme_file(app: &mut SketApp, ctx: &egui::Context) {
    if !FOLLOW_AVAILABLE {
        return;
    }
    let (now, focused) = ctx.input(|i| (i.time, i.focused));
    let follow = &mut app.prefs_window.follow;
    let gained_focus = focused && !follow.focused;
    follow.focused = focused;
    let path = follow.path(&app.preferences.theme_follow).to_string();
    let had_error = follow.error().is_some();
    match follow.poll(&path, now, gained_focus) {
        crate::theme_files::FollowChange::None => {}
        crate::theme_files::FollowChange::Loaded(name) => {
            if had_error {
                app.set_status(format!("Theme file loaded again: {name}"), false);
            }
        }
        crate::theme_files::FollowChange::Failed(e) => app.set_status(format!("Theme file not applied: {e} (the last good theme stays on)"), true),
    }
    if !path.is_empty() {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(crate::theme_files::FOLLOW_INTERVAL));
    }
}

/// `ui.prefs` `theme_follow`: a path follows that file (read at once, so the reply has any
/// error), `""` stops following.
fn set_theme_follow(app: &mut SketApp, value: &serde_json::Value, available: bool) -> Result<(), String> {
    if !available {
        return Err(FOLLOW_UNAVAILABLE.into());
    }
    let path = value.as_str().ok_or("`theme_follow` must be a theme file's path, or \"\" to stop following")?;
    let path = crate::theme_files::follow_path(path)?;
    if path.is_empty() {
        stop_following(app);
        return Ok(());
    }
    app.prefs_window.follow.launch = None;
    app.preferences.theme_follow = path.clone();
    // Read now, so the reply has any error; the next frame's look then follows on as usual.
    let _ = app.prefs_window.follow.poll(&path, f64::NEG_INFINITY, true);
    Ok(())
}

/// Open the dialog at a page.
pub fn open_at(app: &mut SketApp, page: usize) {
    app.prefs_window.open = true;
    app.prefs_window.section = page.min(PAGES.len() - 1);
}

/// The sidebar is drawn before the selected page: defer its Profiles action until that page
/// callback, and leave any search before showing General's existing profile controls.
pub(crate) fn open_profiles(app: &mut SketApp) {
    clear_search(app);
    open_at(app, PAGE_GENERAL);
    app.prefs_window.land_on = Some(PAGE_GENERAL);
}

/// Every setting the dialog changes, for Cancel and Restore Defaults.
#[derive(Clone)]
struct Snapshot {
    prefs: Prefs,
    dark: bool,
    layout: crate::shell::LayoutMode,
    grid: bool,
    navigation_bar: bool,
    tool_cursor: bool,
    shadow: bool,
    perspective: bool,
    style: u8,
    pick_bodies: bool,
    agent: crate::agent_cursor::Settings,
    autosave: f64,
    auto_project: bool,
    keys: std::collections::BTreeMap<String, Vec<String>>,
    /// What a settings profile also switches: the toolbar, the S box favourites and which profile is active.
    toolbar: serde_json::Value,
    favourites: Vec<String>,
    profile: Option<String>,
}

impl Snapshot {
    fn take(app: &SketApp) -> Snapshot {
        Snapshot {
            prefs: app.preferences.clone(),
            dark: app.ui.dark,
            layout: app.ui.layout,
            grid: app.ui.show_grid,
            navigation_bar: app.ui.show_navigation_bar,
            tool_cursor: app.ui.tool_cursor,
            shadow: app.ui.ground_shadow,
            perspective: app.ui.perspective,
            style: app.ui.visual_style,
            pick_bodies: app.ui.pick_bodies,
            agent: app.ui.agent_cursor,
            autosave: app.autosave_minutes,
            auto_project: app.session.auto_project,
            keys: app.keymap.custom.clone(),
            toolbar: app.toolbar_custom.prefs(),
            favourites: app.sbox.pinned.clone(),
            profile: app.settings_profiles.active.clone(),
        }
    }
    fn restore(self, app: &mut SketApp) {
        let graphics = (app.preferences.msaa, app.preferences.window_system.clone(), app.preferences.graphics_adapter.clone());
        let units_changed = app.preferences.default_units != self.prefs.default_units;
        // Update bookkeeping isn't a setting: a check that finished while the dialog was open stays.
        let update = std::mem::take(&mut app.preferences.update);
        app.preferences = self.prefs;
        sync_surface_analysis(app);
        app.preferences.update = update;
        if units_changed {
            default_units_changed(app);
        }
        app.ui.dark = self.dark;
        app.ui.layout = self.layout;
        app.ui.show_grid = self.grid;
        app.ui.show_navigation_bar = self.navigation_bar;
        app.ui.tool_cursor = self.tool_cursor;
        app.ui.ground_shadow = self.shadow;
        app.ui.perspective = self.perspective;
        app.ui.visual_style = self.style;
        app.ui.pick_bodies = self.pick_bodies;
        app.ui.agent_cursor = self.agent;
        app.autosave_minutes = self.autosave;
        set_autosave(app);
        app.session.auto_project = self.auto_project;
        app.keymap.custom = self.keys;
        app.toolbar_custom.load(&self.toolbar);
        app.sbox.pinned = self.favourites;
        app.settings_profiles.active = self.profile.filter(|name| app.settings_profiles.names().contains(name));
        if (app.preferences.msaa, app.preferences.window_system.clone(), app.preferences.graphics_adapter.clone()) != graphics {
            save_graphics(app);
        }
    }

    /// Whether everything is as in the snapshot: nothing Cancel would put back. Destructured, so
    /// a setting added to the snapshot can't be left out of the comparison.
    fn matches(&self, app: &SketApp) -> bool {
        let Snapshot {
            prefs,
            dark,
            layout,
            grid,
            navigation_bar,
            tool_cursor,
            shadow,
            perspective,
            style,
            pick_bodies,
            agent,
            autosave,
            auto_project,
            keys,
            toolbar,
            favourites,
            profile,
        } = self;
        *prefs == app.preferences
            && *dark == app.ui.dark
            && *layout == app.ui.layout
            && *grid == app.ui.show_grid
            && *navigation_bar == app.ui.show_navigation_bar
            && *tool_cursor == app.ui.tool_cursor
            && *shadow == app.ui.ground_shadow
            && *perspective == app.ui.perspective
            && *style == app.ui.visual_style
            && *pick_bodies == app.ui.pick_bodies
            && *agent == app.ui.agent_cursor
            && *autosave == app.autosave_minutes
            && *auto_project == app.session.auto_project
            && *keys == app.keymap.custom
            && *toolbar == app.toolbar_custom.prefs()
            && *favourites == app.sbox.pinned
            && *profile == app.settings_profiles.active
    }
}

/// Whether Apply has anything to do (#8561): something changed since the dialog opened or since
/// the last Apply. False while the dialog is closed.
fn apply_pending(app: &mut SketApp) -> bool {
    let Some(snapshot) = app.prefs_window.snapshot.as_mut() else {
        return false;
    };
    // Update bookkeeping isn't a setting and Cancel keeps it (`restore`), so it isn't a change.
    snapshot.prefs.update.clone_from(&app.preferences.update);
    app.prefs_window.snapshot.as_ref().is_some_and(|s| !s.matches(app))
}

/// The saved-settings keys whose values the snapshot holds, as [`SketApp::prefs`] writes them.
/// While the dialog is open every writer of the settings file saves these instead of the live
/// values, so a change that was never applied can't reach disk through the periodic save, a
/// quit or a window close (#9730). Keys the dialog can't change keep their live values.
pub(crate) fn held_overlay(app: &SketApp) -> Option<serde_json::Map<String, serde_json::Value>> {
    let s = app.prefs_window.snapshot.as_ref()?;
    let mut prefs = s.prefs.clone();
    // Same bookkeeping rule as `restore`: a check that finished while the dialog was open stays.
    prefs.update.clone_from(&app.preferences.update);
    let mut shortcut_box = app.sbox.prefs();
    if let Some(o) = shortcut_box.as_object_mut() {
        o.insert("pinned".into(), serde_json::json!(s.favourites));
    }
    let mut profiles = app.settings_profiles.prefs();
    if let Some(o) = profiles.as_object_mut() {
        o.insert("active".into(), serde_json::json!(s.profile));
    }
    let mut shortcuts = serde_json::Map::new();
    for (id, keys) in &s.keys {
        shortcuts.insert(id.clone(), crate::keymap::keys_value(keys));
    }
    // Bound outside the `json!` macro: the ui-containers guard reads `.shadow` inside a macro
    // as egui's popup shorthand.
    let shadow = s.shadow;
    let mut m = serde_json::Map::new();
    for (key, value) in [
        ("dark", serde_json::json!(s.dark)),
        ("grid", serde_json::json!(s.grid)),
        ("navigation_bar", serde_json::json!(s.navigation_bar)),
        ("tool_cursor", serde_json::json!(s.tool_cursor)),
        ("perspective", serde_json::json!(s.perspective)),
        ("pick_bodies", serde_json::json!(s.pick_bodies)),
        ("auto_project", serde_json::json!(s.auto_project)),
        ("visual_style", serde_json::json!(s.style)),
        ("shortcut_box", shortcut_box),
        ("shortcuts", serde_json::Value::Object(shortcuts)),
        ("toolbar", s.toolbar.clone()),
        ("settings_profiles", profiles),
        ("autosave_minutes", serde_json::json!(s.autosave)),
        ("ground_shadow", serde_json::json!(shadow)),
        ("agent_cursor", serde_json::json!(s.agent)),
        ("preferences", serde_json::to_value(&prefs).unwrap_or_default()),
    ] {
        m.insert(key.to_string(), value);
    }
    Some(m)
}

/// The colour theme (#2255): Dark, Light, System, the built-in theme files and the imported
/// ones, with Import…, Export… (the theme on now as a complete file, to start a new one from)
/// and Remove (an imported theme). Like the other preferences, a choice applies at once.
fn theme_row(app: &mut SketApp, ui: &mut egui::Ui) {
    theme_picker::row(app, ui);
}

fn theme_action(app: &mut SketApp, ui: &mut egui::Ui, action: Option<&str>, action_theme: Option<String>) {
    match action {
        Some("import") => app.pick_open_for(&["json"], |app, path| {
            let Some(path) = path else { return };
            // Through `vfs`, like every other import: in the web app a picked file is there.
            let text = sketcraft_engine::io::vfs::read_to_string(&path, crate::theme_files::MAX_BYTES).map_err(|e| e.to_string());
            import_theme_text(app, text);
        }),
        #[cfg(not(target_arch = "wasm32"))]
        Some("paste") => {
            let text = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
            if looks_like_theme(&text) {
                import_theme_text(app, Ok(text));
            } else {
                app.set_status(app.tr("prefs.theme_paste_none").to_string(), true);
            }
        }
        Some("export") => {
            let system_dark = system_dark(app, ui.ctx()).unwrap_or_else(|| app.prefs_window.system_theme_dark.unwrap_or(app.ui.dark));
            let file = theme_file(app, action_theme.as_deref().unwrap_or(&app.preferences.theme), system_dark);
            let name = file.get("name").and_then(serde_json::Value::as_str).unwrap_or("Theme").to_string();
            let text = serde_json::to_string_pretty(&file).unwrap_or_default();
            app.pick_save(&format!("{name}.json"), &["json"], move |app, path| {
                // Through `vfs`, like every other save: the web app hands it out as a download.
                if let Some(path) = path
                    && let Err(e) = sketcraft_engine::io::vfs::write(&path, text.as_bytes())
                {
                    app.set_status(format!("Theme export: {e}"), true);
                }
            });
        }
        Some("remove") => {
            let id = action_theme.unwrap_or_else(|| app.preferences.theme.clone());
            if let Err(e) = remove_theme(app, &id) {
                app.set_status(e, true);
            }
        }
        _ => {}
    }
}

/// Settings › Interface › Theme: a theme file to follow (#7573), chosen with the file picker,
/// and why its last read was not applied. The desktop app only.
fn follow_row(app: &mut SketApp, ui: &mut egui::Ui) {
    let path = app.prefs_window.follow.path(&app.preferences.theme_follow).to_string();
    let launch = app.prefs_window.follow.launch.is_some();
    let error = app.prefs_window.follow.error().map(str::to_string);
    let (mut choose, mut stop) = (false, false);
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(FOLLOW_AVAILABLE, |ui| {
            let mut on = !path.is_empty();
            let check = ui_kit::checkbox(ui, app.tr("prefs.theme_follow"), &mut on);
            crate::scenario::publish_target("prefs:theme_follow_toggle", check.rect);
            if check.changed() {
                if on {
                    choose = true;
                } else {
                    stop = true;
                }
            }
            ui_kit::help_icon(ui, app.tr("prefs.appearance.follow_help"));
            let button = ui_kit::button(ui, app.tr_label("Choose…"), ui_kit::Button::Small);
            crate::scenario::publish_target("prefs:theme_follow", button.rect);
            choose |= button.clicked();
        });
        ui_kit::settings_state_badge(ui, app.tr("prefs.appearance.desktop_only"), ui_kit::SettingsBadgeTone::Neutral);
        if !path.is_empty() {
            let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            ui_kit::settings_inline_note(ui, &name);
            ui_kit::help_icon(ui, app.tr(if launch { "prefs.theme_follow_launch" } else { "prefs.theme_follow_note" })).backing_on_hover_text(&path);
        }
    });
    if let Some(error) = error.filter(|_| !path.is_empty()) {
        ui_kit::text(ui, &crate::i18n::format("prefs.theme_follow_error", &[("error", &error)]), ui_kit::Text::Error("theme_follow"));
    }
    if stop {
        stop_following(app);
    }
    if choose {
        app.pick_open_for(&["json"], |app, path| {
            let Some(path) = path else { return };
            if let Err(e) = set_theme_follow(app, &serde_json::Value::String(path), FOLLOW_AVAILABLE) {
                app.set_status(format!("Follow a theme file: {e}"), true);
            }
        });
    }
}

/// The interface font (#1290): the built-in font first, then the installed families, with a
/// search box. The choice applies at once, like the other preferences.
fn ui_font_row(app: &mut SketApp, ui: &mut egui::Ui) {
    let builtin = crate::i18n::format("sketch_text.builtin_font", &[("family", "IBM Plex Sans")]);
    let current = if app.preferences.ui_font.trim().is_empty() { builtin.clone() } else { app.preferences.ui_font.clone() };
    let families = crate::sketch_text::families(ui.ctx());
    let mut values = vec![(String::new(), builtin.clone(), "prefs:font_builtin".to_string())];
    if let Some(families) = &families {
        values.extend(families.iter().map(|family| (family.clone(), family.clone(), format!("prefs:font:{family}"))));
    }
    let options: Vec<_> = values.iter().map(|(value, label, handle)| ui_kit::SearchOption { value, label, handle: Some(handle) }).collect();
    let chosen = ui_kit::named_control(ui, app.tr_label("Interface font"), |ui| {
        ui_kit::searchable_dropdown(
            ui,
            "pref_ui_font",
            &current,
            &mut app.prefs_window.font_filter,
            &options,
            None,
            families.is_none().then(|| crate::i18n::tr_label("Listing the installed fonts…")),
        )
    });
    if let Some(rect) = crate::scenario::rect_of(&format!("dialog:widget:{}", crate::i18n::tr_label("Interface font"))) {
        crate::scenario::publish_target("prefs:ui_font", rect);
        crate::scenario::publish_target(&format!("dialog:{}", app.tr("prefs.font")), rect);
        crate::scenario::publish_rect(&format!("dialog:widget:{}", app.tr("prefs.font")), rect);
    }
    if let Some(name) = chosen {
        if name.is_empty() || crate::fonts::ui_font(&name).is_some() {
            app.preferences.ui_font = name;
        } else {
            app.set_status(format!("The font `{name}` could not be read"), true);
        }
        app.prefs_window.font_filter.clear();
    }
}

/// Restore Defaults for one page.
fn defaults(app: &mut SketApp, page: usize) {
    let (msaa, window_system, adapter) = (app.preferences.msaa, app.preferences.window_system.clone(), app.preferences.graphics_adapter.clone());
    let units = app.preferences.default_units.clone();
    reset_preferences(&mut app.preferences, page);
    sync_surface_analysis(app);
    if app.preferences.default_units != units {
        default_units_changed(app);
    }
    match page {
        0 => {
            app.autosave_minutes = 5.0;
            set_autosave(app);
        }
        PAGE_APPEARANCE => {
            app.prefs_window.follow.launch = None;
            app.ui.dark = true;
        }
        PAGE_LAYOUT => {
            app.ui.layout = app.preferences.layout;
            app.ui.show_navigation_bar = true;
        }
        PAGE_ASSISTANT => app.ui.agent_cursor = Default::default(),
        PAGE_TOOLS => app.ui.tool_cursor = true,
        PAGE_DESIGN => {}
        PAGE_SHORTCUTS => {
            crate::keymap::stop_capture(app);
            crate::keymap::reset(app, None);
        }
        PAGE_GRAPHICS => {
            app.ui.visual_style = 0;
            app.ui.show_grid = false;
            app.ui.ground_shadow = false;
            app.ui.perspective = true;
            if app.preferences.msaa != msaa || app.preferences.window_system != window_system || app.preferences.graphics_adapter != adapter {
                save_graphics(app);
            }
        }
        PAGE_SKETCH => {
            app.session.auto_project = true;
            app.ui.pick_bodies = false;
        }
        _ => {}
    }
}

/// Whether the desktop asks for dark colours, for Theme: System; `None` when nothing says.
/// The host's own reading of the desktop (`Services::desktop_dark`, the freedesktop setting on
/// Linux, #7944) comes first, as the window toolkit does not report it on every desktop
/// (GNOME on Wayland), nor its changes; then the window toolkit's.
pub(crate) fn system_dark(app: &SketApp, ctx: &egui::Context) -> Option<bool> {
    app.services.desktop_dark.as_ref().and_then(|f| f()).or_else(|| ctx.system_theme().map(|t| t == egui::Theme::Dark))
}

/// The desktop's dark or light preference from the freedesktop settings a Linux host reads
/// (#7944): `appearance` is `org.freedesktop.appearance color-scheme` (1 prefer dark, 2 prefer
/// light, 0 no preference), `gnome` is GNOME's `org.gnome.desktop.interface color-scheme`
/// (`prefer-dark`, `prefer-light`, `default`) and `gtk_theme` its `gtk-theme` name. An explicit
/// preference wins; otherwise a dark-named GTK theme ("Adwaita-dark") means dark, and anything
/// else light, as GTK applications read it (GNOME's Light is `default`). `None` when no setting
/// was read at all.
pub fn desktop_prefers_dark(appearance: Option<u32>, gnome: Option<&str>, gtk_theme: Option<&str>) -> Option<bool> {
    match appearance {
        Some(1) => return Some(true),
        Some(2) => return Some(false),
        _ => {}
    }
    match gnome.map(|s| s.trim().trim_matches('\'')) {
        Some("prefer-dark") => return Some(true),
        Some("prefer-light") => return Some(false),
        _ => {}
    }
    let theme = gtk_theme.map(|s| s.trim().trim_matches('\'').to_ascii_lowercase()).filter(|s| !s.is_empty());
    if theme.as_deref().is_some_and(|t| t.ends_with("-dark") || t.ends_with(":dark")) {
        return Some(true);
    }
    (appearance.is_some() || gnome.is_some() || theme.is_some()).then_some(false)
}

/// Settings that follow the preferences every frame: the theme when it follows the system, and
/// the interface scale.
/// Tell the native window which appearance to take (#8983): the operating system's while the
/// theme follows it, otherwise the theme's own, so the title bar matches. egui's own window sync
/// (`Options::sync_window_theme`) went by the egui theme, which [`crate::theme::apply`] always
/// sets to light or dark. A macOS window told to be light stays light and no longer reports the
/// system appearance changing, so a theme following the system never turned with it.
fn sync_window_theme(app: &mut SketApp, ctx: &egui::Context, follows_system: bool) {
    let window = if follows_system {
        egui::SystemTheme::SystemDefault
    } else if app.ui.dark {
        egui::SystemTheme::Dark
    } else {
        egui::SystemTheme::Light
    };
    if app.prefs_window.window_theme != Some(window) {
        ctx.options_mut(|o| o.sync_window_theme = false);
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(window));
        app.prefs_window.window_theme = Some(window);
    }
}

pub fn apply(app: &mut SketApp, ctx: &egui::Context) {
    sync_surface_analysis(app);
    apply_network(&app.preferences);
    // The features computed from now on are checked as the preference says.
    app.session.set_validation(app.preferences.validation);
    // The profiler records while the preference is on (switched when it changes; at start-up only
    // when on, so SKETCRAFT_PROFILE=1 is not undone by the default).
    let profile = app.preferences.profile_operations;
    if app.prefs_window.applied_profile != Some(profile) {
        if profile || app.prefs_window.applied_profile.is_some() {
            sketcraft_engine::profile::set_enabled(profile);
        }
        app.prefs_window.applied_profile = Some(profile);
    }
    apply_file_limits(&app.preferences);
    // The viewport's display quality (an unknown saved value draws at Normal).
    sketcraft_engine::doc::display_quality::set_current(
        sketcraft_engine::doc::display_quality::DisplayQuality::parse(&app.preferences.display_quality).unwrap_or_default(),
    );
    // The view's up axis follows the preference (set in the dialog, by ui.prefs or loaded).
    if app.prefs_window.applied_up.as_deref() != Some(app.preferences.up_axis.as_str()) {
        up_axis_changed(app);
    }
    follow_view_fov(app);
    let os_dark = system_dark(app, ctx).unwrap_or_else(|| app.prefs_window.system_theme_dark.unwrap_or(app.ui.dark));
    app.prefs_window.system_theme_dark = Some(os_dark);
    if app.preferences.theme == "system" {
        let id = theme_picker::active_id(&app.preferences, os_dark);
        app.ui.dark = crate::theme_files::find(id, &app.preferences.custom_themes).map_or(id == "dark", |f| f.base.is_dark());
    }
    // A theme file's palette, in place of its base's (dark or light); the visuals follow. A
    // followed file (#7573) wins while it is followed and has read completely once.
    follow_theme_file(app, ctx);
    let file = active_theme_file(app);
    let custom = crate::theme::prepare_tabs(ctx, file, file.and_then(|f| app.preferences.theme_options.get(&f.id)), || {
        file.map(|f| effective_theme_options(&app.preferences, f).0).unwrap_or_default()
    });
    if let Some(custom) = &custom {
        app.ui.dark = custom.dark;
    }
    // Custom Day/Night palettes still need native OS appearance notifications. Fixing
    // the native window to the slot's tone can stop those notifications on macOS.
    let follows_system = app.preferences.theme == "system" && followed_theme(&app.preferences, &app.prefs_window.follow).is_none();
    if crate::theme::set_custom(custom) {
        app.styled = false;
    }
    sync_window_theme(app, ctx, follows_system);
    if crate::theme::set_logo_accent(app.preferences.logo_accent) {
        app.styled = false;
    }
    // egui's end-of-frame shortcuts only change its context; the saved preference would
    // overwrite that change next frame. Consume the desktop shortcuts into the preference.
    if ctx.options(|o| o.zoom_with_keyboard) && !crate::keymap::capturing(app) {
        use egui::gui_zoom::kb_shortcuts::{ZOOM_IN, ZOOM_IN_SECONDARY, ZOOM_OUT, ZOOM_RESET};
        if ctx.input_mut(|i| i.consume_shortcut(&ZOOM_RESET)) {
            app.preferences.ui_scale = 1.0;
        } else {
            let zoom_in = ctx.input_mut(|i| i.consume_shortcut(&ZOOM_IN) || i.consume_shortcut(&ZOOM_IN_SECONDARY));
            let zoom_out = ctx.input_mut(|i| i.consume_shortcut(&ZOOM_OUT));
            if zoom_in != zoom_out {
                app.preferences.ui_scale = (app.preferences.ui_scale + if zoom_in { 0.1 } else { -0.1 }).clamp(0.5, 2.5);
            }
        }
    }
    // Scaling while the pointer is held moves the slider under the pointer, feeding a
    // different value back into it. Commit the layout change after the drag ends.
    let z = app.preferences.ui_scale.clamp(0.5, 2.5);
    if z.is_finite() && !ctx.input(|i| i.pointer.any_down()) && (ctx.zoom_factor() - z).abs() > 1e-3 {
        ctx.set_zoom_factor(z);
    }
}

/// The user's analysis policy applies immediately to every open design, without document edits.
pub(crate) fn sync_surface_analysis(app: &mut SketApp) {
    let single = app.preferences.single_surface_analysis;
    app.session.set_single_surface_analysis(single);
    for slot in app.docs.background_mut() {
        slot.session.set_single_surface_analysis(single);
    }
}

/// The network locations the files a design names may be on follow the preferences.
pub fn apply_network(p: &Prefs) {
    sketcraft_engine::io::vfs::set_network_policy(sketcraft_engine::io::vfs::NetworkPolicy {
        allow_all: p.network_locations,
        hosts: p.network_hosts.clone(),
    });
}

/// A new, empty design takes the preferred units (not an undo step, not a change to save).
/// Callers: a launch with no file, New on a design that is already empty, `file.new`, and the
/// blank design left when the last tab closes. A design that already has features is unchanged.
pub fn apply_new_design(app: &mut SketApp) {
    apply_new_design_units(app, &app.preferences.default_units.clone());
}

/// The new-design preference also applies to the blank, unsaved design already open.
fn default_units_changed(app: &mut SketApp) {
    if crate::documents::active_is_blank(app) {
        apply_new_design(app);
    }
}

/// Explicit units for a new-design tile; the user's default preference stays unchanged.
pub(crate) fn apply_new_design_units(app: &mut SketApp, units: &str) {
    if app.session.doc.features.is_empty() && app.session.doc.units != units {
        let saved = !app.session.is_dirty();
        if app.session.execute("document.units", &json!({ "units": units })).is_ok() {
            app.session.undo.pop();
            if saved {
                app.session.mark_saved();
            }
        }
    }
}

/// The autosave interval follows `app.autosave_minutes` (0: off).
pub fn set_autosave(app: &mut SketApp) {
    let m = app.autosave_minutes;
    if let Some(a) = app.autosave.as_mut() {
        a.interval = if m > 0.0 { std::time::Duration::from_secs_f64(m * 60.0) } else { std::time::Duration::from_secs(u64::MAX / 4) };
    }
}

/// Which navigation dragging `button` with `mods` held does under the preset.
pub fn nav_mode(app: &SketApp, button: NavButton, mods: Modifiers) -> Option<NavMode> {
    // With mouse gestures on, the right button opens the ring of tools instead of orbiting.
    if button == NavButton::Right && crate::mouse_gesture::active(app) {
        return None;
    }
    let mode = nav_preset(&app.preferences.nav).mode(button, mods);
    // "Right drag orbits" off leaves the right button to the context menu (#6492).
    if button == NavButton::Right && mode == Some(NavMode::Orbit) && !app.preferences.right_drag_orbits {
        return None;
    }
    mode
}

/// +1, or −1 when horizontal scrolling pans the other way (#2007).
pub fn pan_sign_x(app: &SketApp) -> f32 {
    if app.preferences.pan_reverse_x { -1.0 } else { 1.0 }
}

/// +1, or −1 when vertical scrolling pans the other way.
pub fn pan_sign_y(app: &SketApp) -> f32 {
    if app.preferences.pan_reverse_y { -1.0 } else { 1.0 }
}

/// +1, or −1 when Shift with two-finger scrolling orbits the other way (#8410).
pub fn orbit_sign(app: &SketApp) -> f64 {
    if app.preferences.orbit_reverse { -1.0 } else { 1.0 }
}

/// Whether the configured suppress-snap chord is held.
pub fn suppress_sketch_snap_held(app: &SketApp, m: Modifiers) -> bool {
    let c = app.preferences.suppress_sketch_snap;
    let held = crate::tool_modifiers::Held::from_modifiers(m);
    // Classic default (`CTRL_ALT`): Ctrl/Cmd **or** Alt/Option each suppress snap.
    // A single-key preference keeps the usual AtLeast match for that key only.
    if c == ModifierCombo::CTRL_ALT {
        return held.primary || held.alt;
    }
    c.held(m)
}

/// Primary always frees manipulator arrows; Alt/Option also frees when `manipulator_free_alt` is on.
pub fn manipulator_free_held(app: &SketApp, m: Modifiers) -> bool {
    use crate::tool_modifiers::Held;
    let held = Held::from_modifiers(m);
    held.primary || (app.preferences.manipulator_free_alt && held.alt)
}

/// +1, or −1 when the wheel zooms the other way.
pub fn zoom_sign(app: &SketApp) -> f64 {
    if app.preferences.zoom_reverse { -1.0 } else { 1.0 }
}

/// The model point under a screen position: on a face, vertex, edge or sketch curve, the point
/// the cursor is on (on an edge or a curve too, not its middle, which can be far off along a long
/// one). Over empty space: the centre of the visible bodies and sketches (what Fit frames), or
/// the view's centre when nothing is visible.
pub(crate) fn pivot_under(app: &SketApp, proj: &crate::viewport::Proj, at: Pos2) -> Vec3 {
    use crate::viewport::Hit;
    for h in crate::viewport::pick(app, proj, at) {
        match h {
            Hit::Face { point, .. } | Hit::Vertex { point, .. } | Hit::Edge { point, .. } | Hit::SketchCurve { point, .. } => {
                return point;
            }
            _ => {}
        }
    }
    // Over empty space: the middle of the bodies in view, not of everything shown (#2616).
    if let Some(c) = crate::viewport::in_view_centre(app, proj) {
        return c;
    }
    let b = crate::viewport::view_bounds(app);
    if b.is_empty() { app.cam.target } else { b.center() }
}

/// A plain number typed into a field, with a decimal point or a decimal comma (#1347): "2.5"
/// and "2,5" are both 2.5, as a numeric keypad types either depending on the locale. Spaces are
/// ignored and "−" is a minus, as in egui's own parser. A comma beside a point ("1.234,5") or a
/// second comma is not guessed at: the text is not a number.
pub fn parse_number(text: &str) -> Option<f64> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).map(|c| if c == '\u{2212}' { '-' } else { c }).collect();
    let t = if !t.contains('.') && t.matches(',').count() == 1 { t.replacen(',', ".", 1) } else { t };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

// Existing callers retain their numeric-editor API; kit owns the widget.

/// A length typed into a field, in millimetres: a bare number is in the design's units (`scale`
/// units per millimetre); an explicit unit ("10 mm", "0.5 in", "2 cm + 3 mm") is honoured.
pub fn parse_length_mm(text: &str, scale: f64) -> Option<f64> {
    use sketcraft_engine::doc::expr;
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let defaults = expr::Defaults { len: 1.0 / scale, ..expr::Defaults::default() };
    let no_params = |n: &str| Err(sketcraft_engine::doc::DocError::Expr(format!("unknown parameter `{n}`")));
    let value = expr::eval_with_defaults(text.trim(), &no_params, defaults).ok()?.to_kind_in(expr::Kind::Length, defaults).ok()?;
    value.is_finite().then_some(value)
}

/// A length, area (`power` 2) or volume (3) given in millimetres, shown in the design's units
/// with the length precision.
pub fn show_mm(app: &SketApp, v: f64, power: i32) -> String {
    let (k, u) = unit_scale(&app.session.doc.units);
    let sup = match power {
        2 => "²",
        3 => "³",
        _ => "",
    };
    let value = v * k.powi(power);
    format!("{:.*} {u}{sup}", length_precision(value, &app.session.doc.units, app.preferences.length_decimals), value)
}

/// Numbers in millimetres, in the design's units without the unit name.
pub fn show_mm_bare(app: &SketApp, v: f64) -> String {
    length_number(v, &app.session.doc.units, app.preferences.length_decimals)
}

/// The window system choices (`Prefs::window_system`, #3083).
pub const WINDOW_SYSTEMS: &[&str] = &["auto", "wayland", "x11"];
/// winit's issue for dropping files onto a window on native Wayland, which the note links to.
pub const WAYLAND_DROP_ISSUE: &str = "https://github.com/rust-windowing/winit/issues/1881";

/// Linux: Graphics › Window system (#3083). It applies at the next start; the note under it says
/// which one is running now, and when native Wayland is in use or chosen, that some features are
/// limited there, linking to [`WAYLAND_DROP_ISSUE`].
#[cfg(target_os = "linux")]
fn window_system_row(app: &mut SketApp, ui: &mut egui::Ui, label_width: f32) {
    let t = Tokens::get();
    let label = |id: &str| crate::i18n::tr(&format!("ui.window_system_{id}")).to_string();
    let labels: Vec<_> = WINDOW_SYSTEMS.iter().copied().map(label).collect();
    let labels: Vec<_> = labels.iter().map(String::as_str).collect();
    let now = app.services.window_system.map(label);
    let text = match &now {
        Some(now) => crate::i18n::format("settings.window_system_note_now", &[("now", now)]),
        None => app.tr("settings.msaa_applies_on_restart").to_string(),
    };
    let row_width = (label_width + t.space_14 + t.space_24 * 10.0).min(ui.available_width());
    ui.horizontal_wrapped(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(row_width, t.control_height), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui_kit::settings_form(ui, label_width, |ui| {
                ui_kit::settings_row_help(ui, app.tr("ui.window_system"), &text, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2((t.space_24 * 10.0).min(ui.available_width()), t.control_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let mut selected = WINDOW_SYSTEMS.iter().position(|id| *id == app.preferences.window_system).unwrap_or(0);
                            let before = selected;
                            let response = ui_kit::dropdown(ui, "window_system", &mut selected, &labels);
                            crate::scenario::publish_target("prefs:window_system", response.rect);
                            if selected != before
                                && let Some(id) = WINDOW_SYSTEMS.get(selected)
                            {
                                app.preferences.window_system = (*id).into();
                            }
                        },
                    );
                });
            });
        });
        ui_kit::settings_badge(ui, "Linux", false);
    });
    let choice = app.preferences.window_system.as_str();
    if choice == "wayland" || (choice == "auto" && app.services.window_system == Some("wayland")) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(app.tr("settings.window_system_wayland_limited")).size(t.font_small).color(t.warning));
            ui.hyperlink_to(egui::RichText::new(app.tr("settings.window_system_details")).size(t.font_small), WAYLAND_DROP_ISSUE);
        });
    }
}

/// Write the anti-aliasing and window system settings where the host reads them at start (neither
/// can change while the window is open).
fn save_graphics(app: &SketApp) {
    if let Some(path) = &app.services.graphics_path {
        // Other settings in the file (the graphics `backend`) stay as they are.
        let mut graphics = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| json!({}));
        graphics["msaa"] = json!(app.preferences.msaa);
        graphics["window_system"] = json!(app.preferences.window_system);
        graphics["adapter"] = json!(app.preferences.graphics_adapter);
        let _ = std::fs::write(path, graphics.to_string());
    }
}

/// At start, reflect a confirmed startup fallback in Graphics, or reset an unsupported
/// saved count to the supported default. The compatibility hint keeps higher choices
/// available for another try; a probe result alone does not overwrite the saved choice.
pub fn check_msaa(app: &mut SketApp, requested: u32) {
    if app.msaa_compatibility {
        app.preferences.msaa = 1;
        return;
    }
    let supported = &app.msaa_supported;
    if requested <= 1 || supported.is_empty() || supported.contains(&requested) {
        return;
    }
    let default = Prefs::default().msaa;
    let reset = supported.iter().copied().filter(|&n| n <= default).max().unwrap_or(1);
    let supported = supported.clone();
    app.preferences.msaa = reset;
    save_graphics(app);
    app.settings_reset.push(SettingReset::Msaa { requested, reset, supported });
}

/// A setting put back at start because it would keep SketCraft from starting.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingReset {
    /// Anti-aliasing samples the graphics device can't render with.
    Msaa { requested: u32, reset: u32, supported: Vec<u32> },
}

impl SettingReset {
    /// The line the notice shows, in the current interface language.
    pub fn message(&self) -> String {
        match self {
            SettingReset::Msaa { requested, reset, supported } => {
                let setting = format!("{} › {}", crate::i18n::tr("ui.graphics"), crate::i18n::tr("ui.anti_aliasing"));
                let supported = supported.iter().map(|n| format!("{n}")).collect::<Vec<_>>().join("×, ") + "×";
                crate::i18n::format(
                    "settings.msaa_reset",
                    &[("setting", &setting), ("requested", &requested.to_string()), ("reset", &reset.to_string()), ("supported", &supported)],
                )
            }
        }
    }
}

/// The Preferences window can't be folded (`collapsible(false)`), but 0.14 and 0.15 folded it
/// when a font was picked, and egui keeps that in the saved window state: it then opened as its
/// title bar alone, for good (#1800). Unfold it.
fn unfold(ctx: &egui::Context) {
    if let Some(mut body) = egui::collapsing_header::CollapsingState::load(ctx, egui::Id::unique("sc_prefs").with("collapsing"))
        && !body.is_open()
    {
        body.set_open(true);
        body.store(ctx);
    }
}

pub fn show(app: &mut SketApp, ctx: &egui::Context) {
    crate::settings_profiles::rename_popup(app, ctx);
    // File › Keyboard Shortcuts and Help › Keyboard Shortcuts open this dialog at that page.
    if app.keymap.open {
        app.keymap.open = false;
        open_at(app, PAGE_SHORTCUTS);
    }
    app.prefs_window.import_tile = None;
    if !app.prefs_window.open {
        app.prefs_window.filter.clear();
        app.prefs_window.search_from = None;
        app.prefs_window.land_on = None;
        app.prefs_window.reveal = None;
        crate::prefs_search::set_reveal(ctx, None);
        app.prefs_window.snapshot = None;
        return;
    }
    if app.prefs_window.snapshot.is_none() {
        app.crashes.refresh();
        app.prefs_window.snapshot = Some(Snapshot::take(app));
    }
    unfold(ctx);
    if let Some(reveal) = &app.prefs_window.reveal
        && crate::now_ms() > reveal.until
    {
        app.prefs_window.reveal = None;
    }
    let sink = crate::prefs_search::shift_key_sink(ctx);
    crate::prefs_search::set_key_sink_prev(ctx, sink);
    let mut button: Option<&str> = None;
    let before = app.preferences.clone();
    let capturing = crate::keymap::capturing(app);
    let pending = apply_pending(app);
    let window = ui_kit::Window::new(egui::Id::unique("sc_prefs"), "settings", app.tr("ui.preferences"))
        .cards(10.0 / 3.0)
        .page_rows(24.0)
        .page_header_in_titlebar()
        .page_padding(egui::Margin {
            left: (Tokens::get().space_24 - Tokens::get().space_2) as i8,
            right: (Tokens::get().space_24 - Tokens::get().space_2) as i8,
            top: (Tokens::get().space_16 + Tokens::get().space_2) as i8,
            bottom: Tokens::get().space_10 as i8,
        })
        .fields()
        .actions("OK", "Cancel", Some("Apply"))
        .apply_enabled(pending);
    let window = if capturing { window.keep_open_on_escape() } else { window };
    let mut selected = app.prefs_window.section;
    let (closed, action) = window.show_pages_with_header(
        ctx,
        &mut selected,
        WINDOW_PAGES,
        app,
        search_header,
        clear_search,
        Some((Tokens::get().space_24 * 2.0, crate::settings_profiles::sidebar)),
        |ui| {
            let defaults = ui_kit::button(ui, crate::i18n::tr("prefs.restore_page_defaults"), ui_kit::Button::Ghost)
                .backing_on_hover_text(crate::i18n::tr("ui.reset_the_settings_on_this_page"));
            crate::scenario::publish_target("prefs:defaults", defaults.rect);
            if defaults.clicked() {
                button = Some("defaults");
            }
        },
    );
    if let Some(rect) = crate::scenario::rect_of("dialog:page_body") {
        crate::scenario::publish_rect("prefs:page_body", rect);
    }
    for name in ["window", "footer", "ok", "apply", "cancel"] {
        if let Some(rect) = crate::scenario::rect_of(&format!("dialog:{name}")) {
            crate::scenario::publish_target(&format!("prefs:{name}"), rect);
        }
    }
    if let Some(rect) = crate::scenario::rect_of("dialog:footer:actions") {
        crate::scenario::publish_rect("prefs:actions", rect);
    }
    button = match action {
        ui_kit::Action::Ok => Some("ok"),
        ui_kit::Action::Apply => Some("apply"),
        ui_kit::Action::Cancel => Some("cancel"),
        ui_kit::Action::None => button,
    };
    if app.preferences.msaa != before.msaa
        || app.preferences.window_system != before.window_system
        || app.preferences.graphics_adapter != before.graphics_adapter
    {
        save_graphics(app);
    }
    if app.preferences.up_axis != before.up_axis {
        up_axis_changed(app);
    }
    match button {
        Some("defaults") => defaults(app, app.prefs_window.section),
        Some("apply") => app.prefs_window.snapshot = Some(Snapshot::take(app)),
        Some("ok") => close(app, false),
        Some("cancel") => close(app, true),
        _ if closed => close(app, true),
        _ => {}
    }
    // The footer was drawn before this frame's edits (and before an Apply): show Apply's new
    // state on the next frame rather than at the next input.
    if app.prefs_window.open && apply_pending(app) != pending {
        ctx.request_repaint();
    }
}

/// Close the dialog; `revert`: put back what was there when it opened (Cancel, Esc, ×).
pub(crate) fn close(app: &mut SketApp, revert: bool) {
    if let Some(s) = app.prefs_window.snapshot.take()
        && revert
    {
        let up = app.preferences.up_axis.clone();
        s.restore(app);
        if app.preferences.up_axis != up {
            up_axis_changed(app);
        }
    }
    app.prefs_window.open = false;
    app.prefs_window.filter.clear();
    app.prefs_window.search_from = None;
    app.prefs_window.land_on = None;
    app.prefs_window.reveal = None;
    app.keymap.read_only = false;
    crate::keymap::stop_capture(app);
}

/// The default orientation changed: the view turns to match.
fn up_axis_changed(app: &mut SketApp) {
    use sketcraft_engine::render::UpAxis;
    app.prefs_window.applied_up = Some(app.preferences.up_axis.clone());
    let up = if app.preferences.up_axis.eq_ignore_ascii_case("y") { UpAxis::Y } else { UpAxis::Z };
    if app.ui.up_axis != up || app.cam.up != up {
        app.set_up_axis(up);
    }
}

/// One page of the dialog. Inputs share the kit's aligned rows and tokenised controls.
/// A search draws every page and keeps the controls that match.
fn page(app: &mut SketApp, ui: &mut egui::Ui, t: &Tokens) {
    let _search = crate::prefs_search::begin(ui, &app.prefs_window.filter);
    if crate::prefs_search::querying(ui) {
        // Paint only the pages that match. A first pass is invisible, so the
        // agent list cannot draw on top of the setting that did match.
        let mut keep = vec![false; WINDOW_PAGES.len()];
        for (index, _) in WINDOW_PAGES.iter().enumerate() {
            crate::prefs_search::begin_page(ui, index, &page_path(index));
            let before = crate::prefs_search::hits(ui);
            let mut probe = crate::prefs_search::detached_child(ui, ("prefs-search-probe", index), true);
            draw_page(app, &mut probe, index, t);
            keep[index] = crate::prefs_search::hits(ui) > before;
        }
        crate::prefs_search::rewind_search_layout(ui);
        for (index, _) in WINDOW_PAGES.iter().enumerate() {
            if !keep.get(index).copied().unwrap_or(false) {
                continue;
            }
            crate::prefs_search::begin_page(ui, index, &page_path(index));
            let before = crate::prefs_search::hits(ui);
            let mut child = crate::prefs_search::detached_child(ui, ("prefs-search-page", index), false);
            draw_page(app, &mut child, index, t);
            crate::prefs_search::commit_search_child(ui, &child, before);
        }
        if crate::prefs_search::hits(ui) == 0 {
            ui.label(egui::RichText::new(crate::i18n::tr("prefs.search_empty")).size(t.font_body).color(t.form_dim));
        }
    } else {
        let label = page_path(app.prefs_window.section);
        crate::prefs_search::begin_page(ui, app.prefs_window.section, &label);
        draw_page(app, ui, app.prefs_window.section, t);
    }
    crate::prefs_search::store_nav(ui);
}

fn draw_page(app: &mut SketApp, ui: &mut egui::Ui, index: usize, t: &Tokens) {
    match index {
        PAGE_SHORTCUTS => {
            let read_only = app.keymap.read_only;
            let mut show = |ui: &mut egui::Ui| {
                ui_kit::settings_heading_with_tag(
                    ui,
                    app.tr("prefs.shortcuts"),
                    app.tr("prefs.shortcuts_intro"),
                    Some(crate::settings_profiles::tag),
                );
                crate::keymap::page(app, ui);
            };
            if read_only {
                show(ui);
            } else {
                ui_kit::settings_page(ui, show);
            }
        }
        PAGE_ASSISTANT => crate::agent_chat::settings::page(app, ui),
        PAGE_PLUGINS => crate::plugins::page(app, ui),
        PAGE_GENERAL => general::show(app, ui),
        PAGE_APPEARANCE => interface::appearance(app, ui, t),
        PAGE_LAYOUT => interface::layout(app, ui),
        PAGE_TOOLS => tools::show(app, ui),
        PAGE_DESIGN => design::show(app, ui),
        PAGE_NAVIGATION => navigation::show(app, ui, t),
        PAGE_GRAPHICS => graphics::show(app, ui),
        PAGE_SKETCH => sketch::show(app, ui),
        _ => {}
    }
}

fn page_index(name: &str) -> Option<usize> {
    PAGES
        .iter()
        .position(|page| page.eq_ignore_ascii_case(name))
        .or_else(|| PAGE_ALIASES.iter().find_map(|(alias, page)| alias.eq_ignore_ascii_case(name).then_some(*page)))
}

/// The control channel's `ui.prefs`: `open` (a page name) opens the dialog there, `button`
/// presses OK, Cancel, Apply or Restore Defaults, `stand_up: true` answers the stand-up offer,
/// dialog or note (`false` leaves the file as it is, like "Leave it"), `theme_import` (a theme file's JSON or
/// text) adds a theme and turns it on, `theme_remove` (an id) removes an imported one and
/// `theme_export: true` returns the theme on now as `theme_file`, `theme_follow` (a path, or
/// `""` to stop) follows a theme file on the desktop (#7573); any other keys set preferences.
/// `open: "Interface/Navigation bar"` opens Interface and scrolls to its bar editor.
/// Returns them all, the open page, the offer (and whether it is the note), the theme ids and the
/// followed file (`theme_followed`, `theme_follow_error`).
pub fn control(app: &mut SketApp, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    // Stage every fallible preference operation before dialogs, document actions or writes.
    let mut rest = p.as_object().cloned().ok_or("preferences must be an object")?;
    let page = rest
        .remove("open")
        .map(|page| {
            let name = page.as_str().unwrap_or("General");
            page_index(name)
                .map(|i| (i, name.eq_ignore_ascii_case("Interface/Navigation bar")))
                .ok_or_else(|| format!("no page `{name}` ({})", PAGES.join(", ")))
        })
        .transpose()?;
    let button = rest.remove("button");
    if let Some(b) = button.as_ref().and_then(|v| v.as_str())
        && !["ok", "cancel", "apply", "defaults"].contains(&b)
    {
        return Err(format!("unknown button `{b}` (ok, cancel, apply, defaults)"));
    }
    let stand_up = rest.remove("stand_up").and_then(|v| v.as_bool());
    let mut staged = app.preferences.clone();
    let mut follow = app.prefs_window.follow.clone();
    let imported = rest
        .remove("theme_import")
        .map(|value| {
            let theme = match value {
                serde_json::Value::String(text) => crate::theme_files::parse(&text)?,
                other => crate::theme_files::from_value(&other)?,
            };
            let id = crate::theme_files::add(&mut staged.custom_themes, theme)?;
            staged.theme = id.clone();
            Ok::<_, String>(id)
        })
        .transpose()?;
    if let Some(id) = rest.remove("theme_remove") {
        let id = id.as_str().unwrap_or_default();
        let i = staged.custom_themes.iter().position(|t| t.id == id).ok_or_else(|| format!("no imported theme `{id}`"))?;
        let gone = staged.custom_themes.remove(i);
        staged.theme_options.remove(id);
        if staged.theme == id {
            staged.theme = if gone.base.is_dark() { "dark" } else { "light" }.into();
        }
    }
    match rest.remove("theme_follow") {
        Some(value) => {
            if !FOLLOW_AVAILABLE {
                return Err(FOLLOW_UNAVAILABLE.into());
            }
            let path = value.as_str().ok_or("`theme_follow` must be a theme file's path, or an empty string to stop following")?;
            staged.theme_follow = crate::theme_files::follow_path(path)?;
            follow.launch = None;
            let _ = follow.poll(&staged.theme_follow, f64::NEG_INFINITY, true);
        }
        None if rest.contains_key("theme") => {
            staged.theme_follow.clear();
            follow.launch = None;
            let _ = follow.poll("", f64::NEG_INFINITY, true);
        }
        None => {}
    }
    let export = rest.remove("theme_export").and_then(|v| v.as_bool()).unwrap_or(false);
    let new = prepare_preferences(app, &staged, followed_theme(&staged, &follow), &serde_json::Value::Object(rest.clone()))?;
    let stopped_following = !app.prefs_window.follow.path(&app.preferences.theme_follow).is_empty() && follow.path(&new.theme_follow).is_empty();
    if stand_up == Some(true) && app.stand_up.is_none() {
        return Err("no file is waiting to be stood up".into());
    }
    if let Some((i, navbar)) = page {
        open_at(app, i);
        app.prefs_window.navbar_scroll = navbar;
        if app.prefs_window.snapshot.is_none() {
            app.prefs_window.snapshot = Some(Snapshot::take(app));
        }
    }
    match stand_up {
        Some(true) => crate::stand_up::stand_up(app)?,
        Some(false) => crate::stand_up::leave(app),
        None => {}
    }
    if stopped_following {
        // Restore the selected theme's dark/light mode even when its saved id did not change.
        stop_following(app);
    }
    app.prefs_window.follow = follow;
    let mut out = apply_preferences(app, new, &serde_json::Value::Object(rest))?;
    match button.as_ref().and_then(|b| b.as_str()) {
        Some("ok") => close(app, false),
        Some("cancel") => close(app, true),
        Some("apply") => app.prefs_window.snapshot = Some(Snapshot::take(app)),
        Some("defaults") => defaults(app, app.prefs_window.section),
        Some(b) => return Err(format!("unknown button `{b}` (ok, cancel, apply, defaults)")),
        None => {}
    }
    if button.is_some() {
        // Cancel and Restore Defaults can change the angle too.
        follow_view_fov(app);
        out = serde_json::to_value(&app.preferences).map_err(|e| e.to_string())?;
    }
    if let Some(o) = out.as_object_mut() {
        if let serde_json::Value::Object(options) = serde_json::to_value(theme_options_reply(app)).map_err(|e| e.to_string())? {
            o.extend(options);
        }
        o.insert("dialog".into(), json!(app.prefs_window.open.then(|| PAGES.get(app.prefs_window.section).copied().unwrap_or_default())));
        o.insert("apply_enabled".into(), json!(apply_pending(app)));
        o.insert("stand_up_offer".into(), json!(app.stand_up));
        o.insert("stand_up_note".into(), json!(app.stand_up.is_some() && app.stand_up_note));
        o.insert("dark".into(), json!(app.ui.dark));
        o.insert("viewport_dark".into(), json!(app.viewport_dark()));
        o.insert("theme_ids".into(), json!(theme_choices(app).into_iter().map(|(id, _)| id).collect::<Vec<_>>()));
        let follow = &app.prefs_window.follow;
        let followed = follow.path(&app.preferences.theme_follow);
        o.insert("theme_followed".into(), json!((!followed.is_empty()).then_some(followed)));
        o.insert("theme_follow_error".into(), json!(follow.error().filter(|_| !followed.is_empty())));
        if let Some(id) = imported {
            o.insert("theme_imported".into(), json!(id));
        }
        if export {
            o.insert("theme_file".into(), current_theme_file(app));
        }
    }
    Ok(out)
}

/// Read stored theme identity/base tables with the existing serde contract, then normalize only
/// option tables using the file-import parser. A stale declaration must not lose the base palette.
fn theme_from_preferences(value: &serde_json::Value, strict: bool) -> Result<crate::theme_files::ThemeFile, String> {
    let mut base = value.clone();
    if let Some(object) = base.as_object_mut() {
        object.remove("options");
        object.remove("overlays");
        object.remove("tabs");
    }
    let mut theme: crate::theme_files::ThemeFile = serde_json::from_value(base).map_err(|e| e.to_string())?;
    if let Err(error) = theme.set_tabs(value) {
        if strict {
            return Err(error);
        }
        log::warn!("preferences: theme `{}` has invalid tab surfaces ({error}); keeping its base palette", theme.id);
    }
    if let Err(error) = theme.set_option_tables(value) {
        if strict {
            return Err(error);
        }
        log::warn!("preferences: theme `{}` has invalid option tables ({error}); keeping its base palette", theme.id);
    }
    Ok(theme)
}

fn themes_that_read<'de, D: serde::Deserializer<'de>>(saved: D) -> Result<Vec<crate::theme_files::ThemeFile>, D::Error> {
    Vec::<serde_json::Value>::deserialize(saved)?.iter().map(|value| theme_from_preferences(value, false).map_err(serde::de::Error::custom)).collect()
}

/// Every theme the Theme list offers, as (id, name): the modes, the built-in theme files, then
/// the imported ones.
fn theme_choices(app: &SketApp) -> Vec<(String, String)> {
    let modes = THEMES.iter().filter(|(id, _)| *id != "system").map(|(id, label)| ((*id).to_string(), crate::i18n::tr_label(label).to_string()));
    let files = crate::theme_files::builtin().iter().chain(&app.preferences.custom_themes).map(|t| (t.id.clone(), t.name.clone()));
    let mut choices: Vec<_> = modes.chain(files).collect();
    choices.sort_by_key(|(id, _)| match id.as_str() {
        "light" => 0,
        "dark" => 1,
        _ => 2,
    });
    choices
}

/// Turn a theme on (a mode or a theme file's id); dark follows it.
fn select_theme(app: &mut SketApp, id: &str) {
    app.preferences.theme = id.to_string();
    match crate::theme_files::find(id, &app.preferences.custom_themes) {
        Some(f) => app.ui.dark = f.base.is_dark(),
        None if id != "system" => app.ui.dark = id == "dark",
        None => {}
    }
}

/// Import a theme file (its text, or its JSON) into the preferences and turn it on; returns
/// the id it is kept under.
/// Import a theme file's text, from a file, a paste or a drop, and say what happened in the status bar.
fn import_theme_text(app: &mut SketApp, text: Result<String, String>) {
    match text.and_then(|text| import_theme(app, &serde_json::Value::String(text))) {
        Ok(id) => {
            let name = crate::theme_files::find(&id, &app.preferences.custom_themes).map(|t| t.name.clone()).unwrap_or(id);
            app.set_status(format!("Theme imported: {name}"), false);
        }
        Err(e) => app.set_status(format!("Theme import: {e}"), true),
    }
}

/// Text that is meant as a theme file: a JSON object naming the theme format. Anything else
/// pasted while Preferences is open keeps its usual meaning.
fn looks_like_theme(text: &str) -> bool {
    text.trim_start().starts_with('{') && text.contains(crate::theme_files::FORMAT)
}

/// Ctrl/Cmd+V on the Appearance page with a theme file's text on the clipboard imports it, like
/// Import… with a file (#8226). Called before the global Copy and Paste shortcuts, which would
/// otherwise paste copied components; the paste is taken out of the input once it is a theme's.
pub fn take_theme_paste(app: &mut SketApp, ctx: &egui::Context) {
    if !app.prefs_window.open || app.prefs_window.section != PAGE_APPEARANCE {
        return;
    }
    let Some(text) = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None })) else {
        return;
    };
    if !looks_like_theme(&text) {
        return;
    }
    ctx.input_mut(|i| {
        i.events.retain(|e| {
            !matches!(e, egui::Event::Paste(_) | egui::Event::Key { key: egui::Key::V, .. } | egui::Event::Key { key: egui::Key::Insert, .. })
        })
    });
    import_theme_text(app, Ok(text));
}

/// Files dropped on the theme Import tile are themes to import, not designs to open (#8226).
pub fn take_theme_drops(app: &mut SketApp, ctx: &egui::Context) {
    if !theme_drop_target(app, ctx) || !ctx.input(|i| !i.raw.dropped_files.is_empty()) {
        return;
    }
    let files = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
    for f in files.iter().take(8) {
        // The browser reads dropped files only asynchronously: the web shell takes them first and
        // hands a theme's bytes to `import_theme_bytes` (#8226), so none reach this on wasm.
        #[cfg(not(target_arch = "wasm32"))]
        match f.bytes() {
            Ok(bytes) => import_theme_bytes(app, bytes),
            Err(e) => import_theme_text(app, Err(e.to_string())),
        }
        #[cfg(target_arch = "wasm32")]
        let _ = f;
    }
}

/// Whether files dropped now land on the Import tile: the Preferences window is open and the
/// pointer was last seen over the tile.
pub fn theme_drop_target(app: &SketApp, ctx: &egui::Context) -> bool {
    app.prefs_window
        .import_tile
        .filter(|_| app.prefs_window.open)
        .is_some_and(|rect| ctx.input(|i| i.pointer.latest_pos()).is_some_and(|p| rect.contains(p)))
}

/// A file dropped on the Import tile, as bytes: imported when it is a theme, refused with the reason
/// otherwise.
pub fn import_theme_bytes(app: &mut SketApp, bytes: Vec<u8>) {
    let text = if bytes.len() > crate::theme_files::MAX_BYTES {
        Err("the file is too large for a theme".to_string())
    } else {
        String::from_utf8(bytes).map_err(|_| "not a theme file".to_string())
    };
    import_theme_text(app, text);
}

pub fn import_theme(app: &mut SketApp, v: &serde_json::Value) -> Result<String, String> {
    let theme = match v {
        serde_json::Value::String(text) => crate::theme_files::parse(text)?,
        other => crate::theme_files::from_value(other)?,
    };
    let id = crate::theme_files::add(&mut app.preferences.custom_themes, theme)?;
    if app.prefs_window.open && app.prefs_window.section == PAGE_APPEARANCE {
        theme_picker::choose(app, app.prefs_window.theme_picker_night, &id);
    } else {
        select_theme(app, &id);
    }
    Ok(id)
}

/// Remove an imported theme; when it is on, its base (dark or light) takes over.
fn remove_theme(app: &mut SketApp, id: &str) -> Result<(), String> {
    let Some(i) = app.preferences.custom_themes.iter().position(|t| t.id == id) else {
        return Err(format!("no imported theme `{id}`"));
    };
    let gone = app.preferences.custom_themes.remove(i);
    app.preferences.theme_options.remove(id);
    if app.preferences.theme == id {
        select_theme(app, if gone.base.is_dark() { "dark" } else { "light" });
    }
    Ok(())
}

/// The theme on now as a complete theme file, a starting point for a new one.
fn current_theme_file(app: &SketApp) -> serde_json::Value {
    match active_theme_file(app) {
        Some(f) => export_theme_file(f),
        None => theme_file(app, &app.preferences.theme, app.prefs_window.system_theme_dark.unwrap_or(app.ui.dark)),
    }
}

fn active_theme_file(app: &SketApp) -> Option<&crate::theme_files::ThemeFile> {
    followed_theme(&app.preferences, &app.prefs_window.follow).or_else(|| {
        crate::theme_files::find(
            theme_picker::active_id(&app.preferences, app.prefs_window.system_theme_dark.unwrap_or(app.ui.dark)),
            &app.preferences.custom_themes,
        )
    })
}

/// The single runtime decision used by controls, ui.prefs and renderer activation. It never
/// changes saved intent or caches accessibility/hardware state in palette tokens.
pub(crate) fn blur_availability(app: &SketApp) -> crate::theme_options::Availability {
    blur_availability_for(app, active_theme_file(app).is_some_and(crate::theme_options::declares_blur))
}

fn blur_availability_for(app: &SketApp, offered: bool) -> crate::theme_options::Availability {
    use crate::theme_options::Availability::{Available, Unavailable};
    let reduced = app.services.reduce_transparency.as_ref().and_then(|read| read(offered));
    if !offered && app.blur_session.harness_radius.is_none() {
        return Available;
    }
    if let Some(reason) = app.blur_session.harness_unavailable {
        return Unavailable(reason);
    }
    // State owns CPU policy, including the explicit cfg(test) rendering fixture. The
    // adapter identity stays real; production CPU capability can never be Available.
    let capability = app.blur.capability();
    if app.blur.software_adapter() && capability != crate::blur::Capability::Available {
        return Unavailable("prefs.blur_software");
    }
    if cfg!(target_arch = "wasm32") {
        return Unavailable("prefs.blur_web");
    }
    if reduced == Some(true) {
        return Unavailable("prefs.blur_reduce_transparency");
    }
    if app.home.open || crate::render_workspace::result_active(app) {
        return Unavailable("prefs.blur_source");
    }
    match capability {
        crate::blur::Capability::Available => Available,
        crate::blur::Capability::Unavailable(_) => Unavailable("prefs.blur_device"),
    }
}

pub(crate) fn prepare_blur(app: &mut SketApp, ctx: &egui::Context) {
    if ctx.viewport_id() != egui::ViewportId::ROOT {
        return;
    }
    let intent = crate::theme::blur_intent(ctx);
    let radius = app.blur_session.harness_radius.unwrap_or(intent.radius);
    // Disabled frames need only cached intent and atomic OS observation. Explicit UI/API
    // queries still report capabilities at radius zero; painting need not probe the GPU.
    let availability = if radius > 0.0 {
        blur_availability_for(app, intent.offered)
    } else {
        if let Some(read) = &app.services.reduce_transparency {
            read(intent.offered);
        }
        crate::theme_options::Availability::Available
    };
    // Resolve raw/default intent separately so temporary unavailability never drives the
    // renderer through radius=0 and clears a latched device/allocation failure.
    let available = availability == crate::theme_options::Availability::Available;
    app.blur_session.sample(ctx, intent.revision, radius, available, app.blur.painted_frame());
    app.blur.set_available(available);
    app.blur.set_tier_cap(app.blur_session.guard.cap);
    if let Err(error) = app.blur.set_radius(radius) {
        log::warn!("theme blur: {error}");
    }
}

fn followed_theme<'a>(prefs: &Prefs, follow: &'a crate::theme_files::Follow) -> Option<&'a crate::theme_files::ThemeFile> {
    (!follow.path(&prefs.theme_follow).is_empty()).then(|| follow.theme()).flatten()
}

/// Preserve authored definitions; themes predating options retain their complete snapshot export.
fn export_theme_file(file: &crate::theme_files::ThemeFile) -> serde_json::Value {
    if !file.options.is_empty() || !file.overlays.is_empty() || !file.tabs.is_empty() || file.needs_newer() {
        crate::theme_files::export_file(file)
    } else {
        crate::theme_files::export(&file.name, file.base, &crate::theme_files::resolve(file))
    }
}

fn effective_theme_options(
    prefs: &Prefs,
    file: &crate::theme_files::ThemeFile,
) -> (BTreeMap<String, crate::theme_files::OptionValue>, Vec<sketcraft_engine::schema::ui_prefs::ThemeOptionNotice>) {
    use crate::theme_files::{OptionKind, OptionNoticeKind, OptionValue};
    use sketcraft_engine::schema::ui_prefs::{ThemeOptionNotice, ThemeOptionNoticeKind as Kind};
    let mut saved = BTreeMap::new();
    let mut notices = Vec::new();
    let definitions = crate::theme_files::ordered_options(file);
    for (id, value) in prefs.theme_options.get(&file.id).into_iter().flatten() {
        let definition = definitions.iter().find(|o| &o.id == id);
        if let Some(definition) = definition
            && let OptionKind::Range { min, max, .. } = &definition.kind
            && value.as_f64().is_some_and(|n| n < f64::from(*min) || n > f64::from(*max))
        {
            notices.push(ThemeOptionNotice { id: id.clone(), kind: Kind::OutOfRange });
            continue;
        }
        match serde_json::from_value::<OptionValue>(value.clone()) {
            Ok(value) => {
                saved.insert(id.clone(), value);
            }
            Err(_) => {
                notices.push(ThemeOptionNotice { id: id.clone(), kind: if definition.is_some() { Kind::WrongType } else { Kind::UnknownOption } })
            }
        }
    }
    let (values, problems) = crate::theme_files::effective_values(file, &saved);
    notices.extend(problems.into_iter().map(|notice| ThemeOptionNotice {
        id: notice.id,
        kind: match notice.kind {
            OptionNoticeKind::UnknownOption => Kind::UnknownOption,
            OptionNoticeKind::WrongType => Kind::WrongType,
            OptionNoticeKind::UnknownChoice => Kind::UnknownChoice,
            OptionNoticeKind::NonFinite => Kind::NonFinite,
            OptionNoticeKind::Clamped => Kind::OutOfRange,
        },
    }));
    notices.sort_by(|a, b| a.id.cmp(&b.id));
    (values, notices)
}

fn resolved_theme(prefs: &Prefs, file: &crate::theme_files::ThemeFile) -> Tokens {
    crate::theme_files::resolve_with(file, &effective_theme_options(prefs, file).0)
}

fn theme_option_text(text: &crate::theme_files::OptionText, language: &str, fallback: &str) -> String {
    text.get(language).or_else(|| text.get("en")).map(String::as_str).unwrap_or(fallback).to_owned()
}

fn theme_option_rows<'a>(
    file: &'a crate::theme_files::ThemeFile,
    language: &str,
    providers: &[crate::theme_options::Provider],
    blur: crate::theme_options::Availability,
) -> Vec<ui_kit::ThemeOptionRow<'a>> {
    use crate::theme_options::{Availability, offered, provider};
    let locale = crate::i18n::Locale::for_language(language);
    offered(file, providers)
        .into_iter()
        .map(|definition| {
            let provider = provider(&definition.id, providers);
            ui_kit::ThemeOptionRow {
                definition,
                blur: definition.id == "blur",
                label: provider
                    .map(|p| locale.text(p.label).to_owned())
                    .unwrap_or_else(|| theme_option_text(&definition.label, language, &definition.id)),
                help: provider.map(|p| locale.text(p.help).to_owned()).unwrap_or_else(|| theme_option_text(&definition.help, language, "")),
                choices: match &definition.kind {
                    crate::theme_files::OptionKind::Choice { values, .. } => {
                        values.iter().map(|v| theme_option_text(&v.label, language, &v.id)).collect()
                    }
                    _ => Vec::new(),
                },
                reason: match crate::theme_options::availability(file, &definition.id, providers, blur) {
                    Availability::Available => None,
                    Availability::Unavailable(reason) => Some(locale.text(reason).to_owned()),
                },
            }
        })
        .collect()
}

fn theme_options_row(app: &mut SketApp, ui: &mut egui::Ui) {
    let availability = blur_availability(app);
    let Some(file) = active_theme_file(app).cloned() else { return };
    let (mut values, notices) = effective_theme_options(&app.preferences, &file);
    // Notices precede the empty-control return: newer requirements intentionally have no rows.
    let mut problems: Vec<_> = notices
        .iter()
        .map(|notice| {
            let raw = app.preferences.theme_options.get(&file.id).and_then(|v| v.get(&notice.id));
            (notice.id.clone(), format!("{:?}:{raw:?}", notice.kind), crate::i18n::format("prefs.theme_option_invalid", &[("option", &notice.id)]))
        })
        .collect();
    if file.needs_newer() {
        problems.push(("newer".into(), file.requires.to_string(), app.tr("prefs.theme_options_newer").to_owned()));
    }
    for (id, identity, text) in problems {
        let key = theme_notice_key(&file.id, &id, &identity);
        if !app.blur_session.dismissed(key) && ui_kit::theme_option_notice(ui, &id, &text, false).1 {
            app.blur_session.dismiss(key);
        }
    }
    if app.blur_session.guard.notice {
        let (undo, dismiss) = ui_kit::theme_option_notice(ui, "guard", app.tr("prefs.blur_guard"), true);
        if undo {
            app.blur_session.guard.undo();
            ui.ctx().request_repaint();
        }
        if dismiss {
            app.blur_session.guard.notice = false;
        }
    }
    let rows = theme_option_rows(&file, crate::i18n::effective_language(&app.preferences.language), crate::theme_options::PROVIDERS, availability);
    if rows.is_empty() {
        return;
    }
    let before = values.clone();
    let reset = ui_kit::settings_card(ui, |ui| ui_kit::theme_options_list(ui, &file.id, &file.name, &rows, &mut values)).inner;
    let changes: BTreeMap<_, _> = values.into_iter().filter(|(id, value)| before.get(id) != Some(value)).collect();
    if reset || !changes.is_empty() {
        let update = if reset { json!({}) } else { json!(changes) };
        let params = json!({"theme_options": {(file.id.clone()): update}});
        // Same atomic validation/merge and followed-file semantics as the control channel.
        // No button, page selection or dialog is added by this live preference update.
        if let Err(error) = control(app, &params) {
            app.set_status(error, true);
        }
    }
}

fn theme_notice_key(theme: &str, problem: &str, raw: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (theme, problem, raw).hash(&mut hash);
    hash.finish()
}

/// Only explicitly supplied entries are checked. Stale saved entries survive unrelated changes.
fn merge_theme_options(prefs: &mut Prefs, followed: Option<&crate::theme_files::ThemeFile>, update: &serde_json::Value) -> Result<(), String> {
    use crate::theme_files::{OptionKind, OptionValue};
    let themes = update.as_object().ok_or("`theme_options` must be a map of theme ids to option maps")?;
    // Apply legacy keys first so a canonical entry in the same request always wins.
    let aliases = themes.iter().filter(|(id, _)| crate::theme_files::legacy_gruvbox(id).is_some());
    let canonical = themes.iter().filter(|(id, _)| crate::theme_files::legacy_gruvbox(id).is_none());
    for (requested_id, update) in aliases.chain(canonical) {
        let theme_id = crate::theme_files::legacy_gruvbox(requested_id).map_or(requested_id.as_str(), |(family, _)| family);
        let key = format!("theme_options.{theme_id}");
        let file = followed.filter(|f| f.id == theme_id).or_else(|| crate::theme_files::find(theme_id, &prefs.custom_themes));
        let mode = THEMES.iter().any(|(id, _)| *id == theme_id);
        if file.is_none() && !mode {
            return Err(format!("`{key}`: unknown theme"));
        }
        let options = update.as_object().ok_or_else(|| format!("`{key}` must be an option map"))?;
        if options.is_empty() {
            prefs.theme_options.remove(theme_id);
            continue;
        }
        for (id, value) in options {
            let key = format!("{key}.{id}");
            let definition = file
                .and_then(|file| crate::theme_files::ordered_options(file).into_iter().find(|o| &o.id == id))
                .ok_or_else(|| format!("`{key}`: unknown option"))?;
            if value.is_null() {
                if let Some(saved) = prefs.theme_options.get_mut(theme_id) {
                    saved.remove(id);
                }
                continue;
            }
            let parsed: OptionValue = serde_json::from_value(value.clone()).map_err(|_| format!("`{key}`: wrong type"))?;
            let valid = match (&definition.kind, &parsed) {
                (OptionKind::Bool { .. }, OptionValue::Bool(_)) => true,
                (OptionKind::Choice { values, .. }, OptionValue::Choice(id)) => values.iter().any(|v| &v.id == id),
                (OptionKind::Range { min, max, .. }, OptionValue::Num(n)) => {
                    // Check the original JSON number too, before f32 conversion rounds at a bound.
                    n.is_finite() && value.as_f64().is_some_and(|n| n.is_finite() && n >= f64::from(*min) && n <= f64::from(*max))
                }
                _ => false,
            };
            if !valid {
                return Err(format!("`{key}`: wrong type, unknown choice or out of range"));
            }
            prefs.theme_options.entry(theme_id.to_owned()).or_default().insert(id.clone(), value.clone());
        }
        if prefs.theme_options.get(theme_id).is_some_and(BTreeMap::is_empty) {
            prefs.theme_options.remove(theme_id);
        }
    }
    Ok(())
}

fn theme_options_reply(app: &SketApp) -> sketcraft_engine::schema::ui_prefs::ThemeOptionsReply {
    use crate::theme_files::{OptionKind, OptionValue};
    use sketcraft_engine::schema::ui_prefs::{ThemeChoice, ThemeOptionDef, ThemeOptionKind, ThemeOptionValue, ThemeOptionsReply};
    let Some(file) = active_theme_file(app) else { return ThemeOptionsReply::default() };
    let text = |texts: &crate::theme_files::OptionText, fallback: &str| {
        texts
            .get(crate::i18n::effective_language(&app.preferences.language))
            .or_else(|| texts.get("en"))
            .map(String::as_str)
            .unwrap_or(fallback)
            .to_string()
    };
    let (values, notices) = effective_theme_options(&app.preferences, file);
    let blur = blur_availability(app);
    ThemeOptionsReply {
        theme_option_defs: crate::theme_files::ordered_options(file)
            .into_iter()
            .map(|o| {
                let standard = crate::theme_options::standard(&o.id);
                let provider = crate::theme_options::provider(&o.id, crate::theme_options::PROVIDERS);
                let availability = crate::theme_options::availability(file, &o.id, crate::theme_options::PROVIDERS, blur);
                ThemeOptionDef {
                    id: o.id.clone(),
                    kind: match &o.kind {
                        OptionKind::Bool { default } => ThemeOptionKind::Bool { default: *default },
                        OptionKind::Choice { default, values } => ThemeOptionKind::Choice {
                            default: default.clone(),
                            values: values.iter().map(|v| ThemeChoice { id: v.id.clone(), label: text(&v.label, &v.id) }).collect(),
                        },
                        OptionKind::Range { default, min, max, step, unit } => {
                            ThemeOptionKind::Range { default: *default, min: *min, max: *max, step: *step, unit: unit.clone() }
                        }
                    },
                    label: provider.map(|p| app.tr(p.label).to_owned()).unwrap_or_else(|| text(&o.label, &o.id)),
                    help: provider.map(|p| app.tr(p.help).to_owned()).unwrap_or_else(|| text(&o.help, "")),
                    parent: o.parent.clone(),
                    advanced: o.advanced,
                    section: o.section.clone(),
                    standard,
                    available: availability == crate::theme_options::Availability::Available,
                    reason: match availability {
                        crate::theme_options::Availability::Unavailable(reason) => Some(app.tr(reason).to_owned()),
                        crate::theme_options::Availability::Available => None,
                    },
                }
            })
            .collect(),
        theme_option_values: values
            .into_iter()
            .map(|(id, value)| {
                (
                    id,
                    match value {
                        OptionValue::Bool(v) => ThemeOptionValue::Bool(v),
                        OptionValue::Choice(v) => ThemeOptionValue::Choice(v),
                        OptionValue::Num(v) => ThemeOptionValue::Num(v),
                    },
                )
            })
            .collect(),
        theme_option_notices: notices,
    }
}

/// Export the tile the user chose, without applying it or changing the dialog snapshot.
fn theme_file(app: &SketApp, id: &str, system_dark: bool) -> serde_json::Value {
    use crate::theme_files::{Base, export, find};
    let id = if id == "system" { theme_picker::slot_id(&app.preferences, system_dark) } else { id };
    match find(id, &app.preferences.custom_themes) {
        Some(f) => export_theme_file(f),
        None if id == "dark" => export("Dark", Base::Dark, &crate::theme::Tokens::dark()),
        None => export("Light", Base::Light, &crate::theme::Tokens::light()),
    }
}

/// Fold a setting an older release kept in its own field into the one this version reads.
/// `orbit_cursor` (`true` = the point under the cursor) became `orbit_around`.
pub fn migrate(mut p: Prefs) -> Prefs {
    migrate_gruvbox(&mut p);
    // Every saved file carries the old default limit; it reads as no limit.
    if p.agent_chat_turn_limit == crate::agent_chat::gate::OLD_DEFAULT_TURN_LIMIT {
        p.agent_chat_turn_limit = 0;
    }
    let valid_resource = |(kind, id): &(String, String)| !kind.is_empty() && kind.len() <= 64 && !id.is_empty() && id.len() <= 4096;
    p.render_library_favorites = p.render_library_favorites.into_iter().filter(valid_resource).take(256).collect();
    p.render_library_recent = p.render_library_recent.into_iter().filter(valid_resource).take(32).collect();
    if !p.joint_rotation_coarse.is_finite() || !(0.001..=180.0).contains(&p.joint_rotation_coarse) {
        p.joint_rotation_coarse = 10.0;
    }
    if !p.joint_rotation_fine.is_finite() || !(0.001..=180.0).contains(&p.joint_rotation_fine) {
        p.joint_rotation_fine = 0.1;
    }
    if !accepted::wheel_zoom_speed(&p) {
        p.wheel_zoom_speed = Prefs::default().wheel_zoom_speed;
    }
    if !accepted::home_fit_size(&p) {
        p.home_fit_size = Prefs::default().home_fit_size;
    }
    if p.orbit_cursor {
        p.orbit_around = OrbitAround::Cursor;
        p.orbit_cursor = false;
    }
    p
}

/// Fold old contrast cards into their family without replacing unrelated saved options.
/// An active old card still names the selected appearance; explicit control updates run after
/// this migration, so their contrast choice (including a reset) takes precedence.
pub(crate) fn migrate_gruvbox(p: &mut Prefs) {
    let selected = crate::theme_files::legacy_gruvbox(&p.theme);
    for (old, family, _) in crate::theme_files::GRUVBOX_ALIASES {
        if let Some(options) = p.theme_options.remove(old) {
            let canonical = p.theme_options.entry(family.into()).or_default();
            for (id, value) in options {
                canonical.entry(id).or_insert(value);
            }
        }
    }
    if let Some((family, contrast)) = selected {
        p.theme = family.into();
        p.theme_options.entry(family.into()).or_default().insert("contrast".into(), json!(contrast));
    }
}

/// Drop plugin ids this page would not show, and keep at most [`crate::plugins::MAX_DISABLED`],
/// so a hand-edited preferences file cannot carry a huge or nonsense list. `set` still refuses
/// those instead of dropping them.
fn limit_disabled_plugins(mut p: Prefs) -> Prefs {
    // The file size limits, likewise, are brought into range (#4086).
    let limits = file_limits(&p);
    let mb = sketcraft_engine::io::limits::MB;
    (p.max_step_file_mb, p.max_design_file_mb) =
        if sketcraft_engine::io::limits::ADJUSTABLE { (limits.step / mb, limits.design / mb) } else { (DEFAULT_STEP_MB, DEFAULT_DESIGN_MB) };
    p.disabled_plugins.retain(|id| crate::plugins::valid_id(id));
    if p.disabled_plugins.len() > crate::plugins::MAX_DISABLED {
        p.disabled_plugins = p.disabled_plugins.into_iter().take(crate::plugins::MAX_DISABLED).collect();
    }
    p
}

/// Keep only known hidden tabs, each once, so a hand-edited preferences
/// file cannot carry a huge or nonsense list. `set` still refuses those instead of dropping them.
fn limit_toolbar_tabs(mut p: Prefs) -> Prefs {
    fn known_once(list: Vec<String>, allowed: &[&str]) -> Vec<String> {
        let mut kept: Vec<String> = Vec::new();
        for tab in list {
            if allowed.contains(&tab.as_str()) && !kept.contains(&tab) {
                kept.push(tab);
            }
        }
        kept
    }
    let allowed: Vec<&str> = crate::workspace::choosable_tabs().collect();
    p.hidden_toolbar_tabs = known_once(std::mem::take(&mut p.hidden_toolbar_tabs), &allowed);
    p
}

/// Saved preferences, keeping every one that still reads. A value this version doesn't
/// understand (a setting a newer or older release saved differently, a hand edit) falls back to
/// its default on its own instead of discarding all of them.
pub fn lenient(saved: &serde_json::Value) -> Prefs {
    if let Ok(p) = serde_json::from_value::<Prefs>(saved.clone()) {
        return limit_toolbar_tabs(limit_disabled_plugins(migrate(p)));
    }
    let Ok(mut cur) = serde_json::to_value(Prefs::default()) else { return Prefs::default() };
    if let (Some(o), Some(src)) = (cur.as_object_mut(), saved.as_object()) {
        for (k, v) in src.iter().take(1000) {
            // Keys this version doesn't have are ignored anyway.
            let Some(old) = o.get(k).cloned() else { continue };
            o.insert(k.clone(), v.clone());
            if serde_json::from_value::<Prefs>(serde_json::Value::Object(o.clone())).is_err() {
                log::warn!("preferences: `{k}` is not understood by this version; using its default");
                o.insert(k.clone(), old);
            }
        }
    }
    serde_json::from_value(cur).map(migrate).map(limit_disabled_plugins).map(limit_toolbar_tabs).unwrap_or_default()
}

/// Validate a proposed preference state without applying it or performing writes.
fn prepare_preferences(
    app: &SketApp,
    base: &Prefs,
    followed: Option<&crate::theme_files::ThemeFile>,
    p: &serde_json::Value,
) -> Result<Prefs, String> {
    for key in ["joint_rotation_coarse", "joint_rotation_fine"] {
        if let Some(value) = p.get(key)
            && value.as_f64().is_none_or(|v| !v.is_finite() || !(0.001..=180.0).contains(&v))
        {
            return Err(format!("`{key}` must be 0.001…180 degrees"));
        }
    }
    let mut cur = serde_json::to_value(base).map_err(|e| e.to_string())?;
    if let (Some(o), Some(src)) = (cur.as_object_mut(), p.as_object()) {
        for (k, v) in src {
            if !o.contains_key(k) {
                return Err(format!("unknown preference `{k}`"));
            }
            if k == "custom_themes" {
                let values = v
                    .as_array()
                    .filter(|values| values.len() <= crate::theme_files::MAX_CUSTOM)
                    .ok_or_else(|| format!("`custom_themes` must be an array of at most {} themes", crate::theme_files::MAX_CUSTOM))?;
                let themes: Vec<_> = values
                    .iter()
                    .enumerate()
                    .map(|(i, value)| theme_from_preferences(value, true).map_err(|error| format!("`custom_themes[{i}]`: {error}")))
                    .collect::<Result<_, _>>()?;
                o.insert(k.clone(), serde_json::to_value(themes).map_err(|e| e.to_string())?);
            } else if k != "theme_options" {
                o.insert(k.clone(), v.clone());
            }
        }
    }
    // Saved preferences read an unknown mode as the default; asked for here, it is an error.
    if let Some(v) = p.get("color_vision")
        && v.as_str().and_then(crate::theme::ColorVision::from_id).is_none()
    {
        return Err("`color_vision` must be off, protan, deutan or tritan".into());
    }
    if let Some(v) = p.get("viewport_theme")
        && v.as_str().and_then(crate::theme::ViewportTheme::from_id).is_none()
    {
        return Err("`viewport_theme` must be theme, light or dark".into());
    }
    if let Some(v) = p.get("validation")
        && v.as_str().and_then(sketcraft_engine::kernel::Validation::parse).is_none()
    {
        return Err("`validation` must be each_step, each_feature or off".into());
    }
    if let Some(value) = p.get("navigation_bar") {
        crate::navigation_bar::validate(value)?;
    }
    if let Some(v) = p.get("stand_up_unknown")
        && !StandUpUnknown::ALL.iter().any(|c| v.as_str() == Some(c.id()))
    {
        return Err("`stand_up_unknown` must be ask, note or off".into());
    }
    if let Some(v) = p.get("dimension_input")
        && !matches!(v.as_str(), Some("view" | "window"))
    {
        return Err("`dimension_input` must be view or window".into());
    }
    let mut new: Prefs = serde_json::from_value(cur).map_err(|e| e.to_string())?;
    migrate_gruvbox(&mut new);
    if let Some(update) = p.get("theme_options") {
        merge_theme_options(&mut new, followed, update)?;
    }
    if !accepted::wheel_zoom_speed(&new) {
        return Err("`wheel_zoom_speed` must be 0.25…4".into());
    }
    if !accepted::home_fit_size(&new) {
        return Err("`home_fit_size` must be 0.1…1".into());
    }
    let new = migrate(new);
    if !UNITS.contains(&new.default_units.as_str()) {
        return Err(format!("units must be one of {}", UNITS.join(", ")));
    }
    if !WINDOW_SYSTEMS.contains(&new.window_system.as_str()) {
        return Err(format!("`window_system` must be one of {}", WINDOW_SYSTEMS.join(", ")));
    }
    if !NAV_PRESETS.iter().any(|p| p.id == new.nav) || !MSAA.contains(&new.msaa) {
        return Err(format!("`nav` must be one of {} and `msaa` 1, 2, 4 or 8", NAV_PRESETS.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")));
    }
    if new.msaa != app.preferences.msaa && !app.msaa_supported.is_empty() && !app.msaa_supported.contains(&new.msaa) {
        return Err(format!("this graphics device doesn't support `msaa` {}; it supports {:?}", new.msaa, app.msaa_supported));
    }
    if new.graphics_adapter.len() > 1024 || new.graphics_adapter.chars().any(char::is_control) {
        return Err("`graphics_adapter` must be a name of at most 1024 bytes without control characters".into());
    }
    let file = crate::theme_files::find(&new.theme, &new.custom_themes);
    if (!THEMES.iter().any(|(id, _)| *id == new.theme) && file.is_none()) || !["z", "y"].contains(&new.up_axis.as_str()) {
        return Err("`theme` must be dark, light, system or a theme's id (see `theme_ids`) and `up_axis` z or y".into());
    }
    if new.custom_themes.len() > crate::theme_files::MAX_CUSTOM || new.custom_themes.iter().any(|t| t.colors.len() > crate::theme_files::MAX_COLORS) {
        return Err(format!("at most {} imported themes of {} colours each", crate::theme_files::MAX_CUSTOM, crate::theme_files::MAX_COLORS));
    }
    for theme in &new.custom_themes {
        theme.validate_radii()?;
        crate::theme_files::parse_icon_palette(&theme.icon_palette)?;
        crate::theme_files::parse_borders(&theme.borders)?;
    }
    if new.theme_follow != app.preferences.theme_follow {
        if !FOLLOW_AVAILABLE && !new.theme_follow.trim().is_empty() {
            return Err(FOLLOW_UNAVAILABLE.into());
        }
        crate::theme_files::follow_path(&new.theme_follow)?;
    }
    if new.language != crate::i18n::SYSTEM && !crate::i18n::is_supported(&new.language) {
        return Err(format!(
            "`language` must be `{}` or one of: {}",
            crate::i18n::SYSTEM,
            crate::i18n::LOCALES.iter().map(|locale| locale.id).collect::<Vec<_>>().join(", ")
        ));
    }
    if new.ui_font != app.preferences.ui_font && !new.ui_font.trim().is_empty() && crate::fonts::ui_font(&new.ui_font).is_none() {
        return Err(format!("`ui_font`: `{}` is not an installed font family or a readable font file", new.ui_font.trim()));
    }
    if new.disabled_plugins.len() > crate::plugins::MAX_DISABLED || new.disabled_plugins.iter().any(|id| !crate::plugins::valid_id(id)) {
        return Err(format!("`disabled_plugins` must be at most {} ids of lowercase letters, digits and hyphens", crate::plugins::MAX_DISABLED));
    }
    if new.max_step_file_mb != app.preferences.max_step_file_mb || new.max_design_file_mb != app.preferences.max_design_file_mb {
        let mb = sketcraft_engine::io::limits::MB;
        let wanted = sketcraft_engine::io::limits::FileLimits {
            step: new.max_step_file_mb.saturating_mul(mb),
            design: new.max_design_file_mb.saturating_mul(mb),
        };
        if !sketcraft_engine::io::limits::ADJUSTABLE && wanted != sketcraft_engine::io::limits::FileLimits::DEFAULT {
            return Err("`max_step_file_mb` and `max_design_file_mb` can't change in the web app: a browser tab can't hold larger files".into());
        }
        wanted.validate().map_err(|e| {
            format!(
                "`max_step_file_mb` must be {DEFAULT_STEP_MB}…{max} and `max_design_file_mb` {DEFAULT_DESIGN_MB}…{max}: {e}",
                max = sketcraft_engine::io::limits::ceiling() / mb
            )
        })?;
    }
    let once = |list: &[String], allowed: &[&str]| {
        list.len() <= allowed.len() && list.iter().all(|tab| allowed.contains(&tab.as_str()) && list.iter().filter(|t| *t == tab).count() == 1)
    };
    let allowed: Vec<&str> = crate::workspace::choosable_tabs().collect();
    if !once(&new.hidden_toolbar_tabs, &allowed) {
        return Err(format!("`hidden_toolbar_tabs` lists toolbar tabs, each once: {}", allowed.join(", ")));
    }
    if !accepted::ui_scale(&new) || !accepted::look_at_speed(&new) {
        return Err("`ui_scale` must be 0.5…2.5 and `look_at_speed` 0.25…4".into());
    }
    if !accepted::print_face_highlight(&new) {
        return Err("`print_face_highlight` must be 0…1".into());
    }
    if !accepted::home_fit_size(&new) {
        return Err("`home_fit_size` must be 0.1…1".into());
    }
    if !accepted::perspective_fov(&new) {
        return Err("`perspective_fov` must be 15°…90° in radians (0.2618…1.5708)".into());
    }
    if !accepted::dimension_step_mm(&new) || !accepted::dimension_step_deg(&new) {
        return Err("`dimension_step_mm` must be 0.001…1000 and `dimension_step_deg` 0.001…180".into());
    }
    if !accepted::solar_time_hours(&new) || !accepted::solar_day_of_year(&new) || !accepted::solar_latitude_deg(&new) {
        return Err("`solar_time_hours` must be 0…24, `solar_day_of_year` 1…365, and `solar_latitude_deg` -90…90".into());
    }
    if sketcraft_engine::doc::display_quality::DisplayQuality::parse(&new.display_quality).is_none() {
        return Err("`display_quality` must be coarse, normal, fine or ultra".into());
    }
    if !SUPERSAMPLING.contains(&new.supersampling) {
        return Err("`supersampling` must be 1 (off), 1.5 or 2".into());
    }
    if MiddleDoubleClick::parse(&new.middle_double_click).is_none() {
        return Err("`middle_double_click` must be home, fit or off".into());
    }
    Ok(new)
}

/// Set preferences from JSON (control channel); returns them all.
pub fn set(app: &mut SketApp, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    let new = prepare_preferences(app, &app.preferences, followed_theme(&app.preferences, &app.prefs_window.follow), p)?;
    apply_preferences(app, new, p)
}

/// All fallible preference checks finish before any application or file write.
fn apply_preferences(app: &mut SketApp, new: Prefs, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    let file = crate::theme_files::find(&new.theme, &new.custom_themes);
    if (new.theme != app.preferences.theme || new.custom_themes != app.preferences.custom_themes) && new.theme != "system" {
        app.ui.dark = file.map_or(new.theme == "dark", |f| f.base.is_dark());
    }
    let units_changed = new.default_units != app.preferences.default_units;
    let up_changed = new.up_axis != app.preferences.up_axis;
    let graphics_changed = new.msaa != app.preferences.msaa
        || new.window_system != app.preferences.window_system
        || new.graphics_adapter != app.preferences.graphics_adapter;
    if p.get("layout").is_some() {
        app.ui.layout = new.layout;
    }
    if new.select_other_hidden != app.preferences.select_other_hidden {
        crate::select_other::close(app);
    }
    app.preferences = new;
    sync_surface_analysis(app);
    apply_file_limits(&app.preferences);
    if units_changed {
        default_units_changed(app);
    }
    if graphics_changed {
        save_graphics(app);
    }
    if up_changed {
        up_axis_changed(app);
    }
    follow_view_fov(app);
    serde_json::to_value(&app.preferences).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) fn reveal_preference(h: &mut crate::scenario::Harness, handle: &str) {
    // Keep small scroll steps so a target cannot be skipped on a long Preferences page.
    for _ in 0..32 {
        let page = crate::scenario::rect_of("dialog:page_body").expect("preference body");
        // Scroll offsets are pixel-rounded: the final item can extend a fraction
        // of a point beyond the viewport even at the maximum scroll offset.
        // Match the tolerance used by the colour-vision sample geometry checks.
        let visible = crate::scenario::rect_of(handle)
            .map(|r| page.expand(1.0).contains_rect(r))
            .unwrap_or_else(|| crate::scenario::handle_at(handle).is_some_and(|p| page.shrink(crate::theme::Tokens::get().space_16).contains(p)));
        if visible {
            return;
        }
        h.call("ui.scroll", json!({"x":page.center().x,"y":page.center().y,"delta":-100}));
        h.frames(20);
    }
    panic!(
        "preference {handle} was not revealed: theme={} language={} section={} filter={:?} screen={:?} window={:?} body={:?} target_rect={:?} target_point={:?}",
        h.app.preferences.theme,
        h.app.preferences.language,
        h.app.prefs_window.section,
        h.app.prefs_window.filter,
        h.ctx.content_rect(),
        crate::scenario::rect_of("prefs:window"),
        crate::scenario::rect_of("dialog:page_body"),
        crate::scenario::rect_of(handle),
        crate::scenario::handle_at(handle)
    );
}

#[cfg(test)]
#[path = "prefs_tests.rs"]
mod tests;

#[cfg(test)]
mod ui_scale_regression_tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, pos2};

    #[test]
    fn desktop_zoom_shortcuts_update_and_persist_the_scale_preference() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.key", json!({"key":"Minus","cmd":true}));
        assert!((h.app.preferences.ui_scale - 0.9).abs() < 1e-4);
        assert!((h.ctx.zoom_factor() - 0.9).abs() < 1e-4);
        h.frames(12);
        assert!((h.ctx.zoom_factor() - 0.9).abs() < 1e-4, "must not flash back to the old preference");
        h.step(&json!({"restart":"close"})).unwrap();
        assert!((h.app.preferences.ui_scale - 0.9).abs() < 1e-4);
        h.call("ui.key", json!({"key":"Equals","cmd":true}));
        assert!((h.ctx.zoom_factor() - 1.0).abs() < 1e-4);
        h.call("ui.key", json!({"key":"Plus","cmd":true}));
        assert!((h.ctx.zoom_factor() - 1.1).abs() < 1e-4);
        h.call("ui.key", json!({"key":"0","cmd":true}));
        assert!((h.ctx.zoom_factor() - 1.0).abs() < 1e-4);
        h.call("ui.prefs", json!({"ui_scale":0.5}));
        h.call("ui.key", json!({"key":"Minus","cmd":true}));
        assert_eq!(h.app.preferences.ui_scale, 0.5);
        h.call("ui.prefs", json!({"ui_scale":2.5}));
        h.call("ui.key", json!({"key":"Plus","cmd":true}));
        assert_eq!(h.app.preferences.ui_scale, 2.5);
    }

    #[test]
    fn dragging_the_scale_slider_keeps_its_window_still_until_release() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"open":"Interface"}));
        let window = crate::scenario::rect_of("prefs:window").unwrap();
        super::reveal_preference(&mut h, "prefs:scale");
        let slider = crate::scenario::rect_of("prefs:scale").unwrap();
        let a = pos2(slider.left() + 20.0, slider.center().y);
        let b = pos2(slider.left() + 90.0, slider.center().y);
        h.app.synthetic.push(Event::PointerMoved(a));
        h.app.synthetic.push(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.frames(3);
        h.app.synthetic.push(Event::PointerMoved(b));
        h.frames(4);
        assert_ne!(h.app.preferences.ui_scale, 1.0, "the slider changes its value during the drag");
        assert_eq!(h.ctx.zoom_factor(), 1.0, "layout must remain stable until release");
        assert_eq!(crate::scenario::rect_of("prefs:window").unwrap(), window);
        h.app.synthetic.push(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.frames(5);
        assert!((h.ctx.zoom_factor() - h.app.preferences.ui_scale).abs() < 1e-4);
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences.ui_scale, 1.0, "Cancel restores the original scale");
        assert_eq!(h.ctx.zoom_factor(), 1.0);
    }
}

#[cfg(test)]
mod ambient_occlusion_tests {
    use super::*;

    #[test]
    fn ambient_occlusion_is_optional_persistent_and_cancelable() {
        let old: Prefs = serde_json::from_value(json!({"msaa": 4})).unwrap();
        assert!(!old.ambient_occlusion);
        let mut h = crate::scenario::Harness::new();
        let camera = h.app.cam;
        let design = h.app.session.doc.to_json();
        h.call("ui.prefs", json!({"open":"Graphics"}));
        super::reveal_preference(&mut h, "view:ambient_occlusion");
        let checkbox = crate::scenario::handle_at("view:ambient_occlusion").unwrap();
        h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
        assert!(h.app.preferences.ambient_occlusion);
        let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
        restored.load_prefs(&h.app.prefs());
        assert!(!restored.preferences.ambient_occlusion, "a change that was not applied is not saved (#9730)");
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(!h.app.preferences.ambient_occlusion);
        h.call("ui.prefs", json!({"open":"Graphics", "ambient_occlusion":true}));
        h.call("ui.prefs", json!({"button":"apply"}));
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert!(!h.app.preferences.ambient_occlusion);
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(h.app.preferences.ambient_occlusion);
        h.call("engine.execute", json!({"command":"view.ambient_occlusion","params":{"enabled":false}}));
        assert!(!h.app.preferences.ambient_occlusion);
        h.call("engine.execute", json!({"command":"view.ambient_occlusion","params":{"enabled":"yes"}}));
        assert!(!h.app.preferences.ambient_occlusion);
        h.app.start("view.ambient_occlusion");
        assert!(h.app.preferences.ambient_occlusion);
        assert_eq!(h.app.cam, camera);
        assert_eq!(h.app.session.doc.to_json(), design);
        assert!(h.app.session.undo.is_empty());
    }

    /// #7974: shading dark parts in linear light with a sky fill is off for older and new
    /// preferences, turned on from the Graphics page, kept, and sent to the shader with the world's
    /// up direction; Restore Defaults turns it off again.
    #[test]
    fn dark_parts_shading_is_optional_persistent_and_follows_the_up_axis() {
        use sketcraft_engine::render::UpAxis;
        let old: Prefs = serde_json::from_value(json!({"msaa": 4})).unwrap();
        assert!(!old.dark_parts_shading && !Prefs::default().dark_parts_shading);
        assert_eq!(crate::viewport::sky_uniform(false, UpAxis::Z), [0.0; 4], "off: shaded as before");
        assert_eq!(crate::viewport::sky_uniform(true, UpAxis::Z), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(crate::viewport::sky_uniform(true, UpAxis::Y), [0.0, 1.0, 0.0, 1.0]);
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"open":"Graphics"}));
        super::reveal_preference(&mut h, "prefs:dark_parts_shading");
        let checkbox = crate::scenario::handle_at("prefs:dark_parts_shading").unwrap();
        h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
        assert!(h.app.preferences.dark_parts_shading);
        h.call("ui.prefs", json!({"button":"apply"}));
        let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
        restored.load_prefs(&h.app.prefs());
        assert!(restored.preferences.dark_parts_shading);
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert!(!h.app.preferences.dark_parts_shading);
    }
}

/// Manual edits are preserved and shown as Custom rather than claiming to match a preset.
pub(crate) fn graphics_preset(prefs: &Prefs) -> &'static str {
    match (prefs.ambient_occlusion, prefs.display_quality.as_str(), prefs.supersampling, prefs.supersampling_adaptive) {
        (false, "normal", 1.0, _) => "performance",
        (true, "fine", 2.0, true) => "quality",
        _ => "custom",
    }
}

#[cfg(test)]
#[path = "prefs_graphics_preset_tests.rs"]
mod graphics_preset_tests;

#[cfg(test)]
#[path = "prefs_perspective_fov_tests.rs"]
mod perspective_fov_tests;
