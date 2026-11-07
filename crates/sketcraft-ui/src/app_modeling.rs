//! Modeling tools (Push/Pull, Offset, Move, Rotate, Scale, Paint): anchors, previews, snapping, commit.
use super::*;

impl KetchupApp {
    // A selected single-face group can be edited without opening its context.
    pub(super) fn selected_modeling_face(&self) -> Option<u64> {
        if !matches!(self.tool, Tool::PushPull | Tool::Offset) || self.selected.len() != 1 {
            return None;
        }
        let group = *self.selected.first()?;
        let model = &self.doc.model;
        if !model.objects.contains_key(&group)
            || model.entity_in_context(group) != Some(group)
            || model.is_locked(group)
            || model.is_hidden(group)
        {
            return None;
        }
        let mut faces = model.faces.keys().copied().filter(|id| {
            model.entity_in_context(*id) == Some(group)
        });
        let face = faces.next()?;
        (faces.next().is_none() && !model.is_hidden(face) && !model.is_locked(face))
            .then_some(face)
    }
    pub(super) fn selection_anchor_snap(&mut self, pos: Pos2) -> Option<Snap> {
        self.prepare_picking();
        let snap = self.infer_snap(pos, None, None, self.doc.snap_to_axes)?;
        Some(if self.snap_to_grid {
            grid::snap_with_geometry(
                snap,
                grid::configured_spacing(&self.doc.camera, &self.doc.display),
                None,
                None,
                &self.doc.model,
                None,
            )
        } else {
            snap
        })
    }

    pub(super) fn face_anchor_contains(&self, id: u64, point: DVec3) -> bool {
        let model = &self.doc.model;
        let face = &model.faces[&id];
        if model.face_contains(face, point, true) {
            return true;
        }
        // Hole boundaries are also valid edge/vertex anchors, though they are
        // excluded from the filled face's interior hit test.
        let Some(plane) = ketchup_core::faces::plane(model, &face.vertices) else {
            return false;
        };
        (point - plane.origin).dot(plane.normal).abs() < 1e-6
            && face.holes.iter().any(|ring| (0..ring.len()).any(|i| {
                ketchup_core::selection::distance_to_segment(
                    plane.project(point),
                    plane.project(model.vertices[&ring[i]].position),
                    plane.project(model.vertices[&ring[(i + 1) % ring.len()]].position),
                ) < 1e-6
            }))
    }

    pub(super) fn face_anchor_snap(&mut self, pos: Pos2) -> Option<(u64, Snap)> {
        self.prepare_picking();
        let ray = self.doc.camera.ray(dv(pos - self.viewport.min), dv(self.viewport.size()));
        let selected = self.selected_modeling_face();
        let inferred = self.infer_snap(pos, None, None, self.doc.snap_to_axes);
        let hit = selected.and_then(|id| {
            self.doc.model.face_hit(&self.doc.model.faces[&id], ray)
                .map(|(_, point)| (id, point))
        }).or_else(|| self.doc.model.nearest_editable_face(ray).map(|(id, _, point)| (id, point)));
        // A nearby edge/vertex can acquire the face even when the raw pointer
        // lies just outside its boundary. Closed groups still need a unique face.
        let (id, point) = hit.or_else(|| {
            let snap = inferred.filter(|snap| snap.kind != Inference::Plane)?;
            let id = selected.filter(|id| self.face_anchor_contains(*id, snap.point))
                .or_else(|| self.doc.model.faces.keys().rev().copied().find(|id| {
                    self.doc.model.parent(*id) == self.doc.model.edit_context
                        && !self.doc.model.is_hidden(*id)
                        && self.face_anchor_contains(*id, snap.point)
                }))?;
            Some((id, snap.point))
        })?;
        let snap = inferred.filter(|snap| {
            snap.kind != Inference::Plane && self.face_anchor_contains(id, snap.point)
        }).unwrap_or(Snap { point, kind: Inference::Plane });
        let snap = if self.snap_to_grid {
            let face = &self.doc.model.faces[&id];
            let plane = ketchup_core::faces::plane(&self.doc.model, &face.vertices)?;
            let quantized = grid::snap_with_geometry(
                snap,
                grid::configured_spacing(&self.doc.camera, &self.doc.display),
                Some(plane.normal),
                grid::movement_axis(snap.point, None, None, self.doc.snap_to_axes),
                &self.doc.model,
                None,
            );
            if self.face_anchor_contains(id, quantized.point) { quantized } else { snap }
        } else {
            snap
        };
        Some((id, snap))
    }

    pub(super) fn begin_modeling(&mut self, pos: Pos2, ctx: &egui::Context) {
        self.last_move_copy = None;
        let size = dv(self.viewport.size());
        let pixel = dv(pos - self.viewport.min);
        let ray = self.doc.camera.ray(pixel, size);
        if self.tool == Tool::Paint {
            if let Some((face, _, _)) = self.doc.model.nearest_context_face(ray) {
                let before = self.doc.model.clone();
                let (result, label) = match self.paint_target {
                    PaintTarget::Face => (
                        self.doc.model.paint_face(face, self.paint_color),
                        "Paint face",
                    ),
                    PaintTarget::Group => {
                        let target = self.doc.model.entity_in_context(face).unwrap();
                        (
                            self.doc.model.paint_object(target, self.paint_color),
                            "Paint group",
                        )
                    }
                };
                match result {
                    Ok(()) => self.finish_edit(label, before, ctx),
                    Err(message) => self.say(ctx, message),
                }
            } else {
                self.say(ctx, self.paint_instructions());
            }
            return;
        }
        let mut face_id = None;
        let (anchor, direction) = if matches!(self.tool, Tool::PushPull | Tool::Offset) {
            let selected_face = self.selected_modeling_face();
            let Some((id, snap)) = self.face_anchor_snap(pos) else {
                self.say(ctx, "Click a filled face first");
                return;
            };
            if self.doc.model.is_locked(id) {
                self.say(ctx, "Unlock the face before editing it");
                return;
            }
            face_id = Some(id);
            let point = snap.point;
            let face = &self.doc.model.faces[&id];
            let plane = ketchup_core::faces::plane(&self.doc.model, &face.vertices).unwrap();
            let direction = if self.tool == Tool::PushPull {
                plane.normal
                    * if plane.normal.dot(self.doc.camera.eye() - point) >= 0. {
                        1.
                    } else {
                        -1.
                    }
            } else {
                let model = &self.doc.model;
                let closest = std::iter::once(&face.vertices)
                    .chain(face.holes.iter())
                    .enumerate()
                    .flat_map(|(hole, ring)| {
                        let area: f64 = (0..ring.len()).map(|i| {
                            plane.project(model.vertices[&ring[i]].position).perp_dot(
                                plane.project(model.vertices[&ring[(i + 1) % ring.len()]].position),
                            )
                        }).sum();
                        let sign = area.signum() * if hole == 0 { 1. } else { -1. };
                        (0..ring.len()).map(move |i| {
                            let a = model.vertices[&ring[i]].position;
                            let b = model.vertices[&ring[(i + 1) % ring.len()]].position;
                            let closest = a + (b - a)
                                * ((point - a).dot(b - a) / (b - a).length_squared()).clamp(0., 1.);
                            (closest, plane.normal.cross(b - a).normalize() * sign)
                        })
                    })
                    .min_by(|a, b| {
                        a.0.distance_squared(point)
                            .total_cmp(&b.0.distance_squared(point))
                    })
                    .unwrap();
                let delta = point - closest.0;
                if delta.length() > 1e-7 { delta.normalize() } else { closest.1 }
            };
            if selected_face != Some(id) {
                self.selected = BTreeSet::from([id]);
            }
            (point, direction)
        } else {
            if self.selected.is_empty() {
                if let Some(id) = self.pick_at(pixel, size, true) {
                    self.selected.insert(id);
                }
            }
            if self.selected.is_empty() {
                self.say(ctx, "Select or click an edge or face first");
                return;
            }
            if self.doc.model.selection_locked(&self.selected) {
                self.say(ctx, "Unlock the selection before editing it");
                return;
            }
            let Some(snap) = self.selection_anchor_snap(pos) else {
                return;
            };
            (snap.point, DVec3::ZERO)
        };
        let center = self
            .doc
            .model
            .selection_center(&self.selected)
            .unwrap_or(anchor);
        let snap_excluded = self.modeling_snap_exclusions(face_id);
        let rotation = (self.tool == Tool::Rotate).then(|| self.rotation_at(anchor, ray));
        self.modeling = Some(Modeling {
            base: self.doc.model.clone(),
            entities: self.selected.clone(),
            face: face_id,
            anchor,
            pixel: pos,
            pointer: pos,
            center,
            scale_constraint: self.axis_constraint(),
            rotation,
            direction: if self.tool == Tool::PushPull {
                self.pull_direction(direction)
            } else {
                direction
            },
            unlocked_direction: direction,
            guides: vec![],
            value: if self.tool == Tool::Scale { 1. } else { 0. },
            copy: self.tool == Tool::Move && self.move_copy,
            preview: None,
            move_snap: matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale)
                .then(|| {
                    if self.tool == Tool::Rotate {
                        ketchup_core::inference::MoveSnapCache::new(&self.doc.model)
                    } else {
                        ketchup_core::inference::MoveSnapCache::excluding(&self.doc.model, &snap_excluded)
                    }
                }),
            snap_excluded,
            move_preview: (self.tool == Tool::Move)
                .then(|| {
                    ketchup_core::modeling::MovePreview::new(&self.doc.model, &self.selected).ok()
                })
                .flatten(),
            preview_delta: None,
            snap: None,
            error: None,
        });
        self.length_input.clear();
    }

    pub(super) fn modeling_snap_exclusions(&self, face: Option<u64>) -> BTreeSet<u64> {
        let model = &self.doc.model;
        let selected = face.map(|id| BTreeSet::from([id]));
        let vertices = model.selection_vertices(selected.as_ref().unwrap_or(&self.selected));
        let mut excluded: BTreeSet<_> = model
            .edges
            .values()
            .filter(|edge| {
                if face.is_some() {
                    vertices.contains(&edge.a) && vertices.contains(&edge.b)
                } else {
                    vertices.contains(&edge.a) || vertices.contains(&edge.b)
                }
            })
            .map(|edge| edge.id)
            .collect();
        if let Some(face) = face {
            excluded.insert(face);
        } else {
            // Connected surfaces also change when any of their vertices move.
            excluded.extend(
                model.faces.values().filter(|face| {
                    face.vertices.iter().chain(face.holes.iter().flatten())
                        .any(|id| vertices.contains(id))
                }).map(|face| face.id),
            );
        }
        excluded
    }

    pub(super) fn offset_snap(&mut self, pos: Pos2) -> Option<Snap> {
        self.prepare_picking();
        let edit = self.modeling.as_ref()?;
        let face = &edit.base.faces[&edit.face?];
        let plane = ketchup_core::faces::plane(&edit.base, &face.vertices)?;
        let constraint = self.axis_constraint();
        let snap = self
            .infer_snap(pos, Some(edit.anchor), self.lock, self.doc.snap_to_axes)
            // Free Offset follows its face, including tilted and vertical faces.
            // Explicit locks use the shared axis/plane through the anchor.
            .filter(|snap| snap.kind != Inference::Plane || constraint.is_some())
            .or_else(|| {
                if constraint.is_some() {
                    return None;
                }
                let ray = self.doc.camera.ray(dv(pos - self.viewport.min), dv(self.viewport.size()));
                let denominator = ray.direction.dot(plane.normal);
                if denominator.abs() < 1e-8 {
                    return None;
                }
                let t = (edit.anchor - ray.origin).dot(plane.normal) / denominator;
                (0.0..1e7).contains(&t).then_some(Snap {
                    point: ray.origin + ray.direction * t,
                    kind: Inference::Plane,
                })
            })?;
        Some(if self.snap_to_grid {
            grid::snap_with_geometry(
                snap,
                grid::configured_spacing(&self.doc.camera, &self.doc.display),
                match constraint {
                    Some(AxisConstraint::Plane(normal)) => Some(normal.vector()),
                    Some(AxisConstraint::Axis(_)) => None,
                    None => Some(plane.normal),
                },
                grid::movement_axis(snap.point, Some(edit.anchor), constraint, self.doc.snap_to_axes),
                &edit.base,
                Some(&edit.snap_excluded),
            )
        } else {
            snap
        })
    }

    pub(super) fn update_modeling(&mut self, pos: Pos2) {
        if self.tool == Tool::Rotate {
            self.update_rotation(pos);
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = viewport_benchmark::scope("modeling_preview");
        // Recompute when a lock is set, switched, or cleared during a pull.
        let constraint = self.axis_constraint();
        let pull_direction = self.modeling.as_ref()
            .map(|edit| self.pull_direction(edit.unlocked_direction));
        if self.tool == Tool::PushPull
            && let Some(edit) = self.modeling.as_mut()
        {
            edit.direction = pull_direction.unwrap();
        }
        // References may be anywhere under the cursor, including geometry
        // outside the current group. Project them onto the extrusion direction.
        let pull_snap = if self.tool == Tool::PushPull
            && self.modeling.is_some()
            && self.length_input.trim().is_empty()
            && (self.doc.snap_to_geometry || self.doc.snap_to_axes)
        {
            self.prepare_picking();
            self.infer_snap(pos, None, None, self.doc.snap_to_axes).filter(|snap| {
                let edit = self.modeling.as_ref().unwrap();
                matches!(snap.kind, Inference::Origin | Inference::Endpoint
                    | Inference::Midpoint | Inference::FaceCenter | Inference::OnEdge | Inference::OnFace
                    | Inference::OnAxis(_) | Inference::Axis(_))
                    // The unchanged starting surface must not swallow free
                    // movement while the preview moves away from it.
                    && (snap.kind != Inference::OnFace
                        || (snap.point - edit.anchor).dot(edit.direction).abs() > 1e-7)
            })
        } else {
            None
        };
        let pull_snap = pull_snap.map(|snap| {
            if self.snap_to_grid {
                grid::snap_with_geometry(
                    snap,
                    grid::configured_spacing(&self.doc.camera, &self.doc.display),
                    None,
                    None,
                    &self.doc.model,
                    self.modeling.as_ref().map(|edit| &edit.snap_excluded),
                )
            } else {
                snap
            }
        });
        let offset_snap = (self.tool == Tool::Offset && self.length_input.trim().is_empty())
            .then(|| self.offset_snap(pos))
            .flatten();
        let Some(edit) = self.modeling.as_mut() else {
            return;
        };
        edit.pointer = pos;
        let size = dv(self.viewport.size());
        let ray = self.doc.camera.ray(dv(pos - self.viewport.min), size);
        edit.value = match self.tool {
            Tool::PushPull => {
                edit.snap = pull_snap;
                pull_snap
                    .map(|snap| (snap.point - edit.anchor).dot(edit.direction))
                    .unwrap_or_else(|| {
                        ray.axis(edit.anchor, edit.direction)
                            .map(|p| (p - edit.anchor).dot(edit.direction))
                            .unwrap_or(
                                (edit.pixel.y - pos.y) as f64 * self.doc.camera.distance / size.y,
                            )
                    })
            }
            Tool::Offset => {
                edit.snap = offset_snap;
                offset_snap.map_or(edit.value, |snap| {
                    (snap.point - edit.anchor).dot(edit.direction)
                })
            }
            Tool::Move | Tool::Scale => {
                edit.scale_constraint = constraint;
                edit.guides.clear();
                edit.snap = None;
                if let Some(inference) = edit.move_snap.as_mut().unwrap().infer_with_constraint(
                    &self.doc.camera,
                    size,
                    dv(pos - self.viewport.min),
                    edit.anchor,
                    constraint,
                    self.doc.snap_to_geometry,
                    self.doc.snap_to_axes,
                    ketchup_core::inference::SnapTargets {
                        edges: self.snap_to_edge,
                        faces: self.snap_to_face,
                        face_centers: self.snap_to_face_center,
                    },
                ) {
                    let snap = if self.tool == Tool::Scale {
                        Self::scale_snap(
                            edit,
                            inference.snap,
                            self.snap_to_grid.then(|| {
                                grid::configured_spacing(&self.doc.camera, &self.doc.display)
                            }),
                            self.doc.snap_to_axes,
                        )
                    } else if self.snap_to_grid {
                        grid::snap_with_geometry(
                            inference.snap,
                            grid::configured_spacing(&self.doc.camera, &self.doc.display),
                            match constraint {
                                Some(AxisConstraint::Plane(normal)) => Some(normal.vector()),
                                _ => None,
                            },
                            grid::movement_axis(
                                inference.snap.point,
                                Some(edit.anchor),
                                constraint,
                                self.doc.snap_to_axes,
                            ),
                            &edit.base,
                            Some(&edit.snap_excluded),
                        )
                    } else {
                        inference.snap
                    };
                    edit.direction = snap.point - edit.anchor;
                    edit.guides = inference.guides;
                    edit.snap = Some(snap);
                } else if let Some(constraint) = constraint {
                    edit.direction = constraint.project(edit.direction);
                }
                if self.tool == Tool::Scale {
                    // Crossing the center changes the reference's sign, not its size.
                    Self::scale_value(edit, &self.doc.camera, size, self.snap_to_grid).abs()
                } else {
                    edit.direction.length()
                }
            }
            _ => return,
        };
        // Reference targets already combine geometry, axes and grid above.
        // Free extrusion uses grid distances along the normal or locked axis.
        if self.tool == Tool::PushPull && self.snap_to_grid && pull_snap.is_none()
            && self.length_input.trim().is_empty()
        {
            let spacing = grid::configured_spacing(&self.doc.camera, &self.doc.display);
            edit.value = (edit.value / spacing).round() * spacing;
        }
        let value =
            precision::modeling_value_with_units(self.tool, &self.length_input, self.doc.display.units)
                .unwrap_or(edit.value);
        if matches!(self.tool, Tool::PushPull | Tool::Offset)
            && value.abs() < 1e-7
            && self.length_input.trim().is_empty()
        {
            edit.preview = None;
            edit.error = None;
            return;
        }
        if self.tool == Tool::Move && !edit.copy {
            let delta = if value.abs() < 1e-7 {
                Some(DVec3::ZERO)
            } else {
                edit.direction
                    .try_normalize()
                    .map(|direction| direction * value)
            };
            if let (Some(plan), Some(delta)) = (&edit.move_preview, delta) {
                if edit.preview_delta == Some(delta) {
                    return;
                }
                let mut preview = edit.preview.take().unwrap_or_else(|| edit.base.clone());
                match plan.update(&mut preview, delta) {
                    Ok(()) => {
                        edit.preview = Some(preview);
                        edit.error = None;
                    }
                    Err(error) => {
                        edit.error = Some(error);
                    }
                }
                edit.preview_delta = Some(delta);
                return;
            }
        }
        match Self::modeling_result(self.tool, edit, value) {
            Ok(model) => {
                edit.preview = Some(model);
                edit.error = None;
            }
            Err(error) => {
                edit.preview = None;
                edit.error = Some(error);
            }
        }
    }

    pub(super) fn modeling_preview_snap(&self) -> Option<Snap> {
        let edit = self.modeling.as_ref()?;
        if self.length_input.trim().is_empty() && let Some(snap) = edit.snap {
            return Some(snap);
        }
        // Typed values move the live marker to the same point as the preview,
        // just as typed drawing lengths move the drawing tool's endpoint.
        let value = precision::modeling_value_with_units(
            self.tool, &self.length_input, self.doc.display.units,
        ).unwrap_or(edit.value);
        let point = match self.tool {
            Tool::Move => edit.anchor + Self::move_delta(edit, value).ok()?,
            Tool::Rotate => Self::rotation_marker(edit, value)?,
            Tool::Scale => {
                let reference = edit.anchor - edit.center;
                let reference = edit.scale_constraint.map_or(reference, |lock| lock.project(reference));
                edit.anchor + reference * (value - 1.)
            }
            Tool::PushPull | Tool::Offset => edit.anchor + edit.direction * value,
            _ => return None,
        };
        let kind = edit.snap.filter(|snap| snap.point.distance(point) < 1e-6)
            .map(|snap| snap.kind)
            .unwrap_or_else(|| self.single_axis_lock().map_or(Inference::Plane, Inference::Axis));
        Some(Snap { point, kind })
    }

    pub(super) fn modeling_result(tool: Tool, edit: &Modeling, value: f64) -> Result<Model, &'static str> {
        let mut model = edit.base.clone();
        match tool {
            Tool::PushPull => {
                let face = &model.faces[&edit.face.unwrap()];
                let face_id = face.id;
                let context = model.edit_context;
                model.edit_context = model.parent(face_id);
                model.push_pull_along(face_id, edit.direction * value)?;
                model.edit_context = context;
            }
            Tool::Offset => {
                let face = edit.face.unwrap();
                let context = model.edit_context;
                model.edit_context = model.parent(face);
                model.offset_face(face, value)?;
                model.edit_context = context;
            }
            Tool::Rotate => {
                let rotation = edit.rotation.as_ref().unwrap();
                if rotation.reference.is_none() {
                    return Err("Click a reference point away from the pivot first");
                }
                let reference = rotation.reference.unwrap();
                if (reference - rotation.normal * reference.dot(rotation.normal)).length_squared() < 1e-12 {
                    return Err("The reference lies on the rotation axis; clear the lock or restart with another reference");
                }
                model.rotate_selection(&edit.entities, edit.anchor, rotation.normal, value)?;
            }
            Tool::Scale => model.scale_selection_constrained(
                &edit.entities, edit.center, value, edit.scale_constraint,
            )?,
            Tool::Move => {
                let delta = Self::move_delta(edit, value)?;
                if edit.copy {
                    let source = edit.base.copy_entities(&edit.entities);
                    return Self::move_copies(&edit.base, &source, delta, CopyArray::default())
                        .map(|(model, _)| model);
                }
                model.move_selection(&edit.entities, delta)?;
            }
            _ => return Err("Choose a modeling tool"),
        }
        Ok(model)
    }

    pub(super) fn commit_modeling(&mut self, ctx: &egui::Context) {
        if let Some(pos) = ctx
            .input(|i| i.pointer.hover_pos())
            .filter(|p| self.viewport.contains(*p))
        {
            self.update_modeling(pos);
        }
        if self.tool == Tool::Rotate && self.choose_rotation_reference(ctx) {
            return;
        }
        let Some(edit) = self.modeling.as_ref() else {
            return;
        };
        let value = if self.length_input.trim().is_empty() {
            edit.value
        } else {
            match precision::modeling_value_with_units(
                self.tool,
                &self.length_input,
                self.doc.display.units,
            ) {
                Ok(value) => value,
                Err(message) => {
                    self.say(ctx, message);
                    return;
                }
            }
        };
        match Self::modeling_result(self.tool, edit, value) {
            Ok(model) => {
                let before = edit.base.clone();
                if self.tool == Tool::PushPull {
                    let group = edit.face.and_then(|face| before.entity_in_context(face))
                        .filter(|id| before.objects.contains_key(id));
                    self.selected = group.into_iter().chain(model
                        .faces
                        .keys()
                        .filter(|id| {
                            !before.faces.contains_key(id)
                                && model.parent(**id) == model.edit_context
                        })
                        .next_back()
                        .copied()
                        .or(edit.face)
                        .filter(|_| group.is_none()))
                        .collect();
                }
                let copy = (self.tool == Tool::Move && edit.copy).then(|| MoveCopy {
                    base: before.clone(),
                    source: before.copy_entities(&edit.entities),
                    delta: Self::move_delta(edit, value).expect("validated move"),
                });
                if copy.is_some() {
                    self.selected = model
                        .selectable_entities()
                        .into_iter()
                        .filter(|id| !before.contains_entity(id))
                        .collect();
                }
                self.doc.model = model;
                self.modeling = None;
                self.length_input.clear();
                self.finish_edit(
                    if copy.is_some() {
                        "Move copy"
                    } else {
                        self.tool.label()
                    },
                    before,
                    ctx,
                );
                self.last_move_copy = copy;
            }
            Err(message) => self.say(ctx, message),
        }
    }
}
