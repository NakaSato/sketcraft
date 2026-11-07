#[path = "sidebar_layout_tests.rs"]
mod layout_tests;
#[path = "sidebar_tool_prefs_tests.rs"]
mod tool_prefs_tests;
#[path = "sidebar_material_prefs_tests.rs"]
mod material_prefs_tests;
#[path = "sidebar_display_prefs_tests.rs"]
mod display_prefs_tests;
#[path = "sidebar_general_prefs_tests.rs"]
mod general_prefs_tests;

use super::*;

use eframe::Storage;

#[derive(Default)]
struct SavedPreferences {
    values: std::collections::BTreeMap<String, String>,
    flushed: bool,
}

impl Storage for SavedPreferences {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.values.insert(key.to_owned(), value);
    }

    fn remove_string(&mut self, key: &str) {
        self.values.remove(key);
    }

    fn flush(&mut self) {
        self.flushed = true;
    }
}

fn scroll_frame(
    ctx: &egui::Context,
    key: &str,
    scope: &str,
    requested: Vec2,
    content: Vec2,
) -> Vec2 {
    let mut offset = Vec2::ZERO;
    let _ = ctx.run_test_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 400.))),
            ..Default::default()
        },
        |ui| {
            ui.push_id(scope, |ui| {
                let scroll = restored_scroll_area(
                    ui,
                    key,
                    egui::ScrollArea::both()
                        .id_salt("test_scroll")
                        .max_width(120.)
                        .max_height(80.)
                        .auto_shrink([false, false])
                        .scroll_offset(requested),
                )
                .show(ui, |ui| {
                    ui.allocate_exact_size(content, Sense::hover());
                });
                offset = scroll.state.clamped_offset();
                remember_scroll(ui.ctx(), key, offset);
            });
        },
    );
    offset
}
