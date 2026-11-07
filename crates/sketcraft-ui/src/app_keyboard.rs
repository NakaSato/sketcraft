//! Keyboard shortcuts and typed input for tools, files and editing.
use super::*;

impl KetchupApp {
    pub(super) fn keyboard(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.updates.open {
            return;
        }

        if self.project.blocked() || self.screenshot.open || self.tape_resize.is_some() {
            return;
        }
        if self.section_drag.is_some() {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.cancel_section_drag(ctx);
            }
            return;
        }
        if self.settings_open {
            return;
        }
        if self.help || self.about || self.release_notes.open {
            let modifiers = ctx.input(|i| i.modifiers);
            self.move_modifier_down = if ctx.os() == egui::os::OperatingSystem::Mac {
                modifiers.alt
            } else {
                modifiers.ctrl
            };
            if (!self.release_notes.open || !egui::Popup::is_any_open(ctx))
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
            {
                self.help = false;
                self.about = false;
                self.release_notes.open = false;
                ctx.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
            }
            return;
        }
        if self.object_creation.is_none()
            && self.object_rename.is_none()
            && self.viewports.renaming.is_none()
            && self.text_editor.is_none()
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, Key::F)
                    | i.consume_key(egui::Modifiers::CTRL, Key::F)
            })
        {
            self.request_outliner_search(ctx);
            return;
        }
        for (modifiers, key, action) in [
            (egui::Modifiers::COMMAND, Key::N, 0),
            (egui::Modifiers::COMMAND, Key::O, 1),
            (egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::S, 2),
            (egui::Modifiers::COMMAND, Key::S, 3),
            (egui::Modifiers::COMMAND, Key::I, 4),
        ] {
            if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                match action {
                    0 => self.request_project(projects::Action::New, ctx),
                    1 => self.request_project(projects::Action::Open, ctx),
                    2 => self.save_project(ctx, true),
                    4 => {
                        self.project.import_format = "any".into();
                        self.import_model(ctx);
                    }
                    _ => self.save_project(ctx, false),
                }
                return;
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.joinery.open && self.object_creation.is_none()
                && self.object_rename.is_none() && self.viewports.renaming.is_none()
                && self.text_editor.is_none()
            {
                self.joinery.open = false;
                self.joinery_dialog = Default::default();
                ctx.memory_mut(|memory| {
                    if let Some(id) = memory.focused() { memory.surrender_focus(id); }
                });
                ctx.request_repaint();
                return;
            }
            let solid_operation = self.solid_first.take().is_some();
            self.flip_hover = None;
            let measurement_operation = self.measurement.take().is_some();
            let editing_operation = self.last_move_copy.is_some()
                || solid_operation
                || measurement_operation
                || self.drawing.is_some()
                || self.modeling.is_some()
                || self.gesture.is_some()
                || self.line_start.is_some()
                || self.paste_preview.is_some()
                || self.object_creation.is_some()
                || self.object_rename.is_some()
                || self.viewports.renaming.is_some()
                || self.text_editor.is_some()
                || self.help
                || ctx.egui_wants_keyboard_input();
            if !editing_operation && self.doc.model.edit_context.is_some() {
                self.close_edit_context(ctx);
            }
            self.move_copy = false;
            self.last_move_copy = None;
            let completed_group = self.line_group;
            self.stop_drawing();
            self.modeling = None;
            self.notice = None;
            self.help = false;
            self.paste_preview = None;
            self.object_creation = None;
            self.object_rename = None;
            self.viewports.renaming = None;
            self.text_editor = None;
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            self.prompt_drawn_group_name(ctx, completed_group);
        }
        if ctx.egui_wants_keyboard_input()
            || self.help
            || self.object_creation.is_some()
            || self.object_rename.is_some()
            || self.viewports.renaming.is_some()
            || self.text_editor.is_some()
        {
            let modifiers = ctx.input(|i| i.modifiers);
            self.move_modifier_down = if ctx.os() == egui::os::OperatingSystem::Mac {
                modifiers.alt
            } else {
                modifiers.ctrl
            };
            return;
        }
        let sidebar_tab = ctx.input_mut(|i| {
            let mut consumed = false;
            if i.focused {
                i.events.retain(|event| {
                    if let egui::Event::Key {
                        key: Key::Tab,
                        pressed: true,
                        repeat,
                        modifiers,
                        ..
                    } = event
                        && !modifiers.any()
                    {
                        consumed = true;
                        if !repeat {
                            self.inspector = !self.inspector;
                        }
                        false
                    } else {
                        true
                    }
                });
            }
            consumed
        });
        if sidebar_tab {
            // egui schedules Tab focus traversal before application shortcuts run.
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            ctx.request_repaint();
        }
        let events = ctx.input(|i| i.events.clone());
        let copy_event = events.iter().any(|e| matches!(e, egui::Event::Copy));
        let cut_event = events.iter().any(|e| matches!(e, egui::Event::Cut));
        let paste_event = events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
        let shortcuts = events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Copy
                    | egui::Event::Cut
                    | egui::Event::Paste(_)
                    | egui::Event::Key {
                        pressed: true,
                        modifiers: egui::Modifiers { ctrl: true, .. },
                        ..
                    }
                    | egui::Event::Key {
                        pressed: true,
                        modifiers: egui::Modifiers { mac_cmd: true, .. },
                        ..
                    }
            )
        });
        for event in events {
            match event {
                egui::Event::ModifiersChanged(modifiers) => {
                    self.move_modifier(ctx, modifiers, shortcuts)
                }
                egui::Event::Copy => self.context_action(ctx, ContextAction::Copy),
                egui::Event::Cut => self.context_action(ctx, ContextAction::Cut),
                egui::Event::Paste(text) => {
                    self.context_position = None;
                    if let Some(json) = text.strip_prefix("ketchup-geometry-v1\n") {
                        match serde_json::from_str::<GeometryClipboard>(json) {
                            Ok(clipboard)
                                if clipboard.anchor.is_finite()
                                    && clipboard.model.validate().is_ok() =>
                            {
                                self.clipboard = Some(clipboard)
                            }
                            _ => {
                                self.say(ctx, "The clipboard geometry could not be read");
                                continue;
                            }
                        }
                    }
                    self.context_action(
                        ctx,
                        if ctx.input(|i| i.modifiers.shift) {
                            ContextAction::PasteInPlace
                        } else {
                            ContextAction::Paste
                        },
                    );
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => {
                    let command = modifiers.command || modifiers.ctrl;
                    if command {
                        match key {
                            Key::Z => self.undo(ctx, modifiers.shift),
                            Key::Y => self.undo(ctx, true),
                            Key::A => self.selected = self.doc.model.selectable_entities(),
                            Key::C if !copy_event => self.context_action(ctx, ContextAction::Copy),
                            Key::X if !cut_event => self.context_action(ctx, ContextAction::Cut),
                            Key::V if !paste_event => {
                                self.context_position = None;
                                self.context_action(
                                    ctx,
                                    if modifiers.shift {
                                        ContextAction::PasteInPlace
                                    } else {
                                        ContextAction::Paste
                                    },
                                );
                            }
                            _ => {}
                        }
                        continue;
                    }
                    // Letter keys belong to an in-progress dimension, including
                    // unit suffixes. Letter shortcuts resume after the dimension is cleared.
                    if self.precision_available()
                        && !self.length_input.is_empty()
                        && (Key::A..=Key::Z).contains(&key)
                    {
                        continue;
                    }
                    if let Some(tool) = self.general_settings.tool_for_key(key) {
                        self.set_tool(tool);
                        continue;
                    }
                    // G makes a component from the selection, as in SketchUp, unless a
                    // tool was assigned G in Settings (handled above).
                    if key == Key::G && !modifiers.shift && !modifiers.alt {
                        if self.selected.is_empty() {
                            self.say(ctx, "Select geometry, then press G to make a component");
                        } else {
                            self.begin_object_creation(ctx, true);
                        }
                        continue;
                    }
                    match key {
                        Key::Delete => self.erase_selected(ctx),
                        Key::Backspace => {
                            if self.precision_available() {
                                self.length_input.pop();
                            } else {
                                self.erase_selected(ctx);
                            }
                        }
                        Key::Enter if self.tool == Tool::Line => self.commit_line(ctx, true),
                        Key::Enter if self.drawing.is_some() => self.commit_drawing(ctx),
                        Key::Enter if self.modeling.is_some() => self.commit_modeling(ctx),
                        Key::Enter if self.last_move_copy.is_some() => self.commit_move_array(ctx),
                        Key::Enter if self.tape_resize_available() => self.request_tape_resize(ctx),
                        Key::ArrowRight | Key::ArrowLeft | Key::ArrowUp
                            if self.tool == Tool::Flip =>
                        {
                            self.apply_flip(
                                match key {
                                    Key::ArrowRight => Axis::X,
                                    Key::ArrowLeft => Axis::Y,
                                    _ => Axis::Z,
                                },
                                ctx,
                            );
                        }
                        Key::ArrowRight | Key::ArrowLeft | Key::ArrowUp
                            if self.tool.is_drawing()
                                || matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale | Tool::PushPull | Tool::Offset)
                                || self.tool.is_measuring() =>
                        {
                            let axis = match key {
                                Key::ArrowRight => Axis::X,
                                Key::ArrowLeft => Axis::Y,
                                _ => Axis::Z,
                            };
                            self.toggle_axis_lock(axis, ctx);
                        }
                        Key::ArrowDown => self.set_axis_lock(None, ctx),
                        Key::F1 => self.help = true,
                        _ => {}
                    }
                }
                egui::Event::Text(text) if self.precision_available() => {
                    if text.chars().all(|c| {
                        c.is_ascii_alphanumeric()
                            || matches!(
                                c,
                                '.' | '-' | '+' | '*' | '/' | '(' | ')' | ',' | '\'' | '"' | ' ' | '°'
                            )
                    }) && self.length_input.len() + text.len() <= precision::MAX_INPUT_LEN
                    {
                        self.length_input.push_str(&text);
                    }
                }
                _ => {}
            }
        }
    }
}
