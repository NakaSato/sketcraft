//! Faces appear when an edge closes a flat loop, as in SketchUp. Around a new edge, each plane
//! it shares with its neighbours is walked: from the edge, always turning to the next edge
//! clockwise, which goes once round the region to the edge's left. A region that comes back to
//! the edge is a face. An edge across a face divides it in two; a loop drawn inside a face
//! becomes a hole in it and a face of its own.

use std::collections::BTreeSet;

use sketcore_record::{Vec2, Vec3};
use sketcraft_geom::prelude::*;

use crate::TOLERANCE;
use crate::model::{EdgeId, Face, FaceId, Mesh, MeshError, VertexId};
use crate::plane::{Plane, inside, ring_inside, signed_area, simple};

/// The most edges one walk follows: a face with more is not made (SketchUp's largest curves have
/// 999 segments), and the walk round the outside of a big drawing stops there.
const MAX_WALK: usize = 100_000;

impl Mesh {
    /// The faces edge `e` closes: made, and the faces they divide or sit in updated. Returns the
    /// new faces.
    pub(crate) fn close_faces(&mut self, e: EdgeId) -> Result<Vec<FaceId>, MeshError> {
        let Some(edge) = self.edge(e).cloned() else { return Err(MeshError::Missing("edge")) };
        let (a, b) = (edge.a, edge.b);
        let (Some(pa), Some(pb)) = (self.position(a), self.position(b)) else { return Err(MeshError::Missing("vertex")) };
        let Some(dir) = (pb - pa).normalized() else { return Ok(Vec::new()) };
        let mut made = Vec::new();
        for normal in self.planes_around(a, b, pa, dir) {
            let plane = Plane { origin: pa, u: dir, v: normal.cross(dir), normal };
            // The holes of a face the edge divides, for whichever side they are on.
            let mut orphans = Vec::new();
            for ring in self.regions(a, b, &plane) {
                if let Some(f) = self.place_face(ring, &plane, (pa + pb) * 0.5, &mut orphans)? {
                    made.push(f);
                }
            }
        }
        Ok(made)
    }

    /// The normals of the planes the edge `a`–`b` shares with the edges next to it (through
    /// the vertices on its line), each pointing the way its largest coordinate is positive.
    fn planes_around(&self, a: VertexId, b: VertexId, origin: Vec3, dir: Vec3) -> Vec<Vec3> {
        let mut normals: Vec<Vec3> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut todo = vec![a, b];
        while let Some(v) = todo.pop() {
            if !seen.insert(v) || seen.len() > MAX_WALK {
                continue;
            }
            for n in self.neighbours(v) {
                let Some(p) = self.position(n) else { continue };
                let cross = dir.cross(p - origin);
                let Some(mut normal) = cross.normalized().filter(|_| cross.len() > TOLERANCE) else {
                    todo.push(n); // on the edge's line: look past it
                    continue;
                };
                let big = [normal.x, normal.y, normal.z].into_iter().max_by(|x, y| x.abs().total_cmp(&y.abs())).unwrap_or(1.0);
                if big < 0.0 {
                    normal = -normal;
                }
                if normals.iter().all(|m| m.dot(normal) < 1.0 - 1e-9) {
                    normals.push(normal);
                }
            }
        }
        normals
    }

    fn neighbours(&self, v: VertexId) -> impl Iterator<Item = VertexId> + '_ {
        self.edges_at(v).filter_map(move |e| self.edge(e).map(|e| e.other(v)))
    }

    /// The bounded regions on either side of edge `a`–`b` in `plane`, as loops running
    /// counter-clockwise seen from the plane's normal.
    fn regions(&self, a: VertexId, b: VertexId, plane: &Plane) -> Vec<Vec<VertexId>> {
        // Each vertex's neighbours in the plane, sorted counter-clockwise, looked up as the walk
        // reaches it (not for the whole model).
        let in_plane = |v: VertexId| self.position(v).is_some_and(|p| plane.distance(p).abs() <= TOLERANCE);
        let around = |v: VertexId| -> Vec<VertexId> {
            let Some(centre) = self.position(v).map(|p| plane.project(p)) else { return Vec::new() };
            let mut next: Vec<(f64, VertexId)> = self
                .neighbours(v)
                .filter(|n| in_plane(*n))
                .filter_map(|n| {
                    let d = plane.project(self.position(n)?) - centre;
                    Some((d.y.atan2(d.x), n))
                })
                .collect();
            next.sort_by(|x, y| x.0.total_cmp(&y.0));
            next.into_iter().map(|(_, n)| n).collect()
        };
        let mut rings = Vec::new();
        for (start, first) in [(a, b), (b, a)] {
            let (mut prev, mut cur) = (start, first);
            let mut ring = vec![start];
            let mut closed = false;
            for _ in 0..MAX_WALK {
                ring.push(cur);
                let next = around(cur);
                let Some(i) = next.iter().position(|n| *n == prev) else { break };
                // The next edge clockwise from the one we came in by.
                let turn = next[(i + next.len() - 1) % next.len()];
                (prev, cur) = (cur, turn);
                if prev == start && cur == first {
                    ring.pop();
                    closed = true;
                    break;
                }
            }
            let ring = strip_spurs(ring);
            if !closed || ring.len() < 3 || ring.iter().collect::<BTreeSet<_>>().len() != ring.len() || !has_edge(&ring, a, b) {
                continue;
            }
            let Some(flat) = self.flat(&ring, plane) else { continue };
            if signed_area(&flat) > TOLERANCE * TOLERANCE && simple(&flat) {
                rings.push(ring);
            }
        }
        rings
    }

    fn flat(&self, ring: &[VertexId], plane: &Plane) -> Option<Vec<Vec2>> {
        ring.iter().map(|v| self.position(*v).map(|p| plane.project(p))).collect()
    }

    /// The faces in `plane` (either way round) whose boxes meet `ring`'s: every face the new
    /// face can divide, hold or sit in.
    fn faces_in(&self, plane: &Plane, ring: &[VertexId]) -> Vec<FaceId> {
        let Some((lo, hi)) = crate::index::bounds(ring.iter().filter_map(|v| self.position(*v))) else { return Vec::new() };
        self.faces_near(lo, hi)
            .into_iter()
            .filter(|id| {
                self.face(*id).is_some_and(|f| {
                    f.normal.dot(plane.normal).abs() > 1.0 - 1e-9
                        && f.outer.iter().all(|v| self.position(*v).is_some_and(|p| plane.distance(p).abs() <= TOLERANCE))
                })
            })
            .collect()
    }

    /// Make the face `ring` bounds, unless it is there already. A face the new edge divides
    /// (its middle `mid` inside it) gives way to the regions on both sides, its holes going to
    /// `orphans` for whichever side holds them; a face the loop sits inside gets it as a hole.
    fn place_face(&mut self, ring: Vec<VertexId>, plane: &Plane, mid: Vec3, orphans: &mut Vec<Vec<VertexId>>) -> Result<Option<FaceId>, MeshError> {
        let key = canonical(&ring);
        let coplanar = self.faces_in(plane, &ring);
        if coplanar.iter().any(|f| self.face(*f).is_some_and(|f| canonical(&f.outer) == key)) {
            return Ok(None);
        }
        let Some(flat) = self.flat(&ring, plane) else { return Ok(None) };
        let mid2 = plane.project(mid);
        // The face the new edge runs across: its middle is inside the face (not in a hole), and
        // the ring shares the face's outline.
        let divided: Vec<FaceId> = coplanar
            .iter()
            .copied()
            .filter(|f| {
                let Some(face) = self.face(*f) else { return false };
                let Some(outer) = self.flat(&face.outer, plane) else { return false };
                inside(mid2, &outer, false)
                    && !face.holes.iter().any(|h| self.flat(h, plane).is_some_and(|h| inside(mid2, &h, true)))
                    && shares_edge(&ring, &face.outer)
            })
            .collect();
        // Loops that may become holes of the new face: the outlines of other faces here, and the
        // holes of the face it divides.
        for f in &divided {
            if let Some(face) = self.face(*f) {
                orphans.extend(face.holes.iter().cloned());
            }
        }
        let mut candidates: Vec<Vec<VertexId>> = orphans.clone();
        for f in coplanar.iter().filter(|f| !divided.contains(f)) {
            if let Some(face) = self.face(*f) {
                candidates.push(face.outer.clone());
            }
        }
        for f in &divided {
            self.unlink_face(*f);
        }
        let holes = self.outermost(&candidates, &flat, plane);
        let id = self.new_face(Face { outer: ring.clone(), holes, normal: plane.normal })?;
        // A face the new loop lies inside gets it as a hole (its holes inside the loop go to
        // the new face, which took them above).
        for f in coplanar.into_iter().filter(|f| !divided.contains(f)) {
            let Some(face) = self.face(f).cloned() else { continue };
            let Some(outer) = self.flat(&face.outer, plane) else { continue };
            if canonical(&face.outer) == key || !ring_inside(&flat, &outer) {
                continue;
            }
            if face.holes.iter().any(|h| self.flat(h, plane).is_some_and(|h| ring_inside(&flat, &h))) {
                continue;
            }
            let Some(mut face) = self.unlink_face(f) else { continue };
            let mut loops = face.holes.clone();
            loops.push(ring.clone());
            face.holes = self.outermost(&loops, &outer, plane);
            self.link_face(f, face);
        }
        Ok(Some(id))
    }

    /// The loops of `candidates` inside `outer` that no other one of them encloses.
    fn outermost(&self, candidates: &[Vec<VertexId>], outer: &[Vec2], plane: &Plane) -> Vec<Vec<VertexId>> {
        let flats: Vec<(Vec<VertexId>, Vec<Vec2>)> =
            candidates.iter().filter_map(|r| Some((r.clone(), self.flat(r, plane)?))).filter(|(_, f)| ring_inside(f, outer)).collect();
        let mut out: Vec<Vec<VertexId>> = Vec::new();
        for (i, (r, f)) in flats.iter().enumerate() {
            let enclosed = flats.iter().enumerate().any(|(j, (s, g))| i != j && canonical(r) != canonical(s) && ring_inside(f, g));
            if !enclosed && !out.iter().any(|o| canonical(o) == canonical(r)) {
                out.push(r.clone());
            }
        }
        out
    }
}

/// The ring started at its smallest vertex, in its smaller direction: equal for the same loop
/// whichever vertex it starts at and whichever way it runs.
pub(crate) fn canonical(ring: &[VertexId]) -> Vec<VertexId> {
    let n = ring.len();
    let Some(start) = (0..n).min_by_key(|i| ring[*i]) else { return Vec::new() };
    let forward: Vec<VertexId> = (0..n).map(|i| ring[(start + i) % n]).collect();
    let backward: Vec<VertexId> = (0..n).map(|i| ring[(start + n - i) % n]).collect();
    forward.min(backward)
}

fn has_edge(ring: &[VertexId], a: VertexId, b: VertexId) -> bool {
    let n = ring.len();
    (0..n).any(|i| {
        let (x, y) = (ring[i], ring[(i + 1) % n]);
        (x == a && y == b) || (x == b && y == a)
    })
}

fn shares_edge(ring: &[VertexId], other: &[VertexId]) -> bool {
    let n = ring.len();
    (0..n).any(|i| has_edge(other, ring[i], ring[(i + 1) % n]))
}

/// A walk into a dead end comes back the way it went (… x, y, x …): drop those spurs.
fn strip_spurs(mut ring: Vec<VertexId>) -> Vec<VertexId> {
    loop {
        let n = ring.len();
        if n < 3 {
            return ring;
        }
        let Some(i) = (0..n).find(|i| ring[(i + n - 1) % n] == ring[(i + 1) % n]) else { return ring };
        // Drop the tip and one copy of the vertex the walk came back to.
        let (tip, back) = (i, (i + 1) % n);
        let (first, second) = if tip < back { (tip, back) } else { (back, tip) };
        ring.remove(second);
        ring.remove(first);
    }
}
