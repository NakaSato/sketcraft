//! Arcs, circles and polygons, as SketchUp keeps them: drawn as straight segments that stay one
//! curve, so the curve can be picked, measured and edited whole, and saved as what it is. Soft
//! and smooth edges are set here too.

use serde::{Deserialize, Serialize};
use sketcore_record::Vec3;
use sketcraft_geom::prelude::*;

use crate::model::{CurveId, EdgeId, Mesh, MeshError, finite};

/// The fewest and most segments a curve is drawn with (SketchUp's range).
pub const SEGMENTS: std::ops::RangeInclusive<u32> = 3..=999;

/// What a curve is, in the terms it was drawn with (normals are unit).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveKind {
    /// From `start`, turning `sweep` degrees about `normal` through `center`.
    Arc {
        center: Vec3,
        normal: Vec3,
        start: Vec3,
        sweep: f64,
    },
    Circle {
        center: Vec3,
        normal: Vec3,
        radius: f64,
    },
    /// A regular polygon with a corner at `corner`.
    Polygon {
        center: Vec3,
        normal: Vec3,
        corner: Vec3,
        sides: u32,
    },
}

/// A curve: what it is, and its edges in order along it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub kind: CurveKind,
    pub edges: Vec<EdgeId>,
}

impl Mesh {
    pub fn curve(&self, c: CurveId) -> Option<&Curve> {
        self.curves.get(&c)
    }
    pub fn curves(&self) -> impl Iterator<Item = (CurveId, &Curve)> {
        self.curves.iter().map(|(k, v)| (*k, v))
    }

    /// An arc from `start`, turning `sweep` degrees (either way, up to a full turn) about `normal`
    /// through `center`, in `segments` straight pieces.
    pub fn add_arc(&mut self, center: Vec3, normal: Vec3, start: Vec3, sweep: f64, segments: u32) -> Result<CurveId, MeshError> {
        let (n, radial) = frame(center, normal, start)?;
        if !sweep.is_finite() || sweep.abs() < 1e-6 || sweep.abs() > 360.0 {
            return Err(MeshError::BadPoint);
        }
        let points = around(center, n, radial, sweep, segments)?;
        self.draw_curve(CurveKind::Arc { center, normal: n, start, sweep }, &points)
    }

    /// A circle of `radius` about `normal` through `center`, in `segments` straight pieces.
    pub fn add_circle(&mut self, center: Vec3, normal: Vec3, radius: f64, segments: u32) -> Result<CurveId, MeshError> {
        let n = normal.normalized().ok_or(MeshError::BadPoint)?;
        if !radius.is_finite() || radius <= crate::TOLERANCE || radius >= 1e9 {
            return Err(MeshError::BadPoint);
        }
        let start = center + n.any_perp() * radius;
        let (n, radial) = frame(center, n, start)?;
        let mut points = around(center, n, radial, 360.0, segments)?;
        points.pop(); // the last point is the first again
        self.draw_curve(CurveKind::Circle { center, normal: n, radius }, &closed(points))
    }

    /// A regular polygon of `sides` about `normal` through `center`, with a corner at `corner`.
    pub fn add_polygon(&mut self, center: Vec3, normal: Vec3, corner: Vec3, sides: u32) -> Result<CurveId, MeshError> {
        let (n, radial) = frame(center, normal, corner)?;
        let mut points = around(center, n, radial, 360.0, sides)?;
        points.pop();
        self.draw_curve(CurveKind::Polygon { center, normal: n, corner, sides }, &closed(points))
    }

    /// Draw the edges through `points` in order and make them curve `kind`.
    fn draw_curve(&mut self, kind: CurveKind, points: &[Vec3]) -> Result<CurveId, MeshError> {
        let id = self.fresh_curve_id()?;
        let mut edges: Vec<EdgeId> = Vec::new();
        for pair in points.windows(2) {
            edges.extend(self.add_edge(pair[0], pair[1])?);
        }
        for e in &edges {
            // An edge drawn over a segment of another curve becomes this one's.
            if let Some(old) = self.edges.get(e).and_then(|x| x.curve)
                && let Some(c) = self.curves.get_mut(&old)
            {
                c.edges.retain(|x| x != e);
                if c.edges.is_empty() {
                    self.curves.remove(&old);
                }
            }
            if let Some(edge) = self.edges.get_mut(e) {
                edge.curve = Some(id);
            }
        }
        self.curves.insert(id, Curve { kind, edges });
        Ok(id)
    }

    /// Soften edge `e` (hidden where it joins two faces) and smooth it (the faces shade as one),
    /// or not.
    pub fn set_soft(&mut self, e: EdgeId, soft: bool, smooth: bool) -> Result<(), MeshError> {
        let edge = self.edges.get_mut(&e).ok_or(MeshError::Missing("edge"))?;
        edge.soft = soft;
        edge.smooth = smooth;
        Ok(())
    }
}

/// The unit normal, and the radial vector from `center` to `start` (square to the normal).
fn frame(center: Vec3, normal: Vec3, start: Vec3) -> Result<(Vec3, Vec3), MeshError> {
    if !finite(center) || !finite(start) {
        return Err(MeshError::BadPoint);
    }
    let n = normal.normalized().ok_or(MeshError::BadPoint)?;
    let radial = start - center;
    let radial = radial - n * radial.dot(n);
    if radial.len() <= crate::TOLERANCE {
        return Err(MeshError::ZeroLength);
    }
    Ok((n, radial))
}

/// `segments + 1` points from `center + radial`, turning `sweep` degrees about `n`.
fn around(center: Vec3, n: Vec3, radial: Vec3, sweep: f64, segments: u32) -> Result<Vec<Vec3>, MeshError> {
    if !SEGMENTS.contains(&segments) {
        return Err(MeshError::BadPoint);
    }
    let side = n.cross(radial);
    Ok((0..=segments)
        .map(|i| {
            let a = (sweep * f64::from(i) / f64::from(segments)).to_radians();
            center + radial * a.cos() + side * a.sin()
        })
        .collect())
}

/// The points with the first again at the end, so drawing them closes the loop.
fn closed(mut points: Vec<Vec3>) -> Vec<Vec3> {
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    points
}
