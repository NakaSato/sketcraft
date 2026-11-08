//! Bounded, chronological checks of a joint trajectory. The motion is sampled, so this is not
//! continuous collision detection. A joint drive decides on the exact bodies whether they pass
//! into each other at a sample ([`ContactGeometry::overlaps`]) and closes in on where they first
//! touch through placements known to be clear ([`ContactGeometry::gap`]); gaps and interference
//! depths are measured on the bodies' contact meshes.
use sketcraft_geom::prelude::*;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use sketcore_record::Vec3;
use sketcraft_doc::joints;
use sketcraft_doc::{Document, IDENTITY, Mat, apply_point, mat_mul};
use sketcraft_geom::{Aabb3, Mesh};

use crate::params::bad;
use crate::{Result, Session};

const MAX_SAMPLES: usize = 4096;
const MAX_PLACEMENTS: usize = 4096;
const MAX_BODIES: usize = 256;
const MAX_TRIANGLES: usize = 50_000;
const MAX_WORK: usize = 50_000_000;
/// Most exact intersections one joint drive works out: one per pair of bodies whose exact boxes
/// meet, at each sample and bisection step.
const MAX_INTERSECTIONS: usize = 2048;

fn limit(message: &str) -> crate::EngineError {
    bad("contact.sweep", message)
}

#[derive(Clone)]
struct CachedBody {
    path: Vec<u64>,
    offset: Mat,
    /// The body itself, for the exact checks, and its exact box in its own frame.
    body: sketcraft_kernel::Body,
    exact_box: Aabb3,
    mesh: Arc<Mesh>,
    size: f64,
    /// How far its mesh may stray from its faces: 0 when every face is flat (the mesh is the
    /// shape), else the chord tolerance it was tessellated to.
    chord: f64,
}

/// Bodies [`ContactMeshes`] keeps; it starts over when full.
const MAX_CACHED_MESHES: usize = 4 * MAX_BODIES;

/// A body's contact mesh and what is measured on it, made once per body content.
#[derive(Clone)]
pub(crate) struct ContactMesh {
    mesh: Arc<Mesh>,
    chord: f64,
    /// Its thinnest width across its facet directions (the sweep's step is a quarter of it).
    thickness: f64,
}

/// Contact meshes by the body's geometry ([`sketcraft_doc::ModelBody::geometry_key`]), shared
/// by a session and its scratch copies: driving a joint again meshes no body that did not change.
#[derive(Default)]
pub(crate) struct ContactMeshes(HashMap<u64, ContactMesh>);

/// Tessellate `body` for contact checks and measure how thin it is.
fn contact_mesh(body: &sketcraft_doc::ModelBody) -> Result<ContactMesh> {
    let tolerance = (body.body.size() * 1e-3).max(1e-3);
    let mesh = body.body.mesh(&sketcraft_kernel::MeshTolerance::chord(tolerance))?;
    if mesh.triangles.len() > MAX_TRIANGLES || mesh.positions.len().saturating_add(mesh.triangles.len().saturating_mul(4)) > 200_000 {
        return Err(limit("body exceeds contact sweep tessellation budget"));
    }
    let extent = mesh.bounds().size();
    // Whole-body widths along bounded distinct facet normals catch tilted
    // plates without interpreting skinny mesh triangles as thin material.
    // Local thin walls of a wider body remain a sampling limitation.
    let mut thickness = extent.x.min(extent.y).min(extent.z);
    let mut directions: Vec<Vec3> = Vec::new();
    for triangle in &mesh.triangles {
        let Some([a, b, c]) = mesh.tri(triangle) else {
            return Err(limit("invalid contact tessellation"));
        };
        let Some(direction) = (b - a).cross(c - a).normalized() else { continue };
        if directions.len() < 32 && !directions.iter().any(|d| d.dot(direction).abs() > 1.0 - 1e-6) {
            directions.push(direction);
        }
    }
    for direction in directions {
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for point in &mesh.positions {
            let projected = point.dot(direction);
            low = low.min(projected);
            high = high.max(projected);
        }
        thickness = thickness.min(high - low);
    }
    if !thickness.is_finite() || thickness <= 1e-9 {
        return Err(limit("body is too thin for a bounded contact sweep"));
    }
    // Flat faces tessellate exactly; only planes known from how the face was
    // made count (an unknown face is taken as curved).
    let faces = body.body.face_count();
    let flat = faces <= 1024 && (0..faces).all(|i| body.body.face_plane(i).is_some());
    let chord = if flat { 0.0 } else { tolerance };
    Ok(ContactMesh { mesh: Arc::new(mesh), chord, thickness })
}

/// A member of a contact set: (occurrence id, name).
pub(super) type Member = (u64, String);

/// Two set members whose bodies pass into each other: their names, and the bodies (indices in
/// [`ContactGeometry`]) that do.
pub(super) struct Overlap {
    pub(super) a: String,
    pub(super) b: String,
    bodies: (usize, usize),
}

#[derive(Clone)]
pub(super) struct ContactGeometry {
    bodies: Vec<CachedBody>,
    /// Body indices, and the set members (occurrence id, name) they are under.
    pairs: Vec<(usize, usize, Member, Member)>,
    step: f64,
}

impl ContactGeometry {
    pub(super) fn new(s: &Session) -> Result<Self> {
        let assembly = &s.doc.assembly;
        let sets: Vec<Vec<u64>> = if assembly.contact_all {
            vec![s.doc.occurrences.iter().map(|o| o.id).collect()]
        } else {
            assembly.contact_sets.iter().filter(|set| !set.suppressed).map(|set| set.occurrences.clone()).collect()
        };
        Self::of_sets(s, &sets)
    }

    /// The geometry of the bodies under `sets` (occurrence ids), paired as contact sets pair
    /// them: bodies under different members of one set.
    pub(super) fn of_sets(s: &Session, sets: &[Vec<u64>]) -> Result<Self> {
        let mut out = Self { bodies: Vec::new(), pairs: Vec::new(), step: f64::INFINITY };
        if sets.is_empty() {
            return Ok(out);
        }
        let state = s.model.state();
        let mut meshes: HashMap<String, (Arc<Mesh>, f64)> = HashMap::new();
        let mut stack = vec![(0, Vec::<u64>::new())];
        let mut count = 0;
        while let Some((component, path)) = stack.pop() {
            count += 1;
            if count > MAX_PLACEMENTS || path.len() > 64 {
                return Err(limit("assembly exceeds contact sweep placement budget"));
            }
            for occurrence in s.doc.occurrences.iter().filter(|o| o.parent == component) {
                let mut child = path.clone();
                child.push(occurrence.id);
                stack.push((occurrence.component, child));
                if stack.len() > MAX_PLACEMENTS {
                    return Err(limit("assembly exceeds contact sweep placement budget"));
                }
            }
            if !sets.iter().any(|set| path.iter().any(|id| set.contains(id))) {
                continue;
            }
            for body in state.bodies.iter().filter(|b| s.doc.model_body_component(b) == component) {
                if body.body.is_mesh() {
                    return Err(limit("contact sweep does not support mesh bodies; disable their contact set to move them"));
                }
                if out.bodies.len() >= MAX_BODIES {
                    return Err(limit("too many bodies for a contact sweep"));
                }
                let (mesh, chord) = if let Some(cached) = meshes.get(&body.name) {
                    cached.clone()
                } else {
                    let key = body.geometry_key();
                    let known = key.and_then(|k| s.contact_meshes.lock().ok().and_then(|c| c.0.get(&k).cloned()));
                    let made = match known {
                        Some(made) => made,
                        None => {
                            let made = contact_mesh(body)?;
                            if let (Some(k), Ok(mut cache)) = (key, s.contact_meshes.lock()) {
                                if cache.0.len() >= MAX_CACHED_MESHES {
                                    cache.0.clear();
                                }
                                cache.0.insert(k, made.clone());
                            }
                            made
                        }
                    };
                    out.step = out.step.min(made.thickness * 0.25);
                    meshes.insert(body.name.clone(), (made.mesh.clone(), made.chord));
                    (made.mesh, made.chord)
                };
                out.bodies.push(CachedBody {
                    path: path.clone(),
                    offset: s.doc.model_body_offset(body).copied().unwrap_or(IDENTITY),
                    body: body.body.clone(),
                    exact_box: body.body.exact_bounds()?,
                    mesh,
                    size: body.body.size(),
                    chord,
                });
            }
        }
        let mut pairs = BTreeSet::new();
        for set in sets {
            let tagged: Vec<(usize, u64)> =
                out.bodies.iter().enumerate().filter_map(|(i, b)| b.path.iter().rev().find(|id| set.contains(id)).map(|id| (i, *id))).collect();
            for (i, (a, oa)) in tagged.iter().enumerate() {
                for (b, ob) in tagged.iter().skip(i + 1) {
                    if oa != ob {
                        pairs.insert((*a, *b, *oa, *ob));
                    }
                }
            }
        }
        let name = |id| s.doc.occurrences.iter().find(|o| o.id == id).map(|o| o.name.clone()).unwrap_or_default();
        out.pairs = pairs.into_iter().map(|(a, b, oa, ob)| (a, b, (oa, name(oa)), (ob, name(ob)))).collect();
        Ok(out)
    }

    pub(super) fn has_pairs(&self) -> bool {
        !self.pairs.is_empty()
    }

    fn frame(body: &CachedBody, s: &Session) -> Result<Mat> {
        Self::frame_in(body, &s.doc)
    }

    fn frame_in(body: &CachedBody, doc: &Document) -> Result<Mat> {
        let mut frame = IDENTITY;
        for id in &body.path {
            let occurrence = doc.occurrences.iter().find(|o| o.id == *id).ok_or_else(|| limit("contact occurrence disappeared"))?;
            frame = mat_mul(&frame, &occurrence.transform);
        }
        Ok(mat_mul(&frame, &body.offset))
    }

    fn bounds(&self, s: &Session) -> Result<Aabb3> {
        let mut bounds = Aabb3::EMPTY;
        for body in &self.bodies {
            let frame = Self::frame(body, s)?;
            for point in &body.mesh.positions {
                bounds.add(apply_point(&frame, *point));
            }
        }
        Ok(bounds)
    }

    pub(super) fn samples(&self, start: &Session, end: &Session) -> Result<usize> {
        let a = joints::joint_values(&start.doc.assembly);
        let b = joints::joint_values(&end.doc.assembly);
        let bounds = self.bounds(start)?.union(&self.bounds(end)?);
        let state = start.model.state();
        let (mut linear, mut angular, mut radius) = (0.0, 0.0, 0.0_f64);
        for joint in start.doc.assembly.joints.iter().filter(|j| !j.suppressed) {
            for (index, _) in joint.kind.dofs().iter().enumerate() {
                let delta = (a.get(&joint.id).and_then(|q| q.get(index)).copied().unwrap_or(0.0)
                    - b.get(&joint.id).and_then(|q| q.get(index)).copied().unwrap_or(0.0))
                .abs();
                if joint.kind.is_angle(index) {
                    angular += delta;
                    if delta == 0.0 {
                        continue;
                    }
                    for session in [start, end] {
                        for origin in [&joint.a, &joint.b] {
                            let frame = session.doc.origin_world(&state, origin)?;
                            let pivot = apply_point(&frame, Vec3::ZERO);
                            let far = Vec3::new(
                                (bounds.min.x - pivot.x).abs().max((bounds.max.x - pivot.x).abs()),
                                (bounds.min.y - pivot.y).abs().max((bounds.max.y - pivot.y).abs()),
                                (bounds.min.z - pivot.z).abs().max((bounds.max.z - pivot.z).abs()),
                            );
                            radius = radius.max(far.len());
                        }
                    }
                } else {
                    linear += delta;
                }
            }
        }
        let required = ((linear + angular * (radius + linear)) / self.step).max(angular / 1_f64.to_radians()).ceil().max(1.0);
        if !required.is_finite() || required > MAX_SAMPLES as f64 {
            return Err(limit("motion exceeds contact sweep sampling budget; drive the joint in smaller increments"));
        }
        let vertices: usize = self.bodies.iter().map(|body| body.mesh.positions.len()).sum();
        if (required as usize).saturating_mul(vertices.saturating_add(self.pairs.len())) > MAX_WORK {
            return Err(limit("motion exceeds contact sweep transform budget; simplify the contact set or use smaller increments"));
        }
        Ok(required as usize)
    }

    /// The pairs of bodies of set members that pass into each other in `s` (every pair, or the
    /// first found when `all` is false), decided on the exact bodies: their intersection holds
    /// volume. Bodies that only touch, such as a cam and its follower, share none. Only bodies
    /// whose exact boxes meet are intersected; `intersections` counts them, refused past the
    /// drive's budget.
    pub(super) fn overlaps(&self, s: &Session, intersections: &mut usize, all: bool) -> Result<Vec<Overlap>> {
        let frames = self.frames(s)?;
        let boxes: Vec<Aabb3> = self.bodies.iter().zip(&frames).map(|(b, f)| placed_box(&b.exact_box, f)).collect();
        let mut placed: Vec<Option<sketcraft_kernel::Body>> = vec![None; self.bodies.len()];
        let mut out = Vec::new();
        for (a, b, oa, ob) in &self.pairs {
            let (Some(xa), Some(xb)) = (boxes.get(*a), boxes.get(*b)) else {
                return Err(limit("invalid contact geometry pair"));
            };
            if !xa.meets(xb) {
                continue;
            }
            *intersections = intersections.saturating_add(1);
            if *intersections > MAX_INTERSECTIONS {
                return Err(limit("motion exceeds contact sweep intersection budget; simplify the contact set or use smaller increments"));
            }
            let undecided = |e: &dyn std::fmt::Display| {
                bad("contact.sweep", format!("whether {} and {} pass into each other could not be decided ({e})", oa.1, ob.1))
            };
            let (pa, pb) = (self.placed_body(*a, &frames, &mut placed)?, self.placed_body(*b, &frames, &mut placed)?);
            let Some(shared) = sketcraft_kernel::boolean(&pa, &pb, sketcraft_kernel::BoolOp::Intersect).map_err(|e| undecided(&e))? else {
                continue;
            };
            if sketcraft_kernel::measure_mass(&shared).map_err(|e| undecided(&e))?.volume > 0.0 {
                out.push(Overlap { a: oa.1.clone(), b: ob.1.clone(), bodies: (*a, *b) });
                if !all {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// The least gap between the bodies of `pairs` in `s`, where they are clear of each other: their
    /// exact distance ([`sketcraft_kernel::body_distance`]), less the meshes' chord tolerances
    /// where it could only be measured on the meshes, so the gap is never more than the bodies'.
    /// Between bodies that pass into each other it means nothing (a distance does not tell them
    /// from touching ones): it is asked only of placements known to be clear.
    pub(super) fn gap(&self, s: &Session, pairs: &[Overlap]) -> Result<f64> {
        let frames = self.frames(s)?;
        let mut placed: Vec<Option<sketcraft_kernel::Body>> = vec![None; self.bodies.len()];
        let mut least = f64::INFINITY;
        for pair in pairs {
            let (a, b) = pair.bodies;
            let (Some(ca), Some(cb), Some(fa), Some(fb)) = (self.bodies.get(a), self.bodies.get(b), frames.get(a), frames.get(b)) else {
                return Err(limit("invalid contact geometry pair"));
            };
            let (pa, pb) = (self.placed_body(a, &frames, &mut placed)?, self.placed_body(b, &frames, &mut placed)?);
            let mesh = |c: &CachedBody, f: &Mat| Mesh {
                positions: c.mesh.positions.iter().map(|p| apply_point(f, *p)).collect(),
                triangles: c.mesh.triangles.clone(),
                ..Mesh::default()
            };
            let (ma, mb) = (mesh(ca, fa), mesh(cb, fb));
            let (sa, sb) = (sketcraft_kernel::DistanceShape::new(&pa, &ma, ca.chord), sketcraft_kernel::DistanceShape::new(&pb, &mb, cb.chord));
            let Some(d) = sketcraft_kernel::body_distance(&sa, &sb, f64::INFINITY)? else {
                return Err(limit("invalid contact geometry pair"));
            };
            let gap = if d.exact { d.distance } else { d.distance - ca.chord - cb.chord };
            least = least.min(gap.max(0.0));
        }
        Ok(least)
    }

    /// The furthest a point of the bodies of `pairs` moves toward the other body of its pair
    /// from where `from` ([`ContactGeometry::frames`]) placed them to where `to` has them
    /// (bounded by the two bodies' own furthest moves, each by the corners of its exact box,
    /// which a rigid move takes furthest).
    pub(super) fn pair_travel(&self, from: &[Mat], to: &Session, pairs: &[Overlap]) -> Result<f64> {
        let moved = |i: usize| -> Result<f64> {
            let (Some(body), Some(fa)) = (self.bodies.get(i), from.get(i)) else {
                return Err(limit("invalid contact geometry pair"));
            };
            let fb = Self::frame(body, to)?;
            let local = &body.exact_box;
            let mut most = 0.0_f64;
            for c in 0..8 {
                let pick = |bit: usize, lo: f64, hi: f64| if c & bit == 0 { lo } else { hi };
                let corner = Vec3::new(pick(1, local.min.x, local.max.x), pick(2, local.min.y, local.max.y), pick(4, local.min.z, local.max.z));
                most = most.max(apply_point(fa, corner).dist(apply_point(&fb, corner)));
            }
            Ok(most)
        };
        let mut most = 0.0_f64;
        for pair in pairs {
            most = most.max(moved(pair.bodies.0)? + moved(pair.bodies.1)?);
        }
        Ok(most)
    }

    /// Where `s` places each body.
    pub(super) fn frames(&self, s: &Session) -> Result<Vec<Mat>> {
        self.bodies.iter().map(|b| Self::frame(b, s)).collect()
    }

    /// Body `i` placed by `frames`, made once per check.
    fn placed_body(&self, i: usize, frames: &[Mat], placed: &mut [Option<sketcraft_kernel::Body>]) -> Result<sketcraft_kernel::Body> {
        if let Some(Some(done)) = placed.get(i) {
            return Ok(done.clone());
        }
        let (Some(body), Some(frame)) = (self.bodies.get(i), frames.get(i)) else {
            return Err(limit("invalid contact geometry pair"));
        };
        let moved = sketcraft_kernel::transform_matrix(&body.body, *frame)?;
        if let Some(slot) = placed.get_mut(i) {
            *slot = Some(moved.clone());
        }
        Ok(moved)
    }

}

impl ContactGeometry {
    /// The size of the largest body (at least 1 mm): what tolerances scale with.
    pub(super) fn scale(&self) -> f64 {
        self.bodies.iter().map(|b| b.size).fold(1.0, f64::max)
    }

}

/// The box holding `local` placed by `frame` (its corners' box: a rigid motion keeps the box's
/// contents inside it).
fn placed_box(local: &Aabb3, frame: &Mat) -> Aabb3 {
    let mut world = Aabb3::EMPTY;
    for c in 0..8 {
        let pick = |bit: usize, lo: f64, hi: f64| if c & bit == 0 { lo } else { hi };
        world.add(apply_point(
            frame,
            Vec3::new(pick(1, local.min.x, local.max.x), pick(2, local.min.y, local.max.y), pick(4, local.min.z, local.max.z)),
        ));
    }
    world
}

