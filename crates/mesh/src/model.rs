//! The model: vertices, edges and faces with stable ids, and the lookups kept beside them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use sketcore_record::Vec3;
use sketcraft_geom::prelude::*;

use crate::index::{Index, bounds};
use crate::{MAX_ENTITIES, TOLERANCE};

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}
id!(VertexId);
id!(EdgeId);
id!(FaceId);
id!(CurveId);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: Vec3,
}

/// A straight edge between two vertices. Soft and smooth are SketchUp's: a soft edge is hidden
/// where it joins two faces, a smooth one shades them as one surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub a: VertexId,
    pub b: VertexId,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soft: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub smooth: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// The arc, circle or polygon the edge is a segment of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<CurveId>,
}

impl Edge {
    pub(crate) fn plain(a: VertexId, b: VertexId) -> Edge {
        Edge { a, b, soft: false, smooth: false, hidden: false, curve: None }
    }

    pub fn other(&self, v: VertexId) -> VertexId {
        if self.a == v { self.b } else { self.a }
    }
}

/// A flat face: its outer loop and its holes, each a loop of vertices, and its unit normal (the
/// outer loop runs counter-clockwise seen from the normal's side).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub outer: Vec<VertexId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<VertexId>>,
    pub normal: Vec3,
}

impl Face {
    /// Every loop of the face, outer first.
    pub fn loops(&self) -> impl Iterator<Item = &Vec<VertexId>> {
        std::iter::once(&self.outer).chain(&self.holes)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MeshError {
    #[error("a point must have finite coordinates under 1e9 mm")]
    BadPoint,
    #[error("the two ends of an edge are the same point")]
    ZeroLength,
    #[error("the model would hold more than {MAX_ENTITIES} entities")]
    TooBig,
    #[error("no {0} with that id")]
    Missing(&'static str),
    #[error("a face with holes would bend; move it flat, or move its holes with it")]
    Bends,
    #[error("the file's model is broken: {0}")]
    Broken(String),
}

/// The saved form: the entities and the next id. The lookups are made again on load.
#[derive(Serialize, Deserialize)]
struct Saved {
    vertices: BTreeMap<VertexId, Vertex>,
    edges: BTreeMap<EdgeId, Edge>,
    faces: BTreeMap<FaceId, Face>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    curves: BTreeMap<CurveId, crate::curves::Curve>,
    next: u64,
}

/// The model. Read it through the accessors; change it through its operations, which keep the
/// lookups right.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(try_from = "Saved", into = "Saved")]
pub struct Mesh {
    pub(crate) vertices: BTreeMap<VertexId, Vertex>,
    pub(crate) edges: BTreeMap<EdgeId, Edge>,
    pub(crate) faces: BTreeMap<FaceId, Face>,
    pub(crate) curves: BTreeMap<CurveId, crate::curves::Curve>,
    next: u64,
    /// The edges at each vertex.
    pub(crate) vertex_edges: BTreeMap<VertexId, BTreeSet<EdgeId>>,
    /// The faces each edge bounds.
    pub(crate) edge_faces: BTreeMap<EdgeId, BTreeSet<FaceId>>,
    /// The vertices in each cell of a grid as fine as the tolerance, to find one near a point.
    grid: HashMap<(i64, i64, i64), Vec<VertexId>>,
    /// The edges and faces by where they are.
    pub(crate) edge_index: Index<EdgeId>,
    pub(crate) face_index: Index<FaceId>,
}

impl From<Mesh> for Saved {
    fn from(m: Mesh) -> Saved {
        Saved { vertices: m.vertices, edges: m.edges, faces: m.faces, curves: m.curves, next: m.next }
    }
}

impl TryFrom<Saved> for Mesh {
    type Error = MeshError;
    fn try_from(s: Saved) -> Result<Mesh, MeshError> {
        if s.vertices.len() + s.edges.len() + s.faces.len() > MAX_ENTITIES {
            return Err(MeshError::TooBig);
        }
        let mut m = Mesh { next: s.next, ..Mesh::default() };
        for (id, v) in s.vertices {
            if !finite(v.position) {
                return Err(MeshError::Broken(format!("vertex {} is not a finite point", id.0)));
            }
            m.next = m.next.max(id.0);
            m.grid.entry(cell(v.position)).or_default().push(id);
            m.vertices.insert(id, v);
        }
        for (id, e) in s.edges {
            if !m.vertices.contains_key(&e.a) || !m.vertices.contains_key(&e.b) || e.a == e.b {
                return Err(MeshError::Broken(format!("edge {} has a missing end", id.0)));
            }
            m.next = m.next.max(id.0);
            m.link_edge(id, e);
        }
        for (id, f) in s.faces {
            m.next = m.next.max(id.0);
            if f.outer.len() < 3 || !finite(f.normal) {
                return Err(MeshError::Broken(format!("face {} has too few vertices", id.0)));
            }
            for ring in f.loops() {
                for (i, a) in ring.iter().enumerate() {
                    let b = ring[(i + 1) % ring.len()];
                    if m.edge_between(*a, b).is_none() {
                        return Err(MeshError::Broken(format!("face {} runs along no edge", id.0)));
                    }
                }
            }
            m.link_face(id, f);
        }
        for (id, c) in s.curves {
            m.next = m.next.max(id.0);
            if c.edges.iter().any(|e| m.edges.get(e).is_none_or(|e| e.curve != Some(id))) {
                return Err(MeshError::Broken(format!("curve {} lists an edge that isn't its", id.0)));
            }
            m.curves.insert(id, c);
        }
        if let Some((id, _)) = m.edges.iter().find(|(_, e)| e.curve.is_some_and(|c| !m.curves.contains_key(&c))) {
            return Err(MeshError::Broken(format!("edge {} is on a curve that isn't there", id.0)));
        }
        Ok(m)
    }
}

pub(crate) fn finite(p: Vec3) -> bool {
    [p.x, p.y, p.z].iter().all(|c| c.is_finite() && c.abs() < 1e9)
}

fn cell(p: Vec3) -> (i64, i64, i64) {
    // `finite` keeps coordinates under 1e9, so the cast never saturates.
    let k = |c: f64| (c / TOLERANCE).floor() as i64;
    (k(p.x), k(p.y), k(p.z))
}

impl Mesh {
    pub fn new() -> Mesh {
        Mesh::default()
    }

    pub fn vertex(&self, id: VertexId) -> Option<&Vertex> {
        self.vertices.get(&id)
    }
    pub fn edge(&self, id: EdgeId) -> Option<&Edge> {
        self.edges.get(&id)
    }
    pub fn face(&self, id: FaceId) -> Option<&Face> {
        self.faces.get(&id)
    }
    pub fn vertices(&self) -> impl Iterator<Item = (VertexId, &Vertex)> {
        self.vertices.iter().map(|(k, v)| (*k, v))
    }
    pub fn edges(&self) -> impl Iterator<Item = (EdgeId, &Edge)> {
        self.edges.iter().map(|(k, v)| (*k, v))
    }
    pub fn faces(&self) -> impl Iterator<Item = (FaceId, &Face)> {
        self.faces.iter().map(|(k, v)| (*k, v))
    }
    /// The edges that meet at `v`.
    pub fn edges_at(&self, v: VertexId) -> impl Iterator<Item = EdgeId> + '_ {
        self.vertex_edges.get(&v).into_iter().flatten().copied()
    }
    /// The faces `e` bounds.
    pub fn faces_of(&self, e: EdgeId) -> impl Iterator<Item = FaceId> + '_ {
        self.edge_faces.get(&e).into_iter().flatten().copied()
    }
    pub fn position(&self, v: VertexId) -> Option<Vec3> {
        self.vertices.get(&v).map(|v| v.position)
    }
    pub fn ends(&self, e: EdgeId) -> Option<(Vec3, Vec3)> {
        let e = self.edges.get(&e)?;
        Some((self.position(e.a)?, self.position(e.b)?))
    }
    pub fn edge_between(&self, a: VertexId, b: VertexId) -> Option<EdgeId> {
        self.edges_at(a).find(|e| self.edges.get(e).is_some_and(|e| e.other(a) == b))
    }

    fn id(&mut self) -> Result<u64, MeshError> {
        if self.vertices.len() + self.edges.len() + self.faces.len() >= MAX_ENTITIES {
            return Err(MeshError::TooBig);
        }
        self.next += 1;
        Ok(self.next)
    }

    pub(crate) fn fresh_edge_id(&mut self) -> Result<EdgeId, MeshError> {
        Ok(EdgeId(self.id()?))
    }

    pub(crate) fn fresh_curve_id(&mut self) -> Result<CurveId, MeshError> {
        Ok(CurveId(self.id()?))
    }

    /// Put vertex `v` at `p` (already checked finite), keeping the grid right.
    pub(crate) fn set_position(&mut self, v: VertexId, p: Vec3) {
        let Some(vx) = self.vertices.get_mut(&v) else { return };
        let old = cell(vx.position);
        vx.position = p;
        if old != cell(p) {
            if let Some(c) = self.grid.get_mut(&old) {
                c.retain(|id| *id != v);
            }
            self.grid.entry(cell(p)).or_default().push(v);
        }
        let edges: Vec<EdgeId> = self.edges_at(v).collect();
        for e in edges {
            self.index_edge(e);
            for f in self.faces_of(e).collect::<Vec<_>>() {
                self.index_face(f);
            }
        }
    }

    fn index_edge(&mut self, e: EdgeId) {
        if let Some((lo, hi)) = self.ends(e).and_then(|(a, b)| bounds([a, b])) {
            self.edge_index.insert(e, lo, hi);
        }
    }

    fn index_face(&mut self, f: FaceId) {
        let points: Vec<Vec3> = self.faces.get(&f).map(|face| face.outer.iter().filter_map(|v| self.position(*v)).collect()).unwrap_or_default();
        if let Some((lo, hi)) = bounds(points) {
            self.face_index.insert(f, lo, hi);
        }
    }

    /// The edges whose boxes meet the box from `lo` to `hi` (check them exactly).
    pub(crate) fn edges_near(&self, lo: Vec3, hi: Vec3) -> Vec<EdgeId> {
        self.edge_index.query(lo, hi)
    }

    /// The faces whose boxes meet the box from `lo` to `hi` (check them exactly).
    pub(crate) fn faces_near(&self, lo: Vec3, hi: Vec3) -> Vec<FaceId> {
        self.face_index.query(lo, hi)
    }

    /// The vertex within the tolerance of `p`, if there is one (the nearest).
    pub fn vertex_at(&self, p: Vec3) -> Option<VertexId> {
        if !finite(p) {
            return None;
        }
        let (x, y, z) = cell(p);
        let mut best: Option<(f64, VertexId)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for id in self.grid.get(&(x + dx, y + dy, z + dz)).into_iter().flatten() {
                        let Some(q) = self.position(*id) else { continue };
                        let d = q.dist(p);
                        if d <= TOLERANCE && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, *id));
                        }
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    /// The vertex at `p`: an existing one within the tolerance, else a new one (splitting the
    /// edge it lands on, if any).
    pub fn add_vertex(&mut self, p: Vec3) -> Result<VertexId, MeshError> {
        if !finite(p) {
            return Err(MeshError::BadPoint);
        }
        if let Some(v) = self.vertex_at(p) {
            return Ok(v);
        }
        let on = self.edges_near(p, p).into_iter().find(|e| self.interior_param(*e, p).is_some());
        let id = VertexId(self.id()?);
        self.vertices.insert(id, Vertex { position: p });
        self.grid.entry(cell(p)).or_default().push(id);
        if let Some(e) = on {
            self.split_edge_at(e, id)?;
        }
        Ok(id)
    }

    /// Where `p` lies along edge `e` (0 to 1, ends excluded), when it is on it.
    pub(crate) fn interior_param(&self, e: EdgeId, p: Vec3) -> Option<f64> {
        let (a, b) = self.ends(e)?;
        let ab = b - a;
        let len2 = ab.len2();
        if len2 <= TOLERANCE * TOLERANCE {
            return None;
        }
        let t = (p - a).dot(ab) / len2;
        let foot = a + ab * t;
        let reach = TOLERANCE / len2.sqrt();
        (t > reach && t < 1.0 - reach && foot.dist(p) <= TOLERANCE).then_some(t)
    }

    /// Split edge `e` at vertex `v` (already made, on the edge): two edges replace it, and every
    /// face along it gets `v` in its loop.
    pub(crate) fn split_edge_at(&mut self, e: EdgeId, v: VertexId) -> Result<(EdgeId, EdgeId), MeshError> {
        let edge = self.edges.get(&e).cloned().ok_or(MeshError::Missing("edge"))?;
        let faces: Vec<FaceId> = self.faces_of(e).collect();
        let at = edge.curve.and_then(|c| self.curves.get(&c)).and_then(|c| c.edges.iter().position(|x| *x == e));
        self.unlink_edge(e);
        let first = EdgeId(self.id()?);
        self.link_edge(first, Edge { a: edge.a, b: v, ..edge.clone() });
        let second = EdgeId(self.id()?);
        self.link_edge(second, Edge { a: v, b: edge.b, ..edge.clone() });
        // The halves take the edge's place on its curve, in the curve's order.
        if let (Some(c), Some(i)) = (edge.curve, at) {
            let before = i.checked_sub(1).and_then(|j| self.curves.get(&c).and_then(|c| c.edges.get(j).copied()));
            // After the removal, index `i` holds the edge that came next.
            let after = self.curves.get(&c).and_then(|c| c.edges.get(i).copied());
            let a_first = match (before.and_then(|x| self.edges.get(&x)), after.and_then(|x| self.edges.get(&x))) {
                (Some(prev), _) => prev.a == edge.a || prev.b == edge.a,
                (None, Some(next)) => next.a == edge.b || next.b == edge.b,
                (None, None) => true,
            };
            if let Some(curve) = self.curves.get_mut(&c) {
                let pair = if a_first { [first, second] } else { [second, first] };
                curve.edges.splice(i..i, pair);
            }
        }
        for f in faces {
            let Some(mut face) = self.unlink_face(f) else { continue };
            for ring in std::iter::once(&mut face.outer).chain(face.holes.iter_mut()) {
                insert_between(ring, edge.a, edge.b, v);
            }
            self.link_face(f, face);
        }
        Ok((first, second))
    }

    pub(crate) fn link_edge(&mut self, id: EdgeId, e: Edge) {
        self.vertex_edges.entry(e.a).or_default().insert(id);
        self.vertex_edges.entry(e.b).or_default().insert(id);
        self.edges.insert(id, e);
        self.index_edge(id);
    }

    pub(crate) fn unlink_edge(&mut self, id: EdgeId) -> Option<Edge> {
        let e = self.edges.remove(&id)?;
        self.edge_index.remove(id);
        if let Some(c) = e.curve.and_then(|c| self.curves.get_mut(&c)) {
            c.edges.retain(|x| *x != id);
        }
        for v in [e.a, e.b] {
            if let Some(set) = self.vertex_edges.get_mut(&v) {
                set.remove(&id);
            }
        }
        self.edge_faces.remove(&id);
        Some(e)
    }

    pub(crate) fn link_face(&mut self, id: FaceId, f: Face) {
        for ring in f.loops() {
            for (i, a) in ring.iter().enumerate() {
                if let Some(e) = self.edge_between(*a, ring[(i + 1) % ring.len()]) {
                    self.edge_faces.entry(e).or_default().insert(id);
                }
            }
        }
        self.faces.insert(id, f);
        self.index_face(id);
    }

    pub(crate) fn unlink_face(&mut self, id: FaceId) -> Option<Face> {
        let f = self.faces.remove(&id)?;
        self.face_index.remove(id);
        for ring in f.loops() {
            for (i, a) in ring.iter().enumerate() {
                if let Some(e) = self.edge_between(*a, ring[(i + 1) % ring.len()])
                    && let Some(set) = self.edge_faces.get_mut(&e)
                {
                    set.remove(&id);
                }
            }
        }
        Some(f)
    }

    pub(crate) fn new_face(&mut self, f: Face) -> Result<FaceId, MeshError> {
        let id = FaceId(self.id()?);
        self.link_face(id, f);
        Ok(id)
    }

    /// Erase a face, leaving its edges.
    pub fn erase_face(&mut self, f: FaceId) -> Result<(), MeshError> {
        self.unlink_face(f).map(|_| ()).ok_or(MeshError::Missing("face"))
    }

    /// Erase an edge, the faces it bounds and the vertices it leaves alone.
    pub fn erase_edge(&mut self, e: EdgeId) -> Result<(), MeshError> {
        let faces: Vec<FaceId> = self.faces_of(e).collect();
        for f in faces {
            self.unlink_face(f);
        }
        let edge = self.unlink_edge(e).ok_or(MeshError::Missing("edge"))?;
        for v in [edge.a, edge.b] {
            if self.edges_at(v).next().is_none() {
                self.remove_vertex(v);
            }
        }
        if let Some(c) = edge.curve
            && self.curves.get(&c).is_some_and(|c| c.edges.is_empty())
        {
            self.curves.remove(&c);
        }
        Ok(())
    }

    fn remove_vertex(&mut self, v: VertexId) {
        if let Some(vx) = self.vertices.remove(&v)
            && let Some(cell) = self.grid.get_mut(&cell(vx.position))
        {
            cell.retain(|id| *id != v);
        }
        self.vertex_edges.remove(&v);
    }

    /// Draw a straight edge from `p` to `q`. Its ends weld to vertices already there; it splits
    /// the edges it lands on or crosses, and the edges it passes along split it; then the faces
    /// it closes appear. Returns the edges it is made of, in order from `p`.
    /// Only the edges near it are looked at (the spatial index).
    pub fn add_edge(&mut self, p: Vec3, q: Vec3) -> Result<Vec<EdgeId>, MeshError> {
        if !finite(p) || !finite(q) {
            return Err(MeshError::BadPoint);
        }
        if p.dist(q) <= TOLERANCE {
            return Err(MeshError::ZeroLength);
        }
        let a = self.add_vertex(p)?;
        let b = self.add_vertex(q)?;
        let (pa, pb) = (self.position(a).ok_or(MeshError::Missing("vertex"))?, self.position(b).ok_or(MeshError::Missing("vertex"))?);
        // Points along the new edge where it must split: existing vertices on it, and where it
        // crosses existing edges (which split there too).
        let mut stops: Vec<(f64, VertexId)> = vec![(0.0, a), (1.0, b)];
        let dir = pb - pa;
        let len2 = dir.len2();
        // Only edges near the new one can meet it; a vertex on it is an end of one of them.
        let (lo, hi) = (pa.min(pb), pa.max(pb));
        let near: Vec<EdgeId> = self.edges_near(lo, hi);
        let reach = TOLERANCE / len2.sqrt();
        let mut on_it = std::collections::BTreeSet::new();
        for e in &near {
            let Some(edge) = self.edges.get(e) else { continue };
            for v in [edge.a, edge.b] {
                if v == a || v == b || !on_it.insert(v) {
                    continue;
                }
                let Some(q) = self.position(v) else { continue };
                let t = (q - pa).dot(dir) / len2;
                if t > reach && t < 1.0 - reach && (pa + dir * t).dist(q) <= TOLERANCE {
                    stops.push((t, v));
                }
            }
        }
        let crossings: Vec<(EdgeId, Vec3, f64)> = near
            .iter()
            .filter_map(|id| {
                let e = self.edges.get(id)?;
                if [e.a, e.b].iter().any(|v| *v == a || *v == b) {
                    return None;
                }
                let (c, d) = (self.position(e.a)?, self.position(e.b)?);
                crossing(pa, pb, c, d).map(|(x, t)| (*id, x, t))
            })
            .collect();
        for (e, x, t) in crossings {
            if stops.iter().any(|(s, _)| (s - t).abs() * len2.sqrt() <= TOLERANCE) {
                continue;
            }
            let v = VertexId(self.id()?);
            self.vertices.insert(v, Vertex { position: x });
            self.grid.entry(cell(x)).or_default().push(v);
            self.split_edge_at(e, v)?;
            stops.push((t, v));
        }
        stops.sort_by(|x, y| x.0.total_cmp(&y.0));
        stops.dedup_by_key(|s| s.1);
        let mut made = Vec::new();
        for pair in stops.windows(2) {
            let (u, w) = (pair[0].1, pair[1].1);
            let e = match self.edge_between(u, w) {
                Some(e) => e,
                None => {
                    let e = EdgeId(self.id()?);
                    self.link_edge(e, Edge::plain(u, w));
                    e
                }
            };
            made.push(e);
        }
        for e in &made {
            self.close_faces(*e)?;
        }
        Ok(made)
    }
}

/// Insert `v` between neighbours `a` and `b` of a loop, wherever they sit next to each other.
fn insert_between(ring: &mut Vec<VertexId>, a: VertexId, b: VertexId, v: VertexId) {
    let n = ring.len();
    for i in 0..n {
        let (x, y) = (ring[i], ring[(i + 1) % n]);
        if (x == a && y == b) || (x == b && y == a) {
            ring.insert(i + 1, v);
            return;
        }
    }
}

/// Where segment `p`–`q` crosses segment `c`–`d` inside both (not at their ends), and how far
/// along `p`–`q` (0 to 1).
fn crossing(p: Vec3, q: Vec3, c: Vec3, d: Vec3) -> Option<(Vec3, f64)> {
    let (u, v, w) = (q - p, d - c, p - c);
    let (a, b, cc, dd, e) = (u.dot(u), u.dot(v), v.dot(v), u.dot(w), v.dot(w));
    let den = a * cc - b * b;
    if den.abs() <= 1e-12 * a * cc {
        return None; // parallel: an overlap splits through the vertices on it
    }
    let s = (b * e - cc * dd) / den;
    let t = (a * e - b * dd) / den;
    let (x, y) = (p + u * s, c + v * t);
    let (ru, rv) = (TOLERANCE / a.sqrt(), TOLERANCE / cc.sqrt());
    (s > ru && s < 1.0 - ru && t > rv && t < 1.0 - rv && x.dist(y) <= TOLERANCE).then_some(((x + y) * 0.5, s))
}
