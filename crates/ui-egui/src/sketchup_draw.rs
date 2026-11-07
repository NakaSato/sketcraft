//! Drawing as SketchUp does: on the SketchUp tab a drawing tool needs no Create Sketch step. The
//! tool waits for its first click in the view; that click picks the plane (the flat face under
//! the pointer, a plane shown in the view, else the ground) and is the tool's first point. The
//! sketch is made there without turning the view, and finishing it goes back to the SketchUp tab.

use serde_json::{Value, json};
use sketcraft_engine::render::UpAxis;

use crate::SolveApp;
use crate::viewport::{Hit, Proj};

/// SketchUp's modelling tools on its tab. SketchUp has no sketch to finish, so choosing one in a
/// sketch begun from the SketchUp tab finishes that sketch first.
pub(crate) const MODEL_TOOLS: &[&str] = &["solid.press_pull", "solid.offset_face", "solid.move", "solid.scale", "solid.sweep"];

/// Whether starting `id` now finishes the open sketch first: one of [`MODEL_TOOLS`], in a sketch
/// begun from the SketchUp tab.
pub(crate) fn finishes_sketch_for(app: &SolveApp, id: &str) -> bool {
    app.session.active_sketch.is_some()
        && app.ui.sketch_return_tab.as_deref() == Some(crate::workspace::SKETCHUP_TAB)
        && MODEL_TOOLS.contains(&id)
}

/// The drawing tool waiting for its first click, if any.
pub(crate) fn pending(app: &SolveApp) -> Option<&'static str> {
    app.ui.draw_on_click
}

/// Get `tool` ready: it starts on the next click in the view.
pub(crate) fn begin(app: &mut SolveApp, tool: &'static str) {
    if app.dialog.is_some() {
        crate::dialogs::cancel(app);
    }
    app.tool = None;
    app.ui.draw_on_click = Some(tool);
    let label = sketcraft_engine::find_command(tool).map_or(tool, |c| c.label);
    let label = app.tr_label(label).to_string();
    app.set_status(format!("{label}: {}", app.tr_label("click a face or the ground to start drawing")), false);
}

/// Stop waiting (Escape, or another command started). Whether a tool was waiting.
pub(crate) fn cancel(app: &mut SolveApp) -> bool {
    let was = app.ui.draw_on_click.take().is_some();
    if was {
        app.set_status(String::new(), false);
    }
    was
}

/// The plane a click on `hits` (front first) draws on: the first flat face or shown plane under
/// the pointer, else the ground (XY, or XZ when Y is up).
fn plane_at(app: &SolveApp, hits: &[Hit]) -> Value {
    for hit in hits {
        match hit {
            Hit::Face { .. } => {
                if let Some(sel) = crate::viewport::hit_sel(hit)
                    && let Some(plane) = crate::sketch_attachment_ui::face_plane(app, &sel)
                {
                    return json!({"plane": plane, "activate_part": app.preferences.sketch_face_activates_part});
                }
            }
            Hit::Plane { name, .. } => return json!({"plane": name}),
            // An edge or a point in front of a face: look further back.
            Hit::Edge { .. } | Hit::Vertex { .. } => {}
            // Other sketches and their profiles: the ground beneath them.
            _ => break,
        }
    }
    json!({"plane": if app.ui.up_axis == UpAxis::Y { "XZ" } else { "XY" }})
}

/// A click at `pos` in the view while a tool waits: make the sketch on the plane there, start
/// the tool in it and give it this click as its first point. Whether the click was taken.
pub(crate) fn click(app: &mut SolveApp, proj: &Proj, pos: egui::Pos2, hits: &[Hit], shift: bool) -> bool {
    let Some(tool) = app.ui.draw_on_click.take() else { return false };
    let params = plane_at(app, hits);
    let on_face = params["plane"].is_object();
    match app.run("sketch.create", params) {
        Ok(_) => {}
        // A curved face: draw on the ground instead, as SketchUp does off a face.
        Err(_) if on_face => {
            let ground = if app.ui.up_axis == UpAxis::Y { "XZ" } else { "XY" };
            if let Err(error) = app.run("sketch.create", json!({"plane": ground})) {
                app.ui.draw_on_click = Some(tool);
                app.set_status(crate::friendly_error("sketch.create", &error), true);
                return true;
            }
        }
        Err(error) => {
            app.ui.draw_on_click = Some(tool);
            app.set_status(crate::friendly_error("sketch.create", &error), true);
            return true;
        }
    }
    app.set_status(String::new(), false);
    app.start(tool);
    if app.tool.is_some() {
        crate::tools::on_hover(app, proj, pos, shift);
        crate::tools::on_click(app, proj, pos, shift);
    }
    true
}
