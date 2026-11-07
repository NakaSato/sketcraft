//! Architectural 3D Canopy Model Generator.
//!
//! Reconstructs the service station canopy from reference imagery:
//! - Structural cylindrical support pillars with plinths and capitals
//! - Warm orange/bronze underside soffit ceiling with recessed light fixtures
//! - Multi-layered curved fascia with filleted corners
//! - Signature vibrant green accent band
//! - Architectural metallic silver/grey upper fascia and roof deck cap
use crate::{
    Camera, DisplaySettings, Document, DVec3, Edge, Face, Model, Object, ObjectKind,
    Projection, Vertex,
};
use std::collections::{BTreeMap, BTreeSet};

const EPS: f64 = 1e-6;

/// Builder helper to create clean, topologically validated models with objects, tags, and materials.
pub struct CanopyBuilder {
    pub model: Model,
    pos_map: BTreeMap<[u64; 3], u64>,
}

impl Default for CanopyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CanopyBuilder {
    pub fn new() -> Self {
        Self {
            model: Model::default(),
            pos_map: BTreeMap::new(),
        }
    }

    /// Retrieve or insert a unique vertex at position p.
    pub fn vertex(&mut self, p: DVec3) -> u64 {
        // Quantize position to 0.1mm to guarantee exact topological matching
        let key = [
            (p.x * 10000.0).round() as i64 as u64,
            (p.y * 10000.0).round() as i64 as u64,
            (p.z * 10000.0).round() as i64 as u64,
        ];
        if let Some(&id) = self.pos_map.get(&key) {
            return id;
        }
        let id = self.model.id();
        self.model.vertices.insert(id, Vertex { id, position: p });
        self.pos_map.insert(key, id);
        id
    }

    /// Retrieve or insert an edge connecting vertices a and b.
    pub fn edge(&mut self, a: u64, b: u64) -> u64 {
        if a == b {
            panic!("Degenerate edge");
        }
        if let Some(e) = self
            .model
            .edges
            .values()
            .find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
        {
            return e.id;
        }
        let id = self.model.id();
        self.model.edges.insert(id, Edge { id, a, b });
        id
    }

    /// Add a planar polygon face defined by an ordered loop of points.
    pub fn add_polygon(&mut self, points: &[DVec3], color: [u8; 3]) -> (u64, Vec<u64>) {
        assert!(points.len() >= 3, "Polygon must have at least 3 points");
        let v_ids: Vec<u64> = points.iter().map(|&p| self.vertex(p)).collect();
        let mut e_ids = Vec::with_capacity(v_ids.len());
        for i in 0..v_ids.len() {
            let a = v_ids[i];
            let b = v_ids[(i + 1) % v_ids.len()];
            e_ids.push(self.edge(a, b));
        }

        let face_id = self.model.id();
        self.model.faces.insert(
            face_id,
            Face {
                id: face_id,
                vertices: v_ids,
                holes: vec![],
            },
        );
        self.model.face_colors.insert(face_id, color);
        (face_id, e_ids)
    }

    /// Add a planar quad wall between two 3D line segments (forming a rectangle in space).
    pub fn add_quad(&mut self, p0: DVec3, p1: DVec3, p2: DVec3, p3: DVec3, color: [u8; 3]) -> (u64, Vec<u64>) {
        self.add_polygon(&[p0, p1, p2, p3], color)
    }

    /// Group a collection of entities under an Object with a tag.
    pub fn create_group(&mut self, name: &str, entities: BTreeSet<u64>, tag_id: Option<u64>) -> u64 {
        let obj_id = self.model.id();
        self.model.objects.insert(
            obj_id,
            Object {
                id: obj_id,
                name: name.into(),
                surface_treatment: Default::default(),
                surface_treatment_customized: false,
                stock_material: None,
                stock_material_id: None,
                outliner_order: None,
                kind: ObjectKind::Group,
                transform: Some(Default::default()),
                definition_entities: BTreeMap::new(),
                entities: entities.clone(),
            },
        );
        if let Some(tag) = tag_id {
            self.model.entity_tags.insert(obj_id, tag);
            for &ent in &entities {
                if self.model.contains_entity(&ent) {
                    self.model.entity_tags.insert(ent, tag);
                }
            }
        }
        obj_id
    }
}

/// Generates a closed 2D rounded rectangle perimeter path with smooth filleted corners.
pub fn rounded_rect_perimeter(
    length_x: f64,
    width_y: f64,
    corner_radius: f64,
    segments_per_corner: usize,
    z_elevation: f64,
) -> Vec<DVec3> {
    let hx = length_x * 0.5;
    let hy = width_y * 0.5;
    let r = corner_radius.min(hx - EPS).min(hy - EPS);
    let mut points = Vec::new();

    let cx = hx - r;
    let cy = hy - r;

    // Corner centers: [top-right, top-left, bottom-left, bottom-right]
    let corners = [
        (cx, cy, 0.0, std::f64::consts::FRAC_PI_2),
        (-cx, cy, std::f64::consts::FRAC_PI_2, std::f64::consts::PI),
        (-cx, -cy, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2),
        (cx, -cy, 3.0 * std::f64::consts::FRAC_PI_2, 2.0 * std::f64::consts::PI),
    ];

    for (ccx, ccy, start_angle, end_angle) in corners {
        for s in 0..segments_per_corner {
            let t = s as f64 / segments_per_corner as f64;
            let theta = start_angle + t * (end_angle - start_angle);
            let px = ccx + r * theta.cos();
            let py = ccy + r * theta.sin();
            points.push(DVec3::new(px, py, z_elevation));
        }
    }

    points
}

/// Generate a cylindrical / faceted pillar.
fn add_pillar(
    builder: &mut CanopyBuilder,
    center_xy: (f64, f64),
    base_z: f64,
    top_z: f64,
    radius: f64,
    segments: usize,
    color: [u8; 3],
) -> BTreeSet<u64> {
    let mut entities = BTreeSet::new();
    let mut bottom_ring = Vec::with_capacity(segments);
    let mut top_ring = Vec::with_capacity(segments);

    for i in 0..segments {
        let theta = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI;
        let px = center_xy.0 + radius * theta.cos();
        let py = center_xy.1 + radius * theta.sin();
        bottom_ring.push(DVec3::new(px, py, base_z));
        top_ring.push(DVec3::new(px, py, top_z));
    }

    // Bottom cap (facing down)
    let mut b_rev = bottom_ring.clone();
    b_rev.reverse();
    let (b_face, _) = builder.add_polygon(&b_rev, color);
    entities.insert(b_face);

    // Top cap (facing up)
    let (t_face, _) = builder.add_polygon(&top_ring, color);
    entities.insert(t_face);

    // Side wall quads
    for i in 0..segments {
        let next = (i + 1) % segments;
        let p0 = bottom_ring[i];
        let p1 = bottom_ring[next];
        let p2 = top_ring[next];
        let p3 = top_ring[i];
        let (w_face, _) = builder.add_quad(p0, p1, p2, p3, color);
        entities.insert(w_face);
    }

    entities
}

/// Builds the complete architectural 3D Canopy model.
pub fn build_canopy_document() -> Document {
    let mut builder = CanopyBuilder::new();

    // 1. Create Tags
    let tag_structure = builder.model.add_tag("Structure").unwrap();
    let tag_ceiling = builder.model.add_tag("Ceiling").unwrap();
    let tag_trim = builder.model.add_tag("Fascia-Trim").unwrap();
    let tag_accent = builder.model.add_tag("Fascia-Accent").unwrap();
    let tag_upper = builder.model.add_tag("Fascia-Upper").unwrap();
    let tag_roof = builder.model.add_tag("Roof").unwrap();

    // Color definitions matching the real-world photo:
    let color_pillar = [65, 60, 58];          // Dark bronze metallic pillar shaft
    let color_pillar_base = [48, 48, 50];     // Dark plinth footing
    let color_pillar_cap = [58, 54, 52];      // Capital collar
    let color_soffit_orange = [222, 107, 31]; // Warm glossy orange ceiling (#DE6B1F)
    let color_light_lens = [245, 248, 252];   // LED light fixture diffuser
    let color_light_frame = [45, 45, 48];     // Recessed fixture bezel
    let color_recess_trim = [46, 48, 50];     // Lower perimeter cove trim
    let color_green_accent = [56, 180, 56];   // Signature vibrant green fascia ribbon (#38B438)
    let color_upper_fascia = [175, 175, 178]; // Architectural metallic silver/grey
    let color_parapet_cap = [142, 142, 146];  // Sloping cap flashing
    let color_roof_deck = [115, 118, 122];    // Weather-tight top deck

    // Canopy geometric parameters:
    let length_x = 22.0; // 22.0 meters long
    let width_y = 9.4;   // 9.4 meters wide
    let corner_r = 1.6;  // 1.6m radius smooth corner
    let corner_segs = 8; // 8 segments per 90-degree corner arc

    let clearance_z = 4.80; // Underside ceiling height
    let trim_z = 4.95;      // Lower cove trim top
    let accent_z = 5.35;    // Green accent band top
    let upper_z = 5.95;     // Upper metallic fascia top
    let roof_z = 6.05;      // Roof deck top

    // ------------------------------------------------------------------------
    // 2. Pillars & Support Columns
    // ------------------------------------------------------------------------
    let mut all_pillar_entities = BTreeSet::new();
    let pillar_positions = [(-4.5, 0.0), (4.5, 0.0)];

    for pos in pillar_positions {
        // Base plinth: Z = 0.0 to 0.20m, radius 0.48m
        let base_ent = add_pillar(&mut builder, pos, 0.0, 0.20, 0.48, 12, color_pillar_base);
        all_pillar_entities.extend(base_ent);

        // Main shaft: Z = 0.20 to 4.60m, radius 0.38m
        let shaft_ent = add_pillar(&mut builder, pos, 0.20, 4.60, 0.38, 12, color_pillar);
        all_pillar_entities.extend(shaft_ent);

        // Capital collar: Z = 4.60 to 4.80m, radius 0.45m
        let cap_ent = add_pillar(&mut builder, pos, 4.60, clearance_z, 0.45, 12, color_pillar_cap);
        all_pillar_entities.extend(cap_ent);
    }
    builder.create_group("Support Columns", all_pillar_entities, Some(tag_structure));

    // ------------------------------------------------------------------------
    // 3. Orange Soffit Ceiling
    // ------------------------------------------------------------------------
    let soffit_perimeter = rounded_rect_perimeter(length_x, width_y, corner_r, corner_segs, clearance_z);
    let mut ceiling_entities = BTreeSet::new();

    // Bottom ceiling face (facing down towards the drive-through lane)
    let mut soffit_rev = soffit_perimeter.clone();
    soffit_rev.reverse();
    let (s_face, _) = builder.add_polygon(&soffit_rev, color_soffit_orange);
    ceiling_entities.insert(s_face);

    // Add Recessed Downlights (8 fixtures in 2 bays)
    let light_xs = [-7.0, -2.5, 2.5, 7.0];
    let light_ys = [-2.2, 2.2];
    let light_size = 0.65;

    for &lx in &light_xs {
        for &ly in &light_ys {
            let half = light_size * 0.5;
            let l_p0 = DVec3::new(lx - half, ly - half, clearance_z - 0.005);
            let l_p1 = DVec3::new(lx + half, ly - half, clearance_z - 0.005);
            let l_p2 = DVec3::new(lx + half, ly + half, clearance_z - 0.005);
            let l_p3 = DVec3::new(lx - half, ly + half, clearance_z - 0.005);

            // Light lens face
            let (l_face, _) = builder.add_polygon(&[l_p3, l_p2, l_p1, l_p0], color_light_lens);
            ceiling_entities.insert(l_face);

            // Fixture frame bezel
            let frame_w = 0.06;
            let f_outer_p0 = DVec3::new(lx - half - frame_w, ly - half - frame_w, clearance_z - 0.002);
            let f_outer_p1 = DVec3::new(lx + half + frame_w, ly - half - frame_w, clearance_z - 0.002);
            let f_outer_p2 = DVec3::new(lx + half + frame_w, ly + half + frame_w, clearance_z - 0.002);
            let f_outer_p3 = DVec3::new(lx - half - frame_w, ly + half + frame_w, clearance_z - 0.002);

            let (f_face, _) = builder.add_polygon(&[f_outer_p3, f_outer_p2, f_outer_p1, f_outer_p0], color_light_frame);
            ceiling_entities.insert(f_face);
        }
    }
    builder.create_group("Orange Soffit Ceiling", ceiling_entities, Some(tag_ceiling));

    // ------------------------------------------------------------------------
    // 4. Multi-Layer Curved Fascia Bands
    // ------------------------------------------------------------------------
    let n_pts = soffit_perimeter.len();

    // Layer 1: Lower Recessed Cove Trim (Z = 4.80 to 4.95m)
    let mut trim_entities = BTreeSet::new();
    for i in 0..n_pts {
        let next = (i + 1) % n_pts;
        let p0 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, clearance_z);
        let p1 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, clearance_z);
        let p2 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, trim_z);
        let p3 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, trim_z);

        let (face, _) = builder.add_quad(p0, p1, p2, p3, color_recess_trim);
        trim_entities.insert(face);
    }
    builder.create_group("Perimeter Cove Trim", trim_entities, Some(tag_trim));

    // Layer 2: Signature Green Accent Band (Z = 4.95 to 5.35m)
    let mut green_entities = BTreeSet::new();
    for i in 0..n_pts {
        let next = (i + 1) % n_pts;
        let p0 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, trim_z);
        let p1 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, trim_z);
        let p2 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, accent_z);
        let p3 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, accent_z);

        let (face, _) = builder.add_quad(p0, p1, p2, p3, color_green_accent);
        green_entities.insert(face);
    }
    builder.create_group("Green Accent Band", green_entities, Some(tag_accent));

    // Layer 3: Upper Metallic Silver/Grey Fascia (Z = 5.35 to 5.95m)
    let mut upper_entities = BTreeSet::new();
    for i in 0..n_pts {
        let next = (i + 1) % n_pts;
        let p0 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, accent_z);
        let p1 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, accent_z);
        let p2 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, upper_z);
        let p3 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, upper_z);

        let (face, _) = builder.add_quad(p0, p1, p2, p3, color_upper_fascia);
        upper_entities.insert(face);
    }
    builder.create_group("Upper Metallic Fascia", upper_entities, Some(tag_upper));

    // ------------------------------------------------------------------------
    // 5. Parapet Cap Flashing & Roof Deck
    // ------------------------------------------------------------------------
    let roof_perimeter = rounded_rect_perimeter(length_x - 0.1, width_y - 0.1, corner_r - 0.05, corner_segs, roof_z);
    let mut roof_entities = BTreeSet::new();

    // Parapet flashing (sloping inward to the roof deck, triangulated for planar precision)
    for i in 0..n_pts {
        let next = (i + 1) % n_pts;
        let p0 = DVec3::new(soffit_perimeter[i].x, soffit_perimeter[i].y, upper_z);
        let p1 = DVec3::new(soffit_perimeter[next].x, soffit_perimeter[next].y, upper_z);
        let p2 = roof_perimeter[next];
        let p3 = roof_perimeter[i];

        let (f1, _) = builder.add_polygon(&[p0, p1, p2], color_parapet_cap);
        let (f2, _) = builder.add_polygon(&[p0, p2, p3], color_parapet_cap);
        roof_entities.insert(f1);
        roof_entities.insert(f2);
    }

    // Top roof deck face (facing up)
    let (roof_face, _) = builder.add_polygon(&roof_perimeter, color_roof_deck);
    roof_entities.insert(roof_face);

    builder.create_group("Roof Deck & Parapet", roof_entities, Some(tag_roof));

    // ------------------------------------------------------------------------
    // 6. Camera Setup (Matching the Reference Photo Perspective)
    // ------------------------------------------------------------------------
    let camera = Camera {
        target: DVec3::new(0.0, 0.0, 3.8),
        distance: 22.0,
        yaw: -0.68,
        pitch: 0.26,
        projection: Projection::Perspective,
        fov: 0.785398, // 45 degrees field of view
    };

    let doc = Document {
        project_id: Some(uuid::Uuid::new_v4()),
        version: 2,
        name: "Service Station Canopy".into(),
        model: builder.model,
        camera,
        display: DisplaySettings {
            shadows: true,
            shadow_darkness: 0.35,
            sun_angle: 65.0,
            sun_elevation: 42.0,
            sky: true,
            ground: true,
            ..Default::default()
        },
        ..Default::default()
    };

    doc.model.validate().expect("Canopy model must validate cleanly");
    doc
}

/// Exports the canopy model to native `.ketchup` JSON format and OBJ.
pub fn export_canopy_files(target_dir: &std::path::Path) -> Result<(), String> {
    let doc = build_canopy_document();
    let json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;

    std::fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
    let ketchup_path = target_dir.join("Canopy.ketchup");
    std::fs::write(&ketchup_path, json.as_bytes()).map_err(|e| e.to_string())?;

    let obj_content = crate::skp::export_obj(&doc.model)?;
    let obj_path = target_dir.join("canopy.obj");
    std::fs::write(&obj_path, obj_content.as_bytes()).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canopy_generation_and_validation() {
        let doc = build_canopy_document();
        assert!(doc.model.vertices.len() > 100);
        assert!(doc.model.faces.len() > 100);
        assert_eq!(doc.model.tags.len(), 6);
        assert_eq!(doc.model.objects.len(), 6);

        // Check that all face colors match design
        let green_faces: Vec<_> = doc
            .model
            .face_colors
            .values()
            .filter(|&&c| c == [56, 180, 56])
            .collect();
        assert_eq!(green_faces.len(), 32); // 32 segments along the perimeter

        let orange_faces: Vec<_> = doc
            .model
            .face_colors
            .values()
            .filter(|&&c| c == [222, 107, 31])
            .collect();
        assert_eq!(orange_faces.len(), 1); // Large underside soffit ceiling

        // Verify JSON roundtrip
        let json = serde_json::to_string(&doc).unwrap();
        let restored = Document::from_json(&json).unwrap();
        assert_eq!(restored.model.faces.len(), doc.model.faces.len());
        assert_eq!(restored.name, "Service Station Canopy");

        // Verify OBJ export
        let obj = crate::skp::export_obj(&doc.model).expect("OBJ export must succeed");
        assert!(obj.contains("v ") && obj.contains("f "));
    }

    #[test]
    fn test_export_canopy_assets() {
        // Resolve project root from CARGO_MANIFEST_DIR
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir.parent().and_then(|p| p.parent()).unwrap();
        let models_dir = root.join("assets/models");
        let examples_dir = root.join("assets/examples");
        export_canopy_files(&models_dir).expect("export to assets/models must succeed");
        export_canopy_files(&examples_dir).expect("export to assets/examples must succeed");
    }
}
