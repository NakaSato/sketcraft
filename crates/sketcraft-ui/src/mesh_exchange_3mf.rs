//! 3MF import: mesh objects, nested components and build items with their transforms,
//! from the package's root model part. Coordinates convert from the model's unit to metres.
use super::*;
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::io::{Cursor, Read};

/// Largest decompressed part read from a package.
const MAX_PART: u64 = 256 << 20;
/// Deepest component nesting followed before the file is rejected.
const MAX_DEPTH: usize = 16;

/// A 3MF affine transform: rows m00..m02, m10..m12, m20..m22, then the translation m30..m32.
type Affine = [f64; 12];
const IDENTITY: Affine = [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.];

fn apply(m: &Affine, p: DVec3) -> DVec3 {
    DVec3::new(
        p.x * m[0] + p.y * m[3] + p.z * m[6] + m[9],
        p.x * m[1] + p.y * m[4] + p.z * m[7] + m[10],
        p.x * m[2] + p.y * m[5] + p.z * m[8] + m[11],
    )
}
/// `inner` then `outer`, as 3MF composes a component inside its parent.
fn compose(inner: &Affine, outer: &Affine) -> Affine {
    let mut out = [0.; 12];
    for row in 0..4 {
        for col in 0..3 {
            let mut v = if row == 3 { outer[9 + col] } else { 0. };
            for k in 0..3 {
                v += inner[row * 3 + k] * outer[k * 3 + col];
            }
            out[row * 3 + col] = v;
        }
    }
    out
}

#[derive(Default)]
struct Object {
    vertices: Vec<DVec3>,
    triangles: Vec<[usize; 3]>,
    components: Vec<(u32, Affine)>,
}

fn attr(e: &BytesStart, name: &[u8]) -> Result<Option<String>, String> {
    for a in e.attributes() {
        let a = a.map_err(|_| "Invalid 3MF attribute")?;
        if a.key.local_name().as_ref() == name {
            let value = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|_| "Invalid 3MF attribute value")?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}
fn required(e: &BytesStart, name: &[u8]) -> Result<String, String> {
    attr(e, name)?.ok_or_else(|| {
        format!("3MF {} is missing {}", String::from_utf8_lossy(e.local_name().as_ref()), String::from_utf8_lossy(name))
    })
}
fn transform(e: &BytesStart) -> Result<Affine, String> {
    let Some(text) = attr(e, b"transform")? else {
        return Ok(IDENTITY);
    };
    let values: Vec<f64> = text.split_whitespace().map(number).collect::<Result<_, _>>()?;
    values.try_into().map_err(|_| "A 3MF transform needs 12 numbers".into())
}
fn index(text: &str, len: usize) -> Result<usize, String> {
    text.parse::<usize>()
        .ok()
        .filter(|&i| i < len)
        .ok_or_else(|| format!("3MF triangle refers to a missing vertex: {text}"))
}

fn read_part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let name = name.trim_start_matches('/');
    let Some(index) = (0..zip.len()).find(|&i| {
        zip.name_for_index(i).is_some_and(|n| n.trim_start_matches('/').eq_ignore_ascii_case(name))
    }) else {
        return Ok(None);
    };
    let file = zip.by_index(index).map_err(|e| format!("Cannot read 3MF part {name}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_PART + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read 3MF part {name}: {e}"))?;
    if bytes.len() as u64 > MAX_PART {
        return Err("A 3MF part is larger than 256 MiB".into());
    }
    Ok(Some(bytes))
}

/// The root model part named by the package relationships, else the conventional path.
fn root_model_path(zip: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<String, String> {
    if let Some(rels) = read_part(zip, "_rels/.rels")? {
        let mut reader = quick_xml::Reader::from_reader(rels.as_slice());
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf).map_err(|e| format!("Invalid 3MF relationships: {e}"))? {
                Event::Empty(e) | Event::Start(e) if e.local_name().as_ref() == b"Relationship" => {
                    if attr(&e, b"Type")?.is_some_and(|t| t.ends_with("/3dmodel")) {
                        return required(&e, b"Target");
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
    }
    Ok("3D/3dmodel.model".into())
}

fn unit_scale(unit: &str) -> Result<f64, String> {
    Ok(match unit {
        "micron" => 1e-6,
        "millimeter" => 1e-3,
        "centimeter" => 1e-2,
        "inch" => 0.0254,
        "foot" => 0.3048,
        "meter" => 1.,
        other => return Err(format!("Unsupported 3MF unit: {other}")),
    })
}

pub(super) fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Not a 3MF package (expected a ZIP archive)")?;
    let path = root_model_path(&mut zip)?;
    let xml = read_part(&mut zip, &path)?.ok_or_else(|| format!("The 3MF package has no model part at {path}"))?;

    let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
    let mut buf = Vec::new();
    let mut scale = 1e-3; // 3MF's default unit is the millimetre.
    let mut objects: HashMap<u32, Object> = HashMap::new();
    let mut items: Vec<(u32, Affine)> = Vec::new();
    let mut current: Option<(u32, Object)> = None;
    let mut vertex_count = 0usize;
    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| format!("Invalid 3MF model XML: {e}"))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                b"model" => scale = unit_scale(attr(e, b"unit")?.as_deref().unwrap_or("millimeter"))?,
                b"object" => {
                    let id = required(e, b"id")?.parse().map_err(|_| "Invalid 3MF object id")?;
                    current = Some((id, Object::default()));
                }
                b"vertex" => {
                    let (_, object) = current.as_mut().ok_or("3MF vertex outside an object")?;
                    vertex_count += 1;
                    if vertex_count > LIMIT {
                        return Err("Import exceeds coordinate or 200,000 vertex limit".into());
                    }
                    object.vertices.push(DVec3::new(
                        number(&required(e, b"x")?)?,
                        number(&required(e, b"y")?)?,
                        number(&required(e, b"z")?)?,
                    ));
                }
                b"triangle" => {
                    let (_, object) = current.as_mut().ok_or("3MF triangle outside an object")?;
                    let len = object.vertices.len();
                    object.triangles.push([
                        index(&required(e, b"v1")?, len)?,
                        index(&required(e, b"v2")?, len)?,
                        index(&required(e, b"v3")?, len)?,
                    ]);
                }
                b"component" => {
                    let (_, object) = current.as_mut().ok_or("3MF component outside an object")?;
                    if attr(e, b"path")?.is_some() {
                        return Err("3MF components in other model parts are not supported".into());
                    }
                    let id = required(e, b"objectid")?.parse().map_err(|_| "Invalid 3MF component id")?;
                    object.components.push((id, transform(e)?));
                }
                b"item" => {
                    let id = required(e, b"objectid")?.parse().map_err(|_| "Invalid 3MF build item id")?;
                    items.push((id, transform(e)?));
                }
                _ => {}
            },
            Event::End(e) if e.local_name().as_ref() == b"object" => {
                if let Some((id, object)) = current.take() {
                    objects.insert(id, object);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        if matches!(event, Event::Empty(ref e) if e.local_name().as_ref() == b"object") {
            if let Some((id, object)) = current.take() {
                objects.insert(id, object);
            }
        }
        buf.clear();
    }
    if items.is_empty() {
        return Err("The 3MF model has no build items".into());
    }
    let mut mesh = Mesh::default();
    for (id, item) in &items {
        emit(&objects, *id, item, scale, 0, &mut mesh)?;
    }
    Ok(mesh)
}

fn emit(objects: &HashMap<u32, Object>, id: u32, m: &Affine, scale: f64, depth: usize, mesh: &mut Mesh) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("3MF components are nested too deeply or refer to themselves".into());
    }
    let object = objects.get(&id).ok_or_else(|| format!("3MF refers to a missing object: {id}"))?;
    for t in &object.triangles {
        mesh.triangle(t.map(|i| apply(m, object.vertices[i]) * scale))?;
    }
    for (child, local) in &object.components {
        emit(objects, *child, &compose(local, m), scale, depth + 1, mesh)?;
    }
    Ok(())
}
