use egui::{Event, Key, Modifiers, MouseWheelUnit, TouchPhase, pos2, vec2};
use serde_json::json;

use super::*;
use crate::SketApp;
use crate::scenario::Harness;

#[test]
fn plain_numbers_step_in_the_unit_they_are_written_in() {
    for (text, tenths, want) in [
        ("20 mm", 10, "21 mm"),
        ("20mm", -10, "19mm"),
        ("20", 100, "30"),
        ("2.5", 1, "2.6"),
        ("2", 1, "2.1"),
        ("2.25 mm", 10, "3.25 mm"),
        ("0.5 mm", -10, "-0.5 mm"),
        ("-0.5", 10, "0.5"),
        ("-1", 10, "0"),
        ("-0.1", 1, "0.0"),
        ("  45 deg ", 10, "  46 deg "),
        ("90°", -100, "80°"),
        ("1 in", 1, "1.1 in"),
        ("12 µm", 10, "13 µm"),
        ("+5", 10, "6"),
        (".5", 10, "1.5"),
        ("4", 30, "7"),
        ("2,5 mm", 10, "3,5 mm"),
        ("2,5", 1, "2,6"),
        ("-0,5", 5, "0,0"),
        ("20\"", 10, "21\""),
        ("3 ′", -10, "2 ′"),
        ("12 inch", 100, "22 inch"),
    ] {
        assert_eq!(stepped(text, tenths).as_deref(), Some(want), "{text:?} by {tenths} tenths");
    }
}

#[test]
fn anything_but_a_plain_number_is_left_as_typed() {
    for text in [
        "",
        "  ",
        "width",
        "width / 2",
        "20 mm + 1",
        "2 * 3",
        "1e3",
        "1,5,3",
        "2,",
        ",5",
        "2 1/8\"",
        "20 widths",
        "mm",
        "-",
        ".",
        "1.2.3",
        "20 mm mm",
        "inf",
        "NaN",
    ] {
        assert_eq!(stepped(text, 10), None, "{text:?}");
    }
    // Hostile input stays a string: no panic, nothing infinite.
    assert_eq!(stepped(&"9".repeat(400), 10), None);
    assert!(stepped("1", i64::MAX).is_some());
    assert!(stepped("1", i64::MIN).is_some());
}

/// One focused value field (a dialog's, through `params_dialog::complete`) in a scroll area taller
/// than the window, with value stepping on (`Field::new`) or off as by default (`Field::off`).
struct Field {
    on: bool,
    ctx: egui::Context,
    app: SketApp,
    text: String,
    id: egui::Id,
    changed: bool,
    rect: egui::Rect,
    scroll: egui::Id,
}

impl Field {
    fn new(text: &str) -> Self {
        Self::with(text, true)
    }

    fn off(text: &str) -> Self {
        Self::with(text, false)
    }

    fn with(text: &str, on: bool) -> Self {
        let ctx = egui::Context::default();
        let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
        crate::params_dialog::publish(&mut app, &ctx);
        let id = egui::Id::unique("value_step_test_field");
        let mut f = Self { on, ctx, app, text: text.into(), id, changed: false, rect: egui::Rect::NOTHING, scroll: egui::Id::NULL };
        f.ctx.memory_mut(|m| m.request_focus(id));
        f.frame(Vec::new());
        f.frame(Vec::new());
        f
    }

    fn frame(&mut self, events: Vec<Event>) {
        let mut out = self.ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 300.0))), events, ..Default::default() },
            |ui| {
                crate::expression_complete::begin_frame(ui.ctx());
                set_enabled(ui.ctx(), self.on);
                crate::params_dialog::publish(&mut self.app, ui.ctx());
                self.scroll = egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut self.text).id(self.id));
                        self.changed = crate::params_dialog::complete(ui, &r, &mut self.text);
                        self.rect = r.rect;
                        ui.add_space(1000.0);
                    })
                    .id;
            },
        );
        out.textures_delta.clear();
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.frame(vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
    }

    fn wheel(&mut self, at: egui::Pos2, unit: MouseWheelUnit, dy: f32) {
        let wheel = Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit,
            delta: vec2(0.0, dy),
            modifiers: Modifiers::NONE,
            phase: TouchPhase::Move,
        };
        self.frame(vec![Event::PointerMoved(at), wheel]);
    }

    fn selection(&self) -> (usize, usize) {
        let r = egui::TextEdit::load_state(&self.ctx, self.id).unwrap().cursor.char_range().unwrap();
        let (a, b) = (r.primary.index.0, r.secondary.index.0);
        (a.min(b), a.max(b))
    }

    fn scrolled(&self) -> f32 {
        egui::scroll_area::State::load(&self.ctx, self.scroll).map_or(0.0, |s| s.clamped_offset().y)
    }
}

#[test]
fn arrows_step_a_focused_field_and_select_the_new_value() {
    let mut f = Field::new("20 mm");
    f.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(f.text, "21 mm");
    assert!(f.changed, "callers see the change");
    assert_eq!(f.selection(), (0, 5), "typing replaces the stepped value");
    f.key(Key::ArrowDown, Modifiers::NONE);
    f.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(f.text, "19 mm");
    f.key(Key::ArrowUp, Modifiers::SHIFT);
    assert_eq!(f.text, "29 mm", "Shift: ten");
    f.key(Key::ArrowDown, Modifiers::ALT);
    assert_eq!(f.text, "28.9 mm", "Alt: a tenth");
    f.key(Key::ArrowUp, Modifiers::COMMAND);
    assert_eq!(f.text, "28.9 mm", "Ctrl/Cmd keep the text cursor's Up");
    assert!(!f.changed);
}

#[test]
fn arrows_move_the_cursor_in_an_expression_and_leave_unfocused_fields_alone() {
    let mut f = Field::new("width / 2");
    f.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(f.text, "width / 2");
    assert_eq!(f.selection(), (0, 0), "Up moved the text cursor home, as in any text box");
    let mut f = Field::new("20 mm");
    f.ctx.memory_mut(|m| m.surrender_focus(f.id));
    f.frame(Vec::new());
    f.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(f.text, "20 mm");
}

#[test]
fn an_open_completion_list_keeps_the_arrows() {
    // A plain number's unit is a whole name, so the list only opens when asked for (Ctrl+Space).
    let mut f = Field::new("20 mm");
    f.key(Key::Space, Modifiers::CTRL);
    f.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(f.text, "20 mm", "the arrows go to the list while it shows");
    f.key(Key::Escape, Modifiers::NONE);
    f.ctx.memory_mut(|m| m.request_focus(f.id));
    f.frame(Vec::new());
    f.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(f.text, "19 mm", "with the list dismissed they step");
}

#[test]
fn the_wheel_over_a_focused_field_steps_it_instead_of_scrolling() {
    let mut f = Field::new("20 mm");
    let over = f.rect.center();
    f.wheel(over, MouseWheelUnit::Line, 1.0);
    assert_eq!(f.text, "21 mm", "a notch up steps up");
    f.wheel(over, MouseWheelUnit::Line, -2.0);
    assert_eq!(f.text, "19 mm");
    // A trackpad's points add up to a notch (a line's worth) before the value moves.
    let speed = f.ctx.options(|o| o.input_options.line_scroll_speed);
    for _ in 0..3 {
        f.wheel(over, MouseWheelUnit::Point, speed / 4.0);
    }
    assert_eq!(f.text, "19 mm");
    f.wheel(over, MouseWheelUnit::Point, speed / 4.0);
    assert_eq!(f.text, "20 mm");
    for _ in 0..30 {
        f.frame(vec![Event::PointerMoved(over)]);
    }
    assert_eq!(f.scrolled(), 0.0, "the panel under the field did not scroll");
    // Off the field the wheel scrolls as before and leaves the value.
    f.wheel(pos2(over.x, over.y + 120.0), MouseWheelUnit::Line, -3.0);
    for _ in 0..30 {
        f.frame(Vec::new());
    }
    assert_eq!(f.text, "20 mm");
    assert!(f.scrolled() > 0.0, "the panel scrolls off the field");
}

/// The Parameters window's expression boxes step too; Enter applies the stepped expression.
#[test]
fn parameter_expressions_step_and_apply() {
    let mut h = Harness::new();
    h.app.preferences.step_values = true;
    h.app.run("parameters.add", json!({"name": "width", "expression": "10 mm"})).unwrap();
    h.step(&json!({"start": "parameters.change"})).unwrap();
    h.frames(4);
    h.step(&json!({"click": {"handle": "param:width"}})).unwrap();
    h.frames(2);
    h.step(&json!({"key": "ArrowUp"})).unwrap();
    h.step(&json!({"key": "ArrowUp"})).unwrap();
    h.frames(2);
    assert_eq!(h.app.params.as_ref().unwrap().editing.get("width").map(String::as_str), Some("12 mm"));
    h.step(&json!({"key": "Enter"})).unwrap();
    h.frames(2);
    assert_eq!(h.app.session.doc.param("width").map(|p| p.expr.as_str()), Some("12 mm"));
}

/// Off, as by default, value fields behave as before: Up moves the text cursor home, the wheel
/// scrolls the panel under the field, and the box beside the geometry follows its arrow.
#[test]
fn off_by_default_the_keys_and_the_wheel_do_what_they_did() {
    assert!(!crate::prefs::Prefs::default().step_values);
    assert!(!serde_json::from_value::<crate::prefs::Prefs>(json!({})).unwrap().step_values, "saved preferences without it");
    let mut f = Field::off("20 mm");
    f.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!((f.text.as_str(), f.selection()), ("20 mm", (0, 0)), "Up moved the cursor home");
    let over = f.rect.center();
    f.wheel(over, MouseWheelUnit::Line, -3.0);
    for _ in 0..30 {
        f.frame(vec![Event::PointerMoved(over)]);
    }
    assert_eq!(f.text, "20 mm");
    assert!(f.scrolled() > 0.0, "the wheel scrolled the panel");

}

/// The preference is on Preferences › Design, and the control channel sets it.
#[test]
fn the_preference_is_on_the_design_page() {
    let mut h = Harness::new();
    crate::prefs::set(&mut h.app, &json!({"step_values": true})).unwrap();
    assert!(h.app.preferences.step_values);
    crate::prefs::set(&mut h.app, &json!({"step_values": false})).unwrap();
    h.call("ui.prefs", json!({"open": "Design"}));
    h.frames(3);
    // The row can be below the page's fold: scroll the page to it, as a user would, a little at a
    // time so the page's length doesn't matter.
    for _ in 0..40 {
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique("sc_prefs"))).unwrap();
        let row = crate::scenario::handle_at("prefs:step_values").expect("the Design page shows the row");
        if window.shrink(60.0).contains(row) {
            break;
        }
        let delta = if row.y > window.center().y { -60.0 } else { 60.0 };
        h.call("ui.scroll", json!({"x": window.center().x, "y": window.center().y, "delta": delta}));
        h.frames(10);
    }
    h.step(&json!({"click": {"handle": "prefs:step_values"}})).unwrap();
    h.frames(2);
    assert!(h.app.preferences.step_values, "the Design page's checkbox turns it on");
}
