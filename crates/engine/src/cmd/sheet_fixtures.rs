//! Test fixtures for open sheets. The Surface tab and its commands are gone, but designs saved
//! with surface features still open and rebuild, so tests that check other commands on a sheet
//! build one here, as the old `surface.extrude`, `surface.revolve` and `surface.thicken` did.
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use sketcraft_doc::expr::Kind;
use sketcraft_doc::{BodyName, FeatureKind, Position, ProfileSel, SketchId};

use super::features::{add_feature, axis_param, check_expr, extent_param, feature_sketch, operation, profiles};
use crate::params::{bool_, expr, str_, string_list, vec3};
use crate::{Result, Session};

/// The sheet's sketch inputs: picked open curves, or profiles when no curve is named.
fn inputs(p: &Value, cmd: &str) -> Result<(Option<ProfileSel>, Vec<String>)> {
    let curves = string_list(cmd, p, "curves")?;
    let profiles = if curves.is_empty() || p.get("profiles").is_some() { Some(profiles(p, cmd)?) } else { None };
    Ok((profiles, curves))
}

/// A sheet extruded from a sketch's curves or profiles: `{sketch, curves?, distance, direction?, name?, body_name?}`.
pub(crate) fn extrude(s: &mut Session, p: Value) -> Value {
    let cmd = "sheet fixture: extrude";
    let sketch = feature_sketch(s, &p, cmd).unwrap();
    let (profiles, curves) = inputs(&p, cmd).unwrap();
    let extent = extent_param(s, &p, cmd).unwrap();
    add_feature(s, &p, FeatureKind::SurfaceExtrude { sketch: SketchId(sketch), profiles, curves, extent }).unwrap()
}

/// A sheet revolved from a sketch's curves or profiles: `{sketch, curves?, axis, angle?, name?, body_name?}`.
pub(crate) fn revolve(s: &mut Session, p: Value) -> Value {
    try_revolve(s, p).unwrap()
}

fn try_revolve(s: &mut Session, p: Value) -> Result<Value> {
    let cmd = "sheet fixture: revolve";
    let sketch = feature_sketch(s, &p, cmd)?;
    let (profiles, curves) = inputs(&p, cmd)?;
    let axis = axis_param(s, sketch, &p, cmd)?;
    let angle = expr(&p, "angle").unwrap_or_else(|| "360 deg".into());
    check_expr(s, &angle, Kind::Angle, cmd, "angle")?;
    add_feature(s, &p, FeatureKind::SurfaceRevolve { sketch: SketchId(sketch), profiles, curves, axis, angle, angle2: None, symmetric: false })
}

/// Sheets thickened into solids: `{bodies, thickness, symmetric?, name?, body_name?}`.
pub(crate) fn thicken(s: &mut Session, p: Value) -> Value {
    let cmd = "sheet fixture: thicken";
    let bodies = string_list(cmd, &p, "bodies").unwrap();
    let thickness = expr(&p, "thickness").unwrap();
    check_expr(s, &thickness, Kind::Length, cmd, "thickness").unwrap();
    let kind = FeatureKind::Thicken {
        bodies: bodies.into_iter().map(BodyName).collect(),
        thickness,
        symmetric: bool_(&p, "symmetric").unwrap_or(false),
        operation: operation(&p, cmd).unwrap(),
        targets: Vec::new(),
    };
    add_feature(s, &p, kind).unwrap()
}

/// A flat patch filling a sketch's profiles: `{sketch, profiles?, name?, body_name?}`.
pub(crate) fn patch(s: &mut Session, p: Value) -> Value {
    let cmd = "sheet fixture: patch";
    let sketch = feature_sketch(s, &p, cmd).unwrap();
    let profiles = profiles(&p, cmd).unwrap();
    let kind = FeatureKind::Patch {
        sketch: Some(SketchId(sketch)),
        profiles,
        body: None,
        edges: Vec::new(),
        crown: 0.0,
        continuity: Default::default(),
        points: Vec::new(),
    };
    add_feature(s, &p, kind).unwrap()
}

/// Faces taken out of a body, leaving it open: `{body, faces: [points on them]}`.
pub(crate) fn unstitch(s: &mut Session, p: Value) -> Value {
    let body = str_(&p, "body").unwrap().to_string();
    let faces = p["faces"].as_array().unwrap().iter().map(|f| Position(vec3(f).unwrap())).collect();
    add_feature(s, &p, FeatureKind::Unstitch { body: BodyName(body), faces }).unwrap()
}
