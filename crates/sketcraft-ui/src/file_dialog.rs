use crate::{
    modal::Modal,
    natural_text::TextUi as _,
    theme::Palette,
};
use eframe::egui::{self, Color32, RichText, ScrollArea, Stroke};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FileDialogMode {
    OpenProject,
    SaveProject,
    ImportGeometry(String),
    ExportGeometry(bool),
    ExportCuttingPlan,
    ExportScreenshots,
    ExportEstimatePdf,
}

#[allow(dead_code)]
pub enum FileDialogPayload {
    None,
    SaveProject {
        contents: String,
    },
    ImportGeometry {
        extension: String,
    },
    ExportGeometry {
        contents: Vec<u8>,
        stl: bool,
    },
    ExportCuttingPlan {
        name: String,
        model: Box<ketchup_core::Model>,
        selected: std::collections::BTreeSet<u64>,
        layout_settings: crate::cutting_layout::Settings,
    },
    ExportScreenshots {
        name: String,
        settings: crate::screenshots::Settings,
        pages: Vec<image::RgbaImage>,
    },
    ExportEstimatePdf {
        contents: Vec<u8>,
    },
}

#[allow(dead_code)]
pub struct FileDialog {
    pub is_open: bool,
    pub mode: FileDialogMode,
    pub payload: FileDialogPayload,
    pub current_dir: PathBuf,
    pub file_name: String,
    pub extensions: Vec<String>,
    pub title: String,
    pub error: Option<String>,
    pub selected_entry: Option<PathBuf>,
}

impl Default for FileDialog {
    fn default() -> Self {
        let initial_dir = dirs_documents_dir()
            .or_else(dirs_home_dir)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        Self {
            is_open: false,
            mode: FileDialogMode::OpenProject,
            payload: FileDialogPayload::None,
            current_dir: initial_dir,
            file_name: String::new(),
            extensions: vec!["ketchup".into()],
            title: "Select File".into(),
            error: None,
            selected_entry: None,
        }
    }
}

#[allow(dead_code)]
pub enum FileDialogResult {
    Confirmed {
        path: PathBuf,
        payload: FileDialogPayload,
    },
    Cancelled,
}

#[allow(dead_code)]
impl FileDialog {
    pub fn open_dialog(
        &mut self,
        mode: FileDialogMode,
        payload: FileDialogPayload,
        extensions: &[&str],
        title: &str,
        default_file_name: &str,
        starting_path: Option<&Path>,
    ) {
        self.is_open = true;
        self.mode = mode;
        self.payload = payload;
        self.title = title.to_string();
        self.file_name = default_file_name.to_string();
        self.extensions = extensions.iter().map(|s| s.to_string()).collect();
        self.error = None;
        self.selected_entry = None;

        if let Some(path) = starting_path {
            if path.is_file() {
                if let Some(parent) = path.parent() {
                    self.current_dir = parent.to_path_buf();
                }
            } else if path.is_dir() {
                self.current_dir = path.to_path_buf();
            }
        }
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.payload = FileDialogPayload::None;
        self.error = None;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<FileDialogResult> {
        if !self.is_open {
            return None;
        }

        let is_save = matches!(
            self.mode,
            FileDialogMode::SaveProject
                | FileDialogMode::ExportGeometry(_)
                | FileDialogMode::ExportCuttingPlan
                | FileDialogMode::ExportScreenshots
                | FileDialogMode::ExportEstimatePdf
        );

        let action_name = if is_save { "Save" } else { "Open" };
        let mut confirmed_path = None;
        let mut should_close = false;

        let response = Modal::new("in_app_file_dialog", 640.)
            .resizable(true)
            .show(ctx, |modal| {
                modal.header(|ui| {
                    ui.text_heading(&self.title);
                    ui.add_space(4.);

                    // Directory navigation bar & shortcuts
                    ui.horizontal(|ui| {
                        if ui.button("⬆ Up").on_hover_text("Go to parent directory").clicked() {
                            if let Some(parent) = self.current_dir.parent() {
                                self.current_dir = parent.to_path_buf();
                                self.selected_entry = None;
                            }
                        }

                        // Shortcut buttons
                        if let Some(home) = dirs_home_dir() {
                            if ui.button("🏠 Home").clicked() {
                                self.current_dir = home;
                                self.selected_entry = None;
                            }
                        }
                        if let Some(docs) = dirs_documents_dir() {
                            if ui.button("📁 Documents").clicked() {
                                self.current_dir = docs;
                                self.selected_entry = None;
                            }
                        }
                        if let Some(downloads) = dirs_downloads_dir() {
                            if ui.button("⬇ Downloads").clicked() {
                                self.current_dir = downloads;
                                self.selected_entry = None;
                            }
                        }

                        ui.separator();
                        let dir_str = self.current_dir.to_string_lossy();
                        ui.label(RichText::new(dir_str).monospace().strong());
                    });
                });

                modal.body(|ui| {
                    if let Some(err) = &self.error {
                        ui.label(RichText::new(err).color(Color32::from_rgb(220, 50, 50)));
                        ui.add_space(4.);
                    }

                    // Directory content list
                    let mut navigate_to = None;
                    let mut double_click_confirmed = false;

                    ScrollArea::vertical()
                        .max_height(340.)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let entries = match std::fs::read_dir(&self.current_dir) {
                                Ok(read_dir) => {
                                    let mut dirs = Vec::new();
                                    let mut files = Vec::new();
                                    for entry in read_dir.flatten() {
                                        let path = entry.path();
                                        if path.is_dir() {
                                            dirs.push(path);
                                        } else if path.is_file() {
                                            // Check extensions filter
                                            if self.extensions.is_empty() {
                                                files.push(path);
                                            } else {
                                                let ext = path
                                                    .extension()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("")
                                                    .to_ascii_lowercase();
                                                if self.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext)) {
                                                    files.push(path);
                                                }
                                            }
                                        }
                                    }
                                    dirs.sort();
                                    files.sort();
                                    (dirs, files)
                                }
                                Err(err) => {
                                    ui.label(format!("Could not read directory: {err}"));
                                    (Vec::new(), Vec::new())
                                }
                            };

                            let (dirs, files) = entries;

                            // Render directories
                            for dir in dirs {
                                let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("folder");
                                let row = ui.selectable_label(false, format!("📁  {name}"));
                                if row.clicked() {
                                    navigate_to = Some(dir);
                                }
                            }

                            // Render matching files
                            for file in files {
                                let name = file.file_name().and_then(|s| s.to_str()).unwrap_or("file");
                                let is_selected = self.selected_entry.as_ref() == Some(&file) || self.file_name == name;
                                let row = ui.selectable_label(is_selected, format!("📄  {name}"));
                                if row.clicked() {
                                    self.selected_entry = Some(file.clone());
                                    self.file_name = name.to_string();
                                }
                                if row.double_clicked() {
                                    self.selected_entry = Some(file.clone());
                                    self.file_name = name.to_string();
                                    if !is_save {
                                        double_click_confirmed = true;
                                    }
                                }
                            }
                        });

                    if let Some(dir) = navigate_to {
                        self.current_dir = dir;
                        self.selected_entry = None;
                    }

                    ui.add_space(8.);
                    ui.separator();
                    ui.add_space(4.);

                    // File name input
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("File name:").strong());
                        let name_response = ui.text_edit_singleline(&mut self.file_name);
                        if name_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            double_click_confirmed = true;
                        }

                        // Filter display badge
                        let ext_badge = if self.extensions.is_empty() {
                            "All files".to_string()
                        } else {
                            format!("*.{}", self.extensions.join(", *."))
                        };
                        ui.badge(ext_badge);
                    });

                    if double_click_confirmed {
                        if let Some(path) = self.resolve_selected_path() {
                            confirmed_path = Some(path);
                        }
                    }
                });

                modal.footer(|ui| {
                    // Modal convention: least impactful action (Cancel) on the right
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }

                    let can_confirm = !self.file_name.trim().is_empty();
                    let confirm_clicked = crate::modal::primary_button(ui, action_name, can_confirm).clicked();

                    if confirm_clicked && can_confirm {
                        if let Some(path) = self.resolve_selected_path() {
                            confirmed_path = Some(path);
                        }
                    }
                });
            });

        if should_close || response.should_close() {
            self.close();
            return Some(FileDialogResult::Cancelled);
        }

        if let Some(path) = confirmed_path {
            let payload = std::mem::replace(&mut self.payload, FileDialogPayload::None);
            self.close();
            return Some(FileDialogResult::Confirmed { path, payload });
        }

        None
    }

    fn resolve_selected_path(&mut self) -> Option<PathBuf> {
        let name = self.file_name.trim();
        if name.is_empty() {
            self.error = Some("Please enter a file name.".into());
            return None;
        }

        let mut path = self.current_dir.join(name);

        // Ensure extension is attached if saving
        if let Some(ext) = self.extensions.first() {
            if path.extension().is_none() {
                path.set_extension(ext);
            }
        }

        Some(path)
    }
}

#[allow(dead_code)]
trait UiBadgeExt {
    fn badge(&mut self, text: impl Into<String>);
}

impl UiBadgeExt for egui::Ui {
    fn badge(&mut self, text: impl Into<String>) {
        let text: String = text.into();
        let colors = Palette::for_painter(&self.painter());
        egui::Frame::new()
            .fill(colors.canvas)
            .stroke(Stroke::new(1., colors.border))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(self, |ui| {
                ui.label(RichText::new(text).size(11.).color(colors.muted));
            });
    }
}

fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn dirs_documents_dir() -> Option<PathBuf> {
    dirs_home_dir().map(|h| h.join("Documents")).filter(|p| p.is_dir())
}

#[allow(dead_code)]
fn dirs_downloads_dir() -> Option<PathBuf> {
    dirs_home_dir().map(|h| h.join("Downloads")).filter(|p| p.is_dir())
}
