//! Flat loops: their plane, their 2D outline in it, and point and loop containment.

use sketcore_record::{Vec2, Vec3};
use sketcraft_geom::prelude::*;

use crate::TOLERANCE;

/// A plane through `origin` with unit axes `u`, `v` and unit `normal` (`u × v`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plane {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub normal: Vec3,
}

impl Plane {
    /// The plane through `origin` across `normal` (unit), with any in-plane axes.
    pub fn across(origin: Vec3, normal: Vec3) -> Plane {
        let u = normal.any_perp();
        Plane { origin, u, v: normal.cross(u), normal }
    }

    pub fn project(&self, p: Vec3) -> Vec2 {
        let d = p - self.origin;
        Vec2::new(d.dot(self.u), d.dot(self.v))
    }

    pub fn distance(&self, p: Vec3) -> f64 {
        (p - self.origin).dot(self.normal)
    }
}

/// Twice the signed area of a 2D loop (positive counter-clockwise).
pub(crate) fn signed_area(points: &[Vec2]) -> f64 {
    let mut a = 0.0;
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        a += p.x * q.y - q.x * p.y;
    }
    a
}

/// Whether `p` is inside the loop (a point on its outline counts as `boundary` says).
pub(crate) fn inside(p: Vec2, ring: &[Vec2], boundary: bool) -> bool {
    let mut odd = false;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        if on_segment(p, *a, b) {
            return boundary;
        }
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                odd = !odd;
            }
        }
    }
    odd
}

fn on_segment(p: Vec2, a: Vec2, b: Vec2) -> bool {
    let ab = b - a;
    let len = ab.len();
    if len <= TOLERANCE {
        return p.dist(a) <= TOLERANCE;
    }
    let t = (p - a).dot(ab) / (len * len);
    (-1e-12..=1.0 + 1e-12).contains(&t) && p.dist(a + ab * t) <= TOLERANCE
}

/// Whether a 2D loop doesn't cross itself (no two edges that aren't neighbours meet).
pub(crate) fn simple(points: &[Vec2]) -> bool {
    let n = points.len();
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (c, d) = (points[j], points[(j + 1) % n]);
            if segments_cross(a, b, c, d) {
                return false;
            }
        }
    }
    true
}

fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let o = |p: Vec2, q: Vec2, r: Vec2| (q - p).cross(r - p);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) && d1 != 0.0 && d2 != 0.0 && d3 != 0.0 && d4 != 0.0
}

/// Whether loop `inner` lies inside loop `outer` (both projected to one plane): every point of
/// `inner` inside or on `outer`, and its middle strictly inside.
pub(crate) fn ring_inside(inner: &[Vec2], outer: &[Vec2]) -> bool {
    if inner.is_empty() || !inner.iter().all(|p| inside(*p, outer, true)) {
        return false;
    }
    // A point just inside the inner loop, next to its first edge, must be inside the outer one.
    let (a, b) = (inner[0], inner[1 % inner.len()]);
    let mid = (a + b) * 0.5;
    let along = b - a;
    let Some(normal) = Vec2::new(-along.y, along.x).normalized() else { return false };
    let side = if signed_area(inner) > 0.0 { 1.0 } else { -1.0 };
    let probe = mid + normal * (side * TOLERANCE * 4.0);
    inside(probe, outer, false)
}
