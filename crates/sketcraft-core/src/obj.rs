//! Wavefront OBJ export of a model's visible geometry.
use crate::Model;

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
