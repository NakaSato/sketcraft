//! SketchUp's double-click with Push/Pull: a flat face double-clicked while Push/Pull waits is
//! pushed or pulled by the distance of the last Push/Pull, in one undo step, and Push/Pull stays.
//!
//! The distance is kept in the preferences (`Prefs::press_pull_distance`, millimetres, positive
//! out of the face), from Push/Pull's Extrude window when it applies and from every
//! `solid.press_pull` run, so it is still there after a restart.

use serde_json::{Value, json};
use sketcraft_engine::Sel;
use sketcraft_engine::doc::expr::Kind as ValueKind;

use crate::SketApp;
use crate::dialogs::{Dialog, Kind};

/// Whether Push/Pull is the open tool: its Extrude window, waiting for a face or with one picked.
pub(crate) fn active(app: &SketApp) -> bool {
    app.dialog.as_ref().is_some_and(|d| d.push_pull && d.editing.is_none() && matches!(d.kind, Kind::Extrude { .. }))
}

fn store(app: &mut SketApp, mm: f64) {
    if mm.is_finite() && mm.abs() > 1e-9 {
        app.preferences.press_pull_distance = Some(mm);
    }
}

/// Push/Pull's window applied: one face moved by a plain distance to one side is a distance to
/// repeat (To Object, All, Thin, Two Sides and Symmetric are not).
pub(crate) fn dialog_applied(app: &mut SketApp, d: &Dialog) {
    if !d.push_pull {
        return;
    }
    let Kind::Extrude { distance, direction, all: false, thin: false, .. } = &d.kind else { return };
    // Only its Profiles input: no To Object or From Object.
    let [input] = d.inputs.as_slice() else { return };
    if !matches!(input.items.as_slice(), [Sel::Face { .. }]) || *direction >= 2 {
        return;
    }
    let Ok(mm) = crate::length_input::eval(&app.session.doc, distance, ValueKind::Length) else { return };
    let side = crate::smart_extrude::side(mm, *direction);
    store(app, f64::from(side) * mm.abs());
}

/// `solid.press_pull` ran on a face: its distance is the one to repeat.
pub(crate) fn command_ran(app: &mut SketApp, params: &Value) {
    if params.get("face").is_none() || params.get("edges").is_some() {
        return;
    }
    let distance = match params.get("distance") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return,
    };
    if let Ok(mm) = app.session.doc.eval(&distance, ValueKind::Length) {
        store(app, mm);
    }
}

/// A double-click on `sel` with Push/Pull open: a flat face (not a fillet's or chamfer's) is
/// pushed or pulled by the remembered distance and Push/Pull waits for the next face. Returns
/// whether the double-click was taken; with no distance remembered it is not.
pub(crate) fn double_click(app: &mut SketApp, sel: Option<&Sel>) -> bool {
    if !active(app) || app.applying.is_some() {
        return false;
    }
    let Some(mm) = app.preferences.press_pull_distance.filter(|d| d.is_finite() && d.abs() > 1e-9) else { return false };
    let Some(Sel::Face { body, index, point }) = sel else { return false };
    if crate::dialogs::planar_face(&app.session, body, *index).is_none() || crate::dialogs::blend_feature(&app.session, body, *index).is_some() {
        return false;
    }
    let point = *point;
    crate::dialogs::cancel(app);
    app.dialog = None;
    let _ = app.run("select.clear", json!({}));
    let distance = crate::length_input::model(&app.session.doc, mm);
    match app.run("solid.press_pull", json!({"face": [point.x, point.y, point.z], "distance": distance})) {
        Ok(_) => {
            let shown = crate::length_input::shown(&app.session.doc, mm);
            app.set_status(crate::i18n::format("ui.press_pull_repeated", &[("distance", &shown)]), false);
        }
        Err(why) => app.set_status(why, true),
    }
    // Push/Pull stays, waiting for the next face.
    let _ = app.run("select.clear", json!({}));
    app.start("solid.press_pull");
    true
}
