//! 3D Warehouse and Architectural Model Discovery panel.
//! Provides model search, discovery, and one-click import of SketchUp, 3MF, OBJ, and glTF models.

use egui::{vec2, Color32, Id, RichText, Sense};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::theme::Tokens;
use crate::{ui_kit, SolveApp};

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredModel {
    pub name: String,
    pub path: String,
    pub category: String,
    pub format: String,
    pub size_bytes: u64,
    pub size_formatted: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct WarehousePanel {
    pub search: String,
    pub selected_category: String,
    pub custom_dir: String,
    pub models: Vec<DiscoveredModel>,
    pub filtered_models: Vec<DiscoveredModel>,
    pub selected_idx: Option<usize>,
    pub z_offset_mm: f64,
    pub auto_center: bool,
    pub last_status: Option<String>,
    pub is_scanning: bool,
}

impl Default for WarehousePanel {
    fn default() -> Self {
        let default_dir = detect_default_model_dir();
        let mut panel = Self {
            search: String::new(),
            selected_category: "All".to_string(),
            custom_dir: default_dir,
            models: Vec::new(),
            filtered_models: Vec::new(),
            selected_idx: None,
            z_offset_mm: 0.0,
            auto_center: true,
            last_status: None,
            is_scanning: false,
        };
        panel.scan_models();
        panel
    }
}

fn detect_default_model_dir() -> String {
    // Check known user directories
    let candidates = [
        "/Users/chanthawat/Library/CloudStorage/OneDrive-Personal/Desktop/3D/Ban Bueng - Organized",
        "/Users/chanthawat/Desktop/3D",
        "/Users/chanthawat/Documents/3D",
        "./crates/sketcraft-core/tests/fixtures",
    ];
    for dir in candidates {
        if Path::new(dir).is_dir() {
            return dir.to_string();
        }
    }
    // Fallback to home Desktop
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join("Desktop");
        if p.is_dir() {
            return p.to_string_lossy().to_string();
        }
    }
    ".".to_string()
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn categorize_file(path: &Path) -> String {
    let lower = path.to_string_lossy().to_lowercase();
    if lower.contains("architect") || lower.contains("bld") || lower.contains("building") || lower.contains("starbucks") || lower.contains("tower") {
        "Architecture".to_string()
    } else if lower.contains("equip") || lower.contains("ev_") || lower.contains("huawei") || lower.contains("power") || lower.contains("charger") || lower.contains("solar") || lower.contains("canopy") {
        "Equipment & EV".to_string()
    } else if lower.contains("vehic") || lower.contains("car") || lower.contains("spectre") || lower.contains("kia") || lower.contains("truck") || lower.contains("forklift") {
        "Vehicles".to_string()
    } else if lower.contains("roof") || lower.contains("warehouse") || lower.contains("complete") || lower.contains("model") || lower.contains("project") {
        "Projects & Warehouses".to_string()
    } else {
        "Components".to_string()
    }
}

impl WarehousePanel {
    pub fn scan_models(&mut self) {
        self.models.clear();
        let root = Path::new(&self.custom_dir);
        if root.is_dir() {
            let mut found = Vec::new();
            scan_dir_recursive(root, 0, 4, &mut found);
            self.models = found;
        }

        // Also ensure built-in test fixtures are included if empty
        if self.models.is_empty() {
            let fixtures = Path::new("./crates/sketcraft-core/tests/fixtures");
            if fixtures.is_dir() {
                scan_dir_recursive(fixtures, 0, 2, &mut self.models);
            }
        }

        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        let q = self.search.trim().to_lowercase();
        let cat = &self.selected_category;

        self.filtered_models = self
            .models
            .iter()
            .filter(|m| {
                if cat != "All" && &m.category != cat {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                m.name.to_lowercase().contains(&q)
                    || m.category.to_lowercase().contains(&q)
                    || m.format.to_lowercase().contains(&q)
                    || m.path.to_lowercase().contains(&q)
            })
            .cloned()
            .collect();
    }
}

fn scan_dir_recursive(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<DiscoveredModel>) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                scan_dir_recursive(&path, depth + 1, max_depth, out);
            }
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "skp" | "3mf" | "obj" | "gltf" | "glb" | "stl") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Model")
                    .to_string();
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let format = ext.to_uppercase();
                let category = categorize_file(&path);
                let size_formatted = format_bytes(size_bytes);
                let description = format!("{format} 3D Model ({size_formatted})");

                out.push(DiscoveredModel {
                    name: stem,
                    path: path.to_string_lossy().to_string(),
                    category,
                    format,
                    size_bytes,
                    size_formatted,
                    description,
                });
            }
        }
    }
}

pub fn open(app: &mut SolveApp) {
    app.warehouse_panel = Some(WarehousePanel::default());
}

pub fn show(app: &mut SolveApp, ctx: &egui::Context) {
    let Some(mut panel) = app.warehouse_panel.take() else { return };
    let t = Tokens::get();
    let mut import_action = None;
    let mut open_action = None;

    let (closed, _) = ui_kit::Window::new(
        Id::unique("sc_warehouse_panel"),
        "library",
        crate::i18n::tr_label("3D Warehouse & Model Discovery"),
    )
    .cards(3.8)
    .actions("", crate::i18n::tr("ui.close"), None)
    .show(ctx, true, |ui| {
        // Top Bar: Search and Directory
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔍").size(15.0));
            let search_edit = ui.add(
                egui::TextEdit::singleline(&mut panel.search)
                    .hint_text("Search warehouse models, EV roofs, equipment, cars…")
                    .desired_width(260.0),
            );
            if search_edit.changed() {
                panel.apply_filter();
            }

            if !panel.search.is_empty() && ui.button("Clear").clicked() {
                panel.search.clear();
                panel.apply_filter();
            }

            ui.separator();

            if ui.button("📁 Refresh Folder").clicked() {
                panel.scan_models();
            }
        });

        // Folder Path row
        ui.horizontal(|ui| {
            ui.label(RichText::new("Directory:").color(t.text_dim));
            let dir_resp = ui.add(
                egui::TextEdit::singleline(&mut panel.custom_dir)
                    .desired_width(320.0),
            );
            if dir_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                panel.scan_models();
            }
            if ui.button("Scan").clicked() {
                panel.scan_models();
            }
        });

        ui.add_space(t.space_6);

        // Category Filter Chips
        ui.horizontal_wrapped(|ui| {
            let categories = [
                "All",
                "Projects & Warehouses",
                "Architecture",
                "Equipment & EV",
                "Vehicles",
                "Components",
            ];
            for cat in categories {
                let is_sel = panel.selected_category == cat;
                let text = match cat {
                    "All" => format!("All ({})", panel.models.len()),
                    "Projects & Warehouses" => "🏭 Projects & Warehouses".to_string(),
                    "Architecture" => "🏢 Architecture".to_string(),
                    "Equipment & EV" => "⚡ Equipment & EV".to_string(),
                    "Vehicles" => "🚗 Vehicles".to_string(),
                    other => other.to_string(),
                };
                if ui.selectable_label(is_sel, text).clicked() {
                    panel.selected_category = cat.to_string();
                    panel.apply_filter();
                }
            }
        });

        ui.add_space(t.space_6);
        ui.separator();

        // Model List Area
        let available_height = 340.0;
        egui::ScrollArea::vertical()
            .max_height(available_height)
            .show(ui, |ui| {
                if panel.filtered_models.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No 3D models found matching filter").color(t.text_dim));
                        ui.label(RichText::new("Check directory path above or adjust search query").small().color(t.text_dim));
                    });
                    ui.add_space(30.0);
                } else {
                    for (idx, model) in panel.filtered_models.iter().enumerate() {
                        let is_selected = panel.selected_idx == Some(idx);
                        let bg_color = if is_selected {
                            t.control_fill
                        } else if idx % 2 == 0 {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 6)
                        } else {
                            Color32::TRANSPARENT
                        };

                        ui.scope(|ui| {
                            let frame = egui::Frame::NONE
                                .fill(bg_color)
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .corner_radius(4.0);

                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Category icon
                                    let icon = match model.category.as_str() {
                                        "Architecture" => "🏢",
                                        "Equipment & EV" => "⚡",
                                        "Vehicles" => "🚗",
                                        "Projects & Warehouses" => "🏭",
                                        _ => "📦",
                                    };
                                    ui.label(RichText::new(icon).size(18.0));

                                    // Title and details
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&model.name).strong());
                                            
                                            // Format badge
                                            let badge_color = match model.format.as_str() {
                                                "SKP" => Color32::from_rgb(180, 60, 60),
                                                "3MF" => Color32::from_rgb(50, 130, 200),
                                                "GLTF" | "GLB" => Color32::from_rgb(60, 160, 90),
                                                _ => Color32::from_rgb(120, 120, 120),
                                            };
                                            let (rect, _) = ui.allocate_exact_size(vec2(36.0, 16.0), Sense::hover());
                                            ui.painter().rect_filled(rect, 3.0, badge_color);
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                &model.format,
                                                egui::FontId::proportional(10.0),
                                                Color32::WHITE,
                                            );

                                            ui.label(RichText::new(&model.size_formatted).color(t.text_dim).small());
                                        });

                                        ui.label(
                                            RichText::new(&model.category)
                                                .color(t.text_dim)
                                                .small(),
                                        );
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        // Actions: Open or Import
                                        if ui.button("📥 Import").on_hover_text("Insert this 3D model into the active scene").clicked() {
                                            import_action = Some(model.path.clone());
                                        }

                                        if ui.button("📂 Open").on_hover_text("Open this model as a new project").clicked() {
                                            open_action = Some(model.path.clone());
                                        }
                                    });
                                });
                            });
                        });
                        ui.add_space(2.0);
                    }
                }
            });

        ui.add_space(t.space_6);
        ui.separator();

        // Footer info
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Discovered: {} models (showing {})",
                    panel.models.len(),
                    panel.filtered_models.len()
                ))
                .small()
                .color(t.text_dim),
            );

            if let Some(status) = &panel.last_status {
                ui.separator();
                ui.label(RichText::new(status).small().color(t.accent));
            }
        });
    });

    // Handle user actions
    if let Some(path) = import_action {
        panel.last_status = Some(format!("Importing {}…", Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or("model")));
        match app.run("file.insert_mesh", json!({ "path": path })) {
            Ok(_) => {
                app.set_status("Imported 3D model successfully", false);
                panel.last_status = Some("Model imported into scene".to_string());
            }
            Err(e) => {
                app.set_status(format!("Import failed: {e}"), true);
                panel.last_status = Some(format!("Error: {e}"));
            }
        }
    }

    if let Some(path) = open_action {
        panel.last_status = Some(format!("Opening {}…", Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or("project")));
        match app.run("doc.open", json!({ "path": path })) {
            Ok(_) => {
                app.set_status("Opened 3D model design", false);
                panel.last_status = Some("Project opened successfully".to_string());
            }
            Err(e) => {
                app.set_status(format!("Open failed: {e}"), true);
                panel.last_status = Some(format!("Error: {e}"));
            }
        }
    }

    if !closed {
        app.warehouse_panel = Some(panel);
    }
}
