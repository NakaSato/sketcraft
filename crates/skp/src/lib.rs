//! SketchUp (.skp) reader.
//!
//! [`import_skp`] reads a file into a [`Model`] of vertices, edges, planar faces, groups, colors
//! and hidden state, and the model triangulates its faces. Positions are metres, Z up. The
//! reader (`src/reader`) is pure Rust and runs on every target, the browser included. It is a
//! port of the vendored OpenSKP C++ reader, which is still compiled on desktop as the oracle
//! for the differential tests in `src/parity.rs`.

#![deny(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod reader;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod parity;

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
            self.hidden.contains(&e) || self.entity_tags.get(&e).and_then(|tag| self.tags.get(tag)).is_some_and(|tag| !tag.visible)
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
            .as_chunks::<3>()
            .0
            .iter()
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
        let planar = ring.iter().all(|id| self.vertices.get(id).is_some_and(|v| (v.position - origin).dot(normal).abs() <= PLANE_TOL));
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

/// Reads a SketchUp file. `name` becomes the name of the root group.
pub fn import_skp(bytes: &[u8], name: &str) -> Result<Model, String> {
    if bytes.len() > MAX_SKP_BYTES {
        return Err("SketchUp import is limited to 1 GiB per file".into());
    }
    reader::import(bytes, name).map_err(|e| format!("Could not read SketchUp file: {e}"))
}

#[cfg(test)]
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

    const WINDOW: &[u8] = include_bytes!("../tests/fixtures/placed-window.skp");
    const TRIANGLE: &[u8] = include_bytes!("../tests/fixtures/modern-triangle.skp");

    /// Every prefix of both fixtures reads without panicking, and a cut through the geometry
    /// is an error.
    #[test]
    fn truncated_files_are_errors() {
        for fixture in [WINDOW, TRIANGLE] {
            for len in 0..fixture.len() {
                let _ = import_skp(&fixture[..len], "Cut");
            }
            assert!(import_skp(&fixture[..fixture.len() / 2], "Cut").is_err());
        }
        assert!(import_skp(&[], "Empty").is_err());
    }

    /// Random bytes, with and without a valid header and version string, are errors.
    #[test]
    fn random_bytes_are_errors() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut noise = |n: usize| -> Vec<u8> {
            (0..n)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect()
        };
        for n in [1, 4, 64, 4096, 100_000] {
            assert!(import_skp(&noise(n), "Noise").is_err());
            let mut modern = TRIANGLE[..0x1b].to_vec();
            modern.extend(noise(n));
            assert!(import_skp(&modern, "Noise").is_err());
            let mut legacy = WINDOW[..0x40].to_vec();
            legacy.extend(noise(n));
            assert!(import_skp(&legacy, "Noise").is_err());
        }
    }

    /// Every single-byte change to both fixtures reads without panicking; a damaged header or
    /// ZIP directory is an error.
    #[test]
    fn flipped_bytes_never_panic() {
        for fixture in [WINDOW, TRIANGLE] {
            for at in 0..fixture.len() {
                for mask in [0x01, 0x80, 0xff] {
                    let mut bytes = fixture.to_vec();
                    bytes[at] ^= mask;
                    let _ = import_skp(&bytes, "Flip");
                }
            }
            let mut bytes = fixture.to_vec();
            bytes[1] ^= 0xff;
            assert!(import_skp(&bytes, "Flip").is_err());
        }
        // The central directory signature of the modern fixture.
        let mut bytes = TRIANGLE.to_vec();
        let cdh = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[cdh] ^= 0xff;
        assert_eq!(import_skp(&bytes, "Flip").unwrap_err(), "Could not read SketchUp file: Invalid ZIP container | stage=zip_extract");
    }

    #[test]
    fn non_empty_arrays_are_rejected() {
        assert!(serde_json::from_str::<Model>(r#"{"faces":[1]}"#).is_err());
    }
}
