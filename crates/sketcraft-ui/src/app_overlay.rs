//! Viewport painting: the 3D scene, tool overlays, compass, edit-context bar and viewport panel.
use super::*;

impl KetchupApp {
    pub(super) fn paint_tool_cursor(&self, ctx: &egui::Context, response: &egui::Response) {
        if response.enabled() && self.joint_marker_under_pointer(ctx, response.rect) {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
            return;
        }
        if !response.enabled() || !response.hovered() || !response.contains_pointer() {
            return;
        }
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        if self.section_drag.is_some() || self.section_handle_at(pos) { return; }
        if self.compass_contains(pos)
            || self
                .gesture
                .as_ref()
                .is_some_and(|gesture| matches!(gesture.kind, GestureKind::Compass { .. }))
        {
            return;
        }
        let tool = if self.navigating() {
            Tool::Orbit
        } else {
            match self.tool {
                Tool::Rectangle | Tool::Circle | Tool::Polygon => Tool::Line,
                tool => tool,
            }
        };
        if tool == Tool::Select {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
            return;
        }
        let size = 28.;
        // Keep the pencil point at the actual interaction position.
        // Symmetric tool icons use their center as the hotspot.
        let hotspot = match tool {
            Tool::Line => egui::vec2(-0.95, 0.95),
            _ => Vec2::ZERO,
        } * (size * 0.36);
        // Keep tool icons clear of the snapped anchor, as for Dimensions.
        let offset = if tool.is_measuring()
            || matches!(tool, Tool::Move | Tool::Rotate | Tool::Scale | Tool::PushPull | Tool::Offset)
        {
            egui::vec2(15., 15.)
        } else {
            Vec2::ZERO
        };
        #[cfg(target_arch = "wasm32")]
        if crate::tool_cursor::show(ctx, tool, hotspot, offset) {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            return;
        }
        // Let the tool cursor overlap adjacent panels at viewport edges.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("viewport_tool_cursor"),
        ));
        ctx.set_cursor_icon(egui::CursorIcon::None);
        icons::tool(
            &painter,
            Rect::from_center_size(pos - hotspot + offset, Vec2::splat(size)),
            tool,
        );
    }

    pub(super) fn paint_scene(&mut self, ui: &egui::Ui, rect: Rect) {
        self.paint_scene_with_background(ui, rect, true);
    }

    pub(super) fn paint_scene_with_background(&mut self, ui: &egui::Ui, rect: Rect, background: bool) {
        // All scene helpers see the same evaluated geometry while painting.
        // Restore source caches before handling input/inspector queries again.
        std::mem::swap(&mut self.face_cache, &mut self.render_face_cache);
        std::mem::swap(&mut self.entity_cache, &mut self.render_entity_cache);
        std::mem::swap(&mut self.picking_cache, &mut self.render_picking_cache);
        self.paint_evaluated_scene(ui, rect, background);
        std::mem::swap(&mut self.face_cache, &mut self.render_face_cache);
        std::mem::swap(&mut self.entity_cache, &mut self.render_entity_cache);
        std::mem::swap(&mut self.picking_cache, &mut self.render_picking_cache);
    }

    pub(super) fn paint_evaluated_scene(&mut self, ui: &egui::Ui, rect: Rect, background: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        let _profile = viewport_benchmark::scope("scene");
        let colors = Palette::for_ui(ui);
        let faces = FacePalette::for_ui(ui);
        // History previews affect rendering only; editing, saving and picking
        // continue to use the committed document.
        let model = self.history_preview.as_ref().unwrap_or_else(|| {
            self.modeling
                .as_ref()
                .and_then(|edit| edit.preview.as_ref())
                .unwrap_or(&self.doc.model)
        });
        {
            #[cfg(not(target_arch = "wasm32"))]
            let _profile = viewport_benchmark::scope("geometry_update");
            self.joinery_geometry.update_render(model,
                self.history_preview.is_none() && self.modeling.as_ref().is_some_and(|edit| edit.preview.is_some()),
                ui.ctx(), &self.doc.display, &self.shadow_cache);
        }
        let prepared_shadows = self.joinery_geometry.prepared_shadows.take();
        let model=&self.joinery_geometry.result.as_ref().unwrap().model;
        let model_revision = self.render_model.update(model);
        let empty_selection = BTreeSet::new();
        let selected = if self.history_preview.is_some() {
            &empty_selection
        } else {
            &self.selected
        };
        let hover = if self.history_preview.is_some() {
            None
        } else {
            self.hover
        };
        self.entity_cache.update(model);
        let entities = &self.entity_cache;
        let joinery_parts = self.joinery_edit_members().map(|members| members.map(|id|
            model.expand_entities(&BTreeSet::from([id]))));
        let joinery_color = |id: u64| joinery_parts.as_ref().and_then(|parts|
            parts.iter().position(|part| part.contains(&id)).map(Self::joinery_part_color));

        let emphasize_group = joinery_parts.is_some() || (self.doc.display.group_emphasis
            && model
                .edit_context
                .is_some_and(|id| model.contains_entity(&id)));
        let size = dv(rect.size());
        let camera = &self.doc.camera;
        let view = camera.view(size);
        let p = ui.painter().with_clip_rect(rect);
        let mut themed_display = display_scene::effective_display(&self.doc.display, model, ui.visuals().dark_mode);
        if joinery_parts.is_some() {
            if themed_display.face_style == ketchup_core::FaceStyle::Wireframe {
                themed_display.face_style = ketchup_core::FaceStyle::Shaded;
            }
            themed_display.x_ray = false;
            themed_display.shadows = false;
            themed_display.section_fill = false;
        }
        let display = &themed_display;
        if background {
            crate::environment::paint(&p, rect, camera, display, ui.visuals().dark_mode, self.doc.display.shadows);
        }
        // Transient previews and annotation layout use the normal render path.
        // Plain model redraws retain the complete CPU/GPU mesh until an input
        // affecting its appearance changes. Screen overlays below stay live.
        let key = (self.modeling.is_none()
            && self.drawing.is_none()
            && self.line_start.is_none()
            && self.paste_preview.is_none()
            && self.measurement.is_none()
            && self.text_editor.is_none()
            && !matches!(
                &self.gesture,
                Some(Gesture {
                    kind: GestureKind::Erase { .. },
                    ..
                })
            ))
        .then(|| renderer::SceneKey {
            joinery_members: self.joinery_edit_members(),
            joint_preview: self.joint_highlight_preview_key(),
            model_revision,
            camera: camera.clone(),
            size,
            pixels_per_point: ui.ctx().pixels_per_point(),
            display: display.clone(),
            dark: ui.visuals().dark_mode,
            tool: self.tool,
            selected: selected.clone(),
            hover,
            outliner_hover: self.outliner_hover.clone(),
        });
        if !key
            .as_ref()
            .is_some_and(|key| self.scene_cache.repaint(key, &p, rect))
        {
            let emphasized_entities = if let Some(parts) = &joinery_parts {
                parts.iter().flat_map(|part| part.iter().copied()).collect()
            } else if emphasize_group {
                model.expand_entities(&model.edit_context.into_iter().collect())
            } else {
                BTreeSet::new()
            };
            let selected_entities = if self.history_preview.is_some() {
                BTreeSet::new()
            } else if let Some(edit) = self
                .modeling
                .as_ref()
                .filter(|_| self.tool == Tool::PushPull)
            {
                let cap = model
                    .faces
                    .keys()
                    .filter(|id| {
                        !edit.base.faces.contains_key(id)
                            && model.parent(**id) == model.edit_context
                    })
                    .next_back()
                    .copied()
                    .or(edit.face);
                cap.into_iter().collect()
            } else if let Some(edit) = self.modeling.as_ref().filter(|edit| edit.copy) {
                let copies = model
                    .selectable_entities()
                    .into_iter()
                    .filter(|id| !edit.base.contains_entity(id))
                    .collect();
                model.expand_entities(&copies)
            } else {
                model.expand_entities(selected)
            };
            let mut mesh = renderer::Mesh::cached(camera, size, &mut self.scene_cache);
            mesh.set_pixels_per_point(ui.ctx().pixels_per_point());
            mesh.set_display(display);
            mesh.enable_gpu_edge_occlusion();
            mesh.set_annotation_occlusion_enabled(
                !model.dimensions.is_empty()
                    || !model.texts.is_empty()
                    || self.measurement.is_some()
                    || self.text_editor.is_some()
                    || self.paste_preview.is_some(),
            );
            #[cfg(not(target_arch = "wasm32"))]
            let grid_profile = viewport_benchmark::scope("grid");
            if self.doc.display.grid {
                let colors = GridPalette::for_ui(ui);
                for line in grid::visible_lines(camera, &self.doc.display, size) {
                    let color = if line.major { colors.major } else { colors.minor };
                    mesh.background_grid_line(line.points, line.opacity, line.major, color);
                }
            }
            for axis in [Axis::X, Axis::Y, Axis::Z]
                .into_iter()
                .filter(|_| display.axes)
            {
                let v = axis.vector() * renderer::ReferenceFade::new(camera).axis_length();
                mesh.axis_line(
                    camera,
                    -v,
                    DVec3::ZERO,
                    0.85,
                    self.axis_color(axis).gamma_multiply(0.35),
                );
                mesh.axis_line(
                    camera,
                    DVec3::ZERO,
                    v,
                    1.1,
                    self.axis_color(axis).gamma_multiply(0.8),
                );
            }
            #[cfg(not(target_arch = "wasm32"))]
            drop(grid_profile);
            #[cfg(not(target_arch = "wasm32"))]
            let face_profile = viewport_benchmark::scope("faces");
            {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = viewport_benchmark::scope("face_cache");
                self.face_cache.update(model);
            }
            if let Some((prepared_display, mut shadows)) = prepared_shadows {
                // Adopt only the partitions computed with the current shadow
                // and visibility settings, paired with this completed model.
                if prepared_display.shadows == display.shadows
                    && (prepared_display.shadow_darkness > 0.) == (display.shadow_darkness > 0.)
                    && prepared_display.sun_angle == display.sun_angle
                    && prepared_display.sun_elevation == display.sun_elevation
                    && prepared_display.section_limits() == display.section_limits()
                    && prepared_display.hidden_objects == display.hidden_objects
                    && prepared_display.hidden_geometry == display.hidden_geometry
                    && (prepared_display.face_style == ketchup_core::FaceStyle::Wireframe)
                        == (display.face_style == ketchup_core::FaceStyle::Wireframe)
                {
                    shadows.adopt_revision(self.face_cache.revision());
                    self.shadow_cache = shadows;
                }
            }
            {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = viewport_benchmark::scope("shadow_update");
                self.shadow_cache.update(&self.face_cache, entities, display);
            }
            let mut visible_faces: Vec<_> = model
                .faces
                .values()
                .filter(|f| entities.get(f.id).displayed(display))
                .filter(|f| !emphasize_group || emphasized_entities.contains(&f.id))
                .map(|face| {
                    (
                        face,
                        if display.x_ray {
                            view.view_distance(self.face_cache.get(face.id).center)
                        } else {
                            0.
                        },
                    )
                })
                .collect();
            if display.x_ray {
                visible_faces.sort_by(|a, b| b.1.total_cmp(&a.1));
            }
            let lighting = display_scene::FaceLighting::new(camera);
            let erasing = match &self.gesture {
                Some(Gesture {
                    kind: GestureKind::Erase { ids },
                    ..
                }) => Some(ids),
                _ => None,
            };
            let mut face_draws = Vec::new();
            for (face, _) in visible_faces
                .into_iter()
                .filter(|_| display.face_style != ketchup_core::FaceStyle::Wireframe)
            {
                let entity = entities.get(face.id);
                let cached = self.face_cache.get(face.id);
                let normal = cached.normal;
                let tag_color = entity.tag_color;
                let rgb = if display.color_by_tag {
                    tag_color.as_ref()
                } else if display.face_style == ketchup_core::FaceStyle::Materials {
                    model.face_colors.get(&face.id)
                } else {
                    None
                };
                let material = if display.face_style == ketchup_core::FaceStyle::HiddenLine {
                    let bg = display.background;
                    Color32::from_rgb(bg[0], bg[1], bg[2])
                } else if display.face_style == ketchup_core::FaceStyle::Monochrome
                    && normal.dot(camera.toward_eye(model.vertices[&face.vertices[0]].position))
                        < 0.
                {
                    faces.back
                } else if let Some(rgb) = rgb {
                    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
                } else {
                    faces.front
                };
                let outliner_hover = self.outliner_hover.contains(&face.id);
                let color = if entity.context.is_none() && !outliner_hover {
                    faces.inactive
                } else if erasing.is_some_and(|ids| {
                    entity
                        .context
                        .is_some_and(|id| ids.contains(&id))
                }) {
                    Color32::from_rgb(220, 91, 76)
                } else if self.tool == Tool::Paint
                    && model.face_colors.contains_key(&face.id)
                    && !outliner_hover
                {
                    material
                } else if let Some(color) = joinery_color(face.id) {
                    // Tint the material softly; edge colors retain full strength.
                    Color32::from_rgb(
                        ((material.r() as u16 * 3 + color.r() as u16) / 4) as u8,
                        ((material.g() as u16 * 3 + color.g() as u16) / 4) as u8,
                        ((material.b() as u16 * 3 + color.b() as u16) / 4) as u8,
                    )
                } else if selected_entities.contains(&face.id) {
                    faces.selected
                } else if hover.is_some() && hover == entity.context
                    || hover == Some(face.id)
                    || outliner_hover
                {
                    faces.hovered
                } else {
                    material
                };
                let color = if display.face_style == ketchup_core::FaceStyle::HiddenLine {
                    color
                } else {
                    lighting.shade(color, normal, model.vertices[&face.vertices[0]].position)
                };
                let color = if display.x_ray || entity.entity_hidden {
                    color.gamma_multiply(0.18)
                } else {
                    color
                };
                face_draws.push(renderer::FaceDraw {
                    face, cached, color,
                    shadow: self.shadow_cache.get(face.id).map(|shadow|
                        (shadow, display_scene::shadow_color(color, display.shadow_darkness))),
                });
            }
            {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = viewport_benchmark::scope("face_emit");
                mesh.draw_faces(&face_draws);
            }
            mesh.set_entity(None);
            display_scene::paint_sections(
                &mut mesh,
                &mut self.section_cache,
                camera,
                model,
                &self.face_cache,
                entities,
                display,
                &faces,
            );
            #[cfg(not(target_arch = "wasm32"))]
            drop(face_profile);
            let profiles = {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = viewport_benchmark::scope("profile_edges");
                if display.profiles && !emphasize_group {
                display_scene::profile_edges(model, camera, &self.face_cache, entities, display)
            } else {
                BTreeSet::new()
                }
            };
            // Resolve ordinary edges in f64 before drawing selected strokes.
            // GPU depth alone can round nearby edges to the same value, letting
            // the wider selection color overwrite a nearer object's outline.
            for selected_pass in [false, true] {
                if selected_pass {
                    mesh.begin_selected_edges();
                    if selected_entities.is_empty() { continue; }
                }
                #[cfg(not(target_arch = "wasm32"))]
                let style_profile = viewport_benchmark::scope("edge_styles");
                let mut edge_draws = Vec::new();
                for e in model
                    .edges
                    .values()
                    .filter(|e| entities.get(e.id).displayed(display))
                    .filter(|e| selected_entities.contains(&e.id) == selected_pass)
                {
                    let entity = entities.get(e.id);
                    let selected = selected_entities.contains(&e.id);
                    let emphasized = emphasized_entities.contains(&e.id);
                    let root = entity.context;
                    let outliner_hover = self.outliner_hover.contains(&e.id);
                    let hover = outliner_hover || hover.is_some() && hover == root;
                    let pending =
                        erasing.is_some_and(|ids| root.is_some_and(|id| ids.contains(&id)));
                    let color = if emphasize_group && !emphasized {
                        colors.muted.gamma_multiply(0.55)
                    } else if root.is_none() && !outliner_hover {
                        if ui.visuals().dark_mode {
                            colors.border
                        } else {
                            Color32::from_rgb(184, 193, 181)
                        }
                    } else if pending {
                        Color32::from_rgb(220, 91, 76)
                    } else if entity.locked {
                        Color32::from_rgb(90, 133, 176)
                    } else if let Some(color) = joinery_color(e.id) {
                        color
                    } else if selected {
                        ACCENT
                    } else if hover && self.tool == Tool::Erase && !outliner_hover {
                        ACCENT
                    } else if hover {
                        colors.green
                    } else {
                        colors.ink
                    };
                    let (a, b) = model.points(e);
                    // Emphasis changes the surrounding geometry, preserving interaction feedback.
                    let active = selected || hover || pending;
                    let width = if emphasize_group && !emphasized {
                        1.
                    } else if active {
                        3.
                    } else if profiles.contains(&e.id) {
                        display.profile_width
                    } else {
                        1.8
                    };
                    let width = if display.depth_cue && !active && !emphasize_group {
                        width
                            * (camera.distance / view.view_distance((a + b) * 0.5).max(0.01))
                                .clamp(0.35, 1.8)
                    } else {
                        width
                    };
                    let color =
                        if display.color_by_tag && !active && (!emphasize_group || emphasized) {
                            entity
                                .tag_color
                                .map(|c| Color32::from_rgb(c[0], c[1], c[2]))
                                .unwrap_or(color)
                        } else {
                            color
                        };
                    edge_draws.push(renderer::EdgeDraw {
                        id: e.id,
                        key: (e.a.min(e.b), e.a.max(e.b)),
                        points: [a, b], width, color,
                        back: display.back_edges || entity.entity_hidden,
                        visible: !entity.entity_hidden && (display.edges || emphasize_group
                            || profiles.contains(&e.id) || active
                            || display.face_style == ketchup_core::FaceStyle::Wireframe),
                        overlay: display.x_ray || display.face_style == ketchup_core::FaceStyle::Wireframe,
                    });
                }
                #[cfg(not(target_arch = "wasm32"))]
                drop(style_profile);
                mesh.draw_edges(&edge_draws, display.extensions, display.endpoints);
            }
            if let (Some(start), Some(snap)) = (self.line_start, self.line_preview_snap()) {
                let color = self
                    .preview_alignment()
                    .or_else(|| self.active_drawing_guide().map(|g| g.axis))
                    .map(|axis| self.axis_color(axis))
                    .unwrap_or(colors.ink);
                mesh.world_line(camera, start, snap.point, 2.2, color);
            }
            for pair in self.drawing_points().windows(2) {
                mesh.world_line(
                    camera,
                    pair[0],
                    pair[1],
                    2.2,
                    self.lock
                        .map(|axis| self.axis_color(axis))
                        .unwrap_or(ACCENT),
                );
            }
            if let Some(preview) = &self.paste_preview {
                self.paste_face_cache.update(&preview.clipboard.model);
                let offset = preview.point - preview.clipboard.anchor;
                for face in preview
                    .clipboard
                    .model
                    .faces
                    .values()
                    .filter(|f| !preview.clipboard.model.is_hidden(f.id))
                {
                    for triangle in &self.paste_face_cache.get(face.id).triangles {
                        mesh.world_triangle(camera, triangle.map(|p| p + offset), faces.paste);
                    }
                }
                for edge in preview
                    .clipboard
                    .model
                    .edges
                    .values()
                    .filter(|e| !preview.clipboard.model.is_hidden(e.id))
                {
                    let (a, b) = preview.clipboard.model.points(edge);
                    mesh.world_line(camera, a + offset, b + offset, 2.4, colors.green);
                }
            }
            mesh.set_entity(None);
            self.paint_joint_highlights(&mut mesh,model,display,ui.visuals().dark_mode);
            let annotations = {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = viewport_benchmark::scope("annotations");
                mesh.occlude_annotations_with_edges();
                let mut shapes = self.dimension_shapes(&mut mesh, &p, rect, model);
                shapes.extend(self.text_shapes(&mut mesh, &p, rect, model));
                shapes
            };
            {
                #[cfg(not(target_arch = "wasm32"))]
                let _profile = viewport_benchmark::scope("mesh_prepare");
                mesh.paint_cached(&p, rect, &mut self.scene_cache);
            }
            p.extend(annotations.iter().cloned());
            self.scene_cache.remember_annotations(&p, rect, annotations);
            self.scene_cache.key = key;
        }
        self.paint_fasteners(&p,rect,model);
        self.paint_joinery(&p, rect);
        let screen = |point: DVec3| view.project(point).map(|v| rect.min + pv(v));
        self.picking_cache.set_display(display);
        self.picking_cache.update(&self.face_cache);
        if display.group_names {
            #[cfg(not(target_arch = "wasm32"))]
            let _profile = viewport_benchmark::scope("group_labels");
            let markers = self.group_marker_cache.update(model, model_revision);
            for marker in markers.iter() {
                let Some(object) = model.objects.get(&marker.id).filter(|object| {
                    matches!(object.kind, ketchup_core::ObjectKind::Group)
                        && entities.get(object.id).context == Some(object.id)
                        && !object.name.trim().is_empty()
                        && entities.get(object.id).displayed(display)
                }) else { continue; };
                if let Some(position) = self.picking_cache.marker_visible(
                    &self.face_cache, entities, camera, size, marker.point, &marker.owners,
                ).then(|| screen(marker.point)).flatten() {
                    if rect.contains(position) {
                        let is_selected = selected.contains(&object.id);
                        let foreground = if is_selected { Color32::WHITE } else { colors.ink };
                        let background = if is_selected { ACCENT } else { colors.paper };
                        let galley = p.layout_no_wrap(
                            object.name.clone(), egui::FontId::proportional(13.), foreground,
                        );
                        let label = egui::Rect::from_center_size(position, galley.size());
                        p.rect_filled(label.expand2(egui::vec2(5., 3.)), 3., background);
                        p.galley(label.min, galley, foreground);
                    }
                }
            }
        }

        if let Some((lo, hi)) = self.face_cache.context_bounds(model, entities) {
            let padding = ((hi - lo).max_element() * 0.04).max(0.05);
            let lo = lo - DVec3::splat(padding);
            let hi = hi + DVec3::splat(padding);
            let corner = |i: usize| {
                DVec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                )
            };
            for i in 0..8 {
                for axis in 0..3 {
                    if i & (1 << axis) != 0 {
                        continue;
                    }
                    if let Some((a, b)) = camera.segment(corner(i), corner(i | (1 << axis)), size) {
                        p.extend(egui::Shape::dashed_line(
                            &[rect.min + pv(a), rect.min + pv(b)],
                            Stroke::new(1., colors.muted),
                            4.,
                            4.,
                        ));
                    }
                }
            }
        }
        if display.axes {
            // Reuse the rendered geometry, including a live modeling preview.
            // A full membership/face scan here makes orbiting grouped imports
            // dramatically slower whenever axes are enabled.
            if let Some(origin) = screen(DVec3::ZERO) {
                if self
                    .picking_cache
                    .visible(&self.face_cache, entities, camera, size, DVec3::ZERO)
                {
                    p.circle_filled(origin, 2.5, colors.muted);
                }
            }
        }
        for id in selected {
            if let Some(e) = model.edges.get(id) {
                let (a, b) = model.points(e);
                for pt in [a, b] {
                    if let Some(point) = screen(pt) {
                        p.circle_filled(point, 3., colors.paper);
                        p.circle_stroke(point, 3., Stroke::new(1.3, colors.accent));
                    }
                }
            }
        }
        if self.history_preview.is_some() {
            return;
        }
        let guide = self
            .active_drawing_guide()
            .map(|g| (g.origin, g.axis))
            .or_else(|| {
                if !self.tool.is_drawing() {
                    return None;
                }
                match self.line_preview_snap()?.kind {
                    Inference::OnAxis(axis) => Some((DVec3::ZERO, axis)),
                    Inference::Axis(axis) => self
                        .line_start
                        .or_else(|| self.drawing.as_ref().map(|d| d.origin))
                        .map(|origin| (origin, axis)),
                    _ => self
                        .preview_alignment()
                        .zip(self.line_start)
                        .map(|(axis, start)| (start, axis)),
                }
            });
        let combined: Vec<_> = self
            .active_drawing_axis_guides()
            .map(|g| (g.origin, g.axis))
            .collect();
        for (origin, axis) in combined
            .iter()
            .copied()
            .chain(guide.filter(|_| combined.is_empty()))
            .filter(|_| display.guides)
        {
            let extent = axis.vector() * camera.distance * 5.;
            if let Some((a, b)) = camera.segment(origin - extent, origin + extent, size) {
                let a = rect.min + pv(a);
                let b = rect.min + pv(b);
                let length = a.distance(b);
                for step in 0..(length / 10.).ceil() as usize {
                    let from = (step as f32 * 10. / length).min(1.);
                    let to = ((step as f32 * 10. + 4.) / length).min(1.);
                    p.line_segment(
                        [a.lerp(b, from), a.lerp(b, to)],
                        Stroke::new(1., self.axis_color(axis).gamma_multiply(0.55)),
                    );
                }
            }
            if self.drawing_guide.is_some() || !combined.is_empty() {
                if let Some(point) = screen(origin) {
                    p.circle_filled(point, 3., colors.paper);
                    p.circle_stroke(point, 3., Stroke::new(1.5, self.axis_color(axis)));
                }
            }
        }
        if let Some((source, target)) = self.active_drawing_projection().filter(|_| display.guides)
        {
            let color = self
                .lock
                .map(|axis| self.axis_color(axis))
                .unwrap_or_else(|| self.inference_color());
            if let Some((a, b)) = camera.segment(source, target, size) {
                p.extend(egui::Shape::dashed_line(
                    &[rect.min + pv(a), rect.min + pv(b)],
                    Stroke::new(1.2, color),
                    4.,
                    4.,
                ));
            }
            if let Some(point) = screen(source) {
                p.circle_filled(point, 3., colors.paper);
                p.circle_stroke(point, 3., Stroke::new(1.5, color));
            }
        }
        if matches!(self.tool, Tool::PushPull | Tool::Offset)
            && let Some(edit) = &self.modeling
            && let Some(snap) = edit.snap
        {
            let color = self.inference_color();
            if display.guides {
                let end = edit.anchor + edit.direction * edit.value;
                if let Some((a, b)) = camera.segment(snap.point, end, size) {
                    p.extend(egui::Shape::dashed_line(
                        &[rect.min + pv(a), rect.min + pv(b)],
                        Stroke::new(1.2, color),
                        4.,
                        4.,
                    ));
                }
            }
        }
        let modeling_anchor = matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale | Tool::Offset | Tool::PushPull)
            && self.modeling.is_none()
            && self.paste_preview.is_none()
            && !self.navigating()
            && p.ctx().input(|i| i.pointer.hover_pos()).is_some_and(|pos| rect.contains(pos));
        let modeling_start = self.modeling.as_ref()
            .filter(|_| matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale | Tool::Offset | Tool::PushPull))
            .map(|edit| edit.anchor);
        if self.tool.is_drawing() || self.tool == Tool::Text || modeling_anchor || modeling_start.is_some() {
            if let Some(start) = self.line_start.or(self.drawing.as_ref().map(|d| d.origin)).or_else(|| {
                (self.tool == Tool::Text).then(|| self.measurement.as_ref()?.points.first().copied()).flatten()
            }).or(modeling_start).filter(|_| !modeling_anchor) {
                if let Some(point) = screen(start) {
                    p.circle_filled(point, 3.5, colors.paper);
                    p.circle_stroke(point, 3.5, Stroke::new(1.5, colors.ink));
                }
            }
            let live_snap = if modeling_start.is_some() {
                self.modeling_preview_snap()
            } else {
                self.line_preview_snap()
            };
            if let Some(snap) = live_snap {
                if let Some(point) = screen(snap.point) {
                    let color = match snap.kind {
                        Inference::Axis(a) | Inference::OnAxis(a) => self.axis_color(a),
                        Inference::Midpoint => rgb(self.doc.display.midpoint_color),
                        Inference::Endpoint => rgb(self.doc.display.endpoint_color),
                        Inference::OnEdge => rgb(self.doc.display.on_edge_color),
                        Inference::OnFace => rgb(self.doc.display.on_face_color),
                        _ => self.inference_color(),
                    };
                    let color = self
                        .lock
                        .filter(|_| self.tool != Tool::Line && !modeling_anchor)
                        .map(|axis| self.axis_color(axis))
                        .unwrap_or(color);
                    match snap.kind {
                        Inference::Midpoint => {
                            p.add(egui::Shape::convex_polygon(
                                vec![
                                    point + egui::vec2(0., -5.),
                                    point + egui::vec2(-5., 4.),
                                    point + egui::vec2(5., 4.),
                                ],
                                colors.paper,
                                Stroke::new(1.5, color),
                            ));
                        }
                        Inference::Endpoint | Inference::Origin => {
                            p.rect_filled(
                                Rect::from_center_size(point, egui::vec2(7., 7.)),
                                1,
                                colors.paper,
                            );
                            p.rect_stroke(
                                Rect::from_center_size(point, egui::vec2(7., 7.)),
                                1,
                                Stroke::new(1.5, color),
                                egui::StrokeKind::Inside,
                            );
                        }
                        _ => {
                            p.circle_stroke(point, 4., Stroke::new(1.5, color));
                        }
                    }
                    let label = if modeling_anchor {
                        snap.kind.label().into()
                    } else if let Some(normal) = self.plane_lock() {
                        format!("{} plane · Locked", match normal {
                            Axis::X => "YZ", Axis::Y => "XZ", Axis::Z => "XY",
                        })
                    } else if let Some(axis) = self.lock {
                        if self.tool.is_drawing() && self.tool != Tool::Line {
                            format!("{} · Plane locked", axis.label())
                        } else {
                            format!("{} · Locked", axis.label())
                        }
                    } else {
                        snap.kind.label().into()
                    };
                    let text = badge_text(color);
                    let galley = p.layout_no_wrap(label, FontId::proportional(11.), text);
                    let at = point + if modeling_anchor || modeling_start.is_some() {
                        egui::vec2(45., 18.)
                    } else {
                        egui::vec2(14., 16.)
                    };
                    let badge = Rect::from_min_size(
                        at - egui::vec2(6., 4.),
                        galley.size() + egui::vec2(12., 8.),
                    );
                    p.rect_filled(badge, badge.height() / 2., color);
                    p.galley(at, galley, text);
                    if let Some(start) = self.line_start {
                        if let Some(a) = screen(start) {
                            // Keep the length clear of the endpoint's inference
                            // tooltip, including short vertical previews.
                            let offset = if (point.x - a.x).abs() < (point.y - a.y).abs() {
                                egui::vec2(-44., 0.)
                            } else {
                                egui::vec2(0., -16.)
                            };
                            let at = a.lerp(point, 0.5) + offset;
                            p.text(
                                at,
                                Align2::CENTER_CENTER,
                                self.doc
                                    .display
                                    .units
                                    .format_length(start.distance(snap.point)),
                                FontId::monospace(11.),
                                colors.ink,
                            );
                        }
                    }
                }
            } else if modeling_anchor
                && matches!(self.tool, Tool::PushPull | Tool::Offset)
                && let Some(point) = p.ctx().input(|i| i.pointer.hover_pos())
            {
                // Above the drawing-plane horizon there is no world target,
                // but the cursor's anchor marker must remain visible.
                p.circle_stroke(point, 4., Stroke::new(1.5, self.inference_color()));
            }
        }
        if let Some(Gesture {
            start,
            last,
            kind: GestureKind::Select { .. },
            ..
        }) = &self.gesture
        {
            if start.distance(*last) > 4. {
                let r = Rect::from_two_pos(*start, *last);
                let color = Color32::from_rgb(89, 136, 173);
                p.rect_filled(r, 0, color.gamma_multiply(0.1));
                p.rect_stroke(r, 0, Stroke::new(1., color), egui::StrokeKind::Inside);
            }
        }
        if let Some(edit) = &self.modeling {
            let text = edit.error.map(str::to_owned).unwrap_or_else(|| {
                let value = precision::modeling_value_with_units(
                    self.tool,
                    &self.length_input,
                    self.doc.display.units,
                )
                .unwrap_or(edit.value);
                if self.tool == Tool::Rotate {
                    if edit.rotation.as_ref().is_some_and(|rotation| rotation.reference.is_none()) {
                        "Click a reference point away from the pivot · Esc to cancel".into()
                    } else {
                        format!("{} · Click to confirm · Esc to cancel", self.doc.display.units.format_angle(value))
                    }
                } else if self.tool == Tool::Scale {
                    format!("{value:.3} × · Click to confirm · Esc to cancel")
                } else {
                    let alignment = if edit.guides.len() > 1 && self.length_input.is_empty() {
                        format!(
                            " · {} aligned",
                            edit.guides
                                .iter()
                                .map(|g| match g.axis {
                                    Axis::X => "X",
                                    Axis::Y => "Y",
                                    Axis::Z => "Z",
                                })
                                .collect::<Vec<_>>()
                                .join(" + ")
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "{}{}{alignment} · Click to confirm · Esc to cancel",
                        if self.tool == Tool::Move {
                            if edit.copy { "Copy · " } else { "Move · " }
                        } else {
                            ""
                        },
                        self.doc.display.units.format_length(value)
                    )
                }
            });
            let galley = p.layout_no_wrap(
                text,
                FontId::monospace(12.),
                if edit.error.is_some() {
                    colors.accent
                } else {
                    colors.ink
                },
            );
            let at = rect.left_bottom() + egui::vec2(20., -40.);
            p.rect_filled(
                Rect::from_min_size(
                    at - egui::vec2(8., 6.),
                    galley.size() + egui::vec2(16., 12.),
                ),
                5,
                colors.paper,
            );
            p.galley(at, galley, colors.ink);
            if self.tool == Tool::Rotate {
                self.paint_rotation(&p, rect);
            }
            if matches!(self.tool, Tool::Move | Tool::Scale) {
                if display.guides && self.length_input.is_empty() {
                    for guide in &edit.guides {
                        if let Some((a, b)) =
                            camera.segment(guide.origin, edit.anchor + edit.direction, size)
                        {
                            let (a, b) = (rect.min + pv(a), rect.min + pv(b));
                            let length = a.distance(b);
                            for step in 0..(length / 10.).ceil() as usize {
                                p.line_segment(
                                    [
                                        a.lerp(b, (step as f32 * 10. / length).min(1.)),
                                        a.lerp(b, ((step as f32 * 10. + 5.) / length).min(1.)),
                                    ],
                                    Stroke::new(1.5, self.axis_color(guide.axis)),
                                );
                            }
                            p.circle_stroke(a, 3., Stroke::new(1., self.axis_color(guide.axis)));
                        }
                    }
                }
                if let (Some(a), Some(b)) =
                    (screen(edit.anchor), screen(edit.anchor + edit.direction))
                {
                    p.line_segment(
                        [a, b],
                        Stroke::new(
                            1.5,
                            self.lock
                                .map(|axis| self.axis_color(axis))
                                .unwrap_or_else(|| self.inference_color()),
                        ),
                    );
                }
            }
        }
        self.paint_solid_tools(&p, rect);
        self.paint_measurements(&p, rect);
        if let Some((notice, until)) = &self.notice {
            if ui.input(|i| i.time) < *until {
                let galley = p.layout(
                    notice.clone(),
                    FontId::proportional(12.),
                    colors.ink,
                    (rect.width() - 80.).clamp(120., 600.),
                );
                let r = Rect::from_min_size(
                    rect.left_top() + egui::vec2(20., 20.),
                    galley.size() + egui::vec2(24., 20.),
                );
                p.rect_filled(r, 7, colors.paper);
                p.rect_stroke(
                    r,
                    7,
                    Stroke::new(1., colors.border),
                    egui::StrokeKind::Inside,
                );
                p.galley(r.min + egui::vec2(12., 10.), galley, colors.ink);
            }
        }
    }
    pub(super) fn edit_context_bar(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        let path = self.doc.model.edit_path();
        let mut destination = None;
        ui.horizontal_centered(|ui| {
            if path.is_empty() {
                ui.text_label(RichText::new("Model").size(13.).strong());
                ui.text_label(RichText::new("/").color(colors.border));
                ui.text_label(RichText::new("No group open").size(12.).color(colors.muted));
            } else {
                if ui.small_button("Model").clicked() {
                    destination = Some(None);
                }
                for id in &path {
                    ui.text_label(RichText::new("›").color(colors.muted));
                    if ui.small_button(&self.doc.model.objects[id].name).clicked() {
                        destination = Some(Some(*id));
                    }
                }
                ui.add_space(10.);
                if ui.small_button("Close level  (Esc)").clicked() {
                    destination = Some(
                        self.doc
                            .model
                            .edit_context
                            .and_then(|id| self.doc.model.parent(id)),
                    );
                }
            }
        });
        if let Some(context) = destination {
            self.navigate_context(context, ui.ctx());
        }
    }
    pub(super) fn drawing_lock_buttons(&mut self, ui: &mut egui::Ui) {
        let enabled = self.tool.is_drawing()
            || matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale | Tool::PushPull | Tool::Offset)
            || self.tool.is_measuring();
        let colors = Palette::for_ui(ui);
        // The viewport toolbar flows right to left: show X, Y, then Z.
        for (axis, label) in [
            (Axis::Z, "Toggle Z axis lock (↑). Select up to two axes to lock their plane; ↓ clears all locks. Shapes and Rotate use a single lock as their plane normal."),
            (Axis::Y, "Toggle Y axis lock (←). Select up to two axes to lock their plane; ↓ clears all locks. Shapes and Rotate use a single lock as their plane normal."),
            (Axis::X, "Toggle X axis lock (→). Select up to two axes to lock their plane; ↓ clears all locks. Shapes and Rotate use a single lock as their plane normal."),
        ] {
            let active = self.lock == Some(axis) || self.second_lock == Some(axis);
            let available = enabled && (active || self.second_lock.is_none());
            let response = ui.add_enabled(
                available,
                egui::Button::new("")
                    .min_size(egui::vec2(32., 32.))
                    .selected(active),
            );
            let opacity = if available { 1. } else { 0.3 };
            icons::axis_lock(
                ui.painter(),
                response.rect.center(),
                axis,
                active,
                self.axis_color(axis).gamma_multiply(opacity),
                colors.muted.gamma_multiply(opacity),
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, available, active, label)
            });
            if response.on_hover_text(label).clicked() {
                self.toggle_axis_lock(axis, ui.ctx());
            }
        }
    }
    pub(super) fn snapping_menu(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        let response = ui
            .add(egui::Button::new("").min_size(egui::vec2(48., 32.)))
            .on_hover_text("Configure edge, face, face center, axis, and grid snapping");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Snapping"));
        icons::line_icon(
            ui.painter(),
            egui::pos2(response.rect.left() + 16., response.rect.center().y),
            "magnet",
            Color32::from_rgb(67, 125, 208),
        );
        icons::dropdown_chevron(
            ui.painter(),
            egui::pos2(response.rect.left() + 36., response.rect.center().y),
            colors.muted,
        );
        egui::Popup::menu(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                Self::menu_style(ui.style_mut());
                let edges_changed = Self::menu_checkbox(ui, &mut self.snap_to_edge, "Snap to edge")
                    .on_hover_text("Snap to object edges, including their endpoints and midpoints.")
                    .changed();
                let faces_changed = Self::menu_checkbox(ui, &mut self.snap_to_face, "Snap to face")
                    .on_hover_text("Snap to object surfaces, independently of edge and face center snapping.")
                    .changed();
                let face_centers_changed = Self::menu_checkbox(ui, &mut self.snap_to_face_center, "Snap to face center")
                    .on_hover_text("Snap to the area center of a visible face, independently of edge and surface snapping.")
                    .changed();
                let axes_changed = Self::menu_checkbox(ui, &mut self.doc.snap_to_axes, "Snap to axes")
                    .on_hover_text("Automatically align drawing with the red, green, and blue axes. Hover an endpoint, midpoint, or edge to acquire axes through that position. Arrow keys can still lock an axis when this is off.")
                    .changed();
                let grid_changed = Self::menu_checkbox(ui, &mut self.snap_to_grid, "Snap to grid")
                    .on_hover_text("Snap points and moves to the grid spacing at the current zoom. Object targets and axis locks take precedence.")
                    .changed();
                if edges_changed || faces_changed || face_centers_changed {
                    self.sync_geometry_snapping();
                }
                if axes_changed || grid_changed || edges_changed || faces_changed || face_centers_changed {
                    self.drawing_reference = None;
                    self.drawing_axis_guides.clear();
                    self.snap = None;
                    if axes_changed {
                        self.changed(ui.ctx());
                    }
                    if self.tool == Tool::Scale {
                        self.refresh_axis_lock(ui.ctx());
                    }
                }
            });
    }
    pub(super) fn group_emphasis_button(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        let response = ui.add(
            egui::Button::new("")
                .min_size(egui::vec2(32., 32.))
                .selected(self.doc.display.group_emphasis),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                ui.is_enabled(),
                self.doc.display.group_emphasis,
                "Emphasize group",
            )
        });
        icons::group_emphasis(
            ui.painter(),
            response.rect.center(),
            if self.doc.display.group_emphasis {
                colors.accent
            } else {
                colors.muted
            },
            colors.muted,
        );
        if response.on_hover_text("Emphasize group: emphasize the group/component currently being edited and show everything else in muted wireframe. Has no visual effect outside a group/component.").clicked() {
            self.doc.display.group_emphasis = !self.doc.display.group_emphasis;
            ui.ctx().request_repaint();
        }
    }
    pub(super) fn compass_center(rect: Rect) -> Option<Pos2> {
        (rect.width() >= 90. * COMPASS_SCALE && rect.height() >= 90. * COMPASS_SCALE)
            .then(|| rect.right_bottom() - egui::vec2(26. + COMPASS_RADIUS, 23. + COMPASS_RADIUS))
    }

    pub(super) fn compass_contains(&self, pos: Pos2) -> bool {
        Self::compass_center(self.viewport)
            .is_some_and(|center| center.distance(pos) <= COMPASS_RADIUS)
    }

    pub(super) fn compass_axes(&self, center: Pos2) -> [(CompassAxis, Pos2); 6] {
        let camera = &self.doc.camera;
        // The camera's analytic basis also works at exact top and bottom views.
        let right = camera.right();
        let up = camera.up();
        let toward_eye = (camera.eye() - camera.target).normalize();
        let mut axes: [CompassAxis; 6] = std::array::from_fn(|index| CompassAxis {
            axis: [Axis::X, Axis::Y, Axis::Z][index % 3],
            negative: index >= 3,
        });
        axes.sort_by(|a, b| {
            a.direction()
                .dot(toward_eye)
                .total_cmp(&b.direction().dot(toward_eye))
        });
        axes.map(|axis| {
            let direction = axis.direction();
            let end = center
                + egui::vec2(
                    direction.dot(right) as f32 * 23. * COMPASS_SCALE,
                    -direction.dot(up) as f32 * 23. * COMPASS_SCALE,
                );
            (axis, end)
        })
    }

    pub(super) fn compass_axis_at(&self, pos: Pos2) -> Option<CompassAxis> {
        let center = Self::compass_center(self.viewport)?;
        // Positive tips win overlaps; ties keep the frontmost painted tip.
        self.compass_axes(center)
            .into_iter()
            .rev()
            .filter(|(_, end)| end.distance(pos) <= COMPASS_TIP_RADIUS)
            .min_by_key(|(axis, _)| axis.negative)
            .map(|(axis, _)| axis)
    }

    pub(super) fn paint_compass(&self, ui: &egui::Ui, response: &egui::Response) {
        let Some(center) = Self::compass_center(self.viewport) else {
            return;
        };
        let p = ui.painter().with_clip_rect(self.viewport);
        let colors = Palette::for_ui(ui);
        let pointer = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| response.enabled() && response.contains_pointer());
        if pointer.is_some_and(|pos| self.compass_contains(pos)) {
            p.circle_filled(center, COMPASS_RADIUS, colors.paper.gamma_multiply(0.45));
        }
        let hovered_axis = pointer.and_then(|pos| self.compass_axis_at(pos));
        if self.gesture.as_ref().is_some_and(|gesture| {
            matches!(gesture.kind, GestureKind::Compass { dragged: true, .. })
        }) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if hovered_axis.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if pointer.is_some_and(|pos| self.compass_contains(pos)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        let axes = self.compass_axes(center);
        for (axis, end) in axes {
            if !axis.negative {
                p.line_segment(
                    [center, end],
                    Stroke::new(1.6 * COMPASS_SCALE, self.axis_color(axis.axis)),
                );
            }
        }
        for (axis, end) in axes {
            let color = self.axis_color(axis.axis);
            let hovered = hovered_axis == Some(axis);
            p.circle(
                end,
                COMPASS_TIP_RADIUS,
                if axis.negative && !hovered {
                    color.gamma_multiply(0.2)
                } else {
                    color
                },
                if axis.negative {
                    Stroke::new(COMPASS_SCALE, color.gamma_multiply(0.7))
                } else {
                    Stroke::NONE
                },
            );
            if hovered {
                p.circle_stroke(
                    end,
                    8.5 * COMPASS_SCALE,
                    Stroke::new(COMPASS_SCALE, colors.ink),
                );
            }
            if !axis.negative || hovered {
                p.text(
                    end,
                    Align2::CENTER_CENTER,
                    axis.label(),
                    FontId::proportional(9. * COMPASS_SCALE),
                    PAPER,
                );
            }
        }
    }

    pub(super) fn viewport_panel(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_ui(ui);
        let ctx = ui.ctx().clone();
        let [r, g, b] =
            crate::environment::for_theme(&self.doc.display, ui.visuals().dark_mode).background;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(r, g, b)))
            .show(ui, |ui| {
                egui::Panel::top("view_options")
                    .exact_size(47.)
                    .frame(
                        egui::Frame::new()
                            .fill(colors.paper)
                            .inner_margin(egui::Margin::symmetric(18, 7)),
                    )
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if Self::icon_button(
                                        ui,
                                        "panel",
                                        "Toggle model inspector (Tab)",
                                        true,
                                        colors.muted,
                                    ) {
                                        self.inspector = !self.inspector;
                                    }
                                    let dark = ctx.theme() == egui::Theme::Dark;
                                    if Self::icon_button(
                                        ui,
                                        if dark { "sun" } else { "moon" },
                                        if dark {
                                            "Switch to light mode"
                                        } else {
                                            "Switch to dark mode"
                                        },
                                        true,
                                        colors.muted,
                                    ) {
                                        ctx.set_theme(if dark {
                                            egui::Theme::Light
                                        } else {
                                            egui::Theme::Dark
                                        });
                                        ctx.request_repaint();
                                    }
                                    if Self::icon_button(
                                        ui,
                                        "fit",
                                        "Fit model to view",
                                        true,
                                        colors.muted,
                                    ) {
                                        self.fit(&ctx);
                                    }
                                    if Self::icon_button(
                                        ui,
                                        "origin",
                                        "Center view on axis origin (0, 0, 0)",
                                        true,
                                        colors.muted,
                                    ) {
                                        self.fit_origin(&ctx);
                                    }
                                    ui.separator();
                                    self.drawing_lock_buttons(ui);
                                    ui.separator();
                                    self.snapping_menu(ui);
                                    self.group_emphasis_button(ui);
                                    // This row flows right to left: Sections sits left of this separator.
                                    ui.separator();
                                    self.sections_menu(ui);
                                    self.paint_menu(ui);
                                    ui.allocate_ui_with_layout(
                                        ui.available_size(),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            self.viewport_tabs(ui);
                                        },
                                    );
                                },
                            );
                        });
                    });
                self.viewport_workspace(ui);
            });
    }
}
