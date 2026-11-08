//! SketCraft's edge/face model, as SketchUp models: loose vertices, edges and faces that glue
//! together. An edge drawn onto another splits it where they meet, and a face appears when edges
//! close a flat loop. There are no solids and no history here; undo and commands live above.
//!
//! The lookups the tools make often are kept up to date as the model changes: the edges at a
//! vertex, the faces on an edge, the vertices near a point.
//! Lengths are millimetres. See `docs/edge-face-rewrite-plan.md`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod curves;
mod faces;
mod index;
mod model;
mod plane;
mod push_pull;
mod transform;

pub use curves::{Curve, CurveKind};
pub use model::{CurveId, Edge, EdgeId, Face, FaceId, Mesh, MeshError, Vertex, VertexId};
pub use sketcore_record::Vec3;

/// How close two points must be to be the same vertex, and a point to an edge to be on it, in mm.
/// SketchUp welds within about a thousandth of an inch; this is a little tighter.
pub const TOLERANCE: f64 = 1e-3;

/// The most vertices, edges or faces one model holds, so hostile input can't exhaust memory.
pub const MAX_ENTITIES: usize = 20_000_000;

#[cfg(test)]
mod tests;
