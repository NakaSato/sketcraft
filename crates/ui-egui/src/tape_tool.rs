//! SketchUp's Tape Measure on the SketchUp tab. Click a point, then another: the distance shows
//! in the status line. Started on a straight edge, it makes a guide line parallel to the edge at
//! the distance dragged (or typed); started on a vertex, a guide point at the second point (or
//! the typed distance towards it). Tapping Ctrl (Option on a Mac) turns guide making off and on,
//! as in SketchUp. Guides are construction axes and points (`construct.axis.two_points`,
//! `construct.point.vertex`), one undo step each, saved with the design.
//!
//! The other tabs keep Measure (`inspect.measure`) as it is.

use egui::{Pos2, Shape, Stroke};
use serde_json::json;
use sketcore_record::Vec3;
use sketcraft_engine::doc::expr::Kind as ValueKind;
use sketcraft_engine::geom::prelude::*;

use crate::SketApp;
use crate::viewport::{Hit, Proj};

/// How far the pointer may move while Ctrl is held for the press to still be a tap, in points.
const TAP_SLOP: f32 = 4.0;
/// The longest length a person types, in characters.
const MAX_TYPED: usize = 32;
/// How long a guide line is drawn in the preview, either side of its start, in mm.
const PREVIEW_REACH: f64 = 1e4;

/// Where the tape starts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum From {
    /// On a straight edge (`dir` its unit direction): the guide is a parallel line.
    Edge { at: Vec3, dir: Vec3 },
    /// On a vertex: the guide is a point.
    Vertex { at: Vec3 },
    /// Anywhere else: the tape only measures.
    Point { at: Vec3 },
}

impl From {
    fn at(self) -> Vec3 {
        match self {
            From::Edge { at, .. } | From::Vertex { at } | From::Point { at } => at,
        }
    }
}

/// The Tape Measure while it is open on the SketchUp tab.
#[derive(Clone, Debug)]
pub(crate) struct TapeTool {
    from: Option<From>,
    /// Where the pointer is on the model (or on the plane the tape is drawn in).
    pointer: Option<Vec3>,
    /// What has been typed since the first click.
    typed: String,
    /// Whether the second click (or Enter) makes a guide.
    guides: bool,
    ctrl_tap: Option<Pos2>,
    ctrl_down: bool,
}

impl Default for TapeTool {
    fn default() -> TapeTool {
        TapeTool { from: None, pointer: None, typed: String::new(), guides: true, ctrl_tap: None, ctrl_down: false }
    }
}

/// Whether starting `id` opens this tool: Measure on the SketchUp tab.
pub(crate) fn wanted(app: &SketApp, id: &str) -> bool {
    id == "inspect.measure" && app.ui.tab == crate::workspace::SKETCHUP_TAB
}

pub(crate) fn active(app: &SketApp) -> bool {
    app.tape_tool.is_some()
}

pub(crate) fn begin(app: &mut SketApp) {
    if app.dialog.is_some() {
        crate::dialogs::cancel(app);
        app.dialog = None;
    }
    app.tool = None;
    app.tape_tool = Some(TapeTool::default());
    hint(app);
}

/// Close the Tape Measure. Whether it was open.
pub(crate) fn cancel(app: &mut SketApp) -> bool {
    let was = app.tape_tool.take().is_some();
    if was {
        app.set_status(String::new(), false);
    }
    was
}

fn hint(app: &mut SketApp) {
    let Some(t) = &app.tape_tool else { return };
    let mut text = app.tr(if t.from.is_some() { "ui.tape_tool.second" } else { "ui.tape_tool.first" }).to_string();
    if !t.guides {
        text = format!("{text} {}", app.tr("ui.tape_tool.no_guides"));
    }
    app.set_status(text, false);
}

/// The unit direction of a straight edge of `body`, when edge `index` is one.
fn straight_edge(app: &SketApp, body: &str, index: usize) -> Option<Vec3> {
    let st = crate::open_meshing::session_world(&app.session);
    let b = st.body(body)?;
    let tol = (b.body.size() * 1e-3).max(1e-3);
    let edge = b.body.edges(tol).ok()?.into_iter().find(|e| e.index == index)?;
    let (first, last) = (*edge.points.first()?, *edge.points.last()?);
    let dir = (last - first).normalized()?;
    // Straight: every point on the line through its ends.
    let length = first.dist(last);
    edge.points.iter().all(|p| (*p - first).cross(dir).len() <= tol.max(length * 1e-6)).then_some(dir)
}

/// Where a click on `hits` (front first) starts the tape.
fn from_hits(app: &SketApp, hits: &[Hit]) -> Option<From> {
    for hit in hits {
        return Some(match hit {
            Hit::Vertex { point, .. } => From::Vertex { at: *point },
            Hit::Edge { body, index, point, .. } => match straight_edge(app, body, *index) {
                Some(dir) => From::Edge { at: *point, dir },
                None => From::Point { at: *point },
            },
            Hit::Face { point, .. } => From::Point { at: *point },
            _ => continue,
        });
    }
    None
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

/// The pointer's point: on the model under it, else on the plane through the start that faces
/// the view (and holds an edge start's direction, so a guide line can be dragged off it).
fn pointer_at(t: &TapeTool, proj: &Proj, pos: Pos2, hits: &[Hit]) -> Option<Vec3> {
    let on_model = hits.iter().find_map(|h| match h {
        Hit::Vertex { point, .. } | Hit::Face { point, .. } => Some(*point),
        Hit::Edge { point, .. } => Some(*point),
        _ => None,
    });
    if on_model.is_some() {
        return on_model;
    }
    let from = t.from?;
    let (_, view) = proj.ray(pos);
    let n = match from {
        From::Edge { dir, .. } => view - dir * view.dot(dir),
        _ => view,
    };
    on_plane(proj, pos, from.at(), n.normalized()?)
}

/// The guide line a pointer at `p` would make from an edge start: through `at + offset`, along
/// `dir`, where the offset is the part of `p - at` across the edge.
fn guide_offset(at: Vec3, dir: Vec3, p: Vec3) -> Vec3 {
    let v = p - at;
    v - dir * v.dot(dir)
}

/// The pointer moved (every frame while the tape is open).
pub(crate) fn hover(app: &mut SketApp, proj: &Proj, pos: Option<Pos2>, hits: &[Hit], ctrl: bool) {
    let Some(mut t) = app.tape_tool.take() else { return };
    let mut toggled = false;
    if ctrl && !t.ctrl_down {
        t.ctrl_tap = pos;
    } else if ctrl {
        if let (Some(at), Some(now)) = (t.ctrl_tap, pos)
            && at.distance(now) > TAP_SLOP
        {
            t.ctrl_tap = None;
        }
    } else if t.ctrl_down && t.ctrl_tap.take().is_some() {
        t.guides = !t.guides;
        toggled = true;
    }
    t.ctrl_down = ctrl;
    if let Some(pos) = pos {
        t.pointer = pointer_at(&t, proj, pos, hits);
    }
    app.tape_tool = Some(t);
    if toggled {
        hint(app);
    }
}

/// A click while the tape is open. Whether it was taken (always, while it is).
pub(crate) fn click(app: &mut SketApp, proj: &Proj, pos: Pos2, hits: &[Hit]) -> bool {
    let Some(mut t) = app.tape_tool.take() else { return false };
    t.ctrl_tap = None;
    match t.from {
        None => {
            t.from = from_hits(app, hits);
            t.typed.clear();
            app.tape_tool = Some(t);
            hint(app);
        }
        Some(from) => {
            let end = pointer_at(&t, proj, pos, hits).or(t.pointer);
            app.tape_tool = Some(t);
            if let Some(end) = end {
                finish(app, from, end);
            }
        }
    }
    true
}

/// The second point is `end`: show the distance, make the guide, and start over.
fn finish(app: &mut SketApp, from: From, end: Vec3) {
    let guides = app.tape_tool.as_ref().is_some_and(|t| t.guides);
    let distance = match from {
        From::Edge { at, dir } => guide_offset(at, dir, end).len(),
        _ => from.at().dist(end),
    };
    let shown = crate::length_input::shown(&app.session.doc, distance);
    app.set_status(crate::i18n::format("ui.tape_tool.distance", &[("distance", &shown)]), false);
    if guides && distance > 1e-6 {
        let made = match from {
            From::Edge { at, dir } => {
                let a = at + guide_offset(at, dir, end);
                let b = a + dir * distance.max(1.0);
                app.run("construct.axis.two_points", json!({"a": {"point": [a.x, a.y, a.z]}, "b": {"point": [b.x, b.y, b.z]}}))
            }
            From::Vertex { .. } => app.run("construct.point.vertex", json!({"point": {"point": [end.x, end.y, end.z]}})),
            From::Point { .. } => Ok(serde_json::Value::Null),
        };
        if let Err(why) = made {
            app.set_status(why, true);
        }
    }
    if let Some(t) = app.tape_tool.as_mut() {
        t.from = None;
        t.typed.clear();
    }
}

/// A key while the tape is open: a typed length and Enter, Backspace, Escape (steps back, then
/// closes). Whether the key was taken.
pub(crate) fn key(app: &mut SketApp, k: egui::Key, mods: egui::Modifiers) -> bool {
    use egui::Key;
    let Some(t) = app.tape_tool.as_mut() else { return false };
    if mods.command || mods.alt {
        return false;
    }
    t.ctrl_tap = None;
    let typing = t.from.is_some();
    match k {
        Key::Escape => {
            if !t.typed.is_empty() {
                t.typed.clear();
            } else if t.from.is_some() {
                t.from = None;
            } else {
                cancel(app);
                return true;
            }
        }
        Key::Backspace if typing => drop(t.typed.pop()),
        Key::Enter if typing => {
            let (Some(from), Some(pointer)) = (t.from, t.pointer) else { return true };
            let typed = t.typed.trim().to_string();
            let mm = match crate::length_input::eval(&app.session.doc, &typed, ValueKind::Length) {
                Ok(mm) if mm.is_finite() && mm > 0.0 => mm,
                _ => {
                    app.set_status(app.tr("ui.tape_tool.bad_length").to_string(), true);
                    return true;
                }
            };
            // The typed distance, the way the pointer points from the start.
            let towards = match from {
                From::Edge { at, dir } => guide_offset(at, dir, pointer),
                _ => pointer - from.at(),
            };
            let Some(unit) = towards.normalized() else { return true };
            finish(app, from, from.at() + unit * mm);
            return true;
        }
        // Lengths carry units and sums (`2.5m`, `3'6"`, `10+5`): while one is typed, its letters
        // are not shortcuts.
        _ if typing => {
            let Some(c) = typed_char(k, mods.shift) else { return false };
            if t.typed.len() < MAX_TYPED {
                t.typed.push(c);
            }
        }
        _ => return false,
    }
    hint(app);
    true
}

/// The character a key types into a length, if it types one.
fn typed_char(k: egui::Key, shift: bool) -> Option<char> {
    use egui::Key;
    Some(match k {
        Key::Minus => '-',
        Key::Plus => '+',
        Key::Equals if shift => '+',
        Key::Period => '.',
        Key::Comma => ',',
        Key::Slash => '/',
        Key::Space => ' ',
        Key::Quote if shift => '"',
        Key::Quote => '\'',
        _ => {
            let name = k.name();
            let mut chars = name.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else { return None };
            if !c.is_ascii_alphanumeric() {
                return None;
            }
            c.to_ascii_lowercase()
        }
    })
}

/// The tape, its distance and the guide it would make, drawn over the view.
pub(crate) fn paint(app: &SketApp, proj: &Proj, painter: &egui::Painter) {
    let Some(t) = &app.tape_tool else { return };
    let tokens = crate::theme::Tokens::view();
    let Some(from) = t.from else {
        if let Some(p) = t.pointer.and_then(|p| proj.to_screen(p)) {
            painter.circle_stroke(p, 4.0, Stroke::new(1.5, tokens.accent));
        }
        return;
    };
    let Some(pointer) = t.pointer else { return };
    let Some(a) = proj.to_screen(from.at()) else { return };
    let stroke = Stroke::new(1.5, tokens.accent);
    let (end, distance) = match from {
        From::Edge { at, dir } => {
            let offset = guide_offset(at, dir, pointer);
            if t.guides {
                let base = at + offset;
                if let (Some(p), Some(q)) = (proj.to_screen(base - dir * PREVIEW_REACH), proj.to_screen(base + dir * PREVIEW_REACH)) {
                    painter.add(Shape::dashed_line(&[p, q], stroke, 8.0, 5.0));
                }
            }
            (at + offset, offset.len())
        }
        _ => (pointer, from.at().dist(pointer)),
    };
    let Some(b) = proj.to_screen(end) else { return };
    painter.add(Shape::dashed_line(&[a, b], Stroke::new(1.0, tokens.text), 4.0, 3.0));
    painter.circle_filled(a, 3.0, tokens.accent);
    let label = if t.typed.is_empty() { crate::length_input::shown(&app.session.doc, distance) } else { t.typed.clone() };
    let mid = a + (b - a) * 0.5 + egui::vec2(0.0, -12.0);
    painter.text(mid, egui::Align2::CENTER_BOTTOM, label, egui::FontId::proportional(tokens.font_label), tokens.text);
}

#[cfg(test)]
#[path = "tape_tool_tests.rs"]
mod tests;
