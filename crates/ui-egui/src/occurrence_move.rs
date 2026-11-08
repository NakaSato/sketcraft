//! Move (`occurrence.move`) on the selected placement of a component: opens the same Move as
//! its Browser menu, on that placement's own path.
use egui::Pos2;
use sketcraft_engine::Sel;

use crate::SketApp;

/// The one occurrence the selection lies in (its faces, edges, bodies or the placement itself);
/// none when it spans several or names no placement.
pub(crate) fn selected_occurrence(app: &SketApp) -> Option<u64> {
    let mut selected = None;
    for item in &app.session.selection {
        let occurrence = match item {
            Sel::Occurrence { path } => *path.last()?,
            Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } => {
                crate::dialogs_assembly::occurrence_of_body(&app.session, body)?
            }
            Sel::Body { name } => crate::dialogs_assembly::occurrence_of_body(&app.session, name)?,
            _ => return None,
        };
        if occurrence == 0 || selected.is_some_and(|s| s != occurrence) {
            return None;
        }
        selected = Some(occurrence);
    }
    selected
}

/// The placement path of the first selected item.
fn selected_path(app: &SketApp) -> Option<Vec<u64>> {
    match app.session.selection.first()? {
        Sel::Occurrence { path } => Some(path.clone()),
        item => {
            let body = match item {
                Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } => body,
                Sel::Body { name } => name,
                _ => return None,
            };
            app.session
                .doc
                .body_frames_with_source(&app.session.model.state())
                .into_iter()
                .find(|(world, ..)| world == body)
                .map(|(_, _, _, path, _)| path)
        }
    }
}

/// Start Move on the selected placement, or say what to select.
pub(crate) fn start(app: &mut SketApp) {
    let item = selected_occurrence(app).and_then(|_| selected_path(app)).and_then(|path| {
        crate::context_menu::items(app, &crate::context_menu::Target::Occurrence { path })
            .into_iter()
            .find(|i| i.id == "ui.moveOccurrence" && i.enabled)
    });
    match item {
        Some(item) => crate::context_menu::run_item(app, &item, Pos2::ZERO),
        None => app.set_status(app.tr("joint_move.select").to_string(), false),
    }
}
