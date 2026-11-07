use eframe::{egui, egui_wgpu, wgpu};
use egui::{Color32, Rect};
use ketchup_core::{Camera, CameraView, DVec2, DVec3, Face, Model};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

#[path = "renderer_mesh.rs"]
mod mesh;

/// Shared reference geometry fade, measured along the camera's forward axis.
pub(crate) struct ReferenceFade {
    eye: DVec3,
    forward: DVec3,
    pub origin_depth: f64,
}

impl ReferenceFade {
    pub fn new(camera: &Camera) -> Self {
        let eye = camera.eye();
        let forward = (camera.target - eye).normalize();
        let depth = (-eye).dot(forward);
        Self {
            eye,
            forward,
            origin_depth: if depth > 0.005 {
                depth
            } else {
                camera.distance
            },
        }
    }

    pub fn depth(&self, point: DVec3) -> f64 {
        (point - self.eye).dot(self.forward)
    }

    pub fn opacity(depth: f64, origin_depth: f64) -> f32 {
        (origin_depth / depth).clamp(0., 1.) as f32
    }

    pub fn axis_length(&self) -> f64 {
        // End below one alpha step, even when the origin is far off screen.
        self.eye.length() + self.origin_depth * 256.
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [u8; 4],
    lateral: f32,
    shadow_color: [u8; 4],
    dash_phase: f32,
}

// A face slope applies only on the face side of its boundary. Extending
// the plane across the silhouette can pull a rear object's stroke forward.
#[derive(Clone, Copy)]
struct EdgeSurface {
    gradient: DVec2,
    inward: DVec2,
}

#[cfg(test)]
impl EdgeSurface {
    fn unbounded(gradient: DVec2) -> Self {
        Self {
            gradient,
            inward: DVec2::ZERO,
        }
    }
}

// Compare document data because callers can edit Model collections directly.
// Share one snapshot across panes and sidebar summaries, rather than cloning a
// large document separately for each camera.
#[derive(Default)]
pub struct ModelSnapshot {
    model: Option<Model>,
    revision: u64,
}

pub(crate) struct GroupMarker {
    pub id: u64,
    pub point: DVec3,
    pub owners: std::collections::BTreeSet<u64>,
}

#[derive(Default)]
pub(crate) struct GroupMarkerCache {
    revision: Option<u64>,
    markers: Arc<[GroupMarker]>,
}

impl GroupMarkerCache {
    pub fn update(&mut self, model: &Model, revision: u64) -> Arc<[GroupMarker]> {
        if self.revision != Some(revision) {
            self.markers = model.objects.values().filter_map(|object| {
                let mut points = model.selection_vertices(&object.entities).into_iter()
                    .filter_map(|id| model.vertices.get(&id).map(|v| v.position));
                let first = points.next()?;
                let (lo, hi) = points.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p)));
                Some(GroupMarker { id: object.id, point: (lo + hi) * 0.5,
                    owners: model.expand_entities(&std::collections::BTreeSet::from([object.id])) })
            }).collect::<Vec<_>>().into();
            self.revision = Some(revision);
        }
        Arc::clone(&self.markers)
    }
}

impl ModelSnapshot {
    pub fn update(&mut self, model: &Model) -> u64 {
        if self.model.as_ref() != Some(model) {
            self.model = Some(model.clone());
            self.revision = self.revision.wrapping_add(1);
        }
        self.revision
    }
}

#[derive(Clone, PartialEq)]
pub struct SceneKey {
    pub joinery_members: Option<[u64; 2]>,
    pub joint_preview: Option<([u64; 2], ketchup_core::joinery::JointSettings)>,
    pub model_revision: u64,
    pub camera: Camera,
    pub size: DVec2,
    pub pixels_per_point: f32,
    pub display: ketchup_core::DisplaySettings,
    pub dark: bool,
    pub tool: ketchup_core::Tool,
    pub selected: std::collections::BTreeSet<u64>,
    pub hover: Option<u64>,
    pub outliner_hover: std::collections::BTreeSet<u64>,
}

#[derive(Default)]
pub struct SceneCache {
    annotations: Option<(Rect, [usize; 2], Vec<egui::Shape>)>,
    joint_surfaces: JointSurfaceCache,
    occlusion: Occlusion,
    selection_occlusion: Occlusion,
    annotation_occluders: Vec<[DVec3; 3]>,
    edge_surfaces: BTreeMap<(u64, u64), Vec<EdgeSurface>>,
    buffers: BufferPool,
    gpu_buffers: GpuBufferPool,
    retained: Option<Arc<SceneCallback>>,
    pub key: Option<SceneKey>,
    #[cfg(test)]
    pub mesh_builds: usize,
}

impl SceneCache {
    pub fn repaint(&self, key: &SceneKey, painter: &egui::Painter, rect: Rect) -> bool {
        if self.key.as_ref() == Some(key)
            && self
                .annotations
                .as_ref()
                .is_none_or(|(saved, atlas, shapes)| {
                    shapes.is_empty()
                        || (*saved == rect
                            && *atlas == painter.ctx().fonts(|f| f.font_image_size()))
                })
        {
            if let Some(callback) = &self.retained {
                painter.add(egui_wgpu::Callback::new_paint_callback(
                    rect,
                    SharedScene(Arc::clone(callback)),
                ));
                if let Some((_, _, annotations)) = &self.annotations {
                    painter.extend(annotations.iter().cloned());
                }
                return true;
            }
        }
        false
    }

    pub fn remember_annotations(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        shapes: Vec<egui::Shape>,
    ) {
        self.annotations = Some((rect, painter.ctx().fonts(|f| f.font_image_size()), shapes));
    }
}

type BufferPool = Arc<Mutex<Vec<MeshBuffers>>>;
type GpuBufferPool = Arc<Mutex<Vec<GpuBuffers>>>;

struct GpuBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}

struct MeshBuffers {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    triangle_ranges: Vec<std::ops::Range<u32>>,
}

#[derive(Default)]
pub(super) struct JointSurface {
    occluders: Option<Vec<(Vec<SurfaceCandidate>, Vec<SurfaceCandidate>)>>,
    triangles: Vec<JointSurfaceTriangle>,
    #[cfg(test)]
    preparation_queries: usize,
}
struct SurfaceCandidate {
    index: usize,
    bounds: Option<[DVec2; 2]>,
    planes: Option<[(DVec2, DVec2, f64); 4]>,
}

struct JointSurfaceTriangle {
    points: [DVec3; 3],
    origin: DVec3,
    gradient: DVec2,
}

#[derive(Clone, Default)]
struct JointSurfaceCache {
    camera: Camera,
    size: DVec2,
    sections: [Option<f64>; 3],
    view_revision: u64,
    entries: BTreeMap<u64, CachedJointSurface>,
    used: std::collections::BTreeSet<u64>,
}
#[derive(Clone)]
struct CachedJointSurface {
    stock: Vec<[DVec3; 3]>,
    surface: Arc<JointSurface>,
    view_revision: u64,
}

pub struct Mesh {
    joint_guide_bounds: BTreeMap<u64, (DVec2, DVec2)>,
    visible_joint_surfaces: BTreeMap<u64, (Arc<JointSurface>, Arc<JointSurface>, (u64, u64))>,
    joint_surfaces: JointSurfaceCache,
    record_annotation_occluders: bool,
    gpu_edge_occlusion: bool,
    shadow_vertices: Vec<Vertex>,
    clip_stroke_footprints: bool,
    selection_occlusion: Occlusion,
    annotation_occluders: Vec<[DVec3; 3]>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    triangle_ranges: Vec<std::ops::Range<u32>>,
    edge_surfaces: BTreeMap<(u64, u64), Vec<EdgeSurface>>,
    occlusion: Occlusion,
    size: DVec2,
    pixels_per_point: f64,
    parallel: bool,
    view: CameraView,
    display: ketchup_core::DisplaySettings,
    buffer_pool: Option<BufferPool>,
    gpu_pool: GpuBufferPool,
}

pub(crate) struct EdgeDraw {
    pub id: u64,
    pub key: (u64, u64),
    pub points: [DVec3; 2],
    pub width: f64,
    pub color: Color32,
    pub back: bool,
    pub visible: bool,
    pub overlay: bool,
}
pub(crate) struct FaceDraw<'a> {
    pub face: &'a Face,
    pub cached: &'a CachedFace,
    pub color: Color32,
    pub shadow: Option<(&'a crate::shadows::FaceShadow, Color32)>,
}
pub(crate) struct JointSurfaceRequest {
    pub owner: u64,
    pub stock: Arc<[[DVec3; 3]]>,
    pub own_entities: Arc<std::collections::BTreeSet<u64>>,
}

// Test centerlines in f64 before applying screen-space width or rounding to
// GPU depth. A steep incident plane must never resurrect an occluded edge.
// A hierarchy rejects whole groups of faces by position and depth. Queries
// subtract hidden spans immediately and stop once the edge is fully hidden.
#[derive(Default)]
struct Occlusion {
    entity: Option<u64>,
    outline_order: bool,
    // Workers share the completed hierarchy. Mutations occur only while
    // collecting the next mask, through copy-on-write ownership.
    triangles: Arc<Vec<OccludingTriangle>>,
    indices: Arc<Vec<usize>>,
    nodes: Arc<Vec<OcclusionNode>>,
    pending: Vec<usize>,
    spans: Vec<(f64, f64)>,
    insertion_cursor: Option<usize>,
    line_cursor: usize,
    lines: Vec<CachedVisibility>,
    revision: u64,
    needs_refit: bool,
    tree_quality: f64,
    #[cfg(test)]
    query_work: (usize, usize),
    #[cfg(test)]
    hierarchy_builds: usize,
    #[cfg(test)]
    polygon_queries: usize,
}

struct CachedVisibility {
    gpu_guard: Option<f64>,
    a: DVec3,
    b: DVec3,
    revision: u64,
    spans: Vec<(f64, f64)>,
}

#[derive(Clone)]
struct OccludingTriangle {
    entity: Option<u64>,
    points: [DVec3; 3],
    gradient: DVec2,
    bounds: OcclusionBounds,
}

#[derive(Clone, Copy)]
struct OcclusionBounds {
    lo: DVec3,
    hi: DVec3,
}

struct ScreenSegment {
    a: DVec3,
    lo: DVec3,
    hi: DVec3,
    inverse: DVec2,
}
impl ScreenSegment {
    fn new(a: DVec3, b: DVec3) -> Self {
        let delta = b - a;
        Self {
            a,
            lo: a.min(b),
            hi: a.max(b),
            inverse: DVec2::new(
                if delta.x == 0. { 0. } else { delta.x.recip() },
                if delta.y == 0. { 0. } else { delta.y.recip() },
            ),
        }
    }
}
impl OcclusionBounds {
    fn overlaps(self, segment: &ScreenSegment) -> bool {
        if self.lo.z >= segment.hi.z
            || self.lo.x > segment.hi.x
            || self.hi.x < segment.lo.x
            || self.lo.y > segment.hi.y
            || self.hi.y < segment.lo.y
        {
            return false;
        }
        // Slab intersections share the reciprocals across every hierarchy node
        // and triangle tested for this edge. No per-node divisions or closures.
        let mut enter = 0_f64;
        let mut exit = 1_f64;
        for axis in 0..2 {
            if segment.inverse[axis] == 0. {
                continue;
            }
            let a = (self.lo[axis] - segment.a[axis]) * segment.inverse[axis];
            let b = (self.hi[axis] - segment.a[axis]) * segment.inverse[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
        enter < exit
    }
}

#[derive(Clone)]
struct OcclusionNode {
    bounds: OcclusionBounds,
    start: usize,
    end: usize,
    children: Option<(usize, usize)>,
}

impl OccludingTriangle {
    fn surface_planes(&self, origin: DVec3, gradient: DVec2) -> [(DVec2, DVec2, f64); 4] {
        let [a, b, c] = self.points.map(DVec3::truncate);
        let winding = (b - a).perp_dot(c - a).signum();
        [
            (
                gradient - self.gradient,
                a,
                origin.z + gradient.dot(a - origin.truncate())
                    - self.points[0].z
                    - 64. * f64::EPSILON,
            ),
            (DVec2::new(-(b - a).y, (b - a).x) * winding, a, 0.),
            (DVec2::new(-(c - b).y, (c - b).x) * winding, b, 0.),
            (DVec2::new(-(a - c).y, (a - c).x) * winding, c, 0.),
        ]
    }

    fn hidden_interval(&self, a: DVec3, b: DVec3) -> Option<(f64, f64)> {
        let mut interval = (0., 1.);
        let delta = b - a;
        let [p, q, r] = self.points.map(|p| p.truncate());
        let winding = (q - p).perp_dot(r - p).signum();
        for (p, q) in [(p, q), (q, r), (r, p)] {
            let side = |v: DVec3| winding * (q - p).perp_dot(v.truncate() - p);
            if !clip_positive(&mut interval, side(a), side(b)) {
                return None;
            }
        }
        let origin = self.points[0];
        let difference = a.z - origin.z - self.gradient.dot((a - origin).truncate());
        let change = delta.z - self.gradient.dot(delta.truncate());
        // Preserve coplanar boundaries; this tolerance covers f64 arithmetic,
        // rather than allowing a whole f32 depth step through the foreground.
        let tolerance = 64. * f64::EPSILON;
        if !clip_positive(
            &mut interval,
            difference - tolerance,
            difference + change - tolerance,
        ) {
            return None;
        }
        Some(interval)
    }
}

fn clip_positive(interval: &mut (f64, f64), a: f64, b: f64) -> bool {
    if a < 0. && b < 0. {
        return false;
    }
    if a < 0. {
        interval.0 = interval.0.max(a / (a - b));
    } else if b < 0. {
        interval.1 = interval.1.min(a / (a - b));
    }
    interval.0 < interval.1
}

impl Occlusion {
    #[cfg(not(target_arch = "wasm32"))]
    fn prepare_queries(&mut self) {
        self.finish_collection();
        if self.nodes.is_empty() && !self.indices.is_empty() {
            self.build(0, self.indices.len());
            self.tree_quality = self.quality();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn query_copy(&self) -> Self {
        Self {
            entity: self.entity,
            outline_order: self.outline_order,
            triangles: self.triangles.clone(),
            indices: self.indices.clone(),
            nodes: self.nodes.clone(),
            revision: self.revision,
            tree_quality: self.tree_quality,
            ..Default::default()
        }
    }

    fn begin_frame(&mut self) {
        self.insertion_cursor = Some(0);
        self.line_cursor = 0;
    }

    fn insert(&mut self, points: [DVec3; 3]) {
        let index = self.insertion_cursor.unwrap_or(self.triangles.len());
        if self
            .triangles
            .get(index)
            .is_some_and(|t| t.points == points && t.entity == self.entity)
        {
            if let Some(cursor) = &mut self.insertion_cursor {
                *cursor += 1;
            }
            return;
        }
        let Some(gradient) = depth_gradient(points) else {
            return;
        };
        let bounds = OcclusionBounds {
            lo: points.iter().copied().reduce(DVec3::min).unwrap(),
            hi: points.iter().copied().reduce(DVec3::max).unwrap(),
        };
        let triangle = OccludingTriangle {
            entity: self.entity,
            points,
            gradient,
            bounds,
        };
        if index < self.triangles.len() {
            Arc::make_mut(&mut self.triangles)[index] = triangle;
            self.needs_refit = true;
        } else {
            Arc::make_mut(&mut self.indices).push(index);
            Arc::make_mut(&mut self.triangles).push(triangle);
            Arc::make_mut(&mut self.nodes).clear();
        }
        if let Some(cursor) = &mut self.insertion_cursor {
            *cursor += 1;
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn finish_collection(&mut self) {
        if let Some(len) = self.insertion_cursor {
            if len < self.triangles.len() {
                Arc::make_mut(&mut self.triangles).truncate(len);
                Arc::make_mut(&mut self.indices).clear();
                Arc::make_mut(&mut self.indices).extend(0..len);
                Arc::make_mut(&mut self.nodes).clear();
                self.revision = self.revision.wrapping_add(1);
            }
        }
        if self.needs_refit && !self.nodes.is_empty() {
            for index in (0..self.nodes.len()).rev() {
                let node = &self.nodes[index];
                let bounds = if let Some((left, right)) = node.children {
                    let a = self.nodes[left].bounds;
                    let b = self.nodes[right].bounds;
                    OcclusionBounds {
                        lo: a.lo.min(b.lo),
                        hi: a.hi.max(b.hi),
                    }
                } else {
                    self.bounds(node.start, node.end)
                };
                Arc::make_mut(&mut self.nodes)[index].bounds = bounds;
            }
            // Refit is linear during navigation. Repartition only when the
            // old grouping becomes substantially less useful at the new view.
            if self.quality() > self.tree_quality * 2. {
                Arc::make_mut(&mut self.nodes).clear();
            }
        }
        self.needs_refit = false;
    }

    fn quality(&self) -> f64 {
        let area = |bounds: OcclusionBounds| {
            let extent = bounds.hi - bounds.lo;
            extent.x * extent.y
        };
        self.nodes.iter().map(|n| area(n.bounds)).sum::<f64>()
            / area(self.nodes[0].bounds).max(1e-12)
    }

    fn bounds(&self, start: usize, end: usize) -> OcclusionBounds {
        self.indices[start..end].iter().fold(
            OcclusionBounds {
                lo: DVec3::splat(f64::INFINITY),
                hi: DVec3::splat(f64::NEG_INFINITY),
            },
            |bounds, &index| {
                let triangle = &self.triangles[index];
                OcclusionBounds {
                    lo: bounds.lo.min(triangle.bounds.lo),
                    hi: bounds.hi.max(triangle.bounds.hi),
                }
            },
        )
    }

    // Outline masks contain thousands of tiny triangles. Sort their screen
    // centers once, then construct bounds bottom-up, avoiding repeated bounds
    // scans and median partitioning at every hierarchy level.
    fn build_outline_order(&mut self) -> usize {
        fn spread(value: u32) -> u64 {
            let mut x = u64::from(value);
            x = (x | (x << 8)) & 0x00ff_00ff;
            x = (x | (x << 4)) & 0x0f0f_0f0f;
            x = (x | (x << 2)) & 0x3333_3333;
            (x | (x << 1)) & 0x5555_5555
        }
        let bounds = self.bounds(0, self.indices.len());
        let extent = (bounds.hi - bounds.lo).max(DVec3::splat(1e-12));
        let mut keys: Vec<_> = self.indices.iter().map(|&index| {
            let triangle = self.triangles[index].bounds;
            let p = (((triangle.lo + triangle.hi) * 0.5 - bounds.lo) / extent)
                .clamp(DVec3::ZERO, DVec3::ONE);
            let x = (p.x * 65535.) as u32;
            let y = (p.y * 65535.) as u32;
            let z = (p.z * 65535.) as u64;
            (((spread(x) | (spread(y) << 1)) << 16) | z, index)
        }).collect();
        keys.sort_unstable();
        let indices = Arc::make_mut(&mut self.indices);
        indices.clear();
        indices.extend(keys.into_iter().map(|(_, index)| index));
        Arc::make_mut(&mut self.nodes).reserve(self.indices.len().div_ceil(4));
        self.build_ordered(0, self.indices.len())
    }

    fn build_ordered(&mut self, start: usize, end: usize) -> usize {
        let index = self.nodes.len();
        Arc::make_mut(&mut self.nodes).push(OcclusionNode {
            bounds: OcclusionBounds { lo: DVec3::ZERO, hi: DVec3::ZERO }, start, end, children: None,
        });
        let (bounds, children) = if end - start > 8 {
            let middle = start + (end - start) / 2;
            let left = self.build_ordered(start, middle);
            let right = self.build_ordered(middle, end);
            let a = self.nodes[left].bounds;
            let b = self.nodes[right].bounds;
            (OcclusionBounds { lo: a.lo.min(b.lo), hi: a.hi.max(b.hi) }, Some((left, right)))
        } else { (self.bounds(start, end), None) };
        let node = &mut Arc::make_mut(&mut self.nodes)[index];
        node.bounds = bounds;
        node.children = children;
        index
    }

    fn build(&mut self, start: usize, end: usize) -> usize {
        if self.outline_order && start == 0 && end == self.indices.len() {
            return self.build_outline_order();
        }
        let bounds = self.bounds(start, end);
        let index = self.nodes.len();
        Arc::make_mut(&mut self.nodes).push(OcclusionNode {
            bounds,
            start,
            end,
            children: None,
        });
        if end - start > 8 {
            let extent = bounds.hi - bounds.lo;
            let (center_lo, center_hi) = self.indices[start..end].iter().fold(
                (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                |(lo, hi), &index| {
                    let b = self.triangles[index].bounds;
                    let center = (b.lo + b.hi) * 0.5;
                    (lo.min(center), hi.max(center))
                },
            );
            let spread = center_hi - center_lo;
            // Stacked, overlapping faces have almost the same screen bounds.
            // Split those by depth so nearer layers are visited together.
            let axis =
                if spread.x <= extent.x * 0.25 && spread.y <= extent.y * 0.25 && spread.z > 0. {
                    2
                } else {
                    usize::from(spread.y > spread.x)
                };
            let middle = start + (end - start) / 2;
            Arc::make_mut(&mut self.indices)[start..end].select_nth_unstable_by(middle - start, |&a, &b| {
                let a = self.triangles[a].bounds;
                let b = self.triangles[b].bounds;
                (a.lo[axis] + a.hi[axis]).total_cmp(&(b.lo[axis] + b.hi[axis]))
            });
            let left = self.build(start, middle);
            let right = self.build(middle, end);
            Arc::make_mut(&mut self.nodes)[index].children = Some((left, right));
        }
        index
    }

    fn visible_intervals(&mut self, a: DVec3, b: DVec3) -> Vec<(f64, f64)> {
        self.visible_intervals_with_guard(a, b, None)
    }
    fn visible_intervals_with_guard(
        &mut self,
        a: DVec3,
        b: DVec3,
        gpu_guard: Option<f64>,
    ) -> Vec<(f64, f64)> {
        self.finish_collection();
        let index = self.line_cursor;
        self.line_cursor += 1;
        let mut visible = std::mem::take(&mut self.spans);
        visible.clear();
        if let Some(cached) = self.lines.get(index) {
            if cached.a == a
                && cached.b == b
                && cached.revision == self.revision
                && cached.gpu_guard == gpu_guard
            {
                visible.extend_from_slice(&cached.spans);
                #[cfg(test)]
                {
                    self.query_work = (0, 0);
                }
                return visible;
            }
        }
        self.compute_intervals(a, b, &mut visible, gpu_guard);
        if index == self.lines.len() {
            self.lines.push(CachedVisibility {
                gpu_guard,
                a,
                b,
                revision: self.revision,
                spans: visible.clone(),
            });
        } else {
            let cached = &mut self.lines[index];
            cached.gpu_guard = gpu_guard;
            cached.a = a;
            cached.b = b;
            cached.revision = self.revision;
            cached.spans.clear();
            cached.spans.extend_from_slice(&visible);
        }
        visible
    }

    #[cfg(test)]
    fn visible_polygons(&mut self, polygon: Vec<DVec2>, depth: f64) -> Vec<Vec<DVec2>> {
        self.visible_surface_polygons(polygon, DVec3::new(0., 0., depth), DVec2::ZERO)
    }

    fn visible_surface_polygons(
        &mut self,
        polygon: Vec<DVec2>,
        origin: DVec3,
        gradient: DVec2,
    ) -> Vec<Vec<DVec2>> {
        let candidates = self.surface_candidates(&polygon, origin, gradient);
        self.visible_surface_candidates(polygon, origin, gradient, &candidates)
    }

    fn surface_candidates(
        &mut self,
        polygon: &[DVec2],
        origin: DVec3,
        gradient: DVec2,
    ) -> Vec<SurfaceCandidate> {
        self.finish_collection();
        let mut candidates = Vec::new();
        if self.triangles.is_empty() || polygon.len() < 3 {
            return candidates;
        }
        if self.nodes.is_empty() {
            self.build(0, self.indices.len());
            self.tree_quality = self.quality();
        }
        let depth_at = |p: DVec2| origin.z + gradient.dot(p - origin.truncate());
        let lo = polygon
            .iter()
            .map(|&p| p.extend(depth_at(p)))
            .reduce(DVec3::min)
            .unwrap();
        let hi = polygon
            .iter()
            .map(|&p| p.extend(depth_at(p)))
            .reduce(DVec3::max)
            .unwrap();
        let overlaps = |bounds: OcclusionBounds| {
            bounds.lo.z < hi.z
                && bounds.lo.x < hi.x
                && bounds.hi.x > lo.x
                && bounds.lo.y < hi.y
                && bounds.hi.y > lo.y
        };
        self.pending.clear();
        self.pending.push(0);
        while let Some(index) = self.pending.pop() {
            let node = &self.nodes[index];
            if !overlaps(node.bounds) {
                continue;
            }
            if let Some((left, right)) = node.children {
                self.pending.extend([left, right]);
                continue;
            }
            for &index in &self.indices[node.start..node.end] {
                let triangle = &self.triangles[index];
                if !overlaps(triangle.bounds) {
                    continue;
                }
                candidates.push(SurfaceCandidate {
                    index,
                    bounds: None,
                    planes: None,
                });
            }
        }
        candidates
    }

    // The depth buffer clips ordinary foreground geometry. Retain f64 CPU
    // clipping only where f32 depth rounding could place a foreground face or
    // outline in the guide's depth layer. The conservative guard covers both
    // the guide's one-step bias and raster interpolation error.
    fn precision_candidates(
        &mut self,
        polygon: &[DVec2],
        origin: DVec3,
        gradient: DVec2,
        gpu_depth: bool,
        own_entities: &std::collections::BTreeSet<u64>,
    ) -> Vec<SurfaceCandidate> {
        let mut candidates = self.surface_candidates(polygon, origin, gradient);
        let mut hidden = Vec::with_capacity(8);
        let mut scratch = Vec::with_capacity(8);
        candidates.retain_mut(|candidate| {
            let triangle = &self.triangles[candidate.index];
            if triangle.entity.is_some_and(|id| own_entities.contains(&id)) {
                return false;
            }
            if gpu_depth
                && triangle.points.iter().all(|p| {
                    origin.z + gradient.dot(p.truncate() - origin.truncate()) - p.z
                        > 4. * f64::from(f32::EPSILON)
                })
            {
                return false;
            }
            let planes = triangle.surface_planes(origin, gradient);
            if planes.iter().any(|&(normal, origin, offset)| {
                polygon
                    .iter()
                    .all(|&p| normal.dot(p - origin) + offset <= 0.)
            }) {
                return false;
            }
            hidden.clear();
            hidden.extend_from_slice(polygon);
            for &(normal, origin, offset) in &planes {
                clip_polygon_into(&hidden, |p| normal.dot(p - origin) + offset, &mut scratch);
                std::mem::swap(&mut hidden, &mut scratch);
                if hidden.len() < 3 {
                    break;
                }
            }
            if gpu_depth {
                let (normal, origin, offset) = planes[0];
                clip_polygon_into(
                    &hidden,
                    |p| 4. * f64::from(f32::EPSILON) - normal.dot(p - origin) - offset,
                    &mut scratch,
                );
                std::mem::swap(&mut hidden, &mut scratch);
            }
            if !has_surface_area(&hidden) {
                return false;
            }
            candidate.bounds = Some([
                hidden.iter().copied().reduce(DVec2::min).unwrap(),
                hidden.iter().copied().reduce(DVec2::max).unwrap(),
            ]);
            candidate.planes = Some(planes);
            true
        });
        candidates
    }

    fn visible_surface_candidates(
        &mut self,
        polygon: Vec<DVec2>,
        origin: DVec3,
        gradient: DVec2,
        candidates: &[SurfaceCandidate],
    ) -> Vec<Vec<DVec2>> {
        #[cfg(test)]
        {
            self.polygon_queries += 1;
        }
        if candidates.is_empty() {
            return vec![polygon];
        }
        let bounds = [
            polygon.iter().copied().reduce(DVec2::min).unwrap(),
            polygon.iter().copied().reduce(DVec2::max).unwrap(),
        ];
        let mut visible = vec![polygon];
        let mut outside = Vec::new();
        for candidate in candidates {
            if candidate.bounds.is_some_and(|[lo, hi]| {
                hi.x < bounds[0].x || hi.y < bounds[0].y || lo.x > bounds[1].x || lo.y > bounds[1].y
            }) {
                continue;
            }
            let planes = candidate.planes.unwrap_or_else(|| self.triangles[candidate.index].surface_planes(origin, gradient));
            outside.clear();
            for polygon in visible.drain(..) {
                if planes.iter().any(|&(normal, origin, offset)| {
                    polygon
                        .iter()
                        .all(|&p| normal.dot(p - origin) + offset <= 0.)
                }) {
                    outside.push(polygon);
                    continue;
                }
                let first_piece = outside.len();
                let mut hidden = polygon.clone();
                for &(normal, origin, offset) in &planes {
                    let side = |p: DVec2| normal.dot(p - origin) + offset;
                    let (inside, piece) = split_screen_polygon(&hidden, side);
                    if !piece.is_empty() { outside.push(piece); }
                    hidden = inside;
                    if hidden.is_empty() { break; }
                }
                if hidden.is_empty() {
                    // Tangencies and misses must preserve the original convex
                    // piece, rather than fragmenting it along irrelevant planes.
                    outside.truncate(first_piece);
                    outside.push(polygon);
                }
            }
            std::mem::swap(&mut visible, &mut outside);
            if visible.is_empty() {
                break;
            }
        }
        visible
    }

    fn compute_intervals(
        &mut self,
        a: DVec3,
        b: DVec3,
        visible: &mut Vec<(f64, f64)>,
        gpu_guard: Option<f64>,
    ) {
        #[cfg(test)]
        {
            self.query_work = (0, 0);
        }
        // Mesh returns this buffer after emitting the stroke, so ordinary
        // edges reuse both the traversal stack and interval allocation.
        visible.push((0., 1.));
        if self.triangles.is_empty() {
            return;
        }
        // Build once after collecting faces, not once per edge. Late preview
        // or section faces invalidate the hierarchy when they are appended.
        if self.nodes.is_empty() {
            self.build(0, self.indices.len());
            self.tree_quality = self.quality();
            #[cfg(test)]
            {
                self.hierarchy_builds += 1;
            }
        }
        let segment = ScreenSegment::new(a, b);
        self.pending.clear();
        self.pending.push(0);
        while let Some(index) = self.pending.pop() {
            #[cfg(test)]
            {
                self.query_work.0 += 1;
            }
            let node = &self.nodes[index];
            if !node.bounds.overlaps(&segment)
                || gpu_guard.is_some_and(|guard| node.bounds.hi.z < segment.lo.z - guard)
            {
                continue;
            }
            if let Some((left, right)) = node.children {
                // Visit nearer groups first to finish fully hidden edges early.
                if self.nodes[left].bounds.lo.z <= self.nodes[right].bounds.lo.z {
                    self.pending.extend([right, left]);
                } else {
                    self.pending.extend([left, right]);
                }
                continue;
            }
            for &index in &self.indices[node.start..node.end] {
                let triangle = &self.triangles[index];
                if !triangle.bounds.overlaps(&segment)
                    || gpu_guard.is_some_and(|guard| triangle.bounds.hi.z < segment.lo.z - guard)
                {
                    continue;
                }
                #[cfg(test)]
                {
                    self.query_work.1 += 1;
                }
                let Some((lo, hi)) = triangle.hidden_interval(a, b) else {
                    continue;
                };
                subtract_hidden(visible, lo, hi);
                if visible.is_empty() {
                    return;
                }
            }
        }
        visible.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    }
}

fn has_surface_area(polygon: &[DVec2]) -> bool {
    polygon.len() >= 3
        && polygon.first().is_some_and(|&origin| {
            polygon
                .windows(2)
                .map(|edge| (edge[0] - origin).perp_dot(edge[1] - origin))
                .sum::<f64>()
                .abs()
                > 1e-12
        })
}

fn annotation_stroke_footprint(a: DVec2, b: DVec2, radius: f64) -> Vec<DVec2> {
    let along = (b - a).normalize() * radius;
    let normal = DVec2::new(-along.y, along.x);
    // Annotation mesh clipping expects a positive polygon winding.
    vec![a - along - normal, b + along - normal, b + along + normal, a - along + normal]
}

fn clip_polygon(polygon: &[DVec2], side: impl Fn(DVec2) -> f64) -> Vec<DVec2> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    clip_polygon_into(polygon, side, &mut clipped);
    clipped
}

/// Both halves share side tests and intersections. Visibility subtraction
/// formerly clipped once to test coverage and twice more to split each plane.
fn split_screen_polygon(polygon: &[DVec2], side: impl Fn(DVec2) -> f64) -> (Vec<DVec2>, Vec<DVec2>) {
    if polygon.len() < 3 { return (Vec::new(), Vec::new()); }
    let mut inside = Vec::with_capacity(polygon.len() + 1);
    let mut outside = Vec::with_capacity(polygon.len() + 1);
    let first_distance = side(polygon[0]);
    let mut da = first_distance;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let db = if i + 1 == polygon.len() { first_distance } else { side(b) };
        if da >= 0. { inside.push(a); }
        if da <= 0. { outside.push(a); }
        let enters = (da >= 0.) != (db >= 0.);
        let leaves = (da <= 0.) != (db <= 0.);
        if enters || leaves {
            let point = a.lerp(b, da / (da - db));
            if enters { inside.push(point); }
            if leaves { outside.push(point); }
        }
        da = db;
    }
    if !has_surface_area(&inside) { inside.clear(); }
    if !has_surface_area(&outside) { outside.clear(); }
    (inside, outside)
}

fn clip_polygon_into(polygon: &[DVec2], side: impl Fn(DVec2) -> f64, clipped: &mut Vec<DVec2>) {
    clipped.clear();
    if polygon.len() < 3 {
        return;
    }
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let (da, db) = (side(a), side(b));
        if da >= 0. {
            clipped.push(a);
        }
        if (da >= 0.) != (db >= 0.) {
            clipped.push(a.lerp(b, da / (da - db)));
        }
    }
    // Half-plane subtraction can leave coincident vertices along a shared
    // triangle boundary. A zero-area region must not become a clipping mask:
    // its zero-length sides would otherwise admit the entire source mesh.
    if clipped.len() < 3 {
        clipped.clear();
        return;
    }
    let origin = clipped[0];
    let area = (1..clipped.len() - 1)
        .map(|i| (clipped[i] - origin).perp_dot(clipped[i + 1] - origin))
        .sum::<f64>();
    if area.abs() <= 1e-12 {
        clipped.clear();
        return;
    }
}

fn clipped_annotation_shapes(
    painter: &egui::Painter,
    viewport: Rect,
    shape: egui::Shape,
    regions: &[Vec<DVec2>],
) -> Vec<egui::Shape> {
    match shape {
        egui::Shape::Vec(shapes) => shapes
            .into_iter()
            .flat_map(|shape| clipped_annotation_shapes(painter, viewport, shape, regions))
            .collect(),
        egui::Shape::Text(mut text) => {
            // Retain texel UVs in the galley until egui's final tessellation:
            // other UI text can still grow the font atlas later in this frame.
            use egui::emath::GuiRounding as _;
            let pos = if painter
                .ctx()
                .tessellation_options(|o| o.round_text_to_pixels)
            {
                text.pos.round_to_pixels(painter.ctx().pixels_per_point())
            } else {
                text.pos
            };
            let galley = Arc::make_mut(&mut text.galley);
            galley.num_vertices = 0;
            galley.num_indices = 0;
            for placed in &mut galley.rows {
                let origin = pos - viewport.min + placed.pos.to_vec2();
                let row = Arc::make_mut(&mut placed.row);
                row.visuals.mesh = clipped_annotation_mesh(&row.visuals.mesh, regions, origin);
                row.visuals.mesh_bounds = row.visuals.mesh.calc_bounds();
                row.visuals.glyph_vertex_range = 0..row.visuals.mesh.vertices.len();
                row.visuals.glyph_index_start = 0;
                galley.num_vertices += row.visuals.mesh.vertices.len();
                galley.num_indices += row.visuals.mesh.indices.len();
            }
            vec![egui::Shape::Text(text)]
        }
        shape => painter
            .ctx()
            .tessellate(
                vec![egui::epaint::ClippedShape::new(painter.clip_rect(), shape)],
                painter.ctx().pixels_per_point(),
            )
            .into_iter()
            .filter_map(|primitive| {
                let egui::epaint::Primitive::Mesh(source) = primitive.primitive else {
                    return None;
                };
                Some(egui::Shape::mesh(clipped_annotation_mesh(
                    &source,
                    regions,
                    -viewport.min.to_vec2(),
                )))
            })
            .collect(),
    }
}

fn clipped_annotation_mesh(
    source: &egui::Mesh,
    regions: &[Vec<DVec2>],
    origin: egui::Vec2,
) -> egui::Mesh {
    let mut clipped = egui::Mesh::with_texture(source.texture_id);
    for region in regions {
        for triangle in source.indices.chunks_exact(3) {
            let mut vertices: Vec<_> = triangle
                .iter()
                .map(|&i| source.vertices[i as usize])
                .collect();
            for i in 0..region.len() {
                let a = region[i];
                let b = region[(i + 1) % region.len()];
                vertices = clip_annotation_vertices(&vertices, |v| {
                    let p = v.pos.to_vec2() + origin;
                    (b - a).perp_dot(DVec2::new(f64::from(p.x), f64::from(p.y)) - a)
                });
            }
            let base = clipped.vertices.len() as u32;
            for i in 1..vertices.len().saturating_sub(1) {
                clipped
                    .indices
                    .extend([base, base + i as u32, base + i as u32 + 1]);
            }
            clipped.vertices.extend(vertices);
        }
    }
    clipped
}

fn clip_annotation_vertices(
    vertices: &[egui::epaint::Vertex],
    side: impl Fn(&egui::epaint::Vertex) -> f64,
) -> Vec<egui::epaint::Vertex> {
    let mut clipped = vec![];
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let (da, db) = (side(&a), side(&b));
        if da >= 0. {
            clipped.push(a);
        }
        if (da >= 0.) != (db >= 0.) {
            let t = (da / (da - db)) as f32;
            let [r, g, b_color, alpha] = std::array::from_fn(|i| {
                egui::lerp(f32::from(a.color[i])..=f32::from(b.color[i]), t).round() as u8
            });
            clipped.push(egui::epaint::Vertex {
                pos: a.pos.lerp(b.pos, t),
                uv: a.uv.lerp(b.uv, t),
                color: Color32::from_rgba_premultiplied(r, g, b_color, alpha),
            });
        }
    }
    clipped
}

fn subtract_hidden(visible: &mut Vec<(f64, f64)>, lo: f64, hi: f64) {
    let mut index = 0;
    while index < visible.len() {
        let (start, end) = visible[index];
        if hi <= start || lo >= end {
            index += 1;
            continue;
        }
        if lo <= start && hi >= end {
            visible.swap_remove(index);
            continue;
        }
        if lo > start && hi < end {
            visible.push((hi, end));
            visible[index].1 = lo;
        } else if lo <= start {
            visible[index].0 = hi;
        } else {
            visible[index].1 = lo;
        }
        index += 1;
    }
}

fn depth_gradient([a, b, c]: [DVec3; 3]) -> Option<DVec2> {
    let u = b - a;
    let v = c - a;
    let determinant = u.truncate().perp_dot(v.truncate());
    if determinant.abs() < 1e-10 {
        return None;
    }
    let gradient = DVec2::new(u.z * v.y - v.z * u.y, u.x * v.z - v.x * u.z) / determinant;
    gradient.is_finite().then_some(gradient)
}

fn surface_depth_offset(offset: DVec2, surfaces: &[EdgeSurface]) -> f64 {
    surfaces
        .iter()
        .filter(|surface| surface.inward.dot(offset) >= -1e-12)
        .map(|surface| surface.gradient.dot(offset))
        .reduce(f64::min)
        .unwrap_or(0.)
}

fn stroke_depth(depth: f64, offset: DVec2, surfaces: &[EdgeSurface]) -> f32 {
    // Match the nearest incident surface across the strip, without moving
    // unrelated faces or giving free edges a slope-dependent allowance.
    let offset = surface_depth_offset(offset, surfaces);
    // One representable step covers rounding between independently projected
    // triangles and strokes without a world-distance or slope-dependent bias.
    ((depth + offset).clamp(0., 1.) as f32).next_down().max(0.)
}

struct Resources {
    face_pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    edge_depth_pipeline: wgpu::RenderPipeline,
    mask_base: wgpu::RenderPipeline,
    mask_shadow: wgpu::RenderPipeline,
    shadow_faces: wgpu::RenderPipeline,
}
pub fn install(cc: &eframe::CreationContext<'_>) -> bool {
    let Some(state) = cc.wgpu_render_state.as_ref() else {
        return false;
    };
    let device = &state.device;
    install_resources(device, state.target_format, &mut state.renderer.write());
    true
}

pub(crate) fn install_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    renderer: &mut egui_wgpu::Renderer,
) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("KetchUp edges"),
        source: wgpu::ShaderSource::Wgsl(include_str!("edges.wgsl").into()),
    });
    let face_pipeline = create_pipeline(device, &shader, format, PipelinePass::Faces);
    let edge_pipeline = create_pipeline(device, &shader, format, PipelinePass::Edges);
    let edge_depth_pipeline = create_pipeline(device, &shader, format, PipelinePass::EdgeDepth);
    let mask_base = create_pipeline_with_samples(
        device,
        &shader,
        wgpu::TextureFormat::Rgba8Unorm,
        PipelinePass::MaskBase,
        4,
    );
    let mask_shadow = create_pipeline_with_samples(
        device,
        &shader,
        wgpu::TextureFormat::Rgba8Unorm,
        PipelinePass::MaskShadow,
        4,
    );
    let shadow_faces = create_pipeline(device, &shader, format, PipelinePass::ShadowFaces);
    renderer.callback_resources.insert(Resources {
        face_pipeline,
        edge_pipeline,
        edge_depth_pipeline,
        mask_base,
        mask_shadow,
        shadow_faces,
    });
}
#[derive(Clone, Copy, PartialEq)]
enum PipelinePass {
    Faces,
    EdgeDepth,
    Edges,
    MaskBase,
    MaskShadow,
    ShadowFaces,
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    pass: PipelinePass,
) -> wgpu::RenderPipeline {
    create_pipeline_with_samples(device, shader, format, pass, 1)
}

fn create_pipeline_with_samples(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    pass: PipelinePass,
    samples: u32,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(match pass {
            PipelinePass::Faces => "KetchUp filled faces",
            PipelinePass::EdgeDepth => "KetchUp opaque edge depth",
            PipelinePass::Edges => "KetchUp antialiased edges",
            PipelinePass::MaskBase => "KetchUp shadow receiver depth",
            PipelinePass::MaskShadow => "KetchUp shadow coverage",
            PipelinePass::ShadowFaces => "KetchUp faces with antialiased shadows",
        }),
        layout: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(if format.is_srgb() {
                "vertex_main_srgb"
            } else {
                "vertex_main"
            }),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Unorm8x4,2=>Float32,3=>Unorm8x4,4=>Float32],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(match pass {
                PipelinePass::EdgeDepth => "fragment_edge_depth",
                PipelinePass::MaskBase => "fragment_mask_base",
                PipelinePass::MaskShadow => "fragment_mask_shadow",
                PipelinePass::ShadowFaces => "fragment_shadow_faces",
                _ => "fragment_main",
            }),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: if pass == PipelinePass::EdgeDepth {
                    wgpu::ColorWrites::empty()
                } else {
                    wgpu::ColorWrites::ALL
                },
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            // Only opaque stroke cores contribute depth; fringes blend later.
            depth_write_enabled: Some(pass != PipelinePass::Edges),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

struct AntialiasedScene {
    bind_group: wgpu::BindGroup,
}

struct SceneCallback {
    shadow_range: std::ops::Range<u32>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    triangle_ranges: Vec<std::ops::Range<u32>>,
    buffer_pool: Option<BufferPool>,
    gpu_pool: GpuBufferPool,
    gpu_buffers: OnceLock<GpuBuffers>,
    antialiasing: bool,
    target_size: [u32; 2],
    antialiased: OnceLock<AntialiasedScene>,
}

struct SharedScene(Arc<SceneCallback>);

impl egui_wgpu::CallbackTrait for SharedScene {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        self.0.prepare(device, queue, screen, encoder, resources)
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        self.0.paint(info, pass, resources);
    }
}

impl Drop for SceneCallback {
    fn drop(&mut self) {
        if let Some(buffers) = self.gpu_buffers.take() {
            if let Ok(mut pool) = self.gpu_pool.lock() {
                if pool.len() < 3 {
                    pool.push(buffers);
                }
            }
        }
        let Some(pool) = &self.buffer_pool else {
            return;
        };
        if let Ok(mut buffers) = pool.lock() {
            // egui may retain callbacks for overlapping frames. Bound the
            // pool while allowing their allocations to rotate between frames.
            if buffers.len() < 3 {
                buffers.push(MeshBuffers {
                    vertices: std::mem::take(&mut self.vertices),
                    indices: std::mem::take(&mut self.indices),
                    triangle_ranges: std::mem::take(&mut self.triangle_ranges),
                });
            }
        }
    }
}
impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get::<Resources>() else {
            return vec![];
        };
        // Every callback owns its buffers until egui releases it. All prepare
        // calls precede all paint calls, so a shared buffer would render only
        // the last camera in every pane. Pool allocations between frames.
        self.gpu_buffers.get_or_init(|| {
            let vertex_bytes = (self.vertices.len() * std::mem::size_of::<Vertex>()) as u64;
            let index_bytes = (self.indices.len() * std::mem::size_of::<u32>()) as u64;
            let cached = self.gpu_pool.lock().ok().and_then(|mut pool| pool.pop());
            let create = |size: u64, usage, label| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: size.max(4096).next_power_of_two(),
                    usage,
                    mapped_at_creation: false,
                })
            };
            let (vertices, indices) = if let Some(cached) = cached {
                (Some(cached.vertices), Some(cached.indices))
            } else {
                (None, None)
            };
            let buffers = GpuBuffers {
                vertices: vertices
                    .filter(|buffer| buffer.size() >= vertex_bytes)
                    .unwrap_or_else(|| {
                        create(
                            vertex_bytes,
                            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            "KetchUp vertices",
                        )
                    }),
                indices: indices
                    .filter(|buffer| buffer.size() >= index_bytes)
                    .unwrap_or_else(|| {
                        create(
                            index_bytes,
                            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                            "KetchUp indices",
                        )
                    }),
            };
            if !self.vertices.is_empty() {
                queue.write_buffer(&buffers.vertices, 0, bytemuck::cast_slice(&self.vertices));
            }
            if !self.indices.is_empty() {
                queue.write_buffer(&buffers.indices, 0, bytemuck::cast_slice(&self.indices));
            }
            buffers
        });
        if self.antialiasing && !self.shadow_range.is_empty() {
            // Render once per retained scene. Each pane owns its resolve texture,
            // so preparing another camera cannot overwrite this pane's image.
            self.antialiased.get_or_init(|| {
                let limit = device.limits().max_texture_dimension_2d;
                let [width, height] = self.target_size.map(|v| v.clamp(1, limit));
                let size = wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                };
                let texture = |format, sample_count, usage| {
                    device
                        .create_texture(&wgpu::TextureDescriptor {
                            label: Some("KetchUp antialiased shadow mask"),
                            size,
                            mip_level_count: 1,
                            sample_count,
                            dimension: wgpu::TextureDimension::D2,
                            format,
                            usage,
                            view_formats: &[],
                        })
                        .create_view(&Default::default())
                };
                let color = texture(
                    wgpu::TextureFormat::Rgba8Unorm,
                    4,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                );
                let resolved = texture(
                    wgpu::TextureFormat::Rgba8Unorm,
                    1,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                );
                let depth = texture(
                    wgpu::TextureFormat::Depth32Float,
                    4,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                );
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("KetchUp 4x antialiasing"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &color,
                            depth_slice: None,
                            resolve_target: Some(&resolved),
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Discard,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.),
                                store: wgpu::StoreOp::Discard,
                            }),
                            stencil_ops: None,
                        }),
                        ..Default::default()
                    });
                    let buffers = self.gpu_buffers.get().unwrap();
                    pass.set_vertex_buffer(0, buffers.vertices.slice(..));
                    pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.set_pipeline(&r.mask_base);
                    for range in &self.triangle_ranges {
                        pass.draw_indexed(range.clone(), 0, 0..1);
                    }
                    pass.set_pipeline(&r.mask_shadow);
                    pass.draw_indexed(self.shadow_range.clone(), 0, 0..1);
                }
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("KetchUp resolved viewport"),
                    layout: &r.shadow_faces.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&resolved),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                AntialiasedScene { bind_group }
            });
        }
        vec![]
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<Resources>() else {
            return;
        };
        if self.vertices.is_empty() {
            return;
        }
        if let Some(scene) = self.antialiased.get() {
            pass.set_bind_group(0, &scene.bind_group, &[]);
            self.draw(
                pass,
                [&r.shadow_faces, &r.edge_depth_pipeline, &r.edge_pipeline],
            );
        } else {
            self.draw(
                pass,
                [&r.face_pipeline, &r.edge_depth_pipeline, &r.edge_pipeline],
            );
        }
    }
}

impl SceneCallback {
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, pipelines: [&wgpu::RenderPipeline; 3]) {
        let Some(buffers) = self.gpu_buffers.get() else {
            return;
        };
        pass.set_vertex_buffer(0, buffers.vertices.slice(..));
        pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
        // Populate depth with faces first, then test edges against that stable
        // surface depth. Grid/overlay depths keep their existing semantics.
        pass.set_pipeline(pipelines[0]);
        for range in &self.triangle_ranges {
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
        // Resolve edge-to-edge occlusion before blending any colors. Otherwise
        // later strokes (including wide selection strokes) overwrite nearer
        // edges because the antialiased color pass cannot safely write depth.
        for pipeline in [pipelines[1], pipelines[2]] {
            pass.set_pipeline(pipeline);
            let mut start = 0;
            for range in &self.triangle_ranges {
                if start < range.start {
                    pass.draw_indexed(start..range.start, 0, 0..1);
                }
                start = range.end;
            }
            if start < self.shadow_range.start {
                pass.draw_indexed(start..self.shadow_range.start, 0, 0..1);
            }
        }
    }
}

/// Keep triangulation independent of camera, selection, hover, and materials.
/// Compare source geometry as Model's collections also allow direct edits.
#[derive(Default)]
pub struct FaceCache {
    faces: BTreeMap<u64, CachedFace>,
    source_vertices: Vec<(u64, DVec3)>,
    adjacent_faces: BTreeMap<(u64, u64), Vec<u64>>,
    revision: u64,
    #[cfg(test)]
    topology_builds: usize,
}
pub struct CachedFace {
    boundary: Face,
    positions: Vec<DVec3>,
    pub normal: DVec3,
    pub center: DVec3,
    pub triangles: Vec<[DVec3; 3]>,
    pub area: f64,
    pub bounds: (DVec3, DVec3),
    plane: Option<ketchup_core::faces::Plane>,
    rings: Vec<Vec<DVec2>>,
}
impl FaceCache {
    pub fn update(&mut self, model: &Model) {
        // Hover and orbit do not change topology. Validate in collection
        // order rather than looking up every face endpoint in the model.
        let same_topology = self.faces.len() == model.faces.len()
            && self
                .faces
                .values()
                .zip(model.faces.values())
                .all(|(cached, face)| cached.boundary == *face);
        if same_topology
            && self
                .source_vertices
                .iter()
                .copied()
                .eq(model.vertices.iter().map(|(&id, v)| (id, v.position)))
        {
            return;
        }
        if !same_topology {
            self.adjacent_faces.clear();
            for face in model.faces.values() {
                for ring in std::iter::once(&face.vertices).chain(face.holes.iter()) {
                    for i in 0..ring.len() {
                        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                        self.adjacent_faces
                            .entry((a.min(b), a.max(b)))
                            .or_default()
                            .push(face.id);
                    }
                }
            }
            #[cfg(test)]
            {
                self.topology_builds += 1;
            }
        }
        let previous_len = self.faces.len();
        self.faces.retain(|id, _| model.faces.contains_key(id));
        if previous_len != self.faces.len() {
            self.revision = self.revision.wrapping_add(1);
        }
        for face in model.faces.values() {
            let positions = || {
                face.vertices
                    .iter()
                    .chain(face.holes.iter().flatten())
                    .map(|id| model.vertices[id].position)
            };
            if self.faces.get(&face.id).is_some_and(|cached| {
                cached.boundary == *face && cached.positions.iter().copied().eq(positions())
            }) {
                continue;
            }
            let plane = ketchup_core::faces::plane(model, &face.vertices);
            let triangles = model.face_triangles(face);
            let area = triangles
                .iter()
                .map(|[a, b, c]| (*b - *a).cross(*c - *a).length() * 0.5)
                .sum();
            let points: Vec<_> = positions().collect();
            let center = points
                .iter()
                .take(face.vertices.len())
                .copied()
                .sum::<DVec3>()
                / face.vertices.len() as f64;
            let bounds = points.iter().fold(
                (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                |(lo, hi), p| (lo.min(*p), hi.max(*p)),
            );
            let rings = plane
                .map(|plane| {
                    std::iter::once(&face.vertices)
                        .chain(face.holes.iter())
                        .map(|ring| {
                            ring.iter()
                                .map(|id| plane.project(model.vertices[id].position))
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.revision = self.revision.wrapping_add(1);
            self.faces.insert(
                face.id,
                CachedFace {
                    boundary: face.clone(),
                    positions: points,
                    normal: plane.map_or(DVec3::Z, |plane| plane.normal),
                    center,
                    triangles,
                    area,
                    bounds,
                    plane,
                    rings,
                },
            );
        }
        self.source_vertices.clear();
        self.source_vertices
            .extend(model.vertices.iter().map(|(&id, v)| (id, v.position)));
    }
    pub fn get(&self, id: u64) -> &CachedFace {
        &self.faces[&id]
    }
    pub fn adjacent_faces(&self, a: u64, b: u64) -> &[u64] {
        self.adjacent_faces
            .get(&(a.min(b), a.max(b)))
            .map_or(&[], Vec::as_slice)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn iter(&self) -> impl Iterator<Item = (u64, &CachedFace)> {
        self.faces.iter().map(|(id, face)| (*id, face))
    }
    pub fn context_bounds(&self, model: &Model, entities: &EntityCache) -> Option<(DVec3, DVec3)> {
        model.edit_context?;
        let active = |id| {
            let state = entities.get(id);
            state.context.is_some() && !state.hidden
        };
        let points = model
            .edges
            .values()
            .filter(|e| active(e.id))
            .flat_map(|edge| {
                let (a, b) = model.points(edge);
                [a, b]
            })
            .chain(model.faces.values().filter(|f| active(f.id)).flat_map(|f| {
                let (lo, hi) = self.get(f.id).bounds;
                [lo, hi]
            }))
            .chain(
                model
                    .dimensions
                    .values()
                    .filter(|d| active(d.id))
                    .flat_map(|d| d.layout(model)),
            );
        let mut points = points.chain(
            model
                .texts
                .values()
                .filter(|t| active(t.id))
                .flat_map(|t| t.layout(model)),
        );
        let first = points.next()?;
        Some(points.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }
}

impl CachedFace {
    pub fn contains_point(&self, point: DVec3) -> bool {
        let Some(plane) = self.plane else { return false };
        if (point - plane.origin).dot(plane.normal).abs() > ketchup_core::document::EPS {
            return false;
        }
        let xy = plane.project(point);
        ketchup_core::faces::in_polygon(xy, &self.rings[0], true)
            && !self.rings.iter().skip(1).any(|ring|
                ketchup_core::faces::in_polygon(xy, ring, true))
    }

    pub fn hit(&self, ray: ketchup_core::Ray) -> Option<(f64, DVec3)> {
        let plane = self.plane?;
        let denominator = ray.direction.dot(plane.normal);
        if denominator.abs() < 1e-9 {
            return None;
        }
        let t = (plane.origin - ray.origin).dot(plane.normal) / denominator;
        if t <= 0.005 {
            return None;
        }
        let point = ray.origin + ray.direction * t;
        let xy = plane.project(point);
        (ketchup_core::faces::in_polygon(xy, &self.rings[0], true)
            && !self
                .rings
                .iter()
                .skip(1)
                .any(|ring| ketchup_core::faces::in_polygon(xy, ring, true)))
        .then_some((t, point))
    }
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub struct EntityState {
    pub hidden: bool,
    pub locked: bool,
    pub context: Option<u64>,
    pub entity_hidden: bool,
    pub object_hidden: bool,
    pub geometry_hidden: bool,
    pub tag_hidden: bool,
    pub tag_color: Option<[u8; 3]>,
}

impl EntityState {
    pub fn displayed(self, d: &ketchup_core::DisplaySettings) -> bool {
        !self.tag_hidden
            && (!self.object_hidden || d.hidden_objects)
            && (!self.geometry_hidden || d.hidden_geometry)
    }
}

/// Moving vertices/instance frames does not change hierarchy or inherited state.
#[derive(Default)]
pub struct EntityCache {
    objects: Vec<(u64, std::collections::BTreeSet<u64>)>,
    hidden: std::collections::BTreeSet<u64>,
    locked: std::collections::BTreeSet<u64>,
    context: Option<u64>,
    states: BTreeMap<u64, EntityState>,
    tags: BTreeMap<u64, ketchup_core::Tag>,
    entity_tags: BTreeMap<u64, u64>,
    revision: u64,
}
impl EntityCache {
    pub fn update(&mut self, model: &Model) {
        if self.context == model.edit_context
            && self.hidden == model.hidden
            && self.locked == model.locked
            && self.tags == model.tags
            && self.entity_tags == model.entity_tags
            && self.objects.len() == model.objects.len()
            && self
                .objects
                .iter()
                .zip(&model.objects)
                .all(|((id, entities), (key, object))| id == key && *entities == object.entities)
        {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.tags.clone_from(&model.tags);
        self.entity_tags.clone_from(&model.entity_tags);
        self.context = model.edit_context;
        self.hidden.clone_from(&model.hidden);
        self.locked.clone_from(&model.locked);
        self.objects = model
            .objects
            .iter()
            .map(|(id, o)| (*id, o.entities.clone()))
            .collect();
        let parents: BTreeMap<_, _> = self
            .objects
            .iter()
            .flat_map(|(id, entities)| entities.iter().map(|child| (*child, *id)))
            .collect();
        self.states.clear();
        for id in parents
            .keys()
            .chain(model.objects.keys())
            .chain(model.entity_tags.keys())
            .chain(model.hidden.iter())
        {
            let mut current = *id;
            let mut hidden = false;
            let mut locked = false;
            let mut object_hidden = false;
            let mut geometry_hidden = false;
            let mut tag_hidden = false;
            let mut tag_color = None;
            let mut context = None;
            loop {
                hidden |= self.hidden.contains(&current);
                object_hidden |=
                    self.hidden.contains(&current) && model.objects.contains_key(&current);
                geometry_hidden |=
                    self.hidden.contains(&current) && !model.objects.contains_key(&current);
                if let Some(tag) = model
                    .entity_tags
                    .get(&current)
                    .and_then(|id| model.tags.get(id))
                {
                    tag_hidden |= !tag.visible;
                    tag_color = tag_color.or(Some(tag.color));
                }
                locked |= self.locked.contains(&current);
                let parent = parents.get(&current).copied();
                if context.is_none() && Some(current) != self.context && parent == self.context {
                    context = Some(current);
                }
                let Some(parent) = parent else {
                    break;
                };
                current = parent;
            }
            // The editing context and its ancestors are never selectable.
            if Some(*id) == self.context {
                context = None;
            }
            self.states.insert(
                *id,
                EntityState {
                    hidden: hidden || tag_hidden,
                    entity_hidden: hidden,
                    object_hidden,
                    geometry_hidden,
                    tag_hidden,
                    tag_color,
                    locked,
                    context,
                },
            );
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn get(&self, id: u64) -> EntityState {
        self.states.get(&id).copied().unwrap_or(EntityState {
            hidden: self.hidden.contains(&id),
            entity_hidden: self.hidden.contains(&id),
            object_hidden: false,
            geometry_hidden: self.hidden.contains(&id),
            tag_hidden: false,
            tag_color: None,
            locked: self.locked.contains(&id),
            context: self.context.is_none().then_some(id),
        })
    }
}

#[cfg(test)]
#[path = "renderer_entity_cache_tests.rs"]
mod entity_cache_tests;

#[cfg(test)]
#[path = "renderer_display_tests.rs"]
mod display_tests;

#[cfg(test)]
#[path = "renderer_reference_axis_tests.rs"]
mod reference_axis_tests;
