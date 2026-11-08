//! Move, Rotate and Scale, as SketchUp's: any vertices, edges and faces move, and the faces
//! joined to them stretch with them. A face that stays flat keeps its shape; one that would bend
//! folds into flat triangles along new edges, as SketchUp folds it.

use std::collections::{BTreeMap, BTreeSet};

use sketcore_record::Vec3;
use sketcraft_geom::prelude::*;

use crate::TOLERANCE;
use crate::model::{Edge, EdgeId, Face, FaceId, Mesh, MeshError, VertexId, finite};

impl Mesh {
    /// The vertices of some edges and faces (with their holes): what moving them moves.
    pub fn vertices_of(&self, edges: &[EdgeId], faces: &[FaceId]) -> BTreeSet<VertexId> {
        let mut out = BTreeSet::new();
        for e in edges.iter().filter_map(|e| self.edge(*e)) {
            out.extend([e.a, e.b]);
        }
        for f in faces.iter().filter_map(|f| self.face(*f)) {
            out.extend(f.loops().flatten().copied());
        }
        out
    }

    /// Move `vertices` by `delta`.
    pub fn move_by(&mut self, vertices: &BTreeSet<VertexId>, delta: Vec3) -> Result<(), MeshError> {
        self.transform(vertices, |p| p + delta)
    }

    /// Turn `vertices` by `degrees` about the line through `origin` along `axis` (right-handed).
    pub fn rotate(&mut self, vertices: &BTreeSet<VertexId>, origin: Vec3, axis: Vec3, degrees: f64) -> Result<(), MeshError> {
        let k = axis.normalized().ok_or(MeshError::BadPoint)?;
        if !degrees.is_finite() || !finite(origin) {
            return Err(MeshError::BadPoint);
        }
        let (s, c) = degrees.to_radians().sin_cos();
        // Rodrigues: v cos a + (k × v) sin a + k (k · v)(1 − cos a).
        self.transform(vertices, |p| {
            let v = p - origin;
            origin + v * c + k.cross(v) * s + k * (k.dot(v) * (1.0 - c))
        })
    }

    /// Scale `vertices` about `origin` by `factors` along X, Y and Z (each above 1e-9 in size).
    pub fn scale(&mut self, vertices: &BTreeSet<VertexId>, origin: Vec3, factors: Vec3) -> Result<(), MeshError> {
        if !finite(origin) || !finite(factors) || [factors.x, factors.y, factors.z].iter().any(|f| f.abs() < 1e-9) {
            return Err(MeshError::BadPoint);
        }
        self.transform(vertices, |p| {
            let v = p - origin;
            origin + Vec3::new(v.x * factors.x, v.y * factors.y, v.z * factors.z)
        })
    }

    /// Move each of `vertices` to `to(position)`, then fit the faces joined to them: a flat one
    /// takes its new normal, one that bends folds into triangles. Nothing changes when it is
    /// refused (a point out of range, or a face with holes that would bend).
    pub fn transform(&mut self, vertices: &BTreeSet<VertexId>, to: impl Fn(Vec3) -> Vec3) -> Result<(), MeshError> {
        let mut moved: BTreeMap<VertexId, Vec3> = BTreeMap::new();
        for v in vertices {
            let p = self.position(*v).ok_or(MeshError::Missing("vertex"))?;
            let q = to(p);
            if !finite(q) {
                return Err(MeshError::BadPoint);
            }
            moved.insert(*v, q);
        }
        let at = |m: &Mesh, v: VertexId| moved.get(&v).copied().or_else(|| m.position(v));
        // The faces joined to what moves, and what each becomes.
        let touched: BTreeSet<FaceId> =
            vertices.iter().flat_map(|v| self.edges_at(*v).collect::<Vec<_>>()).flat_map(|e| self.faces_of(e).collect::<Vec<_>>()).collect();
        let mut flat: Vec<(FaceId, Vec3)> = Vec::new();
        let mut fold: Vec<FaceId> = Vec::new();
        for f in &touched {
            let Some(face) = self.face(*f) else { continue };
            let points: Option<Vec<Vec3>> = face.outer.iter().map(|v| at(self, *v)).collect();
            let points = points.ok_or(MeshError::Missing("vertex"))?;
            match newell(&points) {
                Some(n)
                    if flat_in(&points, n)
                        && face.holes.iter().all(|h| h.iter().all(|v| at(self, *v).is_some_and(|p| (p - points[0]).dot(n).abs() <= TOLERANCE))) =>
                {
                    flat.push((*f, n))
                }
                _ if !face.holes.is_empty() => return Err(MeshError::Bends),
                _ => fold.push(*f),
            }
        }
        // Move the vertices, keeping the grid right.
        for (v, q) in &moved {
            self.set_position(*v, *q);
        }
        for (f, n) in flat {
            if let Some(face) = self.faces.get_mut(&f) {
                face.normal = n;
            }
        }
        for f in fold {
            self.fold(f)?;
        }
        Ok(())
    }

    /// Fold a face that no longer lies flat into a fan of triangles from its first vertex, each
    /// facing the way the face did.
    fn fold(&mut self, f: FaceId) -> Result<(), MeshError> {
        let Some(face) = self.unlink_face(f) else { return Ok(()) };
        let ring = face.outer.clone();
        let tip = ring[0];
        for i in 1..ring.len() - 1 {
            let (b, c) = (ring[i], ring[i + 1]);
            for (x, y) in [(tip, b), (b, c), (c, tip)] {
                if self.edge_between(x, y).is_none() {
                    let e = self.fresh_edge_id()?;
                    self.link_edge(e, Edge::plain(x, y));
                }
            }
            let points: Option<Vec<Vec3>> = [tip, b, c].iter().map(|v| self.position(*v)).collect();
            let Some(mut n) = points.and_then(|p| newell(&p)) else { continue };
            let mut outer = vec![tip, b, c];
            if n.dot(face.normal) < 0.0 {
                n = -n;
                outer.reverse();
            }
            self.new_face(Face { outer, holes: Vec::new(), normal: n })?;
        }
        Ok(())
    }
}

/// The unit normal of a loop of points (Newell's method), when they are not all on one line.
pub(crate) fn newell(points: &[Vec3]) -> Option<Vec3> {
    let mut n = Vec3::ZERO;
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        n.x += (p.y - q.y) * (p.z + q.z);
        n.y += (p.z - q.z) * (p.x + q.x);
        n.z += (p.x - q.x) * (p.y + q.y);
    }
    n.normalized()
}

fn flat_in(points: &[Vec3], n: Vec3) -> bool {
    points.iter().all(|p| (*p - points[0]).dot(n).abs() <= TOLERANCE)
}
