//! Scene mesh construction: turns the model and display settings into GPU vertex data.
use super::*;

impl Mesh {
    pub(crate) fn prepare_joint_surfaces(&mut self, requests: &[JointSurfaceRequest]) {
        self.paint_overlays(requests, 2, |mesh, request| {
            mesh.cached_joint_surface_for_entities(request.owner, &request.stock, &request.own_entities);
        });
    }
    pub fn new(camera: &Camera, size: DVec2) -> Self {
        Self {
            joint_guide_bounds: BTreeMap::new(),
            visible_joint_surfaces: BTreeMap::new(),
            joint_surfaces: JointSurfaceCache {
                camera: camera.clone(),
                size,
                ..Default::default()
            },
            record_annotation_occluders: true,
            gpu_edge_occlusion: false,
            shadow_vertices: Vec::new(),
            clip_stroke_footprints: false,
            selection_occlusion: Occlusion { outline_order: true, ..Default::default() },
            annotation_occluders: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            triangle_ranges: Vec::new(),
            edge_surfaces: BTreeMap::new(),
            occlusion: Occlusion::default(),
            size,
            pixels_per_point: 1.,
            parallel: camera.projection == sketcraft_core::Projection::Parallel,
            view: camera.view(size),
            display: sketcraft_core::DisplaySettings::default(),
            buffer_pool: None,
            gpu_pool: Default::default(),
        }
    }
    pub fn cached(camera: &Camera, size: DVec2, cache: &mut SceneCache) -> Self {
        cache.key = None;
        #[cfg(test)]
        {
            cache.mesh_builds += 1;
        }
        let mut mesh = Self::new(camera, size);
        mesh.joint_surfaces = std::mem::take(&mut cache.joint_surfaces);
        mesh.joint_surfaces.used.clear();
        if mesh.joint_surfaces.camera != *camera || mesh.joint_surfaces.size != size {
            mesh.joint_surfaces.camera = camera.clone();
            mesh.joint_surfaces.size = size;
            mesh.joint_surfaces.view_revision = mesh.joint_surfaces.view_revision.wrapping_add(1);
        }
        mesh.buffer_pool = Some(Arc::clone(&cache.buffers));
        mesh.gpu_pool = Arc::clone(&cache.gpu_buffers);
        if let Ok(mut buffers) = cache.buffers.lock() {
            if let Some(mut buffers) = buffers.pop() {
                buffers.vertices.clear();
                buffers.indices.clear();
                buffers.triangle_ranges.clear();
                mesh.vertices = buffers.vertices;
                mesh.indices = buffers.indices;
                mesh.triangle_ranges = buffers.triangle_ranges;
            }
        }
        mesh.occlusion = std::mem::take(&mut cache.occlusion);
        mesh.occlusion.begin_frame();
        mesh.selection_occlusion = std::mem::take(&mut cache.selection_occlusion);
        mesh.selection_occlusion.outline_order = true;
        mesh.selection_occlusion.begin_frame();
        mesh.annotation_occluders = std::mem::take(&mut cache.annotation_occluders);
        mesh.annotation_occluders.clear();
        mesh.edge_surfaces = std::mem::take(&mut cache.edge_surfaces);
        for surfaces in mesh.edge_surfaces.values_mut() {
            surfaces.clear();
        }
        mesh
    }
    pub fn enable_gpu_edge_occlusion(&mut self) {
        self.gpu_edge_occlusion = true;
    }
    pub fn set_display(&mut self, display: &sketcraft_core::DisplaySettings) {
        self.display = display.clone();
    }
    pub fn set_annotation_occlusion_enabled(&mut self, enabled: bool) {
        self.record_annotation_occluders = enabled;
        if !enabled {
            self.annotation_occluders.clear();
        }
    }
    pub fn set_entity(&mut self, entity: Option<u64>) {
        self.occlusion.entity = entity;
        self.selection_occlusion.entity = entity;
    }
    pub fn set_pixels_per_point(&mut self, pixels_per_point: f32) {
        self.pixels_per_point = f64::from(pixels_per_point).max(0.01);
    }

    pub(super) fn fog_color(&self, color: Color32, point: DVec3) -> Color32 {
        if !self.display.fog {
            return color;
        }
        let amount = ((self.view.view_distance(point) - self.display.fog_start)
            / (self.display.fog_end - self.display.fog_start))
            .clamp(0., 1.) as f32;
        let bg = self.display.background;
        let [r, g, b, alpha] = color.to_srgba_unmultiplied();
        Color32::from_rgba_unmultiplied(
            (r as f32 * (1. - amount) + bg[0] as f32 * amount) as u8,
            (g as f32 * (1. - amount) + bg[1] as f32 * amount) as u8,
            (b as f32 * (1. - amount) + bg[2] as f32 * amount) as u8,
            alpha,
        )
    }
    pub(super) fn section_line(&self, a: DVec3, b: DVec3) -> Option<(DVec3, DVec3)> {
        self.display.section_segment(a, b)
    }

    pub fn overlay_line(&mut self, a: DVec3, b: DVec3, width: f64, color: Color32, dashed: bool) {
        let Some((a, b)) = self.section_line(a, b) else {
            return;
        };
        let color = self.fog_color(color, (a + b) * 0.5);
        if let Some((a, b)) = self.view.segment(a, b) {
            if dashed {
                let delta = b - a;
                if delta.length_squared() < 0.01 { return; }
                let length = delta.length();
                let normal = DVec2::new(-delta.y, delta.x) / length;
                let fringe = (normal.x.abs() + normal.y.abs()) / self.pixels_per_point;
                let n = normal * (width + fringe) * 0.5;
                // Screen overlays do not supply annotation/depth masks. Keep
                // their existing four-pixel dash/four-pixel gap on one strip.
                self.emit_stroke_geometry(&[a + n, b + n, b - n, a - n], a.extend(0.),
                    DVec2::ZERO, a, n, color, Some(delta / (length * 8.)));
            } else {
                self.line(a.extend(0.), b.extend(0.), width, color);
            }
        }
    }
    /// Share projections between joints on the same board, and between frames
    /// whose camera and stock are unchanged. Screen-space stroke width and
    /// scene occlusion are still resolved against the current frame.
    #[cfg(test)]
    pub(crate) fn cached_joint_surface(
        &mut self,
        owner: u64,
        stock: &[[DVec3; 3]],
    ) -> Arc<JointSurface> {
        self.cached_joint_surface_for_entities(owner, stock, &Default::default())
    }

    pub(crate) fn cached_joint_surface_for_entities(
        &mut self,
        owner: u64,
        stock: &[[DVec3; 3]],
        own_entities: &std::collections::BTreeSet<u64>,
    ) -> Arc<JointSurface> {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("guide_surface_cpu");
        let projected = self.projected_joint_surface(owner, stock);
        self.occlusion.finish_collection();
        self.selection_occlusion.finish_collection();
        let revisions = (self.occlusion.revision, self.selection_occlusion.revision);
        if let Some((source, visible, saved)) = self.visible_joint_surfaces.get(&owner) {
            if Arc::ptr_eq(source, &projected) && revisions == *saved {
                return Arc::clone(visible);
            }
        }
        let mut surface = JointSurface::default();
        let mut occluders = Vec::new();
        for triangle in &projected.triangles {
            let mut polygon = triangle.points.map(DVec3::truncate).to_vec();
            if let Some(&(lo, hi)) = self.joint_guide_bounds.get(&owner) {
                for axis in 0..2 {
                    polygon = clip_polygon(&polygon, |p| p[axis] - lo[axis]);
                    polygon = clip_polygon(&polygon, |p| hi[axis] - p[axis]);
                }
            }
            occluders.push((
                self.occlusion.precision_candidates(
                    &polygon,
                    triangle.origin,
                    triangle.gradient,
                    true,
                    own_entities,
                ),
                self.selection_occlusion.precision_candidates(
                    &polygon,
                    triangle.origin,
                    triangle.gradient,
                    true,
                    own_entities,
                ),
            ));
            surface.triangles.push(JointSurfaceTriangle { ..*triangle });
        }
        surface.occluders = Some(occluders);
        let surface = Arc::new(surface);
        self.visible_joint_surfaces
            .insert(owner, (projected, Arc::clone(&surface), revisions));
        surface
    }

    pub(crate) fn register_joint_guide(&mut self, owner: u64, a: DVec3, b: DVec3) {
        let Some((a, b)) = self.section_line(a, b) else { return };
        let Some((a, b)) = self.view.segment_depth(a, b) else { return };
        // Include the complete antialiased footprint at every endpoint. The
        // union is registered before any board surface is queried or cached.
        let padding = DVec2::splat(1.8 + 2. / self.pixels_per_point);
        let lo = a.truncate().min(b.truncate()) - padding;
        let hi = a.truncate().max(b.truncate()) + padding;
        self.joint_guide_bounds.entry(owner).and_modify(|bounds| {
            bounds.0 = bounds.0.min(lo); bounds.1 = bounds.1.max(hi);
        }).or_insert((lo, hi));
    }

    pub(super) fn projected_joint_surface(&mut self, owner: u64, stock: &[[DVec3; 3]]) -> Arc<JointSurface> {
        let sections = self.display.section_limits();
        if self.joint_surfaces.sections != sections {
            self.joint_surfaces.view_revision = self.joint_surfaces.view_revision.wrapping_add(1);
            self.joint_surfaces.sections = sections;
        }
        self.joint_surfaces.used.insert(owner);
        let revision = self.joint_surfaces.view_revision;
        if let Some(entry) = self.joint_surfaces.entries.get(&owner) {
            if entry.view_revision == revision && entry.stock.as_slice() == stock {
                return Arc::clone(&entry.surface);
            }
        }
        let surface = Arc::new(self.joint_surface(stock.iter().copied()));
        self.joint_surfaces
            .entries
            .entry(owner)
            .and_modify(|entry| {
                if entry.stock.as_slice() != stock {
                    entry.stock.clear();
                    entry.stock.extend_from_slice(stock);
                }
                entry.surface = Arc::clone(&surface);
                entry.view_revision = revision;
            })
            .or_insert_with(|| CachedJointSurface {
                stock: stock.to_vec(),
                surface: Arc::clone(&surface),
                view_revision: revision,
            });
        surface
    }

    /// Resolve the visible stock skin once, shared by every contour on this board.
    pub(crate) fn joint_surface(
        &self,
        triangles: impl IntoIterator<Item = [DVec3; 3]>,
    ) -> JointSurface {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("stock_skin");
        let mut surface = JointSurface::default();
        let mut occlusion = Occlusion::default();
        for triangle in triangles {
            let (polygon, len) = self.display.section_triangle(triangle);
            for i in 1..len.saturating_sub(1) {
                self.view.for_each_triangle_depth(
                    [polygon[0], polygon[i], polygon[i + 1]],
                    |points| {
                        if let Some(gradient) = depth_gradient(points) {
                            occlusion.insert(points);
                            surface.triangles.push(JointSurfaceTriangle {
                                points,
                                origin: points[0],
                                gradient,
                            });
                        }
                    },
                );
            }
        }
        // Hidden stock faces never need to participate in per-edge clipping.
        // The closed board's front skin is independent of which guide is drawn.
        let projected = std::mem::take(&mut surface.triangles);
        for triangle in projected {
            for polygon in occlusion.visible_surface_polygons(
                triangle.points.map(DVec3::truncate).to_vec(),
                triangle.origin,
                triangle.gradient,
            ) {
                // Coincident projected silhouettes can differ by a few f64
                // arithmetic steps, especially with parallel cameras. Discard
                // invisible slivers rather than supporting guides on rear skin.
                let perimeter: f64 = (0..polygon.len())
                    .map(|i| polygon[i].distance(polygon[(i + 1) % polygon.len()]))
                    .sum();
                let area = polygon.first().map_or(0., |&origin| {
                    polygon
                        .windows(2)
                        .map(|edge| (edge[0] - origin).perp_dot(edge[1] - origin))
                        .sum::<f64>()
                        .abs()
                });
                if area <= (perimeter * 1e-7).max(1e-12) {
                    continue;
                }
                let point = |p: DVec2| {
                    p.extend(
                        triangle.origin.z + triangle.gradient.dot(p - triangle.origin.truncate()),
                    )
                };
                for i in 1..polygon.len().saturating_sub(1) {
                    surface.triangles.push(JointSurfaceTriangle {
                        points: [point(polygon[0]), point(polygon[i]), point(polygon[i + 1])],
                        origin: triangle.origin,
                        gradient: triangle.gradient,
                    });
                }
            }
        }
        #[cfg(test)]
        {
            surface.preparation_queries = occlusion.polygon_queries;
        }
        surface
    }

    /// Project internal joint guides onto their owning board's visible skin.
    /// The skin's real depth lets other objects hide the entire stroke footprint.
    pub(crate) fn joint_guide(
        &mut self,
        a: DVec3,
        b: DVec3,
        surface: &JointSurface,
        width: f64,
        color: Color32,
        dashed: bool,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("guide_emit_cpu");
        let Some((a, b)) = self.section_line(a, b) else {
            return;
        };
        // The caller supplies cut-created edges only. Show their complete
        // hidden contours through their own stock, including the lower face.
        // Face coplanarity cannot decide whether an edge belongs to the cut.
        let color = self.fog_color(color, (a + b) * 0.5);
        let Some((a, b)) = self.view.segment_depth(a, b) else {
            return;
        };
        let (a, b) = (a.truncate(), b.truncate());
        let delta = b - a;
        let length_squared = delta.length_squared();
        if length_squared < 0.01 {
            return;
        }
        let normal = DVec2::new(-delta.y, delta.x).normalize();
        let fringe = (normal.x.abs() + normal.y.abs()) / self.pixels_per_point;
        let n = normal * (width + fringe) * 0.5;
        let length = delta.length();
        // Fit whole dashes to the complete segment, including both endpoints.
        // Every corner and short connector remains drawn; triangle clipping
        // must not restart the pattern or leave its final corner in a gap.
        let dash_count = (length / 8.).ceil().clamp(1., 64.) as usize;
        let dash_slots = dash_count as f64 - 0.5;
        let mut dash_start = Vec::with_capacity(8);
        let mut dash_end = Vec::with_capacity(8);
        for (index, triangle) in surface.triangles.iter().enumerate() {
            let [p, q, r] = triangle.points.map(DVec3::truncate);
            let winding = (q - p).perp_dot(r - p).signum();
            let sides = [(p, q), (q, r), (r, p)];
            // Expand by the lateral footprint only to bound dash work. Actual
            // pixels are clipped to the exact triangle below, including holes.
            let mut interval = (0., 1.);
            if !sides.iter().all(|&(p, q)| {
                let side = |point: DVec2| winding * (q - p).perp_dot(point - p);
                let allowance = (q - p).perp_dot(n).abs();
                clip_positive(&mut interval, side(a) + allowance, side(b) + allowance)
            }) {
                continue;
            }
            // Clip the complete guide once. Traversing the visibility
            // hierarchies for every individual dash multiplies orbit cost by
            // up to 64 without changing which parts of the guide are visible.
            let (start, end) = (a.lerp(b, interval.0), a.lerp(b, interval.1));
            let mut polygon = vec![start + n, end + n, end - n, start - n];
            for (p, q) in sides {
                polygon = clip_polygon(&polygon, |point| winding * (q - p).perp_dot(point - p));
                if polygon.len() < 3 {
                    break;
                }
            }
            if polygon.len() < 3 {
                continue;
            }
            let polygons = if let Some(occluders) = &surface.occluders {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = crate::app::viewport_benchmark::scope("stroke_clip");
                let polygons = self.occlusion.visible_surface_candidates(
                    polygon,
                    triangle.origin,
                    triangle.gradient,
                    &occluders[index].0,
                );
                polygons
                    .into_iter()
                    .flat_map(|polygon| {
                        self.selection_occlusion.visible_surface_candidates(
                            polygon,
                            triangle.origin,
                            triangle.gradient,
                            &occluders[index].1,
                        )
                    })
                    .collect()
            } else {
                self.visible_stroke_polygons(polygon, triangle.origin, triangle.gradient)
            };
            for polygon in polygons {
                if !dashed {
                    self.emit_stroke_polygon(
                        &polygon,
                        triangle.origin,
                        triangle.gradient,
                        a,
                        n,
                        color,
                    );
                    continue;
                }
                // One GPU strip carries the dash phase. CPU subdivision is
                // needed only for annotation masks, not for rendered vertices.
                self.emit_stroke_geometry(
                    &polygon,
                    triangle.origin,
                    triangle.gradient,
                    a,
                    n,
                    color,
                    Some(delta * (dash_slots / length_squared)),
                );
                if !self.record_annotation_occluders || color.a() != 255 {
                    continue;
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = crate::app::viewport_benchmark::scope("joint_annotation_masks");
                // Split only the already visible polygons into dashes.
                // Their phase still comes from the complete source edge,
                // so both endpoints and triangle seams stay connected.
                let parameter = |point: DVec2| (point - a).dot(delta) / length_squared;
                let (lo, hi) = polygon.iter().fold((1_f64, 0_f64), |(lo, hi), &point| {
                    let t = parameter(point).clamp(0., 1.);
                    (lo.min(t), hi.max(t))
                });
                for i in (lo * dash_slots).floor() as usize
                    ..((hi * dash_slots).ceil() as usize).min(dash_count)
                {
                    let start = i as f64 / dash_slots;
                    let end = (i as f64 + 0.5) / dash_slots;
                    if start >= hi || end <= lo {
                        continue;
                    }
                    clip_polygon_into(&polygon, |point| parameter(point) - start, &mut dash_start);
                    clip_polygon_into(&dash_start, |point| end - parameter(point), &mut dash_end);
                    if dash_end.len() >= 3 {
                        self.queue_annotation_polygon(
                            &dash_end,
                            triangle.origin,
                            triangle.gradient,
                        );
                    }
                }
            }
        }
    }

    /// Attach depth slopes only to the boundaries of faces that are drawn.
    /// NDC depth is affine in screen coordinates, even in perspective.
    pub fn face_edges(&mut self, face: &Face, cached: &CachedFace) {
        let gradient = cached.triangles.iter().find_map(|&triangle| {
            let mut gradient = None;
            self.view.for_each_triangle_depth(triangle, |projected| {
                gradient = gradient.or_else(|| depth_gradient(projected));
            });
            gradient
        });
        let Some(gradient) = gradient else { return };
        let mut position_offset = 0;
        for (ring_index, ring) in std::iter::once(&face.vertices)
            .chain(face.holes.iter())
            .enumerate()
        {
            let projected: Option<Vec<_>> = cached.positions
                [position_offset..position_offset + ring.len()]
                .iter()
                .map(|&point| self.view.project(point))
                .collect();
            position_offset += ring.len();
            let winding = projected.as_ref().map_or(0., |points| {
                let area: f64 = (0..points.len())
                    .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
                    .sum();
                area.signum() * if ring_index == 0 { 1. } else { -1. }
            });
            for i in 0..ring.len() {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                let inward = projected.as_ref().map_or(DVec2::ZERO, |points| {
                    let direction = points[(i + 1) % points.len()] - points[i];
                    DVec2::new(-direction.y, direction.x).normalize_or_zero() * winding
                });
                self.edge_surfaces
                    .entry((a.min(b), a.max(b)))
                    .or_default()
                    .push(EdgeSurface { gradient, inward });
            }
        }
    }
    #[cfg(test)]
    pub fn styled_edge(
        &mut self,
        edge: &sketcraft_core::Edge,
        model: &Model,
        width: f64,
        color: Color32,
        extensions: bool,
        endpoints: bool,
    ) {
        let (a, b) = model.points(edge);
        let key = (edge.a.min(edge.b), edge.a.max(edge.b));
        let surfaces = self
            .edge_surfaces
            .get_mut(&key)
            .map(std::mem::take)
            .unwrap_or_default();
        self.surface_line(a, b, width, color, extensions, endpoints, &surfaces);
        if let Some(entry) = self.edge_surfaces.get_mut(&key) {
            *entry = surfaces;
        }
    }

    pub(super) fn draw_face(&mut self, face: &FaceDraw<'_>) {
        self.set_entity(Some(face.face.id));
        self.face_edges(face.face, face.cached);
        if let Some((shadow, shaded)) = face.shadow {
            if self.display.shadow_antialiasing {
                self.shadow_receiver(&face.cached.triangles, face.color, shaded);
            } else {
                self.shadow_face(&face.cached.triangles, shadow, face.color, shaded);
            }
        } else {
            for &triangle in &face.cached.triangles {
                self.face_triangle(triangle, face.color, true, true);
            }
        }
    }

    pub(crate) fn draw_faces(&mut self, faces: &[FaceDraw<'_>]) {
        #[cfg(not(target_arch = "wasm32"))]
        if faces.len() >= 64 {
            let workers = crate::viewport_workers::parallelism();
            if workers > 1 {
                let chunks: Vec<_> = faces
                    .chunks(faces.len().div_ceil(workers))
                    .map(|chunk| {
                        let mut mesh = Self::new(&self.joint_surfaces.camera, self.size);
                        mesh.display = self.display.clone();
                        (chunk, mesh)
                    })
                    .collect();
                let profiling = crate::app::viewport_benchmark::profiling();
                let meshes = crate::viewport_workers::map(
                    crate::viewport_workers::Kind::Render,
                    chunks,
                    |(chunk, mut mesh)| {
                        crate::app::viewport_benchmark::profile_worker(profiling, || {
                            for face in chunk {
                                mesh.draw_face(face);
                            }
                            mesh
                        })
                    },
                );
                for (mut mesh, timings) in meshes {
                    crate::app::viewport_benchmark::absorb_worker(timings);
                    let base = self.vertices.len() as u32;
                    let index_base = self.indices.len() as u32;
                    self.vertices.append(&mut mesh.vertices);
                    self.indices.extend(mesh.indices.iter().map(|i| base + i));
                    for range in &mesh.triangle_ranges {
                        let range = range.start + index_base..range.end + index_base;
                        if let Some(previous) = self
                            .triangle_ranges
                            .last_mut()
                            .filter(|previous| previous.end == range.start)
                        {
                            previous.end = range.end;
                        } else {
                            self.triangle_ranges.push(range);
                        }
                    }
                    self.shadow_vertices.append(&mut mesh.shadow_vertices);
                    for triangle in mesh.occlusion.triangles.iter() {
                        self.occlusion.entity = triangle.entity;
                        self.occlusion.insert(triangle.points);
                    }
                    for (key, mut surfaces) in mesh.edge_surfaces {
                        self.edge_surfaces
                            .entry(key)
                            .or_default()
                            .append(&mut surfaces);
                    }
                }
                self.draw_shadow_masks(faces);
                return;
            }
        }
        for face in faces {
            self.draw_face(face);
        }
        self.draw_shadow_masks(faces);
    }

    pub(crate) fn draw_edges(&mut self, edges: &[EdgeDraw], extensions: bool, endpoints: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("edge_batch");
        let surfaces = std::mem::take(&mut self.edge_surfaces);
        self.paint_overlays_with_outlines(edges, 128, self.clip_stroke_footprints, |mesh, edge| {
            mesh.set_entity(Some(edge.id));
            let [a, b] = edge.points;
            if edge.back {
                mesh.overlay_line(a, b, 1., edge.color.gamma_multiply(0.4), true);
            }
            if edge.visible {
                if edge.overlay {
                    mesh.overlay_line(a, b, edge.width, edge.color, false);
                } else {
                    mesh.surface_line(
                        a,
                        b,
                        edge.width,
                        edge.color,
                        extensions,
                        endpoints,
                        surfaces
                            .get(&edge.key)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                    );
                }
            }
        });
        self.edge_surfaces = surfaces;
    }

    /// Prepare independent overlay strokes concurrently, then merge in draw
    /// order. Each worker reads the same completed face/outline masks; only
    /// ordinary edges add outlines, which are merged before the selected pass.
    pub(crate) fn paint_overlays<T: Sync>(
        &mut self,
        items: &[T],
        threshold: usize,
        paint: impl Fn(&mut Mesh, &T) + Sync,
    ) {
        self.paint_overlays_with_outlines(items, threshold, true, paint);
    }

    pub(super) fn paint_overlays_with_outlines<T: Sync>(
        &mut self,
        items: &[T],
        threshold: usize,
        read_outlines: bool,
        paint: impl Fn(&mut Mesh, &T) + Sync,
    ) {
        #[cfg(target_arch = "wasm32")]
        let _ = (threshold, read_outlines);
        #[cfg(not(target_arch = "wasm32"))]
        if items.len() >= threshold {
            let workers = crate::viewport_workers::parallelism();
            if workers > 1 {
                let sections = self.display.section_limits();
                if self.joint_surfaces.sections != sections {
                    self.joint_surfaces.sections = sections;
                    self.joint_surfaces.view_revision =
                        self.joint_surfaces.view_revision.wrapping_add(1);
                }
                self.occlusion.prepare_queries();
                // Ordinary edges only append outlines. Do not finish their
                // collection here: that would truncate last frame's hierarchy
                // to the axes and force a full rebuild after every edge pass.
                if read_outlines {
                    self.selection_occlusion.prepare_queries();
                }
                let inherited_outlines = if read_outlines {
                    self.selection_occlusion.triangles.len()
                } else {
                    0
                };
                let chunks: Vec<_> = items
                    .chunks(items.len().div_ceil(workers))
                    .map(|chunk| (chunk, self.overlay_worker(read_outlines)))
                    .collect();
                let profiling = crate::app::viewport_benchmark::profiling();
                let meshes = crate::viewport_workers::map(
                    crate::viewport_workers::Kind::Render,
                    chunks,
                    |(chunk, mut mesh)| {
                        crate::app::viewport_benchmark::profile_worker(profiling, || {
                            for item in chunk {
                                paint(&mut mesh, item);
                            }
                            mesh
                        })
                    },
                );
                for (mut mesh, timings) in meshes {
                    crate::app::viewport_benchmark::absorb_worker(timings);
                    let base = self.vertices.len() as u32;
                    self.vertices.append(&mut mesh.vertices);
                    self.indices.extend(mesh.indices.iter().map(|i| base + i));
                    self.annotation_occluders
                        .append(&mut mesh.annotation_occluders);
                    self.visible_joint_surfaces
                        .append(&mut mesh.visible_joint_surfaces);
                    for triangle in &mesh.selection_occlusion.triangles[inherited_outlines..] {
                        self.selection_occlusion.entity = triangle.entity;
                        self.selection_occlusion.insert(triangle.points);
                    }
                    for owner in &mesh.joint_surfaces.used {
                        self.joint_surfaces.used.insert(*owner);
                        if let Some(entry) = mesh.joint_surfaces.entries.remove(owner) {
                            self.joint_surfaces.entries.insert(*owner, entry);
                        }
                    }
                }
                return;
            }
        }
        for item in items {
            paint(self, item);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn overlay_worker(&self, read_outlines: bool) -> Self {
        let mut mesh = Self::new(&self.joint_surfaces.camera, self.size);
        mesh.display = self.display.clone();
        mesh.pixels_per_point = self.pixels_per_point;
        mesh.gpu_edge_occlusion = self.gpu_edge_occlusion;
        mesh.record_annotation_occluders = self.record_annotation_occluders;
        mesh.clip_stroke_footprints = self.clip_stroke_footprints;
        mesh.occlusion = self.occlusion.query_copy();
        if read_outlines {
            mesh.selection_occlusion = self.selection_occlusion.query_copy();
        }
        mesh.joint_surfaces = self.joint_surfaces.clone();
        mesh.joint_surfaces.used.clear();
        mesh.joint_guide_bounds = self.joint_guide_bounds.clone();
        mesh
    }
    pub fn styled_line(
        &mut self,
        a: DVec3,
        b: DVec3,
        width: f64,
        color: Color32,
        extensions: bool,
        endpoints: bool,
    ) {
        self.surface_line(a, b, width, color, extensions, endpoints, &[]);
    }

    pub(super) fn surface_line(
        &mut self,
        a: DVec3,
        b: DVec3,
        width: f64,
        color: Color32,
        extensions: bool,
        endpoints: bool,
        surfaces: &[EdgeSurface],
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("model_edges");
        let Some((a, b)) = self.section_line(a, b) else {
            return;
        };
        let color = self.fog_color(color, (a + b) * 0.5);
        if let Some((mut a, mut b)) = self.view.segment_depth(a, b) {
            if extensions {
                let direction = (b.truncate() - a.truncate()).normalize_or_zero() * 5.;
                a -= direction.extend(0.);
                b += direction.extend(0.);
            }
            let gpu_guard = (self.gpu_edge_occlusion && !self.clip_stroke_footprints).then(|| {
                let direction = (b - a).truncate().normalize_or_zero();
                let normal = DVec2::new(-direction.y, direction.x);
                let fringe = (normal.x.abs() + normal.y.abs()) / self.pixels_per_point;
                let offset = normal * (width + fringe) * 0.5;
                surfaces
                    .iter()
                    .map(|surface| surface.gradient.dot(offset).abs())
                    .fold(0_f64, f64::max)
                    + 4. * f64::from(f32::EPSILON)
            });
            let visible = self.occlusion.visible_intervals_with_guard(a, b, gpu_guard);
            for &(start, end) in &visible {
                self.surface_stroke(a.lerp(b, start), a.lerp(b, end), width, color, surfaces);
            }
            if endpoints {
                for (t, p) in [(0., a), (1., b)] {
                    if !visible.iter().any(|&(start, end)| start <= t && t <= end) {
                        continue;
                    }
                    self.surface_stroke(
                        p - DVec3::X * 1.5,
                        p + DVec3::X * 1.5,
                        4.,
                        color,
                        surfaces,
                    );
                }
            }
            self.occlusion.spans = visible;
        }
    }
    pub(super) fn line(&mut self, a: DVec3, b: DVec3, width: f64, color: Color32) {
        self.surface_stroke(a, b, width, color, &[]);
    }
    pub(super) fn surface_stroke(
        &mut self,
        a: DVec3,
        b: DVec3,
        width: f64,
        color: Color32,
        surfaces: &[EdgeSurface],
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("edge_emit");
        let depths = [a.z, b.z];
        let a = a.truncate();
        let b = b.truncate();
        let delta = b - a;
        if delta.length_squared() < 0.01 {
            return;
        }
        let normal = DVec2::new(-delta.y, delta.x).normalize();
        // The projected pixel footprint grows on diagonals. Match the shader's
        // fwidth fringe so it softens the edge without eating into its width.
        let fringe = (normal.x.abs() + normal.y.abs()) / self.pixels_per_point;
        let n = normal * (width + fringe) * 0.5;
        // Bounded faces and shared creases need a center seam. Each half
        // follows only the faces on that side; the axis keeps its true depth.
        let (strip, indices): (&[(usize, f64)], &[u32]) =
            if surfaces.len() > 1 || surfaces.iter().any(|surface| surface.inward != DVec2::ZERO) {
                (
                    &[(0, 1.), (0, 0.), (1, 1.), (1, 0.), (0, -1.), (1, -1.)],
                    &[0, 1, 2, 2, 1, 3, 1, 4, 3, 3, 4, 5],
                )
            } else {
                (&[(0, 1.), (0, -1.), (1, 1.), (1, -1.)], &[0, 1, 2, 2, 1, 3])
            };
        if self.clip_stroke_footprints && depths.iter().all(|&z| z > 0. && z < 1.) {
            // Clipping just the centerline leaves corners of a wide or oblique
            // selection strip protruding across the foreground edge. Clip each
            // triangle's complete footprint in f64 before GPU depth rounding.
            let projected: Vec<_> = strip
                .iter()
                .map(|&(endpoint, lateral)| {
                    let offset = n * lateral;
                    let depth_offset = surface_depth_offset(offset, surfaces);
                    ([a, b][endpoint] + offset).extend(depths[endpoint] + depth_offset)
                })
                .collect();
            // Each side is planar. Clip its quad once rather than querying
            // the same visibility hierarchies for both triangle halves.
            let quads: &[[usize; 4]] = if projected.len() == 6 {
                &[[0, 1, 3, 2], [1, 4, 5, 3]]
            } else { &[[0, 1, 3, 2]] };
            for quad in quads {
                let points = quad.map(|index| projected[index]);
                let Some(gradient) = depth_gradient([points[0], points[1], points[2]]) else {
                    continue;
                };
                let origin = points[0];
                self.clipped_stroke_polygon(
                    points.map(DVec3::truncate).to_vec(),
                    origin,
                    gradient,
                    a,
                    n,
                    color,
                );
            }
            return;
        }
        let base = self.vertices.len() as u32;
        self.indices.extend(indices.iter().map(|&i| base + i));
        // Share ordinary outlines with annotation clipping instead of merging
        // them into the face hierarchy and rebuilding it a second time.
        // Selected strokes and cut-guide dashes have their own later masks.
        if color.a() == 255 && depths.iter().all(|&z| z > 0. && z < 1.) {
            let projected: Vec<_> = strip
                .iter()
                .map(|&(endpoint, lateral)| {
                    let offset = n * lateral;
                    let depth_offset = surface_depth_offset(offset, surfaces);
                    ([a, b][endpoint] + offset).extend(depths[endpoint] + depth_offset)
                })
                .collect();
            for triangle in indices.chunks_exact(3) {
                self.selection_occlusion.insert([
                    projected[triangle[0] as usize],
                    projected[triangle[1] as usize],
                    projected[triangle[2] as usize],
                ]);
                if self.record_annotation_occluders && !self.gpu_edge_occlusion {
                    self.annotation_occluders.push([
                        projected[triangle[0] as usize],
                        projected[triangle[1] as usize],
                        projected[triangle[2] as usize],
                    ]);
                }
            }
        }
        let color = color.to_array();
        for &(endpoint, lateral) in strip {
            let p = [a, b][endpoint] + n * lateral;
            self.vertices.push(Vertex {
                dash_phase: -1.,
                position: [
                    (p.x / self.size.x * 2. - 1.) as f32,
                    (1. - p.y / self.size.y * 2.) as f32,
                    stroke_depth(depths[endpoint], n * lateral, surfaces),
                ],
                color,
                lateral: lateral as f32,
                shadow_color: [0; 4],
            });
        }
    }

    pub(super) fn clipped_stroke_polygon(
        &mut self,
        polygon: Vec<DVec2>,
        origin: DVec3,
        gradient: DVec2,
        a: DVec2,
        n: DVec2,
        color: Color32,
    ) {
        for polygon in self.visible_stroke_polygons_with_depth(polygon, origin, gradient, self.gpu_edge_occlusion) {
            self.emit_stroke_polygon(&polygon, origin, gradient, a, n, color);
        }
    }

    pub(super) fn visible_stroke_polygons(
        &mut self,
        polygon: Vec<DVec2>,
        origin: DVec3,
        gradient: DVec2,
    ) -> Vec<Vec<DVec2>> {
        // Annotation shapes paint in egui's screen layer and cannot defer any
        // occlusion to the viewport depth buffer.
        self.visible_stroke_polygons_with_depth(polygon, origin, gradient, false)
    }

    pub(super) fn visible_stroke_polygons_with_depth(
        &mut self, polygon: Vec<DVec2>, origin: DVec3, gradient: DVec2, gpu_depth: bool,
    ) -> Vec<Vec<DVec2>> {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = crate::app::viewport_benchmark::scope("stroke_clip");
        let polygons = if gpu_depth {
            let candidates = self.occlusion.precision_candidates(
                &polygon, origin, gradient, true, &Default::default());
            self.occlusion.visible_surface_candidates(polygon, origin, gradient, &candidates)
        } else {
            self.occlusion.visible_surface_polygons(polygon, origin, gradient)
        };
        // Ordinary outlines occlude strokes only where their real
        // projected surface is nearer, including across wide strokes.
        polygons
            .into_iter()
            .flat_map(|polygon| {
                if gpu_depth {
                    let candidates = self.selection_occlusion.precision_candidates(
                        &polygon, origin, gradient, true, &Default::default());
                    self.selection_occlusion.visible_surface_candidates(polygon, origin, gradient, &candidates)
                } else {
                    self.selection_occlusion.visible_surface_polygons(polygon, origin, gradient)
                }
            })
            .collect()
    }

    pub(super) fn emit_stroke_polygon(
        &mut self,
        polygon: &[DVec2],
        origin: DVec3,
        gradient: DVec2,
        a: DVec2,
        n: DVec2,
        color: Color32,
    ) {
        self.emit_stroke_geometry(polygon, origin, gradient, a, n, color, None);
        if color.a() == 255 && self.record_annotation_occluders {
            self.queue_annotation_polygon(polygon, origin, gradient);
        }
    }

    pub(super) fn emit_stroke_geometry(
        &mut self,
        polygon: &[DVec2],
        origin: DVec3,
        gradient: DVec2,
        a: DVec2,
        n: DVec2,
        color: Color32,
        dash_axis: Option<DVec2>,
    ) {
        let base = self.vertices.len() as u32;
        for &p in polygon {
            let depth = origin.z + gradient.dot(p - origin.truncate());
            self.vertices.push(Vertex {
                position: [
                    (p.x / self.size.x * 2. - 1.) as f32,
                    (1. - p.y / self.size.y * 2.) as f32,
                    (depth as f32).next_down().max(0.),
                ],
                color: color.to_array(),
                lateral: ((p - a).dot(n) / n.length_squared()) as f32,
                shadow_color: [0; 4],
                dash_phase: dash_axis.map_or(-1., |axis| (p - a).dot(axis).max(0.) as f32),
            });
        }
        for i in 1..polygon.len().saturating_sub(1) {
            self.indices
                .extend([base, base + i as u32, base + i as u32 + 1]);
        }
    }

    pub(super) fn queue_annotation_polygon(&mut self, polygon: &[DVec2], origin: DVec3, gradient: DVec2) {
        for i in 1..polygon.len().saturating_sub(1) {
            self.annotation_occluders.push(
                [polygon[0], polygon[i], polygon[i + 1]]
                    .map(|p| p.extend(origin.z + gradient.dot(p - origin.truncate()))),
            );
        }
    }

    pub fn world_line(&mut self, _camera: &Camera, a: DVec3, b: DVec3, width: f64, color: Color32) {
        self.styled_line(a, b, width, color, false, false);
    }
    /// Reference axes have world depth, but remain visible reference geometry
    /// when model section cuts and fog are enabled.
    pub fn axis_line(&mut self, camera: &Camera, a: DVec3, b: DVec3, width: f64, color: Color32) {
        let fade = ReferenceFade::new(camera);
        let inverse = camera.matrix(self.size).inverse();
        let depths = [fade.depth(a), fade.depth(b)];
        let mut cuts = vec![0., 1.];
        // Match the grid's short depth bands, anchoring full opacity at origin
        // depth. This also gives parallel views a smooth reciprocal-depth fade.
        if (depths[1] - depths[0]).abs() > 1e-12 {
            let mut depth = fade.origin_depth;
            while depth < fade.origin_depth * 257. {
                let t = (depth - depths[0]) / (depths[1] - depths[0]);
                if t > 0. && t < 1. {
                    cuts.push(t);
                }
                depth *= 1.05;
            }
        }
        cuts.sort_by(f64::total_cmp);
        for pair in cuts.windows(2) {
            let Some((start, end)) = self
                .view
                .segment_depth(a.lerp(b, pair[0]), a.lerp(b, pair[1]))
            else {
                continue;
            };
            // Recover depth AFTER viewport/near clipping so opacity stays stable
            // as a vanishing point or the camera crosses a viewport boundary.
            let opacity = [start, end].map(|p| {
                let world = inverse
                    * glam::DVec4::new(
                        p.x / self.size.x * 2. - 1.,
                        1. - p.y / self.size.y * 2.,
                        p.z,
                        1.,
                    );
                ReferenceFade::opacity(fade.depth(world.truncate() / world.w), fade.origin_depth)
            });
            let first = self.vertices.len();
            self.line(start, end, width, color);
            // Ordinary axis strokes have two vertices at each endpoint. Keep
            // their existing depth and antialias footprint; vary only color.
            for (i, vertex) in self.vertices[first..].iter_mut().enumerate() {
                vertex.color = color.gamma_multiply(opacity[i / 2]).to_array();
            }
        }
    }
    /// Keep projected depth after centerline clipping so the complete stroke
    /// can also be clipped against foreground surfaces before painting.
    pub fn occlude_annotations_with_edges(&mut self) {
        for triangle in self.annotation_occluders.drain(..) {
            self.occlusion.insert(triangle);
        }
    }

    pub fn begin_selected_edges(&mut self) {
        // Selection has its own outline mask. Keep the face hierarchy free of
        // edge triangles until annotations actually need them; otherwise every
        // selected stroke clips against the same ordinary edges twice.
        self.clip_stroke_footprints = true;
    }

    pub fn annotation_line(&mut self, a: DVec3, b: DVec3) -> Vec<(DVec3, DVec3)> {
        let Some((a, b)) = self.section_line(a, b) else {
            return vec![];
        };
        let Some((a, b)) = self.view.segment_depth(a, b) else {
            return vec![];
        };
        self.visible_annotation_line(a, b)
    }

    pub fn annotation_tick(&mut self, anchor: DVec3, offset: DVec2) -> Vec<(DVec3, DVec3)> {
        if self.section_line(anchor, anchor).is_none() {
            return vec![];
        }
        let Some(point) = self.view.project_depth(anchor) else {
            return vec![];
        };
        self.visible_annotation_line(point - offset.extend(0.), point + offset.extend(0.))
    }

    pub fn annotation_arrow(&mut self, anchor: DVec3, offset: DVec2) -> Vec<(DVec3, DVec3)> {
        if self.section_line(anchor, anchor).is_none() {
            return vec![];
        }
        let Some(point) = self.view.project_depth(anchor) else {
            return vec![];
        };
        self.visible_annotation_line(point, point + offset.extend(0.))
    }

    pub(super) fn visible_annotation_line(&mut self, a: DVec3, b: DVec3) -> Vec<(DVec3, DVec3)> {
        let mut visible = self.occlusion.visible_intervals(a, b);
        if self.gpu_edge_occlusion {
            let outlines = self.selection_occlusion.visible_intervals(a, b);
            let mut intersections = Vec::new();
            for &(lo, hi) in &visible {
                for &(start, end) in &outlines {
                    let (start, end) = (start.max(lo), end.min(hi));
                    if start < end {
                        intersections.push((start, end));
                    }
                }
            }
            self.selection_occlusion.spans = outlines;
            visible.clear();
            visible.extend(intersections);
        }
        let lines = visible
            .iter()
            .map(|&(start, end)| (a.lerp(b, start), a.lerp(b, end)))
            .collect();
        self.occlusion.spans = visible;
        lines
    }

    /// Clip the tessellated stroke footprint as well as its centerline.
    /// Width, end caps, and the antialiasing fringe must all stay behind faces.
    pub fn annotation_stroke(
        &mut self,
        painter: &egui::Painter,
        viewport: Rect,
        a: DVec3,
        b: DVec3,
        stroke: egui::Stroke,
    ) -> Vec<egui::Shape> {
        let screen = |p: DVec3| viewport.min + egui::vec2(p.x as f32, p.y as f32);
        let segment = [screen(a), screen(b)];
        let shape = egui::Shape::line_segment(segment, stroke);
        let bounds = shape
            .visual_bounding_rect()
            .expand(1. / painter.ctx().pixels_per_point());
        let local = bounds.translate(-viewport.min.to_vec2());
        let [start, end] = segment.map(|p| {
            let p = p - viewport.min;
            DVec2::new(f64::from(p.x), f64::from(p.y))
        });
        let polygon = if start.distance_squared(end) > 1e-12 {
            annotation_stroke_footprint(start, end, f64::from(stroke.width) * 0.5
                + 2. / f64::from(painter.ctx().pixels_per_point()))
        } else {
            [local.left_top(), local.right_top(), local.right_bottom(), local.left_bottom()]
                .map(|p| DVec2::new(f64::from(p.x), f64::from(p.y))).into()
        };
        let delta = (b - a).truncate();
        let gradient = if delta.length_squared() > 1e-12 {
            delta * ((b.z - a.z) / delta.length_squared())
        } else {
            DVec2::ZERO
        };
        let visible = self.visible_annotation_polygons(polygon.clone(), a, gradient);
        if visible.len() == 1 && visible[0] == polygon {
            return vec![shape];
        }
        if visible.is_empty() {
            return vec![];
        }
        clipped_annotation_shapes(painter, viewport, shape, &visible)
    }

    /// Labels are screen-facing billboards at their world anchor's depth.
    /// Clip their entire footprint, including glyphs and background, so a
    /// foreground silhouette can cover only part of a label.
    pub fn annotation_label(
        &mut self,
        painter: &egui::Painter,
        viewport: Rect,
        anchor: DVec3,
        bounds: Rect,
        shape: egui::Shape,
    ) -> Vec<egui::Shape> {
        if self.section_line(anchor, anchor).is_none() {
            return vec![];
        }
        let Some(point) = self.view.project_depth(anchor) else {
            return vec![];
        };
        let local = bounds.translate(-viewport.min.to_vec2());
        let polygon: Vec<_> = [
            local.left_top(),
            local.right_top(),
            local.right_bottom(),
            local.left_bottom(),
        ]
        .map(|p| DVec2::new(f64::from(p.x), f64::from(p.y)))
        .into();
        let visible = self.visible_annotation_polygons(polygon.clone(), point, DVec2::ZERO);
        if visible.len() == 1 && visible[0] == polygon {
            return vec![shape];
        }
        if visible.is_empty() {
            return vec![];
        }
        clipped_annotation_shapes(painter, viewport, shape, &visible)
    }

    pub(super) fn visible_annotation_polygons(
        &mut self,
        polygon: Vec<DVec2>,
        origin: DVec3,
        gradient: DVec2,
    ) -> Vec<Vec<DVec2>> {
        if self.gpu_edge_occlusion {
            self.visible_stroke_polygons(polygon, origin, gradient)
        } else {
            self.occlusion
                .visible_surface_polygons(polygon, origin, gradient)
        }
    }

    pub fn background_grid_line(
        &mut self,
        points: [DVec2; 2],
        opacity: [f32; 2],
        major: bool,
        color: Color32,
    ) {
        let [a, b] = points;
        let normal = DVec2::new(-(b - a).y, (b - a).x).normalize();
        let width = if major { 0.8 } else { 0.55 };
        let fringe = (normal.x.abs() + normal.y.abs()) / self.pixels_per_point;
        let offset = normal * (width + fringe) * 0.5;
        let base = self.vertices.len() as u32;
        for (endpoint, lateral) in [(0, 1.), (0, -1.), (1, 1.), (1, -1.)] {
            let p = [a, b][endpoint] + offset * lateral;
            self.vertices.push(Vertex {
                dash_phase: -1.,
                position: [
                    (p.x / self.size.x * 2. - 1.) as f32,
                    (1. - p.y / self.size.y * 2.) as f32,
                    1.,
                ],
                color: color.gamma_multiply(opacity[endpoint]).to_array(),
                lateral: lateral as f32,
                shadow_color: [0; 4],
            });
        }
        self.indices
            .extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
    }
    pub fn world_triangle(&mut self, _camera: &Camera, triangle: [DVec3; 3], color: Color32) {
        self.face_triangle(triangle, color, true, true);
    }
    pub fn shadow_face(
        &mut self,
        triangles: &[[DVec3; 3]],
        shadow: &crate::shadows::FaceShadow,
        color: Color32,
        shaded: Color32,
    ) {
        // Keep the receiving face's original hard silhouette. Only its shadow
        // coverage is multisampled; ordinary geometry never uses that resolve.
        if self.display.shadow_antialiasing {
            self.shadow_receiver(triangles, color, shaded);
            // Replace receivers in the mask with their nonoverlapping lit and
            // shaded partitions. Testing an overlay against the original face
            // fights its independently rounded depth, especially in isometric
            // exports with many small shadow triangles.
            for (triangle, coverage) in shadow
                .lit
                .iter()
                .map(|&triangle| (triangle, [0, 0, 0, 255]))
                .chain(shadow.shaded.iter().map(|&triangle| (triangle, [255; 4])))
            {
                Self::project_shadow_triangle(&self.view, &self.display, self.size,
                    triangle, coverage, &mut self.shadow_vertices);
            }
        } else {
            for &triangle in triangles {
                self.face_triangle(triangle, color, false, true);
            }
            for &triangle in &shadow.lit {
                self.face_triangle(triangle, color, true, false);
            }
            for &triangle in &shadow.shaded {
                self.face_triangle(triangle, shaded, true, false);
            }
        }
    }
    pub(super) fn shadow_receiver(&mut self, triangles: &[[DVec3; 3]], color: Color32, shaded: Color32) {
        for &triangle in triangles {
            let start = self.vertices.len();
            self.face_triangle(triangle, color, true, true);
            let shadow_color = self
                .fog_color(shaded, triangle.iter().sum::<DVec3>() / 3.)
                .to_array();
            for vertex in &mut self.vertices[start..] {
                vertex.shadow_color = shadow_color;
            }
        }
    }

    pub(super) fn project_shadow_triangle(
        view: &CameraView,
        display: &sketcraft_core::DisplaySettings,
        size: DVec2,
        triangle: [DVec3; 3],
        coverage: [u8; 4],
        vertices: &mut Vec<Vertex>,
    ) {
        let (polygon, len) = display.section_triangle(triangle);
        for i in 1..len.saturating_sub(1) {
            view.for_each_triangle_depth([polygon[0], polygon[i], polygon[i + 1]], |triangle| {
                for p in triangle {
                    vertices.push(Vertex {
                        dash_phase: -1.,
                        position: [
                            (p.x / size.x * 2. - 1.) as f32,
                            (1. - p.y / size.y * 2.) as f32,
                            p.z.clamp(0., 1.) as f32,
                        ],
                        color: coverage,
                        lateral: 0.,
                        shadow_color: [0; 4],
                    });
                }
            });
        }
    }

    pub(super) fn draw_shadow_masks(&mut self, faces: &[FaceDraw<'_>]) {
        if !self.display.shadow_antialiasing {
            return;
        }
        let triangles: Vec<_> = faces
            .iter()
            .filter_map(|face| face.shadow)
            .flat_map(|(shadow, _)| {
                shadow
                    .lit
                    .iter()
                    .map(|triangle| (triangle, [0, 0, 0, 255]))
                    .chain(shadow.shaded.iter().map(|triangle| (triangle, [255; 4])))
            })
            .collect();
        #[cfg(not(target_arch = "wasm32"))]
        if triangles.len() >= 64 && crate::viewport_workers::parallelism() > 1 {
            let chunks = triangles
                .chunks(
                    triangles
                        .len()
                        .div_ceil(crate::viewport_workers::parallelism()),
                )
                .map(|chunk| (chunk, self.joint_surfaces.camera.view(self.size)))
                .collect();
            let display = &self.display;
            let size = self.size;
            let profiling = crate::app::viewport_benchmark::profiling();
            let results = crate::viewport_workers::map(
                crate::viewport_workers::Kind::Render,
                chunks,
                |(chunk, view)| {
                    crate::app::viewport_benchmark::profile_worker(profiling, || {
                        let _profile =
                            crate::app::viewport_benchmark::scope("shadow_projection_cpu");
                        let mut vertices = Vec::with_capacity(chunk.len() * 3);
                        for &(triangle, coverage) in chunk {
                            Self::project_shadow_triangle(
                                &view,
                                display,
                                size,
                                *triangle,
                                coverage,
                                &mut vertices,
                            );
                        }
                        vertices
                    })
                },
            );
            for (mut vertices, timings) in results {
                crate::app::viewport_benchmark::absorb_worker(timings);
                self.shadow_vertices.append(&mut vertices);
            }
            return;
        }
        for (triangle, coverage) in triangles {
            Self::project_shadow_triangle(
                &self.view,
                &self.display,
                self.size,
                *triangle,
                coverage,
                &mut self.shadow_vertices,
            );
        }
    }

    pub(super) fn face_triangle(&mut self, triangle: [DVec3; 3], color: Color32, draw: bool, occlude: bool) {
        let start = self.indices.len() as u32;
        let (polygon, len) = self.display.section_triangle(triangle);
        for i in 1..len.saturating_sub(1) {
            let triangle = [polygon[0], polygon[i], polygon[i + 1]];
            let color = self
                .fog_color(color, triangle.iter().sum::<DVec3>() / 3.)
                .to_array();
            let size = self.size;
            let vertices = &mut self.vertices;
            let indices = &mut self.indices;
            let occlusion = &mut self.occlusion;
            self.view.for_each_triangle_depth(triangle, |triangle| {
                if occlude && color[3] == 255 {
                    occlusion.insert(triangle);
                }
                if !draw {
                    return;
                }
                let base = vertices.len() as u32;
                indices.extend([base, base + 1, base + 2]);
                for p in triangle {
                    vertices.push(Vertex {
                        dash_phase: -1.,
                        position: [
                            (p.x / size.x * 2. - 1.) as f32,
                            (1. - p.y / size.y * 2.) as f32,
                            p.z.clamp(0., 1.) as f32,
                        ],
                        color,
                        lateral: 0.,
                        shadow_color: [0; 4],
                    });
                }
            });
        }
        let end = self.indices.len() as u32;
        if start != end {
            if let Some(previous) = self.triangle_ranges.last_mut().filter(|r| r.end == start) {
                previous.end = end;
            } else {
                self.triangle_ranges.push(start..end);
            }
        }
    }
    pub fn paint_cached(mut self, painter: &egui::Painter, rect: Rect, cache: &mut SceneCache) {
        self.occlusion.lines.truncate(self.occlusion.line_cursor);
        cache.occlusion = std::mem::take(&mut self.occlusion);
        cache.selection_occlusion = std::mem::take(&mut self.selection_occlusion);
        cache.annotation_occluders = std::mem::take(&mut self.annotation_occluders);
        self.edge_surfaces
            .retain(|_, surfaces| !surfaces.is_empty());
        cache.edge_surfaces = std::mem::take(&mut self.edge_surfaces);
        let callback = self.retain(cache);
        painter.add(egui_wgpu::Callback::new_paint_callback(
            rect,
            SharedScene(callback),
        ));
    }

    // Compare the actual render output, including previews and annotations.
    // Even rebuilt frames can keep unchanged meshes resident on the GPU,
    // including those using the uncached preview and annotation paths.
    pub(super) fn retain(mut self, cache: &mut SceneCache) -> Arc<SceneCallback> {
        let used = &self.joint_surfaces.used;
        self.joint_surfaces
            .entries
            .retain(|owner, _| used.contains(owner));
        self.joint_surfaces.used.clear();
        cache.joint_surfaces = std::mem::take(&mut self.joint_surfaces);
        self.normalize_parallel_depth();
        let candidate = Arc::new(self.into_callback());
        if let Some(previous) = &cache.retained {
            if bytemuck::cast_slice::<Vertex, u8>(&previous.vertices)
                == bytemuck::cast_slice::<Vertex, u8>(&candidate.vertices)
                && previous.indices == candidate.indices
                && previous.triangle_ranges == candidate.triangle_ranges
                && previous.antialiasing == candidate.antialiasing
                && previous.target_size == candidate.target_size
                && previous.shadow_range == candidate.shadow_range
            {
                return Arc::clone(previous);
            }
        }
        cache.retained = Some(Arc::clone(&candidate));
        candidate
    }

    pub(super) fn normalize_parallel_depth(&mut self) {
        if !self.parallel {
            return;
        }
        // Orthographic projection uses a very distant far plane. WebGL's
        // conversion from [0, 1] to [-1, 1] otherwise rounds nearby faces to
        // the same depth, letting rear faces overwrite visible surfaces.
        // Remap only GPU depths; CPU occlusion keeps its original precision.
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for vertex in self.vertices.iter().chain(&self.shadow_vertices) {
            let depth = f64::from(vertex.position[2]);
            if depth > 0. && depth < 1. {
                lo = lo.min(depth);
                hi = hi.max(depth);
            }
        }
        if !lo.is_finite() {
            return;
        }
        let center = (lo + hi) * 0.5;
        let span = (hi - lo).max(f64::EPSILON);
        for vertex in self.vertices.iter_mut().chain(&mut self.shadow_vertices) {
            let depth = f64::from(vertex.position[2]);
            // Depths 0 and 1 are reserved for overlays and background lines.
            if depth > 0. && depth < 1. {
                vertex.position[2] = (0.5 + 0.8 * (depth - center) / span) as f32;
            }
        }
    }

    pub(super) fn into_callback(mut self) -> SceneCallback {
        // Rendering already draws all faces before all strokes. Pack the index
        // buffer in that same stable order once, rather than issuing one draw
        // per interleaved fragment on every retained frame (and twice for edges).
        // Vertex data, blending order within each pass, and depth stay unchanged.
        if self.triangle_ranges.len() > 1
            || self
                .triangle_ranges
                .first()
                .is_some_and(|range| range.start != 0)
        {
            let mut packed = Vec::with_capacity(self.indices.len());
            for range in &self.triangle_ranges {
                packed.extend_from_slice(&self.indices[range.start as usize..range.end as usize]);
            }
            let face_end = packed.len() as u32;
            let mut start = 0;
            for range in &self.triangle_ranges {
                packed.extend_from_slice(&self.indices[start..range.start as usize]);
                start = range.end as usize;
            }
            packed.extend_from_slice(&self.indices[start..]);
            self.indices = packed;
            self.triangle_ranges.clear();
            self.triangle_ranges.push(0..face_end);
        }
        let shadow_start = self.indices.len() as u32;
        let vertex_start = self.vertices.len() as u32;
        self.indices.extend(vertex_start..vertex_start + self.shadow_vertices.len() as u32);
        self.vertices.append(&mut self.shadow_vertices);
        let shadow_range = shadow_start..self.indices.len() as u32;
        SceneCallback {
            shadow_range,
            vertices: self.vertices,
            indices: self.indices,
            triangle_ranges: self.triangle_ranges,
            buffer_pool: self.buffer_pool,
            gpu_pool: self.gpu_pool,
            gpu_buffers: OnceLock::new(),
            antialiasing: self.display.shadow_antialiasing,
            target_size: [self.size.x, self.size.y]
                .map(|v| (v * self.pixels_per_point).round().max(1.) as u32),
            antialiased: OnceLock::new(),
        }
    }
}
