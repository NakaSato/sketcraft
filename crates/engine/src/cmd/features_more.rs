//! Rib, Web, Emboss, Replace Face, Align and Remove.

use serde_json::Value;
use sketcore_record::Vec3;
use sketcraft_doc::expr::Kind;
use sketcraft_doc::{AlignFrame, FeatureKind};
use sketcraft_doc::{BodyName, SketchId};
use sketcraft_doc::{Dir, FacePick, Position};
use sketcraft_geom::prelude::*;

use super::CommandSpec;
use super::features::{
    add_feature, axis_dir_param, check_expr, feature_sketch, length_operands, point_param, profiles, typed_length_unit,
};
use crate::params::{bad, bool_, expr, req_expr, str_, string_list, vec3};
use crate::{Result, Session};

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("solid.coil", "Coil", coil).at("SOLID", "CREATE").icon("coil").params(
        "diameter; two of revolutions (or turns), height, pitch; section_size; section?: circular|square; section_position?: inside|center|outside; \
         base?: [x,y,z] (numbers in mm or length expressions; default: the origin); axis?: X|Y|Z|[x,y,z] (default Z); start_angle?; clockwise?: bool; \
         sketch? (a sketch; its plane is the section plane, so its placement sets the radius and start phase), profiles? (default: all; one closed profile without holes); \
         operation?, targets?, name?, body_name?",
    ),
    CommandSpec::new("solid.emboss", "Emboss", emboss)
        .at("SOLID", "CREATE")
        .icon("emboss")
        .params("sketch, profiles?; depth; mode?: emboss|deboss; targets?; face?: world point [x,y,z] selecting a planar or complete exterior cylindrical wall (inferred when exactly one supported tangent cylinder matches the selected profiles)"),
    CommandSpec::new("solid.align", "Align", align)
        .at("SOLID", "MODIFY")
        .icon("align")
        .params("bodies: [names]; from: [x,y,z] | \"origin\" | {snap: vertex|edge_mid|face_center|circle_center, at: [x,y,z]} | from_face: [x,y,z] | from_plane: XY|XZ|YZ|construction or offset plane name or id|{origin, normal}; to: (the same) | to_face: [x,y,z] | to_plane: (a plane as from_plane; the point or face goes onto it along its normal, and the plane may move later: the bodies follow); from_frame / to_frame: a frame found again whenever the design changes, so the bodies follow it, in place of that side's point, face or plane: a joint origin as joint.create takes one ({face|circle|edge|point: [x,y,z]}, a face's name, a named joint origin) or {sketch, point|curve} (a sketch point, or a circle's, arc's or ellipse's centre, z along the sketch's normal), on the bodies' own component; faces, planes and frames also turn to meet; flip?: bool; axes?: [x, y, z] (point-to-point only: which world axes the offset moves along; default all three, at least one true); angle?: expr (a turn about the target's direction after the move)"),
    CommandSpec::new("solid.remove", "Remove", remove).at("SOLID", "MODIFY").icon("delete").params("bodies: [names] (removed from here on in the timeline)"),
];

fn coil(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "solid.coil";
    let given = |k: &str| p.get(k).is_some_and(|v| !v.is_null());
    let profile_mode = given("sketch");
    if profile_mode {
        if let Some(k) = ["diameter", "section_size", "section", "section_position", "start_angle"].into_iter().find(|k| given(k)) {
            return Err(bad(cmd, format!("`{k}` does not apply with `sketch`: the section comes from the sketch")));
        }
    } else if given("profiles") {
        return Err(bad(cmd, "`profiles` needs `sketch`"));
    }
    let sketch = if profile_mode { Some(sketcraft_doc::SketchId(super::features::sketch_id(s, p.get("sketch"), cmd, "sketch")?)) } else { None };
    let profiles = if profile_mode { super::features::profiles(p, cmd)? } else { sketcraft_doc::ProfileSel::All };
    let base = point_param(s, p, "base", cmd)?.unwrap_or_else(|| sketcraft_doc::point_expr(Vec3::ZERO));
    let axis = axis_dir_param(p, cmd)?;
    let diameter = if profile_mode {
        None
    } else {
        let d = req_expr(cmd, p, "diameter")?;
        check_expr(s, &d, Kind::Length, cmd, "diameter")?;
        Some(d)
    };
    let turns = expr(p, "revolutions").or_else(|| expr(p, "turns"));
    let (height, pitch) = (expr(p, "height"), expr(p, "pitch"));
    // The pitch or the revolutions from the height keep the units the lengths were given in.
    let unit = typed_length_unit(s);
    let (turns, pitch) = match (turns, height, pitch) {
        (Some(n), _, Some(pt)) => (n, pt),
        (Some(n), Some(h), None) => {
            let [h] = length_operands(s, [(&h, &unit)]);
            (n.clone(), format!("({h}) / ({n})").into())
        }
        (None, Some(h), Some(pt)) => {
            let [h, by] = length_operands(s, [(&h, &unit), (&pt, &unit)]);
            (format!("({h}) / ({by})").into(), pt)
        }
        _ => return Err(bad(cmd, "give two of `revolutions`, `height` and `pitch`")),
    };
    check_expr(s, &turns, Kind::Unitless, cmd, "revolutions")?;
    check_expr(s, &pitch, Kind::Length, cmd, "pitch")?;
    let section_size = if profile_mode {
        None
    } else {
        let size = req_expr(cmd, p, "section_size")?;
        check_expr(s, &size, Kind::Length, cmd, "section_size")?;
        Some(size)
    };
    let section = match str_(p, "section").map(str::to_ascii_lowercase).as_deref() {
        None | Some("circular" | "circle") => sketcraft_doc::CoilSection::Circular,
        Some("square") => sketcraft_doc::CoilSection::Square,
        Some(o) => return Err(bad(cmd, format!("unknown section `{o}` (circular or square)"))),
    };
    let position = match str_(p, "section_position").map(str::to_ascii_lowercase).as_deref() {
        None | Some("center" | "on" | "on_center") => 0,
        Some("inside") => -1,
        Some("outside") => 1,
        Some(o) => return Err(bad(cmd, format!("unknown section position `{o}` (inside, center or outside)"))),
    };
    let start_angle = expr(p, "start_angle");
    if let Some(a) = &start_angle {
        check_expr(s, a, Kind::Angle, cmd, "start_angle")?;
    }
    let clockwise = bool_(p, "clockwise").unwrap_or(false);
    let operation = super::features::operation(p, cmd)?;
    add_feature(
        s,
        p,
        FeatureKind::Coil {
            base,
            axis: Dir(axis),
            sketch,
            profiles,
            diameter,
            pitch,
            turns,
            section_size,
            section,
            position,
            start_angle,
            clockwise,
            operation,
            targets: string_list(cmd, p, "targets")?.into_iter().map(BodyName).collect(),
        },
    )
}

fn emboss(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "solid.emboss";
    let sketch = feature_sketch(s, p, cmd)?;
    let profiles = profiles(p, cmd)?;
    let depth = req_expr(cmd, p, "depth")?;
    check_expr(s, &depth, Kind::Length, cmd, "depth")?;
    let deboss = match str_(p, "mode").map(str::to_ascii_lowercase).as_deref() {
        None | Some("emboss") => false,
        Some("deboss") => true,
        Some(m) => return Err(bad(cmd, format!("unknown mode `{m}` (emboss or deboss)"))),
    };
    let mut targets = string_list(cmd, p, "targets")?;
    let face = if let Some(value) = p.get("face") {
        let local = vec3(value).ok_or_else(|| bad(cmd, "`face` must be a finite point [x,y,z]"))?;
        let at = sketcraft_doc::apply_point(&s.active_component_frame(), local);
        let state = s.world_state()?;
        let mut candidates = Vec::new();
        // The nearest face off the point: (distance, body), for the error when none holds it.
        let mut nearest: Option<(f64, &str)> = None;
        for body in &state.bodies {
            let definition = body.definition_name();
            if !targets.is_empty() && !targets.iter().any(|n| n == definition || n == &body.name) {
                continue;
            }
            let mesh = body.mesh();
            let closest = mesh
                .triangles
                .iter()
                .zip(&mesh.tri_face)
                .filter_map(|(tri, face)| {
                    let [a, b, c] = mesh.tri(tri)?;
                    let n = (b - a).cross(c - a).normalized()?;
                    let q = at - n * (at - a).dot(n);
                    let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, v)| (*v - *u).cross(q - *u).dot(n) >= -1e-9);
                    inside.then_some(((at - a).dot(n).abs(), *face as usize))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            match closest {
                Some((distance, index)) if distance < (body.body.size() * 1e-3).max(1e-3) => candidates.push((body, index)),
                Some((distance, _)) if nearest.is_none_or(|(d, _)| distance < d) => nearest = Some((distance, body.name.as_str())),
                _ => {}
            }
        }
        // #392: say what is wrong with the point, not only that it picks no single face.
        let point = format!("[{}, {}, {}]", local.x, local.y, local.z);
        match (candidates.as_slice(), nearest) {
            ([], Some((d, body))) => {
                return Err(bad(
                    cmd,
                    format!("`face` {point} lies on no face: the nearest, on `{body}`, is {d:.3} mm away; give a point on the face"),
                ));
            }
            ([], None) => return Err(bad(cmd, format!("`face` {point} lies on no face of the target bodies"))),
            ([_], _) => {}
            (more, _) => {
                let names: Vec<&str> = more.iter().map(|(b, _)| b.name.as_str()).collect();
                return Err(bad(
                    cmd,
                    format!("`face` {point} lies on faces of {} bodies ({}): name one in `targets`", names.len(), names.join(", ")),
                ));
            }
        }
        let (selected, index) = candidates.first().copied().ok_or_else(|| bad(cmd, "no target face"))?;
        let name = selected.definition_name();
        let body = &selected.body;
        match sketcraft_kernel::face_surface(body, index) {
            Some(sketcraft_kernel::FaceSurface::Plane { .. }) => {
                targets = vec![name.to_owned()];
                None
            }
            Some(sketcraft_kernel::FaceSurface::Cylinder { .. }) => {
                if targets.len() > 1 {
                    return Err(bad(cmd, "cylinder emboss needs one selected target body"));
                }
                // Viewport picks lie on tessellated chords. Keep their axial and
                // angular position, and certify their distance before storing an
                // exact surface anchor for later replay.
                let at = sketcraft_kernel::cylinder_emboss_anchor(body, index, at).map_err(|e| bad(cmd, e.to_string()))?;
                let face = emboss_selection(s, selected, at)?;
                check_emboss_positions(s, sketch, selected, &face)?;
                targets = vec![name.to_owned()];
                Some(face)
            }
            _ => return Err(bad(cmd, "only planar faces and complete exterior cylindrical walls support emboss")),
        }
    } else {
        let inferred = infer_emboss_selection(s, sketch, &profiles, &depth, deboss, &targets)?;
        if let Some(face) = &inferred {
            targets = vec![face.body.0.clone()];
        }
        inferred
    };
    add_feature(
        s,
        p,
        FeatureKind::Emboss { sketch: SketchId(sketch), profiles, depth, deboss, targets: targets.into_iter().map(BodyName).collect(), face },
    )
}

fn emboss_selection(s: &Session, selected: &sketcraft_doc::ModelBody, at: Vec3) -> Result<sketcraft_doc::EmbossFace> {
    let cmd = "solid.emboss";
    let name = selected.definition_name();
    let original = s.model.state();
    let owner = s.doc.model_body_component(selected);
    let paths: Vec<_> = s
        .doc
        .placements()
        .into_iter()
        .filter(|(comp, path, _)| *comp == owner && s.doc.world_body_name(&original, name, owner, path) == selected.name)
        .map(|(_, path, _)| path)
        .collect();
    if paths.len() != 1 {
        return Err(bad(cmd, "selected cylinder placement is ambiguous"));
    }
    let path = paths.into_iter().next().ok_or_else(|| bad(cmd, "selected cylinder placement"))?;
    let author_path = s.active_occurrence_path.clone().unwrap_or_else(|| s.doc.first_path(s.active_component));
    if path.len() > 64 || author_path.len() > 64 {
        return Err(bad(cmd, "cylinder placement is too deep"));
    }
    let inverse = sketcraft_doc::mat_inverse(&s.active_component_frame()).ok_or_else(|| bad(cmd, "invalid active frame"))?;
    Ok(sketcraft_doc::EmbossFace { body: name.into(), at: FacePick(sketcraft_doc::apply_point(&inverse, at)), path, author_path })
}

fn check_emboss_positions(s: &Session, sketch: u64, body: &sketcraft_doc::ModelBody, face: &sketcraft_doc::EmbossFace) -> Result<()> {
    let cmd = "solid.emboss";
    let owner = s.doc.model_body_component(body);
    let sketch_owner = s.doc.feature(sketch).map(|f| f.component).ok_or_else(|| bad(cmd, "profile sketch no longer exists"))?;
    let sketch_path = if sketch_owner == owner {
        face.path.clone()
    } else if sketch_owner == s.active_component {
        face.author_path.clone()
    } else {
        s.doc.first_path(sketch_owner)
    };
    if face.path.iter().chain(&face.author_path).chain(&sketch_path).any(|id| s.pending_moves.contains_key(id)) {
        return Err(bad(cmd, "capture pending source, author and profile positions with Capture Position before cylinder emboss"));
    }
    Ok(())
}

fn infer_emboss_selection(
    s: &Session,
    sketch: u64,
    profiles: &sketcraft_doc::ProfileSel,
    depth: &str,
    deboss: bool,
    targets: &[String],
) -> Result<Option<sketcraft_doc::EmbossFace>> {
    let cmd = "solid.emboss";
    let original = s.model.state();
    let ss = original.sketch(sketch).ok_or_else(|| bad(cmd, "the sketch is not evaluated"))?;
    let regions = sketcraft_doc::select_profiles(ss, profiles).map_err(|e| bad(cmd, e.to_string()))?;
    let sketch_owner = s.doc.feature(sketch).map(|f| f.component).ok_or_else(|| bad(cmd, "profile sketch no longer exists"))?;
    let d = s.doc.eval(&s.doc.with_design_unit(depth, Kind::Length), Kind::Length).map_err(|e| bad(cmd, e.to_string()))?;
    let world = s.world_state()?;
    let placed = s.placed_document();
    let mut chosen = None;
    let mut faces = 0usize;
    for selected in &world.bodies {
        let name = selected.definition_name();
        if !targets.is_empty() && !targets.iter().any(|n| n == name || n == &selected.name) {
            continue;
        }
        let raw = original.body(name).ok_or_else(|| bad(cmd, "target definition no longer exists"))?;
        faces = faces.saturating_add(raw.body.face_count());
        if faces > 50_000 {
            return Err(bad(cmd, "cylinder target search exceeds face work limit"));
        }
        if !sketcraft_kernel::has_cylinder_emboss_wall(&raw.body).map_err(|e| bad(cmd, e.to_string()))? {
            continue;
        }
        let mut face = emboss_selection(s, selected, Vec3::ZERO)?;
        let body_frame = face.body_frame(&placed, raw).map_err(|e| bad(cmd, e.to_string()))?;
        let inverse = sketcraft_doc::mat_inverse(&body_frame).ok_or_else(|| bad(cmd, "invalid cylinder body frame"))?;
        let displayed = world.sketch(sketch).ok_or_else(|| bad(cmd, "the profile sketch is not displayed"))?;
        let plane = sketcore_record::Plane::new(
            sketcraft_doc::apply_point(&inverse, displayed.plane.origin),
            sketcraft_doc::apply_vector(&inverse, displayed.plane.x),
            sketcraft_doc::apply_vector(&inverse, displayed.plane.y),
        )
        .ok_or_else(|| bad(cmd, "invalid cylinder sketch frame"))?;
        if let Some((_, at)) =
            sketcraft_kernel::infer_cylinder_emboss(&raw.body, &plane, &regions, if deboss { -d } else { d }).map_err(|e| bad(cmd, e.to_string()))?
        {
            face.sketch_frame(&placed, s.active_component, sketch_owner, raw).map_err(|e| bad(cmd, e.to_string()))?;
            check_emboss_positions(s, sketch, selected, &face)?;
            let author_inverse = sketcraft_doc::mat_inverse(&s.active_component_frame()).ok_or_else(|| bad(cmd, "invalid author frame"))?;
            face.at = FacePick(sketcraft_doc::apply_point(&sketcraft_doc::mat_mul(&author_inverse, &body_frame), at));
            if chosen.is_some() {
                return Err(bad(cmd, "multiple tangent cylinders match the selected profiles; select a target face or body"));
            }
            chosen = Some(face);
        }
    }
    Ok(chosen)
}

/// `axes`: three booleans, one per world axis, for a point-to-point Align.
/// `None` when the call does not restrict the move.
fn axes_param(p: &Value, cmd: &str) -> Result<Option<[bool; 3]>> {
    let Some(v) = p.get("axes").filter(|v| !v.is_null()) else { return Ok(None) };
    let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| bad(cmd, "`axes` must be [x, y, z] booleans"))?;
    let mut mask = [false; 3];
    for (slot, v) in mask.iter_mut().zip(a) {
        *slot = v.as_bool().ok_or_else(|| bad(cmd, "`axes` must be [x, y, z] booleans"))?;
    }
    if !mask.iter().any(|on| *on) {
        return Err(bad(cmd, "`axes` must enable at least one of X, Y and Z"));
    }
    Ok((mask != [true; 3]).then_some(mask))
}

fn align(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "solid.align";
    let bodies = string_list(cmd, p, "bodies")?;
    if bodies.is_empty() {
        return Err(bad(cmd, "`bodies` must list the bodies to move"));
    }
    {
        let st = s.model.state();
        if let Some(b) = bodies.iter().find(|b| st.body(b).is_none()) {
            return Err(bad(cmd, format!("no body `{b}`")));
        }
    }
    // Picks and snaps are where the view shows them (the world); the feature moves the bodies
    // in their component's frame, so both ends go into the first moved body's frame. A body
    // picked in another placed component (aligning two imported parts) is found where it
    // is shown, not where its own frame has it.
    let frame = s
        .placed_document()
        .body_frames_with_source(&s.model.state())
        .into_iter()
        .find(|(world, own, ..)| *world == bodies[0] || *own == bodies[0])
        .map(|(.., m)| m);
    let inverse = frame.and_then(|m| sketcraft_doc::mat_inverse(&m)).unwrap_or(sketcraft_doc::IDENTITY);
    // A side is a plane (`from_plane`/`to_plane`: XY|XZ|YZ, a construction or offset plane, or
    // {origin, normal}), kept as a plane so the bodies follow it when it moves; or a point and, for
    // a face, its normal.
    let plane = |s: &Session, key: &str| -> Result<Option<sketcraft_doc::PlaneRef>> {
        p.get(key).map(|_| super::features::plane_param(s, p.get(key), cmd)).transpose()
    };
    let (from_plane, to_plane) = (plane(s, "from_plane")?, plane(s, "to_plane")?);
    // Or a frame (#6257), found again when the feature is evaluated.
    let (from_frame, to_frame) = (align_frame(s, p, "from", &bodies)?, align_frame(s, p, "to", &bodies)?);
    // A target on the bodies that move would move away with them: it must be found the same
    // without them.
    if let Some(fr @ AlignFrame::Origin { .. }) = &to_frame {
        let st = s.model.state();
        let mut rest = (*st).clone();
        rest.bodies.retain(|b| !bodies.contains(&b.name));
        let same = |(a, n): (Vec3, Vec3), (b, m): (Vec3, Vec3)| a.dist(b) <= 1e-9 * (1.0 + a.len()) && n.dot(m) > 1.0 - 1e-12;
        if !matches!((fr.resolve(&s.doc, &st), fr.resolve(&s.doc, &rest)), (Ok(a), Ok(b)) if same(a, b)) {
            return Err(bad(cmd, "`to_frame` is on a body that moves: pick the target on another body or on a sketch"));
        }
    }
    // Worked out from the plane or the frame when the feature is evaluated.
    let held = |plane: &Option<sketcraft_doc::PlaneRef>, frame: &Option<AlignFrame>| plane.is_some() || frame.is_some();
    let world_pick = |s: &Session, pk: &str, fk: &str, held: bool| -> Result<(Vec3, Option<Vec3>)> {
        if held {
            if p.get(pk).is_some() || p.get(fk).is_some() {
                return Err(bad(cmd, format!("give `{pk}_plane` or `{pk}`/`{fk}`, not both")));
            }
            return Ok((Vec3::ZERO, None));
        }
        if let Some(fp) = p.get(fk).and_then(vec3) {
            let n = super::face::face_normal_world(s, fp).ok_or_else(|| bad(cmd, format!("`{fk}` must be on a planar face")))?;
            return Ok((fp, Some(n)));
        }
        // The origin point.
        if p.get(pk).and_then(Value::as_str).is_some_and(|o| o.eq_ignore_ascii_case("origin")) {
            return Ok((Vec3::ZERO, None));
        }
        // A snap point: {snap: vertex|edge_mid|face_center|circle_center, at: [x,y,z]}.
        if let Some(o) = p.get(pk).filter(|v| v.is_object()) {
            let at = o.get("at").and_then(vec3).ok_or_else(|| bad(cmd, format!("`{pk}.at` must be [x, y, z]")))?;
            let kind = o.get("snap").and_then(Value::as_str).unwrap_or("vertex");
            return snap_point(&*s.world_state()?, kind, at).map(|q| (q, None)).ok_or_else(|| bad(cmd, format!("no {kind} to snap to near `{pk}`")));
        }
        p.get(pk)
            .and_then(vec3)
            .map(|v| (v, None))
            .ok_or_else(|| bad(cmd, format!("give `{pk}`, `{fk}` or `{pk}_plane` ([x, y, z], \"origin\", or a plane)")))
    };
    let pick = |s: &Session, pk: &str, fk: &str, held: bool| -> Result<(Vec3, Option<Vec3>)> {
        let (at, n) = world_pick(s, pk, fk, held)?;
        if held {
            return Ok((at, n));
        }
        Ok((sketcraft_doc::apply_point(&inverse, at), n.and_then(|n| sketcraft_doc::apply_vector(&inverse, n).normalized())))
    };
    let (from, from_normal) = pick(s, "from", "from_face", held(&from_plane, &from_frame))?;
    let (to, to_normal) = pick(s, "to", "to_face", held(&to_plane, &to_frame))?;
    // A turn about the target's direction after the move.
    let angle = expr(p, "angle");
    if let Some(a) = &angle {
        check_expr(s, a, Kind::Angle, cmd, "angle")?;
        if to_normal.is_none() && !held(&to_plane, &to_frame) {
            return Err(bad(cmd, "`angle` turns about the target's direction: give `to_frame`, `to_face` or `to_plane`"));
        }
    }
    // Faces turn only to meet another face, a plane or a frame; a plane's normal and a frame's z
    // are read when evaluated. A target face's normal is also the axis of an `angle`.
    let (from_normal, to_normal) = match (from_normal, to_normal) {
        (Some(a), Some(b)) => (Some(a), Some(b)),
        (a, b) if held(&from_plane, &from_frame) || held(&to_plane, &to_frame) || angle.is_some() => (a, b),
        _ => (None, None),
    };
    let axes = axes_param(p, cmd)?;
    if axes.is_some() && (held(&from_plane, &from_frame) || held(&to_plane, &to_frame) || from_normal.is_some() || to_normal.is_some()) {
        return Err(bad(
            cmd,
            "`axes` applies to point-to-point alignment: leave out `from_face`, `to_face`, `from_plane`, `to_plane`, `from_frame` and `to_frame`",
        ));
    }
    // The directions are stored in the same frame as `from`/`to` (the first moved body's frame).
    let axes = axes.map(|mask| {
        mask.into_iter()
            .zip([Vec3::X, Vec3::Y, Vec3::Z])
            .filter(|&(on, _)| on)
            .map(|(_, world)| Dir(sketcraft_doc::apply_vector(&inverse, world).normalized().unwrap_or(world)))
            .collect::<Vec<_>>()
    });
    let flip = bool_(p, "flip").unwrap_or(false);
    add_feature(
        s,
        p,
        FeatureKind::Align {
            bodies: bodies.into_iter().map(BodyName).collect(),
            from: Position(from),
            to: Position(to),
            from_normal: from_normal.map(Dir),
            to_normal: to_normal.map(Dir),
            flip,
            from_plane,
            to_plane,
            axes,
            from_frame,
            to_frame,
            angle,
        },
    )
}

/// The longest sketch point or curve id an Align frame takes.
const MAX_SKETCH_ID: usize = 256;

/// An Align side's frame (#6257), `<key>_frame`: a joint origin as `joint.create` takes one, or a
/// sketch point or the centre of a sketch circle, arc or ellipse (`{sketch, point|curve}`), on the
/// moved bodies' component (all of them on one, placed once). It takes the place of `<key>`,
/// `<key>_face` and `<key>_plane`, and is checked to resolve now.
fn align_frame(s: &Session, p: &Value, key: &str, bodies: &[String]) -> Result<Option<AlignFrame>> {
    let cmd = "solid.align";
    let fk = format!("{key}_frame");
    let Some(v) = p.get(&fk).filter(|v| !v.is_null()) else { return Ok(None) };
    if let Some(other) = [key.to_string(), format!("{key}_face"), format!("{key}_plane")].into_iter().find(|k| p.get(k).is_some()) {
        return Err(bad(cmd, format!("give `{fk}` or `{other}`, not both")));
    }
    let st = s.model.state();
    let (frame, on) = match v.get("sketch") {
        Some(sk) => {
            let sketch = super::features::sketch_id(s, Some(sk), cmd, &fk)?;
            let id = |k: &str| str_(v, k).filter(|n| !n.is_empty() && n.chars().count() <= MAX_SKETCH_ID).map(str::to_string);
            let frame = match (id("point"), id("curve")) {
                (Some(point), None) => AlignFrame::SketchPoint { sketch: SketchId(sketch), point },
                (None, Some(curve)) => AlignFrame::SketchCurve { sketch: SketchId(sketch), curve },
                _ => return Err(bad(cmd, format!("`{fk}`: a sketch frame takes `point` or `curve` (an id of 1 to {MAX_SKETCH_ID} characters)"))),
            };
            (frame, s.doc.feature(sketch).map_or(0, |f| f.component))
        }
        None => {
            let origin = super::joints::origin_param(s, Some(v), cmd)?;
            let on = super::joints::component_of(s, origin.occurrence);
            (AlignFrame::Origin { origin: Box::new(origin) }, on)
        }
    };
    // The move is worked out in that component's coordinates: every body must be in it, and a
    // component placed twice would not say which placement the picks were made in.
    if bodies.iter().any(|b| st.body(b).map_or(0, |b| s.doc.model_body_component(b)) != on) {
        return Err(bad(cmd, format!("not supported yet: `{fk}` on another component than the bodies")));
    }
    if on != 0 && s.doc.occurrences.iter().filter(|o| o.component == on).count() != 1 {
        return Err(bad(cmd, format!("not supported yet: `{fk}` on a component placed more than once")));
    }
    frame.resolve(&s.doc, &st).map_err(|e| bad(cmd, format!("`{fk}`: {e}")))?;
    Ok(Some(frame))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "solid.remove";
    let bodies = string_list(cmd, p, "bodies")?;
    if bodies.is_empty() {
        return Err(bad(cmd, "`bodies` must list bodies"));
    }
    let st = s.model.state();
    if let Some(b) = bodies.iter().find(|b| st.body(b).is_none()) {
        return Err(bad(cmd, format!("no body `{b}`")));
    }
    add_feature(s, p, FeatureKind::Remove { bodies: bodies.into_iter().map(BodyName).collect() })
}

#[cfg(test)]
#[path = "features_more_tests.rs"]
mod tests;

/// A snap point near `at` on the model: the nearest vertex, edge midpoint, planar face's centre
/// or circular edge's centre (the end of a cylindrical face) of the placed model `st`.
pub(crate) fn snap_point(st: &sketcraft_doc::ModelState, kind: &str, at: Vec3) -> Option<Vec3> {
    let near = |pts: Vec<Vec3>| pts.into_iter().min_by(|a, b| a.dist(at).total_cmp(&b.dist(at)));
    match kind {
        "vertex" => near(
            st.bodies
                .iter()
                .flat_map(|b| b.mesh().edges.iter().flat_map(|e| [e.first().copied(), e.last().copied()]).flatten().collect::<Vec<_>>())
                .collect(),
        ),
        "edge_mid" => {
            let d = |e: &sketcraft_doc::kernel::EdgeInfo| e.points.windows(2).map(|w| at.dist_to_segment(w[0], w[1])).fold(f64::MAX, f64::min);
            st.bodies
                .iter()
                .filter_map(|b| b.body.edges((b.body.size() * 2e-3).max(1e-3)).ok())
                .flatten()
                .min_by(|a, b| d(a).total_cmp(&d(b)))
                .map(|e| e.mid)
        }
        "face_center" => st.bodies.iter().find_map(|b| {
            let i = sketcraft_doc::appearance::face_index_at(b, at)?;
            b.body.faces((b.body.size() * 2e-3).max(1e-3)).ok()?.into_iter().find(|f| f.index == i).map(|f| f.centroid)
        }),
        "circle_center" => st.bodies.iter().find_map(|b| {
            let c = sketcraft_doc::kernel::cylinder_face_at(&b.body, at)?;
            let along = (at - c.axis_point).dot(c.axis);
            let end = if (along - c.start).abs() <= (c.end - along).abs() { c.start } else { c.end };
            Some(c.axis_point + c.axis * end)
        }),
        _ => None,
    }
}
