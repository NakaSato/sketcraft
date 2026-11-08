//! Contact Sets, Motion Study and Exploded views.

use sketcraft_geom::prelude::*;
use std::collections::BTreeSet;

use serde_json::{Value, json};
use sketcore_record::Vec3;
use sketcraft_doc::Mat;
use sketcraft_doc::joints::{self};
use sketcraft_doc::motion::{ExplodedView, MotionStudy};
use sketcraft_geom::Aabb3;
use sketcraft_kernel::Body;

use super::CommandSpec;
use super::joints::{effective_values, joint_id, occurrence_id, place_holding, value};
use crate::params::{bad, str_, vec3};
use crate::{Result, Session};

#[path = "motion_trace.rs"]
pub(crate) mod associative;
#[path = "contact_sweep.rs"]
pub(crate) mod contact_sweep;

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("explode.create", "Exploded View", explode_create).at("SOLID", "ASSEMBLE").icon("explode").enabled(has_parts_to_explode).params(
        "name?; scale? (automatic: each occurrence pushed away from the assembly's centre by scale × its offset, default 1); \
         moves?: [{occurrence, translate: [x,y,z]}] (instead of automatic)",
    ),
    CommandSpec::new("explode.show", "Show Exploded View", explode_show)
        .icon("explode")
        .noundo()
        .params("view: id|name (or none: back to the assembled placement) — shown only; the design keeps its placements"),
    CommandSpec::new("explode.list", "List Exploded Views", explode_list).icon("list").noundo(),
    CommandSpec::new("explode.delete", "Delete Exploded View", explode_delete).icon("delete").params("view"),
];

const MAX_ITEMS: usize = 10_000;

/// `prefix` and the first number from 1 that no name in `taken` has: a default name never repeats
/// one in use, after a delete either, as joints' and the app's dialogs' default names (#9738).
fn free_name<'a>(prefix: &str, taken: impl Iterator<Item = &'a str>) -> String {
    let taken: BTreeSet<&str> = taken.collect();
    let n = (1..=taken.len() + 1).find(|n| !taken.contains(format!("{prefix}{n}").as_str())).unwrap_or(1);
    format!("{prefix}{n}")
}

// ---- Contact ----

/// The placed bodies of the design, each with the occurrences it sits in.
fn placed(s: &Session) -> Vec<(Vec<u64>, Body)> {
    let st = s.model.state();
    let mut out = Vec::new();
    for (comp, path, m) in s.doc.placements() {
        if comp == 0 {
            continue;
        }
        for b in st.bodies.iter().filter(|b| s.doc.model_body_component(b) == comp && !b.body.is_mesh()) {
            // Moving a body into another component preserves its world placement using an
            // offset from its authoring frame. Contact and explode centres must use that
            // same offset as display and physical properties.
            let frame = s.doc.model_body_offset(b).map(|offset| sketcraft_doc::mat_mul(&m, offset)).unwrap_or(m);
            if let Ok(t) = sketcraft_kernel::transform_matrix(&b.body, frame) {
                out.push((path.clone(), t));
            }
        }
    }
    out
}

/// Set joint values, stopping short where contact sets would collide or a closed loop could no
/// longer close: the values are moved from where they are toward `want` as far as they go. The
/// `given` values (joint, index) are driven; in a closed loop the others follow. Returns the
/// answer and the last solve.
fn drive_with_contact(s: &mut Session, want: &[(u64, Vec<f64>)], given: &BTreeSet<(u64, usize)>) -> Result<(Value, joints::Solution)> {
    let baseline = s.doc.clone();
    let start: Vec<(u64, Vec<f64>)> =
        want.iter().map(|(id, _)| (*id, s.doc.assembly.joints.iter().find(|j| j.id == *id).map(|j| j.values.clone()).unwrap_or_default())).collect();
    // Closed loops follow the driven values from where they start.
    let held: joints::Held = given
        .iter()
        .filter(|&&(id, i)| want.iter().any(|(w, q)| *w == id && i < q.len()))
        .map(|&(id, i)| {
            // A value the file leaves out is 0, as the solver reads it.
            let from = start.iter().find(|(s, _)| *s == id).and_then(|(_, q)| q.get(i).copied());
            ((id, i), from.unwrap_or(0.0))
        })
        .collect();
    let set = |s: &mut Session, t: f64| {
        // Trial order must not affect free anchors or linked joint placements.
        s.doc = baseline.clone();
        let doc = s.doc_mut();
        for ((id, a), (_, b)) in start.iter().zip(want) {
            if let Some(j) = doc.assembly.joints.iter_mut().find(|j| j.id == *id) {
                let mut q: Vec<f64> = b.iter().enumerate().map(|(i, bv)| a.get(i).copied().unwrap_or(0.0) * (1.0 - t) + bv * t).collect();
                joints::clamp(j, &mut q);
                j.values = q;
            }
        }
        place_holding(s, &held)
    };
    // Loops already open at the start stay as they are; a loop the motion opens stops it.
    let open_at_start: BTreeSet<u64> = set(s, 0.0).conflicts.iter().map(|c| c.0).collect();
    let opened = |sol: &joints::Solution| sol.conflicts.iter().find(|c| !open_at_start.contains(&c.0)).map(|c| c.1.clone());
    let cache = if s.doc.assembly.contact_enabled { Some(contact_sweep::ContactGeometry::new(s)?) } else { None };
    let mut sol;
    let mut stopped = None;
    if let Some(cache) = cache.filter(|cache| cache.has_pairs()) {
        let mut end = s.scratch();
        set(&mut end, 1.0);
        let samples = cache.samples(s, &end)?;
        let mut intersections = 0;
        set(s, 0.0);
        // Keep the existing recovery behaviour: an initially overlapping assembly may move
        // out of its invalid placement. Sweeps constrain motions that start collision-free.
        if cache.overlaps(s, &mut intersections, false)?.is_empty() {
            let mut lo = 0.0;
            let mut bracket = None;
            for sample in 1..=samples {
                let hi = sample as f64 / samples as f64;
                set(s, hi);
                let hits = cache.overlaps(s, &mut intersections, true)?;
                if !hits.is_empty() {
                    bracket = Some((lo, hi, hits));
                    break;
                }
                lo = hi;
            }
            // Where the bodies found passing into each other first touch, closed in on from the
            // clear side (conservative advancement): from a clear placement, a move in which no
            // point of a pair goes further than the pair's gap keeps it clear. Only clear
            // placements are measured, so no intersection is asked to tell touching bodies from
            // overlapping ones within its tolerance.
            if let Some((lo, hi, hits)) = bracket {
                let touch = 1e-9 * cache.scale();
                let mut t = lo;
                set(s, t);
                for _ in 0..MAX_ADVANCES {
                    let gap = cache.gap(s, &hits)?;
                    if gap <= touch {
                        break;
                    }
                    let from = cache.frames(s)?;
                    // The furthest step whose move stays within the gap: guessed from the move to
                    // `hi` (nearly proportional over a sample), then halved until it does.
                    set(s, hi);
                    let whole = cache.pair_travel(&from, s, &hits)?;
                    let mut step = if whole > 0.0 { (hi - t) * (0.5 * gap / whole).min(1.0) } else { hi - t };
                    let mut next = None;
                    for _ in 0..64 {
                        set(s, t + step);
                        if cache.pair_travel(&from, s, &hits)? < gap {
                            next = Some(t + step);
                            break;
                        }
                        step *= 0.5;
                    }
                    match next {
                        Some(n) if n > t => t = n,
                        _ => break,
                    }
                    set(s, t);
                }
                let lo = t;
                sol = set(s, lo);
                let (a, b) = hits.first().map(|h| (h.a.clone(), h.b.clone())).unwrap_or_default();
                stopped = Some(json!({"a": a, "b": b, "fraction": lo}));
            } else {
                sol = set(s, 1.0);
            }
        } else {
            sol = set(s, 1.0);
        }
    } else {
        sol = set(s, 1.0);
    }
    let mut stopped_by_loop = None;
    if let Some(mut name) = opened(&sol) {
        // The furthest the loops still close, short of where contact stopped; the joint named is
        // the one whose loop opens first.
        let (mut lo, mut hi) = (0.0, stopped.as_ref().and_then(|v| v["fraction"].as_f64()).unwrap_or(1.0));
        for _ in 0..14 {
            let mid = (lo + hi) / 2.0;
            match opened(&set(s, mid)) {
                None => lo = mid,
                Some(n) => (hi, name) = (mid, n),
            }
        }
        sol = set(s, lo);
        stopped = None;
        stopped_by_loop = Some(json!({"joint": name, "fraction": lo}));
    }
    let values: Vec<Value> = want.iter().map(|(id, _)| json!({"joint": id, "values": effective_values(s, *id)})).collect();
    let out = json!({"values": values, "stopped_by_contact": stopped, "stopped_by_loop": stopped_by_loop, "conflicts": sol.conflicts.len()});
    Ok((out, sol))
}

/// Most steps a joint drive takes closing in on a contact. Each moves the pair by up to half its
/// gap; past the last, the drive stops at the last clear placement, short of the touch.
const MAX_ADVANCES: usize = 200;

// ---- Motion studies ----

fn study_index(s: &Session, p: &Value, cmd: &str) -> Result<usize> {
    // A number is an id first, text a name first: a study may be named like another's id.
    let (key, id_first) = match p.get("study") {
        Some(Value::Number(n)) => (n.to_string(), true),
        Some(Value::String(x)) => (x.clone(), false),
        _ => return Err(bad(cmd, "`study` must be an id or a name")),
    };
    let studies = &s.doc.assembly.studies;
    let (by_id, by_name) = (studies.iter().position(|m| m.id.to_string() == key), studies.iter().position(|m| m.name == key));
    if id_first { by_id.or(by_name) } else { by_name.or(by_id) }.ok_or_else(|| bad(cmd, format!("no motion study `{key}`")))
}

/// Joint values of a study at a step: (joint, full value vector).
fn values_at(s: &Session, m: &MotionStudy, step: f64) -> Vec<(u64, Vec<f64>)> {
    let mut out: Vec<(u64, Vec<f64>)> = Vec::new();
    for k in &m.keys {
        let Some(v) = k.value_at(step) else { continue };
        let pos = match out.iter().position(|(j, _)| *j == k.joint) {
            Some(i) => i,
            None => {
                let cur = s.doc.assembly.joints.iter().find(|j| j.id == k.joint).map(|j| j.values.clone()).unwrap_or_default();
                out.push((k.joint, cur));
                out.len() - 1
            }
        };
        if let Some((_, q)) = out.get_mut(pos) {
            if q.len() <= k.index {
                q.resize(k.index + 1, 0.0);
            }
            if let Some(x) = q.get_mut(k.index) {
                *x = v;
            }
        }
    }
    out
}

/// The values a study keys: (joint, index).
fn keyed(m: &MotionStudy) -> BTreeSet<(u64, usize)> {
    m.keys.iter().map(|k| (k.joint, k.index)).collect()
}

/// Most samples of a motion trace: each is a solve, and a kept trace keeps every one as a fit
/// point of its spline.
const MAX_TRACE: usize = sketcraft_sketch::MAX_FIT_POINTS;

/// The joint values a trace drives to at sample `k` of `n`.
type Plan = Box<dyn Fn(&Session, usize, usize) -> Vec<(u64, Vec<f64>)>>;

/// The point `p` names (`occurrence` and `point`): the occurrence path placing it and the point
/// in that placement's frame. The occurrence is an occurrence path through nested sub-assemblies
/// (one instance of each), or an occurrence at its first placement (none: the root). The point
/// is a world point where the parts are placed (moves only shown are left out).
fn traced_point(s: &Session, p: &Value, cmd: &str) -> Result<(Vec<u64>, Vec3)> {
    let path: Vec<u64> = match p.get("occurrence") {
        Some(Value::Array(ids)) if ids.len() <= 64 => {
            ids.iter().map(|v| v.as_u64().ok_or_else(|| bad(cmd, "an occurrence path lists occurrence ids"))).collect::<Result<_>>()?
        }
        Some(Value::Array(_)) => return Err(bad(cmd, "an occurrence path has at most 64 ids")),
        v => {
            let occ = occurrence_id(s, v, cmd)?;
            match s.doc.occurrences.iter().find(|o| o.id == occ) {
                Some(o) => s.doc.first_path(o.parent).into_iter().chain([o.id]).collect(),
                None => Vec::new(),
            }
        }
    };
    let at = p.get("point").and_then(vec3).filter(|v| v.len() < 1e7).ok_or_else(|| bad(cmd, "`point` must be [x, y, z], a world point"))?;
    let mut t = s.scratch();
    t.pending_moves.clear();
    let local = sketcraft_doc::mat_inverse(&path_world(&t, &path, cmd)?)
        .map(|m| sketcraft_doc::apply_point(&m, at))
        .ok_or_else(|| bad(cmd, "the occurrence's placement is not invertible"))?;
    Ok((path, local))
}

/// The world placement of an occurrence path (empty: the root).
fn path_world(t: &Session, path: &[u64], cmd: &str) -> Result<Mat> {
    if path.is_empty() { Ok(sketcraft_doc::IDENTITY) } else { t.occurrence_placement(path).map(|o| o.world).map_err(|e| bad(cmd, e.to_string())) }
}

/// Where a point of an occurrence goes as a study plays or a joint is driven: its world
/// positions, sampled on a copy (the design is not changed). Each sample is driven from the one
/// before, so closed loops follow on their branch and contact sets stop them; the trace ends at
/// the sample where the motion stops short. Returns the points and the answer without them.
fn sample_trace(s: &Session, p: &Value, cmd: &str) -> Result<(Vec<Vec3>, Value)> {
    let (path, local) = traced_point(s, p, cmd)?;
    let mut t = s.scratch();
    // Traced from where the parts are placed: moves only shown (an exploded view, a move not
    // captured yet) are not where the joints are solved.
    t.pending_moves.clear();
    let world = |t: &Session| path_world(t, &path, cmd);
    let (default, given, plan) = motion_plan(s, p, cmd)?;
    let n = sample_count(p, default.min(MAX_TRACE), MAX_TRACE, cmd)?;
    let mut points = Vec::new();
    let mut stop = (Value::Null, Value::Null, Value::Null);
    for k in 0..n {
        let want = plan(&t, k, n);
        let (r, _) = drive_with_contact(&mut t, &want, &given)?;
        points.push(sketcraft_doc::apply_point(&world(&t)?, local));
        if r["stopped_by_loop"].is_object() || r["stopped_by_contact"].is_object() {
            stop = (json!(k), r["stopped_by_loop"].clone(), r["stopped_by_contact"].clone());
            break;
        }
    }
    let name = s.doc.occurrences.iter().find(|o| Some(&o.id) == path.last()).map_or_else(|| "root".to_string(), |o| o.name.clone());
    let out = json!({"occurrence": name, "samples": points.len(), "stopped_at": stop.0, "stopped_by_loop": stop.1, "stopped_by_contact": stop.2});
    Ok((points, out))
}

/// `samples`, 2…`max`, or `default` without it.
fn sample_count(p: &Value, default: usize, max: usize, cmd: &str) -> Result<usize> {
    match p.get("samples") {
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 2.0 && *n <= max as f64)
            .map(|n| n.round() as usize)
            .ok_or_else(|| bad(cmd, format!("`samples` must be 2…{max}"))),
        None => Ok(default.max(2)),
    }
}

/// The motion a trace or an animation samples: a study played from start to end, or one joint
/// value driven from `from` to `to`. Returns the default sample count (the study's steps + 1, or
/// 100), the values driven and what they are at sample `k` of `n`.
fn motion_plan(s: &Session, p: &Value, cmd: &str) -> Result<(usize, BTreeSet<(u64, usize)>, Plan)> {
    Ok(match (p.get("study"), p.get("joint")) {
        (Some(_), Some(_)) => return Err(bad(cmd, "give `study` or `joint`, not both")),
        (Some(_), None) => {
            let m = s.doc.assembly.studies.get(study_index(s, p, cmd)?).cloned().ok_or_else(|| bad(cmd, "study"))?;
            let n = (m.steps as usize).saturating_add(1);
            (n, keyed(&m), Box::new(move |t, k, n| values_at(t, &m, f64::from(m.steps) * k as f64 / (n - 1) as f64)))
        }
        (None, Some(j)) => {
            let id = joint_id(s, Some(j), cmd)?;
            let kind = s.doc.assembly.joints.iter().find(|j| j.id == id).map(|j| j.kind).ok_or_else(|| bad(cmd, "joint"))?;
            let dofs = kind.dofs().len();
            if dofs == 0 {
                return Err(bad(cmd, "a rigid joint does not move"));
            }
            let index = match p.get("index") {
                None => 0,
                Some(v) => v
                    .as_u64()
                    .and_then(|i| usize::try_from(i).ok())
                    .filter(|i| *i < dofs)
                    .ok_or_else(|| bad(cmd, format!("`index` must be below {dofs}, the joint's value count")))?,
            };
            let end = |key: &str| match p.get(key) {
                Some(v) => value(s, v, kind.is_angle(index), cmd),
                None => Err(bad(cmd, format!("give `from` and `to`: the joint's values the motion runs between (no `{key}`)"))),
            };
            let (from, to) = (end("from")?, end("to")?);
            let plan: Plan = Box::new(move |t, k, n| {
                let mut q = t.doc.assembly.joints.iter().find(|j| j.id == id).map(|j| j.values.clone()).unwrap_or_default();
                q.resize(q.len().max(dofs), 0.0);
                if let Some(x) = q.get_mut(index) {
                    *x = from + (to - from) * k as f64 / (n - 1) as f64;
                }
                vec![(id, q)]
            });
            (100, BTreeSet::from([(id, index)]), plan)
        }
        (None, None) => return Err(bad(cmd, "give a `study`, or a `joint` with `from` and `to`")),
    })
}

fn finish_trace(points: &mut Vec<Vec3>) {
    // Where the motion stood still (at a limit or a stop), one fit point.
    points.dedup_by(|a, b| a.dist(*b) < 1e-6);
    // A full cycle comes back to where it started (to the loop solver's tolerance): the spline
    // closes there, smoothly, through the start itself (and a sample that would double it goes).
    if points.len() > 3
        && let (Some(&first), Some(&last)) = (points.first(), points.last())
        && first.dist(last) < 1e-4
    {
        points.pop();
        if points.last().is_some_and(|p| p.dist(first) < 1e-6) {
            points.pop();
        }
        points.push(first);
    }
}

// ---- Exploded views ----

/// Why Exploded View is greyed out: it pulls components apart, so a design of plain bodies, or
/// with a single component, has nothing to separate (#9142).
const EXPLODE_NEEDS_COMPONENTS: &str =
    "Exploded View pulls components apart, so the design needs two or more of them (New Component, Insert Design or Create Components from Bodies)";

/// Exploded View is offered once the design has two or more occurrences to move apart (#9142).
fn has_parts_to_explode(s: &Session) -> std::result::Result<(), String> {
    if s.doc.occurrences.len() >= 2 { Ok(()) } else { Err(EXPLODE_NEEDS_COMPONENTS.into()) }
}

fn view_index(s: &Session, p: &Value, cmd: &str) -> Result<usize> {
    let key = match p.get("view") {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(x)) => x.clone(),
        _ => return Err(bad(cmd, "`view` must be an id or a name")),
    };
    s.doc.assembly.exploded.iter().position(|e| e.id.to_string() == key || e.name == key).ok_or_else(|| bad(cmd, format!("no exploded view `{key}`")))
}

/// World centre of each top-level occurrence's bodies.
fn centres(s: &Session) -> Vec<(u64, Vec3)> {
    let bodies = placed(s);
    let mut out = Vec::new();
    for o in s.doc.occurrences.iter().filter(|o| o.parent == 0) {
        let mut bounds = Aabb3::EMPTY;
        for (_, b) in bodies.iter().filter(|(path, _)| path.first() == Some(&o.id)) {
            if let Ok(m) = b.mesh(&sketcraft_kernel::MeshTolerance::chord((b.size() * 1e-2).max(1e-2))) {
                bounds = bounds.union(&m.bounds());
            }
        }
        if !bounds.is_empty() {
            out.push((o.id, bounds.center()));
        }
    }
    out
}

fn explode_create(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "explode.create";
    let moves: Vec<(u64, Vec3)> = match p.get("moves") {
        Some(Value::Array(list)) => {
            if list.len() > MAX_ITEMS {
                return Err(bad(cmd, "too many moves"));
            }
            list.iter()
                .map(|m| {
                    let o = occurrence_id(s, m.get("occurrence"), cmd)?;
                    let t = m
                        .get("translate")
                        .and_then(vec3)
                        .filter(|v| v.len() < 1e7)
                        .ok_or_else(|| bad(cmd, "each move needs `translate`: [x, y, z]"))?;
                    Ok((o, t))
                })
                .collect::<Result<_>>()?
        }
        Some(_) => return Err(bad(cmd, "`moves` must be a list")),
        None => {
            let scale = match p.get("scale") {
                Some(v) => v.as_f64().filter(|x| x.is_finite() && x.abs() <= 100.0).ok_or_else(|| bad(cmd, "`scale` must be a number up to 100"))?,
                None => 1.0,
            };
            let c = centres(s);
            if c.len() < 2 {
                return Err(bad(cmd, "an exploded view needs two or more occurrences"));
            }
            let mid = c.iter().fold(Vec3::ZERO, |a, (_, p)| a + *p) * (1.0 / c.len() as f64);
            c.iter().map(|(o, p)| (*o, (*p - mid) * scale)).collect()
        }
    };
    if s.doc.assembly.exploded.len() >= MAX_ITEMS {
        return Err(bad(cmd, "too many exploded views"));
    }
    let id = s.doc_mut().allocate_id()?;
    let name =
        str_(p, "name").map(str::to_string).unwrap_or_else(|| free_name("Exploded View", s.doc.assembly.exploded.iter().map(|v| v.name.as_str())));
    s.doc_mut().assembly.exploded.push(ExplodedView { id, name: name.clone(), moves: moves.clone() });
    Ok(json!({"view": id, "name": name, "moves": moves.iter().map(|(o, t)| json!({"occurrence": o, "translate": t})).collect::<Vec<_>>()}))
}

fn explode_show(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "explode.show";
    if p.get("view").is_none_or(|v| v.is_null() || v.as_str().is_some_and(|x| x.eq_ignore_ascii_case("none"))) {
        s.pending_moves.clear();
        s.exploded.clear();
        s.revision += 1;
        return Ok(json!({"view": null}));
    }
    let i = view_index(s, p, cmd)?;
    let v = s.doc.assembly.exploded.get(i).cloned().ok_or_else(|| bad(cmd, "view"))?;
    let moves = exploded_at(&explode_moves(s, &v), 1.0);
    if !moves.values().all(sketcraft_doc::is_placement) {
        return Err(bad(cmd, format!("exploded view `{}` moves an occurrence out of range", v.name)));
    }
    // Shown only: `exploded` marks these as the view's, so the design's placements leave them out.
    s.exploded.clone_from(&moves);
    s.pending_moves = moves;
    s.revision += 1;
    Ok(json!({"view": v.id, "name": v.name}))
}

/// Whether the pending moves are an exploded view shown (`explode.show`), which places the
/// occurrences only while it is shown: not a move to capture or save (#9736).
pub(crate) fn shows_exploded_view(s: &Session) -> bool {
    s.doc.assembly.exploded.iter().any(|v| exploded_at(&explode_moves(s, v), 1.0) == s.pending_moves)
}

/// What exploded view `v` moves: (occurrence, its placement, the translation the view adds). Moves
/// of occurrences no longer in the design, or not finite (a hostile design file), are left out;
/// a later move of one occurrence replaces an earlier one.
fn explode_moves(s: &Session, v: &ExplodedView) -> Vec<(u64, Mat, Vec3)> {
    let placed: std::collections::BTreeMap<u64, Mat> = s.doc.occurrences.iter().map(|o| (o.id, o.transform)).collect();
    v.moves.iter().filter(|(_, t)| t.len() < 1e7).filter_map(|(o, t)| Some((*o, *placed.get(o)?, *t))).collect()
}

/// The pending moves that show `moves` `amount` of the way out: 0 assembled, 1 exploded.
fn exploded_at(moves: &[(u64, Mat, Vec3)], amount: f64) -> std::collections::BTreeMap<u64, Mat> {
    let mut out = std::collections::BTreeMap::new();
    for (o, m, t) in moves {
        let mut m = *m;
        if let Some(r) = m.get_mut(3) {
            r[0] += t.x * amount;
            r[1] += t.y * amount;
            r[2] += t.z * amount;
        }
        out.insert(*o, m);
    }
    out
}

fn explode_list(s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(json!({"views": s.doc.assembly.exploded}))
}

fn explode_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let i = view_index(s, p, "explode.delete")?;
    let v = s.doc_mut().assembly.exploded.remove(i);
    Ok(json!({"deleted": v.name}))
}

/// World position of a point of an occurrence (tests).
#[cfg(test)]
pub fn world_point(s: &Session, occurrence: u64, p: Vec3) -> Vec3 {
    sketcraft_doc::apply_point(&joints::occurrence_world(&s.doc, occurrence), p)
}

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;
