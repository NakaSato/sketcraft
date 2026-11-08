//! Split Face (`solid.split_face`): the picked faces cut along a plane, a planar face's plane, or
//! another face (its surface extended). It has a live preview and Edit Feature.

use serde_json::{Map, Value, json};
use sketcraft_engine::Sel;
use sketcraft_engine::doc::{FaceAt, FeatureKind, PlaneRef};

use crate::SketApp;
use crate::dialogs::Kind;
use crate::selection::{FACES, PLANES, SelInput, SelectionExt as _, items, point_value as pt};

fn inputs() -> Vec<SelInput> {
    vec![SelInput::new("Faces to split", FACES, true), SelInput::new("Splitting tool", PLANES | FACES, false)]
}

/// The dialog for Split Face.
pub fn start(id: &str) -> Option<(Kind, Vec<SelInput>)> {
    (id == "solid.split_face").then(|| (Kind::SplitFace, inputs()))
}

/// The splitting tool: a plane by name, a planar face as its plane, or any other face as
/// {body, point} (its surface extended).
fn tool(app: &SketApp, sel: Option<&Sel>) -> Result<(&'static str, Value), String> {
    match sel {
        Some(Sel::Plane { name }) => Ok(("plane", json!(name))),
        Some(Sel::Face { body, index, point }) => match crate::dialogs::planar_face(&app.session, body, *index) {
            Some((_, n)) => Ok(("plane", json!({"origin": pt(*point), "normal": pt(n)}))),
            None => Ok(("tool", json!({"body": body, "point": pt(*point)}))),
        },
        _ => Err("pick a plane or a face as the tool".into()),
    }
}

pub fn commands(app: &SketApp, inputs: &[SelInput]) -> Result<Vec<(String, Value)>, String> {
    if items(inputs, 0).is_empty() {
        return Err("select the faces to split first".into());
    }
    if items(inputs, 1).is_empty() {
        return Err("select the splitting tool first".into());
    }
    let faces: Vec<Value> = items(inputs, 0).face_points().into_iter().map(pt).collect();
    let body = items(inputs, 0).iter().find_map(|x| if let Sel::Face { body, .. } = x { Some(body.clone()) } else { None });
    let (key, t) = tool(app, items(inputs, 1).first())?;
    let mut p = json!({"faces": faces, "body": body});
    p[key] = t;
    Ok(vec![("solid.split_face".to_string(), p)])
}

/// A tool as a selection again: a plane, else the face at the stored point.
fn tool_sel(app: &SketApp, plane: &PlaneRef, tool: Option<&FaceAt>) -> Option<Sel> {
    match tool {
        Some(t) => crate::dialogs::face_sel(&app.session, t.point.0),
        None => crate::dialogs::plane_sel(&app.session, plane),
    }
}

/// The dialog for editing a Split Face feature.
pub fn for_feature(app: &SketApp, kind: &FeatureKind) -> Option<(Kind, Vec<SelInput>, Map<String, Value>)> {
    let FeatureKind::SplitFace { faces, plane, tool, .. } = kind else { return None };
    let s = &app.session;
    let mut ins = inputs();
    let fill: [Vec<Sel>; 2] =
        [faces.iter().filter_map(|p| crate::dialogs::face_sel(s, p.0)).collect(), tool_sel(app, plane, tool.as_ref()).into_iter().collect()];
    for (inp, items) in ins.iter_mut().zip(fill) {
        inp.items = items;
    }
    Some((Kind::SplitFace, ins, Map::new()))
}
