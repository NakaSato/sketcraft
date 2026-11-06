//! SketchUp (.skp) mesh importer.
use std::collections::{BTreeMap, HashMap};

use solidcore_record::Vec3;

use crate::threemf::{MeshObject, weld};
use crate::{IoError, Result};

/// Read SketchUp (.skp) bytes into one or more named mesh objects.
pub fn read_skp(bytes: &[u8], default_name: &str) -> Result<(Vec<MeshObject>, Vec<String>)> {
    let _span = sketcraft_profile::span("import", "read_skp");
    let model = sketcraft_core::skp::import_skp(bytes, default_name)
        .map_err(|e| IoError::Invalid(format!("could not read SketchUp file: {e}")))?;

    let warnings = Vec::new();
    if model.faces.is_empty() {
        return Err(IoError::Invalid("SketchUp file contains no faces".into()));
    }

    // Map each face to its named parent group/component, if any.
    let mut face_group: HashMap<u64, String> = HashMap::new();
    let is_root_container = |name: &str| name == "SketchUp import" || name == default_name;
    for obj in model.objects.values() {
        if is_root_container(&obj.name) && model.objects.len() > 1 {
            continue;
        }
        for &ent in &obj.entities {
            if model.faces.contains_key(&ent) {
                face_group.entry(ent).or_insert_with(|| obj.name.clone());
            }
        }
    }

    // Group faces by body name.
    let mut grouped_faces: BTreeMap<String, Vec<&sketcraft_core::Face>> = BTreeMap::new();
    for (id, face) in &model.faces {
        if model.is_hidden(*id) {
            continue;
        }
        let group_name = face_group.get(id).cloned().unwrap_or_else(|| default_name.to_string());
        grouped_faces.entry(group_name).or_default().push(face);
    }

    if grouped_faces.is_empty() {
        return Err(IoError::Invalid("SketchUp file has no visible faces".into()));
    }

    let mut objects = Vec::new();

    for (name, faces) in grouped_faces {
        let mut raw_positions: Vec<Vec3> = Vec::new();
        let mut raw_triangles: Vec<[u32; 3]> = Vec::new();
        let mut face_colors: Vec<[f32; 3]> = Vec::new();

        for face in faces {
            let tris = model.face_triangles(face);
            if tris.is_empty() {
                continue;
            }
            if let Some(&[r, g, b]) = model.face_colors.get(&face.id) {
                face_colors.push([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]);
            }
            for [a, b, c] in tris {
                // Scale from meters to millimeters
                let pa = Vec3::new(a.x * 1000.0, a.y * 1000.0, a.z * 1000.0);
                let pb = Vec3::new(b.x * 1000.0, b.y * 1000.0, b.z * 1000.0);
                let pc = Vec3::new(c.x * 1000.0, c.y * 1000.0, c.z * 1000.0);

                if !pa.is_finite() || !pb.is_finite() || !pc.is_finite() {
                    continue;
                }

                let base = u32::try_from(raw_positions.len()).unwrap_or(0);
                raw_positions.push(pa);
                raw_positions.push(pb);
                raw_positions.push(pc);
                raw_triangles.push([base, base + 1, base + 2]);
            }
        }

        if raw_triangles.is_empty() {
            continue;
        }

        // Weld vertices closer than 0.1 mm so watertight solids form cleanly.
        let (positions, triangles) = weld(&raw_positions, &raw_triangles, 0.1);

        if triangles.is_empty() {
            continue;
        }

        // Average or primary color for this object
        let color = if face_colors.is_empty() {
            None
        } else {
            let n = face_colors.len() as f32;
            let sum = face_colors.iter().fold([0.0f32, 0.0f32, 0.0f32], |acc, c| [acc[0] + c[0], acc[1] + c[1], acc[2] + c[2]]);
            Some([sum[0] / n, sum[1] / n, sum[2] / n])
        };

        objects.push(MeshObject {
            name,
            positions,
            triangles,
            color,
            ..Default::default()
        });
    }

    if objects.is_empty() {
        return Err(IoError::Invalid("SketchUp file produced no valid triangles".into()));
    }

    Ok((objects, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_skp() {
        let bytes = include_bytes!("../../sketcraft-core/tests/fixtures/placed-window.skp");
        let (objects, warnings) = read_skp(bytes, "Window").expect("read_skp placed-window");
        assert!(!objects.is_empty());
        assert!(warnings.is_empty());
        assert!(objects[0].positions.len() > 0);
        assert!(objects[0].triangles.len() > 0);
    }

    #[test]
    fn test_mesh_import_skp() {
        let bytes = include_bytes!("../../sketcraft-core/tests/fixtures/placed-window.skp");
        let feature = crate::mesh_import_feature_in(bytes, "placed-window.skp", 1.0).expect("mesh_import_feature_in skp");
        assert!(!feature.body_names.is_empty());
    }
}

