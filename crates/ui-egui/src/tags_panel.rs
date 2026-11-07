//! The Tags & Layers management panel: manage tags, visibility, colors, and entity assignments.

use egui::{Color32, Id, RichText, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

use crate::SketApp;
use crate::theme::Tokens;
use crate::ui_kit;

pub const TAG_COLORS: &[[u8; 3]] = &[
    [108, 148, 102], // Sage green
    [95, 143, 177],  // Ocean blue
    [195, 96, 67],   // Terracotta
    [163, 121, 167], // Lavender
    [225, 179, 87],  // Ochre gold
    [72, 169, 166],  // Teal
    [212, 110, 137], // Coral rose
    [120, 135, 171], // Slate blue
    [180, 140, 100], // Sand
    [130, 170, 140], // Mint
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagItem {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagsData {
    pub tags: Vec<TagItem>,
    #[serde(default)]
    pub assignments: BTreeMap<String, u64>,
    #[serde(default)]
    pub color_by_tag: bool,
}

impl Default for TagsData {
    fn default() -> Self {
        Self {
            tags: vec![TagItem { id: 1, name: "Untagged".into(), visible: true, color: [140, 140, 140] }],
            assignments: BTreeMap::new(),
            color_by_tag: false,
        }
    }
}

pub fn load_tags(app: &SketApp) -> TagsData {
    if let Some(val) = app.session.doc.metadata.0.get("sketcraft.tags") {
        if let Ok(data) = serde_json::from_value::<TagsData>(val.clone()) {
            if !data.tags.is_empty() {
                return data;
            }
        }
    }
    TagsData::default()
}

pub fn save_tags(app: &mut SketApp, data: &TagsData) {
    if let Ok(v) = serde_json::to_value(data) {
        let _ = app.run("document.metadata.set", json!({"metadata": {"sketcraft.tags": v}}));
    }
}

pub fn get_selected_entities(app: &SketApp) -> Vec<String> {
    let mut entities = Vec::new();
    for sel in &app.session.selection {
        match sel {
            sketcraft_engine::Sel::Body { name } => {
                if !entities.contains(name) {
                    entities.push(name.clone());
                }
            }
            sketcraft_engine::Sel::Face { body, .. } | sketcraft_engine::Sel::Edge { body, .. } => {
                if !entities.contains(body) {
                    entities.push(body.clone());
                }
            }
            _ => {}
        }
    }
    entities
}

pub fn get_all_bodies(app: &SketApp) -> Vec<String> {
    app.session.model.state().bodies.iter().map(|b| b.name.clone()).collect()
}

pub fn apply_tag_visibility(app: &mut SketApp, data: &TagsData) {
    let hidden_tags: HashSet<u64> = data.tags.iter().filter(|t| !t.visible).map(|t| t.id).collect();
    let all_bodies = get_all_bodies(app);
    let mut to_hide = Vec::new();
    let mut to_show = Vec::new();
    for body in all_bodies {
        let tag_id = data.assignments.get(&body).copied().unwrap_or(1);
        if hidden_tags.contains(&tag_id) {
            to_hide.push(body);
        } else {
            to_show.push(body);
        }
    }
    if !to_hide.is_empty() {
        let _ = app.run("browser.visibility", json!({"items": to_hide, "visible": false}));
    }
    if !to_show.is_empty() {
        let _ = app.run("browser.visibility", json!({"items": to_show, "visible": true}));
    }
}

pub fn apply_color_by_tag(app: &mut SketApp, data: &TagsData) {
    if !data.color_by_tag {
        return;
    }
    for (body, tag_id) in &data.assignments {
        if let Some(tag) = data.tags.iter().find(|t| t.id == *tag_id) {
            let _ = app.run(
                "appearance.assign",
                json!({
                    "bodies": [body],
                    "color": [tag.color[0], tag.color[1], tag.color[2]],
                }),
            );
        }
    }
}

#[derive(Default)]
pub struct TagsPanel {
    pub new_name: String,
    pub editing_tag: Option<u64>,
    pub edit_buffer: String,
}

pub fn open(app: &mut SketApp) {
    app.tags_panel = Some(TagsPanel::default());
}

pub fn show(app: &mut SketApp, ctx: &egui::Context) {
    let Some(mut panel) = app.tags_panel.take() else { return };
    let t = Tokens::get();
    let mut data = load_tags(app);
    let mut data_changed = false;
    let mut sync_visibility = false;
    let mut sync_colors = false;

    let selected_entities = get_selected_entities(app);
    let all_bodies = get_all_bodies(app);

    let (closed, _) = ui_kit::Window::new(Id::unique("sc_tags_panel"), "list", crate::i18n::tr_label("Tags & Layers"))
        .cards(2.8)
        .actions("", crate::i18n::tr("ui.close"), None)
        .show(ctx, true, |ui| {
            // Header: New Tag input & Color by Tag toggle
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr_label("New tag:"));
                let text_resp = ui.add(egui::TextEdit::singleline(&mut panel.new_name).hint_text("e.g. Walls, Furniture").desired_width(130.0));
                let enter = text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("+ Add Tag").clicked() || enter) && !panel.new_name.trim().is_empty() {
                    let name = panel.new_name.trim().to_string();
                    if !data.tags.iter().any(|t| t.name.eq_ignore_ascii_case(&name)) {
                        let next_id = data.tags.iter().map(|t| t.id).max().unwrap_or(1) + 1;
                        let color = TAG_COLORS[(data.tags.len().saturating_sub(1)) % TAG_COLORS.len()];
                        data.tags.push(TagItem { id: next_id, name, visible: true, color });
                        panel.new_name.clear();
                        data_changed = true;
                    }
                }

                ui.separator();

                if ui_kit::checkbox(ui, "Color by Tag", &mut data.color_by_tag).changed() {
                    data_changed = true;
                    sync_colors = true;
                }
            });

            ui.add_space(t.space_6);

            // Active selection tag assignment section
            if !selected_entities.is_empty() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let desc = if selected_entities.len() == 1 {
                            format!("Selected: {}", selected_entities[0])
                        } else {
                            format!("Selected: {} items", selected_entities.len())
                        };
                        ui.label(RichText::new(desc).strong());

                        let current_tag_id = selected_entities.first().and_then(|ent| data.assignments.get(ent).copied()).unwrap_or(1);
                        let current_tag_name = data.tags.iter().find(|t| t.id == current_tag_id).map(|t| t.name.as_str()).unwrap_or("Untagged");

                        ui.label("Tag:");
                        egui::ComboBox::from_id_salt("assign_tag_combo").selected_text(current_tag_name).show_ui(ui, |ui| {
                            for tag in &data.tags {
                                let is_selected = tag.id == current_tag_id;
                                if ui.selectable_label(is_selected, &tag.name).clicked() {
                                    for ent in &selected_entities {
                                        if tag.id == 1 {
                                            data.assignments.remove(ent);
                                        } else {
                                            data.assignments.insert(ent.clone(), tag.id);
                                        }
                                    }
                                    data_changed = true;
                                    sync_visibility = true;
                                    sync_colors = true;
                                }
                            }
                        });
                    });
                });
                ui.add_space(t.space_6);
            }

            ui.separator();

            // Tags table
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                egui::Grid::new("tags_table_grid").num_columns(5).spacing(vec2(10.0, 6.0)).min_col_width(24.0).show(ui, |ui| {
                    // Header
                    ui.label(RichText::new("Vis").strong());
                    ui.label(RichText::new("Color").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Count").strong());
                    ui.label(RichText::new("Actions").strong());
                    ui.end_row();

                    let mut delete_tag = None;
                    let mut select_tag = None;
                    let existing_names: Vec<(u64, String)> = data.tags.iter().map(|t| (t.id, t.name.to_lowercase())).collect();

                    for tag in &mut data.tags {
                        // 1. Visibility toggle (eye)
                        let eye_label = if tag.visible { "👁" } else { "👁‍🗨" };
                        let eye_color = if tag.visible { t.text } else { t.text_dim };
                        let eye_resp = ui.add(egui::Button::new(RichText::new(eye_label).color(eye_color)).frame(false).sense(Sense::click()));
                        if eye_resp.on_hover_text("Toggle tag visibility").clicked() {
                            tag.visible = !tag.visible;
                            data_changed = true;
                            sync_visibility = true;
                        }

                        // 2. Color swatch
                        let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                        let color = Color32::from_rgb(tag.color[0], tag.color[1], tag.color[2]);
                        ui.painter().rect_filled(rect, 3.0, color);
                        ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, t.dialog_border), egui::StrokeKind::Middle);
                        if resp.on_hover_text("Click to cycle color").clicked() {
                            let cur_idx = TAG_COLORS.iter().position(|&c| c == tag.color).unwrap_or(0);
                            tag.color = TAG_COLORS[(cur_idx + 1) % TAG_COLORS.len()];
                            data_changed = true;
                            sync_colors = true;
                        }

                        // 3. Name (with inline edit)
                        if panel.editing_tag == Some(tag.id) {
                            let edit_resp = ui.add(egui::TextEdit::singleline(&mut panel.edit_buffer).desired_width(120.0));
                            if edit_resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let new_name = panel.edit_buffer.trim().to_string();
                                let folded = new_name.to_lowercase();
                                if !new_name.is_empty() && (tag.id == 1 || !existing_names.iter().any(|(id, n)| *id != tag.id && *n == folded)) {
                                    tag.name = new_name;
                                    data_changed = true;
                                }
                                panel.editing_tag = None;
                            }
                        } else {
                            let name_resp = ui.add(egui::Label::new(&tag.name).sense(Sense::click()));
                            if name_resp.on_hover_text("Click to rename").clicked() {
                                panel.editing_tag = Some(tag.id);
                                panel.edit_buffer = tag.name.clone();
                            }
                        }

                        // 4. Count of assigned bodies
                        let count = if tag.id == 1 {
                            all_bodies.iter().filter(|b| data.assignments.get(*b).copied().unwrap_or(1) == 1).count()
                        } else {
                            data.assignments.values().filter(|&&id| id == tag.id).count()
                        };
                        ui.label(RichText::new(count.to_string()).color(t.text_dim));

                        // 5. Actions: Select entities, Delete tag
                        ui.horizontal(|ui| {
                            if count > 0 && ui.button("Select").on_hover_text("Select all entities on this tag").clicked() {
                                select_tag = Some(tag.id);
                            }
                            if tag.id != 1 {
                                let del_resp = ui.add(egui::Button::new("🗑").frame(false));
                                if del_resp.on_hover_text("Delete tag (reassigns to Untagged)").clicked() {
                                    delete_tag = Some(tag.id);
                                }
                            }
                        });

                        ui.end_row();
                    }

                    if let Some(tag_id) = delete_tag {
                        data.tags.retain(|t| t.id != tag_id);
                        data.assignments.retain(|_, id| *id != tag_id);
                        data_changed = true;
                        sync_visibility = true;
                    }

                    if let Some(tag_id) = select_tag {
                        let to_select: Vec<String> = all_bodies
                            .iter()
                            .filter(|b| {
                                if tag_id == 1 {
                                    data.assignments.get(*b).copied().unwrap_or(1) == 1
                                } else {
                                    data.assignments.get(*b).copied() == Some(tag_id)
                                }
                            })
                            .cloned()
                            .collect();
                        if !to_select.is_empty() {
                            app.session.selection = to_select.into_iter().map(|name| sketcraft_engine::Sel::Body { name }).collect();
                        }
                    }
                });
            });
        });

    if sync_visibility {
        apply_tag_visibility(app, &data);
    }

    if sync_colors {
        apply_color_by_tag(app, &data);
    }

    if data_changed {
        save_tags(app, &data);
    }

    if !closed {
        app.tags_panel = Some(panel);
    }
}
