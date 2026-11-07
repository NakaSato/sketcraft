//! Window chrome: title bar controls, top bar menus, status bar and the tool rail.
use super::*;

impl KetchupApp {
    pub(super) fn icon_button(
        ui: &mut egui::Ui,
        kind: &str,
        label: &str,
        enabled: bool,
        color: Color32,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(32., 32.),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        if response.hovered() && enabled {
            ui.painter().rect_filled(
                rect,
                5,
                if kind == "close" {
                    ACCENT
                } else {
                    ui.visuals().widgets.hovered.weak_bg_fill
                },
            );
        }
        icons::line_icon(
            ui.painter(),
            rect.center(),
            kind,
            if enabled {
                color
            } else {
                color.gamma_multiply(0.3)
            },
        );
        response.on_hover_text(label).clicked() && enabled
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn window_controls(ui: &mut egui::Ui) {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let color = ui.visuals().text_color();
        // The toolbar flows right to left, placing Close at the outer edge.
        if Self::icon_button(ui, "close", "Close", true, color) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if Self::icon_button(
            ui,
            if maximized { "restore" } else { "maximize" },
            if maximized { "Restore" } else { "Maximize" },
            true,
            color,
        ) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if Self::icon_button(ui, "minimize", "Minimize", true, color) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        ui.add_space(12.);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn window_resize_handles(ui: &mut egui::Ui) {
        use egui::{CursorIcon, ResizeDirection};

        if ui.input(|i| {
            i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)
        }) {
            return;
        }
        let rect = ui.ctx().viewport_rect();
        // Removing native decorations also removes their resize grips on some platforms.
        // Keep thin edge grips and larger corner targets without taking space from the UI.
        for (x, y, direction, cursor) in [
            (-1, -1, ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
            (0, -1, ResizeDirection::North, CursorIcon::ResizeVertical),
            (1, -1, ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
            (-1, 0, ResizeDirection::West, CursorIcon::ResizeHorizontal),
            (1, 0, ResizeDirection::East, CursorIcon::ResizeHorizontal),
            (-1, 1, ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
            (0, 1, ResizeDirection::South, CursorIcon::ResizeVertical),
            (1, 1, ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
        ] {
            let thickness = if x != 0 && y != 0 { 12. } else { 4. };
            let range = |min: f32, max: f32, side| match side {
                -1 => (min, min + thickness),
                1 => (max - thickness, max),
                _ => (min + 12., max - 12.),
            };
            let (left, right) = range(rect.left(), rect.right(), x);
            let (top, bottom) = range(rect.top(), rect.bottom(), y);
            let response = ui
                .interact(
                    Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)),
                    ui.scope_id().with(("window_resize", x, y)),
                    Sense::drag(),
                )
                .on_hover_cursor(cursor);
            if response.drag_started_by(egui::PointerButton::Primary) {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }

    pub(super) fn app_bar_menu(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
        // MenuConfig carries the same padding into nested submenus.
        // Override the stroke for every state, including hover and an open menu.
        egui::menu::MenuButton::from_button(egui::Button::new(title).stroke(Stroke::NONE))
            .config(egui::menu::MenuConfig::new().style(Self::menu_style))
            .ui(ui, content);
    }

    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        let dark = ui.visuals().dark_mode;
        let foreground = if dark { PAPER } else { colors.ink };
        let ctx = ui.ctx().clone();
        egui::Panel::top("top_bar")
            .exact_size(64.)
            .frame(
                egui::Frame::new()
                    .fill(if dark { colors.canvas } else { Color32::from_gray(235) })
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ui, |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Register the background first so buttons and text fields own their input.
                    let title_bar = ui.interact(
                        ui.max_rect().expand2(egui::vec2(18., 12.)),
                        ui.scope_id().with("window_drag"),
                        Sense::click_and_drag(),
                    );
                    if title_bar.double_clicked() {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    } else if title_bar.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                }
                let compact = ui.available_width() < 1000.;
                ui.visuals_mut().override_text_color = Some(foreground);
                ui.visuals_mut().widgets.inactive.fg_stroke = Stroke::new(1., foreground);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.visuals_mut().widgets.hovered.weak_bg_fill = if dark {
                    Color32::from_rgb(65, 77, 68)
                } else {
                    Color32::from_gray(215)
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    Self::window_controls(ui);
                    #[cfg(not(target_arch = "wasm32"))]
                    if self.updates.ready() && ui.button("Update ready").clicked() {
                        self.updates.open = true;
                    }
                    if self.account.sharing_available() {
                        let avatar = ui.add(egui::Button::new("").min_size(egui::vec2(32., 32.))
                            .corner_radius(16).stroke(Stroke::new(1., foreground.gamma_multiply(0.65))))
                            .on_hover_text(self.account.user.as_ref().map_or("Account · Not connected", |user| user.name.as_str()));
                        avatar.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Account"));
                        if let Some(texture) = &self.account.texture {
                            egui::Image::new((texture.id(), avatar.rect.size())).corner_radius(16).paint_at(ui, avatar.rect.shrink(1.));
                        } else if let Some(user) = &self.account.user {
                            let initial = user.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                            ui.painter().text(avatar.rect.center(), Align2::CENTER_CENTER, initial, FontId::proportional(16.), foreground);
                        } else {
                            icons::line_icon(ui.painter(), avatar.rect.center(), "account", foreground);
                        }
                        if self.account.user.is_some() {
                            self.account.avatar_menu(&avatar);
                        } else if avatar.clicked() {
                            self.account.open = true;
                        }
                        ui.add(egui::Separator::default().vertical().spacing(12.));
                    }
                    let suggestion_icon = egui::Atom::paint(egui::vec2(18., 18.), |ui, args| {
                        icons::line_icon(ui.painter(), args.rect.center(), "suggest_feature", args.fallback_text_color);
                    });
                    if ui.add(egui::Button::new((suggestion_icon, "Suggest a feature")).stroke(Stroke::new(1., foreground.gamma_multiply(0.65))))
                        .on_hover_text("Suggest a feature on GitHub")
                        .clicked()
                    {
                        crate::system_info::request_issue(&ctx);
                    }
                    // This row flows right to left, placing the divider left of the button.
                    ui.add(egui::Separator::default().vertical().spacing(16.));
                    let (text, color) = if self.save_error.is_some() {
                        ("Unsaved", Color32::from_rgb(243, 177, 145))
                    } else if self.dirty_since.is_some() {
                        ("Saving…", Color32::from_rgb(190, 202, 188))
                    } else if self.storage_recovery {
                        ("New workspace", Color32::from_rgb(190, 202, 188))
                    } else if self.project.modified {
                        ("Modified · saved locally", Color32::from_rgb(190, 202, 188))
                    } else {
                        ("Saved locally", Color32::from_rgb(190, 202, 188))
                    };
                    let color = if dark {
                        color
                    } else if self.save_error.is_some() {
                        colors.accent
                    } else {
                        colors.muted
                    };
                    if !compact {
                        let saved = ui.text_label(RichText::new(text).size(12.).color(color));
                        if let Some(error) = &self.save_error {
                            saved.on_hover_text(error);
                        } else {
                            saved.on_hover_text(if self.project.modified {
                                "Autosaved on this device. Use File → Save to update your .ketchup file."
                            } else {
                                "Your model stays on this device"
                            });
                        }
                    }
                    ui.scope(|ui| {
                        dot(
                            ui,
                            if self.save_error.is_some() {
                                ACCENT
                            } else {
                                Color32::from_rgb(152, 183, 142)
                            },
                        );
                    })
                    .response
                    .on_hover_text(self.save_error.as_deref().unwrap_or(text));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let (logo, _) =
                            ui.allocate_exact_size(egui::vec2(35., 35.), Sense::hover());
                        icons::logo(ui.painter(), logo);
                        ui.add_space(4.);
                        if !compact {
                            ui.text_label(RichText::new("SketCraft").size(23.).strong().color(foreground));
                        }
                        ui.add_space(18.);
                        Self::app_bar_menu(ui, "File", |ui| {
                            ui.visuals_mut().override_text_color = Some(colors.ink);
                            for (label, modifiers, key, action) in [
                                ("New project", egui::Modifiers::COMMAND, Key::N, 0),
                                ("Open…", egui::Modifiers::COMMAND, Key::O, 1),
                                ("Save", egui::Modifiers::COMMAND, Key::S, 2),
                                ("Save As…", egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::S, 3),
                            ] {
                                let shortcut = ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, key));
                                if ui.add(egui::Button::new(label).shortcut_text(shortcut)).clicked() {
                                    match action {
                                        0 => self.request_project(projects::Action::New, &ctx),
                                        1 => self.request_project(projects::Action::Open, &ctx),
                                        2 => self.save_project(&ctx, false),
                                        _ => self.save_project(&ctx, true),
                                    }
                                    ui.close();
                                }
                                if action == 1 {
                                    self.recent_projects_menu(ui, &ctx);
                                }
                            }
                            if self.account.sharing_available() && ui.button(self.share_menu_label()).clicked() { self.start_sharing(&ctx); ui.close(); }
                            ui.separator();
                            ui.menu_button("Import", |ui| {
                                let all = ui.add(
                                    egui::Button::new("All supported files…")
                                        .shortcut_text(ctx.format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::I))),
                                ).on_hover_text("SketchUp, STL, OBJ, DXF, 3MF and glTF. The file's extension chooses the format.");
                                if all.clicked() && !self.project.blocked() {
                                    self.project.import_format = "any".into();
                                    self.import_model(&ctx);
                                    ui.close();
                                }
                                ui.separator();
                                for (extension, label, help) in [
                                    ("skp", "SketchUp (.skp)…", "Editable groups, holes and solid colors. No textures or annotations."),
                                    ("stl", "STL (.stl; millimetres)…", "ASCII or binary. Geometry only; coordinates interpreted as millimetres, Z up."),
                                    ("obj", "Wavefront OBJ (.obj; metres)…", "Faces and lines, including negative vertex indices. Metres, Z up. No materials, textures or group hierarchy."),
                                    ("dxf", "DXF 2D (.dxf)…", "Text DXF: LINE, straight LWPOLYLINE, ARC and CIRCLE in XY. Curves approximated with up to 96 segments per circle. No blocks, bulges or widths. Declared units; unitless files use millimetres."),
                                    ("3mf", "3MF (.3mf)…", "Meshes, components and build items with their transforms, in the file's declared unit (millimetres by default). No colors, materials or textures."),
                                    ("glb", "glTF binary (.glb)…", "Triangle meshes placed by the default scene's nodes. Metres, converted from Y up to Z up. No materials, textures or animation."),
                                    ("gltf", "glTF (.gltf; embedded buffers)…", "As .glb, for .gltf files whose buffers are embedded. Files that load separate .bin files are not supported."),
                                ] {
                                    if ui.button(label).on_hover_text(help).clicked() && !self.project.blocked() {
                                        self.project.import_format = extension.into();
                                        self.import_model(&ctx);
                                        ui.close();
                                    }
                                }
                            });
                            ui.menu_button("Export", |ui| {
                                for (stl, label, help) in [
                                    (false, "Wavefront OBJ (.obj)…", "Visible edges and triangulated faces in metres, Z up. Geometry only."),
                                    (true, "STL (.stl; millimetres)…", "Binary STL of visible faces in millimetres, Z up. No edges or materials. Ensure solids are closed before printing; no automatic repair."),
                                ] {
                                    if ui.button(label).on_hover_text(help).clicked() && !self.project.blocked() {
                                        self.project.export_stl = stl;
                                        self.export_model(&ctx);
                                        ui.close();
                                    }
                                }
                            });
                            ui.separator();
                            if ui.button("Settings…").clicked() {
                                self.settings_open = true;
                                ui.close();
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                ui.separator();
                                if ui.button("Quit").clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    ui.close();
                                }
                            }
                        });
                        Self::app_bar_menu(ui, "Edit", |ui| {
                            ui.visuals_mut().override_text_color = Some(colors.ink);
                            Self::configure_entity_menu(ui);
                            for (redo, action, label, modifiers) in [
                                (false, "Undo", self.doc.history.undo_label().map(str::to_owned), egui::Modifiers::COMMAND),
                                (true, "Redo", self.doc.history.redo_label().map(str::to_owned), egui::Modifiers::COMMAND | egui::Modifiers::SHIFT),
                            ] {
                                let enabled = label.is_some();
                                let label = label.map(|label| format!("{action} {label}")).unwrap_or_else(|| action.to_owned());
                                let shortcut = ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, Key::Z));
                                if ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut)).clicked() {
                                    self.undo(&ctx, redo);
                                    ui.close();
                                }
                            }
                            ui.separator();
                            self.context_position = None;
                            self.entity_menu(ui, |_, _| {});
                        });
                        Self::app_bar_menu(ui, "View", |ui| {
                            ui.visuals_mut().override_text_color = Some(colors.ink);
                            self.view_menu(ui);
                        });
                        Self::app_bar_menu(ui, "Generate", |ui| {
                            ui.visuals_mut().override_text_color = Some(colors.ink);
                            if ui.add_enabled(!self.project.blocked(), egui::Button::new("Prepare cutting plan…"))
                                .on_hover_text("Summarize matching parts and export finished cutting dimensions for the selection or whole project.")
                                .clicked() {
                                self.prepare_cutting_plan();
                                ui.close();
                            }
                            if ui.add_enabled(!self.project.blocked(), egui::Button::new("Screenshot")).clicked() {
                                self.screenshot.open = true;
                                ui.close();
                            }
                        });
                        Self::app_bar_menu(ui, "Help", |ui| {
                            ui.visuals_mut().override_text_color = Some(colors.ink);
                            if ui.button("Keyboard shortcuts").clicked() {
                                self.help = true;
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Report a bug").clicked() {
                                crate::system_info::request_issue(&ctx);
                                ui.close();
                            }
                            if ui.button("GitHub").clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab("https://github.com/Ketchup3D/app"));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Download desktop app").clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab("https://ketchup3d.github.io/app/"));
                                ui.close();
                            }
                            if ui.button("Release notes").clicked() {
                                self.release_notes.open();
                                ui.close();
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            if ui.button("Check for updates…").clicked() {
                                self.updates.open = true;
                                self.updates.check(&ctx, false);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("System information").clicked() {
                                crate::system_info::open(&ctx);
                                ui.close();
                            }
                            if ui.button("About SketCraft").clicked() {
                                self.about = true;
                                ui.close();
                            }
                        });
                        let (separator, _) = ui.allocate_exact_size(egui::vec2(32., 24.), Sense::hover());
                        ui.painter().vline(
                            separator.center().x,
                            separator.y_range(),
                            Stroke::new(1., if dark { colors.border } else { colors.muted.gamma_multiply(0.8) }),
                        );
                        ui.scope(|ui| {
                            // Match the axis-lock buttons despite the AppBar's flat menu style.
                            ui.visuals_mut().widgets.inactive = ctx.global_style().visuals.widgets.inactive;
                            for (redo, kind, action, label) in [
                                (false, "undo", "Undo", self.doc.history.undo_label().map(str::to_owned)),
                                (true, "redo", "Redo", self.doc.history.redo_label().map(str::to_owned)),
                            ] {
                                let enabled = label.is_some();
                                let tooltip = label
                                    .map(|label| format!("{action} {label}"))
                                    .unwrap_or_else(|| action.to_owned());
                                let response = ui.add_enabled(
                                    enabled,
                                    egui::Button::new("").min_size(egui::vec2(32., 32.)).stroke(Stroke::new(1., foreground.gamma_multiply(0.65))),
                                );
                                icons::line_icon(
                                    ui.painter(),
                                    response.rect.center(),
                                    kind,
                                    foreground.gamma_multiply(if enabled { 1. } else { 0.3 }),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::Role::Button, enabled, &tooltip)
                                });
                                if response.on_hover_text(&tooltip).clicked() {
                                    self.undo(&ctx, redo);
                                }
                            }
                        });
                        let (separator, _) = ui.allocate_exact_size(egui::vec2(32., 24.), Sense::hover());
                        ui.painter().vline(
                            separator.center().x,
                            separator.y_range(),
                            Stroke::new(1., if dark { colors.border } else { colors.muted.gamma_multiply(0.8) }),
                        );
                        ui.add(crate::natural_text::Label::new(RichText::new(&self.doc.name).color(foreground)).truncate())
                            .on_hover_text(&self.doc.name);
                    });
                });
            });
    }
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        let ctx = ui.ctx().clone();
        egui::Panel::bottom("status")
            .exact_size(42.)
            .frame(
                egui::Frame::new()
                    .fill(colors.paper)
                    .inner_margin(egui::Margin::symmetric(16, 5)),
            )
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let drawing_orbit = self.drawing_orbit_modifier(ctx.input(|i| i.modifiers));
                    let navigating = self
                        .gesture
                        .as_ref()
                        .is_some_and(|g| matches!(g.kind, GestureKind::Navigate { .. }));
                    let shared_status = self.shared_status(ctx.input(|i|i.time));
                    let message = if let Some(status) = shared_status.as_deref() {
                        status
                    } else if drawing_orbit {
                        "Move the cursor to orbit · Hold Shift to pan · Release Ctrl / ⌘ to resume drawing"
                    } else if navigating {
                        if ctx.input(|i| i.modifiers.shift) {
                            "Panning · Release Shift to orbit"
                        } else {
                            "Orbiting · Hold Shift to pan"
                        }
                    } else if self.paste_preview.is_some() {
                        "Click to place the copy · Escape to cancel"
                    } else if self.tool == Tool::Rotate {
                        self.rotation_status()
                    } else if self.tool == Tool::Move {
                        if self.last_move_copy.is_some() {
                            "Copy placed · Type /3 to divide the span or 3x to repeat · Enter to apply"
                        } else {
                            match (ctx.os() == egui::os::OperatingSystem::Mac, self.move_copy) {
                                (true, true) => "Copy · Option toggles Move · Click to place · Type distance + Enter",
                                (true, false) => "Move · Option toggles Copy · Click base point and destination",
                                (false, true) => "Copy · Ctrl toggles Move · Click to place · Type distance + Enter",
                                (false, false) => "Move · Ctrl toggles Copy · Click base point and destination",
                            }
                        }
                    } else if self.tool.is_solid() {
                        self.solid_status()
                    } else if self.tool.is_measuring() {
                        self.measurement_status()
                    } else if self.tool == Tool::Paint {
                        self.paint_instructions()
                    } else if self.tool == Tool::Line && self.line_start.is_some() {
                        "Click to finish · Type length + Enter · Esc to stop"
                    } else if self.drawing.is_some() && self.tool == Tool::Rectangle {
                        "Click the opposite corner · Type width,height (5m,50mm) + Enter"
                    } else if self.drawing.is_some()
                        && matches!(self.tool, Tool::Circle | Tool::Polygon)
                    {
                        "Click to set the radius · Type a radius (5cm) + Enter · Esc to cancel"
                    } else if self.drawing.is_some() && self.tool == Tool::Freehand {
                        "Release to finish · Type total length + Enter · Esc to cancel"
                    } else {
                        self.tool.instructions()
                    };
                    // Reserve the precision controls before fitting the status message.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.tool == Tool::Text { return; }
                        if self.tool.is_measuring() {
                            ui.text_label(
                                RichText::new(
                                    self.measurement_value().unwrap_or_else(|| "—".into()),
                                )
                                .monospace()
                                .strong(),
                            );
                            ui.text_label(
                                RichText::new(if self.tool == Tool::Protractor {
                                    "Angle"
                                } else {
                                    "Distance"
                                })
                                .size(12.)
                                .color(colors.muted),
                            );
                            return;
                        }
                        if self.tool == Tool::Scale {
                            ui.text_label(RichText::new("×").color(colors.muted));
                        }
                        let available = self.precision_available();
                        let hint = if let Some(edit) = &self.modeling {
                            if self.tool == Tool::Scale { format!("{:.3}", edit.value) } else if self.tool == Tool::Rotate { self.doc.display.units.format_angle(edit.value) } else { self.doc.display.units.format_length(edit.value) }
                        } else if self.last_move_copy.is_some() {
                            "/3 or 3x".into()
                        } else if self.drawing.is_some() && self.tool == Tool::Rectangle {
                            "5m,50mm".into()
                        } else if self.drawing.is_some() {
                            "5cm".into()
                        } else if let (Some(a), Some(b)) = (self.line_start, self.snap) {
                            self.doc.display.units.format_length(a.distance(b.point))
                        } else {
                            self.doc.display.units.format_length(0.)
                        };
                        let field = ui.add_enabled(
                            available,
                            egui::TextEdit::singleline(&mut self.length_input)
                                .desired_width(if self.tool == Tool::Rectangle { 300. } else { 212. })
                                .font(egui::TextStyle::Monospace)
                                .hint_text(hint)
                                .char_limit(precision::MAX_INPUT_LEN)
                                .margin(egui::vec2(9., 6.)),
                        ).on_hover_text(if self.tool == Tool::Rotate {
                            format!("Enter an angle or formula, such as 90 or (45 + 45) / 2. Plain numbers use {}. Add deg, °, rad, or gon after the formula to override the unit.", self.doc.display.units.angle.label())
                        } else {
                            format!("Use lengths or formulas: 60cm - 89mm, (1m + 20cm) / 2. Operators: + - * / and parentheses. Plain numbers default to {}; multipliers are unitless. Units: mm, cm, m, km, in, ft, yd. Rectangles accept one formula per dimension, separated by a comma. Scale uses numbers without units.", self.doc.display.units.length.label())
                        });
                        if (field.has_focus() || field.lost_focus())
                            && ctx.input(|i| i.key_pressed(Key::Enter))
                        {
                            if self.modeling.is_some() {
                                self.commit_modeling(&ctx);
                            } else if self.last_move_copy.is_some() {
                                self.commit_move_array(&ctx);
                            } else if self.drawing.is_some() {
                                self.commit_drawing(&ctx);
                            } else {
                                self.commit_line(&ctx, true);
                            }
                            field.surrender_focus();
                        }
                        ui.text_label(
                            RichText::new(match self.tool {
                                Tool::Scale => "Factor",
                                Tool::Rotate => "Angle",
                                Tool::PushPull => "Distance",
                                Tool::Offset => "Offset",
                                Tool::Move if self.last_move_copy.is_some() => "Copies",
                                Tool::Move => "Distance",
                                Tool::Rectangle => "Width, height",
                                Tool::Circle | Tool::Polygon => "Radius",
                                _ => "Length",
                            })
                            .size(12.)
                            .color(colors.muted),
                        );
                    });
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            dot(ui, ACCENT);
                            ui.add(
                                crate::natural_text::Label::new(
                                    RichText::new(message).size(12.).color(colors.muted),
                                )
                                .truncate(),
                            );
                        },
                    );
                });
            });
    }
    pub(super) fn tool_tooltip(&self, response: egui::Response, tool: Tool) -> egui::Response {
        response.on_hover_ui(|ui| {
            let title = if self.general_settings.tool_shortcut(tool).is_empty() {
                tool.label().to_owned()
            } else {
                format!(
                    "{} ({})",
                    tool.label(),
                    self.general_settings.tool_shortcut(tool)
                )
            };
            ui.text_strong(title);
            ui.text_label(tool.instructions());
        })
    }
    pub(super) fn tool_menu_item(&mut self, ui: &mut egui::Ui, tool: Tool, active: bool) -> egui::Response {
        let colors = Palette::for_ui(ui);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::RadioButton, true, active, tool.label())
        });
        if active || response.contains_pointer() || response.has_focus() {
            ui.painter().rect_filled(
                rect,
                5,
                if active {
                    colors.selected
                } else {
                    colors.hovered
                },
            );
        }
        let color = if active { colors.accent } else { colors.ink };
        icons::ui_tool(
            ui.painter(),
            Rect::from_center_size(
                egui::pos2(rect.left() + 18., rect.center().y),
                egui::vec2(22., 22.),
            ),
            tool,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 38., rect.center().y),
            Align2::LEFT_CENTER,
            tool.label(),
            FontId::proportional(13.),
            color,
        );
        let shortcut = self.general_settings.tool_shortcut(tool);
        let shortcut_size = ui.painter().text(
            egui::pos2(rect.right() - 8., rect.center().y),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(11.),
            colors.muted,
        );
        if tool == Tool::Polygon {
            let input_rect = Rect::from_min_size(
                egui::pos2(
                    shortcut_size.left() - ui.spacing().item_spacing.x - 48.,
                    rect.top(),
                ),
                egui::vec2(48., rect.height()),
            );
            // Center the widget's padded height within the row without advancing
            // the menu cursor a second time.
            let mut input_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(input_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            input_ui
                .add(
                    egui::DragValue::new(&mut self.polygon_sides)
                        .range(3..=64)
                        .min_size(egui::vec2(48., 0.)),
                )
                .on_hover_text("Polygon sides");
        }
        self.tool_tooltip(response, tool)
    }
    pub(super) fn tool_rail(&mut self, ui: &mut egui::Ui) {
        if self.large_tool_set {
            return self.large_tool_rail(ui);
        }
        let colors = Palette::for_ui(ui);
        egui::Panel::left("tools")
            .exact_size(76.)
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
                    // Reserve a shape behind every button, then resolve the final selection
                    // after clicks and nested menus have activated their tool this frame.
                    let selection_shape = ui.painter().add(egui::Shape::Noop);
                    let content_top = ui.cursor().top();
                    let mut tool_rects = Vec::with_capacity(Tool::RAIL.len());
                    for rail_tool in Tool::RAIL {
                        let tool = if rail_tool == Tool::Line {
                            self.drawing_tool
                        } else if rail_tool == Tool::Dimensions {
                            self.measurement_tool
                        } else if rail_tool == Tool::Union {
                            self.solid_tool
                        } else {
                            rail_tool
                        };
                        let active = self.tool == tool;
                        let nested =
                            matches!(rail_tool, Tool::Line | Tool::Dimensions | Tool::Union);
                        let tooltip = if self.general_settings.tool_shortcut(tool).is_empty() {
                            tool.label().to_owned()
                        } else {
                            format!(
                                "{} ({})",
                                tool.label(),
                                self.general_settings.tool_shortcut(tool)
                            )
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(56., 55.), Sense::click());
                        tool_rects.push((rail_tool, rect));
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                if nested {
                                    egui::Role::ComboBox
                                } else {
                                    egui::Role::RadioButton
                                },
                                true,
                                active,
                                tooltip.clone(),
                            )
                        });
                        if !active && response.hovered() {
                            ui.painter().rect_filled(
                                rect.shrink2(egui::vec2(0., 3.)),
                                8,
                                colors.hovered,
                            );
                        }
                        let color = if active { colors.accent } else { colors.ink };
                        icons::ui_tool(
                            ui.painter(),
                            Rect::from_center_size(
                                rect.center() - egui::vec2(if nested { 4. } else { 0. }, 8.),
                                egui::vec2(27., 27.),
                            ),
                            tool,
                        );
                        ui.painter().text(
                            rect.center() + egui::vec2(0., 17.),
                            Align2::CENTER_CENTER,
                            if tool == Tool::TapeMeasure {
                                "Tape"
                            } else {
                                tool.label()
                            },
                            FontId::proportional(
                                if matches!(
                                    tool,
                                    Tool::Protractor | Tool::Dimensions | Tool::OuterShell
                                ) {
                                    9.
                                } else {
                                    10.
                                },
                            ),
                            color,
                        );
                        if nested {
                            let tip = rect.right_center() + egui::vec2(-5., -8.);
                            ui.painter().add(egui::Shape::line(
                                vec![tip + egui::vec2(-4., -4.), tip, tip + egui::vec2(-4., 4.)],
                                Stroke::new(1.8, color),
                            ));
                            egui::Popup::menu(&response)
                                .align(egui::RectAlign::RIGHT_START)
                                .width(190.)
                                .show(|ui| {
                                    let options: &[Tool] = match rail_tool {
                                        Tool::Line => &Tool::DRAWING,
                                        Tool::Dimensions => &Tool::MEASURING,
                                        Tool::Union => &Tool::SOLID,
                                        _ => unreachable!(),
                                    };
                                    for &option in options {
                                        let response =
                                            self.tool_menu_item(ui, option, self.tool == option);
                                        if response.clicked() {
                                            if option.is_solid() {
                                                self.start_solid_tool(option, ui.ctx());
                                            } else {
                                                self.set_tool(option);
                                            }
                                            ui.close();
                                        }
                                    }
                                });
                        }
                        if self.tool_tooltip(response, tool).clicked() && !nested {
                            self.set_tool(tool);
                        }
                        ui.add_space(5.);
                    }
                    let selected_rail_tool = if self.tool.is_solid() {
                        Tool::Union
                    } else if Tool::DRAWING.contains(&self.tool) {
                        Tool::Line
                    } else if Tool::MEASURING.contains(&self.tool) {
                        Tool::Dimensions
                    } else {
                        self.tool
                    };
                    if let Some((_, target)) = tool_rects
                        .iter()
                        .find(|(tool, _)| *tool == selected_rail_tool)
                    {
                        // Animate only the content-relative offset: scrolling must not
                        // become another animation target. Zero-duration styles snap.
                        let offset = ui.ctx().animate_value_with_time(
                            ui.scope_id().with("tool_rail_selection"),
                            target.top() - content_top,
                            ui.style().animation_time.min(0.14),
                        );
                        let rect =
                            target.translate(egui::vec2(0., content_top + offset - target.top()));
                        ui.painter().set(
                            selection_shape,
                            egui::Shape::Vec(vec![
                                egui::Shape::rect_filled(
                                    rect.shrink2(egui::vec2(0., 3.)),
                                    8,
                                    colors.selected,
                                ),
                                egui::Shape::rect_filled(
                                    Rect::from_min_size(
                                        rect.left_center() - egui::vec2(10., 12.),
                                        egui::vec2(3., 24.),
                                    ),
                                    2,
                                    colors.accent,
                                ),
                            ]),
                        );
                    }
                });
                sidebar::remember_scroll(ui.ctx(), "sidebar_left", scroll.state.clamped_offset());
            });
    }
    pub(super) fn paint_instructions(&self) -> &'static str {
        match self.paint_target {
            PaintTarget::Face => "Click a face to apply this color.",
            PaintTarget::Group => "Click a face in a group or component to paint all its faces.",
        }
    }
}
