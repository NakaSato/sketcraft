//! SketchUp's Rotate on the SketchUp tab: a protractor in the view. With bodies selected (or the
//! body under the first click), the first click places the protractor's centre (its axis is the
//! normal of the flat face there, or the axis an arrow key locks), the second sets the start
//! direction and the third the angle, snapping every 15°. Typing a number and Enter after the
//! second click turns by that many degrees. Tapping Ctrl (Option on a Mac) toggles Copy. Each
//! turn is one `solid.rotate`, one undo step; Rotate then waits for the next centre.
//!
//! The other tabs keep Move/Copy's Rotate type for `solid.rotate`.

use egui::{Pos2, Shape, Stroke};
use serde_json::json;
use sketcore_record::Vec3;
use sketcraft_engine::Sel;
use sketcraft_engine::geom::prelude::*;

use crate::SketApp;
use crate::viewport::{Hit, Proj};

/// SketchUp's protractor snaps to multiples of this many degrees.
const SNAP_DEGREES: f64 = 15.0;
/// How close to a multiple of [`SNAP_DEGREES`] the angle snaps to it, in degrees.
const SNAP_WITHIN: f64 = 2.5;
/// The protractor's radius on screen, in points.
const RADIUS: f32 = 56.0;
/// How far the pointer may move while Ctrl is held for the press to still be a tap (Ctrl and
/// a move orbits while drawing), in points.
const TAP_SLOP: f32 = 4.0;
/// The longest angle a person types, in characters.
const MAX_TYPED: usize = 16;

/// The Rotate tool while it is open on the SketchUp tab.
#[derive(Clone, Debug, Default)]
pub(crate) struct RotateTool {
    /// The bodies to turn: the selection's when Rotate started, else the first click's body.
    bodies: Vec<String>,
    /// The axis an arrow key locked (X, Y or Z), until Down or a second press of it.
    lock: Option<Vec3>,
    /// The protractor's centre and axis (unit), once the first click placed it.
    center: Option<(Vec3, Vec3)>,
    /// The start direction (unit, in the protractor's plane), once the second click set it.
    start: Option<Vec3>,
    /// Where the pointer is: the protractor's centre and axis before the first click.
    hover: Option<(Vec3, Vec3)>,
    /// The turn under the pointer, in degrees, after the second click.
    angle: f64,
    /// What has been typed since the second click.
    typed: String,
    /// Copy: the turned bodies are copies and the originals stay.
    copy: bool,
    /// Ctrl went down at this pointer position and nothing else happened since.
    ctrl_tap: Option<Pos2>,
    ctrl_down: bool,
}

/// Whether starting `id` opens this tool: Rotate on the SketchUp tab.
pub(crate) fn wanted(app: &SketApp, id: &str) -> bool {
    id == "solid.rotate" && app.ui.tab == crate::workspace::SKETCHUP_TAB
}

pub(crate) fn active(app: &SketApp) -> bool {
    app.rotate_tool.is_some()
}

/// The bodies the selection names (bodies, and the bodies of selected faces and edges).
fn selected_bodies(app: &SketApp) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for s in &app.session.selection {
        let name = match s {
            Sel::Body { name } => name,
            Sel::Face { body, .. } | Sel::Edge { body, .. } => body,
            _ => continue,
        };
        if !v.contains(name) {
            v.push(name.clone());
        }
    }
    v
}

/// Open Rotate: it waits for the protractor's centre.
pub(crate) fn begin(app: &mut SketApp) {
    if app.dialog.is_some() {
        crate::dialogs::cancel(app);
        app.dialog = None;
    }
    app.tool = None;
    let bodies = selected_bodies(app);
    app.rotate_tool = Some(RotateTool { bodies, ..RotateTool::default() });
    hint(app);
}

/// Close Rotate (Escape with nothing placed, or another command). Whether it was open.
pub(crate) fn cancel(app: &mut SketApp) -> bool {
    let was = app.rotate_tool.take().is_some();
    if was {
        app.set_status(String::new(), false);
    }
    was
}

fn hint(app: &mut SketApp) {
    let Some(t) = &app.rotate_tool else { return };
    let id = if t.start.is_some() {
        "ui.rotate_tool.angle"
    } else if t.center.is_some() {
        "ui.rotate_tool.start"
    } else {
        "ui.rotate_tool.center"
    };
    let mut text = app.tr(id).to_string();
    if t.copy {
        text = format!("{text} {}", app.tr("ui.rotate_tool.copy"));
    }
    app.set_status(text, false);
}

/// Where the ray from `pos` meets the plane through `c` across `n`.
fn on_plane(proj: &Proj, pos: Pos2, c: Vec3, n: Vec3) -> Option<Vec3> {
    let (o, d) = proj.ray(pos);
    let along = d.dot(n);
    if along.abs() < 1e-9 {
        return None;
    }
    let t = (c - o).dot(n) / along;
    (t.is_finite() && t > 0.0).then(|| o + d * t)
}

/// The world axes in the order the arrow keys lock them, and the ground's normal.
const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

/// The protractor a pointer at `pos` over `hits` (front first) would place: the point on the
/// first face, edge or vertex there, with the flat face's normal (or the locked axis) as its
/// axis; off the model, on the ground (or the locked axis's plane through the origin).
fn place_at(app: &SketApp, t: &RotateTool, proj: &Proj, pos: Pos2, hits: &[Hit]) -> Option<(Vec3, Vec3)> {
    let ground = if app.ui.up_axis == sketcraft_engine::render::UpAxis::Y { Y } else { Z };
    for hit in hits {
        let (point, normal) = match hit {
            Hit::Face { body, index, point } => (*point, crate::dialogs::planar_face(&app.session, body, *index).map(|(_, n)| n)),
            Hit::Edge { point, .. } | Hit::Vertex { point, .. } => (*point, None),
            _ => continue,
        };
        let axis = t.lock.or(normal).and_then(|n| n.normalized()).unwrap_or(ground);
        return Some((point, axis));
    }
    let axis = t.lock.unwrap_or(ground);
    on_plane(proj, pos, Vec3::ZERO, axis).map(|p| (p, axis))
}

/// The signed turn about `axis` from `from` to `to`, in degrees.
fn turn(from: Vec3, to: Vec3, axis: Vec3) -> f64 {
    from.cross(to).dot(axis).atan2(from.dot(to)).to_degrees()
}

/// `degrees` on a multiple of [`SNAP_DEGREES`] when it is close to one.
fn snapped(degrees: f64) -> f64 {
    let near = (degrees / SNAP_DEGREES).round() * SNAP_DEGREES;
    if (degrees - near).abs() <= SNAP_WITHIN { near } else { degrees }
}

/// The pointer moved (every frame while Rotate is open): follow it, and watch for a Ctrl tap.
pub(crate) fn hover(app: &mut SketApp, proj: &Proj, pos: Option<Pos2>, hits: &[Hit], ctrl: bool) {
    let Some(mut t) = app.rotate_tool.take() else { return };
    // A tap of Ctrl, not a hold to orbit, toggles Copy.
    if ctrl && !t.ctrl_down {
        t.ctrl_tap = pos;
    } else if ctrl {
        if let (Some(at), Some(now)) = (t.ctrl_tap, pos)
            && at.distance(now) > TAP_SLOP
        {
            t.ctrl_tap = None;
        }
    } else if t.ctrl_down && t.ctrl_tap.take().is_some() {
        t.copy = !t.copy;
        app.rotate_tool = Some(t);
        hint(app);
        return finish_hover(app, proj, pos, hits);
    }
    t.ctrl_down = ctrl;
    app.rotate_tool = Some(t);
    finish_hover(app, proj, pos, hits);
}

fn finish_hover(app: &mut SketApp, proj: &Proj, pos: Option<Pos2>, hits: &[Hit]) {
    let Some(pos) = pos else { return };
    let Some(mut t) = app.rotate_tool.take() else { return };
    match (t.center, t.start) {
        (None, _) => t.hover = place_at(app, &t, proj, pos, hits),
        (Some(_), None) => {}
        (Some((c, n)), Some(start)) => {
            if let Some(dir) = on_plane(proj, pos, c, n).and_then(|p| (p - c).normalized()) {
                t.angle = snapped(turn(start, dir, n));
            }
        }
    }
    app.rotate_tool = Some(t);
}

/// A click at `pos` while Rotate is open. Whether the click was taken (always, while it is).
pub(crate) fn click(app: &mut SketApp, proj: &Proj, pos: Pos2, hits: &[Hit]) -> bool {
    let Some(mut t) = app.rotate_tool.take() else { return false };
    t.ctrl_tap = None;
    if t.bodies.is_empty() {
        let body = hits.iter().find_map(|h| match h {
            Hit::Face { body, .. } | Hit::Edge { body, .. } | Hit::Vertex { body, .. } => Some(body.clone()),
            _ => None,
        });
        match body {
            Some(body) => t.bodies.push(body),
            None => {
                app.rotate_tool = Some(t);
                app.set_status(app.tr("ui.rotate_tool.nothing").to_string(), true);
                return true;
            }
        }
    }
    match (t.center, t.start) {
        (None, _) => t.center = place_at(app, &t, proj, pos, hits),
        (Some((c, n)), None) => {
            // The start direction: in the protractor's plane, and not at its centre.
            t.start = on_plane(proj, pos, c, n).map(|p| p - c).filter(|v| v.len() > 1e-6).and_then(|v| v.normalized());
            t.angle = 0.0;
            t.typed.clear();
        }
        (Some(_), Some(_)) => {
            let angle = t.angle;
            app.rotate_tool = Some(t);
            commit(app, angle);
            return true;
        }
    }
    app.rotate_tool = Some(t);
    hint(app);
    true
}

/// Turn the bodies by `degrees` about the protractor's axis through its centre, then wait for
/// the next centre with the same bodies.
fn commit(app: &mut SketApp, degrees: f64) {
    let Some(t) = app.rotate_tool.as_ref() else { return };
    let Some((c, n)) = t.center else { return };
    if !degrees.is_finite() || degrees.abs() < 1e-9 {
        return;
    }
    let params = json!({
        "bodies": t.bodies,
        "axis": [n.x, n.y, n.z],
        "origin": [c.x, c.y, c.z],
        "angle": (degrees * 1e6).round() / 1e6,
        "copy": t.copy,
    });
    match app.run("solid.rotate", params) {
        Ok(_) => {
            let shown = format!("{}°", (degrees * 100.0).round() / 100.0);
            app.set_status(crate::i18n::format("ui.rotate_tool.done", &[("angle", &shown)]), false);
        }
        Err(why) => app.set_status(crate::friendly_error("solid.rotate", &why), true),
    }
    if let Some(t) = app.rotate_tool.as_mut() {
        t.center = None;
        t.start = None;
        t.typed.clear();
        t.angle = 0.0;
    }
}

/// A key while Rotate is open: arrow keys lock the axis, typed digits and Enter give the angle,
/// Backspace takes a character back, Escape steps back a click (or closes Rotate). Whether the
/// key was taken.
pub(crate) fn key(app: &mut SketApp, k: egui::Key, mods: egui::Modifiers) -> bool {
    use egui::Key;
    let Some(t) = app.rotate_tool.as_mut() else { return false };
    if mods.command || mods.alt {
        return false;
    }
    t.ctrl_tap = None;
    let typing = t.start.is_some();
    match k {
        Key::Escape => {
            if !t.typed.is_empty() {
                t.typed.clear();
            } else if t.start.is_some() {
                t.start = None;
            } else if t.center.is_some() {
                t.center = None;
            } else {
                cancel(app);
                return true;
            }
        }
        Key::ArrowRight | Key::ArrowLeft | Key::ArrowUp if t.center.is_none() => {
            let axis = match k {
                Key::ArrowRight => X,
                Key::ArrowLeft => Y,
                _ => Z,
            };
            t.lock = if t.lock == Some(axis) { None } else { Some(axis) };
        }
        Key::ArrowDown if t.center.is_none() => t.lock = None,
        Key::Backspace if typing => drop(t.typed.pop()),
        Key::Enter if typing => {
            let typed = t.typed.trim().replace(',', ".");
            match typed.parse::<f64>() {
                Ok(degrees) if degrees.is_finite() => {
                    commit(app, degrees);
                    return true;
                }
                _ => {
                    app.set_status(app.tr("ui.rotate_tool.bad_angle").to_string(), true);
                    return true;
                }
            }
        }
        _ if typing && t.typed.len() < MAX_TYPED => {
            let c = match k {
                Key::Minus => '-',
                Key::Period | Key::Comma => '.',
                Key::Num0 => '0',
                Key::Num1 => '1',
                Key::Num2 => '2',
                Key::Num3 => '3',
                Key::Num4 => '4',
                Key::Num5 => '5',
                Key::Num6 => '6',
                Key::Num7 => '7',
                Key::Num8 => '8',
                Key::Num9 => '9',
                _ => return false,
            };
            t.typed.push(c);
        }
        _ => return false,
    }
    hint(app);
    true
}

/// The protractor, its start and end rays and the angle, drawn over the view.
pub(crate) fn paint(app: &SketApp, proj: &Proj, painter: &egui::Painter) {
    let Some(t) = &app.rotate_tool else { return };
    let Some((c, n)) = t.center.or(t.hover) else { return };
    let Some(at) = proj.to_screen(c) else { return };
    let tokens = crate::theme::Tokens::view();
    let colour = if n.dot(X).abs() > 0.999 {
        tokens.axis_x
    } else if n.dot(Y).abs() > 0.999 {
        tokens.axis_y
    } else if n.dot(Z).abs() > 0.999 {
        tokens.axis_z
    } else {
        tokens.accent
    };
    // The protractor's size in the model, so it is about RADIUS points across on screen.
    let u = t.start.unwrap_or_else(|| n.any_perp());
    let v = n.cross(u);
    let Some(edge) = proj.to_screen(c + u) else { return };
    let per_mm = at.distance(edge).max(1e-3);
    let r = f64::from(RADIUS / per_mm);
    let ring = |radius: f64, a: f64| proj.to_screen(c + (u * a.cos() + v * a.sin()) * radius);
    let stroke = Stroke::new(1.5, colour);
    let points: Vec<Pos2> = (0..=48).filter_map(|i| ring(r, f64::from(i) / 48.0 * std::f64::consts::TAU)).collect();
    painter.add(Shape::line(points, stroke));
    for i in 0..24 {
        let a = f64::from(i) * SNAP_DEGREES.to_radians();
        let inner = if i % 6 == 0 { 0.8 } else { 0.9 };
        if let (Some(p), Some(q)) = (ring(r * inner, a), ring(r, a)) {
            painter.line_segment([p, q], stroke);
        }
    }
    painter.circle_filled(at, 3.0, colour);
    let Some(start) = t.start else { return };
    let far = r * 1.6;
    if let Some(end) = proj.to_screen(c + start * far) {
        painter.add(Shape::dashed_line(&[at, end], stroke, 6.0, 4.0));
    }
    let a = t.angle.to_radians();
    if let Some(end) = ring(far, a) {
        painter.line_segment([at, end], Stroke::new(2.0, colour));
    }
    let label = if t.typed.is_empty() { format!("{}°", (t.angle * 10.0).round() / 10.0) } else { format!("{}°", t.typed) };
    let label = if t.copy { format!("{label}  {}", app.tr("ui.rotate_tool.copy")) } else { label };
    if let Some(p) = ring(r * 1.25, a / 2.0) {
        painter.text(p, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(tokens.font_label), tokens.text);
    }
}

#[cfg(test)]
#[path = "rotate_tool_tests.rs"]
mod tests;
