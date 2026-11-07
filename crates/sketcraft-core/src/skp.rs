//! SketchUp (.skp) import and interchange.
use crate::{Model};

pub const MAX_SKP_BYTES: usize = 1024 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
pub fn import_skp(bytes: &[u8], name: &str) -> Result<Model, String> {
    if bytes.len() > MAX_SKP_BYTES {
        return Err("SketchUp import is limited to 512 MiB per file".into());
    }
    let json = parse_skp(bytes).map_err(|e| format!("Could not read SketchUp file: {e}"))?;
    decode_model(&json, name)
}

fn decode_model(json: &str, name: &str) -> Result<Model, String> {
    let mut model: Model = serde_json::from_str(json).map_err(|e| e.to_string())?;
    model.validate().map_err(str::to_owned)?;
    if let Some((_, group)) = model.objects.last_key_value() {
        let id = group.id;
        if let Some(obj) = model.objects.get_mut(&id) {
            obj.name = name.to_owned();
        }
    }
    Ok(model)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_skp(bytes: &[u8]) -> Result<String, String> {
    use std::ffi::{CStr, c_char};
    unsafe extern "C" {
        fn ketchup_skp_parse(bytes: *const u8, len: usize, success: *mut i32) -> *mut c_char;
        fn ketchup_skp_free(value: *mut c_char);
    }
    let mut success = 0;
    // SAFETY: bytes.as_ptr() is valid for bytes.len() reads, and ketchup_skp_parse
    // returns a newly allocated null-terminated C string on heap or NULL on OOM.
    let value = unsafe { ketchup_skp_parse(bytes.as_ptr(), bytes.len(), &mut success) };
    if value.is_null() {
        return Err("Not enough memory to read this SketchUp file".into());
    }
    // SAFETY: value is guaranteed non-null, points to valid null-terminated C string.
    let text = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: value was allocated by ketchup_skp_parse and is freed by matching ketchup_skp_free.
    unsafe {
        ketchup_skp_free(value);
    }
    if success == 1 { Ok(text) } else { Err(text) }
}

/// OBJ uses metre coordinates with Z-up convention. Triangulating
/// faces keeps holes intact even in readers without polygon-hole support.
pub fn export_obj(model: &Model) -> Result<String, String> {
    use std::{collections::BTreeMap, fmt::Write};
    let visible_vertices: std::collections::BTreeSet<_> = model
        .edges
        .values()
        .filter(|e| !model.is_hidden(e.id))
        .flat_map(|e| [e.a, e.b])
        .chain(
            model
                .faces
                .values()
                .filter(|f| !model.is_hidden(f.id))
                .flat_map(|f| f.vertices.iter().chain(f.holes.iter().flatten()).copied()),
        )
        .collect();
    // Pre-allocate buffer based on geometry count to prevent unbounded reallocations on large models.
    let estimated_bytes = visible_vertices.len() * 32 + model.faces.len() * 64 + 64;
    let mut out = String::with_capacity(estimated_bytes);
    out.push_str("# SketCraft OBJ export\n# Units: metres; Z up\n");
    let mut indices = BTreeMap::new();
    let mut positions = BTreeMap::new();
    for vertex in model
        .vertices
        .values()
        .filter(|v| visible_vertices.contains(&v.id))
    {
        let p = vertex.position;
        let _ = writeln!(out, "v {} {} {}", p.x, p.y, p.z);
        indices.insert(vertex.id, indices.len() + 1);
        positions.insert(p.to_array().map(f64::to_bits), indices[&vertex.id]);
    }
    for face in model.faces.values().filter(|f| !model.is_hidden(f.id)) {
        for triangle in model.face_triangles(face) {
            let [a, b, c] = triangle.map(|p| positions[&p.to_array().map(f64::to_bits)]);
            let _ = writeln!(out, "f {a} {b} {c}");
        }
    }
    for edge in model.edges.values().filter(|e| !model.is_hidden(e.id)) {
        let _ = writeln!(out, "l {} {}", indices[&edge.a], indices[&edge.b]);
    }
    if !out
        .lines()
        .any(|line| line.starts_with("f ") || line.starts_with("l "))
    {
        return Err("There is no visible geometry to export".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skp_import_fixtures() {
        let model = import_skp(
            include_bytes!("../tests/fixtures/placed-window.skp"),
            "Window",
        )
        .expect("should import placed-window.skp");
        assert_eq!(model.faces.len(), 2);
        assert_eq!(model.objects.len(), 4);

        let modern = import_skp(
            include_bytes!("../tests/fixtures/modern-triangle.skp"),
            "Triangle",
        )
        .expect("should import modern-triangle.skp");
        assert_eq!(modern.faces.len(), 1);
        assert_eq!(modern.edges.len(), 3);
    }
}

