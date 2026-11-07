//! Inspector panel contents: materials and entity info.
use super::*;

impl SketCraftApp {
    pub(super) fn materials_contents(&mut self, ui: &mut egui::Ui) {
        let objects: Vec<_> = self
            .selected
            .iter()
            .copied()
            .filter(|id| self.doc.model.objects.contains_key(id))
            .collect();
        if objects.is_empty() {
            crate::empty_state::EmptyState::new("No objects selected",
                "Select a group or component to edit its material, sanding, and finishing.").show(ui);
            return;
        }
        let first = self.doc.model.objects[&objects[0]].stock_material_id;
        let mixed = objects
            .iter()
            .any(|id| self.doc.model.objects[id].stock_material_id != first);
        let mut chosen = first;
        let mut assign = false;
        ui.add_enabled_ui(!self.doc.model.selection_locked(&self.selected), |ui| {
            ui.text_label("Stock material");
            let label = if mixed {
                "Mixed"
            } else if let Some(id) = first {
                self.general_settings
                    .stock
                    .wood
                    .iter()
                    .find(|stock| stock.id == id)
                    .map(|stock| stock.name.as_str())
                    .or_else(|| {
                        self.doc
                            .model
                            .stock_materials
                            .get(&id)
                            .map(|material| material.name.as_str())
                    })
                    .unwrap_or("Missing material")
            } else {
                "Unassigned"
            };
            assign = stock_settings::material_selector(
                ui,
                label,
                &mut chosen,
                &self.general_settings.stock.wood,
                &self.doc.model.stock_materials,
                &mut self.stock_material_filter,
            )
            .inner;
        });
        if assign && (mixed || chosen != first) {
            let before = self.doc.model.clone();
            if let Some(material_id) = chosen
                && let Some(stock) = self
                    .general_settings
                    .stock
                    .wood
                    .iter()
                    .find(|stock| stock.id == material_id)
            {
                self.doc
                    .model
                    .stock_materials
                    .insert(material_id, stock.definition());
            }
            for id in objects {
                let object = self.doc.model.objects.get_mut(&id).unwrap();
                object.stock_material_id = chosen;
                object.stock_material = None;
            }
            self.finish_edit("Assign stock material", before, ui.ctx());
        }
        ui.add_space(12.);
        self.surface_treatment_info(ui);
    }

    pub(super) fn entity_info_contents(&mut self, ui: &mut egui::Ui) {
        if !self.selected.is_empty()
            && let Some(size) = self
                .solid_volume_cache
                .solid_selection_size(&self.doc.model, &self.selected)
        {
            self.entity_axis_gizmo(ui, size);
            ui.add_space(4.);
        }
        self.solid_inspector(ui);
        self.measurement_inspector(ui);
        self.text_entity_info(ui);
        if self.selected.is_empty() {
            crate::empty_state::EmptyState::new("Nothing selected",
                "Select an entity to see its details.").show(ui);
        } else {
            self.face_cache.update(&self.doc.model);
            let entities = self.doc.model.expand_entities(&self.selected);
            let edges = entities
                .iter()
                .filter(|id| self.doc.model.edges.contains_key(id))
                .count();
            let faces = entities
                .iter()
                .filter(|id| self.doc.model.faces.contains_key(id))
                .count();
            let dimensions = entities
                .iter()
                .filter(|id| self.doc.model.dimensions.contains_key(id))
                .count();
            let texts = entities
                .iter()
                .filter(|id| self.doc.model.texts.contains_key(id))
                .count();
            let noun =
                |n: usize, single: &str| format!("{n} {single}{}", if n == 1 { "" } else { "s" });
            let objects = self
                .selected
                .iter()
                .filter_map(|id| self.doc.model.objects.get(id))
                .collect::<Vec<_>>();
            let selection = if objects.len() == 1 && self.selected.len() == 1 {
                let object = objects[0];
                let kind = if object.kind == ObjectKind::Group {
                    "Group"
                } else {
                    "Component"
                };
                format!("{kind}: {}", object.name)
            } else if !objects.is_empty() {
                noun(self.selected.len(), "entity")
            } else if dimensions > 0 || texts > 0 {
                let mut parts = vec![];
                if edges > 0 {
                    parts.push(noun(edges, "edge"));
                }
                if faces > 0 {
                    parts.push(noun(faces, "face"));
                }
                if dimensions > 0 {
                    parts.push(noun(dimensions, "dimension"));
                }
                if texts > 0 {
                    parts.push(noun(texts, "text label"));
                }
                parts.join(" · ")
            } else if faces == 0 {
                noun(edges, "edge")
            } else if edges == 0 {
                noun(faces, "face")
            } else {
                format!("{} · {}", noun(edges, "edge"), noun(faces, "face"))
            };
            let single_group = self.selected.len() == 1
                && objects.len() == 1
                && objects[0].kind == ObjectKind::Group;
            if !single_group {
                ui.text_label(
                    RichText::new(format!("{selection} selected"))
                        .size(17.)
                        .strong(),
                );
                ui.add_space(12.);
            }
            if self.doc.model.selection_locked(&self.selected) {
                ui.text_label(RichText::new("Locked").color(self.axis_color(Axis::Z)));
            }
            if edges > 0 {
                let length: f64 = entities
                    .iter()
                    .filter_map(|id| self.doc.model.edges.get(id))
                    .map(|edge| self.doc.model.edge_length(edge))
                    .sum();
                Self::property_row(
                    ui,
                    if edges == 1 { "Length" } else { "Total edge length" },
                    &self.doc.display.units.format_length(length),
                );
            }
            if faces > 0 {
                let area: f64 = entities
                    .iter()
                    .filter_map(|id| self.doc.model.faces.get(id))
                    .map(|f| self.face_cache.get(f.id).area)
                    .sum();
                Self::property_row(
                    ui,
                    "Surface area",
                    &self.doc.display.units.format_area(area),
                );
                let side_lengths = |ring: &[u64]| {
                    (0..ring.len())
                        .map(|i| {
                            self.doc.model.vertices[&ring[i]].position.distance(
                                self.doc.model.vertices[&ring[(i + 1) % ring.len()]].position,
                            )
                        })
                        .collect::<Vec<_>>()
                };
                let selected_faces: Vec<_> = entities
                    .iter()
                    .filter_map(|id| self.doc.model.faces.get(id))
                    .collect();
                let perimeter: f64 = selected_faces
                    .iter()
                    .flat_map(|face| std::iter::once(&face.vertices).chain(face.holes.iter()))
                    .flat_map(|ring| side_lengths(ring))
                    .sum();
                Self::property_row(
                    ui,
                    if faces == 1 {
                        "Perimeter"
                    } else {
                        "Total perimeter"
                    },
                    &self.doc.display.units.format_length(perimeter),
                );
                if let [face] = selected_faces.as_slice() {
                    for (index, ring) in std::iter::once(&face.vertices)
                        .chain(face.holes.iter())
                        .enumerate()
                    {
                        let label = if index == 0 {
                            "Side lengths".to_owned()
                        } else {
                            format!("Hole {index} side lengths")
                        };
                        let lengths = side_lengths(ring)
                            .into_iter()
                            .map(|length| self.doc.display.units.format_length(length))
                            .collect::<Vec<_>>()
                            .join(" · ");
                        Self::property_row(ui, &label, &lengths);
                    }
                }
            }
            if self.selected.len() == 1
                && let Some(&id) = self.selected.first()
                && self.doc.model.objects.contains_key(&id)
                && let Ok(volume) = self.solid_volume_cache.volume(&self.doc.model, id)
            {
                Self::property_row(ui, "Volume", &self.doc.display.units.format_volume(volume));
            }
        }
        if self.selected.is_empty() {
            return;
        }
        ui.add_space(8.);
        sidebar::inspector_section(ui, "Appearance", "entity_appearance", true, |ui| {
            self.paint_color_controls(ui, false);
            ui.add_space(8.);
            if ui
                .add_enabled(
                    !self.selected.is_empty()
                        && !self.doc.model.selection_locked(&self.selected),
                    egui::Button::new("Clear Color")
                        .min_size(egui::vec2(ui.available_width(), ui.spacing().interact_size.y)),
                )
                .on_hover_text("Remove color from the selected items and all their children")
                .clicked()
            {
                let before = self.doc.model.clone();
                match self.doc.model.clear_colors(&self.selected) {
                    Ok(()) => self.finish_edit("Clear color", before, ui.ctx()),
                    Err(message) => self.say(ui.ctx(), message),
                }
            }
            ui.text_label(self.paint_instructions());
        });
    }
    pub(super) fn property(ui: &mut egui::Ui, label: &str, value: &str) {
        Self::property_row(ui, label, value);
        ui.add_space(3.);
    }

    pub(super) fn property_row(ui: &mut egui::Ui, label: &str, value: &str) {
        let colors = Palette::for_ui(ui);
        crate::inspector_table::inspector_field(
            ui,
            ("property", label),
            RichText::new(label).size(12.).color(colors.muted),
            |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.add(crate::natural_text::Label::new(RichText::new(value).monospace().size(12.)).wrap())
                })
                .inner
            },
        );
    }

    pub(super) fn inspector_panel(&mut self, ui: &mut egui::Ui) {
        self.connection_sidebar_selection(ui.ctx());
        let fasteners_open = fasteners::AVAILABLE && self.fasteners.open;
        let colors = Palette::for_ui(ui);
        if !self.inspector {
            return;
        }
        if ui.available_width() >= 900. || fasteners_open || self.joinery.open {
            let panel = egui::Panel::right("inspector")
                .default_size(self.inspector_width)
                .size_range(260. ..=ui.available_width() * 0.5)
                .resizable(true)
                .frame(egui::Frame::new().fill(colors.paper))
                .show(ui, |ui| {
                    ui.spacing_mut().scroll.bar_outer_margin = 0.;
                    if fasteners_open {
                        self.automatic_fasteners_sidebar(ui);
                    } else if self.joinery.open {
                        self.wood_joinery_sidebar(ui);
                    } else {
                        self.inspector_contents(ui);
                    }
                });
            self.inspector_width = panel.response.rect.width();
        } else {
            let mut open = true;
            egui::Window::new("Inspector")
                .open(&mut open)
                .default_width(280.)
                .default_height(560.)
                .default_pos(egui::pos2(90., 120.))
                .show(ui.ctx(), |ui| {
                    if fasteners_open {
                        self.automatic_fasteners_sidebar(ui);
                    } else if self.joinery.open {
                        self.wood_joinery_sidebar(ui);
                    } else {
                        self.inspector_contents(ui);
                    }
                });
            if !open {
                self.inspector = false;
            }
        }
    }
}
