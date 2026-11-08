//! Assembly dialogs: New Component, Components from Bodies and Interference, and the snaps
//! hovering geometry finds (a face's centre or a round face's nearer end, an edge's middle or a
//! round edge's centre, a vertex) that Hole placement and other picks share.


use crate::fonts::RichTextExt as _;
use crate::ui_kit::BackingTooltipExt as _;
use crate::ui_kit::{expr_row, text_row};
use egui::{Color32, Pos2, RichText};
use serde_json::{Value, json};
use sketcore_record::Vec3;
use sketcraft_engine::Sel;
use sketcraft_engine::Session;
use sketcraft_engine::geom::prelude::*;
use sketcraft_engine::geom::{Aabb3, Mesh};

use crate::SketApp;
use crate::dialogs::{Dialog, Kind, form_row};
use crate::preview::{Built, Colors};
use crate::selection::{
    BODIES, EDGES, FACES, ORIGIN, POINTS, SelInput, SelectionExt as _, VERTICES, items as sel_items, point_value as pt, polyline_mid,
};
use crate::theme::Tokens;

use crate::viewport::Proj;


/// What a snap input accepts: snaps on faces, edges and vertices, the world origin, and a sketch
/// point (the design origin or a hole centre).
pub const SNAPS: u16 = FACES | EDGES | VERTICES | ORIGIN | POINTS;
/// Commands whose effect is where occurrences sit (previewed on the placed model).
const MOVES: &[&str] = &[
    "occurrence.align",
    "parts.insert",
    "parts.fastener",
];

#[derive(Clone, Debug)]
pub enum Asm {
    FromBodies,
    NewComponent {
        name: String,
        activate: bool,
    },
    Interference {
        result: Option<Value>,
        overlaps: Vec<Mesh>,
        of: Vec<Sel>,
        /// Pairs closer than this (mm, as typed) are listed too.
        clearance: String,
        /// Compute's check under way (its reply lands here).
        check: Option<Check>,
    },
}

/// Interference's check off the window thread: where it leaves its reply (the check's and the
/// shared volumes' meshes), the design and the picks it checked, and when it started.
#[derive(Clone, Debug)]
pub struct Check {
    reply: std::sync::Arc<std::sync::Mutex<Option<(Value, Vec<Mesh>)>>>,
    design: std::sync::Weak<sketcraft_engine::doc::Document>,
    of: Vec<Sel>,
    started: f64,
}

impl Asm {
    pub fn title(&self) -> &'static str {
        match self {
            Asm::FromBodies => "cmd.component_from_bodies",
            Asm::NewComponent { .. } => "NEW COMPONENT",
            Asm::Interference { .. } => "INTERFERENCE",
        }
    }

    pub fn previews(&self) -> bool {
        false
    }
}

/// The dialog for an assembly command: its state and selection inputs.
pub fn start(_app: &SketApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let (asm, inputs) = match id {
        "component.from_bodies" => (Asm::FromBodies, vec![SelInput::new("Bodies", BODIES, true)]),
        "component.create" => (Asm::NewComponent { name: String::new(), activate: true }, vec![SelInput::new("Bodies (optional)", BODIES, true)]),
        "inspect.interference" => (
            Asm::Interference { result: None, overlaps: Vec::new(), of: Vec::new(), clearance: "0".into(), check: None },
            vec![SelInput::new("Bodies (all if none)", BODIES, true)],
        ),
        _ => return None,
    };
    Some((Kind::Assembly(asm), inputs))
}

/// What an open dialog starts on when the control channel's `ui.start` passes `params`: no
/// assembly dialog takes any.
pub fn preset(_s: &Session, k: &mut Asm, _p: &Value) -> Result<(), String> {
    Err(format!("{} takes no params yet", k.title()))
}

/// The box around the bodies of occurrences (and of the occurrences inside them), where they
/// are placed; occurrence 0 is the root, holding the bodies outside every occurrence.
pub fn occurrence_bounds(s: &Session, occs: &[u64]) -> Aabb3 {
    let model = s.model.state();
    let world = crate::open_meshing::session_world(s);
    let mut bb = Aabb3::default();
    for (name, _, path, _) in s.doc.body_frames(&model) {
        let on = occs.iter().any(|o| if *o == 0 { path.is_empty() } else { path.contains(o) });
        if on && let Some(b) = world.body(&name) {
            bb = bb.union(&b.mesh().bounds());
        }
    }
    bb
}

// ---- snaps ----

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapKind {
    FaceCenter,
    CircleCenter,
    EdgeMid,
    Vertex,
}

/// A joint origin snap (world): where it sits, its z axis, and the command's origin parameter.
#[derive(Clone, Debug)]
pub struct SnapPt {
    pub at: Vec3,
    pub z: Vec3,
    pub kind: SnapKind,
    pub param: Value,
}

/// The occurrence placing a world body (0: the root design), the instance its name says.
pub(crate) fn occurrence_of_body(s: &Session, body: &str) -> Option<u64> {
    // The world name tells the instance: a further occurrence of a component is not its first.
    let st = crate::open_meshing::session_world(s);
    let b = st.body(body)?;
    if s.doc.model_body_component(b) == 0 {
        return Some(0);
    }
    let model = s.model.state();
    let (_, _, _, path, _) = s.doc.body_frames_with_source(&model).into_iter().find(|(world, ..)| world == body)?;
    path.last().copied()
}

/// Centre and unit normal of the circle through a polyline's points, if it lies on one.
pub fn circle_of(pts: &[Vec3]) -> Option<(Vec3, Vec3)> {
    let n = pts.len();
    if n < 4 {
        return None;
    }
    let (a, b, c) = (*pts.first()?, *pts.get(n / 3)?, *pts.get(2 * n / 3)?);
    let (ab, ac) = (b - a, c - a);
    let nn = ab.cross(ac);
    let l2 = nn.len2();
    if l2 < 1e-18 {
        return None;
    }
    let center = a + (nn.cross(ab) * ac.len2() + ac.cross(nn) * ab.len2()) / (2.0 * l2);
    let r = center.dist(a);
    if !r.is_finite() || r > 1e6 {
        return None;
    }
    let tol = (r * 2e-3).max(1e-6);
    let normal = nn.normalized()?;
    pts.iter().all(|p| (p.dist(center) - r).abs() < tol && (*p - center).dot(normal).abs() < tol).then_some((center, normal))
}

/// Axis of a straight edge. Its largest component points positive so reversing the
/// tessellation does not reverse a joint's drive direction.
fn straight_edge_axis(pts: &[Vec3]) -> Option<Vec3> {
    let (a, b) = (*pts.first()?, *pts.last()?);
    let dir = (b - a).normalized()?;
    let tol = 1e-6 * a.dist(b).max(1.0);
    if !pts.iter().all(|p| (*p - a).cross(dir).len() < tol) {
        return None;
    }
    let leading = [dir.x, dir.y, dir.z].into_iter().max_by(|a, b| a.abs().total_cmp(&b.abs()))?;
    Some(if leading < 0.0 { -dir } else { dir })
}

/// Normal of the mesh face `f` near `p`.
fn face_normal_near(m: &Mesh, f: u32, p: Vec3) -> Option<Vec3> {
    m.triangles
        .iter()
        .zip(&m.tri_face)
        .filter(|(_, tf)| **tf == f)
        .filter_map(|(t, _)| m.tri(t))
        .min_by(|a, b| ((a[0] + a[1] + a[2]) / 3.0).dist(p).total_cmp(&((b[0] + b[1] + b[2]) / 3.0).dist(p)))
        .and_then(|[a, b, c]| (b - a).cross(c - a).normalized())
}

/// The snap a picked item stands for (world coordinates, as the viewport shows them).
pub fn snap_of(s: &Session, sel: &Sel) -> Option<SnapPt> {
    let st = crate::open_meshing::session_world(s);
    let (body, mut snap) = match sel {
        Sel::Face { body, index, point } => {
            let b = st.body(body)?;
            let tol = (b.body.size() * 2e-3).max(1e-3);
            let f = b.body.faces(tol).ok()?.into_iter().find(|f| f.index == *index)?;
            match f.plane_normal {
                Some(n) => (body.as_str(), SnapPt { at: f.centroid, z: n, kind: SnapKind::FaceCenter, param: json!({"face": pt(*point)}) }),
                None => {
                    let c = sketcraft_engine::kernel::cylinder_face_at(&b.body, *point)?;
                    let sv = (*point - c.axis_point).dot(c.axis);
                    let (at, dir) = if (sv - c.start).abs() <= (c.end - sv).abs() { (c.start, -1.0) } else { (c.end, 1.0) };
                    (
                        body.as_str(),
                        SnapPt {
                            at: c.axis_point + c.axis * at,
                            z: c.axis * dir,
                            kind: SnapKind::CircleCenter,
                            param: json!({"circle": pt(*point)}),
                        },
                    )
                }
            }
        }
        Sel::Edge { body, index, point } => {
            let b = st.body(body)?;
            let m = b.mesh();
            let e = m.edges.get(*index)?;
            match circle_of(e) {
                Some((c, n)) => {
                    // The axis points out of the material: away from the faces' side.
                    let faces = m.edge_faces.get(*index).cloned().unwrap_or_default();
                    let out = faces.iter().filter_map(|f| face_normal_near(&m, *f, *point)).fold(Vec3::ZERO, |a, x| a + x);
                    let z = if out.dot(n) < 0.0 { -n } else { n };
                    // Its round end, kept by name so it follows the hole or boss as the part
                    // changes (a point where it has none).
                    (body.as_str(), SnapPt { at: c, z, kind: SnapKind::CircleCenter, param: json!({"circle": pt(c), "z": pt(z)}) })
                }
                None => {
                    let mid = polyline_mid(e);
                    let z = straight_edge_axis(e)
                        .or_else(|| m.edge_faces.get(*index).and_then(|fs| fs.first()).and_then(|f| face_normal_near(&m, *f, mid)))
                        .unwrap_or(Vec3::Z);
                    // Its middle, kept by the edge's name.
                    (body.as_str(), SnapPt { at: mid, z, kind: SnapKind::EdgeMid, param: json!({"edge": pt(mid), "z": pt(z)}) })
                }
            }
        }
        Sel::Vertex { body, point } => {
            // The corner of the edges meeting there, kept by their names, z along the part's +Z
            // as a point's is.
            let z = occurrence_of_body(s, body)
                .map_or(Vec3::Z, |o| sketcraft_engine::doc::apply_vector(&sketcraft_engine::doc::joints::occurrence_world(&s.doc, o), Vec3::Z));
            (
                body.as_str(),
                SnapPt { at: *point, z: Vec3::Z, kind: SnapKind::Vertex, param: json!({"edge": pt(*point), "corner": pt(*point), "z": pt(z)}) },
            )
        }
        Sel::SketchPoint { id } => {
            // A sketch point's world position, its plane's normal as the joint's z, kept by its
            // sketch and id so it follows the sketch as the part changes. No body names it, so the
            // engine finds the occurrence from the world point (its own body).
            let (sketch, point) = match id.split_once(':') {
                Some((sketch, point)) => (sketch.parse().ok()?, point),
                None => (s.active_sketch?, id.as_str()),
            };
            let ss = st.sketch(sketch)?;
            let at = ss.plane.to_world(ss.sketch.point(ss.sketch.resolve_point(point)?)?);
            let z = ss.plane.normal();
            ("", SnapPt { at, z, kind: SnapKind::Vertex, param: json!({"point": pt(at), "z": pt(z)}) })
        }
        _ => return None,
    };
    if let (Some(occ), Value::Object(m)) = (occurrence_of_body(s, body), &mut snap.param) {
        m.insert("occurrence".into(), json!(occ));
    }
    Some(snap)
}

// ---- rows ----

/// The dialog's own rows (after the selection inputs); true when Enter was pressed in a value.
pub fn rows(app: &mut SketApp, ui: &mut egui::Ui, k: &mut Asm, inputs: &mut [SelInput], active: &mut usize) -> bool {
    let mut enter = false;
    match k {
        Asm::FromBodies => {
            ui.add(egui::Label::new(app.tr("ui.components_from_bodies_hint")).wrap());
            if crate::ui_kit::button(ui, app.tr("ui.select_all"), crate::ui_kit::Button::Normal).clicked()
                && let Some(input) = inputs.first_mut()
            {
                // Transfers take definition names, rather than the names of repeated placements.
                input.items = app.session.model.state().bodies.iter().map(|body| Sel::Body { name: body.name.clone() }).collect();
                *active = 0;
            }
        }
        Asm::NewComponent { name, activate } => {
            enter |= text_row(ui, "Name", name, crate::i18n::tr_label("Component1")).0;
            crate::ui_kit::options(ui, "component_options", &mut [("Activate", activate)]);
        }
        Asm::Interference { result, overlaps, of, clearance, check } => {
            let picked: Vec<Sel> = inputs.first().map(|i| i.items.clone()).unwrap_or_default();
            if let Some(c) = check.take() {
                match c.reply.lock().map(|mut r| r.take()).map_err(|_| ()) {
                    Ok(None) => {
                        *check = Some(c);
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                    }
                    // Done: shown unless the design changed meanwhile (a script, the control
                    // channel, undo). A poisoned lock (its thread panicked) leaves nothing.
                    Ok(Some((r, o))) => {
                        if c.design.upgrade().is_some_and(|d| std::sync::Arc::ptr_eq(&d, &app.session.doc)) {
                            (*result, *overlaps, *of) = (Some(r), o, c.of);
                        }
                        ui.ctx().request_repaint();
                    }
                    Err(()) => {}
                }
            }
            enter |= expr_row(ui, "Clearance", clearance);
            // While it checks: the seconds so far, and Cancel where Compute was.
            if let Some(started) = check.as_ref().map(|c| c.started) {
                let t = Tokens::get();
                form_row(ui, "", |ui| {
                    ui.horizontal(|ui| {
                        let seconds = ((crate::now_ms() - started) / 1000.0).floor().max(0.0);
                        ui.add(egui::Spinner::new().size(t.font_small));
                        ui.label(RichText::new(crate::i18n::format("dialog.computing", &[("seconds", &format!("{seconds:.0}"))])).color(t.text_dim));
                    });
                });
            }
            form_row(ui, "", |ui| {
                if check.is_some() {
                    if crate::ui_kit::button(ui, crate::i18n::tr("ui.cancel"), crate::ui_kit::Button::Normal).clicked() {
                        // Its thread finishes the check; the reply is left unread.
                        *check = None;
                    }
                    return;
                }
                let stale = result.is_some() && *of != picked;
                if crate::ui_kit::button(
                    ui,
                    crate::i18n::tr(if stale { "dialog.compute_again" } else { "dialog.compute" }),
                    crate::ui_kit::Button::Normal,
                )
                .clicked()
                {
                    (*result, *overlaps) = (None, Vec::new());
                    *check = Some(interference(app, &picked, clearance));
                    ui.ctx().request_repaint();
                }
            });
            if check.is_none()
                && let Some(r) = result.as_ref()
            {
                interference_rows(app, ui, r);
            }
        }
    }
    enter
}

/// Start Compute's check (overlaps, pairs closer than the clearance, each body's nearest
/// neighbour) and find the overlapping volumes (world placement) to show in red. Off the window
/// thread, on a kernel thread, when the app runs commands there: a slow pair froze the window
/// until the app was closed (#11483). Else at once.
fn interference(app: &SketApp, picked: &[Sel], clearance: &str) -> Check {
    let check =
        Check { reply: Default::default(), design: std::sync::Arc::downgrade(&app.session.doc), of: picked.to_vec(), started: crate::now_ms() };
    let names: Vec<String> = picked.body_names();
    let (scale, _) = crate::prefs::unit_scale(&app.session.doc.units);
    let reply = match crate::prefs::parse_length_mm(clearance, scale) {
        Some(v) if v >= 0.0 => {
            let mut p = json!({ "clearance": v, "nearest": 1 });
            if !names.is_empty() {
                p["bodies"] = json!(names);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if app.apply_off_thread.is_some() {
                let (ahead, world, out, params) =
                    (app.session.ahead(), crate::open_meshing::session_world(&app.session), check.reply.clone(), p.clone());
                let spawned = sketcraft_engine::kernel::kernel_thread("interference").spawn(move || {
                    let r = check_now(ahead, &world, params);
                    if let Ok(mut slot) = out.lock() {
                        *slot = Some(r);
                    }
                });
                if spawned.is_ok() {
                    return check;
                }
            }
            check_now(app.session.ahead(), &crate::open_meshing::session_world(&app.session), p)
        }
        _ => (json!({ "error": crate::i18n::tr("dialog.clearance_invalid") }), Vec::new()),
    };
    if let Ok(mut slot) = check.reply.lock() {
        *slot = Some(reply);
    }
    check
}

/// `inspect.clearance` with `p` on a copy of the session, and the volume each pair it finds
/// overlapping shares ([`overlap_meshes`]).
fn check_now(ahead: sketcraft_engine::Ahead, world: &sketcraft_engine::doc::ModelState, p: Value) -> (Value, Vec<Mesh>) {
    // One reply for the one command.
    match ahead.replies(&[("inspect.clearance".to_string(), p)]).pop() {
        Some(Ok(r)) => {
            let meshes = overlap_meshes(world, &r);
            (r, meshes)
        }
        Some(Err(e)) => (json!({ "error": e.to_string() }), Vec::new()),
        None => (json!({}), Vec::new()),
    }
}

/// The volume each pair in an `inspect.clearance` reply's `overlaps` shares, meshed in world
/// placement. Each pair runs on a kernel worker with the kernel's stack, whatever thread asks
/// (the window thread's is 1 MiB on Windows).
fn overlap_meshes(st: &sketcraft_engine::doc::ModelState, r: &Value) -> Vec<Mesh> {
    use sketcraft_engine::kernel;
    let pairs: Vec<_> =
        r["overlaps"].as_array().into_iter().flatten().filter_map(|hit| Some((st.body(hit["a"].as_str()?)?, st.body(hit["b"].as_str()?)?))).collect();
    kernel::map_with_local_pools(&pairs, usize::MAX, |&(a, b)| {
        let Some(o) = kernel::boolean(&a.body, &b.body, kernel::BoolOp::Intersect)? else { return Ok(None) };
        o.mesh(&kernel::MeshTolerance::chord((o.size() * 2e-3).max(1e-3))).map(Some)
    })
    .into_iter()
    .filter_map(|m| m.ok().flatten())
    .collect()
}

fn interference_rows(app: &SketApp, ui: &mut egui::Ui, r: &Value) {
    let t = Tokens::get();
    if let Some(e) = r.get("error").and_then(Value::as_str) {
        crate::ui_kit::status_line(ui, e, crate::ui_kit::StatusTone::Error);
        return;
    }
    let hits = r["overlaps"].as_array().cloned().unwrap_or_default();
    form_row(ui, "Checked", |ui| {
        ui.label(crate::i18n::format("dialog.bodies_count", &[("count", &r["bodies"].as_u64().unwrap_or(0).to_string())]));
    });
    let unchecked = r["unchecked"].as_array().cloned().unwrap_or_default();
    let near = r["near"].as_array().cloned().unwrap_or_default();
    let pair = |h: &Value| format!("{} · {}", h["a"].as_str().unwrap_or(""), h["b"].as_str().unwrap_or(""));
    if hits.is_empty() && unchecked.is_empty() {
        crate::ui_kit::status_line(ui, "no interference", crate::ui_kit::StatusTone::Dim);
    }
    // Pairs closer than the clearance, nearest first.
    if !near.is_empty() {
        form_row(ui, "Too close", |ui| {
            ui.label(RichText::new(crate::i18n::tr("ui.distance")).semibold().color(crate::theme::Tokens::get().text));
        });
    }
    for h in &near {
        form_row(ui, &pair(h), |ui| {
            ui.label(RichText::new(crate::prefs::show_mm(app, h["distance_mm"].as_f64().unwrap_or(0.0), 1)).color(t.warning));
        });
    }
    // Each body's nearest neighbour.
    let nearest = r["nearest"].as_object().cloned().unwrap_or_default();
    if !nearest.is_empty() {
        form_row(ui, "Nearest", |ui| {
            ui.label(RichText::new(crate::i18n::tr("ui.distance")).semibold().color(crate::theme::Tokens::get().text));
        });
    }
    for (body, list) in &nearest {
        let Some(n) = list.get(0) else { continue };
        form_row(ui, &format!("{body} → {}", n["body"].as_str().unwrap_or("")), |ui| {
            ui.label(crate::prefs::show_mm(app, n["distance_mm"].as_f64().unwrap_or(0.0), 1));
        });
    }
    if hits.is_empty() && unchecked.is_empty() {
        return;
    }
    if !hits.is_empty() {
        form_row(ui, "Interferes", |ui| {
            ui.label(RichText::new(crate::i18n::tr_label("Volume")).semibold().color(crate::theme::Tokens::get().text));
        });
    }
    for h in hits {
        form_row(ui, &format!("{} · {}", h["a"].as_str().unwrap_or(""), h["b"].as_str().unwrap_or("")), |ui| {
            ui.label(RichText::new(crate::prefs::show_mm(app, h["volume_mm3"].as_f64().unwrap_or(0.0), 3)).color(t.error));
        });
    }
    // Pairs the check could not work out: not clear, not known to interfere.
    for u in unchecked {
        form_row(ui, &format!("{} · {}", u["a"].as_str().unwrap_or(""), u["b"].as_str().unwrap_or("")), |ui| {
            ui.label(RichText::new(crate::i18n::tr("dialog.could_not_be_checked")).color(t.warning))
                .backing_on_hover_text(u["reason"].as_str().unwrap_or(""));
        });
    }
}

// ---- commands ----

/// The commands the dialog's OK runs.
pub fn commands(_app: &SketApp, k: &Asm, inputs: &[SelInput]) -> Result<Vec<(String, Value)>, String> {
    let cmd = |id: &str, p: Value| vec![(id.to_string(), p)];
    Ok(match k {
        Asm::FromBodies => {
            let bodies = sel_items(inputs, 0).body_names();
            if bodies.is_empty() {
                return Err(crate::i18n::tr("ui.components_from_bodies_required").into());
            }
            cmd("component.from_bodies", json!({ "bodies": bodies }))
        }
        Asm::NewComponent { name, activate } => {
            let bodies: Vec<String> = sel_items(inputs, 0).body_names();
            if bodies.is_empty() {
                let mut p = json!({ "activate": activate });
                if !name.trim().is_empty() {
                    p["name"] = json!(name.trim());
                }
                cmd("component.create", p)
            } else {
                cmd("component.from_bodies", json!({ "bodies": bodies }))
            }
        }
        // The check runs from its Compute button; OK only closes.
        Asm::Interference { .. } => Vec::new(),
    })
}

/// Joint and drive previews: the commands run on a scratch copy and the placed model before and
/// after is compared, so the components that move show where they go (`None`: not such
/// commands).
pub fn world_preview(s: &Session, cmds: &[(String, Value)], colors: Colors) -> Option<Result<Built, String>> {
    // Any feature, when components are placed away from their frames: the preview must show
    // the world the viewport shows.
    let moves = cmds.iter().all(|(c, _)| MOVES.contains(&c.as_str()));
    if cmds.is_empty() || !(moves || sketcraft_engine::frames::placed(s)) {
        return None;
    }
    let preview = match s.preview_visible_world(cmds) {
        Ok(preview) => preview,
        Err(error) => return Some(Err(error.to_string())),
    };
    let stopped = cmds
        .iter()
        .zip(&preview.replies)
        .find(|((c, _), r)| {
            c == "joint.drive" && (r["stopped_by_loop"].is_object() || r["stopped_by_contact"].is_object() || r["unconnected"].is_string())
        })
        .map(|(_, r)| r.clone());
    let (before, after) = (preview.before, preview.after);
    // A moved component looks like itself (no new surface), with its old place as the ghost.
    let colors = Colors { added: colors.body, new_body: colors.body, ..colors };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::preview::build(&before, &after, colors)));
    Some(match built {
        Ok(mut b) => {
            if !colors.ghost {
                b.scene.ghost.clear();
            }
            b.bounds = after.bodies.iter().fold(Aabb3::EMPTY, |acc, x| acc.union(&x.mesh().bounds()));
            b.stopped = stopped;
            Ok(b)
        }
        Err(_) => Err("the preview failed".into()),
    })
}

// ---- overlay ----

/// Snap glyphs under the cursor and at the picked origins; the overlap of an interference.
pub fn overlay(_app: &SketApp, painter: &egui::Painter, proj: &Proj, d: &mut Dialog) {
    let t = Tokens::get();
    let Kind::Assembly(k) = &mut d.kind else { return };
    if let Asm::Interference { overlaps, .. } = k {
        // Front faces only, shaded by how they face the view.
        let back = proj.cam.back();
        for m in overlaps.iter() {
            let mut em = egui::Mesh::default();
            for tri in &m.triangles {
                let Some(ps) = m.tri(tri) else { continue };
                let Some(n) = (ps[1] - ps[0]).cross(ps[2] - ps[0]).normalized() else { continue };
                let facing = n.dot(back);
                if facing <= 0.0 {
                    continue;
                }
                let k = 0.55 + 0.45 * facing;
                let shade = |c: u8| (f64::from(c) * k) as u8;
                let ic = t.interference;
                let fill = Color32::from_rgba_unmultiplied(shade(ic.r()), shade(ic.g()), shade(ic.b()), 210);
                let sp: Vec<Pos2> = ps.iter().filter_map(|p| proj.to_screen(*p)).collect();
                if sp.len() == 3 {
                    let base = em.vertices.len() as u32;
                    for p in sp {
                        em.colored_vertex(p, fill);
                    }
                    em.add_triangle(base, base + 1, base + 2);
                }
            }
            painter.add(egui::Shape::mesh(em));
        }
        return;
    }
}

/// What a snap is called (the hover label).
pub(crate) fn snap_name(k: SnapKind) -> &'static str {
    match k {
        SnapKind::FaceCenter => "Face center",
        SnapKind::CircleCenter => "Circle center",
        SnapKind::EdgeMid => "Edge midpoint",
        SnapKind::Vertex => "Vertex",
    }
}

// ---- timeline and browser ----

#[cfg(test)]
#[path = "dialogs_assembly_tests.rs"]
mod tests;

thread_local! {
    /// Named joint origins in the world, for the design state they were found in.
    static ORIGINS: std::cell::RefCell<Option<(String, Vec<(String, sketcraft_engine::doc::Mat)>)>> = const { std::cell::RefCell::new(None) };
}

thread_local! {
    /// Joint frames in the world (joint id → A's frame), for the design state they were found in.
    static JOINT_FRAMES: std::cell::RefCell<Option<(String, Vec<(u64, sketcraft_engine::doc::Mat)>)>> = const { std::cell::RefCell::new(None) };
}

thread_local! {
    /// The size (box diagonal) of each occurrence's bodies, for the design revision it was found in.
    static OCCURRENCE_SIZES: std::cell::RefCell<Option<(u64, std::collections::HashMap<u64, f64>)>> = const { std::cell::RefCell::new(None) };
}


