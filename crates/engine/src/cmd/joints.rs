//! Joints between occurrences (`sketcraft_doc::joints`): Joint Origin, Joint, As-built Joint,
//! Rigid Group, Drive Joints, Motion Link, limits, and Interference. Every change re-solves the
//! joint graph and moves the occurrences it places; the session re-solves after any edit too,
//! so joints follow the geometry their origins snap to.

use sketcraft_geom::prelude::*;
use std::collections::BTreeMap;

use serde_json::{Value, json};
use sketcore_record::Vec3;
use sketcraft_doc::expr::{self, Kind};
use sketcraft_doc::joints::{self, Joint, JointOrigin, Snap, XRef};
use sketcraft_geom::Aabb3;

use crate::params::{bad, str_, vec3};
use crate::{Result, Session};



/// Re-solve the joints and move the occurrences they place (after any edit). Gears, racks and
/// screws first take their ratios from the radii and pitches they are defined by.
pub fn resolve(s: &mut Session) -> joints::Solution {
    reexpress(s);
    recouple(s);
    resolve_holding(s, &BTreeMap::new())
}

/// Update placements after an edit without computing a DOF report that the caller discards.
/// Expressions and geometry-dependent couplings are refreshed just as in [`resolve`].
pub fn place(s: &mut Session) -> joints::Solution {
    reexpress(s);
    recouple(s);
    place_holding(s, &BTreeMap::new())
}

/// Work out the offsets, angles and values joints were given as expressions again (a parameter
/// may have changed). One that can't be worked out (its parameter gone) keeps its numbers; the
/// solve leaves that joint out and says why.
fn reexpress(s: &mut Session) {
    let params = joints::Params::new(&s.doc);
    let changed: Vec<(usize, (f64, f64, Vec<f64>))> = s
        .doc
        .assembly
        .joints
        .iter()
        .enumerate()
        .filter(|(_, j)| !j.exprs.is_empty())
        .filter_map(|(i, j)| {
            let (o, a, mut q) = joints::expressed(&params, j).ok()?;
            joints::clamp(j, &mut q);
            ((o, a) != (j.offset, j.angle) || q != j.values).then_some((i, (o, a, q)))
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    let doc = s.doc_mut();
    for (i, (o, a, q)) in changed {
        if let Some(j) = doc.assembly.joints.get_mut(i) {
            (j.offset, j.angle, j.values) = (o, a, q);
        }
    }
}

/// Work out the ratios (and cam tables) of the coupled motion links again (a radius, a parameter
/// or a cam's profile may have changed). A coupling that can't be worked out (its face gone)
/// keeps its last ratio and table; `joint.list` says why.
fn recouple(s: &mut Session) {
    if s.doc.assembly.links.iter().all(|l| l.coupling.is_none()) {
        return;
    }
    let st = s.model.state();
    let changed: Vec<(usize, (f64, Vec<f64>))> = s
        .doc
        .assembly
        .links
        .iter()
        .enumerate()
        .filter_map(|(i, l)| joints::coupling_values(&s.doc, &st, l)?.ok().filter(|(r, t)| *r != l.ratio || *t != l.lift).map(|v| (i, v)))
        .collect();
    if changed.is_empty() {
        return;
    }
    let doc = s.doc_mut();
    for (i, (r, t)) in changed {
        if let Some(l) = doc.assembly.links.get_mut(i) {
            l.ratio = r;
            l.lift = t;
        }
    }
}

/// Re-solve the joints, keeping the `held` values (the ones being driven, mapped to the values
/// they move from) where loops are closed.
pub fn resolve_holding(s: &mut Session, held: &joints::Held) -> joints::Solution {
    resolve_with_analysis(s, held, joints::Analysis::DegreesOfFreedom)
}

/// Contact and loop-search trials need placements and conflicts, not a DOF report.
pub(super) fn place_holding(s: &mut Session, held: &joints::Held) -> joints::Solution {
    resolve_with_analysis(s, held, joints::Analysis::Placement)
}

fn solution(s: &Session, held: &joints::Held, analysis: joints::Analysis) -> joints::Solution {
    joints::solve_holding_with_analysis(&s.doc, &s.model.state(), held, analysis)
}

fn resolve_with_analysis(s: &mut Session, held: &joints::Held, analysis: joints::Analysis) -> joints::Solution {
    let sol = solution(s, held, analysis);
    if !sol.world.is_empty() {
        let mut next = (*s.doc).clone();
        if next.apply_joint_solution(&sol) {
            *s.doc_mut() = next;
            s.revision += 1;
        }
    }
    sol
}

/// A joint's values as `joint.list` gives them: degrees for angles, mm for distances, each with
/// its name and limits. The numbers are rounded for reading (#10747); the joint keeps full
/// precision.
pub(crate) fn named_values(j: &Joint, q: &[f64]) -> Vec<Value> {
    j.kind
        .dofs()
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (v, angle) = (q.get(i).copied().unwrap_or(0.0), j.kind.is_angle(i));
            json!({"name": n, "value": expr::round_reported(if angle { v.to_degrees() } else { v }), "unit": if angle { "deg" } else { "mm" },
                "limits": j.limits.get(i).copied().flatten().map(|(a, b)| if angle { [expr::round_reported(a.to_degrees()), expr::round_reported(b.to_degrees())] } else { [expr::round_reported(a), expr::round_reported(b)] })})
        })
        .collect()
}

/// A joint's values as the solver places it (after motion links, so a screw's travel follows its
/// turn), named as `joint.list` gives them.
pub(crate) fn effective_values(s: &Session, id: u64) -> Option<Vec<Value>> {
    let j = s.doc.assembly.joints.iter().find(|j| j.id == id)?;
    let vals = joints::joint_values(&s.doc.assembly);
    Some(named_values(j, vals.get(&id).map(Vec::as_slice).unwrap_or(&j.values)))
}

pub(crate) fn occurrence_id(s: &Session, v: Option<&Value>, cmd: &str) -> Result<u64> {
    let key = match v {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(x)) => x.clone(),
        _ => return Err(bad(cmd, "an occurrence must be an id or a name")),
    };
    if key == "0" || key.eq_ignore_ascii_case("root") {
        return Ok(0);
    }
    if let Some(o) = s.doc.occurrences.iter().find(|o| o.id.to_string() == key || o.name == key) {
        return Ok(o.id);
    }
    // A component name: its first occurrence.
    s.doc.find_component(&key).and_then(|c| s.doc.occurrence_of(c)).map(|o| o.id).ok_or_else(|| bad(cmd, format!("no occurrence `{key}`")))
}

/// The occurrence whose bodies are nearest a world point (0 = root bodies).
pub(crate) fn occurrence_at(s: &Session, p: Vec3) -> Option<u64> {
    let st = s.model.state();
    let mut best: Option<(f64, u64)> = None;
    for b in &st.bodies {
        let comp = s.doc.model_body_component(b);
        let occs: Vec<u64> = if comp == 0 { vec![0] } else { s.doc.occurrences.iter().filter(|o| o.component == comp).map(|o| o.id).collect() };
        for occ in occs {
            let t = if occ == 0 { sketcraft_doc::IDENTITY } else { joints::occurrence_world(&s.doc, occ) };
            let inv = sketcraft_doc::mat_inverse(&t)?;
            let q = sketcraft_doc::apply_point(&inv, p);
            let bb = b.mesh().bounds();
            let c = Vec3::new(q.x.clamp(bb.min.x, bb.max.x), q.y.clamp(bb.min.y, bb.max.y), q.z.clamp(bb.min.z, bb.max.z));
            let d = c.dist(q);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, occ));
            }
        }
    }
    best.map(|(_, o)| o)
}

/// A joint origin from parameters: a named origin, or a snap given in world coordinates.
pub(crate) fn origin_param(s: &Session, v: Option<&Value>, cmd: &str) -> Result<JointOrigin> {
    let v = v.ok_or_else(|| bad(cmd, "joint origins `a` and `b` are required"))?;
    if let Value::String(name) = v {
        return s
            .doc
            .assembly
            .origins
            .iter()
            .find(|o| &o.name == name)
            .map(|o| o.origin.clone())
            .ok_or_else(|| bad(cmd, format!("no joint origin `{name}`")));
    }
    // A face given by its name, as `model.faces` lists it.
    if let Some((what, name)) = ["face", "circle"].iter().find_map(|k| v.get(*k).and_then(Value::as_str).map(|n| (*k, n))) {
        return origin_by_name(s, v, what, name, cmd);
    }
    let pick = ["face", "circle", "edge", "point"].iter().find_map(|k| v.get(*k).and_then(vec3).map(|p| (*k, p)));
    let (what, world) = match (pick, v.get("frame")) {
        (Some(x), _) => x,
        (None, Some(f)) => ("frame", f.get("origin").and_then(vec3).ok_or_else(|| bad(cmd, "`frame` needs origin and z"))?),
        _ => return Err(bad(cmd, "a joint origin needs face, circle, edge, point or frame [x, y, z], or a face's name")),
    };
    let occurrence = match v.get("occurrence") {
        Some(o) => occurrence_id(s, Some(o), cmd)?,
        None => occurrence_at(s, world).ok_or_else(|| bad(cmd, "no body there"))?,
    };
    // World → the component's own coordinates.
    let t = if occurrence == 0 { sketcraft_doc::IDENTITY } else { joints::occurrence_world(&s.doc, occurrence) };
    let inv = sketcraft_doc::mat_inverse(&t).ok_or_else(|| bad(cmd, "occurrence transform"))?;
    let local = |p: Vec3| sketcraft_doc::apply_point(&inv, p);
    let local_dir = |d: Vec3| sketcraft_doc::apply_vector(&inv, d);
    let component = component_of(s, occurrence);
    let st = s.model.state();
    let snap = match what {
        "face" | "circle" => {
            let pick = local(world);
            // The face's name, which it is found by from then on (a circle's is its wall's).
            let face = joints::face_name_at(&s.doc, &st, component, pick, what == "circle");
            let x = x_param(s, v, component, &inv, cmd)?;
            if what == "face" { Snap::FaceCenter { pick, face, x } } else { Snap::CircleCenter { pick, face, end: None, x } }
        }
        "edge" => {
            let (on, corner) = (v.get("on").and_then(vec3).map(local), v.get("corner").and_then(vec3).map(local));
            let snap = joints::edge_snap(&s.doc, &st, component, local(world), on, corner);
            match given_z(v, &local_dir, cmd)? {
                None => snap.map_err(|e| bad(cmd, e.to_string()))?,
                // An edge or a corner picked as it is (z given): it follows the edge by name with
                // that z, or, where it can't be (no planar face beside it, no names), stays a
                // point there as before.
                Some(z) => {
                    let at = corner.unwrap_or(local(world));
                    let point = Snap::Point { point: at, z: Some(z), x: None };
                    let followed = snap.ok().map(|snap| joints::edge_axis(&s.doc, &st, component, snap, z));
                    let o = followed.map(|snap| JointOrigin { occurrence, snap }).filter(|o| same_frame(s, o, at, z));
                    return Ok(o.unwrap_or(JointOrigin { occurrence, snap: point }));
                }
            }
        }
        "point" => {
            let z = v.get("z").and_then(vec3).map(local_dir);
            let x = match v.get("x") {
                None | Some(Value::Null) => None,
                Some(x) => Some(vec3(x).map(local_dir).ok_or_else(|| bad(cmd, "a point origin's `x` is a direction [x, y, z]"))?),
            };
            // An x along z (or no direction at all) would leave the frame's turn to chance.
            if let Some(x) = x
                && (!x.is_finite() || x.cross(z.unwrap_or(Vec3::Z)).len() <= 1e-9 * x.len().max(1.0))
            {
                return Err(bad(cmd, "a point origin's `x` must be a direction off its z axis"));
            }
            Snap::Point { point: local(world), z, x }
        }
        _ => {
            let f = v.get("frame").cloned().unwrap_or_default();
            let z = f.get("z").and_then(vec3).ok_or_else(|| bad(cmd, "`frame` needs a z axis"))?;
            Snap::Frame { origin: local(world), z: local_dir(z), x: f.get("x").and_then(vec3).map(local_dir) }
        }
    };
    let o = JointOrigin { occurrence, snap };
    // A round edge's centre with the z it was picked with: its end, by name, where that is the
    // frame it shows; else a point there as before.
    if what == "circle"
        && let Some(z) = given_z(v, &local_dir, cmd)?
    {
        let at = local(world);
        // The turn the point gave, kept.
        let point = Snap::Point { point: at, z: Some(z), x: None };
        let px =
            joints::resolve_origin(&s.doc, &st, &JointOrigin { occurrence, snap: point.clone() }).ok().map(|m| Vec3::new(m[0][0], m[0][1], m[0][2]));
        let turn = |snap: Snap| match (snap, px) {
            (Snap::CircleCenter { pick, face, end, x: None }, Some(x)) => Snap::CircleCenter { pick, face, end, x: Some(XRef::Dir(x)) },
            (snap, _) => snap,
        };
        let by_wall = settle(s, JointOrigin { occurrence, snap: turn(o.snap.clone()) }, v, cmd).ok();
        let by_centre = || {
            let snap = joints::round_end_at(&s.doc, &st, component, at)?;
            settle(s, JointOrigin { occurrence, snap: turn(snap) }, v, cmd).ok()
        };
        let found = by_wall.filter(|o| same_frame(s, o, at, z)).or_else(|| by_centre().filter(|o| same_frame(s, o, at, z)));
        return Ok(found.unwrap_or(JointOrigin { occurrence, snap: point }));
    }
    match settle(s, o, v, cmd) {
        Ok(o) => Ok(o),
        // A circle given by its centre, where no face lies (#393): that end of the cylinder, by
        // its face's name and its end's (a face without a name: the point and axis there).
        Err(e) if what == "circle" => {
            let snap = match joints::round_end_at(&s.doc, &st, component, local(world)) {
                Some(Snap::CircleCenter { pick, face, end, .. }) => Snap::CircleCenter { pick, face, end, x: x_param(s, v, component, &inv, cmd)? },
                _ => circle_centre(s, occurrence, local(world)).ok_or(e)?,
            };
            settle(s, JointOrigin { occurrence, snap }, v, cmd)
        }
        Err(e) => Err(e),
    }
}

/// An origin's `z` (world), in its component's coordinates.
fn given_z(v: &Value, local_dir: &dyn Fn(Vec3) -> Vec3, cmd: &str) -> Result<Option<Vec3>> {
    match v.get("z") {
        None | Some(Value::Null) => Ok(None),
        Some(z) => {
            vec3(z).filter(|z| z.is_finite() && z.len() > 1e-9).map(|z| Some(local_dir(z))).ok_or_else(|| bad(cmd, "`z` is a direction [x, y, z]"))
        }
    }
}

/// Whether origin `o` resolves, now, to the frame at `at` with z along `z` (component coordinates)
/// that its pick showed.
fn same_frame(s: &Session, o: &JointOrigin, at: Vec3, z: Vec3) -> bool {
    let Ok(m) = joints::resolve_origin(&s.doc, &s.model.state(), o) else { return false };
    let (o, oz) = (Vec3::new(m[3][0], m[3][1], m[3][2]), Vec3::new(m[2][0], m[2][1], m[2][2]));
    o.dist(at) <= 1e-6 * at.len().max(100.0) && z.normalized().is_some_and(|z| oz.dot(z) > 1.0 - 1e-9)
}

/// The longest face name a joint origin takes. A face that cuts split and merged keeps the names
/// of the faces around it (`<name>#[<name>,…]`): several hundred characters on a part a text or
/// a pattern of holes was cut through.
const MAX_FACE_NAME: usize = 16_384;

/// The component an occurrence places (0 for the root, or an unknown occurrence).
pub(crate) fn component_of(s: &Session, occurrence: u64) -> u64 {
    if occurrence == 0 { 0 } else { s.doc.occurrences.iter().find(|o| o.id == occurrence).map_or(0, |o| o.component) }
}

/// A joint origin on a face given by its name (`face: "<name>"`: its centre; `circle: "<name>"`:
/// a round end of it), on `body?`, of `occurrence?` (default: the first occurrence of the
/// face's component).
fn origin_by_name(s: &Session, v: &Value, what: &str, name: &str, cmd: &str) -> Result<JointOrigin> {
    if name.is_empty() || name.len() > MAX_FACE_NAME {
        return Err(bad(cmd, format!("a face name is 1…{MAX_FACE_NAME} characters")));
    }
    let st = s.model.state();
    let (component, pick) = joints::named_face_spot(&s.doc, &st, str_(v, "body"), name).map_err(|why| {
        if s.doc.assembly.origins.iter().any(|o| o.name == name) {
            bad(cmd, format!("`{name}` is a joint origin: pass it as the whole origin, not as a `{what}` name"))
        } else {
            bad(cmd, why)
        }
    })?;
    let occurrence = match v.get("occurrence") {
        Some(o) => occurrence_id(s, Some(o), cmd)?,
        None if component == 0 => 0,
        None => {
            s.doc.occurrence_of(component).map(|o| o.id).ok_or_else(|| bad(cmd, format!("face `{name}` is on a component with no occurrence")))?
        }
    };
    if component_of(s, occurrence) != component {
        return Err(bad(cmd, format!("face `{name}` is not on that occurrence's component")));
    }
    let t = if occurrence == 0 { sketcraft_doc::IDENTITY } else { joints::occurrence_world(&s.doc, occurrence) };
    let inv = sketcraft_doc::mat_inverse(&t).ok_or_else(|| bad(cmd, "occurrence transform"))?;
    let x = x_param(s, v, component, &inv, cmd)?;
    let face = Some(name.to_string());
    let snap = if what == "face" { Snap::FaceCenter { pick, face, x } } else { Snap::CircleCenter { pick, face, end: None, x } };
    settle(s, JointOrigin { occurrence, snap }, v, cmd)
}

/// A new joint origin checked: `end?` (a circle's end, by the name of the face across its rim
/// there) set, and the end it takes named so it keeps it; refused when it doesn't resolve.
fn settle(s: &Session, mut o: JointOrigin, v: &Value, cmd: &str) -> Result<JointOrigin> {
    let st = s.model.state();
    let component = component_of(s, o.occurrence);
    if let Some(want) = v.get("end") {
        let want = want
            .as_str()
            .filter(|n| !n.is_empty() && n.len() <= MAX_FACE_NAME)
            .ok_or_else(|| bad(cmd, "`end` is the name of a face a round end lies on"))?;
        let Snap::CircleCenter { end, .. } = &mut o.snap else { return Err(bad(cmd, "`end` is for a round origin (`circle`)")) };
        *end = Some(want.to_string());
        let names = joints::end_faces(&s.doc, &st, component, &o.snap);
        if !names.iter().any(|n| n == want) {
            return Err(bad(cmd, format!("the round face doesn't end on face `{want}` (its ends lie on: {})", names.join(", "))));
        }
    }
    let m = joints::resolve_origin(&s.doc, &st, &o).map_err(|e| bad(cmd, e.to_string()))?;
    joints::pin_end(&s.doc, &st, component, &mut o.snap);
    // A face or circle given no `x` keeps the x it has now, so its frame doesn't turn as the face
    // tilts.
    o.snap = o.snap.with_frame_x(Vec3::new(m[0][0], m[0][1], m[0][2]));
    Ok(o)
}

/// A joint origin's x reference (its turn about z): `x: [x, y, z]` (a direction, world), `x:
/// "<face name>"`, or `x: {face: [x, y, z]}` (the face of the origin's component there, world).
fn x_param(s: &Session, v: &Value, component: u64, inv: &sketcraft_doc::Mat, cmd: &str) -> Result<Option<XRef>> {
    let st = s.model.state();
    let named = |name: &str| -> Result<Option<XRef>> {
        joints::named_face_spot(&s.doc, &st, None, name).map_err(|why| bad(cmd, format!("`x`: {why}")))?;
        Ok(Some(XRef::Face { face: name.to_string(), pick: None }))
    };
    let wrong = || bad(cmd, "`x` is a direction [x, y, z], a face's name, or {face: [x, y, z]}");
    match v.get("x") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name)) if !name.is_empty() && name.len() <= MAX_FACE_NAME => named(name),
        Some(o @ Value::Object(_)) => match o.get("face") {
            Some(Value::String(name)) if !name.is_empty() && name.len() <= MAX_FACE_NAME => named(name),
            Some(p) => {
                let pick = vec3(p).map(|p| sketcraft_doc::apply_point(inv, p)).ok_or_else(wrong)?;
                let face = joints::face_name_at(&s.doc, &st, component, pick, false).ok_or_else(|| bad(cmd, "`x`: no named face there"))?;
                Ok(Some(XRef::Face { face, pick: Some(pick) }))
            }
            None => Err(wrong()),
        },
        Some(d) => {
            let d = vec3(d).filter(|d| d.is_finite() && d.len() > 1e-9).ok_or_else(wrong)?;
            Ok(Some(XRef::Dir(sketcraft_doc::apply_vector(inv, d))))
        }
    }
}

/// The round end of a cylindrical face of the occurrence's bodies centred at `pick`: a point on
/// its axis there, pointing out of that end (as picking its circular edge gives).
fn circle_centre(s: &Session, occurrence: u64, pick: Vec3) -> Option<Snap> {
    let component = if occurrence == 0 { 0 } else { s.doc.occurrences.iter().find(|o| o.id == occurrence)?.component };
    let st = s.model.state();
    let c = st
        .bodies
        .iter()
        .filter(|b| s.doc.model_body_component(b) == component)
        .find_map(|b| sketcraft_doc::kernel::cylinder_end_at(&b.body, pick))?;
    let along = (pick - c.axis_point).dot(c.axis);
    let (at, dir) = if (along - c.start).abs() <= (c.end - along).abs() { (c.start, -1.0) } else { (c.end, 1.0) };
    Some(Snap::Point { point: c.axis_point + c.axis * at, z: Some(c.axis * dir), x: None })
}

pub(crate) fn joint_id(s: &Session, v: Option<&Value>, cmd: &str) -> Result<u64> {
    let key = match v {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(x)) => x.clone(),
        _ => return Err(bad(cmd, "`joint` must be an id or a name")),
    };
    let joint = if let Some(Value::Number(number)) = v {
        number.as_u64().and_then(|id| s.doc.assembly.joints.iter().find(|joint| joint.id == id))
    } else {
        s.doc.assembly.joints.iter().find(|joint| joint.id.to_string() == key || joint.name == key)
    };
    joint.map(|joint| joint.id).ok_or_else(|| bad(cmd, format!("no joint `{key}`")))
}

/// A joint value: numbers are degrees for angles and mm for distances; text is an expression.
pub(crate) fn value(s: &Session, v: &Value, angle: bool, cmd: &str) -> Result<f64> {
    let kind = if angle { Kind::Angle } else { Kind::Length };
    // A number out of range says so, rather than that it is not a number (#9625).
    let e = match v {
        Value::Number(n) => n.as_f64().filter(|x| x.is_finite() && x.abs() < 1e9).map(|x| x.to_string()),
        Value::String(t) => Some(t.clone()),
        _ => None,
    }
    .ok_or_else(|| bad(cmd, format!("a joint value {}", crate::params::expr_problem(Some(v)))))?;
    s.doc.eval(&e, kind).map_err(|e| bad(cmd, e.to_string()))
}

/// Shared mutation for individual joints and timeline folders; the caller resolves once.
pub(super) fn suppress_joints(doc: &mut sketcraft_doc::Document, ids: &[u64], suppressed: bool) {
    let ids: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    for joint in &mut doc.assembly.joints {
        if ids.contains(&joint.id) {
            joint.suppressed = suppressed;
        }
    }
}

pub(super) fn delete_joints(doc: &mut sketcraft_doc::Document, ids: &[u64]) {
    let ids: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    doc.assembly.joints.retain(|joint| !ids.contains(&joint.id));
    doc.assembly.links.retain(|link| !ids.contains(&link.a) && !ids.contains(&link.b));
    for group in &mut doc.timeline_groups {
        group.joints.retain(|id| !ids.contains(id));
    }
}

/// Index pairs `(i, j)`, `i < j`, whose boxes overlap or touch (sort and sweep along X), at most
/// `limit` of them, plus how many more there were.
pub(crate) fn overlapping_pairs(boxes: &[Aabb3], limit: usize) -> (Vec<(usize, usize)>, usize) {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| match (boxes.get(a), boxes.get(b)) {
        (Some(a), Some(b)) => a.min.x.total_cmp(&b.min.x),
        _ => std::cmp::Ordering::Equal,
    });
    let (mut pairs, mut skipped) = (Vec::new(), 0);
    for (k, &i) in order.iter().enumerate() {
        let Some(a) = boxes.get(i) else { continue };
        for &j in order.iter().skip(k + 1) {
            let Some(b) = boxes.get(j) else { continue };
            // Sorted by min.x: once one starts past `a`, all later ones do.
            if b.min.x > a.max.x {
                break;
            }
            if b.max.x < a.min.x || a.max.y < b.min.y || b.max.y < a.min.y || a.max.z < b.min.z || b.max.z < a.min.z {
                continue;
            }
            if pairs.len() < limit {
                pairs.push((i.min(j), i.max(j)));
            } else {
                skipped += 1;
            }
        }
    }
    pairs.sort_unstable();
    (pairs, skipped)
}
