//! Push/Pull, as SketchUp's: a flat face moved along its normal, with walls joining it to where
//! it was.
//!
//! The walls and the moved face are made by drawing edges (one from each corner, then round the
//! moved loops), so the faces appear the way drawn ones do. A face on its own stays where it was,
//! as the bottom of what it makes; a face of a solid (with faces beyond its edges) goes, and each
//! new wall in line with the face beyond its edge joins it into one face.

use std::collections::{BTreeMap, BTreeSet};

use sketcore_record::Vec3;
use sketcraft_geom::prelude::*;

use crate::TOLERANCE;
use crate::faces::canonical;
use crate::model::{EdgeId, Face, FaceId, Mesh, MeshError, VertexId};
use crate::plane::{Plane, inside};

impl Mesh {
    /// Push or pull face `f` by `distance` along its normal (negative pushes it in). Returns the
    /// moved face.
    pub fn push_pull(&mut self, f: FaceId, distance: f64) -> Result<FaceId, MeshError> {
        let face = self.face(f).cloned().ok_or(MeshError::Missing("face"))?;
        if !distance.is_finite() || distance.abs() >= 1e9 {
            return Err(MeshError::BadPoint);
        }
        if distance.abs() <= TOLERANCE {
            return Err(MeshError::ZeroLength);
        }
        let offset = face.normal * distance;
        let loops: Vec<Vec<VertexId>> = face.loops().cloned().collect();
        // A face of a solid: one of its edges bounds another face too.
        let on_solid = loop_edges(&loops).any(|(u, w)| self.edge_between(u, w).is_some_and(|e| self.faces_of(e).count() > 1));
        // The faces beyond the face's edges, by edge, to join the walls in line with them.
        let beyond: BTreeMap<(VertexId, VertexId), Vec3> = loop_edges(&loops)
            .filter_map(|(u, w)| {
                let e = self.edge_between(u, w)?;
                let other = self.faces_of(e).find(|g| *g != f)?;
                Some(((u, w), self.face(other)?.normal))
            })
            .collect();
        // The corners where two segments of one curve meet: the wall edge up from each is soft
        // and smooth, so the walls of a pulled circle shade as one round side.
        let mut rounded: Vec<VertexId> = Vec::new();
        for ring in &loops {
            let n = ring.len();
            for i in 0..n {
                let curve_of = |x: VertexId, y: VertexId| self.edge_between(x, y).and_then(|e| self.edge(e)).and_then(|e| e.curve);
                let (before, after) = (curve_of(ring[(i + n - 1) % n], ring[i]), curve_of(ring[i], ring[(i + 1) % n]));
                if before.is_some() && before == after {
                    rounded.push(ring[i]);
                }
            }
        }
        let start = centroid(self, &face.outer);
        // Out of the face across each of its edges: the way the wall pulled from that edge faces.
        let outward = self.outward_across(&face)?;
        // The faces there before the pull keep the way they face.
        let before: BTreeSet<FaceId> =
            loops.iter().flatten().flat_map(|v| self.edges_at(*v).collect::<Vec<_>>()).flat_map(|e| self.faces_of(e).collect::<Vec<_>>()).collect();
        if on_solid {
            self.erase_face(f)?;
        }
        // One edge from each corner, then round the moved loops.
        let mut moved: BTreeMap<VertexId, VertexId> = BTreeMap::new();
        for v in loops.iter().flatten() {
            let p = self.position(*v).ok_or(MeshError::Missing("vertex"))?;
            let made = self.add_edge(p, p + offset)?;
            let top = made
                .last()
                .and_then(|e| self.edge(*e))
                .map(|e| if self.position(e.a).is_some_and(|q| q.dist(p + offset) <= TOLERANCE) { e.a } else { e.b });
            moved.insert(*v, top.ok_or(MeshError::Missing("vertex"))?);
        }
        for (u, w) in loop_edges(&loops) {
            let (pu, pw) =
                (self.position(moved[&u]).ok_or(MeshError::Missing("vertex"))?, self.position(moved[&w]).ok_or(MeshError::Missing("vertex"))?);
            self.add_edge(pu, pw)?;
        }
        // Join each wall to the face beyond its edge when they are in one plane.
        let mut joined = BTreeSet::new();
        if on_solid {
            for ((u, w), normal) in &beyond {
                if let Some(e) = self.edge_between(*u, *w) {
                    joined.extend(self.join_across(e, *normal, moved[u])?);
                }
            }
        }
        // The moved holes stay open: drawing their loops filled them.
        for hole in &face.holes {
            let ring: Vec<VertexId> = hole.iter().map(|v| moved[v]).collect();
            let key = canonical(&ring);
            // A face filling the hole runs along its first edge.
            let first = ring.first().zip(ring.get(1)).and_then(|(x, y)| self.edge_between(*x, *y));
            let filled: Vec<FaceId> = first
                .into_iter()
                .flat_map(|e| self.faces_of(e).collect::<Vec<_>>())
                .filter(|g| self.face(*g).is_some_and(|g| canonical(&g.outer) == key))
                .collect();
            for g in filled {
                self.erase_face(g)?;
            }
        }
        // Pushed into a solid, the walls run along its sides, leaving strips above the moved face
        // bounded by nothing beyond the old outline: they go, with the edges left without faces.
        if on_solid {
            for (u, w) in loop_edges(&loops) {
                if let Some(e) = self.edge_between(u, w)
                    && self.faces_of(e).count() <= 1
                {
                    self.erase_edge(e)?;
                }
            }
            for v in loops.iter().flatten() {
                for e in self.edges_at(*v).collect::<Vec<_>>() {
                    if self.faces_of(e).next().is_none() {
                        self.erase_edge(e)?;
                    }
                }
            }
        }
        // Turn the new faces out of the solid: away from the middle of what the pull swept.
        let middle = start + offset * 0.5;
        let cap_ring: Vec<VertexId> = face.outer.iter().map(|v| moved[v]).collect();
        let cap_key = canonical(&cap_ring);
        let mut cap = None;
        let mut flipped = BTreeSet::new();
        let touched: Vec<FaceId> = loops
            .iter()
            .flatten()
            .flat_map(|v| [*v, moved[v]])
            .flat_map(|v| self.edges_at(v).collect::<Vec<_>>())
            .flat_map(|e| self.faces_of(e).collect::<Vec<_>>())
            .collect();
        for g in touched {
            let Some(face) = self.face(g) else { continue };
            if canonical(&face.outer) == cap_key {
                cap = Some(g);
            }
            if before.contains(&g) || joined.contains(&g) || canonical(&face.outer) == cap_key || !flipped.insert(g) {
                continue;
            }
            // A wall faces out across the edge it was pulled from; any other new face, away from
            // the middle of what the pull swept.
            let ring = face.outer.clone();
            let wall =
                loop_edges(&loops).find(|(u, w)| (ring.contains(u) && ring.contains(w)) || (ring.contains(&moved[u]) && ring.contains(&moved[w])));
            let out = match wall.and_then(|k| outward.get(&k)) {
                Some(d) => *d,
                None => centroid(self, &face.outer) - middle,
            };
            if out.dot(face.normal) < 0.0 {
                self.flip_face(g)?;
            }
        }
        for v in rounded {
            if let Some(e) = moved.get(&v).and_then(|w| self.edge_between(v, *w)) {
                self.set_soft(e, true, true)?;
            }
        }
        let cap = cap.ok_or(MeshError::Missing("face"))?;
        // A face on its own stays as the bottom of what it made, facing away from it.
        if !on_solid && self.face(f).is_some_and(|g| g.normal.dot(offset) > 0.0) {
            self.flip_face(f)?;
        }
        // The moved face faces the way the face did on a solid, and the way it moved on its own.
        let want = if on_solid { face.normal } else { offset };
        if self.face(cap).is_some_and(|c| c.normal.dot(want) < 0.0) {
            self.flip_face(cap)?;
        }
        Ok(cap)
    }

    /// For each edge of `face`'s loops, the unit direction in its plane away from the face.
    fn outward_across(&self, face: &Face) -> Result<BTreeMap<(VertexId, VertexId), Vec3>, MeshError> {
        let p0 = self.position(face.outer[0]).ok_or(MeshError::Missing("vertex"))?;
        let plane = Plane::across(p0, face.normal);
        let flat =
            |r: &Vec<VertexId>| -> Option<Vec<sketcore_record::Vec2>> { r.iter().map(|v| self.position(*v).map(|p| plane.project(p))).collect() };
        let outer = flat(&face.outer).ok_or(MeshError::Missing("vertex"))?;
        let holes: Vec<_> = face.holes.iter().filter_map(flat).collect();
        let in_face = |q: Vec3| {
            let q = plane.project(q);
            inside(q, &outer, false) && !holes.iter().any(|h| inside(q, h, true))
        };
        let mut out = BTreeMap::new();
        let loops: Vec<Vec<VertexId>> = face.loops().cloned().collect();
        for (u, w) in loop_edges(&loops) {
            let (pu, pw) = (self.position(u).ok_or(MeshError::Missing("vertex"))?, self.position(w).ok_or(MeshError::Missing("vertex"))?);
            let Some(across) = face.normal.cross(pw - pu).normalized() else { continue };
            let probe = (pu + pw) * 0.5 + across * (TOLERANCE * 4.0);
            out.insert((u, w), if in_face(probe) { -across } else { across });
        }
        Ok(out)
    }

    /// Turn face `f` over: its normal the other way, its loops the other way round.
    pub fn flip_face(&mut self, f: FaceId) -> Result<(), MeshError> {
        let face = self.faces.get_mut(&f).ok_or(MeshError::Missing("face"))?;
        face.normal = -face.normal;
        face.outer.reverse();
        for h in &mut face.holes {
            h.reverse();
        }
        Ok(())
    }

    /// When edge `e` joins two faces in one plane, take it out so they are one face, turned as
    /// `normal` says. `keep` is a vertex of the joined face away from `e`, to find it again.
    fn join_across(&mut self, e: EdgeId, normal: Vec3, keep: VertexId) -> Result<Vec<FaceId>, MeshError> {
        let faces: Vec<FaceId> = self.faces_of(e).collect();
        let [g, h] = faces.as_slice() else { return Ok(Vec::new()) };
        let (Some(fg), Some(fh)) = (self.face(*g), self.face(*h)) else { return Ok(Vec::new()) };
        if fg.normal.dot(fh.normal).abs() < 1.0 - 1e-9 {
            return Ok(Vec::new());
        }
        let Some(edge) = self.edge(e).cloned() else { return Ok(Vec::new()) };
        // An edge of the joined face that stays: the wall's edge up from one end of `e`.
        let Some(side) = self
            .edge_between(edge.a, keep)
            .or_else(|| self.edge_between(edge.b, keep))
            .or_else(|| self.edges_at(edge.a).chain(self.edges_at(edge.b)).find(|x| *x != e && self.faces_of(*x).any(|f| f == *g || f == *h)))
        else {
            return Ok(Vec::new());
        };
        self.erase_edge(e)?;
        let made = self.close_faces(side)?;
        for g in &made {
            if self.face(*g).is_some_and(|f| f.normal.dot(normal) < 0.0) {
                self.flip_face(*g)?;
            }
        }
        Ok(made)
    }
}

fn loop_edges(loops: &[Vec<VertexId>]) -> impl Iterator<Item = (VertexId, VertexId)> + '_ {
    loops.iter().flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
}

fn centroid(m: &Mesh, ring: &[VertexId]) -> Vec3 {
    let pts: Vec<Vec3> = ring.iter().filter_map(|v| m.position(*v)).collect();
    let n = pts.len().max(1) as f64;
    pts.into_iter().fold(Vec3::ZERO, |s, p| s + p) * (1.0 / n)
}

impl Face {
    /// The face's area, its holes taken out, in mm².
    pub fn area(&self, m: &Mesh) -> f64 {
        let ring = |r: &Vec<VertexId>| {
            let pts: Vec<Vec3> = r.iter().filter_map(|v| m.position(*v)).collect();
            let mut s = Vec3::ZERO;
            for (i, p) in pts.iter().enumerate() {
                s += p.cross(pts[(i + 1) % pts.len()]);
            }
            s.dot(self.normal).abs() / 2.0
        };
        ring(&self.outer) - self.holes.iter().map(ring).sum::<f64>()
    }
}
