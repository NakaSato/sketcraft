//! Large Tool Set: an optional two-column tool rail that shows every tool without flyouts,
//! plus Section cuts and Tags, like SketchUp's Large Tool Set.
use super::*;

const BUTTON: egui::Vec2 = egui::vec2(56., 50.);
const GAP: f32 = 4.;
/// Width of the two-column rail including its side margins.
pub(super) const LARGE_RAIL_WIDTH: f32 = 10. + 2. * 56. + 4. + 10.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum RailItem {
    Tool(Tool),
    SectionCuts,
    Tags,
}

/// Every tool in rail order, then the two view shortcuts.
pub(super) fn large_tool_set_items() -> Vec<RailItem> {
    Tool::ALL
        .iter()
        .map(|&tool| RailItem::Tool(tool))
        .chain([RailItem::SectionCuts, RailItem::Tags])
        .collect()
}

impl SketCraftApp {
    pub(super) fn large_tool_rail(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        egui::Panel::left("tools")
            .exact_size(LARGE_RAIL_WIDTH)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(colors.paper)
                    .inner_margin(egui::Margin::symmetric(10, 18)),
            )
            .show(ui, |ui| {
                let scroll = sidebar::restored_scroll_area(
                    ui,
                    "sidebar_left",
                    egui::ScrollArea::vertical().id_salt("tool_scroll"),
                )
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
                    for row in large_tool_set_items().chunks(2) {
                        ui.horizontal(|ui| {
                            for &item in row {
                                self.large_tool_button(ui, item);
                            }
                        });
                    }
                });
                sidebar::remember_scroll(ui.ctx(), "sidebar_left", scroll.state.clamped_offset());
            });
    }

    fn large_tool_button(&mut self, ui: &mut egui::Ui, item: RailItem) {
        let colors = Palette::for_ui(ui);
        let (rect, response) = ui.allocate_exact_size(BUTTON, Sense::click());
        let (label, active) = match item {
            RailItem::Tool(tool) => (
                if tool == Tool::TapeMeasure { "Tape" } else { tool.label() },
                self.tool == tool,
            ),
            RailItem::SectionCuts => ("Sections", self.doc.display.section_cuts),
            RailItem::Tags => ("Tags", false),
        };
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                if matches!(item, RailItem::Tool(_)) { egui::Role::RadioButton } else { egui::Role::Button },
                true,
                active,
                label,
            )
        });
        if active {
            ui.painter().rect_filled(rect, 8, colors.selected);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 8, colors.hovered);
        }
        let color = if active { colors.accent } else { colors.ink };
        let icon = Rect::from_center_size(rect.center() - egui::vec2(0., 7.), egui::vec2(24., 24.));
        match item {
            RailItem::Tool(tool) => icons::ui_tool(ui.painter(), icon, tool),
            RailItem::SectionCuts => icons::line_icon(ui.painter(), icon.center(), "sections", color),
            RailItem::Tags => icons::line_icon(ui.painter(), icon.center(), "tags", color),
        }
        let small = label.len() > 8;
        ui.painter().text(
            rect.center() + egui::vec2(0., 15.),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(if small { 8.5 } else { 9.5 }),
            color,
        );
        match item {
            RailItem::Tool(tool) => {
                if self.tool_tooltip(response, tool).clicked() {
                    if tool.is_solid() {
                        self.start_solid_tool(tool, ui.ctx());
                    } else {
                        self.set_tool(tool);
                    }
                }
            }
            RailItem::SectionCuts => {
                if response.on_hover_text("Show or hide section cuts").clicked() {
                    self.doc.display.section_cuts = !self.doc.display.section_cuts;
                    self.changed(ui.ctx());
                }
            }
            RailItem::Tags => {
                if response.on_hover_text("Open Tags in the sidebar").clicked() {
                    self.inspector = true;
                    sidebar::set_section_open(ui.ctx(), "tags", true);
                    ui.ctx().request_repaint();
                }
            }
        }
    }
}
