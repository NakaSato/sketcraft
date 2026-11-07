//! SketchUp (.skp) reader.
//!
//! On desktop the vendored OpenSKP C++ reader (`vendor/skp/native`, wrapped by
//! `src/skp_native.cpp`) turns a file into JSON: vertices, edges, planar faces,
//! groups, colors and hidden state. [`import_skp`] decodes that into a [`Model`]
//! that can triangulate its faces. Positions are metres, Z up.

use std::collections::{BTreeMap, BTreeSet};

use glam::{DVec2, DVec3};
use serde::{Deserialize, Deserializer};

/// Files larger than this are refused before parsing.
pub const MAX_SKP_BYTES: usize = 1024 * 1024 * 1024;

const PLANE_TOL: f64 = 1e-6;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Vertex {
    pub id: u64,
    pub position: DVec3,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Edge {
    pub id: u64,
    pub a: u64,
    pub b: u64,
}

/// A planar face: an outer ring of vertex IDs and optional hole rings.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Face {
    pub id: u64,
    pub vertices: Vec<u64>,
    #[serde(default)]
    pub holes: Vec<Vec<u64>>,
}

/// A group or component placement. `entities` are its direct children.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Object {
    pub id: u64,
    pub name: String,
    pub entities: BTreeSet<u64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Tag {
    pub id: u64,
    pub name: String,
    pub visible: bool,
}

/// The geometry read from one SketchUp file.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Model {
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub vertices: BTreeMap<u64, Vertex>,
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub edges: BTreeMap<u64, Edge>,
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub faces: BTreeMap<u64, Face>,
    /// sRGB face colors by face ID.
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub face_colors: BTreeMap<u64, [u8; 3]>,
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub objects: BTreeMap<u64, Object>,
    /// Entities hidden directly; their children are hidden too.
    #[serde(default)]
    pub hidden: BTreeSet<u64>,
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub tags: BTreeMap<u64, Tag>,
    /// Entity ID to tag ID.
    #[serde(default, deserialize_with = "map_or_empty_array")]
    pub entity_tags: BTreeMap<u64, u64>,
}

impl Model {
    fn contains_entity(&self, id: u64) -> bool {
        self.edges.contains_key(&id) || self.faces.contains_key(&id) || self.objects.contains_key(&id)
    }

    /// The group that directly contains `id`.
    pub fn parent(&self, id: u64) -> Option<u64> {
        self.objects.values().find(|o| o.entities.contains(&id)).map(|o| o.id)
    }

    /// Calls `test` on `id` and each of its ancestors; true when any matches.
    fn any_ancestor(&self, mut id: u64, test: impl Fn(u64) -> bool) -> bool {
        // A malformed file could nest a group inside itself; stop after visiting every group.
        for _ in 0..=self.objects.len() {
            if test(id) {
                return true;
            }
            match self.parent(id) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
        false
    }

    /// Whether the entity, or a group containing it, is hidden or on a hidden tag.
    pub fn is_hidden(&self, id: u64) -> bool {
        if self.hidden.is_empty() && self.entity_tags.is_empty() {
            return false;
        }
        self.any_ancestor(id, |e| {
            self.hidden.contains(&e)
                || self
                    .entity_tags
                    .get(&e)
                    .and_then(|tag| self.tags.get(tag))
                    .is_some_and(|tag| !tag.visible)
        })
    }

    /// `roots` plus everything nested inside them.
    pub fn expand_entities(&self, roots: &BTreeSet<u64>) -> BTreeSet<u64> {
        let mut result = BTreeSet::new();
        let mut pending: Vec<_> = roots.iter().copied().collect();
        while let Some(id) = pending.pop() {
            if !self.contains_entity(id) || !result.insert(id) {
                continue;
            }
            if let Some(object) = self.objects.get(&id) {
                pending.extend(&object.entities);
            }
        }
        result
    }

    /// Triangulates a face, holes included. Returns nothing for a degenerate or non-planar face.
    pub fn face_triangles(&self, face: &Face) -> Vec<[DVec3; 3]> {
        let Some(plane) = self.plane(&face.vertices) else {
            return vec![];
        };
        let mut points = Vec::new();
        let mut coordinates = Vec::new();
        let mut holes = Vec::new();
        for (i, ring) in std::iter::once(&face.vertices).chain(&face.holes).enumerate() {
            if i > 0 {
                holes.push(points.len());
            }
            for id in ring {
                let Some(vertex) = self.vertices.get(id) else {
                    return vec![];
                };
                points.push(vertex.position);
                let xy = plane.project(vertex.position);
                coordinates.extend([xy.x, xy.y]);
            }
        }
        earcutr::earcut(&coordinates, &holes, 2)
            .unwrap_or_default()
            .chunks_exact(3)
            .map(|i| [points[i[0]], points[i[1]], points[i[2]]])
            .collect()
    }

    fn plane(&self, ring: &[u64]) -> Option<Plane> {
        if ring.len() < 3 || ring.iter().collect::<BTreeSet<_>>().len() != ring.len() {
            return None;
        }
        let origin = self.vertices.get(&ring[0])?.position;
        let u = (self.vertices.get(&ring[1])?.position - origin).try_normalize()?;
        let normal = ring.iter().skip(2).filter_map(|id| self.vertices.get(id)).find_map(|v| {
            let cross = u.cross(v.position - origin);
            (cross.length() > PLANE_TOL).then(|| cross.normalize())
        })?;
        let planar = ring.iter().all(|id| {
            self.vertices
                .get(id)
                .is_some_and(|v| (v.position - origin).dot(normal).abs() <= PLANE_TOL)
        });
        planar.then(|| Plane { origin, u, v: normal.cross(u) })
    }
}

#[derive(Clone, Copy)]
struct Plane {
    origin: DVec3,
    u: DVec3,
    v: DVec3,
}

impl Plane {
    fn project(self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }
}

/// The reader writes an empty collection as `[]` and a full one as an object.
fn map_or_empty_array<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    // A visitor rather than `#[serde(untagged)]`: untagged buffers the input, and buffered
    // string keys such as "6529" no longer parse as integer IDs.
    struct MapVisitor<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> serde::de::Visitor<'de> for MapVisitor<K, V> {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map or an empty array")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
            Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
        fn visit_seq<S: serde::de::SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("expected a map; only empty arrays are supported"));
            }
            Ok(BTreeMap::new())
        }
    }
    deserializer.deserialize_any(MapVisitor(std::marker::PhantomData))
}

fn decode_model(json: &str, name: &str) -> Result<Model, String> {
    let mut model: Model = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if model.vertices.len() > 100_000_000 || model.faces.len() > 100_000_000 || model.objects.len() > 100_000_000 {
        return Err("Model is too large".into());
    }
    // The reader puts the file's root group last; name it after the file.
    if let Some(root) = model.objects.values_mut().next_back() {
        root.name = name.to_owned();
    }
    Ok(model)
}

/// Reads a SketchUp file. `name` becomes the name of the root group.
#[cfg(not(target_arch = "wasm32"))]
pub fn import_skp(bytes: &[u8], name: &str) -> Result<Model, String> {
    if bytes.len() > MAX_SKP_BYTES {
        return Err("SketchUp import is limited to 1 GiB per file".into());
    }
    let json = parse_skp(bytes).map_err(|e| format!("Could not read SketchUp file: {e}"))?;
    decode_model(&json, name)
}

/// The browser build has no SketchUp reader.
#[cfg(target_arch = "wasm32")]
pub fn import_skp(_bytes: &[u8], _name: &str) -> Result<Model, String> {
    Err("SketchUp import is only available in the desktop app".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_skp(bytes: &[u8]) -> Result<String, String> {
    use std::ffi::{CStr, c_char};
    unsafe extern "C" {
        fn ketchup_skp_parse(bytes: *const u8, len: usize, success: *mut i32) -> *mut c_char;
        fn ketchup_skp_free(value: *mut c_char);
    }
    let mut success = 0;
    // SAFETY: `bytes` is valid for `bytes.len()` reads. The reader returns a newly
    // allocated, null-terminated string, or null when out of memory.
    let value = unsafe { ketchup_skp_parse(bytes.as_ptr(), bytes.len(), &mut success) };
    if value.is_null() {
        return Err("Not enough memory to read this SketchUp file".into());
    }
    // SAFETY: `value` is non-null and null-terminated.
    let text = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
    // SAFETY: `value` came from `ketchup_skp_parse` and is freed once.
    unsafe { ketchup_skp_free(value) };
    if success == 1 { Ok(text) } else { Err(text) }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn imports_fixtures() {
        let window = import_skp(include_bytes!("../tests/fixtures/placed-window.skp"), "Window").unwrap();
        assert_eq!(window.faces.len(), 2);
        assert_eq!(window.objects.len(), 4);
        assert!(window.faces.values().all(|f| !window.face_triangles(f).is_empty()));

        let triangle = import_skp(include_bytes!("../tests/fixtures/modern-triangle.skp"), "Triangle").unwrap();
        assert_eq!(triangle.faces.len(), 1);
        assert_eq!(triangle.edges.len(), 3);
    }

    #[test]
    fn hidden_state_is_inherited_from_groups_and_tags() {
        let model: Model = serde_json::from_str(
            r#"{"vertices":[],"edges":[],"faces":[],
                "objects":{"10":{"id":10,"name":"G","entities":[1]},"11":{"id":11,"name":"H","entities":[2]}},
                "hidden":[10],
                "tags":{"5":{"id":5,"name":"Off","visible":false}},
                "entity_tags":{"11":5}}"#,
        )
        .unwrap();
        assert!(model.is_hidden(1));
        assert!(model.is_hidden(2));
        assert!(!model.is_hidden(3));
    }

    /// Real files have integer IDs as map keys; buffering the JSON once broke reading them.
    #[test]
    fn integer_keys_and_empty_arrays_decode() {
        let model: Model = serde_json::from_str(
            r#"{"vertices":{"1":{"id":1,"position":[0,0,0]},"2":{"id":2,"position":[1,0,0]},"3":{"id":3,"position":[0,1,0]}},
                "edges":{"7":{"id":7,"a":1,"b":2}},"faces":{"9":{"id":9,"vertices":[1,2,3]}},
                "face_colors":{"9":[200,10,10]},"objects":[],"hidden":[],"next_id":10}"#,
        )
        .unwrap();
        assert_eq!(model.face_triangles(&model.faces[&9]).len(), 1);
        assert_eq!(model.face_colors[&9], [200, 10, 10]);
    }

    /// Optional real files: SKP_FILES=a.skp:b.skp cargo test -p sketcraft-skp -- --ignored
    #[test]
    #[ignore]
    fn imports_local_files() {
        for path in std::env::var("SKP_FILES").unwrap_or_default().split(':').filter(|p| !p.is_empty()) {
            let model = import_skp(&std::fs::read(path).unwrap(), "File").unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(model.faces.values().any(|f| !model.face_triangles(f).is_empty()), "{path}");
        }
    }

    #[test]
    fn non_empty_arrays_are_rejected() {
        assert!(serde_json::from_str::<Model>(r#"{"faces":[1]}"#).is_err());
    }
}

