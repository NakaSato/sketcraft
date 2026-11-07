//! Line, shape and freehand drawing: start, axis and plane locks, snapping, commit.
use super::*;

impl KetchupApp {
    pub(super) fn stop_drawing(&mut self) {
        self.drawing_reference = None;
        self.drawing_guide = None;
        self.drawing_axis_guides.clear();
        self.drawing_projection = None;
        self.drawing = None;
        self.gesture = None;
        self.line_start = None;
        self.line_group = None;
        self.snap = None;
        self.lock = None;
        self.second_lock = None;
        self.length_input.clear();
    }
    pub(super) fn commit_line(&mut self, ctx: &egui::Context, precise: bool) {
        self.refresh_snap(ctx);
        let Some(start) = self.line_start else {
            return;
        };
        let Some(snap) = self.snap else {
            self.say(ctx, "Move over the drawing plane to set a direction");
            return;
        };
        let mut end = snap.point;
        if precise && !self.length_input.trim().is_empty() {
            match precision::positive_length(&self.length_input, self.doc.display.units.length) {
                Ok(length) => {
                    let Some(direction) = (end - start).try_normalize() else {
                        self.say(ctx, "Move the pointer to choose a direction first");
                        return;
                    };
                    end = start + direction * length;
                }
                Err(message) => {
                    self.say(ctx, message);
                    return;
                }
            }
        }
        let before = self.doc.model.clone();
        match self.drawn_geometry(&[start, end], self.line_group) {
            Ok((model, group, closed_shape)) => {
                self.doc.model = model;
                self.line_group = group;
                if let Some(group) = group { self.selected = BTreeSet::from([group]); }
                self.finish_edit("Draw line", before, ctx);
                if closed_shape {
                    self.stop_drawing();
                    self.prompt_drawn_group_name(ctx, group);
                } else {
                    self.line_start = Some(end);
                    self.length_input.clear();
                    self.refresh_snap(ctx);
                }
            }
            Err(message) => self.say(ctx, message),
        }
    }
    pub(super) fn begin_drawing(&mut self, pos: Pos2) {
        self.update_pointer(pos);
        let Some(snap) = self.snap else { return };
        let ray = self
            .doc
            .camera
            .ray(dv(pos - self.viewport.min), dv(self.viewport.size()));
        let surface = self
            .doc
            .model
            .nearest_face(ray)
            .and_then(|(id, _, _)| {
                ketchup_core::faces::plane(&self.doc.model, &self.doc.model.faces[&id].vertices)
            })
            .filter(|plane| (snap.point - plane.origin).dot(plane.normal).abs() < 1e-6);
        let normal = surface.map_or(DVec3::Z, |plane| plane.normal);
        let u = (DVec3::X - normal * normal.x)
            .try_normalize()
            .unwrap_or(DVec3::Y);
        let mut drawing = Drawing {
            origin: snap.point,
            surface_normal: normal,
            on_surface: surface.is_some(),
            normal,
            u,
            v: normal.cross(u),
            points: vec![snap.point],
        };
        drawing.set_lock(self.shape_lock());
        self.drawing = Some(drawing);
        self.update_pointer(pos);
    }
    pub(super) fn set_axis_lock(&mut self, lock: Option<Axis>, ctx: &egui::Context) {
        self.lock = lock;
        self.second_lock = None;
        self.refresh_axis_lock(ctx);
    }

    pub(super) fn plane_lock(&self) -> Option<Axis> {
        let (first, second) = self.lock.zip(self.second_lock)?;
        [Axis::X, Axis::Y, Axis::Z]
            .into_iter()
            .find(|axis| *axis != first && *axis != second)
    }

    pub(super) fn single_axis_lock(&self) -> Option<Axis> {
        self.lock.filter(|_| self.second_lock.is_none())
    }

    pub(super) fn axis_constraint(&self) -> Option<AxisConstraint> {
        self.plane_lock()
            .map(AxisConstraint::Plane)
            .or_else(|| self.lock.map(AxisConstraint::Axis))
    }

    pub(super) fn shape_lock(&self) -> Option<Axis> {
        self.plane_lock().or(self.lock)
    }

    pub(super) fn pull_direction(&self, normal: DVec3) -> DVec3 {
        match self.axis_constraint() {
            Some(AxisConstraint::Axis(axis)) => axis.vector(),
            Some(constraint) => constraint
                .project(normal)
                .try_normalize()
                .unwrap_or_else(|| self.lock.unwrap().vector()),
            None => normal,
        }
    }

    pub(super) fn toggle_axis_lock(&mut self, axis: Axis, ctx: &egui::Context) {
        if self.lock == Some(axis) {
            self.lock = self.second_lock.take();
        } else if self.second_lock == Some(axis) {
            self.second_lock = None;
        } else if self.lock.is_none() {
            self.lock = Some(axis);
        } else if self.second_lock.is_none() {
            self.second_lock = Some(axis);
        } else {
            self.say(ctx, "Unlock an axis before locking another (maximum two)");
            return;
        }
        self.refresh_axis_lock(ctx);
    }

    pub(super) fn refresh_axis_lock(&mut self, ctx: &egui::Context) {
        let lock = self.shape_lock();
        if let Some(drawing) = &mut self.drawing {
            drawing.set_lock(lock);
            self.snap = None;
            self.drawing_guide = None;
            self.drawing_axis_guides.clear();
            self.drawing_projection = None;
        }
        if let Some(pos) = ctx
            .input(|i| i.pointer.hover_pos())
            .filter(|pos| self.viewport.contains(*pos))
            .or_else(|| {
                self.modeling.as_ref()
                    .filter(|_| matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale | Tool::PushPull | Tool::Offset))
                    .map(|edit| edit.pointer)
            })
        {
            self.update_pointer(pos);
        }
    }
    pub(super) fn drawing_snap(&self, pos: Pos2) -> Option<Snap> {
        let drawing = self.drawing.as_ref()?;
        let size = dv(self.viewport.size());
        let pixel = dv(pos - self.viewport.min);
        // Unlocked shapes can acquire targets on any axis; their working plane
        // follows the target. Explicit plane locks reject incompatible targets.
        let inferred = self.infer_snap(
            pos,
            (self.lock.is_none()).then_some(drawing.origin),
            None,
            self.lock.is_none() && self.doc.snap_to_axes,
        );
        if let Some(snap) = inferred.filter(|snap| {
            snap.kind != Inference::Plane
                // A soft local axis must not pull a shape off its starting face.
                && !(self.lock.is_none()
                    && matches!(snap.kind, Inference::Axis(_))
                    && drawing.on_surface
                    && (snap.point - drawing.origin).dot(drawing.surface_normal).abs() > 1e-6)
                && (self.lock.is_none()
                    || (snap.point - drawing.origin).dot(drawing.normal).abs() < 1e-6)
        }) {
            return Some(snap);
        }
        let ray = self.doc.camera.ray(pixel, size);
        let denominator = ray.direction.dot(drawing.normal);
        if denominator.abs() < 1e-8 {
            return None;
        }
        let t = (drawing.origin - ray.origin).dot(drawing.normal) / denominator;
        if !(0.0..1e7).contains(&t) {
            return None;
        }
        let point = ray.origin + ray.direction * t;
        if self.tool == Tool::Freehand
            && self.doc.snap_to_geometry
            && self.snap_to_edge
            && drawing.points.len() > 2
            && self
                .doc
                .camera
                .project(drawing.origin, size)
                .is_some_and(|p| p.distance(pixel) < 9.)
        {
            return Some(Snap {
                point: drawing.origin,
                kind: Inference::Endpoint,
            });
        }
        Some(Snap {
            point,
            kind: Inference::Plane,
        })
    }
    pub(super) fn align_drawing_plane(&mut self) {
        if self.lock.is_none()
            && let Some(snap) = self.snap.filter(|snap| snap.kind != Inference::Plane)
            && let Some(drawing) = &mut self.drawing
        {
            drawing.align_to(snap.point, self.doc.camera.eye());
        }
    }
    pub(super) fn drawing_points(&self) -> Vec<DVec3> {
        self.precise_drawing_points()
            .unwrap_or_else(|_| self.raw_drawing_points())
    }
    pub(super) fn raw_drawing_points(&self) -> Vec<DVec3> {
        let Some(drawing) = &self.drawing else {
            return vec![];
        };
        let Some(snap) = self.snap else {
            return drawing.points.clone();
        };
        let origin = drawing.origin;
        let delta = snap.point - origin;
        match self.tool {
            Tool::Rectangle => {
                let a = drawing.u * delta.dot(drawing.u);
                let b = drawing.v * delta.dot(drawing.v);
                vec![origin, origin + a, origin + a + b, origin + b, origin]
            }
            Tool::Circle | Tool::Polygon => {
                let sides = if self.tool == Tool::Circle {
                    48
                } else {
                    self.polygon_sides
                };
                let tangent = drawing.normal.cross(delta);
                (0..=sides)
                    .map(|i| {
                        let angle = (i % sides) as f64 * std::f64::consts::TAU / sides as f64;
                        origin + delta * angle.cos() + tangent * angle.sin()
                    })
                    .collect()
            }
            Tool::Freehand => {
                let mut points = drawing.points.clone();
                if points.last().is_none_or(|p| p.distance(snap.point) > 1e-7) {
                    points.push(snap.point);
                }
                points
            }
            _ => vec![],
        }
    }
    pub(super) fn sample_freehand(&mut self, pos: Pos2) {
        if !self.viewport.contains(pos) {
            return;
        }
        self.update_pointer(pos);
        if let (Some(drawing), Some(snap)) = (&mut self.drawing, self.snap) {
            let last = *drawing.points.last().unwrap();
            let far_enough = self
                .doc
                .camera
                .project(last, dv(self.viewport.size()))
                .is_some_and(|p| p.distance(dv(pos - self.viewport.min)) >= 4.);
            if far_enough && last.distance(snap.point) > 1e-7 {
                drawing.points.push(snap.point);
            }
        }
    }
    pub(super) fn commit_drawing(&mut self, ctx: &egui::Context) {
        if self.tool != Tool::Freehand {
            if let Some(pos) = ctx
                .input(|i| i.pointer.hover_pos())
                .filter(|p| self.viewport.contains(*p))
            {
                self.update_pointer(pos);
            }
        }
        let points = match self.precise_drawing_points() {
            Ok(points) => points,
            Err(message) => {
                self.say(ctx, message);
                return;
            }
        };
        let flat_rectangle = self.tool == Tool::Rectangle
            && self.length_input.trim().is_empty()
            && points.windows(2).any(|pair| {
                self.doc
                    .camera
                    .segment(pair[0], pair[1], dv(self.viewport.size()))
                    .is_some_and(|(a, b)| a.distance(b) < 1.)
            });
        if points.len() < 2
            || flat_rectangle
            || points.windows(2).any(|p| p[0].distance(p[1]) < 1e-7)
        {
            if self.tool == Tool::Freehand {
                self.drawing = None;
            } else {
                self.say(ctx, "Move the pointer to give the shape some size");
            }
            return;
        }
        let before = self.doc.model.clone();
        let (model, group, _) = match self.drawn_geometry(&points, None) {
            Ok(result) => result,
            Err(message) => { self.say(ctx, message); return; }
        };
        self.doc.model = model;
        if let Some(group) = group { self.selected = BTreeSet::from([group]); }
        self.finish_edit(
            &format!("Draw {}", self.tool.label().to_lowercase()),
            before,
            ctx,
        );
        self.stop_drawing();
        self.prompt_drawn_group_name(ctx, group);
    }
}
