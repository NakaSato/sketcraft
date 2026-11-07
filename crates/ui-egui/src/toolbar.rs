//! Application bar, the workspace toolbar (tabs and panels) and keyboard shortcuts.

use crate::fonts::RichTextExt as _;
use crate::icons::glyphs::pin_icon;
use crate::ui_kit::BackingTooltipExt as _;
use crate::ui_kit::{self, Container, Edge, Surface};
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::{SketApp, icons};

pub use crate::workspace::TABS;

/// Legacy paint arguments and non-SVG affordances follow the same semantic palette as the icon.
/// The requested icon id determines its meaning on ribbons, menus and drag previews alike.
fn command_accent(icon: &str) -> Color32 {
    crate::icon_svg::primary(icon, crate::theme::is_dark(), crate::theme::vision())
}

/// Accent for a tab or a panel with a more specific semantic role.
fn panel_accent(t: &Tokens, tab: &str, panel: &str) -> Color32 {
    if panel == "CONSTRAIN" {
        return t.glyph_text;
    }
    crate::theme::TabAccent::for_tab(t, tab).color
}

/// Panels shown on each tab (in order). Commands come from the registry by (tab, panel).
pub fn panels(tab: &str) -> Vec<&'static str> {
    crate::workspace::panels(tab)
}

/// How a panel is drawn on a toolbar that may be too narrow for every button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Its first buttons (all of them, or the ones that fit); the panel's drop-down lists every
    /// command.
    Buttons,
    /// One button that opens the panel's drop-down (which lists every command, pinned or not).
    Collapsed,
    /// In the » menu at the end of the toolbar.
    More,
}

/// A ribbon button's preferred and minimum widths. The constraint gallery is a fixed-width
/// block; ordinary buttons narrow as their labels wrap and truncate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonSize {
    pub width: f32,
    pub min_width: f32,
}

/// One panel as [`fit`] sees it, in button order (the gallery, if present, is last).
#[derive(Clone, Debug, PartialEq)]
pub struct PanelSize {
    pub buttons: Vec<ButtonSize>,
}

/// Where [`fit`] put a panel: its first buttons and their individual fitted widths.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub fit: Fit,
    pub shown: usize,
    pub widths: Vec<f32>,
}

impl Plan {
    fn full(panel: &PanelSize) -> Self {
        Self { fit: Fit::Buttons, shown: panel.buttons.len(), widths: panel.buttons.iter().map(|b| b.width).collect() }
    }

    fn more() -> Self {
        Self { fit: Fit::More, shown: 0, widths: Vec::new() }
    }

    /// The panel's width on the toolbar (0 in the » menu).
    pub fn width(&self) -> f32 {
        match self.fit {
            Fit::More => 0.0,
            Fit::Collapsed => COLLAPSED_WIDTH,
            Fit::Buttons => panel_width(self.widths.get(..self.shown).unwrap_or_default()),
        }
    }
}

/// A panel's width with these individual button widths.
pub fn panel_width(buttons: &[f32]) -> f32 {
    (buttons.iter().sum::<f32>() + buttons.len().saturating_sub(1) as f32 * BUTTON_GAP + 8.0).max(COLLAPSED_WIDTH)
}

/// A collapsed panel's width: one button's panel.
pub const COLLAPSED_WIDTH: f32 = 72.0;

/// The gap after each panel (its separator).
const PANEL_GAP: f32 = 1.0;
pub const TAB_ROW_HEIGHT: f32 = 30.0;
/// Distance from the ribbon card's edges to the first tab and the search field.
const RIBBON_TAB_INSET: f32 = 4.0;
pub const GROUP_HEIGHT: f32 = 86.0;
pub const RIBBON_HEIGHT: f32 = TAB_ROW_HEIGHT + GROUP_HEIGHT + 1.0;
pub(crate) const WORKSPACE_WIDTH: f32 = 88.0;
const BUTTON_HEIGHT: f32 = 60.0;
const ICON_BUTTON_HEIGHT: f32 = 36.0;
const ICON_GROUP_HEIGHT: f32 = 62.0;

/// The same height drives shell reservations and ribbon painting in either layout.
pub(crate) fn ribbon_height(labels: bool) -> f32 {
    if labels { RIBBON_HEIGHT } else { TAB_ROW_HEIGHT + ICON_GROUP_HEIGHT + 1.0 }
}
const BUTTON_GAP: f32 = 3.0;
const BUTTON_MIN_WIDTH: f32 = 54.0;
const BUTTON_MAX_WIDTH: f32 = 76.0;
const BUTTON_LABEL_PADDING: f32 = 10.0;
/// Where a button's label starts below the button's top edge, under its icon.
const BUTTON_LABEL_TOP: f32 = 32.0;
/// Room kept below a button's label, inside its highlight.
const BUTTON_LABEL_BOTTOM: f32 = 2.0;
const SECTION_HEIGHT: f32 = 20.0;

/// The » button's room at the end of the panels, kept only while some panel is in it.
pub const MORE_ROOM: f32 = 40.0;

/// Fit panels into `room`, giving up as little as it takes, one small step at a time:
/// 1. buttons narrow from their measured label widths to their minimum, wrapping and truncating;
/// 2. panels hand their last buttons to their drop-down one at a time, the panel showing the
///    most buttons first (the rightmost of equals), down to one button (collapsed);
/// 3. the last panels go into the » menu, and any room that frees goes back to buttons.
///
/// Each removal frees at most one button (or the fixed-width constraint gallery), so the toolbar
/// never leaves a larger gap while something is hidden. The result depends only on `room`
/// (no state from earlier frames), so it never flickers. Nothing is dropped.
pub fn fit(panels: &[PanelSize], room: f32) -> Vec<Plan> {
    let room = if room.is_finite() { room } else { 0.0 };
    if let Some(plans) = shrink(panels, room) {
        return plans;
    }
    // 3. The last panels into the » menu (which takes its own room), as few as it takes; the
    // ones left keep as many buttons as fit.
    for kept in (0..panels.len()).rev() {
        if let Some(mut plans) = shrink(panels.get(..kept).unwrap_or_default(), room - MORE_ROOM) {
            plans.extend(panels.iter().skip(kept).map(|_| Plan::more()));
            return plans;
        }
    }
    panels.iter().map(|_| Plan::more()).collect()
}

/// The room plans take on the toolbar (each panel and its separator).
fn used(plans: &[Plan]) -> f32 {
    plans.iter().filter(|p| p.fit != Fit::More).map(|p| p.width() + PANEL_GAP).sum()
}

/// Steps 1 and 2 of [`fit`]: every panel on the toolbar, or None when even one button each
/// doesn't fit.
fn shrink(panels: &[PanelSize], room: f32) -> Option<Vec<Plan>> {
    let mut plans: Vec<Plan> = panels.iter().map(Plan::full).collect();
    if used(&plans) <= room {
        return Some(plans);
    }
    // 1. Narrow together in one-pixel steps, keeping each button's measured width until it
    // reaches its minimum. The gallery's minimum equals its full width.
    for reduction in 1..=((BUTTON_MAX_WIDTH - BUTTON_MIN_WIDTH) as usize) {
        for (plan, panel) in plans.iter_mut().zip(panels) {
            for (width, button) in plan.widths.iter_mut().zip(&panel.buttons) {
                *width = (button.width - reduction as f32).max(button.min_width);
            }
        }
        if used(&plans) <= room {
            return Some(plans);
        }
    }
    // 2. One button at a time into the drop-downs: from the panel showing the most (the
    // rightmost of equals), down to one button each.
    while used(&plans) > room {
        let pick = plans
            .iter()
            .enumerate()
            .filter(|(_, p)| p.fit == Fit::Buttons && (p.shown > 1 || p.width() > COLLAPSED_WIDTH))
            .max_by(|(i, a), (j, b)| a.shown.cmp(&b.shown).then(i.cmp(j)));
        let (i, _) = pick?;
        if let Some(p) = plans.get_mut(i) {
            p.shown = p.shown.saturating_sub(1).max(1);
            if p.shown == 1 {
                p.fit = Fit::Collapsed;
            }
        }
    }
    Some(plans)
}

/// The commands a panel's drop-down (or its » submenu) lists: its own, then any button moved
/// into it from another panel.
fn menu_commands(
    cmds: &[&'static sketcraft_engine::CommandSpec],
    shown: &[&'static sketcraft_engine::CommandSpec],
) -> Vec<&'static sketcraft_engine::CommandSpec> {
    let mut out = cmds.to_vec();
    for c in shown {
        if !out.iter().any(|o| o.id == c.id) {
            out.push(*c);
        }
    }
    out
}

/// Room for the version between Report a bug and the caption buttons, at the least.
const RIGHT_GROUP: f32 = 60.0;
const CLUSTER_BUTTONS: f32 = 108.0;

/// The least room the document tabs keep when a long version takes its extra width: one tab and
/// the + button ([`crate::documents::layout`]).
const TABS_KEPT: f32 = 124.0;

/// The room right of Report a bug for the version (`version_width`, measured), or None where the
/// bar shows none. The version sits 12 points from the caption buttons with a gap before the
/// button, so a longer one (`v0.21.0-rc.1`) moves the button left instead of covering it. Its
/// width over [`RIGHT_GROUP`] comes only from the document tabs' spare room: where it would leave
/// them less than [`TABS_KEPT`], the version is hidden, as it is on a narrow bar.
fn version_room(bar: Rect, captions: f32, tabs_start: f32, version_width: f32) -> Option<f32> {
    if bar.width() < 640.0 {
        return None;
    }
    let need = 12.0 + version_width + 8.0;
    let room = RIGHT_GROUP.max(need);
    // The tabs end 8 points before the cluster, with Report a bug as an icon at the least.
    let tabs = bar.right() - captions - room - 28.0 - CLUSTER_BUTTONS - 8.0 - tabs_start;
    (need <= RIGHT_GROUP || tabs >= TABS_KEPT).then_some(room)
}

/// The centre of Help, between Settings and Report a bug.
fn help_x(report: Rect) -> f32 {
    report.left() - 18.0
}

/// The command whose tool or dialog is open right now, shown pressed in the toolbar.
fn running_command(app: &SketApp) -> Option<String> {
    match (&app.tool, &app.dialog) {
        (Some(tool), _) => Some(tool.cmd.to_string()),
        // Create Sketch never lands in last_command (it is not repeatable).
        (None, Some(d)) if matches!(d.kind, crate::dialogs::Kind::Sketch) => Some("sketch.create".to_string()),
        (None, Some(_)) => app.last_command.as_ref().map(|(id, _)| id.clone()),
        _ => None,
    }
}

/// Whether the toolbar shows command `id` pressed: its tool or dialog is open, or it turned on
/// the view analysis on now (one at a time, `Session::active_analysis`), its tool running or not.
pub(crate) fn shows_pressed(app: &SketApp, id: &str) -> bool {
    running_command(app).as_deref() == Some(id) || app.session.active_analysis() == Some(id)
}

/// [`shows_pressed`] for a ribbon button: a command with a persistent effect is pressed only while
/// its tool runs, since `persistent_ribbon_pressed` alone decides how its effect is shown.
fn ribbon_shows_pressed(app: &SketApp, id: &str) -> bool {
    if persistent_available(app, id) { running_command(app).as_deref() == Some(id) } else { shows_pressed(app, id) }
}

/// The Report a bug button: labelled when the layout asks for it and there is room for its text
/// (`label_width`, measured in the current language), otherwise just the icon. An icon-only
/// layout gives it a square the size of Settings and Help. `version_width` is the measured version.
fn report_rect(bar: Rect, captions: f32, tabs_start: f32, label_width: f32, version_width: f32, layout: HeaderLayout) -> (Rect, bool) {
    let room = if layout.version == VersionPlace::Header { version_room(bar, captions, tabs_start, version_width) } else { None };
    let right = bar.right() - captions - room.unwrap_or(8.0);
    if layout.report == ReportButton::Icon {
        return (Rect::from_min_size(pos2(right - 32.0, bar.center().y - 16.0), vec2(32.0, 32.0)), false);
    }
    // Icon, text and a margin that mirrors the icon side.
    let full = REPORT_TEXT_X + label_width + 11.0;
    let labeled = right - CLUSTER_BUTTONS - tabs_start >= 216.0 + full;
    let width = if labeled { full } else { 28.0 };
    (Rect::from_min_size(pos2(right - width, bar.center().y - 13.0), vec2(width, 26.0)), labeled)
}

/// The closed search field in the application bar, ending `right`: whole when the document tabs
/// keep the room Report a bug's label leaves them (`tabs_room`: theirs without the field), else
/// just its magnifier.
fn header_search_rect(ctx: &egui::Context, bar: Rect, right: f32, tabs_room: f32) -> Rect {
    let height = 26.0;
    let full = crate::ui_kit::CommandSearch::closed_width(ctx);
    let whole = bar.width() >= 640.0 && tabs_room - full - HEADER_SEARCH_GAP >= 216.0;
    let width = if whole { full } else { crate::ui_kit::CommandSearch::compact_width(height) };
    Rect::from_min_size(pos2(right - width, bar.center().y - height / 2.0), vec2(width, height))
}

/// The height of an application-bar pill ([`offer_pill`]), a little under the bar's 26 px buttons.
pub(crate) const PILL_HEIGHT: f32 = 22.0;

/// An application-bar pill for something on offer (Update) at the right of `span` (the bar's free
/// space): accent-filled, with `icon` and the first of `labels` that fits (the icon alone when
/// none does or `labeled` is false). Every label but the last also leaves `keep` points of the
/// span free (for the document tabs). None when even the icon doesn't fit: it never covers the
/// bar's other buttons. `handle` names it for scenarios.
pub(crate) fn offer_pill(
    ui: &mut egui::Ui,
    span: Rect,
    labeled: bool,
    labels: &[&str],
    keep: f32,
    icon: &str,
    handle: &str,
) -> Option<(Rect, egui::Response)> {
    let t = Tokens::get();
    let font = FontId::proportional(t.font_label);
    let (icon_x, text_x, right_pad) = (16.0, 29.0, 11.0);
    let last = labels.len().saturating_sub(1);
    let label = labels.iter().enumerate().filter(|_| labeled).find_map(|(i, text)| {
        let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font.clone(), t.app_bar_text));
        let width = text_x + galley.size().x + right_pad;
        let room = if i < last { span.width() - keep } else { span.width() };
        (width <= room).then_some((*text, width))
    });
    let width = label.map_or(PILL_HEIGHT + 4.0, |(_, w)| w);
    if width > span.width() {
        return None;
    }
    let rect = Rect::from_min_size(pos2(span.right() - width, span.center().y - PILL_HEIGHT / 2.0), vec2(width, PILL_HEIGHT));
    let response = ui.interact(rect, ui.scope_id().with(("sc_offer_pill", handle)), Sense::click());
    let name = label.map(|(text, _)| text).or(labels.first().copied()).unwrap_or_default().to_string();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name));
    crate::scenario::publish_target(handle, rect);
    let hot = response.hovered() || response.has_focus();
    let fill = if hot { t.app_bar_hover_button } else { t.accent };
    let radius = PILL_HEIGHT / 2.0 * f32::from(t.radius_xl) / 10.0;
    ui.painter().rect_filled(rect, radius, fill);
    if response.has_focus() {
        ui.painter().rect_stroke(rect.expand(2.0), Tokens::outset_radius(radius, 2.0), Stroke::new(1.0, t.app_bar_text), egui::StrokeKind::Outside);
    }
    let glyph_x = if label.is_some() { rect.left() + icon_x } else { rect.center().x };
    let glyph = Rect::from_center_size(pos2(glyph_x, rect.center().y), vec2(16.0, 16.0));
    icons::paint_on(ui.painter(), glyph, icon, Some(true), 1.0);
    if let Some((text, _)) = label {
        ui.painter().text(pos2(rect.left() + text_x, rect.center().y), Align2::LEFT_CENTER, text, font, t.app_bar_text);
    }
    Some((rect, response))
}

/// Where the Report a bug text starts, from the button's left edge.
const REPORT_TEXT_X: f32 = 29.0;

/// The application bar's height: the window's title bar on desktop builds.
pub const APP_BAR_HEIGHT: f32 = 34.0;

/// Where the application bar puts command search, Report a bug and the version: chosen by the
/// app that hosts the UI ([`SketApp::header`]) before the first frame. The default is the
/// desktop layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeaderLayout {
    pub search: SearchPlace,
    pub report: ReportButton,
    pub version: VersionPlace,
}

impl HeaderLayout {
    /// Search in the ribbon's tab row, a labelled Report a bug, the version in the bar.
    pub const DESKTOP: HeaderLayout = HeaderLayout { search: SearchPlace::Ribbon, report: ReportButton::Labelled, version: VersionPlace::Header };
    /// Search in the bar beside Settings, Report a bug as an icon, the version in the Help menu.
    pub const COMPACT: HeaderLayout = HeaderLayout { search: SearchPlace::Header, report: ReportButton::Icon, version: VersionPlace::HelpMenu };
}

/// Where the closed command search field sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchPlace {
    /// At the right end of the ribbon's tab row.
    #[default]
    Ribbon,
    /// In the application bar, immediately left of Settings; just its magnifier when the bar is
    /// short of room. The ribbon keeps it where the bar has no Settings button of its own (a
    /// system File menu) or is not shown (an embedded view).
    Header,
}

/// How the application bar shows Report a bug.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReportButton {
    /// A button with its label when there is room for it (the icon alone otherwise).
    #[default]
    Labelled,
    /// Always the icon alone, drawn like Settings and Help; its name stays the tooltip and the
    /// accessible label.
    Icon,
}

/// Where the version is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VersionPlace {
    /// At the application bar's right end, linking to the release notes.
    #[default]
    Header,
    /// As a row of the Help menu, opening the release notes.
    HelpMenu,
}

/// The closed search field goes in the application bar rather than the ribbon.
pub(crate) fn search_in_header(app: &SketApp) -> bool {
    app.header.search == SearchPlace::Header && !app.embedded && !app.embed_editor
}

/// Space between the header search field and Settings.
const HEADER_SEARCH_GAP: f32 = 4.0;

pub fn app_bar(app: &mut SketApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let app_bar = Container::new(ui.ctx(), Surface::ApplicationBar).edge("sc_appbar", Edge::Top);
    let frame = Container::new(ui.ctx(), Surface::ApplicationBar).edge_frame(t.app_bar);
    Container::new(ui.ctx(), Surface::ApplicationBar).show_edge(ui, app_bar.exact_size(APP_BAR_HEIGHT).show_separator_line(false), frame, |ui| {
        let r = ui.max_rect();
        // The bar is the window's title bar: its empty space moves the window.
        if app.custom_titlebar || app.integrated_titlebar {
            crate::titlebar::drag_area(ui, r);
        }
        let left = r.left() + if app.integrated_titlebar { crate::titlebar::TRAFFIC_LIGHTS } else { 0.0 };
        // Keep the current header controls and product name when merging the new mark.
        let compact_actions = r.right() - left < 640.0;
        // Brand mark: the SketCraft logo, drawn in code from the SVG's own coordinates
        // (viewBox 357 x 412). Same 20 px height and centre as the old block.
        {
            let c = pos2(left + 20.0, r.center().y);
            crate::icons::glyphs::brand_mark(ui.painter(), c);
        }
        if !compact_actions {
            ui.painter().text(
                pos2(left + 36.0, r.center().y),
                Align2::LEFT_CENTER,
                "SketCraft",
                crate::fonts::semibold(t.font_heading),
                t.app_bar_text,
            );
        }
        let mut x = left + if compact_actions { 56.0 } else { 128.0 };
        let home = Rect::from_center_size(pos2(x - 4.0, r.center().y), vec2(32.0, 32.0));
        let idle = !app.file_load.busy();
        let home_response = ui.interact(home, ui.scope_id().with("appbar_home"), if idle { Sense::click() } else { Sense::hover() });
        let home_tip = app.tr("ui.start_page");
        home_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, idle, home_tip));
        crate::scenario::publish_handle("appbar:home", home.center());
        crate::scenario::publish_handle("titlebar:home", home.center());
        // Compatibility for scenarios that used the former Home tab.
        crate::scenario::publish_target_as("tab:home", "titlebar:home", home);
        crate::scenario::publish_rect("appbar:home", home);
        if idle && (home_response.hovered() || home_response.has_focus()) {
            ui.painter().rect_filled(home.shrink(3.0), t.radius_sm, t.app_bar_hover_button);
        }
        icons::paint_app_bar(ui.painter(), home.shrink(7.0), "home", if idle { 1.0 } else { 0.35 });
        if home_response.backing_on_hover_text(home_tip).clicked() && idle {
            app.home.show(app.session.revision);
        }
        x = home.right() + 8.0;
        app_bar_separator(ui, r, x, "brand");
        x += 16.0;
        // While a file loads the design waits: no menus or commands from the bar.
        // The File menu: a text button (New, Open, Save, Save As, Export…). Its handle keeps the
        // old `appbar:folder` name too, for scripts written against the folder button.
        let file = {
            let label = app.tr("ui.file").to_string();
            let galley = ui.painter().layout_no_wrap(label.clone(), FontId::proportional(t.font_button), t.app_bar_text);
            let br = Rect::from_min_size(pos2(x - 13.0, r.center().y - 13.0), vec2(galley.size().x + 16.0, 26.0));
            x = br.right() + 17.0;
            let resp = ui.interact(br, ui.scope_id().with(("ab", "file")), if idle { Sense::click() } else { Sense::hover() });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, idle, &label));
            crate::scenario::publish_handle("appbar:file", br.center());
            crate::scenario::publish_target_as("appbar:folder", "appbar:file", br);
            let open = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::unique("sc_file_menu")).unwrap_or(false));
            if idle && (resp.hovered() || resp.has_focus() || open) {
                ui.painter().rect_filled(br, t.radius_sm, t.app_bar_hover);
            }
            let ink = if idle { t.app_bar_text } else { t.app_bar_text.gamma_multiply(0.35) };
            ui.painter().galley(pos2(br.left() + 8.0, br.center().y - galley.size().y / 2.0), galley, ink);
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("sc_file_menu_x"), br.left()));
            idle && resp.clicked()
        };
        // A disabled button is dimmed and ignores clicks (its tooltip still shows).
        let click = |ui: &mut egui::Ui, at: f32, icon: &str, tip: &str, enabled: bool| -> bool {
            let width = if compact_actions && matches!(icon, "save" | "undo" | "redo") { 22.0 } else { 32.0 };
            let br = Rect::from_center_size(pos2(at, r.center().y), vec2(width, 32.0));
            let resp = ui.interact(br, ui.scope_id().with(("ab", icon)), if enabled { Sense::click() } else { Sense::hover() });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
            crate::scenario::publish_target(&format!("appbar:{icon}"), br);
            crate::scenario::publish_target(&format!("titlebar:{icon}"), br);
            if enabled && (resp.hovered() || resp.has_focus()) {
                ui.painter().rect_filled(br.shrink(3.0), t.radius_sm, t.app_bar_hover_button);
            }
            icons::paint_app_bar(ui.painter(), Rect::from_center_size(br.center(), vec2(18.0, 18.0)), icon, if enabled { 1.0 } else { 0.35 });
            let clicked = resp.backing_on_hover_text(tip).clicked();
            enabled && clicked
        };
        let step = if compact_actions { 22.0 } else { 34.0 };
        // Save, Undo and Redo use the same actions as the File menu and shortcuts.
        let shown = design_shown(app) && idle;
        let tip = |id: &str, key: &str| {
            let label = app.tr(id);
            if shown { format!("{label} ({key})") } else { crate::i18n::format("ui.needs_design", &[("command", label)]) }
        };
        let (save_tip, undo_tip, redo_tip) = (tip("ui.save", "Ctrl+S"), tip("ui.undo", "Ctrl+Z"), tip("ui.redo", "Ctrl+Y"));
        if click(ui, x, "save", &save_tip, shown) {
            save(app);
        }
        x += step;
        // Narrow windows retain Save; Undo and Redo remain available from their shortcuts and menus.
        if !compact_actions {
            if click(ui, x, "undo", &undo_tip, shown) {
                FileAction::Undo.run(app);
            }
            x += step;
            if click(ui, x, "redo", &redo_tip, shown) {
                FileAction::Redo.run(app);
            }
            x += step;
            x = quick_access_bar(app, ui, r, x, step, shown);
        }
        if file {
            app.ui.palette_open = false;
            let ctx = ui.ctx().clone();
            let open = ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("sc_file_menu")).unwrap_or(false));
            let pass = ctx.cumulative_pass_nr();
            // The File button toggles the menu. Remember the pass it opened on so the same
            // click isn't also treated as a click outside the menu (which would close it at once).
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::unique("sc_file_menu"), !open);
                d.insert_temp(egui::Id::unique("sc_file_menu_opened"), pass);
            });
        }
        // Document tabs follow the file actions; the house opens the start page.
        let captions = if app.custom_titlebar { crate::titlebar::WIDTH } else { 0.0 };
        let separator_x = x - step + if compact_actions { 19.0 } else { 24.0 };
        app_bar_separator(ui, r, separator_x, "documents");
        let tabs_start = separator_x + 10.0;
        let report_text = crate::i18n::tr("ui.report_a_bug");
        let report_font = FontId::proportional(t.font_button);
        let label_width = ui.painter().layout_no_wrap(report_text.to_string(), report_font.clone(), t.app_bar_text).size().x;
        // The version links to its release notes; measured here so Report a bug leaves it room.
        let version =
            ui.painter().layout_no_wrap(format!("v{}", crate::update::current_version()), FontId::proportional(t.font_small), t.app_bar_text);
        let (report, report_label) = report_rect(r, captions, tabs_start, label_width, version.size().x, app.header);
        // Settings' left edge; command search, when the bar holds it, sits just before it and
        // moves the assistant button and the end of the document tabs left by its width.
        let gear_left = report.left() - 68.0;
        let search = (search_in_header(app) && design_shown(app))
            .then(|| header_search_rect(ui.ctx(), r, gear_left - HEADER_SEARCH_GAP, report.left() - CLUSTER_BUTTONS - tabs_start));
        let search_shift = search.map_or(0.0, |s| s.width() + HEADER_SEARCH_GAP);
        if let Some(search) = search {
            // shortcut_box::show draws the one search editor wherever this rectangle is.
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("sc_ribbon_search"), search));
            crate::scenario::publish_target("titlebar:search", search);
        }
        // Assistant, Settings, Help and Report a bug form the right-hand cluster.
        if click(ui, help_x(report), "help", app.tr("ui.help"), true) {
            app.help.menu = !app.help.menu;
            let at = help_x(report);
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::unique("sc_help_opened"), true);
                d.insert_temp(egui::Id::unique("sc_help_anchor"), at);
            });
        }
        // A newer release: its Update button stays before the right cluster, in the space up to
        // the bar's own file actions on the left.
        let controls_left = report.left() - CLUSTER_BUTTONS - search_shift;
        let span = Rect::from_min_max(pos2(x, report.top()), pos2(controls_left - 2.0, report.bottom()));
        // Labelled where Report a bug is (an icon-only Report a bug leaves the pill to fit by room).
        let update_labeled = report_label || (app.header.report == ReportButton::Icon && r.width() >= 640.0);
        let update = crate::update::button(app, ui, span, update_labeled);
        let tabs_end = update.map_or(controls_left, |u| u.left()) - 8.0;
        // Narrow title bars can fit only part of the tab strip. Its hit regions must not
        // extend into the persistent report and window controls.
        let tabs_rect = Rect::from_min_max(pos2(tabs_start.min(tabs_end), r.top()), pos2(tabs_end, r.bottom()));
        let mut tabs_ui = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect));
        tabs_ui.set_clip_rect(tabs_rect.intersect(ui.clip_rect()));
        crate::documents::tabs(app, &mut tabs_ui, r, tabs_start, tabs_end);
        let response = ui.interact(report, ui.scope_id().with("sc_report_button"), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), report_text));
        crate::scenario::publish_target("titlebar:report", report);
        let hot = response.hovered() || response.has_focus();
        let tip = if app.header.report == ReportButton::Icon {
            // Drawn like Settings and Help beside it.
            if hot {
                ui.painter().rect_filled(report.shrink(3.0), t.radius_sm, t.app_bar_hover_button);
            }
            icons::paint_app_bar(ui.painter(), report.shrink(7.0), "bug_report", 1.0);
            report_text
        } else {
            ui.painter().rect_filled(report, t.radius_sm, if hot { t.app_bar_hover_button } else { t.app_bar });
            ui.painter().rect_stroke(report, t.radius_sm, Stroke::new(1.0, t.app_bar_separator), egui::StrokeKind::Inside);
            let icon = Rect::from_center_size(pos2(report.left() + 14.0, report.center().y), vec2(18.0, 18.0));
            icons::paint_app_bar(ui.painter(), icon, "bug_report", 1.0);
            if report_label {
                ui.painter().text(
                    pos2(report.left() + REPORT_TEXT_X, report.center().y),
                    Align2::LEFT_CENTER,
                    report_text,
                    report_font,
                    t.app_bar_text,
                );
            }
            crate::i18n::tr_label("Report a bug — review a screenshot and open a GitHub draft")
        };
        if response.backing_on_hover_text(tip).clicked() {
            crate::help::run(app, &ui.ctx().clone(), "report");
        }
        // Settings sits immediately left of Help (also Ctrl+,).
        let gear = Rect::from_center_size(pos2(report.left() - 52.0, r.center().y), vec2(32.0, 32.0));
        let gresp = ui.interact(gear, ui.scope_id().with("sc_prefs_button"), Sense::click());
        if gresp.hovered() || gresp.has_focus() {
            ui.painter().rect_filled(gear.shrink(3.0), t.radius_sm, t.app_bar_hover_button);
        }
        icons::paint_app_bar(ui.painter(), gear.shrink(7.0), "settings", 1.0);
        crate::scenario::publish_rect("titlebar:preferences", gear);
        if crate::agent_chat::app_bar_button_shown(app) {
            assistant_button(app, ui, Rect::from_center_size(pos2(report.left() - 86.0 - search_shift, r.center().y), vec2(32.0, 32.0)));
        }
        crate::scenario::publish_handle("titlebar:preferences", gear.center());
        let prefs_tip = if cfg!(target_os = "macos") {
            format!("{} (Cmd+,)", app.tr("ui.preferences"))
        } else {
            format!("{} (Ctrl+,)", app.tr("ui.preferences"))
        };
        if gresp.backing_on_hover_text(prefs_tip).clicked() {
            crate::prefs::open_at(app, 0);
        }
        if app.header.version == VersionPlace::Header && version_room(r, captions, tabs_start, version.size().x).is_some() {
            let galley = version;
            let vrect = Align2::RIGHT_CENTER.anchor_size(pos2(r.right() - 12.0 - captions, r.center().y), galley.size());
            let vresp = ui.interact(
                vrect.expand2(vec2(2.0, 4.0)).intersect(Rect::everything_right_of(report.right())),
                ui.scope_id().with("sc_version_link"),
                Sense::click(),
            );
            let url = crate::update::release_notes_url(crate::update::current_version(), crate::update::release_build());
            let vresp = vresp.on_hover_cursor(egui::CursorIcon::PointingHand).backing_on_hover_text(&url);
            if vresp.hovered() || vresp.has_focus() {
                ui.painter().hline(vrect.x_range(), vrect.bottom(), Stroke::new(1.0, t.app_bar_text));
            }
            ui.painter().galley(vrect.min, galley, t.app_bar_text);
            crate::scenario::publish_handle("titlebar:version", vrect.center());
            if vresp.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
        if app.custom_titlebar {
            crate::titlebar::caption_buttons(app, ui, r);
        }
    });
    file_menu(app, ui.ctx());
    let help_x = ui.ctx().data(|d| d.get_temp::<f32>(egui::Id::unique("sc_help_anchor")).unwrap_or(300.0));
    crate::help::show(app, &ui.ctx().clone(), pos2(help_x - 14.0, APP_BAR_HEIGHT));
}

/// File, edit and application actions shared by the in-window and native menus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileAction {
    /// Create a design using the document lifecycle.
    New,
    /// Pick a design or an exchange file to open.
    Open,
    /// Save the active design, asking for a path when needed.
    Save,
    /// Save the active design under another name.
    SaveAs,
    /// Undo the active design's last command.
    Undo,
    /// Redo the active design's last undone command.
    Redo,
    /// Save a JSON design using the existing save-as flow.
    SaveJson,
    /// Export through the format-specific export command.
    Export(&'static str),
    /// Open the mesh export dialog with the selected format.
    ExportMesh(&'static str),
    /// Start an existing interactive command.
    Command(&'static str),
    /// Open an entry from the recent-design list.
    OpenRecent(String),
    /// Clear recent designs and their last-opened dates.
    ClearRecent,
    /// Show the application's preferences.
    Settings,
    /// Show the keyboard shortcut editor.
    Shortcuts,
    /// Toggle the agent chat panel.
    AgentChat,
    /// Request application exit, checking for unsaved documents.
    Quit,
}

impl FileAction {
    /// Whether this action is available in the current document, picker and report state.
    pub fn enabled(&self, app: &SketApp) -> bool {
        let needs_design = matches!(self, Self::Save | Self::SaveAs | Self::Undo | Self::Redo | Self::ExportMesh(_) | Self::Command(_))
            || matches!(self, Self::Export(extension) if *extension != "json");
        // An open bug report describes the designs as they are, and a window opened behind it
        // covers it (#7188). Quit too, as behind any modal window: it would switch to a design
        // with unsaved changes under the report, and ask about it where it can't be answered.
        let behind_report = app.issue_report.draft.is_some();
        let available = match self {
            Self::Undo => !app.session.undo.is_empty(),
            Self::Redo => !app.session.redo.is_empty(),
            Self::ClearRecent => !app.home.recent.is_empty(),
            Self::AgentChat => crate::agent_chat::app_bar_button_shown(app),
            _ => true,
        };
        !app.file_picker_busy() && !app.file_load.busy() && !behind_report && (!needs_design || design_shown(app)) && available
    }

    /// Whether the native menu offers this action: while a widget has keyboard focus (a report's
    /// text field too), its Undo and Redo go to that widget instead of the design.
    pub fn menu_enabled(&self, app: &SketApp, keyboard_focus: bool) -> bool {
        self.enabled(app) || (matches!(self, Self::Undo | Self::Redo) && keyboard_focus && !app.file_picker_busy() && !app.file_load.busy())
    }

    /// Execute through the same document, save-job and preferences paths as the toolbar.
    pub fn run(&self, app: &mut SketApp) {
        if !self.enabled(app) {
            return;
        }
        match self {
            Self::New => crate::documents::new_design(app),
            Self::Open => app.pick_open(|app, path| {
                if let Some(path) = path {
                    app.open_path_async(&path);
                }
            }),
            Self::Save => save(app),
            Self::SaveAs => save_as(app),
            Self::Undo => drop(app.run("edit.undo", json!({}))),
            Self::Redo => drop(app.run("edit.redo", json!({}))),
            Self::SaveJson => save_as_with_extension(app, "json"),
            Self::Export(extension) => export(app, extension),
            Self::ExportMesh(format) => crate::dialogs_save_mesh::start_with_format(app, format),
            Self::Command(command) => app.start(command),
            Self::OpenRecent(path) => crate::home::open_recent(app, path),
            Self::ClearRecent => {
                app.home.recent.clear();
                app.home.opened_at.clear();
            }
            Self::Settings => crate::prefs::open_at(app, 0),
            Self::Shortcuts => {
                app.keymap.open = true;
                app.keymap.read_only = false;
            }
            Self::AgentChat => crate::agent_chat::toggle(app),
            Self::Quit => crate::documents::request_quit(app),
        }
    }
}

fn file_menu(app: &mut SketApp, ctx: &egui::Context) {
    let id = egui::Id::unique("sc_file_menu");
    if !ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
        return;
    }
    let mut close = false;
    let resp = Container::new(ctx, Surface::Popup)
        .area(egui::Id::unique("sc_file_area"))
        .fixed_pos(pos2(ctx.data(|d| d.get_temp::<f32>(egui::Id::unique("sc_file_menu_x"))).unwrap_or(100.0), APP_BAR_HEIGHT))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            crate::ui_kit::Menu::show_frame(ui, crate::ui_kit::Menu::frame(), |ui| {
                ui.set_min_width(190.0);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
                ui.visuals_mut().widgets.hovered.weak_bg_fill = Tokens::get().hover;
                let item_if = |ui: &mut egui::Ui, label: &str, key: &str, icon: &str, enabled: bool| {
                    crate::context_menu::list_row(ui, &crate::context_menu::Item::action(label, label, icon).key(key).on_if(enabled)).is_some()
                };
                let item = |ui: &mut egui::Ui, label: &str, key: &str, icon: &str| item_if(ui, label, key, icon, true);
                // Insert, Save and Export act on the design, which Home hides.
                let shown = design_shown(app);
                if item(ui, app.tr("ui.new_design"), "Ctrl+N", "new") {
                    FileAction::New.run(app);
                    close = true;
                }
                if item(ui, &format!("{}…", app.tr("ui.open")), "Ctrl+O", "open") {
                    FileAction::Open.run(app);
                    close = true;
                }
                crate::ui_kit::Menu::button(ui, app.tr("ui.open_recent"), |ui| {
                    for path in app.home.recent.clone() {
                        let missing = !crate::home::available(app, &path);
                        let label = format!("{path}{}", if missing { " (missing)" } else { "" });
                        if crate::context_menu::button(ui, &label, "open").clicked() {
                            FileAction::OpenRecent(path).run(app);
                            close = true;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if crate::context_menu::button(ui, app.tr("ui.clear_recent"), "delete").clicked() {
                        FileAction::ClearRecent.run(app);
                        close = true;
                        ui.close();
                    }
                });
                let import_key = crate::keymap::display_for(app, "file.insert_mesh")
                    .unwrap_or_else(|| if cfg!(target_os = "macos") { "Cmd+I".into() } else { "Ctrl+I".into() });
                if item_if(ui, "Import 3D Model…", &import_key, "import", shown) {
                    FileAction::Command("file.insert_mesh").run(app);
                    close = true;
                }
                if item_if(ui, "Insert CAD Solid (STEP / DXF)…", "", "import", shown) {
                    FileAction::Command("file.insert_step").run(app);
                    close = true;
                }
                if item_if(ui, &format!("{}…", app.tr_label("Insert Point Cloud")), "", "import", shown) {
                    app.start("pointcloud.insert");
                    close = true;
                }
                if item_if(ui, &format!("{}…", app.tr_label("Insert Design")), "", "component", shown) {
                    FileAction::Command("component.insert_design").run(app);
                    close = true;
                }
                if item_if(ui, app.tr("ui.save"), "Ctrl+S", "save", shown) {
                    FileAction::Save.run(app);
                    close = true;
                }
                // Save As shows the shortcut of the command it runs, from the app's own registry.
                let save_as_key = crate::keymap::effective(app, "file.save_as").unwrap_or_default();
                if item_if(ui, &format!("{}…", app.tr("ui.save_as")), &save_as_key, "save", shown) {
                    FileAction::SaveAs.run(app);
                    close = true;
                }
                if item(ui, "Save As JSON…", "", "save") {
                    FileAction::SaveJson.run(app);
                    close = true;
                }
                // The browser build: a copy as a downloaded file, whatever Save does.
                if app.services.download_design.is_some() && item_if(ui, "Download design", "", "export", shown) {
                    download_design(app);
                    close = true;
                }
                if app.services.browser_files.is_some() && item(ui, "Files in this browser…", "", "open") {
                    if let Some(show) = &app.services.browser_files {
                        show();
                    }
                    close = true;
                }
                ui.separator();
                // Each format's item runs its own command, so it shows that command's key.
                let key = |id: &str| crate::keymap::effective(app, id).unwrap_or_default();
                let (stl_key, mf_key, step_key, obj_key, urdf_key) =
                    (key("file.export_stl"), key("file.export_3mf"), key("file.export_step"), key("file.export_obj"), key("file.export_urdf"));
                if item_if(ui, "Export STL…", &stl_key, "mesh_export", shown) {
                    FileAction::Command("file.export_stl").run(app);
                    close = true;
                }
                if item_if(ui, "Export 3MF…", &mf_key, "mesh_export", shown) {
                    FileAction::Command("file.export_3mf").run(app);
                    close = true;
                }
                if item_if(ui, "Send to Slicer", "", "mesh_export", shown && crate::dialogs_save_mesh::available(app)) {
                    app.start("file.send_to_slicer");
                    close = true;
                }
                if item_if(ui, "Export STEP…", &step_key, "export", shown) {
                    FileAction::Command("file.export_step").run(app);
                    close = true;
                }
                if item_if(ui, app.tr_label("Export Current View as DXF…"), "", "export", shown) {
                    FileAction::Command("file.export_view_dxf").run(app);
                    close = true;
                }
                if item_if(ui, "Export OBJ…", &obj_key, "export", shown) {
                    FileAction::Command("file.export_obj").run(app);
                    close = true;
                }
                if item_if(ui, "Export URDF…", &urdf_key, "export", shown) {
                    FileAction::Command("file.export_urdf").run(app);
                    close = true;
                }
                if item(ui, "Export Design as JSON…", "", "export") {
                    FileAction::Export("json").run(app);
                    close = true;
                }
                ui.separator();
                if item_if(ui, "Tags & Layers…", "", "list", shown) {
                    FileAction::Command("tags.manage").run(app);
                    close = true;
                }
                if item_if(ui, &format!("{}…", app.tr_label("Design Properties")), "", "settings", shown) {
                    FileAction::Command("document.metadata").run(app);
                    close = true;
                }
                ui.separator();
                // The browser can install SketCraft as an app (`install`): only while it offers to.
                if let Some(offer) = crate::install::offered(app)
                    && crate::context_menu::button(ui, crate::i18n::tr("install.menu"), "new")
                        .backing_on_hover_text(crate::i18n::tr("install.button_tip"))
                        .clicked()
                {
                    crate::install::start(app, offer);
                    close = true;
                }
                if item(ui, &format!("{}…", app.tr("ui.preferences")), "", "settings") {
                    FileAction::Settings.run(app);
                    close = true;
                }
                if item(ui, &format!("{}…", app.tr("ui.keyboard_shortcuts")), "", "list") {
                    FileAction::Shortcuts.run(app);
                    close = true;
                }
                if item(ui, app.tr("ui.quit"), "", "close") {
                    FileAction::Quit.run(app);
                    close = true;
                }
            });
        });
    let just_opened = ctx.data(|d| d.get_temp::<u64>(egui::Id::unique("sc_file_menu_opened"))) == Some(ctx.cumulative_pass_nr());
    if close || (!just_opened && resp.response.clicked_elsewhere()) {
        ctx.data_mut(|d| d.insert_temp(id, false));
    }
}

/// Whether the design is on screen: viewport/menu editing stays disabled behind File.
/// File owns its explicit Save/Save As/export actions and returns to the design before dispatch.
pub fn design_shown(app: &SketApp) -> bool {
    !app.home.open
}

pub fn save(app: &mut SketApp) {
    if crate::browser::needs_capture(app, crate::browser::SAVE) {
        app.tree.capture_prompt = Some(crate::browser::SAVE.into());
        return;
    }
    if app.session.path.is_some() {
        // Read-only, or changed on disk since it was opened: Save As, or a question, instead.
        if crate::open_elsewhere::check_save(app) {
            crate::save_job::start(app, "file.save", json!({}));
        }
    } else {
        save_as(app);
    }
}

pub(crate) fn save_as(app: &mut SketApp) {
    save_as_with_extension(app, sketcraft_engine::io::DESIGN_EXT);
}

/// Save As offering a copy's name ("Bracket copy"), so the file it is a copy of is kept.
pub(crate) fn save_copy(app: &mut SketApp) {
    let name = crate::i18n::format("document.copy_name", &[("name", &app.session.save_as_name())]);
    save_as_named(app, &name, sketcraft_engine::io::DESIGN_EXT);
}

/// Save As offers the open file's own name, not the name stored in the design (#8329).
fn save_as_with_extension(app: &mut SketApp, extension: &str) {
    let name = app.session.save_as_name();
    save_as_named(app, &name, extension);
}

pub(crate) fn save_as_named(app: &mut SketApp, name: &str, extension: &str) {
    // The Capture Position question first, as Save asks it: the file can't hold a move not
    // captured yet (#9736).
    if crate::browser::needs_capture(app, crate::browser::SAVE) {
        app.tree.capture_prompt = Some(crate::browser::save_as_question(name, extension));
        return;
    }
    let name = format!("{name}.{extension}");
    app.pick_save(&name, &[sketcraft_engine::io::DESIGN_EXT, "json"], |app, path| {
        // A save under way answers a closing tab when it ends.
        if let Some(path) = path
            && crate::save_job::start(app, "file.save_as", json!({"path": path}))
        {
            return;
        }
        crate::documents::saved_for_close(app);
    });
}

/// File > Download design (browser build): the open design as a `.sc` file download.
pub(crate) fn download_design(app: &mut SketApp) {
    if crate::browser::needs_capture(app, crate::browser::SAVE) {
        app.tree.capture_prompt = Some(crate::browser::DOWNLOAD.into());
        return;
    }
    let name = format!("{}.{}", app.session.doc.name, sketcraft_engine::io::DESIGN_EXT);
    let result = app
        .session
        .design_container(sketcraft_engine::io::DesignCompression::Fast, false, false)
        .map_err(|e| e.to_string())
        .and_then(|bytes| app.services.download_design.as_ref().map_or(Ok(()), |download| download(&name, &bytes)));
    match result {
        Ok(()) => app.set_status(format!("Downloaded {name}"), false),
        Err(e) => app.set_status(format!("Download design: {e}"), true),
    }
}

pub(crate) fn export(app: &mut SketApp, ext: &str) {
    export_as(app, ext, None);
}

/// Each format's own export command, started from its key, command search or the File menu,
/// does what that File › Export item does: STEP, OBJ and URDF ask where to save, STL and 3MF
/// open the mesh dialog with the format chosen. `false`: `id` is no such command.
pub(crate) fn start_export(app: &mut SketApp, id: &str) -> bool {
    let (extension, mesh) = match id {
        "file.export_step" => ("step", false),
        "file.export_obj" => ("obj", false),
        "file.export_urdf" => ("urdf", false),
        "file.export_stl" => ("stl", true),
        "file.export_3mf" => ("3mf", true),
        _ => return false,
    };
    // As the File menu's items: not while a file is being picked or loaded, or behind a bug report.
    if !FileAction::Export(extension).enabled(app) {
        return true;
    }
    if mesh {
        crate::dialogs_save_mesh::start_with_format(app, extension);
    } else {
        export(app, extension);
    }
    true
}

/// Export to a file with extension `ext`, in `format` when the extension alone doesn't name it.
fn export_as(app: &mut SketApp, ext: &str, format: Option<&str>) {
    let name = format!("{}.{ext}", app.session.doc.name);
    let format = format.map(str::to_string);
    app.pick_save(&name, &[ext], move |app, path| {
        let Some(path) = path else { return };
        let mut params = json!({"path": path});
        if let Some(format) = format {
            params["format"] = json!(format);
        }
        if app.run("file.export", params).is_ok() {
            app.set_status(format!("Exported {path}"), false);
        }
    });
}

/// Choose at most two lines at spaces or after `/`, then truncate each line at
/// Unicode character boundaries. An oversized word stays on one line with an ellipsis.
fn ribbon_button_lines(painter: &egui::Painter, label: &str, width: f32) -> Vec<String> {
    let mut lines = ribbon_label_lines(painter, label, width);
    // Right-to-left labels come in display order (see `bidi`): a line reads from its right end,
    // so a shortened line loses characters on the left, where the ellipsis then goes.
    let rtl = crate::bidi::has_rtl(label);
    for line in &mut lines {
        if ribbon_label_width(painter, line) > width {
            while !line.is_empty() {
                if rtl {
                    line.remove(0);
                } else {
                    line.pop();
                }
                let shortened = if rtl { format!("…{}", line.trim_start()) } else { format!("{}…", line.trim_end()) };
                if ribbon_label_width(painter, &shortened) <= width {
                    break;
                }
            }
            *line = if rtl { format!("…{}", line.trim_start()) } else { format!("{}…", line.trim_end()) };
        }
    }
    lines
}

fn ribbon_label_width(painter: &egui::Painter, text: &str) -> f32 {
    let mut job =
        egui::text::LayoutJob::simple(text.to_owned(), FontId::proportional(crate::theme::Tokens::get().font_body), Color32::WHITE, f32::INFINITY);
    job.halign = egui::Align::Center;
    painter.layout_job(job).size().x
}

fn ribbon_button_width(painter: &egui::Painter, label: &str) -> f32 {
    let widest = ribbon_label_lines(painter, label, BUTTON_MAX_WIDTH - BUTTON_LABEL_PADDING)
        .iter()
        .map(|line| ribbon_label_width(painter, line))
        .fold(0.0_f32, f32::max);
    (widest + BUTTON_LABEL_PADDING).clamp(BUTTON_MIN_WIDTH, BUTTON_MAX_WIDTH)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CjkScript {
    Han,
    Hiragana,
    Katakana,
    Other,
}

fn cjk_script(c: char) -> CjkScript {
    match c {
        '\u{3040}'..='\u{309F}' => CjkScript::Hiragana,
        '\u{30A0}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF65}'..='\u{FF9F}' => CjkScript::Katakana,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' => CjkScript::Han,
        _ => CjkScript::Other,
    }
}

/// Whether byte offset `end` sits at a word boundary in a spaceless Japanese ribbon label.
fn cjk_word_break(label: &str, end: usize) -> bool {
    let (Some(prev), Some(next)) = (label.get(..end).and_then(|s| s.chars().next_back()), label.get(end..).and_then(|s| s.chars().next())) else {
        return false;
    };
    let (a, b) = (cjk_script(prev), cjk_script(next));
    if a == CjkScript::Katakana && b == CjkScript::Katakana {
        let before = label.get(..end).unwrap_or_default();
        return before.ends_with("サーフェス") || before.ends_with("フラット");
    }
    match (a, b) {
        (CjkScript::Han, CjkScript::Katakana)
        | (CjkScript::Katakana, CjkScript::Han | CjkScript::Hiragana)
        | (CjkScript::Hiragana, CjkScript::Han | CjkScript::Katakana) => true,
        (CjkScript::Han, CjkScript::Hiragana) => next == 'を',
        (CjkScript::Other, CjkScript::Katakana | CjkScript::Hiragana) => prev.is_ascii_alphabetic(),
        (CjkScript::Katakana | CjkScript::Hiragana, CjkScript::Other) => next.is_ascii_alphabetic(),
        _ => false,
    }
}

fn ribbon_label_lines(painter: &egui::Painter, label: &str, width: f32) -> Vec<String> {
    if let Some((first, second)) = label.split_once('\n') {
        let first = first.trim();
        let second = second.trim();
        if !first.is_empty() && !second.is_empty() {
            return vec![first.to_owned(), second.to_owned()];
        }
    }
    let measure = |text: &str| ribbon_label_width(painter, text);
    let normalized = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let label = normalized.as_str();
    if crate::bidi::has_rtl(label) {
        // Display-order right-to-left text reads from its end: the first line takes the words
        // at the end of the string, the second line the words before them.
        let breaks = label.char_indices().filter(|(_, c)| c.is_whitespace() || *c == '/').map(|(at, c)| at + c.len_utf8());
        let mut start = label.len();
        for cut in std::iter::once(0).chain(breaks).collect::<Vec<_>>().into_iter().rev() {
            if cut >= start {
                continue;
            }
            if start < label.len() && measure(label.get(cut..).unwrap_or_default().trim()) > width {
                break;
            }
            start = cut;
        }
        let (rest, first) = label.split_at(start);
        let mut lines = vec![first.trim().to_owned()];
        if !rest.trim().is_empty() {
            lines.push(rest.trim().to_owned());
        }
        return lines;
    }
    let has_delimiters = label.chars().any(|c| c.is_whitespace() || c == '/');
    let mut end = 0;
    for (at, ch) in label.char_indices() {
        let next = at + ch.len_utf8();
        let is_break = next == label.len() || ch.is_whitespace() || ch == '/' || (!has_delimiters && cjk_word_break(label, next));
        if !is_break {
            continue;
        }
        if end > 0 && measure(label.get(..next).unwrap_or_default().trim()) > width {
            break;
        }
        end = next;
    }
    let (first, rest) = label.split_at(end);
    let mut lines = vec![first.trim().to_owned()];
    if !rest.trim().is_empty() {
        lines.push(rest.trim().to_owned());
    }
    lines
}

/// Commands opt into persistent presentation; the engine status list remains the only truth.
const PERSISTENT_COMMANDS: &[&str] = &[
    "inspect.accessibility",
    "inspect.continuity",
    "inspect.curvature_map",
    "inspect.draft",
    "inspect.environment_map",
    "inspect.section",
    "inspect.zebra",
];

/// Component Colors is a plain on/off display setting: it shows pressed like the effects above,
/// but is no status item (nothing to clear with Esc), so it stays out of the list.
const COMPONENT_COLORS: &str = "inspect.colored_components";

fn persistent_command(id: &str) -> bool {
    PERSISTENT_COMMANDS.contains(&id) || id == COMPONENT_COLORS
}

fn persistent_available(app: &SketApp, id: &str) -> bool {
    persistent_command(id) && (id != "inspect.section" || app.preferences.viewport_section_status)
}

pub(crate) fn persistent_pressed(app: &SketApp, id: &str) -> bool {
    app.preferences.persistent_ribbon_pressed
        && persistent_available(app, id)
        && if id == COMPONENT_COLORS {
            app.session.colored_components.is_some()
        } else {
            app.session.status_items().iter().any(|item| item.command == id)
        }
}

pub(crate) fn persistent_button_name(app: &SketApp, command: &sketcraft_engine::CommandSpec) -> String {
    let info = crate::render_workspace::interactive_command_info(app, command);
    let label = crate::workspace::ribbon_button_label(command);
    app.tr_label(if label == command.label { info.label } else { label }).to_string()
}

fn persistent_hint(app: &SketApp, command: &sketcraft_engine::CommandSpec, enabled: bool) -> Option<String> {
    (enabled && command.id != COMPONENT_COLORS && persistent_pressed(app, command.id)).then(|| {
        let name = persistent_button_name(app, command);
        crate::i18n::Locale::for_language(&app.preferences.language).format("ui.viewport_status_clear_hint", &[("name", &name)])
    })
}

/// Paint slots retain original z-order, while their presentation is resolved after every
/// command interaction in this ribbon pass. These records never survive the current frame.
struct PersistentRibbonPaint {
    command: &'static sketcraft_engine::CommandSpec,
    response: egui::Response,
    // Keep the allocating layer, clip and opacity when replacing these deferred slots.
    painter: egui::Painter,
    rect: Rect,
    background: egui::layers::ShapeIdx,
    label_slot: egui::layers::ShapeIdx,
    neutral: egui::Shape,
    label: Option<String>,
    enabled: bool,
    disabled: Option<String>,
}

fn paint_persistent_ribbon(app: &SketApp, ui: &egui::Ui, paints: Vec<PersistentRibbonPaint>, tab: &str, namespace: &str) {
    let t = Tokens::get();
    let mut count = 0;
    for paint in paints {
        let selected = persistent_pressed(app, paint.command.id);
        let ink = if !paint.enabled {
            t.text_dim
        } else if selected {
            t.selected_ink
        } else {
            t.text
        };
        // Resolve the genuine live tool after dispatch too: a click may have ended it.
        // The compatibility presentation keeps its original transient recipe.
        let neutral = if app.preferences.persistent_ribbon_pressed && app.tool.as_ref().is_some_and(|tool| tool.cmd == paint.command.id) {
            let accent = crate::theme::TabAccent::for_tab(&t, tab);
            egui::Shape::Rect(egui::epaint::RectShape::new(
                paint.rect,
                t.radius_sm,
                accent.soft,
                Stroke::new(1.0, accent.color),
                egui::StrokeKind::Inside,
            ))
        } else {
            paint.neutral
        };
        let background = if selected {
            count += 1;
            crate::scenario::publish_target(&format!("{namespace}:pressed:{}", paint.command.id), paint.rect);
            ui_kit::persistent_button_shape(ui, &paint.response, paint.rect)
        } else if paint.response.has_focus() && app.preferences.persistent_ribbon_pressed {
            egui::Shape::Vec(vec![neutral, ui_kit::native_button_focus(ui, &paint.response, paint.rect)])
        } else {
            neutral
        };
        paint.painter.set(paint.background, background);
        if paint.response.has_focus() {
            crate::scenario::publish_target(&format!("{namespace}:focus:{}", paint.command.id), paint.rect);
        }
        if let Some(label) = paint.label {
            let galley = ribbon_button_galley_weight(&paint.painter, &label, paint.rect.width() - BUTTON_LABEL_PADDING, ink, selected);
            paint
                .painter
                .set(paint.label_slot, egui::Shape::galley(pos2(paint.rect.center().x, ribbon_label_top(paint.rect, galley.size().y)), galley, ink));
        }
        paint.response.widget_info(|| {
            if app.preferences.persistent_ribbon_pressed && persistent_available(app, paint.command.id) {
                egui::WidgetInfo::selected(egui::Role::Button, paint.enabled, selected, persistent_button_name(app, paint.command))
            } else {
                let info = crate::render_workspace::interactive_command_info(app, paint.command);
                egui::WidgetInfo::labeled(egui::Role::Button, paint.enabled, app.tr_label(info.label))
            }
        });
        let _ = ribbon_tooltip(app, paint.response, paint.command, paint.disabled.as_deref());
    }
    crate::scenario::publish_count(&format!("{namespace}:pressed"), count as f64);
}

fn ribbon_button_galley(painter: &egui::Painter, label: &str, width: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    ribbon_button_galley_weight(painter, label, width, color, false)
}

fn ribbon_button_galley_weight(painter: &egui::Painter, label: &str, width: f32, color: Color32, selected: bool) -> std::sync::Arc<egui::Galley> {
    let lines = ribbon_button_lines(painter, label, width);
    // Explicit newlines only: egui must never split an oversized word.
    let size = crate::theme::Tokens::get().font_body;
    let mut job = egui::text::LayoutJob::simple(
        lines.join("\n"),
        if selected { crate::fonts::semibold(size) } else { FontId::proportional(size) },
        color,
        f32::INFINITY,
    );
    job.halign = egui::Align::Center;
    // Two lines sit closer than the font's own spacing, so they fit in the button under its
    // icon (#8391).
    if lines.len() > 1
        && let Some(section) = job.sections.first_mut()
    {
        section.format.line_height = Some(size + 2.0);
    }
    painter.layout_job(job)
}

/// The top of a button's label, `height` tall, in the button `whole`: below the icon, raised
/// when it would otherwise run past the button's bottom edge (#8391).
fn ribbon_label_top(whole: Rect, height: f32) -> f32 {
    (whole.top() + BUTTON_LABEL_TOP).min(whole.bottom() - BUTTON_LABEL_BOTTOM - height)
}

/// Ribbon headings use the translated labels, with normal word casing instead of registry caps.
pub(crate) fn ribbon_label(app: &SketApp, label: &str) -> String {
    let label = crate::workspace::word_cased(app.tr_label(crate::workspace::tab_label(label)));
    // Partial catalogs may already translate the word-cased name (for example "Model").
    app.tr_label(&label).to_string()
}

fn ribbon_text(ui: &egui::Ui, at: egui::Pos2, text: &str, size: f32, color: Color32, bold: bool) {
    let text = egui::RichText::new(text).size(size).color(color);
    let text = if bold { text.semibold() } else { text };
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
    ribbon_galley(ui, at, galley, color);
}

fn ribbon_galley(ui: &egui::Ui, at: egui::Pos2, galley: std::sync::Arc<egui::Galley>, color: Color32) {
    let origin = at - galley.size() / 2.0;
    let ppp = ui.ctx().pixels_per_point().max(0.5);
    let origin = pos2((origin.x * ppp).round() / ppp, (origin.y * ppp).round() / ppp);
    ui.painter().galley(origin, galley, color);
}

/// SKETCH while editing a sketch and SHEET METAL while a sheet body is active take the contextual
/// tab style; SHEET METAL is an ordinary tab otherwise. WELDMENTS selects itself on a frame member
/// but keeps the ordinary tab look, as MODEL has when selected.
pub(crate) fn contextual_tab(app: &SketApp, tab: &str) -> bool {
    tab == "SKETCH" || (tab == "SHEET METAL" && crate::workspace::sheet_body(app).is_some())
}

/// How one ribbon tab is painted: its fill, its label colour and an outline
/// (`Color32::TRANSPARENT` for none).
#[derive(Clone, Copy, Debug, PartialEq)]
struct RibbonTabStyle {
    fill: Color32,
    ink: Color32,
    outline: Color32,
}

/// A contextual tab (Sketch while editing, Sheet Metal for a sheet body) is the loudest tab
/// when it is selected: the solid accent pill. Unselected, it looks like any unselected tab
/// with its accent as an outline, so it still marks the context without outshining the
/// selected tab. Ordinary tabs use the soft accent when selected and the hover colour when
/// hovered.
fn ribbon_tab_style(t: &Tokens, tab: &str, contextual: bool, active: bool, hovered: bool) -> RibbonTabStyle {
    let accent = crate::theme::TabAccent::for_tab(t, tab);
    let idle = if hovered { t.hover } else { Color32::TRANSPARENT };
    let (fill, ink, outline) = match (contextual, active) {
        (true, true) => (accent.color, accent.on_fill(t), Color32::TRANSPARENT),
        (true, false) => (idle, t.text, accent.color),
        (false, true) => (accent.soft, accent.ink, Color32::TRANSPARENT),
        (false, false) => (idle, t.text, Color32::TRANSPARENT),
    };
    RibbonTabStyle { fill, ink, outline }
}

/// Returns `Some(already shown)` when a tab was clicked.
fn ribbon_tabs(app: &mut SketApp, ui: &mut egui::Ui, row: Rect) -> Option<bool> {
    let t = Tokens::get();
    let tabs = crate::workspace::visible_tabs(app);
    let widths: Vec<_> = tabs
        .iter()
        .map(|tab| {
            ui.painter().layout_no_wrap(ribbon_label(app, tab), crate::fonts::semibold(crate::theme::Tokens::get().font_button), t.text).size().x
                + 24.0
        })
        .collect();
    // A plus after the last tab opens the toolbar's menu, so tabs can be shown or hidden without
    // knowing about the right-click; it stays when every tab is hidden. The Render workspace's
    // one tab can't be hidden.
    let add = !crate::render_workspace::active(app);
    let add_size = TAB_ROW_HEIGHT - 2.0 * RIBBON_TAB_INSET;
    let tabs_width: f32 = widths.iter().sum::<f32>()
        + tabs.len().saturating_sub(1) as f32 * 2.0
        + tabs.iter().filter(|tab| contextual_tab(app, tab)).count() as f32 * 6.0
        + if add { 2.0 + add_size } else { 0.0 };
    // The application bar may hold the search field instead ([`HeaderLayout`]): the tabs then
    // run to the row's end.
    let in_ribbon = !search_in_header(app);
    let search_width = if in_ribbon { crate::ui_kit::CommandSearch::closed_width(ui.ctx()) } else { 0.0 };
    // The first tab and the search field sit the same distance from the card's top, left and right
    // edges, so their outer corners are concentric with the card's corners.
    let floating = Container::for_layout(app.ui.layout, Surface::Panel).floating();
    let inset = RIBBON_TAB_INSET;
    let corner = if floating { t.radius_lg.saturating_sub(inset as u8).max(t.radius_sm / 2) } else { t.control_radius.saturating_sub(1) };
    let render = crate::render_workspace::active(app);
    let action_labels = [app.tr_label("Render").to_string(), app.tr_label("Cancel").to_string()];
    let action_widths = action_labels
        .each_ref()
        .map(|label| ui.painter().layout_no_wrap(label.clone(), crate::fonts::semibold(t.font_button), t.text).size().x + 24.0);
    let action_width = if render { action_widths.iter().sum::<f32>() + 2.0 * t.space_2 } else { 0.0 };
    let search = Rect::from_min_size(
        pos2(row.right() - search_width - inset - action_width, row.top() + inset),
        vec2(search_width, TAB_ROW_HEIGHT - 2.0 * inset),
    );
    if render {
        let mut x = search.right() + t.space_2;
        for ((id, label), width) in ["render.start", "render.cancel"].into_iter().zip(action_labels).zip(action_widths) {
            let rect = Rect::from_min_size(pos2(x, row.top() + inset), vec2(width, search.height()));
            let ready = crate::render_workspace::job_ready(app, id);
            let response = crate::ui_kit::button_at(ui, id, rect, label, crate::ui_kit::Button::Enabled(ready.is_ok()));
            crate::scenario::publish_target(&format!("ribbon:{id}"), response.rect);
            if response.clicked() {
                app.start(id);
            }
            if let Err(reason) = ready {
                response.backing_on_disabled_hover_text(reason);
            }
            x += width + t.space_2;
        }
    }
    // The single editor is drawn on the foreground layer by shortcut_box::show, so it
    // can grow over the ribbon without being clipped to the tab row.
    if in_ribbon {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("sc_ribbon_search"), search));
    }
    let changed = ui.ctx().data_mut(|d| {
        let id = egui::Id::unique("sc_ribbon_visible_tab");
        let state = (app.ui.tab.clone(), row.width());
        let changed = d.get_temp::<(String, f32)>(id) != Some(state.clone());
        d.insert_temp(id, state);
        changed
    });
    let end = if in_ribbon { search.left() - 6.0 } else { row.right() - inset };
    let span = Rect::from_min_max(row.left_top() + vec2(inset, 0.0), pos2(end.max(row.left() + inset), row.bottom()));
    let mut clicked = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(span), |ui| {
        ui.set_clip_rect(span);
        egui::ScrollArea::horizontal()
            .id_salt("ribbon_tabs")
            .animated(false)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let (all, _) = ui.allocate_exact_size(vec2(tabs_width, TAB_ROW_HEIGHT), Sense::hover());
                let mut x = all.left();
                for (tab, width) in tabs.into_iter().zip(widths) {
                    let contextual = contextual_tab(app, tab);
                    if contextual {
                        x += 6.0;
                    }
                    let tr = Rect::from_min_size(pos2(x, all.top() + inset), vec2(width, TAB_ROW_HEIGHT - 2.0 * inset));
                    let active = app.ui.tab == tab;
                    if active && changed {
                        ui.scroll_to_rect(tr, Some(egui::Align::Center));
                    }
                    let response = ui.interact(tr, ui.scope_id().with(("tab", tab)), Sense::click());
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, ribbon_label(app, tab)));
                    let style = ribbon_tab_style(&t, tab, contextual, active, response.hovered());
                    ui.painter().rect(tr, corner, style.fill, Stroke::new(1.0, style.outline), egui::StrokeKind::Inside);
                    ribbon_text(ui, tr.center(), &ribbon_label(app, tab), t.font_button, style.ink, active || contextual);
                    if ui.clip_rect().contains(tr.center()) {
                        crate::scenario::publish_target(&format!("tab:{tab}"), tr);
                        crate::scenario::publish_rect(&format!("toolbar_tab:{tab}"), tr);
                        let aliases: &[&str] = match tab {
                            "MODEL" => &["SOLID", "PLASTIC"],
                            "SURFACE" => &["MESH"],
                            "MANAGE" => &["UTILITIES"],
                            _ => &[],
                        };
                        for alias in aliases {
                            crate::scenario::publish_handle(&format!("tab:{alias}"), tr.center());
                        }
                    }
                    if response.clicked() {
                        clicked = Some(active);
                        app.ui.tab = tab.to_string();
                        // Floating ribbons draw after the viewport. An instant surface switch
                        // needs one more pass too; settled tabs never schedule their own frames.
                        ui.ctx().request_repaint();
                    }
                    if response.secondary_clicked() {
                        crate::context_menu::open_for(
                            app,
                            response.interact_pointer_pos().unwrap_or(tr.left_bottom()),
                            crate::context_menu::Target::Toolbar,
                        );
                    }
                    x += width + 2.0;
                }
                if add {
                    let pr = Rect::from_min_size(pos2(x, all.top() + inset), vec2(add_size, add_size));
                    let open = crate::context_menu::open_from(app, pr, &crate::context_menu::Target::Toolbar);
                    if open && !ui.rect_contains_pointer(pr) {
                        ui.painter().rect_filled(pr, t.control_radius, t.hover);
                    }
                    let response = crate::ui_kit::quiet_icon_button_at(ui, pr, "plus", app.tr("prefs.toolbar_tabs"));
                    if ui.clip_rect().contains(pr.center()) {
                        crate::scenario::publish_target("ribbon:add_tab", pr);
                    }
                    if response.clicked() {
                        // A click leaves no keyboard focus here, as on the tabs, so single-key
                        // shortcuts such as S still work afterwards.
                        response.surrender_focus();
                        crate::context_menu::toggle_for(app, pr, pr.left_bottom(), crate::context_menu::Target::Toolbar);
                    }
                }
            });
    });
    if matches!(app.ui.tab.as_str(), "SKETCH" | "SHEET METAL") {
        ui.painter().line_segment(
            [row.left_bottom() - vec2(0.0, 1.0), row.right_bottom() - vec2(0.0, 1.0)],
            Stroke::new(2.0, panel_accent(&t, &app.ui.tab, "")),
        );
    } else {
        t.secondary_line(
            ui.painter(),
            [row.left_bottom() - vec2(0.0, 0.5), row.right_bottom() - vec2(0.0, 0.5)],
            t.toolbar.lerp_to_gamma(t.border, 0.5),
        );
    }
    clicked
}

/// The dropdown chevron every ribbon menu shares.
fn dropdown_arrow(painter: &egui::Painter, at: egui::Pos2, ink: Color32) {
    crate::ui_kit::dropdown_indicator(painter, Rect::from_center_size(at, vec2(10.0, 10.0)), ink);
}

pub(crate) fn shell_layout(app: &SketApp) -> crate::shell::LayoutMode {
    if crate::render_workspace::active(app) { crate::shell::LayoutMode::Floating } else { app.ui.layout }
}

pub(crate) fn shell_cards(app: &SketApp, available: Rect) -> crate::shell::Cards {
    if crate::render_workspace::active(app) {
        crate::ui_kit::StageBar::cards(available)
    } else {
        crate::shell::Cards::new(available, shell_layout(app), reserved_height(app))
    }
}

/// The ribbon is folded to its tab row (`ribbon_tabs_only`, #10112). The Render workspace keeps
/// its own stage bar whole.
pub(crate) fn tabs_only(app: &SketApp) -> bool {
    app.preferences.ribbon_tabs_only && !crate::render_workspace::active(app)
}

/// The height the ribbon keeps from the view: the tab row alone while it is folded, whose panels
/// then open over the view.
pub(crate) fn reserved_height(app: &SketApp) -> f32 {
    if tabs_only(app) { TAB_ROW_HEIGHT + 1.0 } else { ribbon_height(app.preferences.ribbon_labels) }
}

/// Where one pass of the ribbon draws: the folded tab row, the whole ribbon, or the whole ribbon
/// opened over the view from the folded row.
#[derive(Clone, Copy)]
struct RibbonShape {
    floating: bool,
    cards: crate::shell::Cards,
    r: Rect,
    ws: Rect,
    folded: bool,
}

pub fn toolbar(app: &mut SketApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    crate::workspace::sync_context(app);
    let layout = shell_layout(app);
    let floating = Container::for_layout(layout, Surface::Panel).floating();
    let folded = tabs_only(app);
    if !folded {
        app.ui.ribbon_open = false;
    }
    let available = ui.available_rect_before_wrap();
    let cards = shell_cards(app, available);
    let height = if floating { cards.bottom - available.top() } else { reserved_height(app) };
    let fill = if floating { Color32::TRANSPARENT } else { t.toolbar };
    let host = Container::for_layout(layout, Surface::Chrome);
    let ribbon = host.edge("sc_toolbar", Edge::Top);
    let frame = host.edge_frame(fill);
    let mut tab_click = None;
    let chrome = host.show_edge(ui, ribbon.exact_size(height).show_separator_line(false), frame, |ui| {
        let r = if floating { Rect::from_min_max(cards.workspace.min, cards.ribbon.max) } else { frame.outer_rect(ui.max_rect()) };
        let ws = if floating { cards.workspace } else { Rect::from_min_size(r.left_top(), vec2(WORKSPACE_WIDTH, height - 1.0)) };
        tab_click = ribbon_contents(app, ui, RibbonShape { floating, cards, r, ws, folded });
    });
    if floating {
        let frame = host.frame(ui.ctx(), t.toolbar, crate::frame::Side::Left);
        for card in [cards.workspace, cards.ribbon] {
            Container::paint_inset(ui.painter(), frame.widget_rect(card), frame);
        }
    } else {
        Container::paint_inset(ui.painter(), chrome.response.rect - frame.outer_margin, frame);
    }
    if folded && (app.ui.ribbon_open || tab_click.is_some()) {
        app.ui.ribbon_open = true;
        open_ribbon(app, ui, available, chrome.response.rect);
    }
}

/// The folded ribbon's tab, clicked: its panels open over the view, as the whole ribbon would
/// show them, until a command starts (`SketApp::start` closes it), the pointer leaves them,
/// Escape is pressed or the open tab is clicked again (#10112).
fn open_ribbon(app: &mut SketApp, ui: &mut egui::Ui, available: Rect, folded: Rect) {
    let t = Tokens::get();
    let layout = shell_layout(app);
    let floating = Container::for_layout(layout, Surface::Panel).floating();
    let full = ribbon_height(app.preferences.ribbon_labels);
    let cards = crate::shell::Cards::new(available, layout, full);
    let r = if floating {
        Rect::from_min_max(cards.workspace.min, cards.ribbon.max)
    } else {
        Rect::from_min_size(folded.min, vec2(folded.width(), full))
    };
    let ws = if floating { cards.workspace } else { Rect::from_min_size(r.left_top(), vec2(WORKSPACE_WIDTH, full - 1.0)) };
    let host = Container::for_layout(layout, Surface::Chrome);
    let mut tab_click = None;
    host.area(egui::Id::unique("sc_ribbon_open")).fixed_pos(r.min).show(ui.ctx(), |ui| {
        ui.set_min_size(r.size());
        if !floating {
            let frame = host.frame(ui.ctx(), t.toolbar, crate::frame::Side::Left);
            Container::paint_frame(ui.painter(), frame, r);
        }
        tab_click = ribbon_contents(app, ui, RibbonShape { floating, cards, r, ws, folded: false });
        if tab_click.is_some() {
            // The other tab's panels are drawn now; the old tab's open drop-down is gone with them.
            ui.ctx().data_mut(|d| d.remove::<egui::Id>(egui::Id::unique("sc_panel_open")));
        }
        if floating {
            let frame = host.frame(ui.ctx(), t.toolbar, crate::frame::Side::Left);
            for card in [cards.workspace, cards.ribbon] {
                Container::paint_inset(ui.painter(), frame.widget_rect(card), frame);
            }
        }
    });
    crate::scenario::publish_rect("ribbon:open", r);
    let ctx = ui.ctx().clone();
    // A panel's drop-down, a menu or a dragged button keep the panels open while the pointer is
    // on them, away from the panels.
    let busy = ctx.data(|d| d.get_temp::<egui::Id>(egui::Id::unique("sc_panel_open")).is_some())
        || app.viewport.context_menu.is_some()
        || egui::DragAndDrop::payload::<ToolbarDrag>(&ctx).is_some();
    let left = ctx.input(|i| i.pointer.latest_pos()).is_none_or(|p| !r.expand(t.space_4).contains(p));
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if tab_click == Some(true) || (!busy && (left || escape)) {
        app.ui.ribbon_open = false;
        ctx.request_repaint();
    }
}

/// One pass of the ribbon in `shape`. Returns `Some(already shown)` when a tab was clicked.
fn ribbon_contents(app: &mut SketApp, ui: &mut egui::Ui, shape: RibbonShape) -> Option<bool> {
    let t = Tokens::get();
    let layout = shell_layout(app);
    let host = Container::for_layout(layout, Surface::Chrome);
    let labels = app.preferences.ribbon_labels;
    let button_height = if labels { BUTTON_HEIGHT } else { ICON_BUTTON_HEIGHT };
    let group_height = if labels { GROUP_HEIGHT } else { ICON_GROUP_HEIGHT };
    let gallery_width = if labels { CONSTRAINT_GRID_WIDTH } else { 6.0 * 27.0 };
    let RibbonShape { floating, cards, r, ws, folded } = shape;
    {
        let ribbon_left = if floating { cards.ribbon.left() } else { ws.right() };
        if floating {
            for card in [ws, cards.ribbon] {
                let fill =
                    if crate::render_workspace::active(app) && card == ws { crate::theme::TabAccent::current(ui.ctx()).soft } else { t.toolbar };
                let frame = host.frame(ui.ctx(), fill, crate::frame::Side::Left);
                host.paint_background(ui.painter(), frame, card);
                // Blank card space owns clicks as well as its buttons.
                ui.interact(card, ui.scope_id().with(("shell_card", card.left().to_bits())), Sense::click_and_drag());
            }
            if !crate::render_workspace::active(app) {
                crate::scenario::publish_rect("ribbon:card", cards.ribbon);
            }
        }
        app.shell.cards = vec![ws, Rect::from_min_max(pos2(ribbon_left, r.top()), r.max)];
        if crate::render_workspace::active(app) {
            ui.set_clip_rect(ui.clip_rect().intersect(r));
        }
        let painter = ui.painter().clone();
        let bg = ui.interact(if floating { cards.ribbon } else { r }, ui.scope_id().with("toolbar_bg"), Sense::click());
        if bg.secondary_clicked() {
            crate::context_menu::open_for(app, bg.interact_pointer_pos().unwrap_or(r.left_bottom()), crate::context_menu::Target::Toolbar);
        }
        let wresp = ui.interact(ws, ui.scope_id().with("workspace_switcher"), Sense::click());
        let open = app.viewport.context_menu.is_some() && app.menu.target == crate::context_menu::Target::Workspace;
        if wresp.hovered() || open {
            painter.rect_filled(ws, if floating { t.radius_lg } else { 0 }, t.hover);
        }
        // Keep the accent straight while leaving the floating card's top corners round.
        let accent_inset = if crate::render_workspace::active(app) {
            t.space_12
        } else if floating {
            f32::from(t.radius_lg)
        } else {
            0.0
        };
        painter.rect_filled(
            Rect::from_min_size(ws.left_top() + vec2(accent_inset, 0.0), vec2((ws.width() - 2.0 * accent_inset).max(0.0), t.space_3)),
            egui::CornerRadius { nw: 0, ne: 0, sw: if floating { t.radius_sm / 2 } else { 0 }, se: if floating { t.radius_sm / 2 } else { 0 } },
            if crate::render_workspace::active(app) { crate::theme::TabAccent::current(ui.ctx()).color } else { t.accent },
        );
        if !floating {
            t.secondary_line(&painter, [ws.right_top(), ws.right_bottom()], t.border);
        }
        let render = crate::render_workspace::active(app);
        if render {
            crate::ui_kit::StageBar::workspace(ui, ws, "ws_render", app.tr("ui.render"), app.tr("ui.ribbon_workspace"));
        } else if folded {
            // Folded to the tab row: the workspace's name and its arrow, on one line.
            let galley = egui::WidgetText::from(egui::RichText::new(app.tr("ui.design")).size(t.font_button).color(t.text)).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                (ws.width() - 2.0 * t.space_8 - t.space_12).max(0.0),
                egui::TextStyle::Button,
            );
            let label_width = galley.size().x;
            ribbon_galley(ui, pos2(ws.center().x - 6.0, ws.center().y + 1.0), galley, t.text);
            dropdown_arrow(&painter, pos2(ws.center().x - 6.0 + label_width / 2.0 + 7.0, ws.center().y + 1.5), t.text_dim);
        } else {
            let workspace_icon = if render { "ws_render" } else { "ws_design" };
            icons::paint_icon(
                &painter,
                Rect::from_center_size(pos2(ws.center().x, ws.top() + if labels { 39.0 } else { 25.0 }), vec2(30.0, 30.0)),
                workspace_icon,
                1.0,
            );
            ribbon_text(
                ui,
                pos2(ws.center().x, ws.top() + if labels { 69.0 } else { 50.0 }),
                app.tr(if render { "ui.render" } else { "ui.design" }),
                t.font_button,
                t.text,
                true,
            );
            let label = app.tr("ui.ribbon_workspace");
            // Long translations share the switcher's width with the arrow and its gap.
            let galley = egui::WidgetText::from(egui::RichText::new(label).size(t.font_small).color(t.text_dim)).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                (ws.width() - 2.0 * t.space_8 - t.space_12).max(0.0),
                egui::TextStyle::Button,
            );
            let label_width = galley.size().x;
            ribbon_galley(ui, pos2(ws.center().x - 6.0, ws.top() + if labels { 89.0 } else { 70.0 }), galley, t.text_dim);
            dropdown_arrow(&painter, pos2(ws.center().x - 6.0 + label_width / 2.0 + 7.0, ws.top() + if labels { 89.5 } else { 70.5 }), t.text_dim);
        }
        if wresp.backing_on_hover_text(app.tr("ui.change_workspace")).clicked() {
            crate::context_menu::toggle_for(app, ws, ws.left_bottom(), crate::context_menu::Target::Workspace);
        }
        crate::scenario::publish_rect("toolbar", r);
        crate::scenario::publish_rect("workspace_switcher", ws);
        crate::scenario::publish_target("ribbon:workspace", ws);
        if render {
            crate::render_ui::stage_bar(app, ui, cards.ribbon);
            return None;
        }
        let tab_click = ribbon_tabs(app, ui, Rect::from_min_max(pos2(ribbon_left, r.top()), pos2(r.right(), r.top() + TAB_ROW_HEIGHT)));
        if folded {
            crate::scenario::publish_rect("ribbon:folded", r);
            return tab_click;
        }
        // Panels.
        let tab = app.ui.tab.clone();
        // The command whose tool or dialog is open right now, shown pressed in the toolbar.
        let running = running_command(app);
        // The command the search points out (#7287): a preview while a result is hovered, or the
        // short pulse after starting one.
        let now = ui.input(|i| i.time);
        let reveal = app.sbox.reveal.clone().filter(|r| r.preview || now - r.since < crate::shortcut_box::REVEAL_PULSE);
        let pulse = reveal.as_ref().map_or(0.0, |r| r.strength(now, ui.ctx().global_style().animation_time > 0.0));
        if reveal.is_some() {
            ui.ctx().request_repaint();
        }
        let mut persistent_paints = Vec::new();
        let mut px = ribbon_left;
        let top = r.top() + TAB_ROW_HEIGHT;
        let mut drop: Option<(&'static str, &'static str, Option<&'static str>)> = None;
        let mut menu: Option<(egui::Pos2, &'static str)> = None;
        // A drop-down row's pin icon clicked: pin or unpin that command.
        let mut toggle: Option<&'static str> = None;
        // The open drop-down (a button dropped on it leaves the toolbar).
        let mut popup_rect: Option<Rect> = None;
        // Search can draw a preview of its own. Only the open panel menu follows the pointer.
        let mut open_panel_menu = None;
        let mut hovered_panel_menu = None;
        let follow_panel_hover = app.preferences.ribbon_menus_follow_hover
            && ui.input(|i| {
                !i.pointer.any_down()
                    && i.events.iter().any(|e| matches!(e, egui::Event::PointerMoved(_) | egui::Event::PointerGone))
                    && !i.events.iter().any(|e| matches!(e, egui::Event::Touch { .. }))
            })
            && !egui::DragAndDrop::has_payload_of_type::<ToolbarDrag>(ui.ctx());
        let mut overflow = Vec::new();
        // Every panel with all its buttons (the user's pins have no cap); a toolbar too narrow
        // for them narrows the buttons, then trims panels into their drop-down and the » menu.
        let editing_in_place = app.edit_in_place.active.is_some();
        let lists: Vec<_> = crate::toolbar_custom::panel_lists(&tab)
            .into_iter()
            .filter(|(panel, _, _)| editing_in_place || !(tab == "MODEL" && *panel == "FINISH EDITING"))
            .map(|(panel, mut cmds, promote)| {
                let promote = if tab == "SKETCH" && panel == "CREATE" && !app.preferences.sketch_construction_lines {
                    promote.saturating_sub(1)
                } else {
                    promote
                };
                if !app.preferences.sketch_construction_lines {
                    cmds.retain(|c| sketcraft_engine::cmd::construction_line_mode(c.id).is_none());
                }
                let mut shown = crate::toolbar_custom::buttons(app, &tab, panel, &cmds, promote);
                // Gallery cells are painted together after the large buttons, in reading order.
                if tab == "SKETCH" && panel == "CONSTRAIN" {
                    shown.sort_by_key(|c| {
                        if app.toolbar_custom.in_gallery(&tab, panel, c.id) {
                            crate::workspace::CONSTRAINT_GALLERY.iter().position(|id| *id == c.id).map(|i| i + 1).unwrap_or(0)
                        } else {
                            0
                        }
                    });
                }
                (panel, cmds, shown)
            })
            .collect();
        let sizes: Vec<PanelSize> = lists
            .iter()
            .map(|(panel, _, shown)| {
                let mut widths: Vec<_> = shown
                    .iter()
                    .filter(|c| !app.toolbar_custom.in_gallery(&tab, panel, c.id))
                    .map(|c| {
                        let family = crate::sketch_families::family(c.id)
                            .filter(|f| tab == "SKETCH" && *panel == "CREATE" && f.commands.first().copied() == Some(c.id));
                        let label = family.map_or_else(|| crate::workspace::ribbon_button_label(c), crate::workspace::ribbon_family_label);
                        if labels {
                            ButtonSize { width: ribbon_button_width(&painter, app.tr_label(label)), min_width: BUTTON_MIN_WIDTH }
                        } else {
                            let width = if family.is_some() { 50.0 } else { 40.0 };
                            ButtonSize { width, min_width: width }
                        }
                    })
                    .collect();
                if shown.iter().any(|c| app.toolbar_custom.in_gallery(&tab, panel, c.id)) {
                    widths.push(ButtonSize { width: gallery_width, min_width: gallery_width });
                }
                PanelSize { buttons: widths }
            })
            .collect();
        let right = r.right();
        if crate::workspace::visible_tabs(app).is_empty() {
            // Every tab is hidden: the ribbon says how to bring them back.
            let at = pos2((ribbon_left + right) / 2.0, top + group_height / 2.0);
            ribbon_text(ui, at, app.tr("ui.toolbar_tabs_hidden"), t.font_body, t.text_dim, false);
            crate::scenario::publish_handle("ribbon:tabs_hidden", at);
        }
        // The Sketch exit group follows the last sketch panel, and is always kept room for so it
        // stays reachable at every width; when panels go into the » menu it moves to the far right.
        let finish_plan =
            sizes.last().and_then(|size| if tab == "SKETCH" || (tab == "MODEL" && editing_in_place) { Some(Plan::full(size)) } else { None });
        let finish_width = finish_plan.as_ref().map_or(0.0, |p| p.width().max(80.0));
        // The Finish group keeps its own gap before it, so the panels on its left give up room
        // (collapse) before they would touch it.
        let finish_gap = if finish_plan.is_some() { 4.0 } else { 0.0 };
        let normal_count = sizes.len().saturating_sub(usize::from(finish_plan.is_some()));
        let mut plans = fit(sizes.get(..normal_count).unwrap_or_default(), right - 4.0 - px - finish_width - finish_gap);
        let finish_right = plans.iter().any(|p| p.fit == Fit::More);
        plans.extend(finish_plan);
        crate::scenario::publish_count("toolbar_collapsed", plans.iter().filter(|p| p.fit == Fit::Collapsed).count() as f64);
        crate::scenario::publish_count("toolbar_more", plans.iter().filter(|p| p.fit == Fit::More).count() as f64);
        crate::scenario::publish_count(
            "toolbar_trimmed",
            plans.iter().zip(&sizes).filter(|(p, s)| p.fit == Fit::Buttons && p.shown < s.buttons.len()).count() as f64,
        );
        for ((panel, cmds, shown), plan) in lists.into_iter().zip(plans) {
            let panel: &&str = &panel;
            if plan.fit == Fit::More {
                overflow.push((*panel, menu_commands(&cmds, &shown)));
                continue;
            }
            // A panel left with no buttons (all removed from the toolbar, or saved buttons that no
            // longer exist) is drawn collapsed too: one button that opens its drop-down, so its
            // commands never vanish from the toolbar (#7166). Both are COLLAPSED_WIDTH wide.
            let collapsed = plan.fit == Fit::Collapsed || (shown.is_empty() && !cmds.is_empty());
            let trimmed = plan.shown < plan.widths.len();
            let finish = (tab == "SKETCH" && *panel == "FINISH SKETCH") || (tab == "MODEL" && *panel == "FINISH EDITING");
            let pinned = finish;
            let width = if pinned { finish_width } else { plan.width() };
            if pinned && finish_right {
                px = right - finish_width;
                painter.line_segment([pos2(px, top), pos2(px, r.bottom() - 1.0)], t.tertiary_border(t.border));
            }
            let button_widths = &plan.widths;
            let sketch_create = tab == "SKETCH" && *panel == "CREATE";
            let enabled_panel = !cmds.is_empty() || *panel == "SELECT";
            // Per tab: panels of the same name on other tabs (MODIFY, CREATE…) list other commands,
            // and the drop-down's area remembers its size by this id (#874).
            let popup_id = ui.scope_id().with(("panel_menu", &tab, panel));
            // Dropped on the panel's free space (all of a collapsed panel): the button goes to
            // its end.
            let gallery: Vec<_> = shown.iter().copied().filter(|c| app.toolbar_custom.in_gallery(&tab, panel, c.id)).collect();
            let large: Vec<_> = shown.iter().copied().filter(|c| !gallery.iter().any(|g| g.id == c.id)).collect();
            let drawn = if collapsed { 0 } else { plan.shown.min(button_widths.len()) };
            // The commands the ribbon shows as buttons now (a narrow window shows fewer), which
            // `ribbon_menus_skip_pinned` leaves out of the drop-down.
            let on_ribbon: Vec<&str> =
                large.iter().take(drawn).map(|c| c.id).chain(gallery.iter().filter(|_| !collapsed && drawn > large.len()).map(|c| c.id)).collect();
            // Buttons narrower than the panel's minimum width (a one-button panel) are centred.
            let buttons_width = button_widths.iter().take(drawn).sum::<f32>() + drawn.saturating_sub(1) as f32 * BUTTON_GAP;
            let inset = if drawn == 0 { 0.0 } else { ((width - 8.0 - buttons_width) / 2.0).max(0.0).floor() };
            let bx = px + 4.0 + inset;
            let free_x = bx + buttons_width + BUTTON_GAP;
            let free = Rect::from_min_size(pos2(free_x, top + 2.0), vec2((px + width - 4.0 - free_x).max(0.0), button_height));
            let fresp = ui.interact(free, ui.scope_id().with(("panel_free", panel)), if collapsed { Sense::click() } else { Sense::hover() });
            let collapsed_hovered = collapsed && fresp.hovered();
            let mut opened = false;
            if collapsed {
                // One button: the panel's first command's icon over a caret; it opens the
                // drop-down, which lists every command of the panel.
                let br = Rect::from_center_size(free.center(), vec2((width - 8.0).max(0.0), button_height));
                crate::scenario::publish_handle(&format!("collapsed:{tab}:{panel}"), br.center());
                if fresp.hovered() && enabled_panel {
                    painter.rect_filled(br, t.radius_sm, t.hover);
                }
                if let Some(c) = shown.first().or(cmds.first()) {
                    let info = c.info(&app.session);
                    icons::paint_icon(
                        &painter,
                        Rect::from_center_size(br.center() - vec2(0.0, 3.0), vec2(26.0, 26.0)),
                        info.icon,
                        f32::from(t.icon.a()) / 255.0,
                    );
                }
                dropdown_arrow(&painter, pos2(br.center().x, br.bottom() - 3.0), t.text_dim);
                let tip = format!("{} ({})", app.tr_label(panel), shown.len().max(cmds.len()));
                if fresp.backing_on_hover_text(tip).clicked() && enabled_panel {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("sc_panel_open"), popup_id));
                    opened = true;
                }
            }
            let mut button_x = bx;
            for (c, button_width) in large.iter().zip(button_widths.iter().copied()).take(drawn) {
                // Only the family's default pin follows the part's choice. Explicitly pinned
                // variants remain exact commands, including in customised toolbars.
                let family = crate::sketch_families::family(c.id).filter(|f| sketch_create && f.commands.first().copied() == Some(c.id));
                let selected = family.and_then(|f| app.preferences.sketch_variants.selected(f)).unwrap_or(c);
                let whole = Rect::from_min_size(pos2(button_x, top + 2.0), vec2(button_width, button_height));
                button_x += button_width + BUTTON_GAP;
                let mut br = whole;
                if family.is_some() {
                    if labels {
                        br.max.y = whole.top() + 32.0;
                    } else {
                        br.max.x -= 14.0;
                    }
                }
                let info = crate::render_workspace::interactive_command_info(app, selected);
                let resp = ui.interact(br, ui.scope_id().with(("cmd", c.id)), Sense::click_and_drag());
                crate::scenario::publish_handle(&format!("toolbar:{}", c.id), br.center());
                // The button itself (`toolbar:<id>` is also a drop-down row's while one is open).
                crate::scenario::publish_handle(&format!("button:{}", c.id), br.center());
                crate::scenario::publish_rect(&format!("button:{}", c.id), whole);
                if selected.id != c.id {
                    crate::scenario::publish_handle(&format!("button:{}", selected.id), br.center());
                }
                if resp.drag_started() {
                    resp.dnd_set_drag_payload(ToolbarDrag { id: c.id, from_menu: false });
                }
                if let Some(d) = resp.dnd_hover_payload::<ToolbarDrag>()
                    && d.id != c.id
                {
                    ui_kit::insertion_marker(&painter, br.left() - 1.0, br.top() + 2.0, br.bottom() - 2.0);
                }
                if let Some(d) = resp.dnd_release_payload::<ToolbarDrag>()
                    && d.id != c.id
                {
                    drop = Some((d.id, *panel, Some(c.id)));
                }
                if resp.secondary_clicked() {
                    menu = Some((resp.interact_pointer_pos().unwrap_or(br.left_bottom()), c.id));
                }
                let persistent = persistent_available(app, selected.id);
                let background_slot = painter.add(egui::Shape::Noop);
                let accent = crate::theme::TabAccent::for_tab(&t, &tab);
                let family_open = family.is_some_and(|f| egui::Popup::is_id_open(ui.ctx(), ui.scope_id().with(("family", f.label)).with("popup")));
                // The buttons that leave the sketch or the edit (Finish, Finish with AutoConstrain,
                // Cancel, Finish Edit) never show pressed; a tool pinned into their group does.
                let exit =
                    finish && matches!(selected.id, "sketch.finish" | "sketch.finish_auto_constrain" | "sketch.cancel" | "component.finish_edit");
                let neutral = if !exit
                    && (resp.is_pointer_button_down_on()
                        || (ribbon_shows_pressed(app, selected.id) && !(persistent && app.preferences.persistent_ribbon_pressed))
                        || family_open
                        || (selected.id == "view.shots_strip" && app.preferences.render_shots_strip))
                {
                    egui::Shape::Rect(egui::epaint::RectShape::new(
                        whole,
                        t.radius_sm,
                        accent.soft,
                        Stroke::new(1.0, accent.color),
                        egui::StrokeKind::Inside,
                    ))
                } else if resp.hovered() && info.enabled {
                    egui::Shape::rect_filled(whole, t.radius_sm, t.hover)
                } else {
                    egui::Shape::Noop
                };
                if !persistent {
                    painter.set(background_slot, neutral.clone());
                }
                let revealed = reveal.as_ref().filter(|r| r.id == c.id || r.id == selected.id || family.is_some_and(|f| f.commands.contains(&r.id)));
                if revealed.is_some() {
                    paint_reveal(&painter, whole, &accent, pulse, true);
                    crate::scenario::publish_rect("ribbon:reveal", whole);
                }
                if let Some(r) = revealed.filter(|r| r.preview)
                    && let Some(command) = sketcraft_engine::find_command(r.id)
                {
                    reveal_tooltip(app, ui, whole, command, egui::RectAlign::BOTTOM_START);
                }
                let icon = if selected.id == "sketch.finish" { "finish" } else { info.icon };
                let side = if finish { 30.0 } else { 26.0 };
                let icon_rect =
                    Rect::from_center_size(pos2(br.center().x, whole.top() + if labels { 16.0 } else { button_height / 2.0 }), vec2(side, side));
                icons::paint_icon(&painter, icon_rect, icon, if info.enabled { 1.0 } else { 0.35 });
                let tr_label = app.tr_label(info.label).to_string();
                if !persistent {
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled() && info.enabled, &tr_label));
                }
                let clicked = resp.clicked();
                if !persistent {
                    let _ = ribbon_tooltip(app, resp.clone(), selected, info.disabled_reason.as_deref());
                }
                if clicked && info.enabled {
                    if persistent && app.preferences.persistent_ribbon_pressed {
                        resp.request_focus();
                    }
                    let id = selected.id;
                    if !end_running_tool(app, id) {
                        app.start_pinned(id, Some(whole));
                    }
                }
                if persistent {
                    let label = persistent_button_name(app, selected);
                    persistent_paints.push(PersistentRibbonPaint {
                        command: selected,
                        response: resp,
                        painter: painter.clone(),
                        rect: whole,
                        background: background_slot,
                        label_slot: painter.add(egui::Shape::Noop),
                        neutral,
                        label: labels.then_some(label),
                        enabled: ui.is_enabled() && info.enabled,
                        disabled: info.disabled_reason.clone(),
                    });
                } else if let Some(f) = family {
                    let ar = if labels {
                        Rect::from_min_max(pos2(whole.left(), whole.top() + 30.0), pos2(whole.right(), whole.top() + 48.0))
                    } else {
                        Rect::from_min_max(pos2(br.right(), whole.top()), whole.right_bottom())
                    };
                    let arrow = ui.interact(ar, ui.scope_id().with(("family", f.label)), Sense::click());
                    crate::scenario::publish_handle(&format!("family:{}", f.label), ar.center());
                    if arrow.hovered() {
                        painter.rect_filled(ar, t.radius_sm, t.hover);
                    }
                    // The title sits on the same line as the other buttons' labels.
                    if labels {
                        let title_y = whole.top() + 39.0;
                        let galley =
                            ribbon_button_galley(&painter, app.tr_label(crate::workspace::ribbon_family_label(f)), ar.width() - 16.0, t.text);
                        painter.galley(pos2(ar.center().x - 4.0, title_y - galley.size().y / 2.0), galley, t.text);
                        dropdown_arrow(&painter, pos2(ar.right() - 6.0, title_y + 0.5), t.text_dim);
                    } else {
                        dropdown_arrow(&painter, ar.center(), t.text_dim);
                    }
                    arrow.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, app.tr_label(f.label)));
                    Container::new(ui.ctx(), Surface::Popup).menu(&arrow).show(|ui| {
                        let variants: Vec<_> = f.commands.iter().filter_map(|id| sketcraft_engine::find_command(id)).collect();
                        dropdown_keys(app, ui, &variants);
                        let width = dropdown_width(app, ui, &variants);
                        sketch_menu_rows(app, ui, &variants, width, &mut menu, &mut toggle);
                    });
                } else if labels {
                    let button_label = crate::workspace::ribbon_button_label(selected);
                    let label = app.tr_label(if button_label == selected.label { info.label } else { button_label });
                    let color = if info.enabled { t.text } else { t.text_dim };
                    let galley = ribbon_button_galley(&painter, label, whole.width() - BUTTON_LABEL_PADDING, color);
                    painter.galley(pos2(whole.center().x, ribbon_label_top(whole, galley.size().y)), galley, t.text);
                }
            }
            if !collapsed && drawn > large.len() {
                if !gallery.is_empty() {
                    // A quiet rule sets the grid apart from the large button before it.
                    t.secondary_line(
                        &painter,
                        [pos2(button_x - 1.5, top + 8.0), pos2(button_x - 1.5, top + button_height.min(46.0))],
                        t.toolbar.lerp_to_gamma(t.border, 0.5),
                    );
                }
                for (index, c) in gallery.iter().enumerate() {
                    let columns = if labels { 3 } else { 6 };
                    let cell = Rect::from_min_size(
                        pos2(button_x + (index % columns) as f32 * 27.0, top + 6.0 + (index / columns) as f32 * 23.0),
                        vec2(24.0, 20.0),
                    );
                    let info = crate::render_workspace::interactive_command_info(app, c);
                    let resp = ui.interact(cell, ui.scope_id().with(("cmd", c.id)), Sense::click_and_drag());
                    let active = ribbon_shows_pressed(app, c.id);
                    let accent = crate::theme::TabAccent::for_tab(&t, &tab);
                    painter.rect(
                        cell,
                        t.radius_sm,
                        if active {
                            if accent.color == t.accent { t.panel.lerp_to_gamma(command_accent(c.icon), 0.18) } else { accent.soft }
                        } else if resp.hovered() && info.enabled {
                            t.hover
                        } else {
                            t.panel
                        },
                        Stroke::new(
                            1.0,
                            if active { if accent.color == t.accent { command_accent(c.icon) } else { accent.color } } else { t.border },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    if let Some(r) = reveal.as_ref().filter(|r| r.id == c.id) {
                        paint_reveal(&painter, cell, &accent, pulse, true);
                        crate::scenario::publish_rect("ribbon:reveal", cell);
                        if r.preview {
                            reveal_tooltip(app, ui, cell, c, egui::RectAlign::BOTTOM_START);
                        }
                    }
                    icons::paint_icon(
                        &painter,
                        Rect::from_center_size(cell.center(), vec2(15.0, 15.0)),
                        info.icon,
                        if info.enabled { 1.0 } else { 0.35 },
                    );
                    for prefix in ["toolbar", "button", "ribbon:constraint"] {
                        crate::scenario::publish_target(&format!("{prefix}:{}", c.id), cell);
                    }
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, info.enabled, app.tr_label(info.label)));
                    if resp.drag_started() {
                        resp.dnd_set_drag_payload(ToolbarDrag { id: c.id, from_menu: false });
                    }
                    if let Some(d) = resp.dnd_release_payload::<ToolbarDrag>()
                        && d.id != c.id
                    {
                        drop = Some((d.id, *panel, Some(c.id)));
                    }
                    if resp.secondary_clicked() {
                        menu = Some((resp.interact_pointer_pos().unwrap_or(cell.left_bottom()), c.id));
                    }
                    if ribbon_tooltip(app, resp, c, info.disabled_reason.as_deref()).clicked() && info.enabled && !end_running_tool(app, c.id) {
                        app.start_at(c.id, Some(cell));
                    }
                }
            }
            // A 20 px label row is also the section's existing command-list button.
            // The group area ends 1 px above the card's bottom edge, so the highlight is
            // RIBBON_TAB_INSET from the card's bottom edge (and from its left edge in the first
            // section): its corner is concentric with the card's.
            let lr = Rect::from_min_size(
                pos2(px + RIBBON_TAB_INSET, top + group_height - SECTION_HEIGHT - (RIBBON_TAB_INSET - 1.0)),
                vec2((width - 2.0 * RIBBON_TAB_INSET).max(0.0), SECTION_HEIGHT),
            );
            let lresp = ui.interact(lr, ui.scope_id().with(("panel", panel)), Sense::click());
            crate::scenario::publish_handle(&format!("panel:{tab}:{panel}"), lr.center());
            crate::scenario::publish_target(&format!("ribbon:section:{tab}:{panel}"), lr);
            // Commands this panel has only in its drop-down (not drawn as a button or cell).
            let drawn_ids: Vec<&str> = large
                .iter()
                .take(drawn)
                .flat_map(|c| crate::sketch_families::family(c.id).filter(|_| sketch_create).map_or_else(|| vec![c.id], |f| f.commands.to_vec()))
                .chain(gallery.iter().filter(|_| !collapsed && drawn > large.len()).map(|c| c.id))
                .collect();
            let hidden = |id: &str| cmds.iter().any(|c| c.id == id) && !drawn_ids.contains(&id);
            let hidden_running = app.preferences.ribbon_reveal && running.as_deref().is_some_and(hidden);
            let hidden_reveal = reveal.as_ref().filter(|r| hidden(r.id));
            let panel_open = ui.ctx().data(|d| d.get_temp::<egui::Id>(egui::Id::unique("sc_panel_open"))) == Some(popup_id);
            let accent = crate::theme::TabAccent::for_tab(&t, &tab);
            let hot = lresp.hovered() && enabled_panel;
            if follow_panel_hover && enabled_panel && !finish && (hot || collapsed_hovered) {
                hovered_panel_menu = Some(popup_id);
            }
            if hidden_running && !panel_open {
                // Its command is running from the drop-down: the label shows it like a pressed button.
                painter.rect(lr, t.radius_sm, accent.soft, Stroke::new(1.0, accent.color), egui::StrokeKind::Inside);
                crate::scenario::publish_rect(&format!("ribbon:running_section:{tab}:{panel}"), lr);
            } else if panel_open || hot {
                painter.rect_filled(lr, t.radius_sm, if panel_open { accent.soft } else { t.hover });
            }
            if hidden_reveal.is_some_and(|r| !r.preview) {
                paint_reveal(&painter, lr, &accent, pulse, true);
                crate::scenario::publish_rect("ribbon:reveal", lr);
            }
            let label = if finish { app.tr("ui.ribbon_exit").to_string() } else { ribbon_label(app, panel) };
            let panel_open = panel_open || hidden_running;
            let font = if panel_open || hot {
                crate::fonts::semibold(crate::theme::Tokens::get().font_caption)
            } else {
                FontId::proportional(crate::theme::Tokens::get().font_caption)
            };
            let label_width = painter.layout_no_wrap(label.clone(), font, t.text_dim).size().x;
            let ink = if panel_open { accent.ink } else { t.text_dim };
            ribbon_text(ui, lr.center() - vec2(if finish { 0.0 } else { 6.0 }, 0.0), &label, t.font_caption, ink, panel_open || hot);
            if !finish {
                dropdown_arrow(&painter, pos2(lr.center().x + label_width / 2.0 + 1.0, lr.center().y + 0.5), ink);
            }
            if lresp.clicked() && enabled_panel {
                ui.ctx().data_mut(|d| {
                    let key = egui::Id::unique("sc_panel_open");
                    if d.get_temp::<egui::Id>(key) == Some(popup_id) {
                        d.remove::<egui::Id>(key);
                    } else {
                        d.insert_temp(key, popup_id);
                    }
                });
            }
            // Labels, gaps and margins append; actual buttons keep their insert-before target.
            // Respect the current layer so a dropdown covering the panel is not a drop target.
            let panel_rect = Rect::from_min_max(pos2(px, top + 2.0), pos2(px + width, r.bottom()));
            crate::scenario::publish_rect(&format!("panel:{tab}:{panel}"), panel_rect);
            let over_button = ui.input(|input| input.pointer.hover_pos()).is_some_and(|p| {
                let mut x = bx;
                button_widths.iter().take(drawn).any(|width| {
                    let rect = Rect::from_min_size(pos2(x, top + 2.0), vec2(*width, button_height));
                    x += width + BUTTON_GAP;
                    rect.contains(p)
                })
            });
            if !over_button && ui.rect_contains_pointer(panel_rect) && egui::DragAndDrop::payload::<ToolbarDrag>(ui.ctx()).is_some() {
                ui_kit::insertion_marker(&painter, px + width - 4.0, top + 4.0, top + 38.0);
                if ui.input(|input| input.pointer.any_released())
                    && let Some(d) = egui::DragAndDrop::take_payload::<ToolbarDrag>(ui.ctx())
                {
                    drop = Some((d.id, *panel, None));
                }
            }
            let open = ui.ctx().data(|d| d.get_temp::<egui::Id>(egui::Id::unique("sc_panel_open"))) == Some(popup_id);
            // A hovered search result in this drop-down opens it on its row, under the search sheet.
            let previewed = hidden_reveal.is_some_and(|r| r.preview && !open);
            if open || previewed {
                let cmds = if collapsed || trimmed { menu_commands(&cmds, &shown) } else { cmds.clone() };
                let cmds = if app.preferences.ribbon_menus_skip_pinned { skip_on_ribbon(cmds, &on_ribbon) } else { cmds };
                let area = Container::new(ui.ctx(), Surface::Popup)
                    .area(popup_id)
                    .fixed_pos(lr.left_bottom())
                    .order(if open { egui::Order::Foreground } else { egui::Order::Middle })
                    .show(ui.ctx(), |ui| {
                        crate::ui_kit::Menu::show_frame(ui, crate::ui_kit::Menu::frame(), |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.set_min_width(200.0);
                            if *panel == "SELECT" {
                                selection_filter(app, ui);
                            } else if cmds.is_empty() {
                                ui.label(egui::RichText::new(crate::i18n::tr_label("Not available yet")).color(t.text_dim));
                            }
                            if sketch_create && app.preferences.ribbon_icon_grid && !cmds.is_empty() {
                                let mut paints = Vec::new();
                                let action = dropdown_grid(app, ui, &tab, &sketch_create_grouped(&cmds), &[], &mut paints);
                                grid_action(app, ui, action, &mut menu, &mut toggle);
                                paint_persistent_ribbon(app, ui, paints, &tab, "ribbon_grid");
                                return;
                            }
                            if sketch_create {
                                // One width for every row and the footer, as in the other panels.
                                let row_w = dropdown_width(app, ui, &cmds);
                                ui.set_width(row_w);
                                sketch_create_menu(app, ui, &cmds, row_w, &mut menu, &mut toggle);
                                section_footer(ui, "ui.ribbon_pin_footer");
                                return;
                            }
                            let grouped = crate::workspace::grouped_commands(&tab, panel, &cmds);
                            if app.preferences.ribbon_icon_grid && !grouped.is_empty() {
                                let mut paints = Vec::new();
                                let action = dropdown_grid(app, ui, &tab, &grouped, &crate::workspace::ungrouped(&tab, panel, &cmds), &mut paints);
                                grid_action(app, ui, action, &mut menu, &mut toggle);
                                // Paint in this popup's layer, after its command has changed the session.
                                paint_persistent_ribbon(app, ui, paints, &tab, "ribbon_grid");
                                return;
                            }
                            let cmds: Vec<_> = grouped.iter().map(|(_, c)| *c).collect();
                            let row_w = dropdown_width(app, ui, &cmds);
                            ui.set_width(row_w);
                            // Up and Down move the keyboard focus through the enabled rows (from none
                            // focused: the first or the last), instead of egui's own spatial move.
                            let (down, up) = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp)));
                            if down || up {
                                let enabled: Vec<&str> = cmds.iter().filter(|c| c.info(&app.session).enabled).map(|c| c.id).collect();
                                let focused = ui.ctx().memory(|m| m.focused());
                                let at = enabled.iter().position(|id| focused == Some(dropdown_row_id(id)));
                                let n = enabled.len();
                                let next = match (at, down) {
                                    (None, true) => Some(0),
                                    (None, false) => n.checked_sub(1),
                                    (Some(i), true) => Some((i + 1) % n.max(1)),
                                    (Some(i), false) => Some((i + n.max(1) - 1) % n.max(1)),
                                };
                                if let Some(id) = next.and_then(|i| enabled.get(i)) {
                                    ui.ctx().memory_mut(|m| {
                                        m.request_focus(dropdown_row_id(id));
                                        m.move_focus(egui::FocusDirection::None);
                                    });
                                }
                            }
                            // The rows scroll when the drop-down would run past the window's bottom.
                            // `min_scrolled_height` lets the rows grow past the size the area kept from
                            // an earlier, shorter list (#874); they still shrink to fit their content.
                            let room = (ui.ctx().content_rect().bottom() - ui.cursor().top() - 64.0).max(ROW_H * 3.0);
                            egui::ScrollArea::vertical().id_salt("panel_menu_rows").max_height(room).min_scrolled_height(room).show(ui, |ui| {
                                for (heading, c) in &grouped {
                                    if let Some(heading) = heading {
                                        section_heading(ui, heading, row_w);
                                    }
                                    let pinned = crate::toolbar_custom::pinned_on(app, &tab, c.id).is_some();
                                    let revealed = reveal.as_ref().is_some_and(|r| r.id == c.id);
                                    match dropdown_row(app, ui, c, row_w, pinned, revealed.then_some(pulse)) {
                                        RowAction::None => {}
                                        RowAction::Menu(at) => menu = Some((at, c.id)),
                                        RowAction::TogglePin => toggle = Some(c.id),
                                        RowAction::Start(rect) => {
                                            app.start_at(c.id, Some(rect));
                                            ui.ctx().data_mut(|d| d.remove::<egui::Id>(egui::Id::unique("sc_panel_open")));
                                        }
                                    }
                                }
                            });
                            section_footer(ui, "ui.ribbon_pin_footer");
                        });
                    });
                popup_rect = Some(area.response.rect);
                if open {
                    open_panel_menu = Some((popup_id, area.response.rect));
                }
                crate::scenario::publish_rect("toolbar_dropdown", area.response.rect);
                // A drag out of the drop-down ends with a release elsewhere: not a click elsewhere.
                let dragging = egui::DragAndDrop::has_payload_of_type::<ToolbarDrag>(ui.ctx());
                if open
                    && ((area.response.clicked_elsewhere() && !lresp.clicked() && !opened && !dragging)
                        || ui.input(|i| i.key_pressed(egui::Key::Escape)))
                {
                    ui.ctx().data_mut(|d| d.remove::<egui::Id>(egui::Id::unique("sc_panel_open")));
                }
            }
            px += width;
            painter.line_segment([pos2(px, top + 4.0), pos2(px, top + group_height - 3.0)], t.tertiary_border(t.border));
            px += PANEL_GAP;
        }
        if !overflow.is_empty() {
            let at = Rect::from_min_size(pos2(px.min(right - finish_width - 40.0), top + 4.0), vec2(38.0, button_height));
            ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
                let response = crate::ui_kit::Menu::button(ui, "»", |ui| {
                    for (panel, cmds) in &overflow {
                        crate::ui_kit::Menu::button(ui, *panel, |ui| {
                            if tab == "SKETCH" && *panel == "CREATE" {
                                sketch_create_menu(app, ui, cmds, dropdown_width(app, ui, cmds), &mut menu, &mut toggle);
                                return;
                            }
                            if *panel == "SELECT" {
                                selection_filter(app, ui);
                            }
                            for c in cmds {
                                let info = crate::render_workspace::interactive_command_info(app, c);
                                let shortcut = crate::keymap::display_for(app, c.id).unwrap_or_default();
                                let response = if persistent_available(app, c.id) && app.preferences.persistent_ribbon_pressed {
                                    let mut item = ui_kit::MenuItem::new(c.id, info.label, info.icon);
                                    item.shortcut = &shortcut;
                                    item.enabled = info.enabled;
                                    item.checked = Some(persistent_pressed(app, c.id));
                                    item.show(ui)
                                } else {
                                    ui_kit::command_menu_button(ui, c.id, info.label, info.icon, &shortcut, info.enabled)
                                };
                                let response = if let Some(hint) = persistent_hint(app, c, info.enabled) {
                                    response.backing_on_hover_text(hint)
                                } else {
                                    response
                                };
                                crate::scenario::publish_handle(&format!("toolbar:{}", c.id), response.rect.center());
                                if response.clicked() {
                                    app.start_at(c.id, Some(response.rect));
                                    ui.close();
                                }
                            }
                        });
                    }
                });
                crate::scenario::publish_target("toolbar:overflow", response.response.rect);
                response.response.backing_on_hover_text(crate::i18n::tr_label("More toolbar panels"));
            });
        }

        // Resolve persistent paint only after large buttons, dropdowns and overflow have
        // dispatched their actions. No already-painted old analysis can survive replacement.
        paint_persistent_ribbon(app, ui, persistent_paints, &tab, "ribbon");

        if follow_panel_hover {
            let ribbon = Rect::from_min_max(pos2(ribbon_left, r.top()), r.max);
            let pointer = ui.input(|i| i.pointer.hover_pos());
            let changed = ui.ctx().data_mut(|d| {
                let key = egui::Id::unique("sc_panel_open");
                let Some(open) = d.get_temp::<egui::Id>(key) else { return false };
                let menu = open_panel_menu.filter(|(id, _)| *id == open).map(|(_, rect)| rect);
                let stays_open =
                    pointer.zip(menu).is_some_and(|(p, menu)| ribbon.expand(t.space_4).contains(p) || menu.expand(t.space_4).contains(p));
                if !stays_open {
                    d.remove::<egui::Id>(key);
                    true
                } else if let Some(hovered) = hovered_panel_menu.filter(|hovered| *hovered != open) {
                    // Change the id after drawing all panels, so only one menu is drawn per pass.
                    d.insert_temp(key, hovered);
                    true
                } else {
                    false
                }
            });
            if changed {
                ui.ctx().request_repaint();
            }
        }

        // Dragging a command: its icon follows the pointer; a toolbar button dragged off the
        // toolbar (or into the open drop-down) shows it will be removed.
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let over_popup = |p: egui::Pos2| popup_rect.is_some_and(|pr| pr.contains(p));
        let leaves = |p: egui::Pos2| !r.contains(p) || over_popup(p);
        if let (Some(d), Some(p)) = (egui::DragAndDrop::payload::<ToolbarDrag>(ui.ctx()), pointer) {
            let removing = !d.from_menu && leaves(p);
            drag_ghost(app, ui.ctx(), d.id, p, removing);
            if ui.input(|i| i.pointer.any_released()) && drop.is_none() {
                egui::DragAndDrop::clear_payload(ui.ctx());
                if removing {
                    crate::toolbar_custom::remove(app, &tab, d.id);
                }
            }
        }
        if let Some((id, panel, before)) = drop {
            crate::toolbar_custom::reorder(app, &tab, id, panel, before);
        }
        if let Some(id) = toggle {
            crate::toolbar_custom::toggle(app, &tab, id);
        }
        if let Some((at, id)) = menu {
            ui.ctx().data_mut(|d| d.remove::<egui::Id>(egui::Id::unique("sc_panel_open")));
            crate::context_menu::open_for(app, at, crate::context_menu::Target::ToolbarCommand { id: id.to_string() });
        }
        tab_click
    }
}

/// A quick-access button being dragged along the bar (#10223).
#[derive(Clone)]
struct QuickAccessDrag(String);

/// The quick-access bar (#10223): the commands added from a command's menu, after Save, Undo and
/// Redo, each with its own icon. A click starts the command as its ribbon button does, a
/// right-click opens the command's menu (Remove from Quick Access), and a drag along the bar
/// reorders the buttons. Draws nothing while it is empty. Returns where the next control goes.
fn quick_access_bar(app: &mut SketApp, ui: &mut egui::Ui, bar: Rect, mut x: f32, step: f32, shown: bool) -> f32 {
    let t = Tokens::get();
    let ids: Vec<&'static sketcraft_engine::CommandSpec> =
        app.toolbar_custom.quick_access.iter().filter_map(|id| sketcraft_engine::find_command(id)).collect();
    if ids.is_empty() {
        return x;
    }
    app_bar_separator(ui, bar, x - step / 2.0 + 8.0, "quick_access");
    x += 8.0;
    let first = x;
    let mut buttons = Vec::with_capacity(ids.len());
    let mut start = None;
    let mut menu = None;
    for c in &ids {
        let br = Rect::from_center_size(pos2(x, bar.center().y), vec2(32.0, 32.0));
        x += step;
        let info = c.info(&app.session);
        let enabled = shown && info.enabled;
        let resp = ui.interact(br, ui.scope_id().with(("quick_access", c.id)), Sense::click_and_drag());
        let label = app.tr_label(info.label).to_string();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &label));
        crate::scenario::publish_target(&format!("appbar:quick:{}", c.id), br);
        if enabled && (resp.hovered() || resp.has_focus()) {
            ui.painter().rect_filled(br.shrink(3.0), t.radius_sm, t.app_bar_hover_button);
        }
        icons::paint_on(ui.painter(), Rect::from_center_size(br.center(), vec2(20.0, 20.0)), info.icon, Some(true), if enabled { 1.0 } else { 0.35 });
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), QuickAccessDrag(c.id.to_string()));
        }
        if resp.secondary_clicked() {
            menu = Some((resp.interact_pointer_pos().unwrap_or(br.left_bottom()), c.id));
        }
        let tip = match crate::keymap::display_for(app, c.id) {
            Some(key) if !key.is_empty() => format!("{label} ({key})"),
            _ => label.clone(),
        };
        let tip = if shown { tip } else { crate::i18n::format("ui.needs_design", &[("command", &label)]) };
        if resp.on_hover_text(tip).clicked() && enabled {
            start = Some((c.id, br));
        }
        buttons.push((c.id, br));
    }
    let span = Rect::from_min_max(pos2(first - step / 2.0, bar.top()), pos2(x - step / 2.0, bar.bottom()));
    crate::scenario::publish_rect("appbar:quick_access", span);
    // Dragged along the bar: the line shows where it goes; let go and it moves there.
    if let Some(drag) = egui::DragAndDrop::payload::<QuickAccessDrag>(ui.ctx())
        && let Some(p) = ui.input(|i| i.pointer.latest_pos())
    {
        let before = buttons.iter().find(|(_, br)| p.x < br.center().x).map(|(id, _)| *id);
        let line = before.and_then(|b| buttons.iter().find(|(id, _)| *id == b)).map_or(span.right(), |(_, br)| br.left());
        if span.expand2(vec2(step, 0.0)).contains(p) {
            ui_kit::insertion_marker(ui.painter(), line, bar.top() + 6.0, bar.bottom() - 6.0);
        }
        if ui.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            if span.expand2(vec2(step, 0.0)).contains(p) {
                crate::toolbar_custom::quick_move(app, &drag.0, before);
            }
        }
    }
    if let Some((id, br)) = start {
        // The exact pinned command, without replacing its family's remembered ribbon choice.
        app.start_pinned(id, Some(br));
    }
    if let Some((at, id)) = menu {
        crate::context_menu::open_for(app, at, crate::context_menu::Target::ToolbarCommand { id: id.to_string() });
    }
    x
}

/// A short divider between the brand, file actions and document tabs.
fn app_bar_separator(ui: &egui::Ui, bar: Rect, x: f32, group: &str) {
    let rect = Rect::from_center_size(pos2(x, bar.center().y), vec2(1.0, 20.0));
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, Tokens::get().app_bar.lerp_to_gamma(Tokens::get().app_bar_separator, 0.4)),
    );
    crate::scenario::publish_rect(&format!("titlebar:separator:{group}"), rect);
}

/// The chat panel's compact entry point. State stays in agent_chat; the bar only paints it.
fn assistant_button(app: &mut SketApp, ui: &mut egui::Ui, at: Rect) {
    use crate::agent_chat::AssistantMark;
    let resp = ui.interact(at, ui.scope_id().with("assistant_button"), Sense::click());
    crate::scenario::publish_target("titlebar:assistant", at);
    let mark = crate::agent_chat::assistant_mark(app);
    let tip = crate::agent_chat::assistant_tip(app, mark);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &tip));
    let state = match mark {
        AssistantMark::Quiet => crate::ui_kit::AssistantEntryState::Quiet,
        AssistantMark::Ready => crate::ui_kit::AssistantEntryState::Ready,
        AssistantMark::Busy => crate::ui_kit::AssistantEntryState::Busy,
        AssistantMark::NeedsAuth => crate::ui_kit::AssistantEntryState::NeedsAuth,
        AssistantMark::Stopped => crate::ui_kit::AssistantEntryState::Stopped,
        AssistantMark::Waiting(count) => crate::ui_kit::AssistantEntryState::Waiting(count),
    };
    crate::ui_kit::assistant_entry(ui, &resp, state, app.agent_chat.open, ui.ctx().global_style().animation_time > 0.0);
    if resp.backing_on_hover_text(tip).clicked() {
        crate::agent_chat::toggle(app);
    }
}

/// What a click on a panel drop-down row asks for.
enum RowAction {
    None,
    Start(Rect),
    Menu(egui::Pos2),
    TogglePin,
}

const ROW_H: f32 = 30.0;

const CONSTRAINT_GRID_WIDTH: f32 = 82.0;

/// Registry parameter syntax is not a user-facing description. Reuse existing catalog prose
/// where present; other commands intentionally show only their name and registered shortcut.
fn ribbon_tooltip(app: &SketApp, response: egui::Response, command: &sketcraft_engine::CommandSpec, disabled: Option<&str>) -> egui::Response {
    if let Some(hint) = persistent_hint(app, command, disabled.is_none() && response.enabled() && command.info(&app.session).enabled) {
        return response.backing_on_hover_text(hint);
    }
    response.backing_on_hover_ui(|ui| {
        let t = Tokens::get();
        let name = app.tr_label(crate::render_workspace::interactive_command_info(app, command).label);
        let name_width =
            ui.painter().layout_no_wrap(name.to_string(), crate::fonts::semibold(crate::theme::Tokens::get().font_heading), t.text).size().x;
        let key_width = crate::keymap::display_for(app, command.id)
            .map(|key| {
                ui.painter().layout_no_wrap(key.to_string(), FontId::monospace(crate::theme::Tokens::get().font_body), t.text_dim).size().x + 24.0
            })
            .unwrap_or(0.0);
        ui.set_width(280.0_f32.max(name_width + key_width + 12.0));
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(name_width, 20.0), Sense::hover());
            ribbon_text(ui, rect.center(), name, t.font_heading, t.text, true);
            if let Some(key) = crate::keymap::display_for(app, command.id) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    crate::ui_kit::Container::show_backing(
                        ui,
                        egui::Frame::NONE
                            .fill(t.panel_header)
                            .stroke(t.tertiary_border(t.border))
                            .corner_radius(t.radius_sm.saturating_sub(1))
                            .inner_margin(egui::Margin::symmetric(5, 1)),
                        |ui| {
                            ui.label(egui::RichText::new(key).monospace().color(t.text_dim));
                        },
                    );
                });
            }
        });
        let description = match command.id {
            "render.scene" => Some(app.tr("render.preview_help")),
            "render.start" => Some(app.tr("render.image_help")),
            // Sending again replaces the file the slicer has open.
            "file.send_to_slicer" => Some(app.tr("ui.slicer_resend_hint")),
            "render.preset.resolve" | "render.preset.apply" => Some(app.tr("render.ribbon_studios_help")),
            "render.library.appearance.apply" | "render.library.surface.bind" => Some(app.tr("render.ribbon_materials_help")),
            "render.library.studio.prepare" | "render.library.studio.apply" => Some(app.tr("render.ribbon_lights_help")),
            "render.library.environment.apply" | "render.stage.backdrop" => Some(app.tr("render.ribbon_environment_help")),
            "render.preflight" => Some(app.tr("render.ribbon_check_help")),
            "render.export" => Some(app.tr("render.output_format_status")),
            _ => None,
        };
        if let Some(description) = description {
            ui.add(egui::Label::new(description).wrap());
        }
        // The tool's own help, when it has one (#2758).
        if let Some(help) = crate::command_help::help(command.id) {
            ui.add(egui::Label::new(egui::RichText::new(help).color(t.text_dim)).wrap());
        }
        if let Some(reason) = disabled {
            ui.add(egui::Label::new(egui::RichText::new(app.tr_label(reason)).color(t.text_dim)).wrap());
        }
    })
}

/// The pulse around a command the search points out (#7287); `strength` 0 draws nothing.
/// `fill` tints the inside too, for a button painted before its icon and label.
fn paint_reveal(painter: &egui::Painter, rect: Rect, accent: &crate::theme::TabAccent, strength: f32, fill: bool) {
    if strength <= 0.0 {
        return;
    }
    let t = Tokens::get();
    painter.rect(
        rect,
        t.radius_sm,
        if fill { accent.soft.gamma_multiply(strength) } else { Color32::TRANSPARENT },
        Stroke::new(2.0, accent.color.gamma_multiply(strength)),
        egui::StrokeKind::Inside,
    );
}

/// A revealed command's tooltip, open without the pointer on it (#7287).
fn reveal_tooltip(app: &SketApp, ui: &egui::Ui, rect: Rect, command: &sketcraft_engine::CommandSpec, align: egui::RectAlign) {
    let mut tip = egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::unique(("ribbon_reveal", command.id)), rect);
    tip.popup = tip.popup.align(align);
    // The same text as a drop-down row's own tooltip: name and shortcut, its help, and why it is
    // disabled when it is.
    let info = crate::render_workspace::interactive_command_info(app, command);
    let label = app.tr_label(info.label);
    let shortcut = crate::keymap::display_for(app, command.id).map(|k| format!("  ({k})")).unwrap_or_default();
    let help = crate::command_help::help(command.id);
    if let Some(shown) = crate::ui_kit::Container::show_tooltip(egui::Id::unique(("ribbon_reveal", command.id)), tip, |ui| {
        ui.set_max_width(300.0);
        ui.label(egui::RichText::new(format!("{label}{shortcut}")).strong());
        if let Some(help) = help {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(help).weak());
        }
        if let Some(reason) = info.disabled_reason.as_deref() {
            ui.label(egui::RichText::new(reason).weak());
        }
    }) {
        crate::scenario::publish_rect("ribbon:reveal_tooltip", shown.response.rect);
    }
}

/// A group heading inside a panel drop-down (Edges, Faces…; Line, Rectangle…), in the list and
/// the grid: a step below the drop-down's own header (`font_caption`) and written as named, not
/// in capitals, so it reads as part of the menu rather than above it.
fn group_heading_font() -> FontId {
    crate::fonts::semibold(Tokens::get().font_micro)
}

/// A group heading's text: its name in the interface language, as written.
fn group_heading_text(label: &str) -> String {
    crate::i18n::tr_label(label).to_string()
}

fn section_heading(ui: &mut egui::Ui, label: &str, width: f32) {
    let t = Tokens::get();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    ui.painter().text(rect.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, group_heading_text(label), group_heading_font(), t.text_dim);
}

/// The note under a panel drop-down on how pinning works: `message` is its catalog id.
fn section_footer(ui: &mut egui::Ui, message: &str) {
    ui.separator();
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(egui::RichText::new(crate::i18n::tr(message)).size(crate::theme::Tokens::get().font_small).color(Tokens::get().text_dim))
            .wrap(),
    );
    ui.add_space(4.0);
}

/// Stable row id inside and outside the scrolling list, for keyboard focus.
fn dropdown_row_id(id: &str) -> egui::Id {
    egui::Id::unique(("dropdown_row", id))
}

/// How wide a panel drop-down's rows are: the widest label and shortcut, at least 200.
fn dropdown_width(app: &SketApp, ui: &egui::Ui, cmds: &[&'static sketcraft_engine::CommandSpec]) -> f32 {
    let t = Tokens::get();
    let font = FontId::proportional(t.font_button);
    cmds.iter()
        .map(|c| {
            let label = ui.painter().layout_no_wrap(app.tr_label(c.info(&app.session).label).to_string(), font.clone(), t.text).size().x;
            let key = crate::keymap::display_for(app, c.id)
                .map(|k| ui.painter().layout_no_wrap(k.to_string(), FontId::monospace(t.font_button), t.text).size().x + 24.0)
                .unwrap_or(0.0);
            6.0 + 20.0 + 8.0 + label + key + 30.0
        })
        .fold(200.0, f32::max)
}

/// True once the pointer has rested on this drop-down row long enough for its tip. egui's
/// shared tooltip grace time lets every row after the first show its tip the moment the
/// pointer reaches it; timing each row's own hover restarts the delay for each newly hovered
/// row instead (#10481).
fn dropdown_tip_ready(ui: &egui::Ui, resp: &egui::Response) -> bool {
    let ctx = ui.ctx();
    let key = resp.id.with("sc_dropdown_tip");
    if !resp.hovered() {
        ctx.data_mut(|d| d.remove::<(f64, u64)>(key));
        return false;
    }
    let pass = ctx.cumulative_pass_nr();
    let now = ctx.input(|i| i.time);
    let (mut began, seen) = ctx.data(|d| d.get_temp::<(f64, u64)>(key)).unwrap_or((now, 0));
    // A drop-down that closed and opened again under the pointer starts the delay afresh.
    if seen + 1 < pass {
        began = now;
    }
    ctx.data_mut(|d| d.insert_temp(key, (began, pass)));
    let delay = f64::from(ui.style().interaction.tooltip_delay);
    if now - began < delay {
        ctx.request_repaint_after_secs((delay - (now - began)) as f32);
        return false;
    }
    true
}

/// Show `add` as `resp`'s tip beside the drop-down rather than under the pointer: to the
/// right of the menu, or to its left when the right has no room, so the tip never covers
/// the menu's rows (#10481). Returns the tip's rect when it was shown.
fn dropdown_tip(ui: &egui::Ui, menu: Rect, resp: &egui::Response, add: impl FnOnce(&mut egui::Ui)) -> Option<Rect> {
    if !dropdown_tip_ready(ui, resp) {
        return None;
    }
    let ctx = ui.ctx();
    let tip_id = resp.id.with("sc_dropdown_tip_area");
    // Last pass's size; the first pass assumes the tip fits on the right.
    let size = ctx.read_response(tip_id).map(|r| r.rect.size()).unwrap_or(egui::Vec2::ZERO);
    let screen = ctx.content_rect();
    let gap = 6.0;
    let x = if menu.right() + gap + size.x <= screen.right() { menu.right() + gap } else { (menu.left() - gap - size.x).max(screen.left()) };
    let y = resp.rect.top().min((screen.bottom() - size.y).max(screen.top()));
    let area = Container::new(ctx, Surface::Popup)
        .area(tip_id)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(pos2(x, y))
        .show(ctx, |ui| Container::new(ctx, Surface::Popup).show_frame(ui, Container::menu_frame(), ui.max_rect(), add));
    Some(area.inner.response.rect)
}

#[cfg(test)]
mod dropdown_tip_tests {
    use super::*;
    use egui::{Event, RawInput};

    /// One pass of a fake two-row drop-down at `menu`; returns the tips shown this pass.
    fn tip_pass(ctx: &egui::Context, menu: Rect, time: f64, at: egui::Pos2) -> Vec<(usize, Rect)> {
        let raw = RawInput { events: vec![Event::PointerMoved(at)], time: Some(time), ..Default::default() };
        let mut shown: Vec<(usize, Rect)> = Vec::new();
        crate::test_support::frame(ctx, raw, |ui| {
            let ctx = ui.ctx();
            egui::Area::new(egui::Id::unique("test_dropdown")).fixed_pos(menu.min).show(ctx, |ui| {
                for row in 0..2 {
                    let rect = Rect::from_min_size(menu.min + vec2(0.0, row as f32 * 32.0), vec2(menu.width(), 32.0));
                    let resp = ui.interact(rect, egui::Id::unique(("test_row", row)), Sense::hover());
                    let tip = if row == 0 {
                        dropdown_tip(ui, menu, &resp, |ui| {
                            ui.label("first row");
                        })
                    } else {
                        dropdown_tip(ui, menu, &resp, |ui| {
                            ui.label("second row");
                        })
                    };
                    if let Some(rect) = tip {
                        shown.push((row, rect));
                    }
                }
            });
        });
        shown
    }

    /// egui registers hover a pass or two after the pointer arrives (measured), while leaving
    /// a row drops hover the same pass. Timings below keep a margin around that lag.
    #[test]
    fn each_newly_hovered_row_waits_the_full_tip_delay() {
        let ctx = crate::fonts::test_context();
        let menu = Rect::from_min_size(pos2(20.0, 20.0), vec2(220.0, 64.0));
        let a = menu.min + vec2(10.0, 16.0);
        let b = menu.min + vec2(10.0, 48.0);
        let mut tips = Vec::new();
        for (time, at, expect) in [
            (0.0, a, None),
            (0.1, a, None),
            (0.3, a, None), // hover registers around here; the first row's delay starts
            (0.5, a, None), // still inside the delay
            (0.9, a, Some(0)),
            (1.0, b, None),    // the first row lost hover the moment the pointer left it
            (1.2, b, None),    // the second row must not inherit the open tip's grace (#10481)
            (1.4, b, None),    // its own delay cannot have elapsed yet, whenever hover registered
            (2.2, b, Some(1)), // after its own full delay, with margin for late registration
        ] {
            let got = tip_pass(&ctx, menu, time, at);
            match expect {
                Some(row) => {
                    assert_eq!(got.len(), 1, "exactly row {row}'s tip at t={time}, got {got:?}");
                    assert_eq!(got[0].0, row, "wrong row's tip at t={time}");
                    let tip = got[0].1;
                    assert!(tip.min.x >= menu.max.x, "the tip sits beside the menu, never over its rows: {tip:?} vs {menu:?}");
                    tips.push((row, tip));
                }
                None => assert!(got.is_empty(), "no tip expected at t={time}, got {got:?}"),
            }
        }
        assert_eq!(tips.len(), 2, "both rows showed a tip after their own delay");
    }
}

/// One command row of a panel drop-down: icon, label and shortcut, with the hover background
/// under the pointer or keyboard focus (like every menu).
/// `revealed` is the pulse strength when the command search points this row out (#7287).
fn dropdown_row(
    app: &SketApp,
    ui: &mut egui::Ui,
    c: &'static sketcraft_engine::CommandSpec,
    width: f32,
    pinned: bool,
    revealed: Option<f32>,
) -> RowAction {
    let t = Tokens::get();
    let info = crate::render_workspace::interactive_command_info(app, c);
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
    // Every row drags onto the toolbar (to pin it there); enabled rows also start their command.
    let sense = if info.enabled { Sense::click_and_drag() } else { Sense::drag() };
    let resp = ui.interact(rect, dropdown_row_id(c.id), sense);
    // Focus can settle on the following frame, after the key event has gone.
    // Keep the focused row visible even then in a drop-down taller than the window.
    if resp.has_focus() && !ui.clip_rect().contains_rect(rect) {
        resp.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
    if resp.drag_started() {
        resp.dnd_set_drag_payload(ToolbarDrag { id: c.id, from_menu: true });
    }
    // The pin at the row's end (above the row, so it takes its own clicks).
    let pr = Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), vec2(20.0, 20.0));
    let pin = ui.interact(pr, ui.scope_id().with(("dropdown_pin", c.id)), Sense::click());
    let pointer_on = (resp.hovered() || pin.hovered()) && !egui::DragAndDrop::has_any_payload(ui.ctx());
    let label = app.tr_label(info.label).to_string();
    let mut checked = None;
    if persistent_available(app, c.id) && app.preferences.persistent_ribbon_pressed {
        checked = Some(persistent_pressed(app, c.id));
    }
    if c.id == "view.shots_strip" {
        checked = Some(app.preferences.render_shots_strip);
    }
    // A checkable row announces its on/off state, since its label no longer says it.
    resp.widget_info(|| match checked {
        Some(on) => egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled() && info.enabled, on, &label),
        None => egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled() && info.enabled, &label),
    });
    crate::scenario::publish_handle(&format!("toolbar:{}", c.id), rect.center());
    crate::scenario::publish_handle(&format!("dropdown:{}", c.id), rect.center());
    let hot = info.enabled && (pointer_on || resp.has_focus());
    let mut row = crate::ui_kit::MenuItem::new(c.id, &label, info.icon);
    row.enabled = info.enabled;
    row.checked = checked;
    row.highlighted = hot || revealed.is_some();
    let shortcut = crate::keymap::display_for(app, c.id).unwrap_or_default();
    row.shortcut = &shortcut;
    row.paint(ui, rect, &resp, 26.0);
    if let Some(strength) = revealed {
        paint_reveal(ui.painter(), rect, &crate::theme::TabAccent::for_tab(&t, &app.ui.tab), strength, false);
        crate::scenario::publish_rect("ribbon:reveal", rect);
        if !ui.clip_rect().contains_rect(rect) {
            ui.scroll_to_rect(rect, None);
        }
        reveal_tooltip(app, ui, rect, c, egui::RectAlign::RIGHT_START);
    }
    if hot {
        crate::scenario::publish_handle(&format!("dropdown_hot:{}", c.id), rect.center());
    }
    let p = ui.painter();
    if pointer_on || resp.has_focus() {
        if pin.hovered() {
            p.rect_filled(pr, t.radius_sm.saturating_sub(1), t.border.gamma_multiply(0.6));
        }
        pin_icon(p, pr, pinned, if pinned { t.accent } else { t.text_dim });
        crate::scenario::publish_handle(&format!("dropdown_pin:{}", c.id), pr.center());
    }
    let tip = if pinned { "Remove from Toolbar" } else { "Pin to Toolbar" };
    // The drop-down's rect from the last pass: tips sit beside it, never over its rows (#10481).
    let menu = egui::containers::AreaState::load(ui.ctx(), ui.layer_id().id).map(|state| state.rect()).unwrap_or(rect);
    if pin.clicked() {
        return RowAction::TogglePin;
    }
    dropdown_tip(ui, menu, &pin, |ui| {
        ui.label(crate::i18n::tr_label(tip));
    });
    // A disabled row says why; a documented command also shows its help (#2758).
    let reason = info.disabled_reason.as_deref().filter(|_| !info.enabled).map(|reason| app.tr_label(reason));
    if let Some(hint) = persistent_hint(app, c, info.enabled) {
        dropdown_tip(ui, menu, &resp, move |ui| {
            ui.label(hint);
        });
    } else if let Some(tip) = crate::command_help::row_tip(c.id, &label, Some(shortcut.as_str()).filter(|k| !k.is_empty()), reason) {
        let help = crate::command_help::help(c.id);
        dropdown_tip(ui, menu, &resp, move |ui| {
            ui.set_max_width(300.0);
            ui.label(tip);
            if let Some(help) = help {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(help).weak());
            }
        });
    }
    if resp.secondary_clicked() {
        return RowAction::Menu(resp.interact_pointer_pos().unwrap_or(rect.left_bottom()));
    }
    if resp.clicked() && info.enabled {
        return RowAction::Start(rect);
    }
    RowAction::None
}

/// Take up what was done in a grid drop-down, as for a list row: start the command (and close
/// the drop-down), open its menu, or pin or unpin it.
fn grid_action(
    app: &mut SketApp,
    ui: &egui::Ui,
    action: Option<(&'static str, RowAction)>,
    menu: &mut Option<(egui::Pos2, &'static str)>,
    toggle: &mut Option<&'static str>,
) {
    match action {
        Some((id, RowAction::Menu(at))) => *menu = Some((at, id)),
        Some((id, RowAction::TogglePin)) => *toggle = Some(id),
        Some((id, RowAction::Start(rect))) => {
            app.start_at(id, Some(rect));
            ui.ctx().data_mut(|d| d.remove::<egui::Id>(egui::Id::unique("sc_panel_open")));
        }
        Some((_, RowAction::None)) | None => {}
    }
}

/// Sketch › Create in the grid: each family of variants under its own heading (Line, Rectangle,
/// Circle…), then the other tools under "More". The list keeps its family sub-menus.
fn sketch_create_grouped(cmds: &[&'static sketcraft_engine::CommandSpec]) -> Vec<(Option<&'static str>, &'static sketcraft_engine::CommandSpec)> {
    let mut out = Vec::new();
    for f in crate::sketch_families::FAMILIES {
        let mut heading = Some(f.label);
        for id in f.commands {
            if let Some(c) = cmds.iter().find(|c| c.id == *id) {
                out.push((heading.take(), *c));
            }
        }
    }
    let mut heading = Some("More");
    for c in cmds.iter().filter(|c| crate::sketch_families::family(c.id).is_none()) {
        out.push((heading.take(), *c));
    }
    out
}

/// A drop-down's commands without those the ribbon shows as buttons (`ribbon_menus_skip_pinned`),
/// unless that leaves none.
fn skip_on_ribbon(cmds: Vec<&'static sketcraft_engine::CommandSpec>, on_ribbon: &[&str]) -> Vec<&'static sketcraft_engine::CommandSpec> {
    let rest: Vec<_> = cmds.iter().copied().filter(|c| !on_ribbon.contains(&c.id)).collect();
    if rest.is_empty() { cmds } else { rest }
}

/// A grid drop-down's cell (#4739): the icon twice the size of a list row's, with room around it.
const GRID_CELL: f32 = 48.0;
const GRID_ICON: f32 = 32.0;
/// Columns a grid fills before a second row; a panel with fewer commands is as wide as it needs.
const GRID_COLUMNS: usize = 5;
/// Rows at most: a bigger menu grows to the right instead (unless the window is too narrow).
const GRID_ROWS: usize = 3;

/// The height of a group's heading in a grid drop-down.
const GRID_HEADING: f32 = 24.0;
/// Icons side by side in a group's column, when groups stand side by side.
const GROUP_COLUMNS: usize = 2;

/// Where a grid drop-down puts things, relative to its top left: each command's icon with its
/// (row, column) for the arrow keys, each group heading (where, how wide), the separators
/// between groups side by side (their x), and the whole size.
struct GridLayout {
    cells: Vec<(Rect, (usize, usize))>,
    headings: Vec<(egui::Pos2, f32, &'static str)>,
    separators: Vec<f32>,
    size: egui::Vec2,
}

/// Lay out a grid drop-down, at most [`GRID_ROWS`] high, growing to the right. A menu of two or
/// more headed groups (Sketch › Create's families, Model › Modify's Edges, Faces…) puts the
/// groups side by side, each heading over a column of its icons, [`GROUP_COLUMNS`] across (more
/// for a group too big for three rows), when that fits in `room` (wide). Any other menu, or one
/// too wide for the window, fills rows: [`GRID_COLUMNS`] across, more when that would take over
/// three rows, fewer when the window is narrower; a heading starts a new row. `loose` names the
/// commands in no group (Model › Modify's Delete): they stand apart after the groups, without a
/// heading, so they stay put when a group is left out (`ribbon_menus_skip_pinned`).
fn grid_layout(grouped: &[(Option<&'static str>, &'static sketcraft_engine::CommandSpec)], loose: &[&str], room: f32) -> GridLayout {
    let mut groups: Vec<(Option<&'static str>, Vec<usize>)> = Vec::new();
    for (i, (heading, c)) in grouped.iter().enumerate() {
        let after_a_group = groups.last().is_some_and(|(h, _)| h.is_some());
        if heading.is_some() || groups.is_empty() || (after_a_group && loose.contains(&c.id)) {
            groups.push((*heading, Vec::new()));
        }
        if let Some((_, items)) = groups.last_mut() {
            items.push(i);
        }
    }
    // A heading needs two icons' width; loose commands only as many columns as they fill.
    let group_columns = |heading: Option<&str>, n: usize| n.div_ceil(GRID_ROWS).max(if heading.is_some() { GROUP_COLUMNS } else { 1 });
    let headed = groups.iter().filter(|(h, _)| h.is_some()).count();
    let side_columns: usize = groups.iter().map(|(h, items)| group_columns(*h, items.len())).sum();
    let side_by_side = headed >= 2 && side_columns as f32 * GRID_CELL <= room;
    let mut cells = vec![(Rect::NOTHING, (0, 0)); grouped.len()];
    let mut headings = Vec::new();
    let mut separators = Vec::new();
    let cell = |x: f32, y: f32| Rect::from_min_size(pos2(x, y), vec2(GRID_CELL, GRID_CELL));
    let size = if side_by_side {
        let (mut first_column, mut rows) = (0, 0);
        for (g, (heading, items)) in groups.iter().enumerate() {
            let columns = group_columns(*heading, items.len());
            let x = first_column as f32 * GRID_CELL;
            if g > 0 {
                separators.push(x);
            }
            if let Some(h) = heading {
                headings.push((pos2(x, 0.0), columns as f32 * GRID_CELL, *h));
            }
            for (k, i) in items.iter().enumerate() {
                let (row, column) = (k / columns, k % columns);
                if let Some(slot) = cells.get_mut(*i) {
                    *slot = (cell(x + column as f32 * GRID_CELL, GRID_HEADING + row as f32 * GRID_CELL), (row, first_column + column));
                }
            }
            rows = rows.max(items.len().div_ceil(columns));
            first_column += columns;
        }
        vec2(first_column as f32 * GRID_CELL, GRID_HEADING + rows as f32 * GRID_CELL)
    } else {
        let n = grouped.len();
        let fit = ((room / GRID_CELL).floor() as usize).max(1);
        let columns = n.div_ceil(GRID_ROWS).max(n.min(GRID_COLUMNS)).min(fit).max(1);
        let (mut y, mut first_row) = (0.0, 0);
        for (heading, items) in &groups {
            if let Some(h) = heading {
                headings.push((pos2(0.0, y), columns as f32 * GRID_CELL, *h));
                y += GRID_HEADING;
            }
            for (k, i) in items.iter().enumerate() {
                let (row, column) = (k / columns, k % columns);
                if let Some(slot) = cells.get_mut(*i) {
                    *slot = (cell(column as f32 * GRID_CELL, y + row as f32 * GRID_CELL), (first_row + row, column));
                }
            }
            let rows = items.len().div_ceil(columns);
            y += rows as f32 * GRID_CELL;
            first_row += rows;
        }
        vec2(columns as f32 * GRID_CELL, y)
    };
    GridLayout { cells, headings, separators, size }
}

/// An arrow key in a grid drop-down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GridKey {
    Left,
    Right,
    Up,
    Down,
}

/// Where an arrow key moves the focus in a grid drop-down, as an index into `enabled` (the
/// enabled commands, as indices into `cells`): Left and Right step through them in order, Up and
/// Down to the nearest one in the row above or below, all wrapping. From none focused, Right
/// and Down go to the first, Left and Up to the last.
fn grid_step(cells: &[(usize, usize)], enabled: &[usize], at: Option<usize>, key: GridKey) -> Option<usize> {
    let n = enabled.len();
    if n == 0 {
        return None;
    }
    let Some(at) = at.filter(|at| *at < n) else {
        return Some(if matches!(key, GridKey::Right | GridKey::Down) { 0 } else { n - 1 });
    };
    let cell = |k: usize| enabled.get(k).and_then(|i| cells.get(*i)).copied();
    match key {
        GridKey::Right => Some((at + 1) % n),
        GridKey::Left => Some((at + n - 1) % n),
        GridKey::Up | GridKey::Down => {
            let (row, column) = cell(at)?;
            let rows: Vec<usize> = (0..n).filter_map(|k| cell(k).map(|c| c.0)).collect();
            let target = if key == GridKey::Down {
                rows.iter().copied().filter(|r| *r > row).min().or_else(|| rows.iter().copied().min())
            } else {
                rows.iter().copied().filter(|r| *r < row).max().or_else(|| rows.iter().copied().max())
            }?;
            (0..n)
                .filter_map(|k| cell(k).filter(|c| c.0 == target).map(|c| (k, c.1.abs_diff(column))))
                .min_by_key(|(_, distance)| *distance)
                .map(|(k, _)| k)
        }
    }
}

/// Arrow keys move the keyboard focus through a grid drop-down's enabled icons ([`grid_step`]).
fn grid_keys(app: &SketApp, ui: &egui::Ui, cmds: &[&'static sketcraft_engine::CommandSpec], cells: &[(usize, usize)]) {
    let key = ui.input(|i| {
        [
            (egui::Key::ArrowLeft, GridKey::Left),
            (egui::Key::ArrowRight, GridKey::Right),
            (egui::Key::ArrowUp, GridKey::Up),
            (egui::Key::ArrowDown, GridKey::Down),
        ]
        .into_iter()
        .find(|(k, _)| i.key_pressed(*k))
        .map(|(_, key)| key)
    });
    let Some(key) = key else { return };
    let enabled: Vec<usize> = cmds.iter().enumerate().filter(|(_, c)| c.info(&app.session).enabled).map(|(i, _)| i).collect();
    let focused = ui.ctx().memory(|m| m.focused());
    let at = enabled.iter().position(|i| cmds.get(*i).is_some_and(|c| focused == Some(dropdown_row_id(c.id))));
    if let Some(c) = grid_step(cells, &enabled, at, key).and_then(|k| enabled.get(k)).and_then(|i| cmds.get(*i)) {
        ui.ctx().memory_mut(|m| {
            m.request_focus(dropdown_row_id(c.id));
            m.move_focus(egui::FocusDirection::None);
        });
    }
}

/// A panel drop-down drawn as a grid of large icons (the `ribbon_icon_grid` preference, #4739):
/// the same commands, headings and actions as the list, the names in the tooltips. Returns the
/// command acted on, and how.
fn dropdown_grid(
    app: &SketApp,
    ui: &mut egui::Ui,
    tab: &str,
    grouped: &[(Option<&'static str>, &'static sketcraft_engine::CommandSpec)],
    loose: &[&str],
    paints: &mut Vec<PersistentRibbonPaint>,
) -> Option<(&'static str, RowAction)> {
    // Wide enough for groups side by side, short of the window's edge.
    let room = ui.ctx().content_rect().right() - ui.cursor().left() - 24.0;
    let layout = grid_layout(grouped, loose, room);
    let width = layout.size.x.max(200.0);
    // A narrow grid sits in the middle of the drop-down's least width.
    let inset = ((width - layout.size.x) / 2.0).max(0.0);
    ui.set_width(width);
    let cmds: Vec<_> = grouped.iter().map(|(_, c)| *c).collect();
    let keys: Vec<(usize, usize)> = layout.cells.iter().map(|(_, at)| *at).collect();
    grid_keys(app, ui, &cmds, &keys);
    // Like the list, the grid scrolls when it would run past the window's bottom (#874).
    let height = (ui.ctx().content_rect().bottom() - ui.cursor().top() - 64.0).max(GRID_CELL * 2.0);
    let mut action = None;
    egui::ScrollArea::vertical().id_salt("panel_menu_grid").max_height(height).min_scrolled_height(height).show(ui, |ui| {
        let (area, _) = ui.allocate_exact_size(vec2(width, layout.size.y), Sense::hover());
        let origin = area.left_top() + vec2(inset, 0.0);
        let t = Tokens::get();
        for (at, w, heading) in &layout.headings {
            let text = group_heading_text(heading);
            let clip = ui.clip_rect().intersect(Rect::from_min_size(origin + at.to_vec2(), vec2(*w, GRID_HEADING)));
            let painter = ui.painter().with_clip_rect(clip);
            // Centred over its group's icons.
            painter.text(origin + at.to_vec2() + vec2(w / 2.0, GRID_HEADING / 2.0), Align2::CENTER_CENTER, text, group_heading_font(), t.text_dim);
        }
        // A line between groups side by side, as between the ribbon's sections.
        for x in &layout.separators {
            let x = origin.x + x;
            ui.painter().line_segment([pos2(x, origin.y + 4.0), pos2(x, origin.y + layout.size.y - 4.0)], Stroke::new(1.0, t.border));
        }
        for ((_, c), (rect, _)) in grouped.iter().zip(&layout.cells) {
            let rect = rect.translate(origin.to_vec2());
            let pinned = crate::toolbar_custom::pinned_on(app, tab, c.id).is_some();
            match dropdown_cell(app, ui, c, rect, pinned, paints) {
                RowAction::None => {}
                done => action = Some((c.id, done)),
            }
        }
    });
    action
}

/// How long the pointer rests on a grid icon before its corner pin can be clicked (seconds).
const PIN_DELAY: f64 = 2.0;

/// Whether the corner pin of grid icon `id` takes clicks: the pointer (`on`) has rested on the
/// icon for [`PIN_DELAY`]. One icon is timed at a time; the window redraws when the pin is due.
fn pin_armed(ui: &egui::Ui, id: &'static str, on: bool) -> bool {
    let key = egui::Id::unique("sc_grid_pin_hover");
    let now = ui.input(|i| i.time);
    let since = ui.ctx().data_mut(|d| {
        let rest = d.get_temp::<(&'static str, f64)>(key).filter(|(at, _)| *at == id);
        match (on, rest) {
            (true, Some((_, since))) => Some(since),
            (true, None) => {
                d.insert_temp(key, (id, now));
                Some(now)
            }
            (false, Some(_)) => {
                d.remove::<(&'static str, f64)>(key);
                None
            }
            (false, None) => None,
        }
    });
    let Some(since) = since else { return false };
    let left = PIN_DELAY - (now - since);
    if left > 0.0 {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left));
    }
    left <= 0.0
}

/// One icon of a grid drop-down: clicked, dragged onto the ribbon (to pin it) or right-clicked
/// like a list row; its tooltip is a ribbon button's (name, shortcut, help, why it is off), and a
/// pin in its corner says it is on the ribbon.
fn dropdown_cell(
    app: &SketApp,
    ui: &mut egui::Ui,
    c: &'static sketcraft_engine::CommandSpec,
    rect: Rect,
    pinned: bool,
    paints: &mut Vec<PersistentRibbonPaint>,
) -> RowAction {
    let t = Tokens::get();
    let info = c.info(&app.session);
    let sense = if info.enabled { Sense::click_and_drag() } else { Sense::drag() };
    let resp = ui.interact(rect, dropdown_row_id(c.id), sense);
    if resp.has_focus() && !ui.clip_rect().contains_rect(rect) {
        resp.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
    if resp.drag_started() {
        resp.dnd_set_drag_payload(ToolbarDrag { id: c.id, from_menu: true });
    }
    // The pin in the corner, as at a list row's end. It takes clicks (above the cell) only once
    // the pointer has rested on the icon for PIN_DELAY: until then a click anywhere starts the
    // command, so a quick click near the corner never pins or unpins it by accident.
    let pr = Rect::from_min_size(pos2(rect.right() - 15.0, rect.top()), vec2(15.0, 15.0));
    let dragging = egui::DragAndDrop::has_any_payload(ui.ctx());
    let armed = pin_armed(ui, c.id, ui.rect_contains_pointer(rect) && !dragging);
    let pin = armed.then(|| ui.interact(pr, ui.scope_id().with(("dropdown_pin", c.id)), Sense::click()));
    let pin_hovered = pin.as_ref().is_some_and(egui::Response::hovered);
    let persistent = app.preferences.persistent_ribbon_pressed && persistent_available(app, c.id);
    if !persistent {
        let label = app.tr_label(c.label).to_string();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled() && info.enabled, &label));
    }
    crate::scenario::publish_handle(&format!("toolbar:{}", c.id), rect.center());
    crate::scenario::publish_handle(&format!("dropdown:{}", c.id), rect.center());
    let pointer_on = (resp.hovered() || pin_hovered) && !dragging;
    let hot = info.enabled && (pointer_on || resp.has_focus());
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    let cell = rect.shrink(2.0);
    let mut neutral = Vec::new();
    if hot {
        neutral.push(egui::Shape::rect_filled(cell, t.control_radius, t.accent_soft));
        crate::scenario::publish_handle(&format!("dropdown_hot:{}", c.id), rect.center());
    }
    if resp.has_focus() && !persistent {
        neutral.push(egui::Shape::rect_stroke(cell, t.control_radius, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside));
    }
    let neutral = egui::Shape::Vec(neutral);
    if persistent {
        // Reuse the native ribbon recipe; only responses/paint slots survive until dispatch.
        paints.push(PersistentRibbonPaint {
            command: c,
            response: resp.clone(),
            painter: painter.clone(),
            rect: cell,
            background: painter.add(egui::Shape::Noop),
            label_slot: painter.add(egui::Shape::Noop),
            neutral,
            label: None,
            enabled: ui.is_enabled() && info.enabled,
            disabled: info.disabled_reason.clone(),
        });
    } else {
        painter.add(neutral);
    }
    icons::paint_icon(&painter, Rect::from_center_size(rect.center(), vec2(GRID_ICON, GRID_ICON)), c.icon, if info.enabled { 1.0 } else { 0.4 });
    // A pinned command always shows its pin, clear of the icon's own artwork; any other shows one
    // to pin it once it can be clicked.
    if pinned || armed {
        if pin_hovered {
            painter.rect_filled(pr, 3.0, t.border.gamma_multiply(0.6));
        }
        pin_icon(&painter, pr.shrink(2.5), pinned, if pinned { t.accent } else { t.text_dim });
    }
    if let Some(pin) = pin {
        crate::scenario::publish_handle(&format!("dropdown_pin:{}", c.id), pr.center());
        let tip = if pinned { "Remove from Toolbar" } else { "Pin to Toolbar" };
        if pin.backing_on_hover_text(crate::i18n::tr_label(tip)).clicked() {
            return RowAction::TogglePin;
        }
    }
    let resp = if persistent { resp } else { ribbon_tooltip(app, resp, c, info.disabled_reason.as_deref()) };
    if resp.secondary_clicked() {
        return RowAction::Menu(resp.interact_pointer_pos().unwrap_or(rect.left_bottom()));
    }
    if resp.clicked() && info.enabled {
        return RowAction::Start(rect);
    }
    RowAction::None
}

/// Creation menus keep related variants together, including collapsed and overflow panels.
/// The plain rows are `width` wide, the width the menu gives its family rows.
fn sketch_create_menu(
    app: &mut SketApp,
    ui: &mut egui::Ui,
    cmds: &[&'static sketcraft_engine::CommandSpec],
    width: f32,
    menu: &mut Option<(egui::Pos2, &'static str)>,
    toggle: &mut Option<&'static str>,
) {
    let room = (ui.ctx().content_rect().bottom() - ui.cursor().top() - 64.0).max(ROW_H * 3.0);
    egui::ScrollArea::vertical().id_salt("sketch_create_rows").max_height(room).show(ui, |ui| {
        for f in crate::sketch_families::FAMILIES {
            let variants: Vec<_> = f.commands.iter().filter_map(|id| cmds.iter().find(|c| c.id == *id).copied()).collect();
            if let Some(first) = variants.first() {
                // The row translates its own label.
                let mut item = crate::ui_kit::MenuItem::new(f.label, f.label, first.icon);
                item.submenu = true;
                let response = crate::ui_kit::Menu::item_button(ui, &item, |ui| {
                    dropdown_keys(app, ui, &variants);
                    let variants_w = dropdown_width(app, ui, &variants);
                    sketch_menu_rows(app, ui, &variants, variants_w, menu, toggle);
                });
                crate::scenario::publish_handle(&format!("create_family:{}", f.label), response.response.rect.center());
            }
        }
        ui.separator();
        let rest: Vec<_> = cmds.iter().copied().filter(|c| crate::sketch_families::family(c.id).is_none()).collect();
        dropdown_keys(app, ui, &rest);
        sketch_menu_rows(app, ui, &rest, width, menu, toggle);
    });
}

/// Family popups use the same ordered arrow-key navigation as a panel's command list.
fn dropdown_keys(app: &SketApp, ui: &egui::Ui, cmds: &[&'static sketcraft_engine::CommandSpec]) {
    let (down, up) = ui.input(|i| (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp)));
    if !down && !up {
        return;
    }
    // A submenu renders before the remaining rows of its parent. Handle a key once, but
    // leave the event available for the focused row's scroll-into-view behaviour.
    let handled = egui::Id::unique("sketch_menu_key_pass");
    let pass = ui.ctx().cumulative_pass_nr();
    if ui.ctx().data(|d| d.get_temp::<u64>(handled)) == Some(pass) {
        return;
    }
    ui.ctx().data_mut(|d| d.insert_temp(handled, pass));
    let enabled: Vec<_> = cmds.iter().filter(|c| crate::render_workspace::interactive_command_info(app, c).enabled).collect();
    if enabled.is_empty() {
        return;
    }
    let focused = ui.ctx().memory(|m| m.focused());
    let at = enabled.iter().position(|c| focused == Some(dropdown_row_id(c.id)));
    let n = enabled.len();
    let next = match (at, down) {
        (None, true) => 0,
        (None, false) => n - 1,
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
    };
    if let Some(c) = enabled.get(next) {
        ui.ctx().memory_mut(|m| {
            m.request_focus(dropdown_row_id(c.id));
            m.move_focus(egui::FocusDirection::None);
        });
    }
}

fn sketch_menu_rows(
    app: &mut SketApp,
    ui: &mut egui::Ui,
    cmds: &[&'static sketcraft_engine::CommandSpec],
    width: f32,
    menu: &mut Option<(egui::Pos2, &'static str)>,
    toggle: &mut Option<&'static str>,
) {
    for c in cmds {
        let pinned = crate::toolbar_custom::pinned_on(app, "SKETCH", c.id).is_some();
        // A hovered search result in a sketch creation menu is marked steadily (#7287).
        let revealed = app.sbox.reveal.as_ref().is_some_and(|r| r.preview && r.id == c.id);
        match dropdown_row(app, ui, c, width, pinned, revealed.then_some(1.0)) {
            RowAction::None => {}
            RowAction::Menu(at) => *menu = Some((at, c.id)),
            RowAction::TogglePin => *toggle = Some(c.id),
            RowAction::Start(rect) => {
                app.start_at(c.id, Some(rect));
                ui.ctx().data_mut(|d| d.remove::<egui::Id>(egui::Id::unique("sc_panel_open")));
                ui.close();
            }
        }
    }
}

type ToolbarDrag = ui_kit::ReorderPayload<&'static str>;

/// The dragged command's icon under the pointer, crossed out when releasing removes it.
fn drag_ghost(app: &SketApp, ctx: &egui::Context, id: &str, at: egui::Pos2, removing: bool) {
    let t = Tokens::get();
    let Some(c) = sketcraft_engine::find_command(id) else { return };
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("sc_toolbar_drag")));
    let r = Rect::from_center_size(at + vec2(14.0, 16.0), vec2(30.0, 30.0));
    p.rect(r, t.radius_sm, t.toolbar.gamma_multiply(0.92), Stroke::new(1.0, if removing { t.error } else { t.accent }), egui::StrokeKind::Inside);
    let info = c.info(&app.session);
    icons::paint_icon(&p, r.shrink(5.0), info.icon, f32::from(t.icon.a()) / 255.0);
    if removing {
        let s = Stroke::new(2.0, t.error);
        let x = Rect::from_center_size(r.right_top(), vec2(9.0, 9.0));
        p.circle_filled(x.center(), 7.0, t.toolbar);
        crate::icons::glyphs::cross(&p, x, s);
    }
    crate::scenario::publish_handle(if removing { "toolbar_drag:remove" } else { "toolbar_drag" }, at);
}

/// The SELECT panel: what a click in the view picks.
pub(crate) fn selection_filter(app: &mut SketApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    ui.label(egui::RichText::new(crate::i18n::tr_label("Selection filter")).color(t.text_dim).size(t.font_small));
    for (mode, label) in [("body", app.tr_label("Select bodies (a face picks its body)")), ("component", app.tr("ui.select_components_by_face"))] {
        let mut on = app.ui.pick_mode() == mode;
        if ui_kit::checkbox(ui, label, &mut on).changed() {
            // The Faces checkbox below is independent. The standalone command enables faces.
            let filters = std::mem::take(&mut app.ui.pick_off);
            let _ = app.run("select.filter", json!({"mode": if on { mode } else { "face" }}));
            app.ui.pick_off = filters;
        }
    }
    for (k, label) in [
        ("faces", "Faces"),
        ("edges", "Edges"),
        ("vertices", "Vertices"),
        ("sketch", "Sketch geometry and profiles"),
        ("sketch_curves", "Sketch curves"),
        ("sketch_points", "Sketch points"),
        ("dimensions", "Sketch dimensions"),
        ("constraints", "Sketch constraints"),
    ] {
        let mut on = !app.ui.pick_off.iter().any(|x| x == k);
        if crate::ui_kit::checkbox(ui, crate::i18n::tr_label(label), &mut on).changed() {
            app.ui.pick_off.retain(|x| x != k);
            if !on {
                app.ui.pick_off.push(k.to_string());
            }
        }
    }
    // The selection shape (#4376): a drag draws a freehand lasso instead of the rectangle.
    let label = app.tr("ui.lasso_select").to_string();
    ui_kit::checkbox(ui, &label, &mut app.ui.lasso_select);
}

/// A click on the lit button of the sketch tool that is running ends the tool, as a toggle does,
/// so clicks in the view select again (#10151). With Preferences › Sketch "Clicking the running
/// tool's button ends it" off, the click starts the tool again, as it did before. Returns whether
/// the tool was ended. A persistent analysis button already turns its analysis off its own way.
fn end_running_tool(app: &mut SketApp, id: &str) -> bool {
    if !app.preferences.ribbon_click_ends_tool || persistent_available(app, id) || !app.tool.as_ref().is_some_and(|t| t.cmd == id) {
        return false;
    }
    crate::dim_view::cancel();
    app.tool = None;
    true
}

/// Two Esc presses within this many seconds finish the open sketch.
pub const ESCAPE_TWICE_S: f64 = 0.4;

/// Whether `cancel_unfinished` has something to cancel.
pub(crate) fn unfinished(app: &SketApp) -> bool {
    app.tool.is_some()
        || crate::select_other::is_open(app)
        || app.menu.joint_move_warning.is_some()
        || app.viewport.joint_handles.is_some()
        || app.viewport.text_drag.is_some()
        || crate::occurrence_drag::active(app)
}

/// Cancels what one Esc cancels before it reaches the dialog, the selection or the sketch:
/// a drag, the Select Other list, or the active tool's points and then the tool. The view's
/// corner Cancel (#6709) takes this step first too. Returns whether anything was cancelled.
pub(crate) fn cancel_unfinished(app: &mut SketApp) -> bool {
    if crate::joint_handles::cancel(app) || crate::sketch_text_move::cancel_drag(app) || crate::occurrence_drag::cancel(app) {
        return true;
    }
    if crate::select_other::is_open(app) {
        crate::select_other::close(app);
    } else if app.tool.is_some() {
        crate::tools::finish(app);
    } else {
        return false;
    }
    true
}

/// Global keyboard shortcuts (when no text field has focus).
pub fn shortcuts(app: &mut SketApp, ctx: &egui::Context) {
    crate::viewport_status::reconcile(app, ctx);
    crate::browser::update_folder_pick(app, ctx);
    let chat_owner = crate::agent_chat::owns_keyboard_input(app, ctx);
    if design_shown(app) && !chat_owner {
        crate::navigation::step_key_hold(app, ctx);
    } else {
        app.viewport.key_hold = None;
    }
    if app.file_load.busy() {
        return;
    }
    if crate::viewport_status::enabled(app)
        && (app.esc_handled || app.escape_owner.is_some())
        && ctx.input(|input| input.key_pressed(egui::Key::Escape))
    {
        return;
    }
    // File is a custom Area, so it must claim Escape before the underlying editors and
    // sketch shortcuts. Clear raw events too: egui may replay them on a layout pass.
    let file_menu_id = egui::Id::unique("sc_file_menu");
    if !app.native_file_menu
        && ctx.data(|d| d.get_temp::<bool>(file_menu_id).unwrap_or(false))
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        ctx.data_mut(|d| d.insert_temp(file_menu_id, false));
        egui::Popup::close_all(ctx);
        ctx.input_mut(|i| {
            i.raw.events.retain(|event| !matches!(event, egui::Event::Key { key: egui::Key::Escape, modifiers, .. } if modifiers.matches_logically(egui::Modifiers::NONE)));
        });
        app.esc_handled = true;
        app.last_escape = None;
        return;
    }
    // The Keyboard Shortcuts window is waiting for a key: it is not a shortcut.
    if crate::keymap::capturing(app) {
        // Parameters is drawn before Preferences; do not let its footer see this Escape.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            crate::keymap::stop_capture(app);
            app.esc_handled = true;
        }
        return;
    }
    // These owners render after global shortcuts. Leave their Escape available to the
    // normal window/palette handler, but retain ownership if egui repeats the layout.
    if ctx.input(|input| input.key_pressed(egui::Key::Escape))
        && (app.prefs_window.open || app.keymap.open || app.ui.palette_open || ctx.memory(|memory| memory.top_modal_layer().is_some()))
    {
        app.escape_owner = Some(crate::EscapeOwner::Overlay);
        if crate::viewport_status::enabled(app) {
            app.last_escape = None;
            return;
        }
    }
    if crate::params_dialog::cancel_on_escape(app, ctx) {
        return;
    }
    // A dimension value that is taking the keyboard: letters belong to it (a parameter name
    // starting with L or D), even on the frame before its field holds the focus.
    let popup_owner = egui::Popup::is_any_open(ctx)
        || crate::dialogs_save_mesh::popup_open(ctx)
        || crate::hole_presets::name_prompt_open(ctx, app.dialog.as_ref())
        || crate::personal_sheet_rules::name_prompt_open(ctx, app.dialog.as_ref());
    let inline_editor = crate::dim_view::editing();
    if !popup_owner
        && app.viewport_status.open_key.is_some()
        && crate::ui_kit::details_field_owned_escape(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        app.esc_handled = true;
        app.escape_owner = Some(crate::EscapeOwner::Overlay);
        return;
    }
    if popup_owner
        || inline_editor
        || chat_owner
        || (crate::viewport_status::enabled(app) && app.ui.palette_open)
        || (ctx.egui_wants_keyboard_input() && !(crate::viewport_status::enabled(app) && app.viewport_status.owns_focus(ctx)))
    {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            let owner = if app.dialog.is_some() && !popup_owner && !inline_editor { crate::EscapeOwner::Dialog } else { crate::EscapeOwner::Overlay };
            // Reserve first refusal without claiming that cancellation already occurred.
            // A dialog's own late handler must still be able to cancel its focused field.
            app.escape_owner.get_or_insert(owner);
            if crate::viewport_status::enabled(app) {
                app.last_escape = None;
            }
        }
        return;
    }
    // A quick chord can press and release all modifiers in one frame. Dispatch each key
    // with its own modifiers, not the final frame state (an unbound Ctrl+Shift+A is not A).
    let keys: Vec<_> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(
                |e| if let egui::Event::Key { key, pressed: true, repeat, modifiers, .. } = e { Some((*key, *repeat, *modifiers)) } else { None },
            )
            .collect()
    });
    // A theme pasted on the Interface page is a theme, not copied components (#8226).
    crate::prefs::take_theme_paste(app, ctx);
    // Home hides the design: only New, Open and Preferences work there.
    let shown = design_shown(app);
    if shown {
        crate::edit_clipboard::handle(app, ctx);
    } else {
        crate::edit_clipboard::flush(app, ctx);
    }
    for (k, repeat, mods) in keys {
        use egui::Key;
        // Only Esc pressed twice in a row finishes a sketch: any other key in between resets it.
        if k != Key::Escape {
            app.last_escape = None;
        }
        if shown && (crate::keymap::drag_zoom_key(app, k, mods) || crate::navigation::view_key(app, ctx, k, mods, repeat)) {
            continue;
        }
        if mods.command {
            match k {
                // An editing embed's host has its own New, Open, Save, Close and settings.
                Key::N | Key::O | Key::S | Key::W | Key::Comma if app.embed_editor => {}
                Key::N => crate::documents::new_design(app),
                Key::Comma => crate::prefs::open_at(app, 0),
                Key::O => {
                    app.pick_open(|app, path| {
                        if let Some(path) = path {
                            app.open_path_async(&path);
                        }
                    });
                }
                _ if !shown => {}
                // Close the design shown (asking first when it is unsaved); closing the last one
                // shows the start page (#5366).
                Key::W if !mods.shift && !mods.alt => crate::documents::close(app, app.docs.active, false),
                Key::Z => {
                    if !crate::tools::undo_spline_point(app) {
                        drop(app.run("edit.undo", json!({})));
                    }
                }
                Key::Y => drop(app.run("edit.redo", json!({}))),
                // Copy, Cut and Paste, and Paste New with Shift (`edit_clipboard::handle`).
                Key::C | Key::X if !mods.shift && !mods.alt => {}
                Key::V if !mods.alt => {}
                // Ctrl+Shift+S is Save As; Ctrl+S keeps saving over the current file (#6558).
                Key::S if mods.shift && !mods.alt => save_as(app),
                Key::S => save(app),
                _ => crate::keymap::press(app, &crate::keymap::key_name(k, mods), repeat),
            }
            continue;
        }
        if !shown {
            continue;
        }
        match k {
            Key::Escape => {
                app.esc_handled = true;
                if crate::render_workspace::result_active(app) {
                    crate::keymap::press(app, "Escape", repeat);
                    continue;
                }
                // End navigation before the underlying tool or dialog; sketch double-Escape
                // handling starts with the next press.
                if app.viewport.nav.take().is_some() {
                    app.last_escape = None;
                    continue;
                }
                // A SketchUp drawing tool still waiting for its first click.
                if crate::sketchup_draw::cancel(app) {
                    app.last_escape = None;
                    continue;
                }
                if crate::render_workspace::assign::cancel(app) {
                    app.last_escape = None;
                    continue;
                }
                // Two quick presses in a sketch finish it (keeping it, like Finish Sketch);
                // the first one has already cancelled whatever was active.
                let now = ctx.input(|i| i.time);
                let again = app.last_escape.is_some_and(|t| now - t <= ESCAPE_TWICE_S);
                app.last_escape = Some(now);
                // Not while a tool is still out (its first Esc only ended a chain): this press
                // puts it down, and the next quick one finishes.
                if again
                    && !(crate::viewport_status::enabled(app) && (app.viewport_status.open_key.is_some() || crate::viewport_status::has_effect(app)))
                    && app.tool.is_none()
                    && app.dialog.is_none()
                    && let Some(sketch) = app.session.active_sketch
                {
                    app.last_escape = None;
                    // A sketch left blank is taken away instead, in one undo step, so no empty
                    // sketch stays behind.
                    let blank = app.session.doc.sketch(sketch).is_ok_and(crate::sketch_palette::is_blank);
                    let depth = app.session.undo.len();
                    app.finish_sketch();
                    if blank && app.session.active_sketch.is_none() && app.run("timeline.delete", json!({ "feature": sketch })).is_ok() {
                        app.session.undo.truncate(depth + 1);
                    }
                    continue;
                }
                if cancel_unfinished(app) {
                    continue;
                }
                if app.dialog.is_some() || app.pending_feature_edit.is_some() {
                    crate::dialogs::cancel(app);
                } else if crate::viewport_status::enabled(app) && crate::viewport_status::escape(app, repeat) {
                    app.last_escape = None;
                } else if !app.session.selection.is_empty() {
                    crate::edit_clipboard::drop_cut(app);
                    app.tree.picked_joints.clear();
                    let _ = app.run("select.clear", json!({}));
                } else if app.session.active_component != 0 {
                    let _ = app.run("component.activate", json!({ "component": 0 }));
                } else if app.session.active_sketch.is_none() {
                    crate::edit_clipboard::drop_cut(app);
                    app.tree.picked_joints.clear();
                    let _ = app.run("select.clear", json!({}));
                }
                // A single Esc with nothing left to cancel keeps the sketch open (#1804); a quick
                // second press finishes it (above).
            }
            Key::S if !mods.shift && !mods.alt && !mods.command && app.session.active_sketch.is_none() => {
                let bodies: Vec<String> = app
                    .session
                    .selection
                    .iter()
                    .filter_map(|s| match s {
                        sketcraft_engine::Sel::Body { name } => Some(name.clone()),
                        sketcraft_engine::Sel::Face { body, .. } => Some(body.clone()),
                        _ => None,
                    })
                    .collect();
                if !bodies.is_empty() {
                    app.start("solid.scale");
                } else {
                    app.start("ui.palette");
                }
            }
            Key::S if !mods.shift && !mods.alt => {
                app.start("ui.palette");
            }
            // With a menu open, V runs that menu's Show/Hide on what the menu is for.
            Key::V => {
                if !crate::context_menu::press_shortcut(app, "V") {
                    crate::context_menu::toggle_visibility(app);
                }
            }
            Key::F2 => {
                if !crate::context_menu::press_shortcut(app, "F2") {
                    let at = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(ctx.content_rect().center());
                    crate::context_menu::rename_selection(app, at);
                }
            }
            Key::F6 => app.animate_view("fit"),
            Key::Space if !mods.shift && !mods.alt && !mods.command => {
                if app.tool.is_some() {
                    crate::tools::finish(app);
                } else if app.dialog.is_some() {
                    crate::dialogs::cancel(app);
                } else {
                    let _ = app.run("select.clear", json!({}));
                }
            }
            Key::Backspace if crate::tools::undo_spline_point(app) => {}
            // L, R and C go through the keymap (`keymap::press`): their keys can be changed, a
            // second press cycles the tool's modes, and on the SketchUp tab the first click picks
            // the plane (`sketchup_draw`).
            Key::M if !mods.shift && !mods.alt && !mods.command && app.session.active_sketch.is_none() => {
                app.start("occurrence.move");
            }
            Key::D if !mods.shift && !mods.alt && !mods.command => {
                if app.session.active_sketch.is_some() {
                    app.start("sketch.dimension");
                } else {
                    app.start("inspect.measure");
                }
            }
            Key::P if !mods.shift && !mods.alt && !mods.command => {
                app.start("solid.press_pull");
            }
            Key::T if !mods.shift && !mods.alt && !mods.command => {
                app.start("inspect.measure");
            }
            Key::G if !mods.shift && !mods.alt && !mods.command && app.session.active_sketch.is_none() => {
                let bodies: Vec<String> = app
                    .session
                    .selection
                    .iter()
                    .filter_map(|s| match s {
                        sketcraft_engine::Sel::Body { name } => Some(name.clone()),
                        sketcraft_engine::Sel::Face { body, .. } => Some(body.clone()),
                        _ => None,
                    })
                    .collect();
                if !bodies.is_empty() {
                    let _ = app.run("component.from_bodies", json!({ "bodies": bodies }));
                } else {
                    app.start("component.create");
                }
            }
            Key::B if !mods.shift && !mods.alt && !mods.command && app.session.active_sketch.is_none() => {
                app.start("appearance.paint");
            }
            Key::Delete | Key::Backspace | Key::E => {
                if !crate::timeline_left::delete_selected_folders(app) {
                    crate::viewport::delete_selection(app);
                }
            }
            _ => crate::keymap::press(app, &crate::keymap::key_name(k, mods), repeat),
        }
    }
}

#[cfg(test)]
#[path = "toolbar_header_report_tests.rs"]
mod header_report_tests;

#[cfg(test)]
#[path = "toolbar_home_tests.rs"]
mod home_tests;

#[cfg(test)]
#[path = "toolbar_dropdown_tests.rs"]
mod dropdown_tests;

#[cfg(test)]
mod panel_drop_tests {
    use super::*;

    #[test]
    fn panel_label_gap_and_margins_append_while_buttons_insert_before() {
        for target in ["label", "gap", "left margin", "right margin", "button"] {
            let mut h = crate::scenario::Harness::new();
            h.step(&json!({"click":{"handle":"panel:MODEL:CREATE"}})).unwrap();
            let label = crate::scenario::handle_at("panel:MODEL:CREATE").unwrap();
            let panel = crate::scenario::rect_of("panel:MODEL:CREATE").unwrap();
            let destination = match target {
                "gap" => label - vec2(0.0, 10.0),
                "left margin" => pos2(panel.left() + 1.0, label.y - 30.0),
                "right margin" => pos2(panel.right() - 1.0, label.y - 30.0),
                "button" => crate::scenario::handle_at("button:solid.extrude").unwrap(),
                _ => label,
            };
            h.step(&json!({"drag":[{"handle":"dropdown:solid.cylinder"}, [destination.x,destination.y]],"steps":12})).unwrap();
            h.step(&json!({"key":"Escape"})).unwrap();
            let order = if target == "button" {
                json!(["button:solid.cylinder", "button:solid.extrude", "button:solid.revolve", "button:solid.hole"])
            } else {
                json!(["button:solid.extrude", "button:solid.revolve", "button:solid.hole", "button:solid.cylinder"])
            };
            h.step(&json!({"expect":{"handle_order":order,"tool":null,"dialog":null,"errors":0}})).unwrap();
            assert!(h.app.session.doc.features.is_empty(), "{target}: pinning must not run the command");
            h.step(&json!({"restart":"close"})).unwrap();
            h.step(&json!({"expect":{"handle_order":order}})).unwrap();
        }
    }

    #[test]
    fn panel_label_drop_moves_a_button_between_panels() {
        let mut h = crate::scenario::Harness::new();
        h.step(&json!({"drag":[{"handle":"button:solid.extrude"},{"handle":"panel:MODEL:MODIFY"}],"steps":12})).unwrap();
        h.step(&json!({"expect":{"handle_order":["button:solid.fillet","button:solid.chamfer","button:solid.shell","button:solid.extrude"],"tool":null,"dialog":null,"errors":0}})).unwrap();
        h.step(&json!({"restart":"close"})).unwrap();
        h.step(&json!({"expect":{"handle_order":["button:solid.shell","button:solid.extrude"]}})).unwrap();
    }
}

#[cfg(test)]
#[path = "toolbar_dropdown_toggle_tests.rs"]
mod dropdown_toggle_tests;

#[cfg(test)]
mod app_bar_tests {
    use super::*;

    #[test]
    fn bar_hover_inks_and_dividers_remain_readable_in_both_themes() {
        fn luminance(c: Color32) -> f32 {
            let c = egui::Rgba::from(c);
            0.2126 * c.r() + 0.7152 * c.g() + 0.0722 * c.b()
        }
        fn contrast(a: Color32, b: Color32) -> f32 {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }
        for dark in [false, true] {
            for vision in crate::theme::ColorVision::ALL {
                let t = Tokens::themed(dark, vision);
                for fill in [t.app_bar_hover, t.app_bar_hover_subtle, t.app_bar_hover_button, t.app_bar_hover_strong] {
                    let channel = |fg: u8, bg: u8| fg.saturating_add((u16::from(bg) * u16::from(255 - fill.a()) / 255) as u8);
                    let bg = Color32::from_rgb(channel(fill.r(), t.app_bar.r()), channel(fill.g(), t.app_bar.g()), channel(fill.b(), t.app_bar.b()));
                    assert!(contrast(t.app_bar_text, bg) >= 4.5, "dark={dark}, {vision:?}, {fill:?}");
                }
                assert!(contrast(t.app_bar_separator, t.app_bar) >= 3.0);
                assert!(contrast(t.app_bar, t.warning) >= 4.5, "approval counts need dark ink on amber");
            }
        }
    }

    /// Save writes on a worker thread: frames until it has finished.
    fn wait_for_save(h: &mut crate::scenario::Harness) {
        for _ in 0..500 {
            if h.app.saving.is_none() {
                break;
            }
            h.frames(1);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        h.frames(1);
    }

    #[test]
    fn save_button_uses_the_existing_save_flow_for_new_and_named_designs() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length":2,"width":3,"height":4})).unwrap();
        h.frames(3);
        h.step(&json!({"click":{"handle":"titlebar:save"}})).unwrap();
        wait_for_save(&mut h);
        let path = h.app.session.path.clone().expect("Save chooses a path for an untitled design");
        h.app.run("solid.box", json!({"length":1,"width":1,"height":1,"corner":[10,0,0]})).unwrap();
        h.step(&json!({"click":{"handle":"titlebar:save"}})).unwrap();
        wait_for_save(&mut h);
        assert_eq!(h.app.session.path.as_deref(), Some(path.as_str()));
        let saved = sketcraft_engine::io::read_design(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved, *h.app.session.doc);
    }
}

#[cfg(test)]
#[path = "toolbar_ribbon_label_tests.rs"]
mod ribbon_label_tests;

#[cfg(test)]
mod ribbon_gallery_tests {
    use super::*;
    use crate::scenario::{Harness, rect_of};

    #[test]
    fn dragged_constraint_becomes_full_button_and_reset_restores_gallery() {
        let mut h = Harness::new();
        h.call("ui.resize", json!({"width":1800,"height":900}));
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        h.frames(3);
        let id = crate::workspace::CONSTRAINT_GALLERY[0];
        let cell_handle = format!("ribbon:constraint:{id}");
        let button_handle = format!("toolbar:{id}");
        let button = format!("button:{id}");
        let cell = rect_of(&cell_handle).unwrap();
        let count = || crate::scenario::handles("ribbon:constraint:").len();
        let original_count = count();
        h.step(&json!({"drag":[{"handle":button_handle},{"handle":"toolbar:sketch.dimension"}]})).unwrap();
        h.frames(3);
        assert!(rect_of(&cell_handle).is_none());
        let full = rect_of(&button).unwrap();
        assert!(full.height() > cell.height());
        assert!(full.left() < rect_of("button:sketch.dimension").unwrap().left());
        assert_eq!(count(), original_count - 1);
        let next = format!("ribbon:constraint:{}", crate::workspace::CONSTRAINT_GALLERY[1]);
        assert_eq!(rect_of(&next).unwrap().top(), cell.top());
        h.step(&json!({"drag":[{"handle":button_handle},{"handle":next}]})).unwrap();
        h.frames(3);
        assert_eq!(rect_of(&cell_handle), Some(cell));
        assert_eq!(count(), original_count);
        h.step(&json!({"drag":[{"handle":button_handle},{"handle":"toolbar:sketch.dimension"}]})).unwrap();
        h.frames(3);
        assert!(rect_of(&cell_handle).is_none());
        let saved = crate::toolbar_custom::export(&h.app);
        crate::toolbar_custom::reset(&mut h.app, "SKETCH");
        crate::toolbar_custom::import(&mut h.app, &saved).unwrap();
        h.frames(3);
        assert!(rect_of(&cell_handle).is_none());
        assert_eq!(rect_of(&button).unwrap().height(), full.height());
        // Settings saved before the gallery could change load with the default gallery.
        let mut older = h.app.toolbar_custom.prefs();
        older.as_object_mut().unwrap().remove("gallery_buttons");
        let mut loaded = crate::toolbar_custom::ToolbarCustom::default();
        loaded.load(&older);
        assert_eq!(loaded.panels, h.app.toolbar_custom.panels);
        assert!(loaded.in_gallery("SKETCH", "CONSTRAIN", id));
        crate::toolbar_custom::reset(&mut h.app, "MODEL");
        h.frames(2);
        assert!(rect_of(&cell_handle).is_none());
        crate::toolbar_custom::reset(&mut h.app, "SKETCH");
        h.frames(3);
        assert_eq!(rect_of(&cell_handle), Some(cell));
        assert_eq!(count(), original_count);
    }

    #[test]
    fn constraint_gallery_preserves_button_handles_order_and_pin_actions() {
        let mut h = Harness::new();
        h.call("ui.resize", json!({"width":1800,"height":900}));
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        h.frames(3);
        let dimension = rect_of("button:sketch.dimension").unwrap();
        let mut cells = Vec::new();
        for id in crate::workspace::CONSTRAINT_GALLERY {
            let cell = rect_of(&format!("ribbon:constraint:{id}")).unwrap();
            assert_eq!(cell.size(), vec2(24.0, 20.0));
            assert_eq!(rect_of(&format!("button:{id}")), Some(cell));
            assert!(cell.left() > dimension.right());
            cells.push(cell);
        }
        for row in cells.chunks(3) {
            assert!(row.windows(2).all(|pair| pair[0].right() < pair[1].left() && pair[0].top() == pair[1].top()));
        }
        for col in 0..3 {
            assert_eq!(cells[col].left(), cells[col + 3].left());
            assert!(cells[col].bottom() < cells[col + 3].top());
        }
        h.step(&json!({"click":{"handle":"ribbon:constraint:sketch.constraint.equal"}})).unwrap();
        assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.constraint.equal"));
        h.step(&json!({"key":"Escape"})).unwrap();
        crate::toolbar_custom::remove(&mut h.app, "SKETCH", "sketch.constraint.equal");
        h.frames(2);
        assert!(rect_of("ribbon:constraint:sketch.constraint.equal").is_none());
        crate::toolbar_custom::pin(&mut h.app, "SKETCH", "sketch.constraint.equal").unwrap();
        h.frames(2);
        assert!(rect_of("ribbon:constraint:sketch.constraint.equal").is_some());
    }
}

#[cfg(test)]
#[path = "toolbar_fill_width_tests.rs"]
mod fill_width_tests;

#[cfg(test)]
mod sketch_return_tab_tests {
    use super::*;

    /// #3250: finishing a sketch goes back to the tab it was started from, not always to MODEL.
    #[test]
    fn finishing_a_sketch_returns_to_the_tab_it_started_from() {
        for start in ["MODEL", "SURFACE", "ASSEMBLE", "INSERT", "INSPECT", "MANAGE", "VIEW"] {
            let mut h = crate::scenario::Harness::new();
            h.app.home.open = false;
            h.app.ui.tab = start.into();
            h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
            assert_eq!(h.app.ui.tab, "SKETCH", "{start}: the sketch tab shows while sketching");
            h.app.run("sketch.finish", json!({})).unwrap();
            assert!(h.app.session.active_sketch.is_none());
            assert_eq!(h.app.ui.tab, start, "{start}: back on the tab the sketch started from");
        }
    }

    /// A second sketch started from another tab returns to that tab, and a start tab that is
    /// not a normal toolbar tab (a stale name) falls back to MODEL.
    #[test]
    fn sketch_return_tab_follows_each_sketch_and_falls_back_to_model() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        for start in ["SURFACE", "INSPECT"] {
            h.app.ui.tab = start.into();
            h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
            h.app.run("sketch.finish", json!({})).unwrap();
            assert_eq!(h.app.ui.tab, start);
        }
        let start = "NO SUCH TAB";
        h.app.ui.tab = start.into();
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        assert_eq!(h.app.ui.tab, "MODEL", "{start}: falls back to MODEL");
    }
}

#[cfg(test)]
mod command_start_tab_tests {
    use super::*;
    use crate::scenario::Harness;

    /// The issue's starting point: a design with a box, nothing selected, the ribbon on MODEL.
    fn on_model_with_a_box() -> Harness {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.app.ui.tab = "MODEL".into();
        h.app.status = None;
        h
    }

    /// #10540: a command that only reports a missing parameter or that there is nothing to work
    /// on starts nothing, so the ribbon stays on the tab it was started from, as a disabled
    /// command already does. The commands are the issue's table, started from MODEL.
    #[test]
    fn a_refused_command_leaves_the_ribbon_on_the_tab_it_was_started_from() {
        for id in [
            "occurrence.ground",
            "joint.limits",
            "component.break_link",
            "timeline.dependents",
            "view.rename",
            "view.delete",
            "view.navigate",
            "edit.restore_checkpoint",
            "parameters.move",
            "library.inspect",
            // surface.fair and surface.match open their dialogs (#4644), so they start something.
            "sheet.export_bend_table",
        ] {
            let mut h = on_model_with_a_box();
            h.app.start(id);
            assert_eq!(h.app.ui.tab, "MODEL", "{id}: a refused command leaves the tab where it was");
            assert!(h.app.dialog.is_none() && h.app.tool.is_none(), "{id}: nothing started");
        }
    }

    /// The issue's exact repro through the control channel: Ground from MODEL says `occurrence`
    /// must be an id or name; the status line keeps the refusal, the ribbon does not move to
    /// ASSEMBLE.
    #[test]
    fn ground_from_command_search_names_the_missing_occurrence_and_stays_on_model() {
        let mut h = on_model_with_a_box();
        h.call("ui.start", json!({"command": "occurrence.ground"}));
        assert_eq!(h.app.ui.tab, "MODEL", "Ground started nothing: the ribbon stays on MODEL");
        let (status, _, error) = h.app.status.clone().expect("the refusal reaches the status line");
        assert!(error, "{status}");
        assert!(status.contains("occurrence"), "{status}");
    }

    /// #10540: View Fit, Home and the other view display commands act on the view the same way
    /// however they are started. F6, the navigation bar and the view cube never move the ribbon,
    /// so command search and the VIEW tab's menus must not either.
    #[test]
    fn view_display_commands_keep_the_ribbon_on_the_tab_they_were_started_from() {
        for id in ["view.fit", "view.home", "view.spin", "view.grid", "view.ground_shadow", "view.navigation_bar", "view.visual_style", "view.tool"] {
            let mut h = on_model_with_a_box();
            h.app.ui.tab = "INSPECT".into();
            h.app.start(id);
            assert_eq!(h.app.ui.tab, "INSPECT", "{id}: started from INSPECT, the ribbon stays there");
        }
    }

    /// The other half of #10540: a command that does start still moves the ribbon to its tab —
    /// Extrude's dialog opens and the ribbon follows it to MODEL from INSPECT.
    #[test]
    fn a_command_that_opens_still_moves_the_ribbon_to_its_tab() {
        let mut h = on_model_with_a_box();
        h.app.ui.tab = "INSPECT".into();
        h.app.start("solid.extrude");
        assert!(h.app.dialog.is_some(), "Extrude's dialog opens");
        assert_eq!(h.app.ui.tab, "MODEL", "the ribbon follows the command that started");
    }

    /// A command that runs without a dialog or tool has run: the ribbon keeps the tab it moved
    /// to (#10540's "or it runs"). List Contact Sets answers with the sets, no parameters.
    #[test]
    fn a_command_that_runs_still_moves_the_ribbon() {
        let mut h = on_model_with_a_box();
        h.app.start("contact.list");
        assert_eq!(h.app.ui.tab, "ASSEMBLE", "the command ran: the ribbon stays on its tab");
        assert!(!h.app.status.as_ref().is_some_and(|(_, _, error)| *error));
    }
}

#[cfg(test)]
#[path = "toolbar_persistent_ribbon_tests.rs"]
mod persistent_ribbon_tests;

#[cfg(test)]
#[path = "toolbar_quick_access_tests.rs"]
mod quick_access_tests;
