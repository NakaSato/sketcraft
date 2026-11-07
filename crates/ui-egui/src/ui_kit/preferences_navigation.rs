//! Compact navigation preference compositions and explanatory figures.
use super::metrics::dimension;
pub(crate) use super::preferences_figure_data::{paint_orbit_centre, paint_orbit_cursor, paint_orbit_origin};
pub(crate) use super::preferences_figure_ops::{paint_navigation_axes, paint_navigation_cube, paint_navigation_custom};
use crate::theme::Tokens;
use egui::{Rect, Sense, Ui, pos2, vec2};

/// One mouse silhouette beside the actual mapping selector and binding legend.
/// The figure is descriptive content, not an icon asset or an application viewport.
pub(crate) fn settings_mouse_mapping(ui: &mut Ui, bindings: &[(Vec<String>, String)], mapping: impl FnOnce(&mut Ui)) {
    let keys: Vec<Vec<&str>> = bindings.iter().map(|(keys, _)| keys.iter().map(String::as_str).collect()).collect();
    let rows: Vec<(&[&str], &str)> = bindings.iter().zip(&keys).map(|((_, action), keys)| (keys.as_slice(), action.as_str())).collect();
    if crate::prefs_search::suppress_chrome(ui) {
        mapping(ui);
        super::settings_bindings(ui, &rows);
        return;
    }
    let t = Tokens::get();
    mapping(ui);
    ui.add_space(t.space_8);
    let available = dimension(ui.available_width());
    let figure_size = vec2(t.space_24 * 5.0, t.space_24 * 6.0);
    let gap = t.space_24 - t.space_2;
    let beside = available >= figure_size.x + gap + t.space_24 * 12.0;
    let content_width = if beside { available - figure_size.x - gap } else { available };
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let (figure, _) = ui.allocate_exact_size(vec2(figure_size.x.min(available), figure_size.y), Sense::hover());
        let content = ui.allocate_ui_with_layout(vec2(content_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            super::settings_bindings(ui, &rows);
        });
        let centre_y = if beside { content.response.rect.center().y } else { figure.center().y };
        let figure = Rect::from_center_size(pos2(figure.center().x, centre_y), figure.size());
        super::preferences_figure_ops::paint_mouse(&ui.painter_at(figure), figure);
    });
}
