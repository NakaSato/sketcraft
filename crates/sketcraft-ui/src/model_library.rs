//! Model Library: browse a folder of model files on disk and import one into the current model.
//! The folder, search text and format filter are app preferences; the scan results are not.
use super::*;
use crate::inspector_table::{Column, InspectorTable};

/// Deepest subfolder level scanned below the chosen folder.
const MAX_DEPTH: usize = 6;
/// Directory entries visited per scan, so a huge folder cannot freeze the window.
const MAX_VISITED: usize = 20_000;
/// Model files kept from one scan.
const MAX_FILES: usize = 2_000;
/// Rows drawn at once; a narrower search shows the rest.
const MAX_ROWS: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub(super) folder: String,
    pub(super) search: String,
    /// An extension from `mesh_exchange::IMPORT_EXTENSIONS`, or empty for every format.
    pub(super) format: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub(super) path: std::path::PathBuf,
    /// Path below the scanned folder, shown so same-named files stay distinguishable.
    pub(super) relative: String,
    pub(super) extension: String,
    pub(super) bytes: u64,
}

#[derive(Default)]
pub(super) struct ModelLibrary {
    pub(super) prefs: Preferences,
    pub(super) entries: Vec<Entry>,
    /// The folder `entries` came from, so a changed folder is rescanned.
    pub(super) scanned: Option<String>,
    pub(super) status: Option<String>,
}

fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / f64::from(1u32 << 30)),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / f64::from(1u32 << 20)),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / 1024.),
        b => format!("{b} B"),
    }
}

/// Every importable file below `root`, sorted by relative path, within the scan limits.
/// Returns whether a limit stopped the scan early.
pub(super) fn scan(root: &std::path::Path) -> Result<(Vec<Entry>, bool), String> {
    if !root.is_dir() {
        return Err("Choose an existing folder".into());
    }
    let mut entries = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    let mut truncated = false;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue; // Unreadable subfolders are skipped, not fatal.
        };
        for item in read.flatten() {
            visited += 1;
            if visited > MAX_VISITED || entries.len() >= MAX_FILES {
                truncated = true;
                break;
            }
            let path = item.path();
            let Ok(kind) = item.file_type() else { continue };
            let hidden = item.file_name().to_string_lossy().starts_with('.');
            if kind.is_dir() && !hidden && depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            } else if kind.is_file() && !hidden {
                let extension = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                if crate::mesh_exchange::IMPORT_EXTENSIONS.contains(&extension.as_str()) {
                    let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
                    let bytes = item.metadata().map(|m| m.len()).unwrap_or(0);
                    entries.push(Entry { path, relative, extension, bytes });
                }
            }
        }
    }
    entries.sort_by(|a, b| a.relative.to_lowercase().cmp(&b.relative.to_lowercase()));
    Ok((entries, truncated))
}

impl ModelLibrary {
    pub(super) fn rescan(&mut self) {
        let folder = self.prefs.folder.trim().to_owned();
        self.scanned = Some(folder.clone());
        self.entries.clear();
        self.status = match scan(std::path::Path::new(&folder)) {
            Ok((entries, truncated)) => {
                let note = if truncated { " (scan limit reached; choose a smaller folder)" } else { "" };
                let found = format!("{} model files found{note}", entries.len());
                self.entries = entries;
                Some(found)
            }
            Err(message) => Some(message),
        };
    }
    /// Entries matching the search text and format filter.
    pub(super) fn matches(&self) -> Vec<&Entry> {
        let search = self.prefs.search.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|e| self.prefs.format.is_empty() || e.extension == self.prefs.format)
            .filter(|e| search.is_empty() || e.relative.to_lowercase().contains(&search))
            .collect()
    }
}

impl KetchupApp {
    /// Insert a library file through the same import path as File → Import.
    pub(super) fn import_library_file(&mut self, ctx: &egui::Context, path: std::path::PathBuf) {
        if self.project.blocked() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.project.operation = projects::Operation::Import;
            self.project.pending = Some(crate::project_files::import_path(
                ctx,
                self.doc.model.clone(),
                path,
                "any".into(),
            ));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (ctx, path);
        }
    }

    pub(super) fn model_library_contents(&mut self, ui: &mut egui::Ui) {
        if cfg!(target_arch = "wasm32") {
            ui.text_weak("Browsing folders on disk is available in the desktop app. Use File → Import here.");
            return;
        }
        let library = &mut self.model_library;
        crate::inspector_table::inspector_field(ui, "model_library_folder", "Folder", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut library.prefs.folder)
                    .hint_text("Paste a folder path")
                    .desired_width(f32::INFINITY),
            )
        });
        ui.horizontal(|ui| {
            let folder_set = !library.prefs.folder.trim().is_empty();
            let changed = library.scanned.as_deref() != Some(library.prefs.folder.trim());
            if ui
                .add_enabled(folder_set, egui::Button::new(if changed { "Scan folder" } else { "Rescan" }))
                .clicked()
            {
                library.rescan();
            }
            if let Some(status) = &library.status {
                ui.text_weak(status);
            }
        });
        if library.scanned.is_none() && !library.prefs.folder.trim().is_empty() {
            // First view after a restart: show the saved folder's files right away.
            library.rescan();
        }
        crate::inspector_table::inspector_field(ui, "model_library_search", "Search", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut library.prefs.search)
                    .hint_text("File or folder name")
                    .desired_width(f32::INFINITY),
            )
        });
        crate::inspector_table::inspector_field(ui, "model_library_format", "Format", |ui| {
            let selected = if library.prefs.format.is_empty() {
                "All formats".to_owned()
            } else {
                format!(".{}", library.prefs.format)
            };
            egui::ComboBox::from_id_salt("model_library_format")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut library.prefs.format, String::new(), "All formats");
                    for extension in crate::mesh_exchange::IMPORT_EXTENSIONS {
                        ui.selectable_value(&mut library.prefs.format, extension.to_owned(), format!(".{extension}"));
                    }
                })
                .response
        });
        let matches: Vec<Entry> = library.matches().into_iter().cloned().collect();
        if library.entries.is_empty() {
            return;
        }
        if matches.is_empty() {
            ui.text_weak("No model files match the search and format.");
            return;
        }
        let mut import = None;
        let mut table = InspectorTable::new(
            ui.available_width(),
            [Column::flex(3.), Column::at_least(56.), Column::at_least(64.)],
        )
        .centered();
        table.header(ui, ["File", "Size", ""]);
        for entry in matches.iter().take(MAX_ROWS) {
            table.row(ui, ("model_library_row", &entry.relative), |column, ui| match column {
                0 => {
                    ui.text_label(&entry.relative).on_hover_text(entry.path.to_string_lossy());
                }
                1 => {
                    ui.text_weak(size_text(entry.bytes));
                }
                _ => {
                    if ui
                        .add_enabled(!self.project.blocked(), egui::Button::new("Import"))
                        .on_hover_text("Insert this model into the current model")
                        .clicked()
                    {
                        import = Some(entry.path.clone());
                    }
                }
            });
        }
        if matches.len() > MAX_ROWS {
            ui.text_weak(format!("Showing {MAX_ROWS} of {} files. Narrow the search to see the rest.", matches.len()));
        }
        if let Some(path) = import {
            self.import_library_file(ui.ctx(), path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_importable_files_in_subfolders_and_skips_hidden_and_other_files() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("Vehicles/Trucks");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.path().join(".cache")).unwrap();
        for (path, bytes) in [
            ("Canopy.SKP", 10),
            ("Vehicles/Trucks/hauler.glb", 2048),
            ("Vehicles/notes.txt", 5),
            (".cache/ghost.stl", 1),
            ("bracket.3mf", 3),
        ] {
            std::fs::write(root.path().join(path), vec![0u8; bytes]).unwrap();
        }
        let (entries, truncated) = scan(root.path()).unwrap();
        assert!(!truncated);
        let names: Vec<_> = entries.iter().map(|e| (e.relative.replace('\\', "/"), e.extension.as_str())).collect();
        assert_eq!(
            names,
            [("bracket.3mf".into(), "3mf"), ("Canopy.SKP".into(), "skp"), ("Vehicles/Trucks/hauler.glb".into(), "glb")]
        );
        assert_eq!(entries[2].bytes, 2048);
        assert!(scan(&root.path().join("missing")).is_err());

        let mut library = ModelLibrary { entries, ..Default::default() };
        library.prefs.search = "TRUCK".into();
        assert_eq!(library.matches().len(), 1);
        library.prefs.search.clear();
        library.prefs.format = "skp".into();
        assert_eq!(library.matches()[0].relative, "Canopy.SKP");
        assert_eq!(size_text(2048), "2 KB");
    }
}
