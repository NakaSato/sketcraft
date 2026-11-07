//! glTF 2.0 import (.glb, or .gltf with embedded buffers): triangle meshes placed by the
//! default scene's node transforms. glTF is metres with Y up; KetchUp is metres with Z up.
use super::*;
use glam::{DMat4, DVec4};
use gltf::buffer::Source;
use gltf::mesh::Mode;

/// Deepest node nesting followed before the file is rejected.
const MAX_DEPTH: usize = 64;

/// Standard (RFC 4648) base64, ignoring whitespace; `None` on any other character.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn buffers(document: &gltf::Document, blob: Option<Vec<u8>>) -> Result<Vec<Vec<u8>>, String> {
    let mut blob = blob;
    document
        .buffers()
        .map(|buffer| {
            let data = match buffer.source() {
                Source::Bin => blob.take().ok_or("The GLB file has no binary chunk")?,
                Source::Uri(uri) => {
                    let Some((_, encoded)) = uri.strip_prefix("data:").and_then(|d| d.split_once(";base64,")) else {
                        return Err("glTF files that load separate .bin files are not supported; export a .glb or embed the buffers".to_string());
                    };
                    base64(encoded).ok_or("Invalid base64 data in a glTF buffer")?
                }
            };
            if data.len() < buffer.length() {
                return Err("A glTF buffer is shorter than declared".into());
            }
            Ok(data)
        })
        .collect()
}

fn to_z_up(p: DVec3) -> DVec3 {
    DVec3::new(p.x, -p.z, p.y)
}

pub(super) fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| format!("Invalid glTF file: {e}"))?;
    let gltf::Gltf { document, blob } = gltf;
    let data = buffers(&document, blob)?;
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or("The glTF file has no scene")?;
    let mut mesh = Mesh::default();
    for node in scene.nodes() {
        emit(&node, DMat4::IDENTITY, &data, 0, &mut mesh)?;
    }
    if mesh.faces.is_empty() {
        return Err("The glTF scene has no triangle meshes".into());
    }
    Ok(mesh)
}

fn emit(node: &gltf::Node, parent: DMat4, data: &[Vec<u8>], depth: usize, mesh: &mut Mesh) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("glTF nodes are nested too deeply".into());
    }
    let local = DMat4::from_cols_array_2d(&node.transform().matrix().map(|c| c.map(f64::from)));
    let world = parent * local;
    if let Some(node_mesh) = node.mesh() {
        for primitive in node_mesh.primitives() {
            let reader = primitive.reader(|b| data.get(b.index()).map(Vec::as_slice));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let points: Vec<DVec3> = positions
                .map(|[x, y, z]| {
                    let p = world * DVec4::new(f64::from(x), f64::from(y), f64::from(z), 1.);
                    to_z_up(p.truncate())
                })
                .collect();
            if points.len() > LIMIT {
                return Err("Import exceeds coordinate or 200,000 vertex limit".into());
            }
            let indices: Vec<usize> = match reader.read_indices() {
                Some(indices) => indices.into_u32().map(|i| i as usize).collect(),
                None => (0..points.len()).collect(),
            };
            if indices.iter().any(|&i| i >= points.len()) {
                return Err("A glTF primitive refers to a missing vertex".into());
            }
            let triangles: Vec<[usize; 3]> = match primitive.mode() {
                Mode::Triangles => indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
                Mode::TriangleStrip => indices
                    .windows(3)
                    .enumerate()
                    .map(|(i, t)| if i % 2 == 0 { [t[0], t[1], t[2]] } else { [t[1], t[0], t[2]] })
                    .collect(),
                Mode::TriangleFan => indices.windows(2).skip(1).map(|t| [indices[0], t[0], t[1]]).collect(),
                _ => continue, // Points and lines carry no faces.
            };
            for t in triangles {
                mesh.triangle(t.map(|i| points[i]))?;
            }
        }
    }
    for child in node.children() {
        emit(&child, world, data, depth + 1, mesh)?;
    }
    Ok(())
}
