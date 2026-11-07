//! Viewport pointer input: picking, snapping, erase sweeps, orbit, pan and zoom.
use super::*;

impl KetchupApp {
    pub(super) fn erase_target(&mut self, pos: Pos2) -> Option<u64> {
        let pixel = dv(pos - self.viewport.min);
        let size = dv(self.viewport.size());
        let id = self.pick_at(pixel, size, false).or_else(|| {
            self.picking_cache
                .nearest(
                    &self.face_cache,
                    &self.entity_cache,
                    self.doc.camera.ray(pixel, size),
                    false,
                )
                .and_then(|(id, _, _)| self.entity_cache.get(id).context)
        })?;
        (!self.doc.model.is_locked(id)).then_some(id)
    }
    pub(super) fn sweep_erase(&mut self, from: Pos2, to: Pos2) -> BTreeSet<u64> {
        let a = dv(from - self.viewport.min);
        let b = dv(to - self.viewport.min);
        let size = dv(self.viewport.size());
        let mut ids: BTreeSet<_> = self
            .doc
            .model
            .edges
            .values()
            .filter(|edge| {
                !self.doc.model.is_hidden(edge.id)
                    && !self.doc.model.is_locked(edge.id)
                    && self.doc.model.entity_in_context(edge.id).is_some()
            })
            .filter_map(|edge| {
                let (start, end) = self.doc.model.points(edge);
                let (c, d) = self.doc.camera.segment(start, end, size)?;
                if selection::segment_distance(a, b, c, d) >= 9. {
                    return None;
                }
                let pixel = selection::closest_on_second_segment(a, b, c, d);
                let point = self.doc.camera.ray(pixel, size).segment_point(start, end);
                selection::visible(&self.doc.model, &self.doc.camera, size, point)
                    .then(|| self.doc.model.entity_in_context(edge.id))
                    .flatten()
            })
            .collect();
        // Boundary crossings are handled by the edge sweep. Pick interiors
        // explicitly so clicks and strokes contained within a face work too.
        if let Some(id) = self.erase_target(from) {
            ids.insert(id);
        }
        if to != from {
            if let Some(id) = self.erase_target(to) {
                ids.insert(id);
            }
        }
        ids
    }
    pub(super) fn prepare_picking(&mut self) {
        #[cfg(test)]
        {
            self.picking_preparations += 1;
        }
        self.entity_cache.update(&self.doc.model);
        self.face_cache.update(&self.doc.model);
        self.picking_cache.set_display(&self.doc.display);
        self.picking_cache.update(&self.face_cache);
    }
    // Call after prepare_picking so inference shares the viewport's cached
    // planes and spatial hierarchy instead of rebuilding every face per target.
    pub(super) fn snap_targets(&self) -> ketchup_core::inference::SnapTargets {
        ketchup_core::inference::SnapTargets {
            edges: self.snap_to_edge,
            faces: self.snap_to_face,
            face_centers: self.snap_to_face_center,
        }
    }
    // Keep the legacy document flag compatible with older clients. The three
    // target preferences are the controls; there is no separate master switch.
    pub(super) fn sync_geometry_snapping(&mut self) {
        self.doc.snap_to_geometry =
            self.snap_to_edge || self.snap_to_face || self.snap_to_face_center;
    }
    pub(super) fn infer_snap(
        &self,
        pos: Pos2,
        start: Option<DVec3>,
        lock: Option<Axis>,
        axes: bool,
    ) -> Option<Snap> {
        ketchup_core::inference::infer_with_constraint(
            &self.doc.model,
            &self.doc.camera,
            dv(self.viewport.size()),
            dv(pos - self.viewport.min),
            start,
            lock.map(|axis| {
                self.plane_lock().map_or(AxisConstraint::Axis(axis), AxisConstraint::Plane)
            }),
            self.doc.snap_to_geometry,
            axes,
            Some(
                &self
                    .picking_cache
                    .snap_query(&self.face_cache, &self.entity_cache)
                    .excluding(self.modeling.as_ref().map(|edit| &edit.snap_excluded)),
            ),
            self.snap_targets(),
        )
    }
    pub(super) fn pick_at(&mut self, pixel: DVec2, size: DVec2, include_faces: bool) -> Option<u64> {
        // Select and move the visible machined boards, retaining their source
        // object IDs. The cache restores stock geometry in the active edit scope.
        if matches!(self.tool,Tool::Select|Tool::Move|Tool::Rotate|Tool::Scale)
            && self.doc.model.connections.values().any(|c| matches!(c.operation,ketchup_core::connections::ConnectionOperation::Joinery(_)))
            {
            return self.joinery_geometry.pick(&self.doc.model,&self.doc.camera,&self.doc.display,size,pixel,include_faces);
        }
        self.prepare_picking();
        self.picking_cache.pick(
            &self.doc.model,
            &self.face_cache,
            &self.entity_cache,
            &self.doc.camera,
            size,
            pixel,
            8.,
            include_faces,
        )
    }
    pub(super) fn update_pointer(&mut self, pos: Pos2) {
        // Navigation and selection rectangles do not need hover queries.
        // Keep their feedback responsive on large imported models.
        let selecting_box = self.gesture.as_ref().is_some_and(|gesture| {
            matches!(gesture.kind, GestureKind::Select { .. })
                && gesture.start.distance(gesture.last) > 4.
        });
        if self.tool == Tool::Orbit || self.navigating() || selecting_box {
            self.hover = None;
            self.flip_hover = None;
            return;
        }
        if self.modeling.is_some() && matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale) {
            self.hover = None;
            self.update_modeling(pos);
            return;
        }
        if self.modeling.is_none() && matches!(self.tool, Tool::PushPull | Tool::Offset) {
            let anchor = self.face_anchor_snap(pos);
            self.hover = anchor.map(|(id, _)| id);
            self.snap = anchor.filter(|(id, _)| !self.doc.model.is_locked(*id))
                .map(|(_, snap)| snap)
                .or_else(|| self.selection_anchor_snap(pos));
            return;
        }
        let local = dv(pos - self.viewport.min);
        let size = dv(self.viewport.size());
        self.hover = if self.tool == Tool::Erase {
            self.erase_target(pos)
        } else if let Some(face) = self.selected_modeling_face().filter(|face| {
            self.doc.model.face_hit(&self.doc.model.faces[face], self.doc.camera.ray(local, size)).is_some()
        }) {
            Some(face)
        } else if matches!(self.tool, Tool::PushPull | Tool::Offset | Tool::Paint) {
            self.prepare_picking();
            self.picking_cache
                .nearest(
                    &self.face_cache,
                    &self.entity_cache,
                    self.doc.camera.ray(local, size),
                    self.tool != Tool::Paint,
                )
                .and_then(|(id, _, _)| {
                    if self.tool == Tool::Paint && self.paint_target == PaintTarget::Group {
                        self.doc
                            .model
                            .entity_in_context(id)
                            .filter(|id| self.doc.model.objects.contains_key(id))
                    } else {
                        Some(id)
                    }
                })
        } else if matches!(self.tool, Tool::Select | Tool::Move | Tool::Rotate | Tool::Scale)
            || self.tool.is_solid()
        {
            self.pick_at(local, size, true)
        } else {
            self.pick_at(local, size, false)
        };
        if self.tool == Tool::Flip {
            self.flip_hover = self.flip_plane_at(pos);
        }
        if self.tool.is_measuring() {
            self.update_measurement(pos);
        }
        if matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale)
            && self.modeling.is_none()
            && self.paste_preview.is_none()
        {
            // Match the first click's anchor query, before geometry starts moving.
            self.snap = self.selection_anchor_snap(pos);
        }
        if self.tool.is_drawing() || self.paste_preview.is_some() {
            self.snap = self.infer_snap(
                pos,
                if self.paste_preview.is_some() {
                    None
                } else {
                    self.line_start
                },
                if self.paste_preview.is_some() {
                    None
                } else {
                    self.lock
                },
                self.doc.snap_to_axes,
            );
            if self.drawing.is_some() && self.paste_preview.is_none() {
                if self.lock.is_none()
                    && let Some(drawing) = &mut self.drawing
                {
                    // Inferred planes belong to the current snap only. Start
                    // each query on the original plane so leaving a target
                    // restores it without moving earlier freehand samples.
                    drawing.set_normal(drawing.surface_normal);
                }
                self.snap = self.drawing_snap(pos);
                self.align_drawing_plane();
            }
            self.update_drawing_inference(pos);
            if self.paste_preview.is_none() {
                self.align_drawing_plane();
            }
            self.apply_grid_snap();
            if let (Some(preview), Some(snap)) = (&mut self.paste_preview, self.snap) {
                preview.point = snap.point;
            }
        }
        if self.modeling.is_some() {
            self.update_modeling(pos);
        }
    }
    pub(super) fn refresh_snap(&mut self, ctx: &egui::Context) {
        if self.tool == Tool::Line {
            if let Some(pos) = ctx
                .input(|i| i.pointer.hover_pos())
                .filter(|pos| self.viewport.contains(*pos))
            {
                self.update_pointer(pos);
            }
        }
    }
    pub(super) fn preview_alignment(&self) -> Option<Axis> {
        if self.single_axis_lock().is_some() {
            return self.single_axis_lock();
        }
        if !self.doc.snap_to_axes {
            return None;
        }
        let delta = self.snap?.point - self.line_start?;
        if delta.length() < 1e-7 {
            return None;
        }
        [Axis::X, Axis::Y, Axis::Z]
            .into_iter()
            .find(|axis| delta.cross(axis.vector()).length() < 1e-7)
    }
    pub(super) fn orbit_center(&mut self) -> DVec3 {
        // Match the renderer's inherited object/tag visibility, including the
        // options that reveal hidden objects and geometry. Prepare it once at
        // the start of an orbit; keep the pivot fixed throughout the gesture.
        let model = &self.doc.model;
        self.entity_cache.update(model);
        let entities = &self.entity_cache;
        let display = &self.doc.display;
        let points = model
            .edges
            .values()
            .filter(|edge| entities.get(edge.id).displayed(display))
            .flat_map(|edge| {
                let (a, b) = model.points(edge);
                [a, b]
            })
            .chain(
                model
                    .faces
                    .values()
                    .filter(|face| entities.get(face.id).displayed(display))
                    .flat_map(|face| {
                        face.vertices
                            .iter()
                            .chain(face.holes.iter().flatten())
                            .map(|id| model.vertices[id].position)
                    }),
            )
            .chain(
                model
                    .dimensions
                    .values()
                    .filter(|d| display.dimensions && entities.get(d.id).displayed(display))
                    .flat_map(|d| d.layout(model)),
            );
        let mut points = points.chain(
            model
                .texts
                .values()
                .filter(|t| entities.get(t.id).displayed(display))
                .flat_map(|t| t.layout(model)),
        );
        let Some(first) = points.next() else {
            return DVec3::ZERO;
        };
        let (lo, hi) = points.fold((first, first), |(lo, hi), point| {
            (lo.min(point), hi.max(point))
        });
        (lo + hi) * 0.5
    }

    pub(super) fn navigating(&self) -> bool {
        self.drawing_orbit_down
            || self.gesture.as_ref().is_some_and(|g| {
                matches!(
                    g.kind,
                    GestureKind::Navigate { .. } | GestureKind::Compass { dragged: true, .. }
                )
            })
    }

    pub(super) fn drawing_orbit_modifier(&self, modifiers: egui::Modifiers) -> bool {
        self.tool.is_drawing() && (modifiers.ctrl || modifiers.command || modifiers.mac_cmd)
    }

    pub(super) fn update_drawing_orbit(&mut self, modifiers: egui::Modifiers) {
        let down = self.drawing_orbit_modifier(modifiers);
        if down != self.drawing_orbit_down {
            self.drawing_orbit_down = down;
            self.drawing_orbit_pivot = None;
            if let Some(gesture) = &mut self.gesture {
                // An interrupted point click must not place geometry on release.
                if down && matches!(gesture.kind, GestureKind::Line | GestureKind::Shape) {
                    gesture.kind = GestureKind::DrawingNavigate;
                }
                if down && let GestureKind::Freehand { paused } = &mut gesture.kind {
                    *paused = true;
                }
            }
        }
        if down {
            self.hover = None;
            self.flip_hover = None;
        }
    }

    pub(super) fn navigate(&mut self, ctx: &egui::Context, delta: DVec2, pivot: &mut Option<DVec3>) {
        if ctx.input(|i| i.modifiers.shift) {
            self.doc.camera.pan(
                delta * self.general_settings.pan_speed,
                self.viewport.height() as f64,
            );
            *pivot = None;
        } else if delta != DVec2::ZERO {
            let center = *pivot.get_or_insert_with(|| self.orbit_center());
            self.doc
                .camera
                .orbit_around(delta * self.general_settings.orbit_speed, center);
        }
        self.changed(ctx);
    }

    pub(super) fn update_compass_orbit(&mut self, ctx: &egui::Context, gesture: &mut Gesture, pos: Pos2) {
        let GestureKind::Compass { pivot, dragged, .. } = &mut gesture.kind else {
            return;
        };
        if !matches!(
            gesture.button,
            egui::PointerButton::Primary | egui::PointerButton::Middle
        ) {
            return;
        }
        // Once a drag starts, returning to its start must not align an axis.
        if !*dragged && gesture.start.distance(pos) < 5. {
            return;
        }
        let delta = if *dragged {
            pos - gesture.last
        } else {
            pos - gesture.start
        };
        *dragged = true;
        if delta != Vec2::ZERO {
            if ctx.input(|i| i.modifiers.shift) {
                // Share Orbit's pan speed, viewport scale, and pivot reset.
                // Read Shift on every movement so it can change mid-drag.
                self.navigate(ctx, dv(delta), pivot);
            } else {
                let center = *pivot.get_or_insert_with(|| self.orbit_center());
                self.doc.camera.orbit_around(dv(delta), center);
                self.changed(ctx);
            }
            ctx.request_repaint();
        }
    }

    pub(super) fn zoom_viewport(&mut self, ctx: &egui::Context) -> Option<Pos2> {
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() <= 0.01 { return None; }
        let pos = ctx.input(|i| i.pointer.hover_pos())?;
        self.doc.camera.zoom(
            scroll as f64 * self.general_settings.zoom_speed
                * if self.general_settings.reverse_zoom { -1. } else { 1. },
            dv(pos - self.viewport.min),
            dv(self.viewport.size()),
        );
        self.changed(ctx);
        Some(pos)
    }

    pub(super) fn viewport_input(&mut self, ctx: &egui::Context, response: &egui::Response) {
        if self.fastener_handle_under_pointer(ctx,response.rect) { return; }
        #[cfg(not(target_arch = "wasm32"))]
        if self.updates.open {
            return;
        }

        if self.project.blocked() || self.screenshot.open {
            return;
        }
        if self.settings_open
            || self.help
            || self.about
            || self.release_notes.open
            || self.object_creation.is_some()
            || self.object_rename.is_some()
            || self.viewports.renaming.is_some()
            || self.text_editor.is_some()
        {
            self.drawing_orbit_down = false;
            self.drawing_orbit_pivot = None;
            self.viewport_pointer = None;
            if self.tool == Tool::Freehand {
                self.drawing = None;
            }
            self.gesture = None;
            self.section_drag = None;
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let focused = ctx.input(|i| i.focused);
        if !focused {
            self.drawing_orbit_down = false;
            self.drawing_orbit_pivot = None;
            self.viewport_pointer = None;
            if self.tool == Tool::Freehand {
                self.drawing = None;
            }
            self.gesture = None;
            self.section_drag = None;
            self.hover = None;
            return;
        }
        if response.enabled() && self.joint_marker_under_pointer(ctx, response.rect) {
            self.zoom_viewport(ctx);
            return;
        }
        let mut refresh_pointer = !events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerMoved(_)));
        // Native mouse input interleaves absolute positions and raw deltas.
        // We do not use raw deltas; discard them before looking ahead so they
        // cannot turn one pending pointer position into hundreds of snap queries.
        let mut events = events
            .into_iter()
            .filter(|event| !matches!(event, egui::Event::MouseMoved(_)))
            .peekable();
        while let Some(event) = events.next() {
            // Keep every stroke sample, but coalesce consecutive hover/navigation
            // positions. Button and keyboard events remain ordered barriers.
            let stroking = self.gesture.as_ref().is_some_and(|gesture| {
                matches!(
                    gesture.kind,
                    GestureKind::Freehand { .. } | GestureKind::Erase { .. }
                )
            });
            if matches!(event, egui::Event::PointerMoved(_))
                && matches!(events.peek(), Some(egui::Event::PointerMoved(_)))
                && !stroking
            {
                continue;
            }
            if self.section_plane_event(ctx, &event, response) {
                refresh_pointer = false;
                continue;
            }
            match event {
                egui::Event::ModifiersChanged(modifiers) => {
                    refresh_pointer |=
                        self.drawing_orbit_down && !self.drawing_orbit_modifier(modifiers);
                    self.update_drawing_orbit(modifiers);
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } if response.contains_pointer() && self.viewport.contains(pos) => {
                    if self.compass_contains(pos) {
                        if self.gesture.is_none() {
                            self.gesture = Some(Gesture {
                                button,
                                start: pos,
                                last: pos,
                                kind: GestureKind::Compass {
                                    axis: (button == egui::PointerButton::Primary)
                                        .then(|| self.compass_axis_at(pos))
                                        .flatten(),
                                    pivot: None,
                                    dragged: false,
                                },
                            });
                            self.hover = None;
                            self.flip_hover = None;
                            self.viewport_pointer = None;
                        }
                        continue;
                    }
                    self.update_drawing_orbit(modifiers);
                    if button == egui::PointerButton::Secondary {
                        self.solid_first = None;
                        self.flip_hover = None;
                        self.measurement = None;
                        self.drawing = None;
                        self.gesture = None;
                        self.paste_preview = None;
                        self.modeling = None;
                        self.last_move_copy = None;
                        self.length_input.clear();
                        self.context_position = Some(pos);
                        let hit = self.pick_at(
                            dv(pos - self.viewport.min),
                            dv(self.viewport.size()),
                            true,
                        );
                        self.select_context_target(hit);
                        continue;
                    }
                    if self.gesture.is_some() {
                        continue;
                    }
                    let kind = if button == egui::PointerButton::Middle
                        || (button == egui::PointerButton::Primary && self.tool == Tool::Orbit)
                    {
                        self.hover = None;
                        self.flip_hover = None;
                        self.drawing_orbit_pivot = None;
                        GestureKind::Navigate { pivot: None }
                    } else if button == egui::PointerButton::Primary {
                        if self.drawing_orbit_down {
                            GestureKind::DrawingNavigate
                        } else if self.paste_preview.is_some() {
                            GestureKind::Paste
                        } else {
                            match self.tool {
                                Tool::Select => GestureKind::Select {
                                    additive: modifiers.shift,
                                },
                                Tool::TapeMeasure
                                | Tool::Protractor
                                | Tool::Dimensions
                                | Tool::Text => GestureKind::Measure,
                                Tool::Flip
                                | Tool::OuterShell
                                | Tool::Union
                                | Tool::Intersect
                                | Tool::Trim
                                | Tool::Split => GestureKind::Solid,
                                Tool::Line => GestureKind::Line,
                                Tool::Rectangle | Tool::Circle | Tool::Polygon => {
                                    GestureKind::Shape
                                }
                                Tool::Freehand => {
                                    self.begin_drawing(pos);
                                    GestureKind::Freehand { paused: false }
                                }
                                Tool::PushPull
                                | Tool::Scale
                                | Tool::Offset
                                | Tool::Move
                                | Tool::Rotate
                                | Tool::Paint => GestureKind::Modeling,
                                Tool::Erase => GestureKind::Erase {
                                    ids: self.sweep_erase(pos, pos),
                                },
                                Tool::Orbit => unreachable!(),
                            }
                        }
                    } else {
                        continue;
                    };
                    self.gesture = Some(Gesture {
                        button,
                        start: pos,
                        last: pos,
                        kind,
                    });
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
                egui::Event::PointerMoved(pos) => {
                    let compass_drag = self
                        .gesture
                        .as_ref()
                        .is_some_and(|gesture| matches!(gesture.kind, GestureKind::Compass { .. }));
                    let in_viewport = self.viewport.contains(pos)
                        && response.contains_pointer()
                        && !self.compass_contains(pos)
                        && !compass_drag;
                    let mouse_navigation = self.gesture.as_ref().is_some_and(|gesture| {
                        matches!(
                            gesture.kind,
                            GestureKind::Navigate { .. } | GestureKind::Compass { .. }
                        )
                    });
                    // Modifier navigation follows hover motion as well as drags.
                    // Use the last processed position, not egui's final position
                    // for this frame, so the first movement after key-down counts.
                    if self.drawing_orbit_down && in_viewport && !mouse_navigation {
                        if let Some(previous) = self.viewport_pointer {
                            let mut pivot = self.drawing_orbit_pivot;
                            self.navigate(ctx, dv(pos - previous), &mut pivot);
                            self.drawing_orbit_pivot = pivot;
                        }
                    }
                    self.viewport_pointer = in_viewport.then_some(pos);
                    if let Some(mut gesture) = self.gesture.take() {
                        self.update_compass_orbit(ctx, &mut gesture, pos);
                        match &mut gesture.kind {
                            GestureKind::Navigate { pivot } => {
                                self.navigate(ctx, dv(pos - gesture.last), pivot);
                            }
                            GestureKind::Erase { ids } => {
                                ids.extend(self.sweep_erase(gesture.last, pos));
                            }
                            GestureKind::Freehand { paused } if !self.drawing_orbit_down => {
                                if !*paused || pos != gesture.last {
                                    *paused = false;
                                    self.sample_freehand(pos);
                                }
                            }
                            GestureKind::Modeling
                                if self.modeling.is_none()
                                    && matches!(self.tool, Tool::PushPull | Tool::Move | Tool::Offset)
                                    && gesture.start.distance(pos) >= 5. =>
                            {
                                self.begin_modeling(gesture.start, ctx);
                                gesture.kind = GestureKind::ModelingDrag {
                                    active: self.modeling.is_some(),
                                };
                            }
                            _ => {}
                        }
                        gesture.last = pos;
                        self.gesture = Some(gesture);
                    }
                    if in_viewport {
                        self.update_pointer(pos);
                    } else {
                        self.hover = None;
                        self.flip_hover = None;
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers,
                } => {
                    self.update_drawing_orbit(modifiers);
                    if self.gesture.as_ref().is_some_and(|g| g.button == button) {
                        let mut gesture = self.gesture.take().unwrap();
                        self.update_compass_orbit(ctx, &mut gesture, pos);
                        match gesture.kind {
                            GestureKind::Compass {
                                axis: Some(axis),
                                dragged: false,
                                ..
                            } if gesture.start.distance(pos) < 5.
                                && self.compass_axis_at(pos) == Some(axis) =>
                            {
                                self.doc.camera.set_standard_view(axis.view());
                                self.changed(ctx);
                                ctx.request_repaint();
                            }
                            GestureKind::Navigate { .. } | GestureKind::DrawingNavigate => {}
                            GestureKind::Select { additive } => {
                                let dragged = gesture.start.distance(pos) > 4.;
                                let hits = if dragged {
                                    selection::box_select(
                                        &self.doc.model,
                                        &self.doc.camera,
                                        dv(self.viewport.size()),
                                        dv(gesture.start - self.viewport.min),
                                        dv(pos - self.viewport.min),
                                    )
                                } else {
                                    self.pick_at(
                                        dv(pos - self.viewport.min),
                                        dv(self.viewport.size()),
                                        true,
                                    )
                                    .into_iter()
                                    .collect()
                                };
                                if !dragged
                                    && !additive
                                    && !modifiers.shift
                                    && self.viewport.contains(pos)
                                {
                                    if hits.is_empty() && self.doc.model.edit_context.is_some() {
                                        self.close_edit_context(ctx);
                                        return;
                                    }
                                    if response.double_clicked_by(egui::PointerButton::Primary) {
                                        if let Some(id) = hits
                                            .iter()
                                            .find(|id| self.doc.model.texts.contains_key(id))
                                            .copied()
                                        {
                                            self.selected = BTreeSet::from([id]);
                                            self.begin_text_edit(id, ctx);
                                            return;
                                        }
                                        if let Some(id) = hits
                                            .iter()
                                            .find(|id| self.doc.model.objects.contains_key(id))
                                            .copied()
                                        {
                                            self.enter_object(id, ctx);
                                            return;
                                        }
                                    }
                                }
                                if additive || modifiers.shift {
                                    for id in hits {
                                        if !self.selected.remove(&id) {
                                            self.selected.insert(id);
                                        }
                                    }
                                } else {
                                    self.selected = hits;
                                }
                            }
                            GestureKind::Paste
                                if self.viewport.contains(pos)
                                    && gesture.start.distance(pos) < 5. =>
                            {
                                self.update_pointer(pos);
                                if let Some(preview) = self.paste_preview.take() {
                                    self.paste_at(
                                        ctx,
                                        &preview.clipboard,
                                        preview.point - preview.clipboard.anchor,
                                    );
                                    self.snap = None;
                                }
                            }
                            GestureKind::Solid
                                if self.viewport.contains(pos)
                                    && gesture.start.distance(pos) < 5. =>
                            {
                                self.solid_click(pos, ctx);
                            }
                            GestureKind::Measure
                                if self.viewport.contains(pos)
                                    && gesture.start.distance(pos) < 5. =>
                            {
                                self.measurement_click(pos, ctx);
                            }
                            GestureKind::Line
                                if self.viewport.contains(pos)
                                    && gesture.start.distance(pos) < 5. =>
                            {
                                self.update_pointer(pos);
                                if self.line_start.is_some() {
                                    self.commit_line(ctx, !self.length_input.is_empty());
                                } else if let Some(snap) = self.snap {
                                    self.line_start = Some(snap.point);
                                    self.update_pointer(pos);
                                }
                            }
                            GestureKind::Shape
                                if self.viewport.contains(pos)
                                    && gesture.start.distance(pos) < 5. =>
                            {
                                self.update_pointer(pos);
                                if self.drawing.is_some() {
                                    self.commit_drawing(ctx);
                                } else {
                                    self.begin_drawing(pos);
                                }
                            }
                            GestureKind::Freehand { paused } => {
                                if !paused && self.viewport.contains(pos) {
                                    self.update_pointer(pos);
                                } else {
                                    self.snap =
                                        self.drawing.as_ref().and_then(|d| d.points.last()).map(
                                            |&point| Snap {
                                                point,
                                                kind: Inference::Plane,
                                            },
                                        );
                                }
                                self.commit_drawing(ctx);
                            }
                            GestureKind::Erase { mut ids } => {
                                ids.extend(self.sweep_erase(gesture.last, pos));
                                let before = self.doc.model.clone();
                                self.doc.model.erase(&ids);
                                self.finish_edit("Erase geometry", before, ctx);
                            }
                            GestureKind::Modeling
                                if self.viewport.contains(pos)
                                    && gesture.start.distance(pos) < 5. =>
                            {
                                if self.modeling.is_some() {
                                    self.update_modeling(pos);
                                    self.commit_modeling(ctx);
                                } else {
                                    self.begin_modeling(pos, ctx);
                                }
                            }
                            GestureKind::ModelingDrag { active: true } if self.modeling.is_some() => {
                                self.update_modeling(pos);
                                self.commit_modeling(ctx);
                            }
                            _ => {}
                        }
                    }
                }
                egui::Event::PointerGone => {
                    self.drawing_reference = None;
                    self.drawing_guide = None;
                    self.drawing_axis_guides.clear();
                    self.drawing_projection = None;
                    self.viewport_pointer = None;
                    self.drawing_orbit_pivot = None;
                    if self.tool == Tool::Freehand {
                        self.drawing = None;
                    }
                    self.gesture = None;
                    self.hover = None;
                }
                _ => {}
            }
        }
        self.update_drawing_orbit(ctx.input(|i| i.modifiers));
        // Menus and other overlays can cover the viewport without pointer motion.
        let over_compass = ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|pos| self.compass_contains(pos));
        let compass_drag = self
            .gesture
            .as_ref()
            .is_some_and(|gesture| matches!(gesture.kind, GestureKind::Compass { .. }));
        let over_section = self.section_drag.is_some() || ctx.input(|i| i.pointer.hover_pos()).is_some_and(|pos| self.section_handle_at(pos));
        if !response.contains_pointer() || over_compass || compass_drag || over_section {
            self.hover = None;
            self.flip_hover = None;
        } else if response.hovered() && refresh_pointer {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                self.update_pointer(pos);
            }
        }
        if response.hovered() && !over_compass && !compass_drag && !over_section {
            if let Some(pos) = self.zoom_viewport(ctx) {
                self.update_pointer(pos);
            }
        }
    }
}
