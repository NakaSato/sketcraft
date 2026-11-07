//! The 3D viewport: navigation, GPU (or CPU fallback) rendering, picking, overlays, the axis
//! view and the navigation bar.

use crate::ui_kit::BackingTooltipExt as _;
use crate::ui_kit::{Container, Surface};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use serde_json::json;
use solidcore_record::{Plane, Vec2, Vec3};
use sketcraft_engine::Sel;
use sketcraft_engine::geom::prelude::*;
use sketcraft_engine::render::{Camera, Mat4, Rgb};
use sketcraft_engine::sketch::CurveKind;
use sketcraft_engine::view::{GridFade, GridPlane, colors, grid_ext, grid_lines, grid_plane, grid_step, sketch_lines_with_sampler};
use std::hash::{Hash, Hasher};

use crate::gpu::{Frame, GpuScene, GpuTarget, SceneSlot, ViewportCallback};
use crate::navigation::{CameraInput, NavCaps, orbit_drag};
use crate::selection::{BoxSel, origin_quads, origin_size, ray_quad};
use crate::theme::Tokens;
use crate::{SolveApp, icons};

#[cfg(test)]
#[path = "viewport_appearance_tests.rs"]
mod appearance_tests;

#[cfg(test)]
#[path = "viewport_navigation_cache_tests.rs"]
mod navigation_cache_tests;

#[cfg(test)]
#[path = "viewport_overlays_tests.rs"]
mod overlays_tests;

#[cfg(test)]
#[path = "viewport_removed_later_tests.rs"]
mod removed_later_tests;

#[cfg(test)]
#[path = "viewport_plane_target_tests.rs"]
mod plane_target_tests;

#[cfg(test)]
#[path = "viewport_ground_tests.rs"]
mod ground_tests;

#[cfg(test)]
#[path = "viewport_refine_wait_tests.rs"]
mod refine_wait_tests;

#[path = "body_gizmo.rs"]
mod body_gizmo;

#[path = "navigation_cube.rs"]
mod navigation_cube;

#[path = "viewport_drag_layer.rs"]
mod drag_layer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMode {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Default)]
pub struct ViewportState {
    /// The faces the modelling kernel refuses that the window points at (a failed operation's
    /// message, a "needs repair" row under the pointer): ringed in the error colour. What this
    /// frame asks for ([`crate::refused_part::ring`]) is drawn from the next one on, so the ring
    /// goes when nothing asks for it any more.
    pub(crate) refused_ring: Vec<(String, usize)>,
    pub(crate) refused_ring_next: Vec<(String, usize)>,
    /// A drag on the handles of the drawing being placed (Insert SVG / DXF).
    pub(crate) insert_drag: Option<crate::insert_handles::Drag>,
    /// A drag on the handles of the Transform window's box.
    pub(crate) transform_drag: Option<crate::sketch_transform_ui::Drag>,
    /// The arrow turning the view that is still down (`navigation::step_key_hold`).
    pub(crate) key_hold: Option<crate::navigation::KeyHold>,
    /// A two-point drawing tool's (by preference also the Line tool's, #7247) first point was
    /// placed by a press (here) that became a drag: the release places the second (#1630), unless
    /// it lands within [`DRAW_DRAG_MIN`] of the press. Such a drag was a click that moved on the
    /// way (#2448): the release places the first point instead and the shape goes on by clicks,
    /// rather than a sliver of a shape.
    pub(crate) draw_drag: Option<Pos2>,
    /// The draw-drag release above already placed or kept the tool's point this frame: ignore
    /// egui's ordinary `clicked` for the same release so a second on-click cannot finish the
    /// shape (or wipe its Diameter box) when the centre is an existing sketch point (#8899).
    pub(crate) draw_drag_handled: bool,
    /// A tool's click moved on the way (#2448): egui takes a press that moves past its
    /// click distance, or is held past its click time, as a drag, so its release is the click.
    pub(crate) tool_click: bool,
    /// A Trim drag is drawing its path (`sketch_tools::trim_drag_*`).
    pub(crate) trim_drag: bool,
    /// Per-body boxes of one world state, held (not just its address, which a later state can
    /// reuse once the old one is freed, so stale boxes would come back).
    pick_bounds: std::sync::Mutex<Option<BodyBounds>>,
    /// The sketches' box in the world of one world state (held, as `pick_bounds` is). Every
    /// `projection` reads it, several times a frame, and each sketch's box samples its curves.
    sketch_bounds: std::sync::Mutex<Option<(std::sync::Arc<sketcraft_engine::doc::ModelState>, sketcraft_engine::geom::Aabb3)>>,
    /// Membership of the active assembly placement, shared by model and feedback rendering.
    active_bodies: std::sync::Mutex<Option<ActiveBodies>>,
    /// The model scene's blocks of bodies by key, from the last scene built.
    pub(crate) blocks: std::sync::Mutex<std::collections::HashMap<u64, CachedBlock>>,
    /// The triangles of each mesh the last scene drew as instances, by the mesh's address (the
    /// mesh held, so its address stays its own).
    instanced_tris: std::sync::Mutex<InstancedTris>,
    /// The levels of detail of large instanced meshes ([`crate::mesh_lod`]).
    pub(crate) mesh_lods: crate::mesh_lod::MeshLods,
    /// View-dependent display refinement (#1320): which bodies are drawn finer than the chosen
    /// display quality, and their refined plans.
    pub(crate) lod: std::sync::Mutex<crate::display_lod::Lod>,
    /// The view and model the refinement was last chosen for.
    lod_view: u64,
    /// Refinement waits for the camera to stop ([`REFINE_STILL_MS`]).
    lod_rest: CameraRest,
    /// Each body mesh's box (the mesh held, so its address stays its own), from the last state.
    mesh_bounds: std::sync::Mutex<MeshBounds>,
    /// Each shown body mesh's section index (the mesh held, so its address stays its own): a
    /// Section Analysis dragged or tilted cuts through it.
    section_index: std::sync::Mutex<
        std::collections::HashMap<usize, (std::sync::Arc<sketcraft_engine::geom::Mesh>, std::sync::Arc<sketcraft_engine::view::SectionIndex>)>,
    >,
    /// Each body mesh's picking index (made the first time a pick reaches the body), kept while
    /// the mesh is part of the world state.
    pub(crate) pick_index: crate::pick_index::PickIndexes,
    /// The last click in the view was the middle button's (for the middle double-click).
    pub(crate) middle_clicked: bool,
    /// The right button held down has moved or been held too long for a click, so it navigates.
    pub(crate) right_drag: bool,
    /// The last pick (its key and hits): reused while nothing it depends on changed.
    last_pick: Option<(u64, Vec<Hit>)>,
    pub rect: Option<Rect>,
    pub(crate) layout: Option<crate::viewport_layout::Layout>,
    pub(crate) palette_rect: Option<Rect>,
    /// None before the host's early confirmation paint; Some(NOTHING) means
    /// no visible pair this pass. Late controls retain the already drawn rect.
    pub(crate) host_confirm: Option<Rect>,
    /// Snapshot of this host pass's shared persistent Window paint. None is a
    /// bounded-ledger failure; the usable seam then refuses placement/framing.
    pub(crate) host_windows: Option<Vec<Rect>>,
    pub(crate) dialog_rect: Option<Rect>,
    /// OK or Cancel clicked in the view's corner for the open dialog (#6709), which
    /// `dialogs::show` acts on in the same frame as its own buttons.
    pub(crate) corner: Option<crate::ui_kit::Action>,
    /// Whether the open dialog's OK was enabled when it was last drawn: the corner's check
    /// greys out with it.
    pub(crate) corner_ok: bool,
    pub gpu: Option<GpuTarget>,
    /// The GPU plan evaluation still compiling: `gpu.tess` once it is ready (#4860).
    pub(crate) tess_ready: Option<crate::gpu::TessReady>,
    /// What the GPU drew last frame, shared with the renderer (the frame-rate details, #2548).
    pub(crate) draw_stats: Option<crate::gpu::DrawStatsHandle>,
    /// The status line has said the view is drawn at a lower quality ([`DrawStats::lowered`]).
    ///
    /// [`DrawStats::lowered`]: crate::gpu::DrawStats::lowered
    lowered_noted: bool,
    /// The graphics backend and adapter, as the frame-rate details show them.
    pub(crate) adapter: String,
    pub(crate) slot: SceneSlot,
    pub(crate) key: u64,
    /// Ground geometry is reused independently of the camera-dependent sketch/grid scene.
    shadow: std::sync::Mutex<Option<ShadowGeometry>>,
    pub(crate) sketch_slot: SceneSlot,
    sketch_key: u64,
    physical_scale: f32,
    pub(crate) display: std::sync::Mutex<crate::sketch_display::DisplayCache>,
    pub(crate) hl_slot: SceneSlot,
    pub(crate) hl_key: u64,
    /// Browser/timeline body feedback, independent of the camera and reference widget scale.
    hover_block: std::sync::Mutex<Option<HoverBlock>>,
    /// A component's Move triad drag drawn as a layer of its own (`viewport_drag_layer.rs`),
    /// the world the last whole model scene was built from (where such a drag begins), and the
    /// dragged bodies' scene.
    drag: Option<drag_layer::DragLayer>,
    scene_world: Option<drag_layer::SceneWorld>,
    moving_slot: SceneSlot,
    moving_key: u64,
    /// Each shown body the model builds on the CPU: the block of the model scene that holds its
    /// triangles, and the body's index and vertex ranges in it (for whole-body highlights).
    body_tris: std::sync::Mutex<std::collections::HashMap<String, crate::gpu::BlockTris>>,
    /// The stable palette slots of the model scene (body faces, edges, placements).
    pub(crate) palette: std::sync::Mutex<crate::scene_palette::PaletteArena>,
    /// Each body's resolved look, kept while its geometry and the design's appearance state are
    /// the same: a theme or visibility rebuild does not resolve every face again.
    appearance: std::sync::Mutex<AppearanceCache>,
    pub nav: Option<NavMode>,
    /// The preset's navigation for the left-button drag under way (Alias: Shift+Alt), taken
    /// when the button went down, so the drag neither selects nor moves sketch geometry.
    pub primary_nav: Option<NavMode>,
    /// A primary press that started with the Zoom shortcut owns the pointer until release.
    pub(crate) zoom_shortcut_drag: bool,
    /// Button-free gesture and its last owned pointer position, kept through pauses.
    pub(crate) modifier_nav: Option<(NavMode, Pos2)>,
    /// The mode the middle or right drag under way started with, kept until its button is
    /// released when `nav_drag_keeps_mode` is on (#9557).
    pub(crate) button_nav: Option<(crate::prefs::NavButton, Option<NavMode>)>,
    /// What a click would pick now (after the active command's filter).
    pub hover: Option<Hit>,
    /// The centre of the circle or arc of the sketch being edited whose edge was hovered last
    /// (sketch, point id): it is drawn, and it takes the pointer from further away
    /// ([`WOKEN_CENTRE_PX`]), until another curve or point is hovered.
    pub woken_centre: Option<(u64, String)>,
    /// The centre or midpoint most recently hovered for visual H/V tracking.
    pub(crate) tracking: crate::inference::Tracking,
    pub mouse: Option<Pos2>,
    /// Box selection being dragged.
    pub boxsel: Option<BoxSel>,
    /// Timeline item under the cursor: its bodies or resulting faces are highlighted.
    pub hover_feature: Option<u64>,
    /// Bodies whose browser row is under the cursor: highlighted the same way.
    pub hover_bodies: Vec<String>,
    /// The origin or construction plane whose browser row is under the cursor: lit as if the
    /// pointer were on it in the view.
    pub hover_plane: Option<String>,
    /// The model point an orbit turns around (Preferences: orbit around the cursor).
    pub orbit_pivot: Option<Vec3>,
    /// The orbit drag under way, summed from where it began (see [`crate::navigation::orbit`]).
    pub orbit_from: Option<crate::navigation::OrbitFrom>,
    /// An orbit drag moved the cursor to the view's other side: the jump, until the pointer
    /// reports it (see [`crate::navigation::wrap_cursor`]).
    pub(crate) cursor_wrap: Option<egui::Vec2>,
    /// Continuous orbit: the view turns about the vertical until the next click or key.
    pub spin: bool,
    /// The point continuous orbit turns about (Orbit around: origin or the cursor's model point),
    /// chosen when it starts and kept until it stops, so the model stays in view (#1044).
    pub spin_pivot: Option<Vec3>,
    /// The 3D mouse's motion this frame, for the page showing the camera to apply with what its
    /// camera may do (`navigation::device`).
    pub device_motion: Option<crate::space_mouse::DeviceMotion>,
    /// A device app's motion since the page last took it (`space_mouse::bridge_frame`), applied
    /// the same way.
    pub bridge_motion: Option<crate::space_mouse::BridgeMotion>,
    /// The model point the 3D mouse turns the view around: chosen when the cap leaves rest and
    /// kept until it comes back (`navigation::device`, `space_mouse::navigate_frame`).
    pub device_pivot: Option<Vec3>,
    /// After immediate zoom, ignore egui's outstanding smoothing until the gesture ends.
    pub(crate) wheel_zoom_raw_until_idle: bool,
    /// While trackpad scrolling runs, ignore egui's smoothing tail for zoom so
    /// two-finger input is not replayed as zoom after it pans.
    pub(crate) trackpad_until_idle: bool,
    /// Inside a bracketed trackpad scroll (`Start` without its `End` yet):
    /// point moves pan, bare moves are a mouse.
    pub(crate) trackpad_gesture: bool,
    /// The inertia after the fingers lift, which ends when the modifiers change (#7327).
    pub(crate) trackpad_coast: crate::navigation::TrackpadCoast,
    /// A primary press on the axis gizmo: the pointer position used by its last orbit step.
    pub axis_drag: Option<Pos2>,
    /// The centre the gizmo drag last warped the cursor to, until the pointer next lands near
    /// it (steps measured before that echo would jump the view by the whole warp distance).
    pub(crate) axis_warp: Option<Pos2>,
    /// Unsnapped camera while Shift-orbit previews a standard or diagonal view.
    pub orbit_free: Option<Camera>,
    pub build_ms: f64,
    /// Adaptive supersampling (#1288) and ambient occlusion while moving (#7165): the view drawn
    /// last (its view-projection and pixels) and since when it has held still (app time, s).
    pub(crate) ss_view: (u64, f64),
    /// The right-click menu, open at this screen position.
    pub context_menu: Option<Pos2>,
    /// Select Other: its list, a press being held, and click cycling.
    pub select_other: crate::select_other::State,
    /// A sketch point or curve being dragged (its id, the undo depth when the drag began: the
    /// whole drag is one undo step, and for a curve where it was last under the cursor).
    pub point_drag: Option<(String, usize, Option<Vec2>)>,
    /// Checkpoint and source for a cancellable whole-text drag.
    pub(crate) text_drag: Option<(u64, String)>,
    /// A part that joints place, being dragged.
    pub(crate) joint_drag: Option<crate::joint_drag::Drag>,
    pub(crate) joint_handles: Option<crate::joint_handles::Mode>,
    /// A free part being dragged to a new place (#2804).
    pub(crate) occurrence_drag: Option<crate::occurrence_drag::Drag>,
    /// Extrude by pulling a face or profile.
    pub(crate) drag_extrude: crate::drag_extrude::State,
    /// The plane the grid was last drawn on while Preferences ▸ Grid follows the view is on
    /// (#1470), kept so the choice is hysteretic near the thresholds (#1993).
    pub(crate) grid_plane: std::sync::Mutex<GridPlane>,
}

/// Something under the cursor. The control channel reports it in the selection's own form
/// (`{"type": "edge", "body", "index", …}`, #9087).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Hit {
    SketchPoint {
        sketch: u64,
        id: String,
        at: Vec2,
    },
    /// A selectable line midpoint, before any associated helper point has been made.
    SketchMidpoint {
        sketch: u64,
        id: String,
        at: Vec2,
    },
    /// A sketch curve; `straight` for lines (usable as an axis). `point` is the curve's point
    /// under the cursor.
    SketchCurve {
        sketch: u64,
        id: String,
        straight: bool,
        point: Vec3,
    },
    Vertex {
        body: String,
        point: Vec3,
    },
    /// A body edge: `mid` is its middle (what a selection of it records), `point` its point
    /// under the cursor.
    Edge {
        body: String,
        index: usize,
        mid: Vec3,
        point: Vec3,
    },
    Profile {
        sketch: u64,
        index: usize,
    },
    Face {
        body: String,
        index: usize,
        point: Vec3,
    },
    /// An origin plane (XY, XZ, YZ) or a construction plane (by name).
    Plane {
        name: String,
        point: Vec3,
    },
    /// A construction point (by feature name).
    ConstructionPoint {
        name: String,
    },
    /// An origin or construction axis.
    Axis {
        name: String,
    },
}

impl ViewportState {
    pub fn pixels_per_point(&self) -> f32 {
        if self.physical_scale > 0.0 { self.physical_scale } else { 1.0 }
    }
    #[cfg(test)]
    pub(crate) fn set_pixels_per_point(&mut self, scale: f32) {
        self.physical_scale = scale;
    }
}

fn display_proj(app: &SolveApp) -> Proj {
    projection(app, app.viewport.rect.unwrap_or_else(|| Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))))
}

/// World ↔ screen mapping for the current frame.
#[derive(Clone, Copy)]
pub struct Proj {
    pub cam: Camera,
    pub vp: Mat4,
    pub rect: Rect,
}

impl Proj {
    pub fn to_screen(&self, p: Vec3) -> Option<Pos2> {
        let c = self.vp.apply(p);
        if c[3] <= 1e-9 {
            return None;
        }
        let (x, y) = (c[0] / c[3], c[1] / c[3]);
        Some(pos2(
            self.rect.left() + ((x + 1.0) * 0.5 * self.rect.width() as f64) as f32,
            self.rect.top() + ((1.0 - y) * 0.5 * self.rect.height() as f64) as f32,
        ))
    }
    pub fn ray(&self, p: Pos2) -> (Vec3, Vec3) {
        self.cam.ray((p.x - self.rect.left()) as f64, (p.y - self.rect.top()) as f64, self.rect.width() as f64, self.rect.height() as f64)
    }
}

/// A projected 3D face (convex): an unfeathered triangle fan, outlined one edge at a time (no
/// joins between the edges). Unlike `Shape::convex_polygon`, it stays inside its outline when
/// the face turns edge-on.
pub(crate) fn projected_polygon(pts: &[Pos2], fill: Color32, stroke: Stroke) -> Vec<Shape> {
    let mut mesh = egui::Mesh::default();
    for (i, p) in (0u32..).zip(pts) {
        mesh.colored_vertex(*p, fill);
        if i >= 2 {
            mesh.add_triangle(0, i - 1, i);
        }
    }
    let mut out = vec![Shape::mesh(mesh)];
    out.extend(pts.iter().zip(pts.iter().skip(1).chain(pts.first())).map(|(a, b)| Shape::line_segment([*a, *b], stroke)));
    out
}

/// [`Shape::dashed_line`] for points projected into the viewport, cut to `clip` (grown by two
/// dash periods, so the cuts lie off screen). epaint makes one shape per dash and steps along in
/// f32, so a line reaching far off screen made millions of shapes, or never ended: an Extrude
/// distance of 1e9 mm filled the memory within a second. A line inside the bounds is dashed as
/// before; a cut one keeps its dashes in phase along the whole line, so they don't crawl as the
/// view moves.
pub(crate) fn dashed_line(pts: &[Pos2], clip: Rect, stroke: Stroke, dash: f32, gap: f32) -> Vec<Shape> {
    let bounds = clip.expand(2.0 * (dash + gap));
    if pts.iter().all(|p| bounds.contains(*p)) {
        return Shape::dashed_line(pts, stroke, dash, gap);
    }
    let period = f64::from(dash + gap);
    let mut out = Vec::new();
    let mut run: Vec<Pos2> = Vec::new();
    let mut offset = 0.0;
    let mut flush = |run: &mut Vec<Pos2>, offset: f32| {
        if run.len() >= 2 && period > 0.0 {
            out.extend(Shape::dashed_line_with_offset(run, stroke, &[dash], &[gap], offset));
        }
        run.clear();
    };
    let mut along = 0.0_f64;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = f64::from(a.distance(b));
        match clip_segment(a, b, bounds).filter(|_| len.is_finite()) {
            Some((t0, t1)) => {
                // In f64: a cut a trillion points from `a` is lost in f32.
                let at = |t: f64| pos2((f64::from(a.x) + f64::from(b.x - a.x) * t) as f32, (f64::from(a.y) + f64::from(b.y - a.y) * t) as f32);
                let (from, to) = (at(t0), at(t1));
                if run.is_empty() || t0 > 0.0 {
                    flush(&mut run, offset);
                    // How far into its period the pattern is where the run begins: a cut run
                    // waits for the next dash (the wait is off screen), the line's start begins one.
                    let phase = (along + len * t0) % period;
                    offset = if t0 > 0.0 { ((period - phase) % period) as f32 } else { 0.0 };
                    run.push(from);
                }
                run.push(to);
                if t1 < 1.0 {
                    flush(&mut run, offset);
                }
            }
            None => flush(&mut run, offset),
        }
        if len.is_finite() {
            along += len;
        }
    }
    flush(&mut run, offset);
    out
}

/// The part of segment `a`–`b` inside `r`, as parameters `0 ≤ t0 ≤ t1 ≤ 1` along it; `None` when
/// it misses `r` or a coordinate is not finite (Liang–Barsky, in f64).
fn clip_segment(a: Pos2, b: Pos2, r: Rect) -> Option<(f64, f64)> {
    let (x0, y0, dx, dy) = (f64::from(a.x), f64::from(a.y), f64::from(b.x - a.x), f64::from(b.y - a.y));
    if ![x0, y0, dx, dy].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [(-dx, x0 - f64::from(r.min.x)), (dx, f64::from(r.max.x) - x0), (-dy, y0 - f64::from(r.min.y)), (dy, f64::from(r.max.y) - y0)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

/// View directions all round, a few degrees apart, none exactly along an axis.
#[cfg(test)]
pub(crate) fn orbit_directions() -> impl Iterator<Item = Vec3> {
    (0..90).flat_map(|i| {
        (0..45).map(move |j| {
            let (yaw, pitch) = (f64::from(i) * 4.0f64.to_radians() + 0.003, f64::from(j) * 4.0f64.to_radians() - 1.5);
            Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin())
        })
    })
}

/// Every vertex egui draws for `shapes`.
#[cfg(test)]
pub(crate) fn drawn_points(shapes: Vec<Shape>) -> Vec<Pos2> {
    let mut tess = egui::epaint::Tessellator::new(1.0, egui::epaint::TessellationOptions::default(), [1, 1], vec![]);
    let mut mesh = egui::Mesh::default();
    for shape in shapes {
        tess.tessellate_shape(shape, &mut mesh);
    }
    mesh.vertices.iter().map(|v| v.pos).collect()
}

fn rgba(c: Rgb) -> [u8; 4] {
    [c.0, c.1, c.2, 255]
}

fn scene_bounds(app: &SolveApp) -> sketcraft_engine::geom::Aabb3 {
    let mut b = model_bounds(app);
    if !app.preview.bounds.is_empty() {
        b = b.union(&app.preview.bounds);
    }
    b
}

/// The ambient occlusion radius (0: off): 4% of what the frame draws, the box the depth range uses,
/// so a preview in an empty design has its own. Nothing to draw has no occlusion (#9327).
fn ao_radius(app: &SolveApp) -> f32 {
    let size = scene_bounds(app).diagonal();
    if app.preferences.ambient_occlusion && app.ui.visual_style != 2 && size > 0.0 { (size * 0.04).clamp(1e-4, 1e6) as f32 } else { 0.0 }
}

pub fn scene_radius(app: &SolveApp) -> f64 {
    let b = scene_bounds(app);
    let r = if b.is_empty() { 50.0 } else { b.diagonal() * 0.5 + b.center().dist(app.cam.target) };
    r.max(app.cam.half_height() * 2.0).max(10.0)
}

/// How far behind the nearest face a point of a body this size may lie and still count as on it,
/// rather than through it: the body's own mesh error, with room to spare. A facet sits at most
/// a chord tolerance inside its surface, and that tolerance is a fraction of the body's size
/// (see [`DisplayQuality::chord`]), so a tolerance taken from the body is the same whatever the
/// camera does. One cut from the scene or the camera instead reached through a small part in a
/// large assembly, and reached further with every step of zoom out.
///
/// [`DisplayQuality::chord`]: sketcraft_engine::doc::display_quality::DisplayQuality::chord
pub(crate) fn pick_slack(size: f64) -> f64 {
    use sketcraft_engine::doc::display_quality::DisplayQuality;
    // The coarsest level, so the slack covers the mesh at any display quality.
    DisplayQuality::Coarse.chord(size) * 4.0
}

/// The slack for the whole scene, for picks that belong to no one body (a sketch profile or a
/// plane against the face it lies on).
pub(crate) fn scene_pick_slack(app: &SolveApp) -> f64 {
    let b = scene_bounds(app);
    pick_slack(if b.is_empty() { 100.0 } else { b.diagonal() })
}

pub fn projection(app: &SolveApp, rect: Rect) -> Proj {
    // Product paint, control-channel world coordinates, navigation and picking share
    // one camera frame. The helper leaves explicit subframes and Design unchanged.
    let rect = crate::render_workspace::camera_rect(app, rect);
    let mut cam = app.cam;
    // Render owns an explicit product camera, including its saved/custom FOV.
    if !crate::render_workspace::active(app) {
        cam.fov = app.view_fov();
    }
    let aspect = rect.width() as f64 / rect.height().max(1.0) as f64;
    Proj { cam, vp: cam.view_proj_bounds(aspect, &scene_bounds(app)), rect }
}

/// The far plane the picture's projection uses: the grid's fade ends before that plane cuts the
/// ground (#4214). The scene's radius is not the radius the picture frames, so the planes of the
/// projection itself are the ones that count.
fn picture_far(app: &SolveApp, cam: &Camera) -> f64 {
    cam.clip_bounds(&scene_bounds(app)).1
}

/// Sketches drawn in the viewport: the active one, and finished ones not yet used by a feature.
pub(crate) fn visible_sketches(app: &SolveApp) -> Vec<u64> {
    let st = crate::open_meshing::shown_world(app);
    st.sketches.iter().filter(|s| sketcraft_engine::view::sketch_shown(&app.session, s.feature)).map(|s| s.feature).collect()
}

/// What Home, Fit and the orbit pivot frame: the bodies and sketches shown now, where they are
/// now (see [`sketcraft_engine::view::bounds`]).
pub fn view_bounds(app: &SolveApp) -> sketcraft_engine::geom::Aabb3 {
    // Bodies still being meshed after an open frame the view by their outlines.
    // A model that cannot be placed shows no placed bodies (see `open_meshing::session_world`).
    if crate::open_meshing::framing(app) {
        return sketcraft_engine::view::unmeshed_bounds(&app.session).unwrap_or(sketcraft_engine::geom::Aabb3::EMPTY);
    }
    let own = sketcraft_engine::view::bounds(&app.session).unwrap_or(sketcraft_engine::geom::Aabb3::EMPTY);
    crate::edit_in_place::bounds(app).map_or(own, |context| if own.is_empty() { context } else { own.union(&context) })
}

/// The middle of the shown bodies the view shows part of: what a cursor orbit begun over empty
/// space turns about. The middle of everything shown can be far off screen when the part in
/// view is far from other geometry (a part far from the origin and a sketch at it, #2616), and
/// the part then swings out of view. `None` when no body is in view.
pub(crate) fn in_view_centre(app: &SolveApp, proj: &Proj) -> Option<Vec3> {
    let st = crate::open_meshing::shown_world(app);
    let b = body_bounds(app, &st)
        .iter()
        .filter(|(name, b)| !b.is_empty() && app.session.body_visible(name) && in_view(proj, b))
        .fold(sketcraft_engine::geom::Aabb3::EMPTY, |a, (_, b)| a.union(b));
    (!b.is_empty()).then(|| b.center())
}

/// Does the view show part of box `b`? A box partly behind the eye counts.
fn in_view(proj: &Proj, b: &sketcraft_engine::geom::Aabb3) -> bool {
    let mut seen = Rect::NOTHING;
    let mut behind = 0;
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { b.min.x } else { b.max.x },
            if i & 2 == 0 { b.min.y } else { b.max.y },
            if i & 4 == 0 { b.min.z } else { b.max.z },
        );
        match proj.to_screen(corner) {
            Some(p) => seen.extend_with(p),
            None => behind += 1,
        }
    }
    match behind {
        8 => false,
        0 => seen.intersects(proj.rect),
        _ => true,
    }
}

pub(crate) fn scene_key(app: &SolveApp) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // Not the session revision: selection and view-only commands bump it without changing the
    // model. The document identity catches edits, including appearance-only ones (whose world
    // state may be the same Arc).
    (std::sync::Arc::as_ptr(&app.session.doc) as usize).hash(&mut h);
    app.session.colored_components.hash(&mut h);
    (std::sync::Arc::as_ptr(&crate::open_meshing::shown_world(app)) as usize).hash(&mut h);
    base_scene_key(app).hash(&mut h);
    lod_generation(app).hash(&mut h);
    app.viewport.mesh_lods.generation().hash(&mut h);
    h.finish()
}

/// Everything the model scene shows but the world, the session's revision and the display
/// refinement (a drag drawn as a layer keeps the refinement it began with).
fn base_scene_key(app: &SolveApp) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    app.ui.visual_style.hash(&mut h);
    app.preferences.vertex_markers.hash(&mut h);
    // The cached ground AO receiver and the ellipse share the selected ground plane.
    app.preferences.ground_shadow_follows_model.hash(&mut h);
    app.preferences.solar_shadows.hash(&mut h);
    app.preferences.solar_time_hours.to_bits().hash(&mut h);
    app.preferences.solar_day_of_year.hash(&mut h);
    app.preferences.solar_latitude_deg.to_bits().hash(&mut h);
    sketcraft_engine::doc::display_quality::current().hash(&mut h);
    // How bodies are drawn: from their plans on the GPU, or built here (until the evaluation
    // shader has compiled, #4860).
    app.viewport.gpu.is_some_and(|g| g.tess).hash(&mut h);
    // The section plane (its cut outlines are part of the model scene).
    section_plane(app).map(|(o, n)| [o.x, o.y, o.z, n.x, n.y, n.z].map(f64::to_bits)).hash(&mut h);
    app.cam.up.hash(&mut h);
    app.session.visibility.hidden_bodies.hash(&mut h);
    app.dialog.as_ref().map(|d| &d.edit_source_bodies).hash(&mut h);
    app.session.visibility.hidden_occurrences.hash(&mut h);
    app.session.visibility.hidden_sketches.hash(&mut h);
    app.session.visibility.shown_sketches.hash(&mut h);
    app.session.visibility.hidden_folders.hash(&mut h);
    app.session.visibility.hidden_profiles.hash(&mut h);
    app.session.active_component.hash(&mut h);
    app.session.active_occurrence_path.hash(&mut h);
    app.preview.effective_replaced().hash(&mut h);
    app.preview.rolled_back.hash(&mut h);
    app.preferences.cosmetic_threads.hash(&mut h);
    app.preferences.section_hatch.hash(&mut h);
    crate::theme::is_dark().hash(&mut h);
    crate::theme::vision().hash(&mut h);
    // Two themes may share a light/dark base while drawing different geometry colours.
    crate::theme_files::color_key(&Tokens::base()).hash(&mut h);
    // The SketchUp model screen draws its own edge and face colours.
    crate::sketchup_screen::on(app).hash(&mut h);
    app.session.active_sketch.hash(&mut h);
    crate::edit_in_place::scene_key(app).hash(&mut h);
    // A reset palette arena (another design) invalidates every cached block's indices.
    app.viewport.palette.lock().map_or(0, |a| a.generation()).hash(&mut h);
    // Continuity seams are model-scene lines but are not part of the document or world state.
    for seam in &app.session.continuity {
        seam.body.hash(&mut h);
        seam.edge.hash(&mut h);
        seam.points.len().hash(&mut h);
        format!("{:?}", seam.verdict).hash(&mut h);
        seam.order.hash(&mut h);
        seam.points.first().map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).hash(&mut h);
    }
    // So are the faces Isocurve Analysis shows (their curves follow the bodies, hashed above).
    for pick in &app.session.isocurves {
        pick.body.hash(&mut h);
        [pick.at.x, pick.at.y, pick.at.z].map(f64::to_bits).hash(&mut h);
        pick.count.hash(&mut h);
    }
    h.finish()
}

fn sketch_scene_key(app: &SolveApp, proj: &Proj) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    scene_key(app).hash(&mut h);
    app.ui.ground_shadow.hash(&mut h);
    app.preferences.solar_shadows.hash(&mut h);
    app.preferences.solar_time_hours.to_bits().hash(&mut h);
    app.preferences.solar_day_of_year.hash(&mut h);
    app.preferences.solar_latitude_deg.to_bits().hash(&mut h);
    // The grid this scene draws follows the view when the preference says so (#1470): turning it
    // on or off redraws even while the camera holds still.
    app.ui.show_grid.hash(&mut h);
    app.preferences.grid_follows_view.hash(&mut h);
    // Finished sketches are drawn over the bodies when the preference says so.
    app.preferences.reference_geometry_through_bodies.hash(&mut h);
    app.preferences.sketch_halo.hash(&mut h);
    // The grid previews the plane Create Sketch would take (#6975).
    previewed_sketch_plane(app).map(|p| [p.origin, p.x, p.y].map(|v| [v.x, v.y, v.z].map(f64::to_bits))).hash(&mut h);
    // Conflicting curves in their own colour, or as before (#7244).
    app.preferences.sketch_conflict_colour.hash(&mut h);
    (std::sync::Arc::as_ptr(&crate::open_meshing::shown_world(app)) as usize).hash(&mut h);
    // The open sketch follows a dimension's typed value.
    crate::dim_view::preview_solve(app).hash(&mut h);
    crate::sketch_display::projection_key(proj, app.viewport.pixels_per_point()).hash(&mut h);
    h.finish()
}

#[cfg(test)]
pub(crate) fn display_scene_keys(app: &SolveApp, proj: &Proj) -> (u64, u64) {
    (scene_key(app), sketch_scene_key(app, proj))
}

/// A sketch curve colour from the engine, in the current theme and colour vision mode.
fn sketch_color(t: &Tokens, c: Rgb) -> [u8; 4] {
    if c == colors::SKETCH {
        c4(t.sketch)
    } else if c == colors::SKETCH_DONE || c == colors::SKETCH_FIXED {
        c4(t.sketch_fixed)
    } else if c == colors::SKETCH_LOCKED {
        c4(t.sketch_locked)
    } else if c == colors::SKETCH_PROJECTED {
        c4(t.sketch_projected)
    } else if c == colors::SKETCH_LOST {
        c4(t.sketch_lost)
    } else if c == colors::SKETCH_CONSTRUCTION {
        c4(t.sketch_construction)
    } else if c == colors::SKETCH_CENTERLINE {
        c4(t.sketch_centerline)
    } else {
        rgba(c)
    }
}

pub(crate) type ActiveBodyNames = std::sync::Arc<std::collections::HashSet<String>>;

struct ActiveBodies {
    doc: std::sync::Arc<sketcraft_engine::doc::Document>,
    state: std::sync::Arc<sketcraft_engine::doc::ModelState>,
    path: Vec<u64>,
    names: ActiveBodyNames,
}

/// Expanding a nested assembly walks its placements. Camera and pointer changes do not change
/// membership: keep it until the document, evaluated model or active placement changes. Hold
/// both Arcs so a later allocation cannot reuse their addresses and resurrect stale names.
pub(crate) fn active_body_names(app: &SolveApp) -> Option<ActiveBodyNames> {
    let s = &app.session;
    let Some(path) = s.active_occurrence_path.as_ref() else {
        if let Ok(mut cache) = app.viewport.active_bodies.lock() {
            *cache = None;
        }
        return None;
    };
    let state = s.model.state();
    if let Some(names) = app.viewport.active_bodies.lock().ok().and_then(|cache| {
        cache
            .as_ref()
            .filter(|c| std::sync::Arc::ptr_eq(&c.doc, &s.doc) && std::sync::Arc::ptr_eq(&c.state, &state) && c.path == *path)
            .map(|c| c.names.clone())
    }) {
        return Some(names);
    }
    let names: ActiveBodyNames = std::sync::Arc::new(s.occurrence_bodies(path, true).ok()?.into_iter().collect());
    if let Ok(mut cache) = app.viewport.active_bodies.lock() {
        *cache = Some(ActiveBodies { doc: s.doc.clone(), state, path: path.clone(), names: names.clone() });
    }
    Some(names)
}

/// With a component active, bodies outside it are drawn faded.
pub(crate) fn body_faded(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody, active_bodies: &Option<ActiveBodyNames>) -> bool {
    let s = &app.session;
    if let Some(names) = active_bodies {
        !names.contains(&b.name)
    } else {
        s.active_component != 0 && !s.doc.component_within(b.component, s.active_component)
    }
}

/// How many times the viewport's pixels to draw this frame each way (#1288; 1: off): the
/// preference, and with adaptive supersampling only once the view has held still for a moment,
/// so orbiting and zooming stay as quick as without it.
fn supersampling(app: &mut SolveApp, ctx: &egui::Context, view_proj: [[f32; 4]; 4], size_px: [f32; 2]) -> f32 {
    let factor = app.preferences.supersampling;
    if !(factor > 1.0) {
        return 1.0;
    }
    if !app.preferences.supersampling_adaptive {
        return factor;
    }
    if view_settled(app, ctx, view_proj, size_px) { factor } else { 1.0 }
}

/// Ambient occlusion samples per pixel this frame (#7165): the full count once the view has held
/// still for a moment, fewer while it moves, so orbiting with AO on stays quick.
/// The shader's sky (#7974): the world's up direction, and 1 to shade in linear light with a sky
/// fill (Preferences › Graphics), else zero.
pub(crate) fn sky_uniform(on: bool, up: sketcraft_engine::render::UpAxis) -> [f32; 4] {
    if !on {
        return [0.0; 4];
    }
    match up {
        sketcraft_engine::render::UpAxis::Z => [0.0, 0.0, 1.0, 1.0],
        sketcraft_engine::render::UpAxis::Y => [0.0, 1.0, 0.0, 1.0],
    }
}

fn ao_samples(app: &mut SolveApp, ctx: &egui::Context, view_proj: [[f32; 4]; 4], size_px: [f32; 2]) -> u32 {
    // Without AO (off, or wireframe) the count is unused: no repaint is asked for it.
    if !app.preferences.ambient_occlusion || app.ui.visual_style == 2 || view_settled(app, ctx, view_proj, size_px) {
        sketcraft_gpu::view::AO_SAMPLES
    } else {
        sketcraft_gpu::view::AO_SAMPLES_MOVING
    }
}

/// Whether the frame on screen is the full-quality one a screenshot should wait for: with
/// ambient occlusion drawn, the view has held still for [`SS_SETTLE`], so the picture has all
/// its samples (#7165). Reads the state the last painted frame left.
pub(crate) fn ao_settled(app: &SolveApp, ctx: &egui::Context) -> bool {
    !app.preferences.ambient_occlusion || app.ui.visual_style == 2 || ctx.input(|i| i.time) - app.viewport.ss_view.1 >= SS_SETTLE
}

/// Whether the view (its view-projection and pixels) has held still for [`SS_SETTLE`]. Until it
/// has, a repaint is asked for when it will have, so the full-quality frame follows by itself.
fn view_settled(app: &mut SolveApp, ctx: &egui::Context, view_proj: [[f32; 4]; 4], size_px: [f32; 2]) -> bool {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    view_proj.iter().flatten().chain(&size_px).for_each(|x| x.to_bits().hash(&mut h));
    let view = h.finish();
    let now = ctx.input(|i| i.time);
    if view != app.viewport.ss_view.0 {
        app.viewport.ss_view = (view, now);
    }
    let still = now - app.viewport.ss_view.1;
    if still < SS_SETTLE {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SS_SETTLE - still + 0.01));
    }
    still >= SS_SETTLE
}

/// How long the view holds still before adaptive supersampling and ambient occlusion draw it at
/// full quality (s).
const SS_SETTLE: f64 = 0.25;

/// How far (screen points) a press must be dragged for its release to draw a rectangle, circle
/// or polygon (#1630); a shorter drag is a click that moved on the way (#2448).
const DRAW_DRAG_MIN: f32 = 24.0;

/// The display plan the model scene draws `b` from on the GPU, or `None` when its faces are built
/// here from the reference mesh. Opaque shaded bodies (`opaque`: not faded, and no see-through
/// body or face) are drawn from their plans where the device evaluates them; faded and
/// see-through ones (sorted and blended per triangle) are built here. A body seen up close is
/// drawn from its refined plan ([`crate::display_lod`]).
fn drawn_plan(
    app: &SolveApp,
    b: &sketcraft_engine::doc::ModelBody,
    opaque: bool,
) -> Option<std::sync::Arc<sketcraft_engine::geom::tess::TessPlan>> {
    let shaded = app.ui.visual_style != 2;
    (shaded && opaque && app.viewport.gpu.is_some_and(|g| g.tess))
        .then(|| refined(app, b).map(|(plan, _)| plan).or_else(|| b.display_plan()))
        .flatten()
        .filter(|plan| plan.vertex_count() <= sketcraft_gpu::MAX_BATCH)
}

/// The refined plan and mesh `b` is drawn from, when the view has it drawn finer than the chosen
/// display quality (#1320).
fn refined(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody) -> crate::display_lod::Refined {
    app.viewport.lod.lock().ok()?.refined(b, sketcraft_engine::doc::display_quality::current())
}

/// Changes whenever a body's refinement changes (the scene and highlight keys).
fn lod_generation(app: &SolveApp) -> u64 {
    app.viewport.lod.lock().map_or(0, |l| l.generation)
}

/// The mesh whose triangles the model scene shows for `b`'s faces: the plan's (evaluated on the
/// CPU, the same triangles the GPU draws) or [`drawn_mesh`]. A face highlighted over the model
/// must use the same triangles: another triangulation of the same face fights it at equal depth
/// and shimmers.
fn shown_mesh(
    app: &SolveApp,
    b: &sketcraft_engine::doc::ModelBody,
    active_bodies: &Option<ActiveBodyNames>,
) -> std::sync::Arc<sketcraft_engine::geom::Mesh> {
    if let Some((_, mesh)) = refined(app, b) {
        // Refined: faces drawn from the refined plan on the GPU or built from its mesh here.
        return mesh;
    }
    (app.viewport.gpu.is_some_and(|g| g.tess) && model_plan_class(app, b, active_bodies).is_some())
        .then(|| drawn_plan(app, b, true))
        .flatten()
        // A large body's plan mesh is made on a worker: until then its faces are lit on the
        // mesh drawn here (#3745).
        .and_then(|_| b.plan_mesh_soon())
        .unwrap_or_else(|| drawn_mesh(app, b))
}

/// The mesh the model scene builds `b` from on the CPU (its edges always, its faces when they
/// are not drawn from the plan on the GPU): a refined plan's mesh when the view has one for it
/// (#1320), the reference mesh at Normal display quality, else the plan's mesh at the chosen
/// quality (finer or coarser faces and edges alike), falling back to the reference mesh when the
/// body has no plan.
fn drawn_mesh(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody) -> std::sync::Arc<sketcraft_engine::geom::Mesh> {
    if crate::render_workspace::active(app) {
        return b.mesh();
    }
    use sketcraft_engine::doc::display_quality::{DisplayQuality, current};
    if let Some((_, mesh)) = refined(app, b) {
        return mesh;
    }
    if current() == DisplayQuality::Normal { b.mesh() } else { b.plan_mesh().unwrap_or_else(|| b.mesh()) }
}

/// The move a Move dialog's earlier types made to body `name`, if it is one they moved
/// (`dialogs_move::chained`): its edges, faces and centres are shown where that move puts them.
fn chain_of(chain: Option<&(Vec<String>, sketcraft_engine::doc::Mat)>, name: &str) -> Option<sketcraft_engine::doc::Mat> {
    chain.filter(|(names, _)| names.iter().any(|n| n == name)).map(|(_, c)| *c)
}

/// `mesh` moved by `chain`, or as it is without one.
fn carried_mesh(
    chain: Option<sketcraft_engine::doc::Mat>,
    mesh: std::sync::Arc<sketcraft_engine::geom::Mesh>,
) -> std::sync::Arc<sketcraft_engine::geom::Mesh> {
    let Some(c) = chain else { return mesh };
    let mut m = (*mesh).clone();
    m.positions.iter_mut().for_each(|p| *p = sketcraft_engine::doc::apply_point(&c, *p));
    m.normals.iter_mut().for_each(|n| *n = sketcraft_engine::doc::apply_vector(&c, *n));
    m.edges.iter_mut().flatten().for_each(|p| *p = sketcraft_engine::doc::apply_point(&c, *p));
    std::sync::Arc::new(m)
}

/// How long (ms) the camera must stand still before display refinements are chosen and swapped in.
const REFINE_STILL_MS: f64 = 200.0;

/// The camera the last frame had and when it last differed from the one before (ms).
#[derive(Default)]
struct CameraRest {
    camera: u64,
    moved_at: f64,
}

impl CameraRest {
    /// Notes this frame's camera (a hash of the view) at `now` (ms); how much longer (ms) the
    /// camera has to stand still, 0 once it has for [`REFINE_STILL_MS`].
    fn wait(&mut self, camera: u64, now: f64) -> f64 {
        if camera != self.camera {
            *self = CameraRest { camera, moved_at: now };
        }
        (REFINE_STILL_MS - (now - self.moved_at)).max(0.0)
    }
}

/// Choose which bodies to draw finer than the chosen display quality for this view (#1320):
/// each shown body's screen-space error from its box on screen, refinements planned off the UI
/// thread and swapped in when ready. Keeps repainting while refinements are being planned.
fn update_refinement(app: &mut SolveApp, ctx: &egui::Context, proj: &Proj) {
    if app.viewport.gpu.is_none() || !crate::display_lod::enabled() {
        return; // The CPU picture draws the reference mesh.
    }
    let q = sketcraft_engine::doc::display_quality::current();
    // Nothing to choose again while the view, the model and the planning stand still.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    scene_key(app).hash(&mut h);
    crate::sketch_display::projection_key(proj, app.viewport.pixels_per_point()).hash(&mut h);
    let view = h.finish();
    // A moving camera is left alone: every refinement swapped in rebuilds and uploads the whole
    // model scene, which in a large assembly takes tens of milliseconds a time. The bodies keep
    // the step they have until the camera has stood still for [`REFINE_STILL_MS`].
    let camera = crate::sketch_display::projection_key(proj, app.viewport.pixels_per_point());
    let wait = app.viewport.lod_rest.wait(camera, crate::now_ms());
    if wait > 0.0 {
        ctx.request_repaint_after(std::time::Duration::from_millis(wait as u64 + 5));
        return;
    }
    // While refinements are being planned, the view standing still: only collect finished ones.
    if view == app.viewport.lod_view {
        let (busy, finished) = app.viewport.lod.lock().map_or((false, false), |mut l| {
            let finished = l.collect();
            (l.busy(), finished)
        });
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        if !finished {
            return;
        }
    }
    let t0 = crate::now_ms();
    app.viewport.lod_view = view;
    let st = crate::open_meshing::shown_world(app);
    let ppp = f64::from(app.viewport.pixels_per_point());
    let lens = crate::display_lod::Lens::new(&proj.cam, f64::from(proj.rect.height()) * ppp);
    let bounds = body_bounds(app, &st);
    let mut live = std::collections::HashSet::with_capacity(st.bodies.len());
    let mut candidates = Vec::new();
    for (b, (name, bx)) in st.bodies.iter().zip(bounds.iter()) {
        let id = crate::display_lod::body_id(b).0;
        live.insert(id);
        if b.name != *name || b.body.is_mesh() {
            continue;
        }
        let shown = app.body_visible(&b.name) && !app.preview.effective_replaced().contains(&b.name);
        let on_screen = match screen_box(proj, bx, 0.0).filter(|_| shown) {
            Some(r) => r.intersects(proj.rect).then(|| f64::from(r.width().max(r.height())) * ppp),
            // A corner behind the eye: the view is inside or right by the box.
            None => (shown && proj.cam.fov > 0.0 && lens.in_front(bx)).then_some(f64::INFINITY),
        };
        let err_px = crate::display_lod::error_px(q, b.body.size(), bx, on_screen, &lens);
        candidates.push(crate::display_lod::Candidate { body: b, err_px, triangles: b.mesh_in_place().0.triangles.len() });
    }
    let busy = match app.viewport.lod.lock() {
        Ok(mut lod) => {
            lod.update(q, candidates, &live);
            lod.stats.choose_ms = crate::now_ms() - t0;
            lod.busy()
        }
        Err(_) => false,
    };
    if busy {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
}

/// A body's look as the scene resolves it: the component/body colours, the body opacity and the
/// faces with looks of their own.
#[derive(Clone)]
struct ResolvedAppearance {
    component: Option<Color32>,
    body: Option<Color32>,
    opacity: f64,
    face_looks: std::collections::HashMap<usize, sketcraft_engine::doc::appearance::Look>,
}

/// [`ViewportState::appearance`]: resolved bodies by name, with their geometry key and the
/// appearance state they were resolved from.
#[derive(Default)]
struct AppearanceCache {
    /// The document the face index below was built from, and whether components coloured bodies.
    doc: Option<(usize, bool)>,
    /// A hash of each body's face appearances, so a body's look is keyed without scanning them.
    faces: std::collections::HashMap<String, u64>,
    /// Each body's appearance key, reused while the document and its geometry are unchanged.
    keys: std::collections::HashMap<String, u64>,
    bodies: std::collections::HashMap<String, (u64, std::sync::Arc<ResolvedAppearance>)>,
    /// Bodies actually resolved (the test asserts an unrelated edit resolves only its body).
    #[cfg(test)]
    resolved: u32,
}

/// A hash per body of the face appearances that name it, in design order (a later assignment
/// wins). Rebuilt when the document changes.
fn face_appearance_hashes(doc: &sketcraft_engine::doc::Document) -> std::collections::HashMap<String, u64> {
    use std::hash::Hasher as _;
    let mut out: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for f in &doc.appearances.faces {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        out.get(&f.body).hash(&mut h);
        serde_json::to_string(f).ok().hash(&mut h);
        out.insert(f.body.clone(), h.finish());
    }
    out
}

fn hash_look(look: Option<&sketcraft_engine::doc::appearance::Look>, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash as _;
    match look {
        Some(l) => {
            1u8.hash(h);
            l.name.hash(h);
            l.color.hash(h);
            l.opacity.to_bits().hash(h);
            match &l.pbr {
                Some(p) => {
                    1u8.hash(h);
                    p.metallic().to_bits().hash(h);
                    p.roughness().to_bits().hash(h);
                    p.dielectric_ior().to_bits().hash(h);
                }
                None => 0u8.hash(h),
            }
        }
        None => 0u8.hash(h),
    }
}

/// What a body's resolved look depends on: its geometry (imported paint), the body/component
/// looks, its material and that material's look, its face appearances, and component colouring.
fn body_appearance_key(
    doc: &sketcraft_engine::doc::Document,
    b: &sketcraft_engine::doc::ModelBody,
    faces: &std::collections::HashMap<String, u64>,
    colored: bool,
) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.geometry_key().unwrap_or(0).hash(&mut h);
    let name = b.definition_name();
    hash_look(doc.appearances.bodies.get(name), &mut h);
    hash_look(doc.appearances.components.get(&b.component), &mut h);
    let material = doc.materials.get(name);
    material.hash(&mut h);
    hash_look(material.and_then(|m| doc.material_look(m)).as_ref(), &mut h);
    faces.get(name).copied().unwrap_or(0).hash(&mut h);
    colored.hash(&mut h);
    if colored {
        doc.component_identification_color(b.component).hash(&mut h);
    }
    h.finish()
}

fn resolve_appearance(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody) -> ResolvedAppearance {
    let s = &app.session;
    ResolvedAppearance {
        component: sketcraft_engine::view::component_color(s, b.component).map(|[r, g, b]| Color32::from_rgb(r, g, b)),
        body: body_color(&s.doc, b),
        opacity: s.doc.model_body_opacity(b),
        face_looks: if s.colored_components.is_none() { s.doc.face_colors(b).into_iter().collect() } else { Default::default() },
    }
}

/// How a body whose faces all share one class may be drawn from its plan: opaque on any GPU
/// path; faded or see-through only where the renderer blends them order-independently (the
/// sorted fallback needs the triangles on the CPU). `None`: faces of different classes, which
/// are built on the CPU. `faces` are the classes [`build_scene`] gives the body's faces.
fn plan_class(app: &SolveApp, faces: &[crate::gpu::TriClass]) -> Option<crate::gpu::TriClass> {
    let first = faces.first().copied().unwrap_or(crate::gpu::TriClass::Opaque);
    if faces.iter().any(|c| *c != first) {
        return None;
    }
    (first == crate::gpu::TriClass::Opaque || app.viewport.gpu.is_some_and(|g| g.oit)).then_some(first)
}

/// The classes [`build_scene`] gives `b`'s faces: faded outside the active component, else
/// see-through below full opacity (the body's, or a face's own look).
fn face_classes(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody, active_bodies: &Option<ActiveBodyNames>) -> Vec<crate::gpu::TriClass> {
    if body_faded(app, b, active_bodies) {
        return vec![crate::gpu::TriClass::Faded; b.body.face_count()];
    }
    if app.ui.visual_style == 4 {
        return vec![crate::gpu::TriClass::Glass; b.body.face_count()];
    }
    let resolved = resolve_appearance(app, b);
    (0..b.body.face_count())
        .map(|f| {
            let opacity = resolved.face_looks.get(&f).map_or(resolved.opacity, |l| l.opacity);
            if opacity < 0.999 { crate::gpu::TriClass::Glass } else { crate::gpu::TriClass::Opaque }
        })
        .collect()
}

/// Whether the model scene draws `b` from its plan's GPU-evaluated triangles (and with which
/// class), as [`build_scene`] decides: a live preview stands in for none.
fn model_plan_class(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody, active_bodies: &Option<ActiveBodyNames>) -> Option<crate::gpu::TriClass> {
    if app.preview.effective_replaced().contains(&b.name) {
        return None;
    }
    plan_class(app, &face_classes(app, b, active_bodies))
}

/// A model scene block kept by the viewport: its bytes and the meshes they were built from
/// (held, so no other mesh can take their addresses while the block is kept).
pub(crate) struct CachedBlock {
    bytes: std::sync::Arc<crate::gpu::BlockBytes>,
    meshes: Vec<std::sync::Arc<sketcraft_engine::geom::Mesh>>,
    /// Each body's share of the block: its vertices, its opaque runs of them (whole-body
    /// highlights draw those again), and the face classes the runs were built from.
    ranges: Vec<BodyRange>,
}

/// One body's share of a block.
struct BodyRange {
    name: String,
    /// Its vertices: three per triangle, in mesh order (none when its faces are drawn elsewhere).
    vertices: std::ops::Range<u32>,
    opaque: Vec<std::ops::Range<u32>>,
    face_classes: Vec<crate::gpu::TriClass>,
}

/// [`ViewportState::mesh_bounds`]: each box by the shared mesh it was measured on (held, so its
/// address stays its own) and the placement that moved it ([`ModelBody::mesh_in_place`]).
///
/// [`ModelBody::mesh_in_place`]: sketcraft_engine::doc::ModelBody::mesh_in_place
type MeshBounds = std::collections::HashMap<(usize, [u64; 16]), (std::sync::Arc<sketcraft_engine::geom::Mesh>, sketcraft_engine::geom::Aabb3)>;

/// [`ViewportState::instanced_tris`].
type InstancedTris = std::collections::HashMap<usize, (std::sync::Arc<sketcraft_engine::geom::Mesh>, std::sync::Arc<crate::gpu::SharedTris>)>;

/// A body alone is drawn as an instance of its triangles (shared vertices, indexed) from this many
/// triangles: a large imported mesh. Smaller ones are drawn so only when placed more than once.
pub(crate) const INSTANCE_ALONE: usize = 50_000;

/// Shared vertices made ahead, off the window thread ([`warm_shared_tris`]), by mesh address
/// (the mesh held, so its address stays its own), until a scene takes them.
static WARMED: std::sync::Mutex<Vec<(std::sync::Arc<sketcraft_engine::geom::Mesh>, std::sync::Arc<crate::gpu::SharedTris>)>> =
    std::sync::Mutex::new(Vec::new());
/// Most meshes kept warmed and not taken yet (a design opened and closed before it was drawn).
const MAX_WARMED: usize = 8;

/// Make a large mesh's shared vertices ahead of its first scene, on the caller's thread (the
/// background mesher, after a file is opened or inserted): welding a scanned part takes a
/// noticeable part of a second, which the window thread would otherwise spend on the first frame
/// that shows it.
pub(crate) fn warm_shared_tris(mesh: &std::sync::Arc<sketcraft_engine::geom::Mesh>) {
    if mesh.triangles.len() < INSTANCE_ALONE || WARMED.lock().map_or(true, |w| w.iter().any(|(m, _)| std::sync::Arc::ptr_eq(m, mesh))) {
        return;
    }
    let tris = std::sync::Arc::new(crate::gpu::SharedTris::new(mesh));
    if let Ok(mut warmed) = WARMED.lock() {
        if warmed.len() >= MAX_WARMED {
            warmed.remove(0);
        }
        warmed.push((mesh.clone(), tris));
    }
}

/// The shared vertices made ahead for `mesh`, taken from the warmed ones.
fn taken_shared_tris(mesh: &std::sync::Arc<sketcraft_engine::geom::Mesh>) -> Option<std::sync::Arc<crate::gpu::SharedTris>> {
    let mut warmed = WARMED.lock().ok()?;
    let at = warmed.iter().position(|(m, _)| std::sync::Arc::ptr_eq(m, mesh))?;
    Some(warmed.remove(at).1)
}

/// One body as the model scene draws it. Its geometry lives in a block's canonical vertex
/// buffer; its colours are palette indices, so appearance edits never touch vertex bytes.
struct BodyDraw {
    /// The body's name in the world state, for the highlight's lookup of its triangles.
    name: String,
    /// Its mesh where it stands, or (with `place`) the shared mesh `place` moves there.
    mesh: std::sync::Arc<sketcraft_engine::geom::Mesh>,
    /// The move from `mesh` to where the body stands, for a body drawn as an instance.
    place: Option<sketcraft_engine::doc::Mat>,
    /// The palette index of each face's colour, and the body's fallback.
    face_colors: Vec<crate::gpu::PaletteIndex>,
    body_color: crate::gpu::PaletteIndex,
    /// The render class of each face (opaque, glass or faded).
    face_classes: Vec<crate::gpu::TriClass>,
    faded: bool,
    /// The body encloses a volume: only closed bodies take part in a section's crossing count
    /// ([`crate::gpu::BlockBytes::closed`]).
    closed: bool,
    /// Its faces are drawn elsewhere, from its plan on the GPU or as an instance of shared
    /// triangles (only its edges are built here).
    on_gpu: bool,
    /// The body's first face slot: stable while the body exists, so cached block indices are.
    slot_base: u32,
    edge: crate::gpu::PaletteIndex,
    hidden_edge: crate::gpu::PaletteIndex,
}

impl BodyDraw {
    /// Everything [`BodyDraw::build`] reads. Colours, opacity and classes are deliberately left
    /// out: changing them must not change the block's vertices.
    fn key(&self, shaded: bool, edges: bool, hidden_edges: bool, markers: bool) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (std::sync::Arc::as_ptr(&self.mesh) as usize).hash(&mut h);
        self.place.iter().flatten().flatten().for_each(|v| v.to_bits().hash(&mut h));
        (self.faded, self.closed, self.on_gpu, shaded, edges, hidden_edges, markers, self.slot_base, self.edge.0, self.hidden_edge.0).hash(&mut h);
        h.finish()
    }

    fn build(&self, out: &mut crate::gpu::BlockBuilder, shaded: bool, edges: bool, hidden_edges: bool, markers: bool) {
        let m = &self.mesh;
        let color = |ti: usize| m.tri_face.get(ti).and_then(|f| self.face_colors.get(*f as usize)).copied().unwrap_or(self.body_color);
        let class = |ti: usize| {
            if self.faded {
                return crate::gpu::TriClass::Faded;
            }
            m.tri_face.get(ti).and_then(|f| self.face_classes.get(*f as usize)).copied().unwrap_or(crate::gpu::TriClass::Opaque)
        };
        out.open_body(!self.closed);
        if shaded && !self.on_gpu {
            let corner = |k: u32| {
                let i = k as usize;
                (m.positions.get(i).copied().unwrap_or(Vec3::ZERO).to_f32(), m.normals.get(i).copied().unwrap_or(Vec3::ZERO).to_f32())
            };
            // Exactly three vertices per triangle, in mesh order: a class change regenerates the
            // runs without touching the vertices (see `reindex_block`).
            for (ti, t) in m.triangles.iter().enumerate() {
                let [a, b, c] = t.map(corner);
                out.triangle([a.0, b.0, c.0], [a.1, b.1, c.1], color(ti), class(ti));
            }
        }
        let at = |p: Vec3| self.place.as_ref().map_or(p, |m| sketcraft_engine::doc::apply_point(m, p)).to_f32();
        for (ei, e) in m.edges.iter().enumerate().filter(|_| edges) {
            if !m.edge_visible(ei) || (shaded && !m.shaded_edge_visible(ei)) {
                continue;
            }
            for w in e.windows(2) {
                let (a, b) = (at(w[0]), at(w[1]));
                out.line(a, b, self.edge, 1.3);
                if hidden_edges {
                    out.hidden_line(a, b, self.hidden_edge, 1.0);
                }
            }
        }
        // Vertex markers follow the edges: drawn where they are, faint behind the model where
        // hidden edges are, and not in plain shaded (#7834).
        if edges && markers {
            for p in vertex_markers(m) {
                let p = at(p);
                out.marker(p, self.edge, VERTEX_MARKER_PX, false);
                if hidden_edges {
                    out.marker(p, self.hidden_edge, VERTEX_MARKER_PX, true);
                }
            }
        }
    }
}

/// How large a vertex marker is on the screen, in pixels: a small dot next to the 1.3 px edges,
/// well inside the hover ring drawn over a vertex that can be picked.
const VERTEX_MARKER_PX: f32 = 4.0;

/// Where `mesh`'s vertices are: the ends of the edges a vertex can be picked from, each once. A
/// mesh body's outlines (no B-rep edges) have none; ends of tangent edges the shaded view leaves
/// out are kept, as they can still be picked.
fn vertex_markers(mesh: &sketcraft_engine::geom::Mesh) -> Vec<Vec3> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (ei, e) in mesh.edges.iter().enumerate() {
        if !mesh.edge_pickable(ei) {
            continue;
        }
        for p in [e.first(), e.last()].into_iter().flatten() {
            if [p.x, p.y, p.z].iter().all(|v| v.is_finite()) && seen.insert([p.x, p.y, p.z].map(|v| (v + 0.0).to_bits())) {
                out.push(*p);
            }
        }
    }
    out
}

/// Add a block of bodies to the scene: the bytes built for the same bodies last time when
/// there were any, else built now.
fn push_block(
    sc: &mut GpuScene,
    kept: &mut std::collections::HashMap<u64, CachedBlock>,
    made: &mut std::collections::HashMap<u64, CachedBlock>,
    key: u64,
    draws: Vec<BodyDraw>,
    shaded: bool,
    build: impl Fn(&BodyDraw, &mut crate::gpu::BlockBuilder),
) {
    let same = |c: &CachedBlock| c.meshes.len() == draws.len() && c.meshes.iter().zip(&draws).all(|(m, d)| std::sync::Arc::ptr_eq(m, &d.mesh));
    let classes_same = |c: &CachedBlock| c.ranges.len() == draws.len() && c.ranges.iter().zip(&draws).all(|(r, d)| r.face_classes == d.face_classes);
    let mut block = match kept.remove(&key).or_else(|| made.remove(&key)).filter(same) {
        Some(c) if classes_same(&c) => c,
        Some(c) => reindex_block(c, &draws),
        None => build_block(&draws, shaded, &build),
    };
    // The body names are not part of a block's key (its bytes are the same whatever a body is
    // called): keep the ranges' names those of the bodies drawn from them this time.
    if block.ranges.len() == draws.len() {
        for (range, d) in block.ranges.iter_mut().zip(&draws) {
            if range.name != d.name {
                range.name.clone_from(&d.name);
            }
        }
    }
    sc.blocks.push(crate::gpu::SceneBlock { key, bytes: block.bytes.clone() });
    made.insert(key, block);
}

/// Write a body's colour into the stable arena and into the scene palette being built from it.
fn set_palette_color(
    arena: &mut crate::scene_palette::PaletteArena,
    palette: &mut std::sync::Arc<crate::gpu::Palette>,
    slot: crate::gpu::PaletteIndex,
    rgba: [u8; 4],
) {
    // The scene palette starts as a copy of the arena's entries: an unchanged slot is already
    // right, and most rebuilds (visibility, LOD, section) change none.
    if arena.get(slot) == Some(rgba) {
        return;
    }
    arena.set(slot, rgba);
    std::sync::Arc::make_mut(palette).set(slot, rgba);
}

fn build_block(draws: &[BodyDraw], shaded: bool, build: &impl Fn(&BodyDraw, &mut crate::gpu::BlockBuilder)) -> CachedBlock {
    let mut builder = crate::gpu::BlockBuilder::default();
    if shaded {
        builder.reserve_triangles(draws.iter().filter(|d| !d.on_gpu).map(|d| d.mesh.triangles.len()).sum());
    }
    let at = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let mut spans = Vec::with_capacity(draws.len());
    for d in draws {
        let v0 = builder.vertices_len();
        // Its box goes with its triangles, so a view without it leaves them out.
        builder.begin_body();
        build(d, &mut builder);
        builder.end_body();
        spans.push(at(v0)..at(builder.vertices_len()));
    }
    let ranges = draws
        .iter()
        .zip(spans)
        .map(|(d, vertices)| BodyRange {
            name: d.name.clone(),
            opaque: builder.runs_within(crate::gpu::TriClass::Opaque, vertices.clone()),
            vertices,
            face_classes: d.face_classes.clone(),
        })
        .collect();
    CachedBlock { bytes: builder.freeze(), meshes: draws.iter().map(|d| d.mesh.clone()).collect(), ranges }
}

/// Keep a block's vertex, line and hidden artifacts and regenerate only the class runs (an
/// opacity class change). A body's vertices are three per triangle in mesh order, so this reads
/// each triangle's face, not the mesh's geometry.
fn reindex_block(block: CachedBlock, draws: &[BodyDraw]) -> CachedBlock {
    use crate::gpu::TriClass;
    let mut runs: [Vec<std::ops::Range<u32>>; 3] = Default::default();
    let mut closed = Vec::new();
    for (range, d) in block.ranges.iter().zip(draws) {
        // The triangles this body wrote (none when its faces are drawn from a plan or as an
        // instance), never more than its mesh has.
        let written = ((range.vertices.end.saturating_sub(range.vertices.start)) / 3) as usize;
        for j in 0..written.min(d.mesh.triangles.len()) {
            let class = d.mesh.tri_face.get(j).and_then(|f| d.face_classes.get(*f as usize)).copied().unwrap_or(TriClass::Opaque);
            let slot = match class {
                TriClass::Opaque => 0,
                TriClass::Glass => 1,
                TriClass::Faded => 2,
            };
            let at = range.vertices.start.saturating_add(u32::try_from(j * 3).unwrap_or(u32::MAX));
            if let Some(list) = runs.get_mut(slot) {
                crate::gpu::extend_runs(list, at);
            }
            if class == TriClass::Opaque && d.closed {
                crate::gpu::extend_runs(&mut closed, at);
            }
        }
    }
    let [opaque, glass, faded] = runs;
    let ranges = block
        .ranges
        .iter()
        .zip(draws)
        .map(|(range, d)| BodyRange {
            name: d.name.clone(),
            vertices: range.vertices.clone(),
            opaque: crate::gpu::runs_within(&opaque, range.vertices.clone()),
            face_classes: d.face_classes.clone(),
        })
        .collect();
    let old = block.bytes;
    let bytes = std::sync::Arc::new(crate::gpu::BlockBytes {
        vertices: old.vertices.clone(),
        bodies: old.bodies.clone(),
        opaque: std::sync::Arc::new(opaque),
        closed: std::sync::Arc::new(closed),
        glass: std::sync::Arc::new(glass),
        faded: std::sync::Arc::new(faded),
        lines: old.lines.clone(),
        hidden: old.hidden.clone(),
    });
    CachedBlock { bytes, meshes: draws.iter().map(|d| d.mesh.clone()).collect(), ranges }
}

/// The model scene's instanced meshes: imported reference meshes, shared triangles drawn more
/// than once, and large bodies ([`INSTANCE_ALONE`]), with each body's placement.
struct Instances {
    /// Each candidate body's shared mesh, placement and eligibility for existing LODs, by name.
    candidates: std::collections::HashMap<String, (std::sync::Arc<sketcraft_engine::geom::Mesh>, sketcraft_engine::doc::Mat, bool)>,
    /// The meshes drawn as instances, in the order their first placement came, by address.
    meshes: Vec<(std::sync::Arc<sketcraft_engine::geom::Mesh>, crate::gpu::InstancedMesh)>,
    /// Where a mesh's look already is in `meshes`, so placing a body is a lookup, not a scan.
    by_mesh: std::collections::HashMap<(usize, crate::gpu::InstanceLook), usize>,
    kept: InstancedTris,
    lods: crate::mesh_lod::MeshLods,
}

impl Instances {
    /// Keep reference meshes at every size: their triangles do not change when a joint moves.
    /// Other bodies share triangles when repeated or large enough to warrant a separate draw.
    ///
    /// A scene of only some of the bodies (`whole` false: a drag's layer and the model beside
    /// it) leaves the kept triangles of the others where they are.
    fn new(app: &SolveApp, shown: &[&sketcraft_engine::doc::ModelBody], shaded: bool, whole: bool) -> Instances {
        let kept = app.viewport.instanced_tris.lock().map(|mut k| if whole { std::mem::take(&mut *k) } else { k.clone() }).unwrap_or_default();
        let mut candidates = std::collections::HashMap::new();
        if shaded {
            let mut uses: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
            let placed: Vec<_> = shown.iter().filter_map(|b| Some((b.name.clone(), b.body.is_mesh(), shared_mesh(app, b)?))).collect();
            for (_, _, (mesh, _)) in &placed {
                *uses.entry(std::sync::Arc::as_ptr(mesh) as usize).or_default() += 1;
            }
            candidates = placed
                .into_iter()
                .filter_map(|(name, reference, (mesh, placement))| {
                    let lod_eligible =
                        mesh.triangles.len() >= INSTANCE_ALONE || uses.get(&(std::sync::Arc::as_ptr(&mesh) as usize)).is_some_and(|n| *n > 1);
                    (reference || lod_eligible).then_some((name, (mesh, placement, lod_eligible)))
                })
                .collect();
        }
        Instances { candidates, meshes: Vec::new(), by_mesh: std::collections::HashMap::new(), kept, lods: app.viewport.mesh_lods.clone() }
    }

    /// Draw `b` as an instance when it is one, in colour `col` (its alpha for a see-through
    /// `look`): its shared mesh and placement.
    fn place(
        &mut self,
        b: &sketcraft_engine::doc::ModelBody,
        col: crate::gpu::PaletteIndex,
        look: crate::gpu::InstanceLook,
        oit: bool,
    ) -> Option<(std::sync::Arc<sketcraft_engine::geom::Mesh>, sketcraft_engine::doc::Mat)> {
        let (mesh, m, lod_eligible) = self.candidates.get(&b.name)?.clone();
        // A smaller single reference mesh formerly used the sorted CPU glass stream.
        if !lod_eligible && matches!(look, crate::gpu::InstanceLook::Glass | crate::gpu::InstanceLook::Faded) && !oit {
            return None;
        }
        let rows = [0, 1, 2].map(|r| [m[0][r] as f32, m[1][r] as f32, m[2][r] as f32, m[3][r] as f32]);
        let key = std::sync::Arc::as_ptr(&mesh) as usize;
        let at = match self.by_mesh.get(&(key, look)).copied() {
            Some(at) => at,
            None => {
                // The same triangles (and levels) for each look of a mesh.
                let made = self.meshes.iter().find(|(k, _)| std::sync::Arc::ptr_eq(k, &mesh)).map(|(_, i)| (i.tris.clone(), i.levels.clone()));
                let (tris, levels) = match made {
                    Some(made) => made,
                    None => (
                        match self.kept.get(&key).filter(|(k, _)| std::sync::Arc::ptr_eq(k, &mesh)) {
                            Some((_, t)) => t.clone(),
                            None => taken_shared_tris(&mesh).unwrap_or_else(|| std::sync::Arc::new(crate::gpu::SharedTris::new(&mesh))),
                        },
                        // Keep the existing detail of newly instanced single reference meshes.
                        if lod_eligible { self.lods.levels(&mesh).map(|l| l.as_ref().clone()).unwrap_or_default() } else { Vec::new() },
                    ),
                };
                self.meshes.push((
                    mesh.clone(),
                    crate::gpu::InstancedMesh { key: key as u64, tris, instances: Vec::new(), look, levels, closed: b.is_closed() },
                ));
                self.by_mesh.insert((key, look), self.meshes.len() - 1);
                self.meshes.len() - 1
            }
        };
        let (_, inst) = self.meshes.get_mut(at)?;
        inst.place(rows, col);
        Some((mesh, m))
    }

    /// The instanced meshes, keeping their triangles for the next scene (and, for a scene of
    /// only some bodies, the others' too: their levels of detail stay).
    fn finish(self, app: &SolveApp, whole: bool) -> Vec<crate::gpu::InstancedMesh> {
        let kept: InstancedTris =
            self.meshes.iter().map(|(mesh, inst)| (std::sync::Arc::as_ptr(mesh) as usize, (mesh.clone(), inst.tris.clone()))).collect();
        if whole {
            self.lods.retain(&kept.keys().copied().collect());
        }
        if let Ok(mut k) = app.viewport.instanced_tris.lock() {
            if whole {
                *k = kept;
            } else {
                k.extend(kept);
            }
        }
        self.meshes.into_iter().map(|(_, inst)| inst).collect()
    }
}

/// The shared mesh `b`'s faces are drawn from and the move that places it ([`drawn_mesh`]
/// without making the moved copy); `None` for a body drawn finer than its display quality.
fn shared_mesh(
    app: &SolveApp,
    b: &sketcraft_engine::doc::ModelBody,
) -> Option<(std::sync::Arc<sketcraft_engine::geom::Mesh>, sketcraft_engine::doc::Mat)> {
    use sketcraft_engine::doc::display_quality::{DisplayQuality, current};
    if refined(app, b).is_some() {
        return None;
    }
    let q = current();
    Some(if q == DisplayQuality::Normal { b.mesh_in_place() } else { b.plan_mesh_in_place_at(q).unwrap_or_else(|| b.mesh_in_place()) })
}

pub(crate) fn build_scene(app: &SolveApp) -> GpuScene {
    // Bodies where their occurrences place them.
    let st = crate::open_meshing::shown_world(app);
    // The blocks the last scene built, reused when their bodies come back as they were.
    let mut kept = app.viewport.blocks.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
    let (mut sc, made) = body_scene(app, &st, |_| true, true, &mut kept);
    keep_body_tris(app, &made);
    if let Ok(mut b) = app.viewport.blocks.lock() {
        *b = made;
    }
    sc.ground_ao = ground_ao(app, ground_bounds(app, &st));
    section_outlines(&mut sc, app, &st);
    continuity_seams(&mut sc, app);
    isocurves(&mut sc, app);
    crate::edit_in_place::draw_context(app, &mut sc);
    sc
}

/// Section: the outline of each body where the plane cuts it, in the view's text colour (#4146),
/// and the cut solid bodies' hatches by part on their palette entries.
fn section_outlines(sc: &mut GpuScene, app: &SolveApp, st: &sketcraft_engine::doc::ModelState) {
    let Some((o, n)) = section_plane(app) else { return };
    let s = &app.session;
    let active_bodies = active_body_names(app);
    let col = sc.color(c4(Tokens::view().text.gamma_multiply(0.9)));
    let mut kept = app.viewport.section_index.lock().map(|mut k| std::mem::take(&mut *k)).unwrap_or_default();
    let mut made = std::collections::HashMap::with_capacity(kept.len());
    // The closed solid bodies the plane cuts and where it cuts them: their hatches by part.
    let by_part = app.preferences.section_hatch == crate::prefs::SectionHatch::ByPart;
    let mut cuts: Vec<(&str, Vec<(Vec3, Vec3)>)> = Vec::new();
    for b in st.bodies.iter().filter(|b| app.body_visible(&b.name)) {
        let mesh = b.mesh();
        let key = std::sync::Arc::as_ptr(&mesh) as usize;
        let index = match kept.remove(&key).filter(|(m, _)| std::sync::Arc::ptr_eq(m, &mesh)) {
            Some((_, index)) => index,
            None => std::sync::Arc::new(sketcraft_engine::view::SectionIndex::new(&mesh)),
        };
        let segments = index.segments(&mesh, o, n);
        for (a, c) in &segments {
            sc.line(a.to_f32(), c.to_f32(), col, 1.8, false);
        }
        made.insert(key, (mesh, index));
        let solid = b.is_closed() && !body_faded(app, b, &active_bodies) && s.doc.model_body_opacity(b) >= 0.999;
        if by_part && solid && !segments.is_empty() {
            cuts.push((b.name.as_str(), segments));
        }
    }
    // Each cut body's hatch on all its palette entries (its colour, faces and placements).
    let parts: Vec<sketcraft_engine::render::hatch::CutPart> = cuts
        .iter()
        .map(|(name, segments)| sketcraft_engine::render::hatch::CutPart { seed: sketcraft_engine::render::hatch::seed(name), segments })
        .collect();
    let hatches = sketcraft_engine::render::hatch::hatches(&parts, n);
    let arena = app.viewport.palette.lock().unwrap_or_else(|e| e.into_inner());
    let palette = std::sync::Arc::make_mut(&mut sc.palette);
    for ((name, _), hatch) in cuts.iter().zip(hatches) {
        let Some(slots) = arena.slots(name) else { continue };
        for slot in (0..slots.face_capacity as usize).map(|f| slots.face(f)).chain([slots.body, slots.instance]) {
            palette.set_hatch(slot, hatch);
        }
    }
    if let Ok(mut k) = app.viewport.section_index.lock() {
        *k = made;
    }
}

/// Isocurve Analysis: the isoparametric curves of each face it shows, on the faces (depth
/// tested, as edges are).
fn isocurves(sc: &mut GpuScene, app: &SolveApp) {
    let iso = app.session.isocurve_lines();
    if !iso.is_empty() {
        let col = sc.color(c4(Tokens::get().accent));
        for line in iso.iter().flat_map(|lines| lines.iter()) {
            for w in line.points.windows(2) {
                if let (Some(a), Some(b)) = (w.first(), w.get(1)) {
                    sc.line(a.to_f32(), b.to_f32(), col, 1.4, false);
                }
            }
        }
    }
}

/// Continuity analysis: each seam `inspect.continuity` measured, drawn over the model in the
/// colour of its worst verdict.
fn continuity_seams(sc: &mut GpuScene, app: &SolveApp) {
    let tk = Tokens::get();
    let s = &app.session;
    for (seam, detail) in s.continuity.iter().zip(s.continuity_details()) {
        if !detail.current {
            continue;
        }
        let col = sc.color(c4(continuity_color(&tk, seam.order)));
        let width = seam_width(seam.order);
        for w in seam.points.windows(2) {
            if let (Some(a), Some(b)) = (w.first(), w.get(1)) {
                sc.line(a.to_f32(), b.to_f32(), col, width, true);
            }
        }
    }
}

/// Where each CPU-built body's triangles are in the model's blocks, for whole-body
/// highlights (see `highlight_block_tris`).
fn keep_body_tris(app: &SolveApp, made: &std::collections::HashMap<u64, CachedBlock>) {
    let mut body_tris = std::collections::HashMap::new();
    for (key, c) in made {
        for range in &c.ranges {
            if !range.vertices.is_empty() {
                body_tris
                    .insert(range.name.clone(), crate::gpu::BlockTris { block: *key, tris: range.opaque.clone(), vertices: range.vertices.clone() });
            }
        }
    }
    if let Ok(mut m) = app.viewport.body_tris.lock() {
        *m = body_tris;
    }
}

/// The scene of the shown bodies of `st` that `takes` (by their place in it) and nothing else,
/// built from the blocks in `kept` where their bodies come back as they were; with the blocks
/// it holds by key. `whole`: all of the shown bodies (see [`Instances::new`]).
fn body_scene(
    app: &SolveApp,
    st: &sketcraft_engine::doc::ModelState,
    takes: impl Fn(usize) -> bool,
    whole: bool,
    kept: &mut std::collections::HashMap<u64, CachedBlock>,
) -> (GpuScene, std::collections::HashMap<u64, CachedBlock>) {
    let tk = Tokens::get();
    let s = &app.session;
    let active_bodies = active_body_names(app);
    let (shaded, edges) = match app.ui.visual_style {
        1 => (true, false),
        2 => (false, true),
        _ => (true, true),
    };
    let edge_col = if shaded { c4(crate::sketchup_screen::edge(app).unwrap_or(tk.body_edge)) } else { c4(tk.text) };
    // Shaded with hidden edges: edges behind the model show faintly.
    let hidden_edges = app.ui.visual_style == 3;
    // Vertex markers are an option (#7834), off by default.
    let markers = app.preferences.vertex_markers;
    let hidden_col = c4(tk.text.gamma_multiply(0.35));
    // Every shown body, and the ones this scene takes. The arena and the appearance cache
    // keep every shown body, so a scene of some of them (a drag drawn as a layer) leaves the
    // others' slots and looks as they were.
    let visible = |b: &&sketcraft_engine::doc::ModelBody| app.body_visible(&b.name) && !app.preview.effective_replaced().contains(&b.name);
    let shown_names: Vec<&str> = st.bodies.iter().filter(visible).map(|b| b.name.as_str()).collect();
    let shown: Vec<&sketcraft_engine::doc::ModelBody> =
        st.bodies.iter().enumerate().filter(|(i, b)| takes(*i) && visible(b)).map(|(_, b)| b).collect();
    let mut arena = app.viewport.palette.lock().unwrap_or_else(|e| e.into_inner());
    arena.sync(shown_names.iter().copied());
    // Allocate every body's slots before the scene's palette is copied, so the stable prefix
    // covers them; the colours themselves are written in the loop below, into both the arena
    // and the scene's palette.
    let slots: Vec<_> = shown.iter().map(|b| arena.body(&b.name, b.body.face_count())).collect();
    let mut sc = GpuScene::with_palette(crate::gpu::Palette::with_stable(arena.entries()));
    // Resolve looks once per body and appearance state: a theme, visibility or LOD rebuild does
    // not scan the whole design's face appearances again.
    let mut appearance = app.viewport.appearance.lock().unwrap_or_else(|e| e.into_inner());
    let doc = std::sync::Arc::as_ptr(&s.doc) as usize;
    let colored = s.colored_components.is_some();
    if appearance.doc != Some((doc, colored)) {
        appearance.doc = Some((doc, colored));
        appearance.faces = face_appearance_hashes(&s.doc);
        // A design or colouring edit can key any body differently; the resolved looks whose
        // key comes back unchanged are still reused.
        appearance.keys.clear();
    }
    let mut made: std::collections::HashMap<u64, CachedBlock> = std::collections::HashMap::new();
    let mut run: Vec<BodyDraw> = Vec::new();
    let mut run_key = std::collections::hash_map::DefaultHasher::new();
    // Bodies drawn from the same triangles more than once (a part placed by many occurrences)
    // are drawn as instances of them: one copy of the triangles on the GPU, and none moved on
    // the CPU.
    let mut instances = Instances::new(app, &shown, shaded, whole);
    let mut thread_budget = sketcraft_engine::cosmetic_threads::Budget::default();
    for (b, slots) in shown.into_iter().zip(slots) {
        let faded = body_faded(app, b, &active_bodies);
        let identity = match appearance.keys.get(&b.name).copied() {
            Some(key) => key,
            None => {
                let key = body_appearance_key(&s.doc, b, &appearance.faces, colored);
                appearance.keys.insert(b.name.clone(), key);
                key
            }
        };
        let resolved = match appearance.bodies.get(&b.name) {
            Some((key, resolved)) if *key == identity => resolved.clone(),
            _ => {
                let resolved = std::sync::Arc::new(resolve_appearance(app, b));
                appearance.bodies.insert(b.name.clone(), (identity, resolved.clone()));
                #[cfg(test)]
                {
                    appearance.resolved = appearance.resolved.saturating_add(1);
                }
                resolved
            }
        };
        set_palette_color(&mut arena, &mut sc.palette, slots.edge, edge_col);
        set_palette_color(&mut arena, &mut sc.palette, slots.hidden_edge, hidden_col);
        if app.preferences.cosmetic_threads
            && thread_budget.available()
            && shaded
            && !faded
            && thread_budget.has_thread(&b.name, &st.threads)
            && resolved.opacity >= 0.999
            && resolved.face_looks.values().all(|l| l.opacity >= 0.999)
        {
            let mesh = shown_mesh(app, b, &active_bodies);
            for (a, c) in sketcraft_engine::cosmetic_threads::segments_with_budget(b, &mesh, &st.threads, &mut thread_budget) {
                sc.line(a.to_f32(), c.to_f32(), slots.edge, 0.85, false);
            }
        }
        let base = resolved.component.or(resolved.body).unwrap_or(crate::sketchup_screen::face(app).unwrap_or(tk.body));
        let col = if faded { c4(base.gamma_multiply(0.35)) } else { c4(base) };
        set_palette_color(&mut arena, &mut sc.palette, slots.body, col);
        // Appearances: the body's opacity, and faces with looks of their own.
        let opacity = if app.ui.visual_style == 4 { resolved.opacity.min(0.45) } else { resolved.opacity };
        let face_looks = &resolved.face_looks;
        let faces = b.body.face_count();
        let mut face_colors = Vec::with_capacity(faces);
        let mut face_classes = Vec::with_capacity(faces);
        for f in 0..faces {
            let look = face_looks.get(&f);
            let color = look.map_or(base, |l| Color32::from_rgb(l.color[0], l.color[1], l.color[2]));
            let opacity = look.map_or(opacity, |l| if app.ui.visual_style == 4 { l.opacity.min(0.45) } else { l.opacity });
            let (rgba, class) = if faded {
                (c4(color.gamma_multiply(0.35)), crate::gpu::TriClass::Faded)
            } else if opacity < 0.999 || app.ui.visual_style == 4 {
                let mut rgba = c4(color);
                rgba[3] = (opacity.clamp(0.05, 1.0) * 255.0).round() as u8;
                (rgba, crate::gpu::TriClass::Glass)
            } else {
                (c4(color), crate::gpu::TriClass::Opaque)
            };
            let slot = slots.face(f);
            set_palette_color(&mut arena, &mut sc.palette, slot, rgba);
            face_colors.push(slot);
            face_classes.push(class);
        }
        let closed = b.is_closed();
        let class = plan_class(app, &face_classes);
        let planned = class.and_then(|class| drawn_plan(app, b, class == crate::gpu::TriClass::Opaque).map(|plan| (plan, class)));
        let on_gpu = planned.is_some();
        if let Some((plan, class)) = planned {
            sc.planned.push(crate::gpu::PlannedBody { plan, face_colors: face_colors.clone(), class, closed });
        }
        // Opaque, faded and see-through bodies alike, in one colour (faces of their own colour
        // are built as before).
        let look = if faded {
            crate::gpu::InstanceLook::Faded
        } else if opacity < 0.999 || app.ui.visual_style == 4 {
            crate::gpu::InstanceLook::Glass
        } else {
            crate::gpu::InstanceLook::Opaque
        };
        let alpha = match look {
            crate::gpu::InstanceLook::Faded => col[3],
            crate::gpu::InstanceLook::Glass => (opacity.clamp(0.05, 1.0) * 255.0).round() as u8,
            crate::gpu::InstanceLook::Opaque => 255,
        };
        set_palette_color(&mut arena, &mut sc.palette, slots.instance, [col[0], col[1], col[2], alpha]);
        let instanced =
            (!on_gpu && face_looks.is_empty()).then(|| instances.place(b, slots.instance, look, app.viewport.gpu.is_some_and(|g| g.oit))).flatten();
        let name = b.name.clone();
        let body = match instanced {
            Some((mesh, place)) => BodyDraw {
                name,
                mesh,
                place: Some(place),
                face_colors,
                body_color: slots.body,
                face_classes,
                faded,
                closed,
                on_gpu: true,
                slot_base: slots.faces,
                edge: slots.edge,
                hidden_edge: slots.hidden_edge,
            },
            None => BodyDraw {
                name,
                mesh: drawn_mesh(app, b),
                place: None,
                face_colors,
                body_color: slots.body,
                face_classes,
                faded,
                closed,
                on_gpu,
                slot_base: slots.faces,
                edge: slots.edge,
                hidden_edge: slots.hidden_edge,
            },
        };
        let key = body.key(shaded, edges, hidden_edges, markers);
        key.hash(&mut run_key);
        run.push(body);
        // Blocks end where a body's own key says, so a body added or removed changes only its
        // own block, not every block after it.
        if key.is_multiple_of(16) || run.len() >= 64 {
            let draws = std::mem::take(&mut run);
            let block_key = std::mem::replace(&mut run_key, std::collections::hash_map::DefaultHasher::new()).finish();
            push_block(&mut sc, kept, &mut made, block_key, draws, shaded, |d, out| d.build(out, shaded, edges, hidden_edges, markers));
        }
    }
    if !run.is_empty() {
        push_block(&mut sc, kept, &mut made, run_key.finish(), run, shaded, |d, out| d.build(out, shaded, edges, hidden_edges, markers));
    }
    // Every shown body has its entries, so only a larger map holds bodies no longer shown.
    if appearance.bodies.len() > shown_names.len() || appearance.keys.len() > shown_names.len() {
        let names: std::collections::HashSet<&str> = shown_names.iter().copied().collect();
        appearance.bodies.retain(|name, _| names.contains(name.as_str()));
        appearance.keys.retain(|name, _| names.contains(name.as_str()));
    }
    sc.instanced = instances.finish(app, whole);
    (sc, made)
}

/// The colour of a measured seam whose highest order met is `order`, from the theme's
/// `continuity` tokens by its verdict ([`SeamVerdict::of`]): curvature continuous (G2 and up),
/// tangent with a curvature jump (G1), a crease (G0), and faces that do not meet.
///
/// [`SeamVerdict::of`]: sketcraft_engine::kernel::SeamVerdict::of
pub(crate) fn continuity_color(t: &Tokens, order: Option<sketcraft_engine::kernel::Continuity>) -> Color32 {
    use sketcraft_engine::kernel::SeamVerdict as V;
    let [smooth, jump, sharp, gap] = t.continuity;
    match V::of(order) {
        V::Smooth => smooth,
        V::CurvatureJump => jump,
        V::Sharp => sharp,
        V::Gap => gap,
    }
}

/// The stroke width of a measured seam: 2.6 px, and one more for each order past G2 the
/// curvature-continuous seams meet (G3, G4), which share G2's colour.
pub(crate) fn seam_width(order: Option<sketcraft_engine::kernel::Continuity>) -> f32 {
    let past_g2 = order.map_or(0, |g| g.order().saturating_sub(2));
    2.6 + past_g2 as f32
}

/// Rebuild the sketch scene when what it shows has changed.
pub(crate) fn refresh_sketch_scene(app: &mut SolveApp, proj: &Proj) {
    let sk = sketch_scene_key(app, proj);
    if sk != app.viewport.sketch_key {
        let scene = build_sketch_scene(app, proj);
        if let Ok(mut slot) = app.viewport.sketch_slot.lock() {
            *slot = Some(scene);
        }
        app.viewport.sketch_key = sk;
    }
}

/// View-dependent geometry: grid, sketch feedback and the ground shadow's visibility.
pub(crate) fn build_sketch_scene(app: &SolveApp, proj: &Proj) -> GpuScene {
    let tk = Tokens::get();
    let mut sc = GpuScene::default();
    let s = &app.session;
    let st = crate::open_meshing::shown_world(app);
    if app.ui.ground_shadow && app.ui.visual_style != 2 && app.cam.pitch > 0.0 {
        let geometry = shadow_geometry(app, &st);
        sc.shadow = geometry.bytes.as_ref().clone();
        sc.palette = geometry.palette;
    } else if let Ok(mut cache) = app.viewport.shadow.lock()
        && cache.as_ref().is_some_and(|c| !std::sync::Arc::ptr_eq(&c.state, &st))
    {
        // A hidden shadow must not keep an earlier document's whole model alive.
        *cache = None;
    }
    let mut fill_budget = 131_072;
    // The open sketch's plane in the model this frame shows.
    let sketch_plane = s.active_sketch.and_then(|id| st.sketch(id)).map(|ss| ss.plane).or_else(|| previewed_sketch_plane(app));
    // An open sketch has its grid whatever the Grid switch says, unless the sketch's own Sketch
    // Grid option (Sketch panel) is off (#9147); outside a sketch that option is never off.
    if (sketch_plane.is_some() || app.ui.show_grid) && !active_view(app).hide_grid {
        let major = grid_step(app.cam.half_height()).1;
        // An open sketch has its grid on its own plane, and so does the plane Create Sketch
        // would take; otherwise the ground plane: XY with Z up, XZ with Y up (laid out in the
        // camera's Z-up frame), or the upright origin plane facing the view when the ground is
        // seen edge-on.
        let (c, u, v, slant, ext, far_reach, w3): (Vec3, Vec3, Vec3, f64, f64, f64, Box<dyn Fn(Vec3) -> [f32; 3]>) = if let Some(pl) = &sketch_plane {
            let (c, u, v) = sketcraft_engine::view::sketch_grid_frame(pl, app.cam.target, major);
            // How steeply the sketch's plane is seen: the grid reaches as far on it as the view
            // is wide, so the fade with the distance does too (#4214). The sketch's plane is in
            // the world, as the camera's direction is.
            let n = u.cross(v);
            let slant = app.cam.back().dot(n);
            let ext = grid_ext(app.cam.half_height(), major, slant);
            let far_reach = sketcraft_engine::view::grid_far_reach(&proj.cam, picture_far(app, &proj.cam), c, n);
            (c, u, v, slant, ext, far_reach, Box::new(|p| p.to_f32()))
        } else {
            let tf = app.cam.to_frame(app.cam.target);
            // On the ground (XY) unless the Grid follows the view preference is on (#1470); then on
            // the plane the last frame drew, so the choice is hysteretic near the thresholds (#1993).
            let (u, v) = if app.preferences.grid_follows_view {
                app.viewport.grid_plane.lock().map_or((Vec3::X, Vec3::Y), |mut last| {
                    *last = grid_plane(app.cam.to_frame(app.cam.back()), *last);
                    last.axes()
                })
            } else {
                (Vec3::X, Vec3::Y)
            };
            // Centred where the view's axis meets the ground, the part of the ground in the middle
            // of the view (#3087): the camera's target may lie off the ground (above a part, or
            // below the ground after an orbit), and zoomed in, a grid round the point under the
            // target was off screen. Seen at a slant the ground stretches across the view, so the
            // grid reaches further.
            let back = app.cam.to_frame(app.cam.back());
            let n = u.cross(v);
            let slant = back.dot(n);
            let ext = grid_ext(app.cam.half_height(), major, slant);
            let centre = grid_centre(tf, back, n, ext);
            let snap = |a: Vec3| a * ((centre.dot(a) / major).round() * major);
            let c = snap(u) + snap(v);
            // The far plane cuts the ground, and the grid's plane is the camera's frame's.
            let far_reach =
                sketcraft_engine::view::grid_far_reach(&proj.cam, picture_far(app, &proj.cam), app.cam.from_frame(c), app.cam.from_frame(n));
            (c, u, v, slant, ext, far_reach, Box::new(|p| app.cam.from_frame(p).to_f32()))
        };
        // The grid fades out with the distance from the middle of the view (#4214): it used to
        // stop at a fixed distance, so its edge showed as a cut across the picture. Each line is
        // now drawn in pieces, each as faint as the fade makes it there, and the grid is gone
        // before its own edge, which is therefore never seen.
        let fade = GridFade::new(app.cam.half_height(), slant, ext, far_reach);
        let (grid, grid_major) = (c4(tk.grid), c4(tk.grid_major));
        // Near a switch of the spacing the outgoing and incoming levels cross-fade (#9570), each
        // line on its own place on the plane: the grid's centre on each axis, from the plane's
        // origin (the sketch plane's, or the camera frame's).
        let origin = sketch_plane.as_ref().map_or(Vec3::ZERO, |pl| pl.origin);
        let (at_u, at_v) = ((c - origin).dot(u), (c - origin).dot(v));
        // Two lines through the point `c + u * t`: one along v, one along u.
        for (along, across, at) in [(v, u, at_u), (u, v, at_v)] {
            for line in grid_lines(app.cam.half_height(), at, fade.end, 300) {
                let t = line.offset;
                // A line two levels share with different kinds blends between the minor and
                // the major look.
                let col = grid_blend(grid, grid_major, line.major);
                let wd = 0.7 + 0.3 * line.major;
                for (s0, s1, alpha) in fade.pieces(t) {
                    let col = sc.color(grid_colour(col, alpha * line.weight));
                    sc.under_line(w3(c + across * t + along * s0), w3(c + across * t + along * s1), col, wd);
                }
            }
        }
    }
    // While a dimension's value is typed, the open sketch as that value leaves it. Sampled past
    // the display cache, which holds the design's own solve for picking.
    let typed = crate::dim_view::preview_world(app);
    let ppp = app.viewport.pixels_per_point();
    let mut uncached = crate::sketch_display::Uncached::default();
    for sid in visible_sketches(app) {
        let active = s.active_sketch == Some(sid);
        let typed = typed.as_ref().filter(|_| active);
        let Some(ss) = typed.unwrap_or(&st).sketch(sid) else { continue };
        // Its curves are sampled before its profiles, which share their budget: past it a fill
        // goes missing rather than a curve.
        let mut typed_curves: std::collections::HashMap<usize, Vec<Vec<Vec3>>> = match typed {
            Some(_) => (0..ss.sketch.curves.len())
                .filter(|i| sketcraft_engine::view::sketch_curve_visible(&ss.sketch, *i))
                .map(|i| (i, uncached.curve(&ss.sketch, i, &ss.plane, proj, ppp)))
                .collect(),
            None => std::collections::HashMap::new(),
        };
        // Closed profiles are shaded (lifted off a face they may lie on).
        let lift = ss.plane.normal() * 0.01;
        let profiles = if app.session.visibility.hidden_profiles.contains(&sid) || ss.sketch.view.hide_profile { &[][..] } else { &ss.profiles[..] };
        let profile_fill = sc.color(c4(tk.profile_fill));
        for (index, profile) in profiles.iter().enumerate() {
            let (sampled, cached);
            let display = if typed.is_some() {
                sampled = uncached.profile(&profile.region, &ss.plane, proj, ppp);
                &sampled
            } else {
                cached = crate::sketch_display::profile_region(app, sid, index, &ss.plane, proj);
                cached.as_ref()
            };
            for tri in display.as_ref().map_or_else(Vec::new, |region| crate::sketch_display::fill_triangles(region, &mut fill_budget)) {
                for q in tri {
                    sc.trans_tri((ss.plane.to_world(q) + lift).to_f32(), [0.0; 3], profile_fill);
                }
            }
        }
        // Construction curves are dashed, centerlines dash-dot (lengths follow the grid).
        // When enabled, a finished sketch's own curves sit on a wider halo (#4862) to read on a
        // shaded face. Projected (reference) curves get none: they trace the face's own edges.
        // All the halos go first, so one curve's halo never cuts into another where they meet.
        let unit = grid_step(app.cam.half_height()).0;
        let mut strokes = Vec::new();
        // The open sketch's curves a failing constraint holds take the conflict colour (#7244).
        // The lines come curve by curve, each curve's parts in the order its sampler returned
        // them, before the virtual corners and 3D curves: `owners` names each part's curve.
        let conflict = if active { Conflict::of(app, &ss.sketch, &ss.report) } else { Conflict::default() };
        let mut owners = Vec::new();
        let lines = sketch_lines_with_sampler(&ss.sketch, &ss.plane, active, &ss.report.curve_determined, |_, index, plane| {
            let parts = match typed {
                Some(_) => typed_curves.remove(&index).unwrap_or_default(),
                None => crate::sketch_display::curve_sample(app, sid, index, plane, proj).parts.clone(),
            };
            owners.extend(std::iter::repeat_n(index, parts.len()));
            parts
        });
        for (k, (pts, col, cons)) in lines.into_iter().enumerate() {
            let conflicting = owners.get(k).is_some_and(|&curve| conflict.curve(curve));
            let segs = sketch_segments(&pts, sketch_line_mode(col, cons), unit);
            let haloed = app.preferences.sketch_halo && !active && col != colors::SKETCH_PROJECTED && col != colors::SKETCH_LOST;
            let colour = if conflicting { c4(tk.sketch_conflict) } else { sketch_color(&tk, col) };
            strokes.push((segs, sc.color(colour), if cons { 1.2 } else { 2.0 }, haloed));
        }
        // Active sketches retain their existing through-body display. Inactive sketches
        // can opt into the same behaviour in Preferences.
        let through = active || app.preferences.reference_geometry_through_bodies;
        let halo = sc.color(c4(tk.sketch_halo));
        for (segs, _, width, _) in strokes.iter().filter(|stroke| stroke.3) {
            for (a, b) in segs {
                sc.line(a.to_f32(), b.to_f32(), halo, width + SKETCH_HALO_WIDTH, through);
            }
        }
        for (segs, color, width, _) in &strokes {
            for (a, b) in segs {
                sc.line(a.to_f32(), b.to_f32(), *color, *width, through);
            }
        }
    }
    let guide_unit = grid_step(app.cam.half_height()).0;
    let warning = sc.color(c4(tk.warning));
    for points in crate::sketch_attachment_ui::display_guide_lines(app, proj) {
        for (a, b) in dashes(&points, &[guide_unit * 0.25, guide_unit * 0.15]) {
            sc.line(a.to_f32(), b.to_f32(), warning, 1.5, true);
        }
    }
    sc
}

#[derive(Clone)]
struct ShadowGeometry {
    key: u64,
    state: std::sync::Arc<sketcraft_engine::doc::ModelState>,
    bytes: std::sync::Arc<Vec<u8>>,
    palette: std::sync::Arc<crate::gpu::Palette>,
}

/// Changing sides of the ground only changes whether this small shadow is drawn; it never
/// rebuilds the model's bodies or uploads their plans and colours again.
fn shadow_geometry(app: &SolveApp, st: &std::sync::Arc<sketcraft_engine::doc::ModelState>) -> ShadowGeometry {
    let key = scene_key(app);
    if let Some(geometry) =
        app.viewport.shadow.lock().ok().and_then(|c| c.as_ref().filter(|c| c.key == key && std::sync::Arc::ptr_eq(&c.state, st)).cloned())
    {
        return geometry;
    }
    let mut scene = GpuScene::default();
    ground_shadow(&mut scene, app, ground_boxes(app, st));
    let geometry = ShadowGeometry { key, state: st.clone(), bytes: std::sync::Arc::new(scene.shadow), palette: scene.palette };
    if let Ok(mut cache) = app.viewport.shadow.lock() {
        *cache = Some(geometry.clone());
    }
    geometry
}

/// Each shown body's box in the camera's Z-up frame (the ground is its XY plane).
fn ground_boxes(app: &SolveApp, st: &std::sync::Arc<sketcraft_engine::doc::ModelState>) -> Vec<sketcraft_engine::geom::Aabb3> {
    let boxes = body_bounds(app, st);
    let shown = st.bodies.iter().zip(boxes.iter()).filter(|(x, (_, bb))| !bb.is_empty() && app.body_visible(&x.name));
    shown.map(|(_, (_, bb))| frame_bounds(app, box_corners(bb).into_iter())).collect()
}

/// The box around all shown bodies in the camera's Z-up frame.
fn ground_bounds(app: &SolveApp, st: &std::sync::Arc<sketcraft_engine::doc::ModelState>) -> sketcraft_engine::geom::Aabb3 {
    ground_boxes(app, st).iter().fold(sketcraft_engine::geom::Aabb3::EMPTY, |a, b| a.union(b))
}

/// The corners of a box.
fn box_corners(bb: &sketcraft_engine::geom::Aabb3) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { bb.min.x } else { bb.max.x },
            if i & 2 == 0 { bb.min.y } else { bb.max.y },
            if i & 4 == 0 { bb.min.z } else { bb.max.z },
        )
    })
}

/// The ground the model's ambient occlusion darkens (#3413) under the bodies whose box in the
/// camera's frame is `bounds`; none in Wireframe.
fn ground_ao(app: &SolveApp, bounds: sketcraft_engine::geom::Aabb3) -> Option<sketcraft_gpu::view::GroundAo> {
    if app.ui.visual_style == 2 {
        return None;
    }
    let radius = (bounds.diagonal() * 0.04).clamp(1e-4, 1e6) as f32;
    sketcraft_gpu::view::GroundAo::new(bounds, ground_height(app, bounds), app.cam.up, radius)
}

/// The box around `corners` in the camera's Z-up frame.
fn frame_bounds(app: &SolveApp, corners: impl Iterator<Item = Vec3>) -> sketcraft_engine::geom::Aabb3 {
    // The bodies' box in the camera's Z-up frame (the ground is its XY plane).
    let mut b = sketcraft_engine::geom::Aabb3::EMPTY;
    for p in corners {
        b.add(app.cam.to_frame(p));
    }
    b
}

/// Both ground effects share the selected plane, independent of the origin grid.
fn ground_height(app: &SolveApp, bounds: sketcraft_engine::geom::Aabb3) -> f64 {
    // Raised bodies cast onto the origin. Keep the plane below lower bodies, and
    // retain the former floating ground when requested.
    let ground = if app.preferences.ground_shadow_follows_model { bounds.min.z } else { bounds.min.z.min(0.0) };
    ground - 1e-3 * (1.0 + bounds.diagonal())
}

pub fn calculate_sun_direction(time_hours: f64, day_of_year: u32, latitude_deg: f64) -> solidcore_record::Vec3 {
    use std::f64::consts::PI;
    let rad = |deg: f64| deg * PI / 180.0;
    let lat = rad(latitude_deg);
    let declination = rad(23.45 * ((360.0 / 365.0) * (day_of_year as f64 - 81.0)).to_radians().sin());
    let hour_angle = rad(15.0 * (time_hours - 12.0));
    let sin_alt = lat.sin() * declination.sin() + lat.cos() * declination.cos() * hour_angle.cos();
    let alt = sin_alt.clamp(-1.0, 1.0).asin();
    let cos_az = (declination.sin() - sin_alt * lat.sin()) / (alt.cos().max(1e-4) * lat.cos().max(1e-4));
    let mut az = cos_az.clamp(-1.0, 1.0).acos();
    if hour_angle > 0.0 {
        az = 2.0 * PI - az;
    }
    let sx = alt.cos() * az.sin();
    let sy = alt.cos() * az.cos();
    let sz = alt.sin();
    solidcore_record::Vec3::new(sx, sy, sz)
}

/// A soft dark ellipse on the ground under each group of shown bodies (darkest in the middle).
fn ground_shadow(sc: &mut GpuScene, app: &SolveApp, framed: Vec<sketcraft_engine::geom::Aabb3>) {
    let all = framed.iter().fold(sketcraft_engine::geom::Aabb3::EMPTY, |a, b| a.union(b));
    if all.is_empty() {
        return;
    }
    // Every ellipse lies on the lowest body's ground, so none floats beside a lower body.
    let z = ground_height(app, all);
    let mut dark = if crate::theme::viewport_dark() { 150 } else { 80 };
    let (sun, use_solar) = if app.preferences.solar_shadows {
        let s = calculate_sun_direction(
            app.preferences.solar_time_hours,
            app.preferences.solar_day_of_year,
            app.preferences.solar_latitude_deg,
        );
        (s, true)
    } else {
        (solidcore_record::Vec3::Z, false)
    };

    if use_solar && sun.z <= 0.02 {
        // Sun is at or below horizon: no solar ground shadow
        return;
    }

    if use_solar {
        dark = ((dark as f64) * (sun.z / 0.7).clamp(0.25, 1.0)).round() as u32;
    }

    for b in shadow_groups(framed) {
        let (c, (rx, ry)) = (b.center(), shadow_radii(&b));
        if !(rx > 0.0 && ry > 0.0) {
            continue;
        }
        let (cx, cy, rx_final, ry_final) = if use_solar && sun.z > 0.02 {
            let h = (c.z - z).max(b.size().z * 0.5);
            let dx = -h * (sun.x / sun.z);
            let dy = -h * (sun.y / sun.z);
            let elong = (1.0 / sun.z).clamp(1.0, 3.5);
            let horiz = (sun.x * sun.x + sun.y * sun.y).sqrt().max(1e-4);
            let rx_stretch = rx * (1.0 + (elong - 1.0) * (sun.x.abs() / horiz));
            let ry_stretch = ry * (1.0 + (elong - 1.0) * (sun.y.abs() / horiz));
            (c.x + dx, c.y + dy, rx_stretch, ry_stretch)
        } else {
            (c.x, c.y, rx, ry)
        };
        let at = |r: f64, a: f64| app.cam.from_frame(Vec3::new(cx + rx_final * r * a.cos(), cy + ry_final * r * a.sin(), z)).to_f32();
        shadow_ellipse(sc, dark, at);
    }
}

/// The half-axes of the ground shadow's ellipse under a box (in the camera's Z-up frame).
fn shadow_radii(b: &sketcraft_engine::geom::Aabb3) -> (f64, f64) {
    let size = b.size();
    (size.x * 0.62 + size.z * 0.15, size.y * 0.62 + size.z * 0.15)
}

/// The boxes the ground shadow draws one ellipse under each (#10442): bodies whose boxes meet on
/// the ground share one, and bodies apart get one each. One ellipse over all of them was darkest
/// in its middle, which for two bodies side by side is the empty gap between them.
fn shadow_groups(mut boxes: Vec<sketcraft_engine::geom::Aabb3>) -> Vec<sketcraft_engine::geom::Aabb3> {
    // More groups than this is a crowd (an array, an assembly): one ellipse under it all, as
    // before, keeps the scene small.
    const MOST: usize = 64;
    let meet = |a: &sketcraft_engine::geom::Aabb3, b: &sketcraft_engine::geom::Aabb3| {
        a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
    };
    // Merge until no two groups meet: a merged box can reach a group it missed before.
    let mut merged = true;
    while merged && boxes.len() > 1 {
        merged = false;
        let mut out: Vec<sketcraft_engine::geom::Aabb3> = Vec::with_capacity(boxes.len());
        for b in boxes {
            match out.iter_mut().find(|o| meet(o, &b)) {
                Some(o) => {
                    *o = o.union(&b);
                    merged = true;
                }
                None => out.push(b),
            }
            if out.len() > MOST {
                return vec![out.iter().fold(sketcraft_engine::geom::Aabb3::EMPTY, |a, b| a.union(b))];
            }
        }
        boxes = out;
    }
    boxes
}

/// One soft ellipse of shadow triangles, `at(r, angle)` placing a point `r` (0 middle, 1 rim)
/// of the way out.
fn shadow_ellipse(sc: &mut GpuScene, dark: u32, at: impl Fn(f64, f64) -> [f32; 3]) {
    // Rings from the middle out, fading to nothing.
    let rings = [(0.0, dark), (0.55, dark * 3 / 4), (0.85, dark / 3), (1.0, 0)];
    let n = 48;
    let angle = |k: usize| k as f64 / n as f64 * std::f64::consts::TAU;
    for w in rings.windows(2) {
        let ((r0, a0), (r1, a1)) = (w[0], w[1]);
        let (c0, c1) = (sc.color([0, 0, 0, a0 as u8]), sc.color([0, 0, 0, a1 as u8]));
        for k in 0..n {
            let (p00, p01, p10, p11) = (at(r0, angle(k)), at(r0, angle(k + 1)), at(r1, angle(k)), at(r1, angle(k + 1)));
            sc.shadow_tri(p00, c0);
            sc.shadow_tri(p10, c1);
            sc.shadow_tri(p11, c1);
            if r0 > 0.0 {
                sc.shadow_tri(p00, c0);
                sc.shadow_tri(p11, c1);
                sc.shadow_tri(p01, c0);
            }
        }
    }
}

/// The [`sketch_segments`] line type of a [`sketch_lines_with_sampler`] polyline: solid,
/// construction (dashed) or centerline (dash-dot).
fn sketch_line_mode(col: Rgb, cons: bool) -> u8 {
    if !cons {
        0
    } else if col == colors::SKETCH_CENTERLINE {
        2
    } else {
        1
    }
}

/// Sketch line types share the same world-space dash phase in the scene and overlays.
pub(crate) fn sketch_segments(pts: &[Vec3], mode: u8, unit: f64) -> Vec<(Vec3, Vec3)> {
    match mode {
        // Construction, and infinite lines (#1359), which are construction too.
        1 | 3 => dashes(pts, &[0.3 * unit, 0.2 * unit]),
        2 => dashes(pts, &[0.6 * unit, 0.15 * unit, 0.05 * unit, 0.15 * unit]),
        _ => pts.windows(2).map(|w| (w[0], w[1])).collect(),
    }
}

/// The drawn pieces of a polyline in a dash pattern (on, off, on, off… lengths).
pub(crate) fn dashes(pts: &[Vec3], pattern: &[f64]) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    if pattern.is_empty() || pattern.iter().any(|l| !(l.is_finite() && *l > 0.0)) {
        return pts.windows(2).map(|w| (w[0], w[1])).collect();
    }
    let (mut k, mut left) = (0usize, pattern[0]);
    for w in pts.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut len = (b - a).len();
        // Cap the pieces per segment so a huge zoom-out can't explode the scene.
        let mut guard = 0;
        while len > 1e-12 && guard < 10_000 {
            guard += 1;
            let step = left.min(len);
            let q = a + (b - a) * (step / len);
            if k % 2 == 0 {
                out.push((a, q));
            }
            left -= step;
            len -= step;
            a = q;
            if left <= 1e-12 {
                k = (k + 1) % pattern.len();
                left = pattern.get(k).copied().unwrap_or(1.0);
            }
        }
    }
    out
}

/// Colour as straight (unpremultiplied) sRGBA bytes for the GPU.
pub(crate) fn c4(c: Color32) -> [u8; 4] {
    c.to_srgba_unmultiplied()
}

/// Origin planes and axes currently shown, sized for the view: (name, normal, quad).
pub fn origin_planes(app: &SolveApp) -> Vec<(&'static str, Vec3, [Vec3; 4])> {
    if !app.origin_visible() {
        return Vec::new();
    }
    // The SketchUp model screen shows the axes alone, unless a plane is being picked.
    if crate::sketchup_screen::on(app) && !app.picking_plane() {
        return Vec::new();
    }
    // While the Origin is hidden, a dialog picking a plane brings the planes back: then every
    // plane; otherwise only those not hidden one by one. Project is the exception (#10778):
    // its pick takes planes too, but each line a plane gives, an axis already gives, so a
    // hidden Origin stays hidden for it.
    let reveal = !app.origin_shown() && app.picking_plane() && !app.picking_project_geometry();
    if !app.origin_shown() && !reveal {
        return Vec::new();
    }
    origin_quads(origin_size(app.cam.half_height()))
        .into_iter()
        .filter(|(n, _, _)| reveal || !app.session.visibility.hidden_origin.iter().any(|h| h == n))
        .collect()
}

/// The plane Create Sketch would put the sketch on: the origin or construction plane under the
/// pointer while it waits (#6975), or the plane selected in the view (#10804). The grid is drawn
/// on it, so the sketch's plane shows before the click.
pub fn previewed_sketch_plane(app: &SolveApp) -> Option<Plane> {
    if app.session.active_sketch.is_some() {
        return None;
    }
    if app.picking_sketch_plane() {
        let name = match (&app.viewport.hover_plane, &app.viewport.hover) {
            (Some(name), _) | (None, Some(Hit::Plane { name, .. })) => name,
            _ => return selected_plane(app),
        };
        return named_plane(app, name);
    }
    selected_plane(app)
}

/// The plane selected in the view, when there is one: Create Sketch takes it, so the grid shows it.
fn selected_plane(app: &SolveApp) -> Option<Plane> {
    let name = app.session.selection.iter().find_map(|sel| match sel {
        Sel::Plane { name } => Some(name.as_str()),
        _ => None,
    })?;
    named_plane(app, name)
}

/// Origin or construction plane `name`.
fn named_plane(app: &SolveApp, name: &str) -> Option<Plane> {
    sketcraft_engine::view::construction_planes(&app.session)
        .into_iter()
        .find(|(_, n, _)| n == name)
        .map(|(_, _, p)| p)
        .or_else(|| origin_planes(app).into_iter().find(|(n, _, _)| *n == name).and_then(|(n, _, _)| Plane::named(n)))
}

/// A point on origin plane `name`'s square that a click at its screen point picks: the middle,
/// or else the middle of a quarter the other planes don't cover from this view (in an iso view
/// the XZ square hides the middle of the XY one). The middle when no candidate is picked.
pub fn origin_plane_target(app: &SolveApp, proj: &Proj, name: &str) -> Option<Vec3> {
    let (_, _, q) = origin_planes(app).into_iter().find(|(n, _, _)| *n == name)?;
    let mid = (q[0] + q[1] + q[2] + q[3]) / 4.0;
    let picked = |w: &Vec3| proj.to_screen(*w).is_some_and(|p| matches!(pick(app, proj, p).first(), Some(Hit::Plane { name: n, .. }) if n == name));
    Some(std::iter::once(mid).chain(q.iter().map(|c| (mid + *c) / 2.0)).find(picked).unwrap_or(mid))
}

pub fn origin_axes(app: &SolveApp) -> Vec<(&'static str, Vec3)> {
    if !app.origin_visible() {
        return Vec::new();
    }
    [("X", Vec3::X), ("Y", Vec3::Y), ("Z", Vec3::Z)]
        .into_iter()
        .filter(|(n, _)| !app.session.visibility.hidden_origin.iter().any(|h| h == n))
        .collect()
}

/// Construction axes shown, as segments: across the model where the axis passes it (a little
/// past it either side), at least the origin axes' length around the axis's origin: (name,
/// from, to).
pub fn construction_axis_segments(app: &SolveApp) -> Vec<(String, Vec3, Vec3)> {
    let axes: Vec<_> =
        sketcraft_engine::view::construction_axes(&app.session).into_iter().filter(|(_, n, _, _)| app.session.construction_visible(n)).collect();
    if axes.is_empty() {
        return Vec::new();
    }
    let bb = view_bounds(app);
    axes.into_iter()
        .map(|(_, name, o, d)| {
            let (a, b) = axis_span(app, &bb, o, d);
            (name, a, b)
        })
        .collect()
}

/// How far an axis through `o` along `d` is drawn: across the model's box `bb` where it passes
/// it (a little past it either side), at least the origin axes' length around `o`.
fn axis_span(app: &SolveApp, bb: &sketcraft_engine::geom::Aabb3, o: Vec3, d: Vec3) -> (Vec3, Vec3) {
    let h = origin_size(app.cam.half_height()) * AXIS_LEN;
    let (mut lo, mut hi) = (-h, h);
    if !bb.is_empty() {
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { bb.min.x } else { bb.max.x },
                if i & 2 == 0 { bb.min.y } else { bb.max.y },
                if i & 4 == 0 { bb.min.z } else { bb.max.z },
            );
            let t = (c - o).dot(d);
            lo = lo.min(t - h * 0.3);
            hi = hi.max(t + h * 0.3);
        }
    }
    (o + d * lo, o + d * hi)
}

/// What a round edge or face stands for in the open dialog's active input, drawn as construction
/// geometry while it is under the cursor or picked: the axis an axis input takes (a dashed line
/// through a circle's centre along its normal, or along a cylinder's, cone's or torus's axis),
/// or the centre a point input takes from a circular edge (a small star).
pub(crate) fn round_pick_feedback(app: &SolveApp) -> (Vec<(Vec3, Vec3)>, Vec<Vec3>) {
    let (mut lines, mut centres): (Vec<(Vec3, Vec3, Vec3, Vec3)>, Vec<Vec3>) = (Vec::new(), Vec::new());
    let Some(d) = app.dialog.as_ref() else { return (Vec::new(), centres) };
    let Some(inp) = d.active_input() else { return (Vec::new(), centres) };
    let s = &app.session;
    let hovered = app.viewport.hover.as_ref().and_then(|h| d.candidate(s, h));
    let chained = crate::dialogs_move::chained(app);
    let carry = |body: &str, p: Vec3, dir: Vec3| match chain_of(chained.as_ref(), body) {
        Some(c) => (sketcraft_engine::doc::apply_point(&c, p), sketcraft_engine::doc::apply_vector(&c, dir)),
        None => (p, dir),
    };
    if inp.accept == crate::selection::AXIS_PICKS {
        let bb = view_bounds(app);
        for sel in hovered.iter().chain(inp.items.iter()) {
            let body = match sel {
                Sel::Edge { body, .. } | Sel::Face { body, .. } => body.as_str(),
                _ => "",
            };
            if let Some((o, dir)) = crate::dialogs::round_axis(s, sel).map(|(o, dir)| carry(body, o, dir))
                && !lines.iter().any(|(_, _, q, e)| q.dist(o) < 1e-9 && e.cross(dir).len() < 1e-9)
            {
                let (a, b) = axis_span(app, &bb, o, dir);
                lines.push((a, b, o, dir));
            }
        }
    } else if let (Some(Hit::Edge { body, index, .. }), Some(sel)) = (app.viewport.hover.as_ref(), hovered.as_ref()) {
        // Where the round edge is taken as its centre (`Dialog::circle_centre_pick`, Move's points).
        if (matches!(sel, Sel::Vertex { .. }) || d.edge_is_centre(inp))
            && let Some((c, ..)) = crate::dialogs::edge_circle(s, body, *index)
        {
            centres.push(carry(body, c, Vec3::Z).0);
        }
    }
    (lines.into_iter().map(|(a, b, ..)| (a, b)).collect(), centres)
}

/// Construction points shown: (name, position).
pub fn construction_points(app: &SolveApp) -> Vec<(String, Vec3)> {
    sketcraft_engine::view::construction_points(&app.session)
        .into_iter()
        .filter(|(_, n, _)| app.session.construction_visible(n))
        .map(|(_, n, p)| (n, p))
        .collect()
}

/// Construction planes drawn as squares around their origin: (name, normal, quad).
pub fn construction_quads(app: &SolveApp) -> Vec<(String, Vec3, [Vec3; 4])> {
    let h = origin_size(app.cam.half_height()) * 1.2;
    sketcraft_engine::view::construction_planes(&app.session)
        .into_iter()
        .filter(|(_, n, _)| app.session.construction_visible(n))
        .map(|(_, name, pl)| {
            let (o, x, y) = (pl.origin, pl.x * h, pl.y * h);
            (name, pl.normal(), [o - x - y, o + x - y, o + x + y, o - x + y])
        })
        .collect()
}

/// Pick what is under `pos`: sketch points, sketch curves, vertices, edges and axes first, then
/// the surfaces along the ray (profiles, faces, planes) nearest first.
/// Bounding boxes of the shown model's bodies (cached until the model changes).
pub(crate) fn body_bounds(
    app: &SolveApp,
    st: &std::sync::Arc<sketcraft_engine::doc::ModelState>,
) -> std::sync::Arc<Vec<(String, sketcraft_engine::geom::Aabb3)>> {
    if let Ok(mut g) = app.viewport.pick_bounds.lock() {
        if let Some((k, b)) = g.as_ref()
            && std::sync::Arc::ptr_eq(k, st)
        {
            return b.clone();
        }
        // Boxes of meshes the last state had too are kept (each body's box walks its mesh). A
        // placed part's box is its shared mesh's points moved, so no moved copy is made.
        let mut known = app.viewport.mesh_bounds.lock().map(|mut k| std::mem::take(&mut *k)).unwrap_or_default();
        let mut now: MeshBounds = std::collections::HashMap::with_capacity(st.bodies.len());
        let b = std::sync::Arc::new(
            st.bodies
                .iter()
                .map(|b| {
                    let (mesh, m) = b.mesh_in_place();
                    let key = (std::sync::Arc::as_ptr(&mesh) as usize, std::array::from_fn(|i| m[i / 4][i % 4].to_bits()));
                    let bounds = match known.remove(&key).or_else(|| now.get(&key).cloned()) {
                        Some((_, bounds)) => bounds,
                        None => b.mesh_bounds(),
                    };
                    now.insert(key, (mesh, bounds));
                    (b.name.clone(), bounds)
                })
                .collect::<Vec<_>>(),
        );
        // Pick indexes are kept for the meshes this state is picked from (a placed part's
        // shared mesh).
        let live: std::collections::HashSet<usize> = now.keys().map(|(mesh, _)| *mesh).collect();
        app.viewport.pick_index.retain(|k| live.contains(&k));
        if let Ok(mut k) = app.viewport.mesh_bounds.lock() {
            *k = now;
        }
        *g = Some((st.clone(), b.clone()));
        return b;
    }
    std::sync::Arc::new(Vec::new())
}

/// `boxes` are the boxes of `st`'s bodies (worked out without its meshes).
fn known_body_bounds(app: &SolveApp, st: &std::sync::Arc<sketcraft_engine::doc::ModelState>, boxes: Vec<(String, sketcraft_engine::geom::Aabb3)>) {
    if let Ok(mut g) = app.viewport.pick_bounds.lock() {
        *g = Some((st.clone(), std::sync::Arc::new(boxes)));
    }
}

type BodyBounds = (std::sync::Arc<sketcraft_engine::doc::ModelState>, std::sync::Arc<Vec<(String, sketcraft_engine::geom::Aabb3)>>);

/// The model's extent: bodies (from the cached boxes) and sketches.
pub fn model_bounds(app: &SolveApp) -> sketcraft_engine::geom::Aabb3 {
    let st = crate::open_meshing::shown_world(app);
    let bodies = body_bounds(app, &st).iter().fold(sketcraft_engine::geom::Aabb3::EMPTY, |a, (_, x)| a.union(x));
    let mut b = bodies.union(&sketch_bounds(app, &st));
    for c in sketcraft_engine::view::shown_point_clouds(&app.session) {
        for p in c.world_corners().into_iter().flatten() {
            b.add(p);
        }
    }
    for point in crate::sketch_attachment_ui::guide_lines(app).into_iter().flatten() {
        b.add(point);
    }
    b
}

/// The box of every sketch of `st` in the world, kept for that state: a design with hundreds of
/// sketches spent most of each frame sampling their splines again for `projection`.
fn sketch_bounds(app: &SolveApp, st: &std::sync::Arc<sketcraft_engine::doc::ModelState>) -> sketcraft_engine::geom::Aabb3 {
    if let Ok(g) = app.viewport.sketch_bounds.lock()
        && let Some((k, b)) = g.as_ref()
        && std::sync::Arc::ptr_eq(k, st)
    {
        return *b;
    }
    let mut b = sketcraft_engine::geom::Aabb3::EMPTY;
    for ss in &st.sketches {
        if let Some((lo, hi)) = ss.sketch.bounds() {
            for p in [lo, hi] {
                b.add(ss.plane.to_world(p));
            }
        }
    }
    if let Ok(mut g) = app.viewport.sketch_bounds.lock() {
        *g = Some((std::sync::Arc::clone(st), b));
    }
    b
}

/// Where a ray enters a box (None: it misses).
pub(crate) fn ray_box(o: Vec3, d: Vec3, b: &sketcraft_engine::geom::Aabb3, pad: f64) -> Option<f64> {
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for (oo, dd, lo, hi) in [(o.x, d.x, b.min.x, b.max.x), (o.y, d.y, b.min.y, b.max.y), (o.z, d.z, b.min.z, b.max.z)] {
        let (lo, hi) = (lo - pad, hi + pad);
        if dd.abs() < 1e-15 {
            if oo < lo || oo > hi {
                return None;
            }
        } else {
            let (a, c) = ((lo - oo) / dd, (hi - oo) / dd);
            t0 = t0.max(a.min(c));
            t1 = t1.min(a.max(c));
        }
    }
    (t0 <= t1).then_some(t0)
}

/// The point of segment `a`–`b` nearest the ray from `o` along `d`.
pub(crate) fn ray_segment_closest(o: Vec3, d: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let u = b - a;
    let w = a - o;
    let (dd, du, uu) = (d.dot(d), d.dot(u), u.dot(u));
    let den = dd * uu - du * du;
    // Parallel or degenerate: every point is as near, so the start stands for the segment.
    if !(den.is_finite() && den > 1e-12 * dd * uu) {
        return a;
    }
    let s = (du * d.dot(w) - dd * u.dot(w)) / den;
    if !s.is_finite() {
        return a;
    }
    a + u * s.clamp(0.0, 1.0)
}

/// A box's outline on screen, grown by `px` (None: behind the eye).
pub(crate) fn screen_box(proj: &Proj, b: &sketcraft_engine::geom::Aabb3, px: f32) -> Option<Rect> {
    let mut r = Rect::NOTHING;
    for i in 0..8 {
        let p = Vec3::new(
            if i & 1 == 0 { b.min.x } else { b.max.x },
            if i & 2 == 0 { b.min.y } else { b.max.y },
            if i & 4 == 0 { b.min.z } else { b.max.z },
        );
        r.extend_with(proj.to_screen(p)?);
    }
    Some(r.expand(px))
}

/// Mesh outlines have no B-rep identity, but their points and lengths can be measured.
/// Keep them out of modeling operands and invalidate hover picks when Measure opens/closes.
fn measuring_mesh_edges(app: &SolveApp) -> bool {
    app.dialog.as_ref().is_some_and(|d| matches!(d.kind, crate::dialogs::Kind::Measure { .. }))
}

fn edge_pickable(app: &SolveApp, mesh: &sketcraft_engine::geom::Mesh, index: usize) -> bool {
    mesh.edge_pickable(index) || (measuring_mesh_edges(app) && mesh.display_only_edges && mesh.edge_visible(index))
}

/// What is under `pos`, reusing the last answer while the cursor, the camera and the model are
/// unchanged.
pub(crate) fn pick_cached(app: &mut SolveApp, proj: &Proj, pos: Pos2) -> Vec<Hit> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    crate::render_workspace::active(app).hash(&mut h);
    pos.x.to_bits().hash(&mut h);
    pos.y.to_bits().hash(&mut h);
    for v in [app.cam.yaw, app.cam.pitch, app.cam.roll, app.cam.distance, app.cam.target.x, app.cam.target.y, app.cam.target.z] {
        v.to_bits().hash(&mut h);
    }
    app.session.revision.hash(&mut h);
    (std::sync::Arc::as_ptr(&crate::open_meshing::shown_world(app)) as usize).hash(&mut h);
    app.session.active_sketch.hash(&mut h);
    app.tool.as_ref().map(|tool| tool.cmd).hash(&mut h);
    app.dialog.is_none().hash(&mut h);
    measuring_mesh_edges(app).hash(&mut h);
    app.preferences.prefer_free_sketch_points.hash(&mut h);
    app.preferences.select_hidden_edges.hash(&mut h);
    app.ui.visual_style.hash(&mut h);
    app.viewport.woken_centre.hash(&mut h);
    crate::sketch3d::drawing(app).hash(&mut h);
    app.dialog.as_ref().and_then(|d| d.active_input()).map(|i| i.accept).hash(&mut h);
    crate::hole_place::snapping(app).hash(&mut h);
    crate::drag_snap::suppressed().hash(&mut h);
    app.session.visibility.hidden_bodies.hash(&mut h);
    app.dialog.as_ref().map(|d| &d.edit_source_bodies).hash(&mut h);
    app.session.visibility.hidden_occurrences.hash(&mut h);
    origin_point_shown(app).hash(&mut h);
    app.view_fov().to_bits().hash(&mut h);
    (proj.rect.width().to_bits(), proj.rect.height().to_bits()).hash(&mut h);
    crate::sketch_display::projection_key(proj, app.viewport.pixels_per_point()).hash(&mut h);
    let key = h.finish();
    if let Some((k, hits)) = &app.viewport.last_pick
        && *k == key
    {
        return hits.clone();
    }
    // Hover feedback mirrors what a 3D bridge click will snap to; clicks still go through `pick`.
    let hits = pick_with_sketch_points(app, proj, pos, crate::sketch3d::drawing(app));
    app.viewport.last_pick = Some((key, hits.clone()));
    hits
}

/// What is under `pos`. Sketch points come from the active sketch only (from every shown sketch
/// while a dialog input takes points, while idle, or while projecting), so sketch tools never get
/// another sketch's local point and id. The 3D bridge snap asks for cross-sketch points explicitly with [`pick_3d`].
pub fn pick(app: &SolveApp, proj: &Proj, pos: Pos2) -> Vec<Hit> {
    pick_with_sketch_points(app, proj, pos, false)
}

/// Operand picking excludes idle MID selection, including while a tool is temporarily taken
/// out of `app.tool` to handle its click.
pub(crate) fn pick_for_tool(app: &SolveApp, proj: &Proj, pos: Pos2) -> Vec<Hit> {
    pick_hits(app, proj, pos, false, false, false, false)
}

/// Like [`pick`], but also returns points of every visible sketch: only for the 3D drawing snap,
/// which converts them to world space through their own sketch's plane.
pub fn pick_3d(app: &SolveApp, proj: &Proj, pos: Pos2) -> Vec<Hit> {
    pick_with_sketch_points(app, proj, pos, true)
}

/// Everything under `pos` (Select Other): like [`pick`], but every sketch point, sketch curve,
/// vertex, edge, axis and profile near the cursor, and the nearest face of every body along the
/// ray, instead of the nearest of each kind. Each kind is nearest first.
pub fn pick_all(app: &SolveApp, proj: &Proj, pos: Pos2) -> Vec<Hit> {
    pick_hits(app, proj, pos, false, true, true, false)
}

/// See-through candidates in a small screen-space disk. The nine rays are fixed and bounded;
/// ordinary picking and repeat-click cycling follow the visual style and edge preference.
pub(crate) fn pick_through(app: &SolveApp, proj: &Proj, pos: Pos2) -> Vec<Hit> {
    if !pos.x.is_finite() || !pos.y.is_finite() || !proj.rect.contains(pos) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (x, y) in [(0.0, 0.0), (6.0, 0.0), (-6.0, 0.0), (0.0, 6.0), (0.0, -6.0), (4.0, 4.0), (-4.0, 4.0), (4.0, -4.0), (-4.0, -4.0)] {
        for hit in pick_hits(app, proj, pos + vec2(x, y), false, true, true, true) {
            if !out.iter().any(|previous| same_target(previous, &hit)) {
                out.push(hit);
            }
        }
    }
    out
}

fn pick_with_sketch_points(app: &SolveApp, proj: &Proj, pos: Pos2, other_sketch_points: bool) -> Vec<Hit> {
    // Gray canvas can navigate but cannot supply an invisible surface for selection,
    // cursor orbit or surface zoom beyond the rendered image's captured camera frame.
    if crate::render_workspace::active(app) && !app.preferences.render_legacy_image_defaults && !proj.rect.contains(pos) {
        return Vec::new();
    }
    let mut hits = pick_hits(app, proj, pos, other_sketch_points, false, true, false);
    // A Single hole's face place snaps to the face's centre and its round edges' centres.
    crate::hole_place::snap_hits(app, proj, pos, &mut hits);
    hits
}

/// The same thing picked twice (one row per target in [`pick_all`]).
pub fn same_target(a: &Hit, b: &Hit) -> bool {
    match (a, b) {
        (Hit::SketchMidpoint { sketch: s, id: i, .. }, Hit::SketchMidpoint { sketch: t, id: j, .. })
        | (Hit::SketchPoint { sketch: s, id: i, .. }, Hit::SketchPoint { sketch: t, id: j, .. })
        | (Hit::SketchCurve { sketch: s, id: i, .. }, Hit::SketchCurve { sketch: t, id: j, .. }) => s == t && i == j,
        (Hit::Vertex { body: a, point: p }, Hit::Vertex { body: b, point: q }) => a == b && p.dist(*q) < 1e-9,
        (Hit::Edge { body: a, index: i, .. }, Hit::Edge { body: b, index: j, .. })
        | (Hit::Face { body: a, index: i, .. }, Hit::Face { body: b, index: j, .. }) => a == b && i == j,
        (Hit::Profile { sketch: s, index: i }, Hit::Profile { sketch: t, index: j }) => s == t && i == j,
        (Hit::Plane { name: a, .. }, Hit::Plane { name: b, .. })
        | (Hit::Axis { name: a }, Hit::Axis { name: b })
        | (Hit::ConstructionPoint { name: a }, Hit::ConstructionPoint { name: b }) => a == b,
        _ => false,
    }
}

/// The candidates of one kind: the nearest only, or with `all` every target (nearest first).
struct Near<T> {
    all: bool,
    v: Vec<(T, Hit)>,
}

impl<T: PartialOrd + Copy> Near<T> {
    fn new(all: bool) -> Self {
        Near { all, v: Vec::new() }
    }
    /// Whether a candidate at `d` would be kept.
    fn wants(&self, d: T) -> bool {
        self.all || self.v.first().is_none_or(|(b, _)| d < *b)
    }
    fn offer(&mut self, d: T, h: Hit) {
        if !self.all {
            if self.wants(d) {
                self.v = vec![(d, h)];
            }
            return;
        }
        match self.v.iter_mut().find(|(_, x)| same_target(x, &h)) {
            Some(e) if d < e.0 => *e = (d, h),
            Some(_) => {}
            None => self.v.push((d, h)),
        }
    }
    fn sorted(mut self) -> Vec<(T, Hit)> {
        self.v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        self.v
    }
    fn hits(self) -> impl Iterator<Item = Hit> {
        self.sorted().into_iter().map(|x| x.1)
    }
}

fn pick_hits(app: &SolveApp, proj: &Proj, pos: Pos2, other_sketch_points: bool, all: bool, include_midpoints: bool, through: bool) -> Vec<Hit> {
    // Select Other and tool picks share the same visible-image boundary as ordinary picks.
    if crate::render_workspace::active(app) && !app.preferences.render_legacy_image_defaults && !proj.rect.contains(pos) {
        return Vec::new();
    }
    let s = &app.session;
    let st = crate::open_meshing::shown_world(app);
    let bounds = body_bounds(app, &st);
    // Bodies a Move dialog's earlier types moved are picked where they stand, as the preview
    // draws them: their mesh placed, and their box, moved on by that move.
    let chained = crate::dialogs_move::chained(app);
    let carry = |b: &sketcraft_engine::doc::ModelBody| chain_of(chained.as_ref(), &b.name);
    let placed = |b: &sketcraft_engine::doc::ModelBody| {
        let (m, place) = b.mesh_in_place();
        (m, carry(b).map_or(place, |c| sketcraft_engine::doc::mat_mul(&c, &place)))
    };
    let body_box = |i: usize, b: &sketcraft_engine::doc::ModelBody| {
        let (_, bb) = bounds.get(i)?;
        let Some(c) = carry(b) else { return Some(*bb) };
        let mut moved = sketcraft_engine::geom::Aabb3::EMPTY;
        for k in 0..8 {
            let corner = Vec3::new(
                if k & 1 == 0 { bb.min.x } else { bb.max.x },
                if k & 2 == 0 { bb.min.y } else { bb.max.y },
                if k & 4 == 0 { bb.min.z } else { bb.max.z },
            );
            moved.add(sketcraft_engine::doc::apply_point(&c, corner));
        }
        Some(moved)
    };
    let sketches = if through { st.sketches.iter().map(|s| s.feature).collect() } else { visible_sketches(app) };
    let mut hits = Vec::new();
    let (o, d) = proj.ray(pos);
    // MID is an idle sketch selection target. Tools and dialogs keep their existing picks.
    // Put it before a materialized helper point so later constraints still refer to the edge.
    if include_midpoints
        && app.tool.is_none()
        && app.dialog.is_none()
        && let Some(ss) = s.active_sketch.and_then(|sid| st.sketch(sid))
    {
        let mut mids = Near::new(all);
        for (i, curve) in ss.sketch.curves.iter().enumerate() {
            if !sketcraft_engine::view::sketch_curve_visible(&ss.sketch, i) {
                continue;
            }
            if let Some(at) = crate::selection::line_midpoint(&ss.sketch, i)
                && let Some(screen) = proj.to_screen(ss.plane.to_world(at))
            {
                let distance = screen.distance(pos);
                if distance < 8.0 {
                    mids.offer(distance, Hit::SketchMidpoint { sketch: ss.feature, id: curve.id.clone(), at });
                }
            }
        }
        hits.extend(mids.hits());
    }
    // Sketch points of the active sketch (of every shown sketch while a command takes points,
    // while idle or projecting, and while 3D drawing snaps to points on other visible sketches).
    let wants_points = app.dialog.as_ref().and_then(|d| d.active_input()).is_some_and(|i| i.accept & crate::selection::POINTS != 0);
    let external_points =
        wants_points || other_sketch_points || (app.dialog.is_none() && app.tool.as_ref().is_none_or(|tool| tool.cmd == "sketch.project"));
    let point_sketches: Vec<u64> = if crate::render_workspace::active(app) {
        Vec::new()
    } else if external_points {
        sketches.clone()
    } else {
        s.active_sketch.into_iter().collect()
    };
    // A face place (the Single hole's) snaps only to points their sketches show, so a nearer
    // point that isn't shown (a letter's outline) neither takes the pick nor hides a shown one.
    let snapping = app.dialog.as_ref().and_then(|d| d.active_input()).is_some_and(|i| crate::dialogs::snaps_to_points(i.accept));
    let prefer_free = app.preferences.prefer_free_sketch_points && include_midpoints && app.tool.is_none() && app.dialog.is_none();
    let mut best = Near::new(all);
    // A woken centre's extra reach (past the usual 8) yields to a curve under the pointer, so a
    // click on a small circle's edge still takes the circle.
    let mut woken_far: Option<(f32, Hit)> = None;
    for ss in point_sketches.iter().filter_map(|sid| st.sketch(*sid)) {
        let sid = ss.feature;
        let shown = snapping.then(|| crate::dialogs::SnapPoints::of(&ss.sketch));
        let visible = sketcraft_engine::view::SketchPointVisibility::new(&ss.sketch);
        for (p, locked) in ss.sketch.points.iter().zip(ss.sketch.locked_points()) {
            if !visible.takes(p) || shown.as_ref().is_some_and(|f| !f.takes(p)) {
                continue;
            }
            if let Some(sp) = proj.to_screen(ss.plane.to_world(p.pos)) {
                let dd = sp.distance(pos) - if s.active_sketch == Some(sid) { 2.0 } else { 0.0 };
                // On a distance tie, grab a movable point before the origin or a linked point.
                let rank = (dd, prefer_free && locked);
                let woken = app.viewport.woken_centre.as_ref().is_some_and(|(k, id)| *k == sid && *id == p.id);
                if dd < 8.0 && best.wants(rank) {
                    best.offer(rank, Hit::SketchPoint { sketch: sid, id: p.id.clone(), at: p.pos });
                } else if woken && dd < WOKEN_CENTRE_PX && woken_far.as_ref().is_none_or(|(d0, _)| dd < *d0) {
                    woken_far = Some((dd, Hit::SketchPoint { sketch: sid, id: p.id.clone(), at: p.pos }));
                }
            }
        }
    }
    for (name, point) in construction_points(app) {
        if let Some(sp) = proj.to_screen(point) {
            let dd = sp.distance(pos);
            let rank = (dd, prefer_free);
            if dd < 8.0 && best.wants(rank) {
                best.offer(rank, Hit::ConstructionPoint { name });
            }
        }
    }
    hits.extend(best.hits());
    // Sketch curves: the active sketch's own before projected and other sketches' ones, then
    // nearest (the active sketch wins ties).
    let mut bestc = Near::new(all);
    for sid in sketches.clone().into_iter().filter(|_| !crate::render_workspace::active(app)) {
        let Some(ss) = st.sketch(sid) else { continue };
        let active = s.active_sketch == Some(sid);
        let bonus = if active { 2.0 } else { 0.0 };
        let tier = |own: bool| u8::from(!(active && own));
        // Trim / Extend / Break edit free curves; projected edges under the same click should
        // yield so a drawn rectangle on a face is not refused as projected geometry (#8979).
        let trim_prefers_free = app.tool.as_ref().is_some_and(|t| matches!(t.cmd, "sketch.trim" | "sketch.extend" | "sketch.break"));
        for (i, c) in ss.sketch.curves.iter().enumerate() {
            if !sketcraft_engine::view::sketch_curve_visible(&ss.sketch, i) {
                continue;
            }
            let straight = matches!(c.kind, CurveKind::Line { .. });
            let own = c.link.is_none() || ss.sketch.is_text_curve(i);
            let tier = tier(own);
            // Construction geometry lying on a regular curve yields to it (#6396); Select Other
            // still offers both.
            let helper = if c.construction || c.centerline { 0.5 } else { 0.0 };
            let linked_penalty = if trim_prefers_free && !own { 3.5 } else { 0.0 };
            for part in &crate::sketch_display::curve_sample(app, sid, i, &ss.plane, proj).parts {
                let pts: Vec<(Pos2, Vec3)> = part.iter().filter_map(|q| proj.to_screen(*q).map(|sp| (sp, *q))).collect();
                for w in pts.windows(2) {
                    let dd = seg_dist(pos, w[0].0, w[1].0) - bonus + helper + linked_penalty;
                    if dd < 6.0 && bestc.wants((tier, dd)) {
                        let point = ray_segment_closest(o, d, w[0].1, w[1].1);
                        bestc.offer((tier, dd), Hit::SketchCurve { sketch: sid, id: c.id.clone(), straight, point });
                    }
                }
            }
        }
        // 3D sketch curves (paths for sweeps and pipes).
        for w in &ss.sketch.wires {
            let straight = w.fit.len() == 2;
            let tier = tier(w.link.is_none());
            let pts: Vec<(Pos2, Vec3)> = w.pts.iter().filter_map(|q| proj.to_screen(*q).map(|sp| (sp, *q))).collect();
            for seg in pts.windows(2) {
                let dd = seg_dist(pos, seg[0].0, seg[1].0) - bonus;
                if dd < 6.0 && bestc.wants((tier, dd)) {
                    let point = ray_segment_closest(o, d, seg[0].1, seg[1].1);
                    bestc.offer((tier, dd), Hit::SketchCurve { sketch: sid, id: w.id.clone(), straight, point });
                }
            }
        }
    }
    let before_curves = hits.len();
    hits.extend(bestc.hits());
    if let Some((_, far)) = woken_far
        && hits.len() == before_curves
        && !hits.iter().any(|h| matches!(h, Hit::SketchPoint { .. }))
    {
        hits.push(far);
    }
    // A section hides the material its normal points at: nothing there is picked, and where the
    // plane crosses a body its cap hides what lies behind it (#6328), as `view.pick` answers.
    let section = section_plane(app);
    let kept = |p: Vec3| section.is_none_or(|(so, sn)| sn.dot(p) <= sn.dot(so) + crate::view_pick::SECTION_EPS);
    let walk_eps = 1e-6 * (1.0 + scene_radius(app));
    // Nearest face along the ray (hides what is behind it); with `all`, every body's nearest.
    let mut bestf: Option<(f64, Hit)> = None;
    let mut faces = Near::new(all);
    // Bounds retain the state's body order. Looking each one up by name made both passes
    // quadratic for assemblies with many small parts.
    for (i, b) in st.bodies.iter().enumerate() {
        if !through && !app.body_visible(&b.name) {
            continue;
        }
        // Bodies whose box the ray misses (or enters behind the best hit so far) are skipped.
        if let Some(bb) = body_box(i, b) {
            let pad = bb.diagonal() * 1e-6 + 1e-9;
            match ray_box(o, d, &bb, pad) {
                None => continue,
                Some(t0) if !all && bestf.as_ref().is_some_and(|(bt, _)| t0 > *bt) => continue,
                _ => {}
            }
        }
        // A placed part is picked in its own frame, from its shared mesh and index: the ray is
        // moved there (a rigid move keeps distances along it), so no moved copy is made.
        let (m, place) = placed(b);
        let index = app.viewport.pick_index.of(&m);
        // The nearest kept face: past any the section removes (a bounded walk along the ray).
        let mut nearest = None;
        let mut from = 0.0f64;
        for _ in 0..64 {
            let Some((lo, ld)) = local_ray(&place, o + d * from, d) else { break };
            let Some((dt, ti)) = crate::pick_index::raycast(index.as_deref(), &m, lo, ld) else { break };
            let t = from + dt;
            if kept(o + d * t) {
                nearest = Some((t, ti));
                break;
            }
            from = t + walk_eps;
        }
        if let Some((t, ti)) = nearest {
            let fi = m.tri_face.get(ti).copied().unwrap_or(0) as usize;
            let hit = Hit::Face { body: b.name.clone(), index: fi, point: o + d * t };
            if bestf.as_ref().is_none_or(|(bt, _)| t < *bt) {
                bestf = Some((t, hit.clone()));
            }
            if all {
                faces.offer(t, hit);
            }
        }
    }
    if crate::render_workspace::active(app) {
        return if all { faces.sorted().into_iter().map(|(_, hit)| hit).collect() } else { bestf.into_iter().map(|(_, hit)| hit).collect() };
    }
    // Find the cap independently: a face within the clipping tolerance may lie just ahead
    // of it. That face stays selectable, but cannot expose edges behind the cap.
    let cap = section.and_then(|(so, sn)| {
        let den = sn.dot(d);
        let t_cap = (sn.dot(so) - sn.dot(o)) / den;
        let crosses = den.abs() > 1e-12 && t_cap > 0.0;
        let replaced = app.preview.effective_replaced();
        let shown_bodies = &None;
        (crosses && crate::view_pick::body_cut_at(app, &st, &bounds, o, d, t_cap, true, replaced, shown_bodies).is_some()).then_some(t_cap)
    });
    if let Some(t_cap) = cap {
        if bestf.as_ref().is_some_and(|(t, _)| t_cap < *t) {
            bestf = None;
        }
        faces.v.retain(|(t, _)| *t <= t_cap);
    }
    let hidden_from = bestf.as_ref().map(|(t, _)| *t).or(cap);
    let slack = scene_pick_slack(app);
    // Wireframe draws no faces, so nothing hides an edge or a vertex. `slack` is the body's own:
    // what counts as lying on the face in front depends on how coarsely that body is meshed.
    let wireframe = app.ui.visual_style == 2;
    let hidden_edges = app.ui.visual_style == 3 && app.preferences.select_hidden_edges;
    let visible = |p: Vec3, slack: f64| kept(p) && (through || wireframe || hidden_from.is_none_or(|t| (p - o).dot(d) <= t + slack));
    // Vertices (edge end points) and edges, within 7 and 6 points of the cursor.
    let reach = crate::pick_index::reach(proj, pos, 8.0);
    let mut bestv = Near::new(all);
    let mut beste = Near::new(all);
    for (i, b) in st.bodies.iter().enumerate() {
        if !through && !app.body_visible(&b.name) {
            continue;
        }
        let mut body_slack = slack;
        if let Some(bb) = body_box(i, b) {
            // Only bodies whose outline on screen is near the cursor can have an edge under it.
            if screen_box(proj, &bb, 8.0).is_some_and(|r| !r.contains(pos)) {
                continue;
            }
            body_slack = pick_slack(bb.diagonal());
        }
        // In the part's own frame, as for faces; its points are moved back to where it stands.
        let (m, place) = placed(b);
        let Some((lo, ld)) = local_ray(&place, o, d) else { continue };
        let moved = !sketcraft_engine::doc::is_identity(&place);
        let at = |p: Vec3| if moved { sketcraft_engine::doc::apply_point(&place, p) } else { p };
        // Only the segments that can come near the cursor, from the index, in the order a scan
        // of every edge would meet them, so ties go the same way.
        let near = crate::pick_index::segments_near(app.viewport.pick_index.of(&m).as_deref(), &m, lo, ld, reach);
        let mut last_edge = None;
        for (ei, si) in near {
            let Some(e) = m.edges.get(ei) else { continue };
            if !edge_pickable(app, &m, ei) {
                continue;
            }
            if last_edge != Some(ei) {
                last_edge = Some(ei);
                for v in [e.first(), e.last()].into_iter().flatten().map(|v| at(*v)) {
                    if let Some(sp) = proj.to_screen(v) {
                        let dd = sp.distance(pos);
                        if dd < 7.0 && visible(v, body_slack) && bestv.wants(dd) {
                            bestv.offer(dd, Hit::Vertex { body: b.name.clone(), point: v });
                        }
                    }
                }
            }
            let (Some(p0), Some(p1)) = (e.get(si).map(|p| at(*p)), e.get(si + 1).map(|p| at(*p))) else { continue };
            let (Some(a), Some(c)) = (proj.to_screen(p0), proj.to_screen(p1)) else { continue };
            let dd = seg_dist(pos, a, c);
            // Visible where the cursor is: the edge's point under the cursor, not an end or
            // the middle, which can lie deeper than the face the cursor is on.
            let point = ray_segment_closest(o, d, p0, p1);
            // Hidden-edge mode changes only edges of shown bodies. Section clipping,
            // vertex picks and modelling edge identities keep their ordinary rules.
            let seen = visible(point, body_slack);
            let key = (dd, !seen);
            // A section cap still hides kept material; only ordinary faces permit hidden edges.
            let hidden = hidden_edges && kept(point) && cap.is_none_or(|t| (point - o).dot(d) <= t + body_slack);
            if dd < 6.0 && beste.wants(key) && (seen || hidden) {
                beste.offer(key, Hit::Edge { body: b.name.clone(), index: ei, mid: at(polyline_mid(e)), point });
            }
        }
    }
    // The origin point, for an input that takes it: a vertex of no body, drawn over everything.
    if app.dialog.as_ref().and_then(|d| d.active_input()).is_some_and(|i| i.accept & crate::selection::ORIGIN != 0)
        && origin_point_shown(app)
        && let Some(sp) = proj.to_screen(Vec3::ZERO)
    {
        let dd = sp.distance(pos);
        if dd < 7.0 && bestv.wants(dd) {
            bestv.offer(dd, Hit::Vertex { body: String::new(), point: Vec3::ZERO });
        }
    }
    hits.extend(bestv.hits());
    hits.extend(beste.hits());
    // Origin axes.
    let size = origin_size(app.cam.half_height());
    let mut besta = Near::new(all);
    for (name, dir) in origin_axes(app) {
        if let (Some(a), Some(b)) = (proj.to_screen(Vec3::ZERO), proj.to_screen(dir * (size * AXIS_LEN))) {
            let dd = seg_dist(pos, a, b);
            if dd < 5.0 && besta.wants(dd) {
                besta.offer(dd, Hit::Axis { name: name.into() });
            }
        }
    }
    // Construction axes, picked like the origin axes.
    for (name, a, b) in construction_axis_segments(app) {
        if let (Some(a), Some(b)) = (proj.to_screen(a), proj.to_screen(b)) {
            let dd = seg_dist(pos, a, b);
            if dd < 5.0 && besta.wants(dd) {
                besta.offer(dd, Hit::Axis { name });
            }
        }
    }
    hits.extend(besta.hits());
    // Surfaces by depth: profiles (slightly preferred over a face they lie on), faces, planes.
    let mut surf: Vec<(f64, Hit)> = Vec::new();
    let mut bestp: Option<(f64, f64, Hit)> = None;
    let mut profiles_all: Vec<(f64, f64, Hit)> = Vec::new();
    for sid in sketches.clone() {
        let Some(ss) = st.sketch(sid) else { continue };
        let n = ss.plane.normal();
        let den = n.dot(d);
        if den.abs() < 1e-12 {
            continue;
        }
        let t = n.dot(ss.plane.origin - o) / den;
        if t < 0.0 {
            continue;
        }
        let lp = ss.plane.to_local(o + d * t);
        // Show Profile off: the sketch being edited offers no profiles.
        let profiles = if ss.sketch.view.hide_profile && s.active_sketch == Some(sid) { &[][..] } else { &ss.profiles[..] };
        for (pi, p) in profiles.iter().enumerate() {
            if !p.contains(lp) {
                continue;
            }
            if all {
                profiles_all.push((t, p.area, Hit::Profile { sketch: sid, index: pi }));
            } else if bestp.as_ref().is_none_or(|(_, a, _)| p.area < *a) {
                bestp = Some((t, p.area, Hit::Profile { sketch: sid, index: pi }));
            }
        }
    }
    // Smallest profile first.
    profiles_all.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (t, _, h) in bestp.into_iter().chain(profiles_all) {
        surf.push((t - slack, h));
    }
    let front = bestf.as_ref().map(|(t, _)| *t);
    if all {
        surf.extend(faces.sorted());
    } else if let Some(f) = bestf {
        surf.push(f);
    }
    let mut planes: Vec<(f64, Hit)> = Vec::new();
    for (name, _, q) in origin_planes(app) {
        if let Some(t) = ray_quad(o, d, &q) {
            planes.push((t, Hit::Plane { name: name.into(), point: o + d * t }));
        }
    }
    for (name, _, q) in construction_quads(app) {
        if let Some(t) = ray_quad(o, d, &q) {
            planes.push((t, Hit::Plane { name, point: o + d * t }));
        }
    }
    planes.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Planes give way to a face or profile at the same depth (a floor on the XZ plane). While a
    // dialog waits for a plane they are drawn over the bodies, so when the nearest face hides the
    // nearest plane, the planes are picked through it: just ahead of that face, in their own
    // depth order, still behind a profile on it (#5431).
    let through = match (front, planes.first()) {
        (Some(f), Some((t, _))) if app.picking_plane() && *t > f + slack => Some(f - slack * 0.5),
        _ => None,
    };
    // The sort is stable, so planes moved ahead of the face keep their order.
    surf.extend(planes.into_iter().map(|(t, h)| (through.unwrap_or(t + slack), h)));
    surf.sort_by(|a, b| a.0.total_cmp(&b.0));
    hits.extend(surf.into_iter().map(|x| x.1));
    hits
}

/// The section cut shown now: the open Section Analysis dialog's plane, else the session's.
/// The active sketch's display options (defaults when not sketching).
pub fn active_view(app: &SolveApp) -> sketcraft_engine::sketch::SketchView {
    let st = crate::open_meshing::shown_world(app);
    app.session.active_sketch.and_then(|s| st.sketch(s)).map(|ss| ss.sketch.view.clone()).unwrap_or_default()
}

pub fn section_plane(app: &SolveApp) -> Option<(Vec3, Vec3)> {
    if let Some(d) = app.dialog.as_ref().filter(|d| matches!(d.kind, crate::dialogs::Kind::Section { .. })) {
        let cmds = crate::dialogs::apply_commands(app, d).ok()?;
        let (_, p) = cmds.into_iter().next()?;
        let mut scratch = app.session.scratch();
        scratch.execute("inspect.section", &p).ok()?;
        return scratch.section;
    }
    slice_plane(app).or(app.session.section)
}

/// Slice (Sketch Palette): the plane of the sketch being edited, when Slice is on in it. It
/// stands in for a Section Analysis until it is turned off or the sketch closes: with the
/// section first, turning Slice on did nothing (#10705).
fn slice_plane(app: &SolveApp) -> Option<(Vec3, Vec3)> {
    let id = app.session.active_sketch?;
    let st = crate::open_meshing::shown_world(app);
    let ss = st.sketch(id).filter(|ss| ss.sketch.view.slice)?;
    Some((ss.plane.origin, ss.plane.normal()))
}

/// Whether the section [`section_plane`] shows has hatched cut faces: an open Section Analysis
/// dialog's Hatch option, a Slice's always, else the section's own.
pub fn section_hatch(app: &SolveApp) -> bool {
    match app.dialog.as_ref().map(|d| &d.kind) {
        Some(crate::dialogs::Kind::Section { hatch, .. }) => *hatch,
        _ => app.session.section_hatch || slice_plane(app).is_some(),
    }
}

/// Visible geometry belonging to a timeline feature. Body creators retain their whole-body
/// feedback; modifiers use persistent face names, which survive later edits and placement.
/// Do not expand these faces into surface patches: neighbouring faces can belong to another
/// feature even when they share a surface.
fn feature_geometry(app: &SolveApp, id: u64) -> Vec<Sel> {
    feature_parts(app, id, true)
}

/// A feature's geometry as [`feature_geometry`] finds it; `shown_only` leaves out hidden bodies
/// and bodies a dialog's preview stands in for.
fn feature_parts(app: &SolveApp, id: u64, shown_only: bool) -> Vec<Sel> {
    if !app.session.model.result(id).is_some_and(|r| !r.skipped && r.error.is_none()) {
        return Vec::new();
    }
    let st = crate::open_meshing::shown_world(app);
    let mut out = Vec::new();
    for b in &st.bodies {
        if shown_only && (!app.body_visible(&b.name) || app.preview.effective_replaced().contains(&b.name)) {
            continue;
        }
        if b.feature == id {
            out.push(Sel::Body { name: b.name.clone() });
            continue;
        }
        let names = sketcraft_engine::doc::naming::face_names(b);
        let m = b.mesh();
        for (index, _) in names.iter().enumerate().filter(|(_, n)| sketcraft_engine::doc::persistent_name::made_by(n) == Some(id)) {
            if let Some(point) = first_triangle_point(&m, index) {
                out.push(Sel::Face { body: b.name.clone(), index, point });
            }
        }
    }
    if out.is_empty() {
        // A later edit renamed the faces this feature made (an Offset Face, Replace Face or
        // Delete Face leaves them named after whatever last touched them): light the faces it
        // changed, found by comparing the states either side of it.
        out = changed_faces(app, id, &st, shown_only);
    }
    out
}

/// A point on face `index` of `m`: the middle of its first triangle.
fn first_triangle_point(m: &sketcraft_engine::geom::Mesh, index: usize) -> Option<solidcore_record::Vec3> {
    m.triangles.iter().zip(&m.tri_face).find_map(|(t, f)| (*f as usize == index).then(|| m.tri(t)).flatten().map(|[a, b, c]| (a + b + c) / 3.0))
}

/// The faces a feature changed, for the states either side of it, on the bodies as they are now:
/// a face whose name is new, or whose area, centroid or plane moved. A face a later feature
/// renamed is found again by its plane and nearest centre.
fn changed_faces(app: &SolveApp, id: u64, now: &sketcraft_engine::doc::ModelState, shown_only: bool) -> Vec<Sel> {
    let Some(index) = app.session.doc.features.iter().position(|f| f.id == id) else { return Vec::new() };
    let after = match app.session.doc.features.get(index + 1) {
        Some(next) => app.session.model.state_before(next.id),
        None => app.session.model.state(),
    };
    let before = app.session.model.state_before(id);
    let mut out = Vec::new();
    for b in &after.bodies {
        if shown_only && (!app.session.body_visible(&b.name) || app.preview.effective_replaced().contains(&b.name)) {
            continue;
        }
        let (Some(now_body), Some(old)) = (now.body(&b.name), before.body(&b.name)) else { continue };
        let tol = (b.body.size() * 1e-3).max(1e-4);
        let (Ok(new_faces), Ok(old_faces)) = (b.body.faces(tol), old.body.faces(tol)) else { continue };
        let names = sketcraft_engine::doc::naming::face_names(b);
        let old_names = sketcraft_engine::doc::naming::face_names(old);
        for (i, name) in names.iter().enumerate() {
            let Some(new) = new_faces.get(i) else { continue };
            // A face the feature moved takes a new name (Offset Face); a face it only trimmed
            // or stretched keeps its own. Either is a face the feature changed.
            let moved = match old_names.iter().position(|old| old == name) {
                None => true,
                Some(j) => {
                    let Some(was) = old_faces.get(j) else { continue };
                    (new.area - was.area).abs() > new.area.max(1e-9) * 1e-4
                        || new.centroid.dist(was.centroid) > tol
                        || matches!((new.plane_normal, was.plane_normal), (Some(a), Some(b)) if a.dot(b) < 1.0 - 1e-7)
                }
            };
            if !moved {
                continue;
            }
            let Some((index, point)) = current_face(now_body, name, new, tol) else { continue };
            if !out.iter().any(|s| matches!(s, Sel::Face { body, index: i, .. } if body == &b.name && *i == index)) {
                out.push(Sel::Face { body: b.name.clone(), index, point });
            }
        }
    }
    out
}

/// Face `name` of `now` (or the nearest face on its plane when a later feature renamed it),
/// with a point on it.
fn current_face(
    now: &sketcraft_engine::doc::ModelBody,
    name: &str,
    sample: &sketcraft_engine::kernel::FaceInfo,
    tol: f64,
) -> Option<(usize, solidcore_record::Vec3)> {
    let m = now.mesh();
    let names = sketcraft_engine::doc::naming::face_names(now);
    if let Some(index) = names.iter().position(|n| n == name) {
        return first_triangle_point(&m, index).map(|point| (index, point));
    }
    let normal = sample.plane_normal?;
    let faces = now.body.faces(tol).ok()?;
    // The same face, moved on by later edits: the nearest centre on a parallel plane, a face
    // still on the plane it was on winning over one that only faces the same way.
    let mut best: Option<(f64, f64, usize)> = None;
    for f in faces.iter().filter(|f| f.plane_normal.is_some_and(|n| n.dot(normal) > 1.0 - 1e-7)) {
        let (off, d) = ((f.centroid - sample.centroid).dot(normal).abs(), f.centroid.dist(sample.centroid));
        if best.is_none_or(|b| off.total_cmp(&b.0).then(d.total_cmp(&b.1)).is_lt()) {
            best = Some((off, d, f.index));
        }
    }
    let index = best?.2;
    first_triangle_point(&m, index).map(|point| (index, point))
}

/// The sketches picked among `items` (a sketch row in the Browser or the timeline) that the
/// highlight draws, shown or hidden (#7156). The sketch being edited shows its own selection.
fn selected_sketches(app: &SolveApp, items: &[Sel]) -> Vec<u64> {
    let st = crate::open_meshing::shown_world(app);
    let mut out = Vec::new();
    for item in items {
        if let Sel::Feature { id } = item
            && app.session.active_sketch != Some(*id)
            && st.sketch(*id).is_some()
            && !out.contains(id)
        {
            out.push(*id);
        }
    }
    out
}

fn displayed_selection(app: &SolveApp, item: &Sel) -> Vec<Sel> {
    match item {
        Sel::Feature { id } => feature_geometry(app, *id),
        Sel::Occurrence { path } => app
            .session
            .occurrence_bodies(path, true)
            .unwrap_or_default()
            .into_iter()
            .filter(|name| app.body_visible(name))
            .map(|name| Sel::Body { name })
            .collect(),
        _ => grouped_selection(&app.session, item),
    }
}

/// The box around what is selected (bodies, faces, edges, vertices, sketch curves and the
/// bodies of selected features), where it is shown.
pub fn selection_bounds(app: &SolveApp) -> Option<sketcraft_engine::geom::Aabb3> {
    bounds_of(app, app.session.selection.iter().flat_map(|item| displayed_selection(app, item)))
}

/// The box around the geometry a feature made (its bodies, or its faces on bodies later features
/// changed), also where a dialog's preview stands in for it: a pattern of the feature previews
/// the body the feature is on. None when the feature made nothing.
pub(crate) fn feature_bounds(app: &SolveApp, id: u64) -> Option<sketcraft_engine::geom::Aabb3> {
    bounds_of(app, feature_parts(app, id, false).into_iter())
}

fn bounds_of(app: &SolveApp, items: impl Iterator<Item = Sel>) -> Option<sketcraft_engine::geom::Aabb3> {
    let st = crate::open_meshing::shown_world(app);
    let mut b = sketcraft_engine::geom::Aabb3::EMPTY;
    for s in items {
        match &s {
            Sel::Body { name } => {
                if let Some(x) = st.body(name) {
                    b = b.union(&x.mesh().bounds());
                }
            }
            Sel::Face { body, index, .. } => {
                if let Some(x) = st.body(body) {
                    let m = x.mesh();
                    for (t, f) in m.triangles.iter().zip(&m.tri_face) {
                        if *f as usize == *index {
                            for p in m.tri(t).into_iter().flatten() {
                                b.add(p);
                            }
                        }
                    }
                }
            }
            Sel::Edge { body, index, .. } => {
                if let Some(e) = st.body(body).and_then(|x| x.mesh().edges.get(*index).cloned()) {
                    for p in e {
                        b.add(p);
                    }
                }
            }
            Sel::Vertex { point, .. } => b.add(*point),
            Sel::SketchMidpoint { id } => {
                if let Some(ss) = app.session.active_sketch.and_then(|sid| st.sketch(sid))
                    && let Some(line) = ss.sketch.curve_index(id)
                    && let Some(at) = crate::selection::line_midpoint(&ss.sketch, line)
                {
                    b.add(ss.plane.to_world(at));
                }
            }
            Sel::ExternalSketchPoint { sketch, id } => {
                if let Some(ss) = st.sketch(*sketch)
                    && let Some(p) = ss.sketch.resolve_point(id).and_then(|i| ss.sketch.point(i))
                {
                    b.add(ss.plane.to_world(p));
                }
            }
            Sel::ExternalSketchCurve { sketch, id } => {
                if let Some(ss) = st.sketch(*sketch)
                    && let Some(i) = ss.sketch.curve_index(id)
                {
                    for p in ss.sketch.polyline(i) {
                        b.add(ss.plane.to_world(p));
                    }
                }
            }
            Sel::SketchCurve { id, sketch } => {
                for ss in st.sketches.iter().filter(|ss| sketch.is_none_or(|k| ss.feature == k)) {
                    if let Some(i) = ss.sketch.curve_index(id) {
                        for q in ss.sketch.polyline(i) {
                            b.add(ss.plane.to_world(q));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (!b.is_empty()).then_some(b)
}

/// The section as the shader's clip plane (normal, d), zero when there is none.
/// The surface analysis for the shader (see `gpu::Frame::analysis`).
fn analysis_uniform(app: &SolveApp) -> [f32; 8] {
    sketcraft_gpu::view::analysis_uniform(app.session.analysis.map(|a| a.mode()), crate::access::map(app).as_ref())
}

/// Why the model cannot be placed, across the bottom of the viewport, while it cannot: the
/// viewport then shows no placed bodies (see `open_meshing::session_world`).
fn placement_note(app: &SolveApp, painter: &egui::Painter, rect: Rect) {
    let Some(why) = crate::open_meshing::placement_error(&app.session) else { return };
    let t = Tokens::get();
    let text = format!("The model cannot be placed: {why}");
    painter.text(pos2(rect.center().x, rect.bottom() - 24.0), Align2::CENTER_BOTTOM, text, FontId::proportional(t.font_button), t.error);
}

/// While Draft, Accessibility or Curvature Map analysis is on, what each of its colours means and
/// the values it stands for, at the bottom left of the viewport (#973). The colours alone don't
/// say, and with colour vision loss they can be hard to tell apart.
pub(crate) type AnalysisLegend = (&'static str, Option<String>, Vec<(Color32, String, bool)>);

pub(crate) fn analysis_legend_data(app: &SolveApp) -> Option<AnalysisLegend> {
    use sketcraft_engine::SurfaceAnalysis as A;
    let t = Tokens::get();
    let tr = crate::i18n::tr_label;
    let col = |c: [f32; 3]| {
        let u = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgb(u(c[0]), u(c[1]), u(c[2]))
    };
    let dir = |v: Vec3| format!("({:.2}, {:.2}, {:.2})", v.x, v.y, v.z);
    let a = &t.analysis;
    // (title, the direction it measures along, rows of colour, text, hatched).
    let (title, along, rows) = match app.session.analysis {
        Some(A::Draft { pull, angle }) => (
            tr("Draft Analysis"),
            Some(format!("{} {}", tr("Pull direction"), dir(pull))),
            vec![
                (col(a.draft[0]), format!("≥ {angle:.1}°"), false),
                (col(a.draft[1]), format!("−{angle:.1}° … {angle:.1}°"), false),
                (col(a.draft[2]), format!("≤ −{angle:.1}°"), a.hatch),
            ],
        ),
        Some(A::Access { dir: d }) => (
            tr("Accessibility Analysis"),
            Some(format!("{} {}", tr("Tool direction"), dir(d))),
            vec![(col(a.access[0]), tr("Reachable").to_string(), false), (col(a.access[1]), tr("Blocked").to_string(), a.hatch)],
        ),
        // The shader's scale reaches its middle at twice the analysis radius.
        Some(A::Curvature { radius }) => (
            tr("Curvature Map Analysis"),
            None,
            vec![
                (col(a.curvature[0]), tr("ui.flat").to_string(), false),
                (col(a.curvature[1]), format!("{} {}", tr("Radius"), crate::prefs::show_mm(app, 2.0 * radius, 1)), false),
                (col(a.curvature[2]), format!("{} ≤ {}", tr("Radius"), crate::prefs::show_mm(app, radius, 1)), false),
            ],
        ),
        _ => return None,
    };
    Some((title, along, rows))
}

fn section_clip(app: &SolveApp) -> [f32; 4] {
    match section_plane(app) {
        Some((o, n)) => [n.x as f32, n.y as f32, n.z as f32, n.dot(o) as f32],
        None => [0.0; 4],
    }
}

/// Space kept free at the top right of the viewport for the axis view (docked dialogs start below).
pub(crate) fn axis_view_size(style: crate::prefs::NavigatorStyle) -> f32 {
    match style {
        crate::prefs::NavigatorStyle::Axes => 116.0,
        crate::prefs::NavigatorStyle::Cube | crate::prefs::NavigatorStyle::Custom => navigation_cube::SIZE,
    }
}
// Keep command panels anchored independently of the compact cube's display size.
pub const AXIS_VIEW_CLEARANCE: f32 = 168.0;

/// Axis length as a multiple of the origin plane size (solid part 1, dashed beyond).
const AXIS_LEN: f64 = 1.8;

/// The hit as a plain selection (no command filter).
pub fn hit_sel(h: &Hit) -> Option<Sel> {
    Some(match h {
        Hit::SketchMidpoint { id, .. } => Sel::SketchMidpoint { id: id.clone() },
        Hit::SketchPoint { id, .. } => Sel::SketchPoint { id: id.clone() },
        Hit::SketchCurve { id, .. } => Sel::SketchCurve { id: id.clone(), sketch: None },
        Hit::Vertex { body, point } => Sel::Vertex { body: body.clone(), point: *point },
        Hit::Edge { body, index, mid, .. } => Sel::Edge { body: body.clone(), index: *index, point: *mid },
        Hit::Profile { sketch, index } => Sel::Profile { sketch: *sketch, index: *index },
        Hit::Face { body, index, point } => Sel::Face { body: body.clone(), index: *index, point: *point },
        Hit::Plane { name, .. } => Sel::Plane { name: name.clone() },
        Hit::Axis { name } => Sel::Axis { name: name.clone() },
        Hit::ConstructionPoint { name } => Sel::ConstructionPoint { name: name.clone() },
    })
}

/// The item as a hit: what lights up when it is hovered. `None` for bodies, features and
/// components, which light up whole instead, and for constraints.
pub(crate) fn sel_hit(app: &SolveApp, sel: &Sel) -> Option<Hit> {
    let st = app.session.world_state().ok()?;
    let active = app.session.active_sketch;
    let point = |sketch: u64, id: &str| {
        let sk = &st.sketch(sketch)?.sketch;
        sk.resolve_point(id).and_then(|i| sk.point(i))
    };
    let curve = |sketch: u64, id: &str| {
        let ss = st.sketch(sketch)?;
        let sk = &ss.sketch;
        let c = sk.curves.get(sk.curve_index(id)?)?;
        // Its start is where an orbit from the hover would turn about.
        let point = c.kind.ends().and_then(|(a, _)| sk.point(a)).map_or(Vec3::ZERO, |at| ss.plane.to_world(at));
        Some(Hit::SketchCurve { sketch, id: id.to_string(), straight: matches!(c.kind, CurveKind::Line { .. }), point })
    };
    Some(match sel {
        Sel::Edge { body, index, point } => Hit::Edge { body: body.clone(), index: *index, mid: *point, point: *point },
        Sel::Face { body, index, point } => Hit::Face { body: body.clone(), index: *index, point: *point },
        Sel::Vertex { body, point } => Hit::Vertex { body: body.clone(), point: *point },
        Sel::Profile { sketch, index } => Hit::Profile { sketch: *sketch, index: *index },
        Sel::Plane { name } => Hit::Plane { name: name.clone(), point: Vec3::ZERO },
        Sel::Axis { name } => Hit::Axis { name: name.clone() },
        Sel::ConstructionPoint { name } => Hit::ConstructionPoint { name: name.clone() },
        Sel::SketchCurve { id, sketch } => curve((*sketch).or(active)?, id)?,
        Sel::ExternalSketchCurve { sketch, id } => curve(*sketch, id)?,
        Sel::ExternalSketchPoint { sketch, id } => Hit::SketchPoint { sketch: *sketch, id: id.clone(), at: point(*sketch, id)? },
        // A point of another sketch is picked as `sketch:id`.
        Sel::SketchPoint { id } => {
            let (sketch, id) = match id.split_once(':').and_then(|(s, p)| Some((s.parse().ok()?, p))) {
                Some(other) => other,
                None => (active?, id.as_str()),
            };
            Hit::SketchPoint { sketch, id: id.to_string(), at: point(sketch, id)? }
        }
        Sel::SketchMidpoint { id } => {
            let sketch = active?;
            let sk = &st.sketch(sketch)?.sketch;
            let at = crate::selection::line_midpoint(sk, sk.curve_index(id)?)?;
            Hit::SketchMidpoint { sketch, id: id.clone(), at }
        }
        Sel::Body { .. } | Sel::Feature { .. } | Sel::Occurrence { .. } | Sel::SketchConstraint { .. } => return None,
    })
}

/// A dialog's picked-items row under the pointer (#3683) is the hover, as the item would be
/// under the pointer in the view. Bodies, components and features light up as their Browser
/// and timeline rows do instead (`build_highlight`).
fn list_hover(app: &mut SolveApp) {
    if let Some((_, item)) = app.dialog.as_ref().and_then(|d| d.list_hover.as_ref()) {
        app.viewport.hover = sel_hit(app, item);
    }
}

/// What lights up under the pointer while a tool runs: the nearest hit its click takes.
fn tool_hover(app: &SolveApp, hits: &[Hit]) -> Option<Hit> {
    let cmd = app.tool.as_ref()?.cmd;
    hits.iter().find(|h| crate::sketch_tools::takes(cmd, h)).cloned()
}

/// A sketch annotation under either a left-click or a context-menu click. Dimension text
/// wins over geometry; dimension lines yield to points and curves, and glyphs to midpoints.
pub(crate) fn sketch_constraint_at(app: &SolveApp, pos: Pos2, hit: Option<&Hit>) -> Option<Sel> {
    app.session.active_sketch?;
    let off = |kind: &str| app.ui.pick_off.iter().any(|x| x == kind);
    let on_geometry = matches!(hit, Some(Hit::SketchPoint { .. } | Hit::SketchMidpoint { .. } | Hit::SketchCurve { .. }));
    let dimension =
        crate::dim_view::hit_text(pos).or_else(|| if on_geometry { None } else { crate::dim_view::hit(pos) }).filter(|_| !off("dimensions"));
    let glyph = crate::sketch_tools::glyph_at(pos).filter(|_| !off("constraints") && !matches!(hit, Some(Hit::SketchMidpoint { .. })));
    dimension.or(glyph).map(|id| Sel::SketchConstraint { id })
}

/// What a click would pick: the first hit the open dialog's active input accepts, or with no
/// dialog the first hit (sketch entities only belong to the active sketch).
pub fn candidate(app: &SolveApp, hits: &[Hit]) -> Option<(Hit, Sel)> {
    if let Some(d) = app.dialog.as_ref().filter(|d| d.wants_picks()) {
        let snap = d.face_snap(&app.session, hits);
        return hits
            .iter()
            .find_map(|h| d.candidate_on(&app.session, h, snap.as_ref()).map(|s| (h.clone(), crate::dialogs_move::component_pick(app, h, s))));
    }
    hits.iter().filter(|h| pick_allowed(app, h)).find_map(|h| hit_candidate(app, h))
}

/// Every hit a click could pick, in order, each with its selection (Select Other).
pub fn candidates(app: &SolveApp, hits: &[Hit]) -> Vec<(Hit, Sel)> {
    if let Some(d) = app.dialog.as_ref().filter(|d| d.wants_picks()) {
        // The face under the cursor is looked up once for every sketch point a face place may
        // snap to, before Select Other caps its rows.
        let snap = d.face_snap(&app.session, hits);
        return hits
            .iter()
            .filter_map(|h| d.candidate_on(&app.session, h, snap.as_ref()).map(|s| (h.clone(), crate::dialogs_move::component_pick(app, h, s))))
            .collect();
    }
    hits.iter().filter(|h| pick_allowed(app, h)).filter_map(|h| hit_candidate(app, h)).collect()
}

/// What the selection filter lets a click pick with no dialog.
fn pick_allowed(app: &SolveApp, h: &Hit) -> bool {
    let off = |k: &str| app.ui.pick_off.iter().any(|x| x == k);
    match h {
        Hit::SketchCurve { .. } => !off("sketch") && !off("sketch_curves"),
        Hit::SketchPoint { .. } | Hit::SketchMidpoint { .. } => !off("sketch") && !off("sketch_points"),
        Hit::Profile { .. } => !off("sketch"),
        Hit::Vertex { .. } => !off("vertices") && !app.ui.pick_bodies && !app.ui.pick_components,
        Hit::Edge { .. } => !off("edges") && !app.ui.pick_bodies && !app.ui.pick_components,
        Hit::Face { .. } => !off("faces") || app.ui.pick_bodies || app.ui.pick_components,
        Hit::Plane { .. } | Hit::Axis { .. } | Hit::ConstructionPoint { .. } => true,
    }
}

/// The selection a click on `h` makes with no dialog (sketch entities only belong to the active sketch).
fn hit_candidate(app: &SolveApp, h: &Hit) -> Option<(Hit, Sel)> {
    match h {
        Hit::SketchCurve { sketch, id, .. } if app.session.active_sketch != Some(*sketch) => {
            Some((h.clone(), Sel::ExternalSketchCurve { sketch: *sketch, id: id.clone() }))
        }
        Hit::SketchPoint { sketch, id, .. } if app.session.active_sketch != Some(*sketch) => {
            Some((h.clone(), Sel::ExternalSketchPoint { sketch: *sketch, id: id.clone() }))
        }
        Hit::Face { body, .. } if app.ui.pick_components => Some((h.clone(), component_selection(app, body))),
        Hit::Face { body, .. } if app.ui.pick_bodies => Some((h.clone(), Sel::Body { name: body.clone() })),
        _ => hit_sel(h).map(|s| (h.clone(), s)),
    }
}

/// Use the Browser's exact occurrence identity, including nested/repeated placements.
/// Root bodies have no occurrence row, so retain their body identity.
pub(crate) fn component_selection(app: &SolveApp, body: &str) -> Sel {
    let doc = app.session.placed_document();
    let state = crate::open_meshing::session_world(&app.session);
    if let Some(b) = state.body(body) {
        let component = doc.model_body_component(b);
        let model = app.session.model.state();
        if let Some((_, path, _)) = doc
            .placements()
            .into_iter()
            .filter(|(c, _, _)| *c == component)
            .find(|(_, path, _)| doc.world_body_name(&model, b.definition_name(), component, path) == body)
            .filter(|(_, path, _)| !path.is_empty())
        {
            return Sel::Occurrence { path };
        }
    }
    Sel::Body { name: body.to_string() }
}

/// A pick for the open dialog's active input (a click, or a Select Other row).
pub(crate) fn dialog_pick(app: &mut SolveApp, hit: &Hit, sel: Sel) {
    // While OK computes, the dialog's inputs hold.
    if app.applying.is_some() {
        return;
    }
    // Press Pull's first pick of a curved face, a chamfer's face or an edge reopens it as an
    // offset, the chamfer or a fillet.
    if let Some(d) = &app.dialog
        && d.press_pull
        && d.inputs.first().is_some_and(|i| i.items.is_empty())
        && match &sel {
            Sel::Edge { .. } => true,
            Sel::Face { body, index, .. } => {
                crate::dialogs::planar_face(&app.session, body, *index).is_none()
                    || crate::dialogs::blend_feature(&app.session, body, *index).is_some()
            }
            _ => false,
        }
    {
        crate::dialogs::cancel(app);
        app.session.selection = vec![sel];
        app.start("solid.press_pull");
        return;
    }
    let sel = crate::dialogs_move::from_shown(app, sel);
    let Some(mut d) = app.dialog.take() else { return };
    // With holes moving by clicking (Preferences), a Single hole picked again on its face moves there.
    if app.preferences.hole_moves && crate::hole_place::pick_moves(&mut d, &app.session, &sel) {
        d.focus = true;
        d.awaiting_pick = false;
        d.pick_refused = None;
    } else {
        d.pick(&app.session, sel);
    }
    if let crate::dialogs::Kind::PathPlane { sketch, .. } = &mut d.kind {
        *sketch = match hit {
            Hit::SketchCurve { sketch, .. } => Some(*sketch),
            _ => None,
        };
        d.extra.remove("path");
    }
    // Split Body's sketch line keeps the sketch it was picked in (curve ids repeat across sketches).
    if matches!(d.kind, crate::dialogs::Kind::Split) {
        if let Hit::SketchCurve { sketch, .. } = hit {
            d.extra.insert("tool_sketch".into(), serde_json::json!(sketch));
        } else {
            d.extra.remove("tool_sketch");
        }
    }
    app.dialog = Some(d);
}

/// The edge a selection refers to now: by index if it still passes through the stored point,
/// otherwise the edge nearest the point.
fn edge_of(m: &sketcraft_engine::geom::Mesh, index: usize, point: Vec3) -> Option<&Vec<Vec3>> {
    let tol = m.bounds().diagonal().max(1.0) * 1e-6;
    let near = |e: &Vec<Vec3>| e.windows(2).map(|w| point.dist_to_segment(w[0], w[1])).fold(f64::INFINITY, f64::min);
    if let Some(e) = m.edges.get(index)
        && near(e) < tol
    {
        return Some(e);
    }
    m.edges.iter().min_by(|a, b| near(a).total_cmp(&near(b))).filter(|e| near(e) < tol * 1e3)
}

fn face_tris(sc: &mut GpuScene, m: &sketcraft_engine::geom::Mesh, face: usize, col: [u8; 4], lit: bool) {
    mesh_tris(sc, m, col, lit, |f| f as usize == face);
}

/// Emit a set of faces in one mesh pass. Whole-body feedback must not scan every triangle
/// again for each face of a detailed imported part.
fn mesh_tris(sc: &mut GpuScene, m: &sketcraft_engine::geom::Mesh, col: [u8; 4], lit: bool, takes: impl Fn(u32) -> bool) {
    let col = sc.color(col);
    for (t, f) in m.triangles.iter().zip(&m.tri_face) {
        if !takes(*f) {
            continue;
        }
        for k in t {
            let i = *k as usize;
            if let (Some(p), Some(n)) = (m.positions.get(i), m.normals.get(i)) {
                sc.tri(p.to_f32(), if lit { n.to_f32() } else { [0.0; 3] }, col);
            }
        }
    }
}

/// How much wider a sketch curve's halo is than the curve (#4862): a 1 px rim each side.
const SKETCH_HALO_WIDTH: f32 = 2.0;

/// A highlighted edge: a halo and a core, and where the model hides it, the core dimmed (so a
/// pick on the far side still shows).
/// The palette entries of a highlighted edge: its halo, its core, and its dimmed part where the
/// model hides it. Interned once for every edge drawn in them.
#[derive(Clone, Copy)]
struct EdgeColours {
    halo: crate::gpu::PaletteIndex,
    core: crate::gpu::PaletteIndex,
    hidden: crate::gpu::PaletteIndex,
}

impl EdgeColours {
    fn new(sc: &mut GpuScene, core: Color32, halo: Color32) -> EdgeColours {
        EdgeColours { halo: sc.color(c4(halo)), core: sc.color(c4(core)), hidden: sc.color(c4(core.gamma_multiply(0.45))) }
    }
}

fn edge_lines(sc: &mut GpuScene, e: &[Vec3], cols: EdgeColours, w: f32, normals: &[[[f32; 3]; 2]]) {
    let segments = e.len().saturating_sub(1);
    sc.reserve_lines(2 * segments, segments);
    for (col, width) in [(cols.halo, w + 3.0), (cols.core, w)] {
        // Segments without their faces' normals share one width word.
        let flat = crate::gpu::lifted_width(width, [[0.0; 3]; 2]);
        for (i, p) in e.windows(2).enumerate() {
            let word = normals.get(i).map_or(flat, |n| crate::gpu::lifted_width(width, *n));
            sc.lifted_line_word(p[0].to_f32(), p[1].to_f32(), col, word);
        }
    }
    for p in e.windows(2) {
        sc.hidden_line(p[0].to_f32(), p[1].to_f32(), cols.hidden, (w * 0.7).max(1.0));
    }
}

/// The normals of the faces beside an edge, for each of its segments (the two most different
/// ones found at the segment's ends; zero where none is found), so its highlight can be lifted
/// over a face seen at a grazing angle (`GpuScene::lifted_line`). One pass over the mesh's
/// vertices: the tessellation shares the edge's points with the faces on either side.
fn edge_normals(m: &sketcraft_engine::geom::Mesh, e: &[Vec3]) -> Vec<[[f32; 3]; 2]> {
    let tol = m.bounds().diagonal().max(1.0) * 1e-7;
    let key = |p: Vec3| ((p.x / tol).round() as i64, (p.y / tol).round() as i64, (p.z / tol).round() as i64);
    let mut at: std::collections::HashMap<(i64, i64, i64), Vec<Vec3>> = e.iter().map(|p| (key(*p), Vec::new())).collect();
    for (p, n) in m.positions.iter().zip(&m.normals) {
        if let Some(list) = at.get_mut(&key(*p))
            && list.len() < 8
        {
            list.push(*n);
        }
    }
    e.windows(2)
        .map(|w| {
            let mut found: Vec<Vec3> = Vec::new();
            for p in w {
                found.extend(at.get(&key(*p)).into_iter().flatten().copied());
            }
            let Some(first) = found.first().copied() else { return [[0.0; 3]; 2] };
            let other = found.iter().copied().min_by(|a, b| a.dot(first).total_cmp(&b.dot(first))).unwrap_or(first);
            [first.to_f32(), other.to_f32()]
        })
        .collect()
}

/// The outline of a highlighted face where the model hides it (dimmed).
fn hidden_face_outline(sc: &mut GpuScene, m: &sketcraft_engine::geom::Mesh, face: usize, col: Color32) {
    let col = sc.color(c4(col.gamma_multiply(0.45)));
    for (ei, e) in m.edges.iter().enumerate() {
        let on = m.edge_faces.get(ei).is_some_and(|f| f.iter().any(|x| *x as usize == face));
        if on && m.edge_pickable(ei) {
            for p in e.windows(2) {
                sc.hidden_line(p[0].to_f32(), p[1].to_f32(), col, 1.4);
            }
        }
    }
}

pub(crate) fn profile_fill(sc: &mut GpuScene, app: &SolveApp, sketch: u64, index: usize, fill: Color32, edge: Color32) {
    let st = crate::open_meshing::shown_world(app);
    let Some(ss) = st.sketch(sketch) else { return };
    let Some(p) = ss.profiles.get(index) else { return };
    let plane = match crate::dialogs::emboss_profile_plane(app, sketch) {
        Ok(Some(plane)) => plane,
        Ok(None) => ss.plane,
        Err(_) => return,
    };
    // Lift toward the eye so a profile on a face shows over it.
    let lift = app.cam.back() * (app.cam.half_height() * 0.002);
    let w = |q: solidcore_record::Vec2| (plane.to_world(q) + lift).to_f32();
    let col = sc.color(c4(fill));
    let edge = sc.color(c4(edge));
    // In the translucent pass (after the grid and edges) so it covers them.
    let proj = display_proj(app);
    let display = crate::sketch_display::profile_region(app, sketch, index, &plane, &proj);
    for [a, b, c] in display.as_ref().as_ref().map_or_else(Vec::new, |region| crate::sketch_display::fill_triangles(region, &mut 65_536)) {
        sc.trans_tri(w(a), [0.0; 3], col);
        sc.trans_tri(w(b), [0.0; 3], col);
        sc.trans_tri(w(c), [0.0; 3], col);
    }
    for lp in std::iter::once(&p.region.outer).chain(&p.region.holes) {
        for part in crate::sketch_display::boundary_parts(lp, &plane, &proj, app.viewport.pixels_per_point()) {
            for pair in part.windows(2) {
                sc.line((pair[0] + lift).to_f32(), (pair[1] + lift).to_f32(), edge, 2.0, false);
            }
        }
    }
}

/// An origin or construction plane: reference geometry, which a section does not cut (#1229).
/// A reference plane's square. `through` draws it over the model (Create Sketch: a plane inside
/// a body stays visible and pickable; or the Preferences setting to show reference geometry
/// through bodies), else it is depth-tested like the rest of the scene.
fn quad(sc: &mut GpuScene, q: &[Vec3; 4], fill: Color32, edge: Color32, through: bool) {
    let col = sc.color(c4(fill));
    for i in [0, 1, 2, 0, 2, 3] {
        if let Some(p) = q.get(i) {
            if through {
                sc.xray_tri(p.to_f32(), [0.0; 3], col);
            } else {
                sc.guide_tri(p.to_f32(), col);
            }
        }
    }
    for i in 0..4 {
        let (a, b) = (q[i].to_f32(), q[(i + 1) % 4].to_f32());
        reference_line(sc, a, b, c4(edge), 1.2, through);
    }
}

/// A reference line (plane edge, axis): over the model when `through` (as [`quad`]), else depth-tested and
/// left whole by a section.
fn reference_line(sc: &mut GpuScene, a: [f32; 3], b: [f32; 3], col: [u8; 4], width: f32, through: bool) {
    let col = sc.color(col);
    if through {
        sc.line(a, b, col, width, true);
    } else {
        sc.guide_line(a, b, col, width);
    }
}

/// What the highlight draws of the hovered item. A face or a plane is lit the same wherever on it
/// the pointer is, so the point under the pointer is left out: the highlight (a whole scanned
/// mesh is one face) is not built again at every move of the pointer across it.
fn hover_drawn(hover: &Option<Hit>) -> String {
    match hover {
        Some(Hit::Face { body, index, .. }) => format!("face {body:?} {index}"),
        Some(Hit::Plane { name, .. }) => format!("plane {name:?}"),
        other => format!("{other:?}"),
    }
}

/// The body the current hover actually selects, after the active dialog's filter.
fn hovered_body(app: &SolveApp) -> Option<String> {
    if app.tool.is_some() {
        return None;
    }
    let hit = app.viewport.hover.as_ref()?;
    let (_, selected) = candidate(app, std::slice::from_ref(hit))?;
    match selected {
        Sel::Body { name } => Some(name),
        _ => None,
    }
}

fn highlight_geometry_key(app: &SolveApp) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let selected = app.highlighted();
    app.session.revision.hash(&mut h);
    (std::sync::Arc::as_ptr(&crate::open_meshing::shown_world(app)) as usize).hash(&mut h);
    serde_json::to_string(&selected).unwrap_or_default().hash(&mut h);
    hover_drawn(&app.viewport.hover).hash(&mut h);
    hovered_body(app).hash(&mut h);
    app.viewport.hover_feature.hash(&mut h);
    app.viewport.hover_bodies.hash(&mut h);
    app.viewport.hover_plane.hash(&mut h);
    app.dialog.as_ref().and_then(|d| d.list_hover.as_ref()).map(|(_, item)| format!("{item:?}")).hash(&mut h);
    // Which mesh lights a face follows how the model draws its body (see `shown_mesh`), and at
    // which display quality its plan and edges are made.
    sketcraft_engine::doc::display_quality::current().hash(&mut h);
    lod_generation(app).hash(&mut h);
    // Whole-body highlights redraw the model scene's own blocks: they must be the scene's
    // current ones (their keys and the bodies' ranges in them).
    scene_key(app).hash(&mut h);
    // And the mode the model draws bodies in: from their plans on the GPU, or built here.
    app.viewport.gpu.is_some_and(|g| g.tess).hash(&mut h);
    app.session.active_component.hash(&mut h);
    app.session.active_occurrence_path.hash(&mut h);
    app.ui.visual_style.hash(&mut h);
    crate::render_workspace::assign::preview_key(app).hash(&mut h);
    app.ui.pick_bodies.hash(&mut h);
    app.ui.pick_components.hash(&mut h);
    crate::render_workspace::active(app).hash(&mut h);
    app.session.active_sketch.hash(&mut h);
    (app.dialog.is_some(), app.tool.is_some()).hash(&mut h);
    // Which input is active decides what a round edge or face under the cursor stands for.
    app.dialog.as_ref().and_then(|d| d.active_input()).map(|i| i.accept).hash(&mut h);
    crate::theme::is_dark().hash(&mut h);
    crate::theme::vision().hash(&mut h);
    // Two themes may share a light/dark base while drawing different geometry colours.
    crate::theme_files::color_key(&Tokens::base()).hash(&mut h);
    app.preview.effective_replaced().hash(&mut h);
    app.preview.rolled_back.hash(&mut h);
    app.viewport.refused_ring.hash(&mut h);
    // Where a Move dialog has moved its bodies; while its triad is dragged, only that it is.
    if crate::dialogs_move::dragging(app) {
        "dragging".hash(&mut h);
    } else {
        crate::dialogs_move::chained(app).map(|(_, c)| c.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>()).hash(&mut h);
    }
    app.origin_visible().hash(&mut h);
    app.picking_plane().hash(&mut h);
    app.preferences.reference_geometry_through_bodies.hash(&mut h);
    app.session.visibility.hidden_origin.hash(&mut h);
    app.session.visibility.hidden_bodies.hash(&mut h);
    app.dialog.as_ref().map(|d| &d.edit_source_bodies).hash(&mut h);
    app.session.visibility.hidden_occurrences.hash(&mut h);
    app.session.visibility.hidden_folders.hash(&mut h);
    // How strongly print faces are tinted (Preferences › General › Slicer).
    app.preferences.print_face_highlight.to_bits().hash(&mut h);
    image_target_outlines(app).hash(&mut h);
    h.finish()
}

/// Product pixels must stay visible while their appearance target is selected.
fn image_target_outlines(app: &SolveApp) -> bool {
    crate::render_workspace::active(app) && !app.preferences.render_legacy_image_defaults && crate::render_workspace::live::has_picture(app)
}

/// Display-only triangle-mesh polylines are not modeling edges. Targets without
/// drawable pickable boundaries keep their existing face/body fill feedback.
fn has_pickable_outline(mesh: &sketcraft_engine::geom::Mesh) -> bool {
    mesh.edges.iter().enumerate().any(|(index, edge)| edge.len() >= 2 && mesh.edge_pickable(index))
}

pub(crate) fn highlight_key(app: &SolveApp) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    highlight_geometry_key(app).hash(&mut h);
    let selected = app.highlighted();
    app.cam.half_height().to_bits().hash(&mut h);
    // A typed dimension value hides the hovered profile (see `build_highlight`).
    crate::dim_view::preview_solve(app).is_some().hash(&mut h);
    // Body triangles and outlines live in world space; the GPU applies the new camera.
    // Only sketch feedback needs projected tessellation and the profile's lift toward the eye.
    let projected = selected.iter().any(|s| matches!(s, Sel::Profile { .. } | Sel::SketchCurve { .. } | Sel::ExternalSketchCurve { .. }))
        || matches!(app.viewport.hover, Some(Hit::Profile { .. }))
        || !selected_sketches(app, &selected).is_empty();
    if projected {
        crate::sketch_display::projection_key(&display_proj(app), app.viewport.pixels_per_point()).hash(&mut h);
    }
    h.finish()
}

#[derive(Clone)]
struct HoverBlock {
    key: u64,
    state: std::sync::Arc<sketcraft_engine::doc::ModelState>,
    doc: std::sync::Arc<sketcraft_engine::doc::Document>,
    /// The colours the block's palette indices refer to.
    palette: std::sync::Arc<crate::gpu::Palette>,
    block: crate::gpu::SceneBlock,
    /// Whole bodies the model draws from their plans: reused like the block when the camera
    /// moves (they are the model's own evaluated triangles, not a copy of them).
    hover_planned: Vec<std::sync::Arc<sketcraft_engine::geom::tess::TessPlan>>,
    selected_planned: Vec<std::sync::Arc<sketcraft_engine::geom::tess::TessPlan>>,
    /// CPU-built bodies: their triangles in the model scene's blocks, reused the same way.
    hover_blocks: Vec<crate::gpu::BlockTris>,
    selected_blocks: Vec<crate::gpu::BlockTris>,
    /// Bodies the model draws as placements of shared triangles: the same placements with the
    /// highlight colour.
    hover_instanced: Vec<crate::gpu::InstancedMesh>,
    selected_instanced: Vec<crate::gpu::InstancedMesh>,
    /// Faded or see-through bodies in the model's blocks, drawn again in this palette.
    recoloured: Vec<crate::gpu::BlockTris>,
}

/// The plan the model draws a whole highlighted body from, when it draws one on the GPU. The
/// highlight then redraws those evaluated triangles in the highlight colour instead of building
/// a copy of every triangle on the CPU: lighting a whole assembly's faces that way took seconds
/// (every triangle of every body, again), while the model already had them all on the GPU.
fn highlight_plan(
    app: &SolveApp,
    b: &sketcraft_engine::doc::ModelBody,
    active_bodies: &Option<ActiveBodyNames>,
) -> Option<std::sync::Arc<sketcraft_engine::geom::tess::TessPlan>> {
    if crate::render_workspace::active(app) {
        return None;
    }
    drawn_plan(app, b, model_plan_class(app, b, active_bodies).is_some())
}

/// Whether the model draws a whole body opaque on the GPU, rather than building it on the CPU:
/// a live preview stands in for none, faded and see-through bodies stay on the CPU, and face
/// opacities matter only where faces are shaded.
fn highlight_opaque(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody, active_bodies: &Option<ActiveBodyNames>) -> bool {
    if crate::render_workspace::active(app) || app.preview.effective_replaced().contains(&b.name) {
        return false;
    }
    let doc = &app.session.doc;
    let faces_opaque = app.ui.visual_style == 2 || doc.face_colors(b).iter().all(|(_, l)| l.opacity >= 0.999);
    !body_faded(app, b, active_bodies) && doc.model_body_opacity(b) >= 0.999 && faces_opaque
}

/// Whole highlighted bodies the model draws as placements of shared triangles, collected as one
/// [`InstancedMesh`] per mesh (a placement and its colour each): the model scene already holds
/// the shared vertices, so a whole assembly under the pointer draws them again, not a copy.
#[derive(Default)]
struct HighlightInstances {
    meshes: Vec<crate::gpu::InstancedMesh>,
    /// The index in `meshes` of each mesh, by its address.
    by_mesh: std::collections::HashMap<usize, usize>,
}

impl HighlightInstances {
    /// Add the body at `color`, when the model draws it as an instanced placement of an opaque
    /// shared mesh; `false` when it does not (the caller builds its feedback on the CPU).
    fn add(&mut self, app: &SolveApp, b: &sketcraft_engine::doc::ModelBody, color: crate::gpu::PaletteIndex) -> bool {
        // A live preview stands in for the body; a body with face appearances of its own is built
        // on the CPU, never as an instance. A faded or see-through placement is lit like an opaque
        // one: the highlight colour is the placement's own here, not the model's.
        if app.preview.effective_replaced().contains(&b.name) || !app.session.doc.face_colors(b).is_empty() {
            return false;
        }
        let Some((mesh, m)) = shared_mesh(app, b) else { return false };
        let key = std::sync::Arc::as_ptr(&mesh) as usize;
        let at = match self.by_mesh.get(&key) {
            Some(i) => *i,
            None => {
                // The model scene's shared triangles (made ahead, or with its own instances).
                let tris = app
                    .viewport
                    .instanced_tris
                    .lock()
                    .ok()
                    .and_then(|c| c.get(&key).filter(|(held, _)| std::sync::Arc::ptr_eq(held, &mesh)).map(|(_, t)| t.clone()))
                    .or_else(|| taken_shared_tris(&mesh));
                let Some(tris) = tris else { return false };
                self.meshes.push(crate::gpu::InstancedMesh {
                    key: key as u64,
                    tris,
                    instances: Vec::new(),
                    look: crate::gpu::InstanceLook::Opaque,
                    levels: Vec::new(),
                    closed: true,
                });
                self.by_mesh.insert(key, self.meshes.len() - 1);
                self.meshes.len() - 1
            }
        };
        let rows = [0, 1, 2].map(|r| [m[0][r] as f32, m[1][r] as f32, m[2][r] as f32, m[3][r] as f32]);
        match self.meshes.get_mut(at) {
            Some(inst) => {
                inst.place(rows, color);
                true
            }
            None => false,
        }
    }

    /// The meshes with their placements (none when nothing was added).
    fn finish(self) -> Vec<crate::gpu::InstancedMesh> {
        self.meshes
    }
}

/// The model scene's triangles for a whole highlighted body it builds on the CPU (no display
/// plan): the block holding them and the body's vertex range in it. Drawing those again tinted
/// costs no CPU work, where copying every triangle did (a whole assembly hovered at once stalled
/// the window). See-through and faded bodies have no range here: they are not in the block's
/// opaque triangles, and the CPU feedback colours them.
fn highlight_block_tris(app: &SolveApp, b: &sketcraft_engine::doc::ModelBody) -> Option<crate::gpu::BlockTris> {
    if crate::render_workspace::active(app) || app.preview.effective_replaced().contains(&b.name) {
        return None;
    }
    let map = app.viewport.body_tris.lock().ok()?;
    map.get(&b.name).cloned()
}

/// The world's bodies by name, the map made at the first lookup: [`ModelState::body`] looks along
/// every body, so a highlight with thousands of bodies selected looked up each of them that way.
/// A name held by more than one body finds the first, as `ModelState::body` does.
///
/// [`ModelState::body`]: sketcraft_engine::doc::ModelState::body
struct BodiesByName<'a> {
    state: &'a sketcraft_engine::doc::ModelState,
    map: Option<std::collections::HashMap<&'a str, &'a sketcraft_engine::doc::ModelBody>>,
}

impl<'a> BodiesByName<'a> {
    fn new(state: &'a sketcraft_engine::doc::ModelState) -> Self {
        Self { state, map: None }
    }

    fn get(&mut self, name: &str) -> Option<&'a sketcraft_engine::doc::ModelBody> {
        let state = self.state;
        // Last inserted wins: the bodies go in from the last, so the first of a name is kept.
        let map = self.map.get_or_insert_with(|| state.bodies.iter().rev().map(|b| (b.name.as_str(), b)).collect());
        map.get(name).copied()
    }
}

/// How a whole body is lit: hovered or selected, and in which colour (where the model's own
/// colour is not kept: placements, and faded or see-through bodies).
#[derive(Clone, Copy)]
struct WholeBody {
    selected: bool,
    colour: [u8; 4],
}

/// Light the whole of `b` from what the model already has on the GPU, copying no triangle: its
/// plan or its opaque runs (tinted over the model's own colours), its block's vertices recoloured
/// in the highlight's palette (faded and see-through bodies), or its placements. `false` when
/// none applies (wireframe, a live preview standing in for the body): the caller copies it.
fn light_whole_body(
    app: &SolveApp,
    b: &sketcraft_engine::doc::ModelBody,
    active_bodies: &Option<ActiveBodyNames>,
    lit: WholeBody,
    body_slots: &crate::scene_palette::PaletteArena,
    feedback: &mut GpuScene,
    instances: &mut HighlightInstances,
) -> bool {
    if let Some(plan) = highlight_plan(app, b, active_bodies) {
        if lit.selected {
            feedback.selected_planned.push(plan)
        } else {
            feedback.hover_planned.push(plan)
        }
        return true;
    }
    let opaque = highlight_opaque(app, b, active_bodies);
    if let Some(tris) = highlight_block_tris(app, b) {
        if opaque && !tris.tris.is_empty() {
            if lit.selected {
                feedback.selected_blocks.push(tris)
            } else {
                feedback.hover_blocks.push(tris)
            }
            return true;
        }
        // Faded or see-through: the body's every vertex again, its slots in this palette holding
        // the highlight colour (as the copy it replaces was drawn).
        if !opaque && let Some(slots) = body_slots.slots(&b.name) {
            let palette = std::sync::Arc::make_mut(&mut feedback.palette);
            palette.set(slots.body, lit.colour);
            for f in 0..b.body.face_count() {
                palette.set(slots.face(f), lit.colour);
            }
            feedback.recoloured.push(tris);
            return true;
        }
    }
    let colour = feedback.color(lit.colour);
    instances.add(app, b, colour)
}

/// The highlight scene: origin widget, construction planes, selection and hover.
pub(crate) fn build_highlight(app: &SolveApp) -> GpuScene {
    let began = crate::now_ms();
    let t = Tokens::get();
    // The model scene's stable palette slots first: whole faded or see-through bodies are drawn
    // again from the model's own vertices in this palette, their slots set to the highlight's
    // colour (see `light_whole_body`); this scene's own colours follow.
    // The key first: it reads the arena (`scene_key`), which is held below.
    let key = highlight_geometry_key(app);
    let body_slots = app.viewport.palette.lock().unwrap_or_else(|e| e.into_inner());
    let mut sc = GpuScene::with_palette(crate::gpu::Palette::with_stable(body_slots.entries()));
    let st = crate::open_meshing::shown_world(app);
    let s = &app.session;
    let active_bodies = active_body_names(app);
    // Whole bodies are lit by drawing the model's own triangles again, but a drag drawn as a layer
    // moves its bodies' triangles on the GPU from where the drag found them: those are built
    // where they stand.
    let dragged = drag_layer::dragged_names(app);
    let gpu_lit = |b: &sketcraft_engine::doc::ModelBody| !dragged.contains(b.name.as_str());
    // Faces lit over the model use the triangles the model shows them with.
    let product = crate::render_workspace::active(app);
    let outlines = image_target_outlines(app);
    // Bodies a Move dialog moves are lit where they stand (as they are picked). While its triad
    // is dragged they aren't lit at all, so the highlight is built again once, when it lets go,
    // rather than every frame.
    let live = crate::dialogs_move::chained(app);
    let dragging = crate::dialogs_move::dragging(app);
    let moving = |name: &str| dragging && chain_of(live.as_ref(), name).is_some();
    let chained = live.as_ref().filter(|_| !dragging);
    let carry = |b: &sketcraft_engine::doc::ModelBody| chain_of(chained, &b.name);
    let replaced = |name: &str| moving(name) || (app.preview.effective_replaced().iter().any(|n| n == name) && chain_of(chained, name).is_none());
    let shown = |b: &sketcraft_engine::doc::ModelBody| carried_mesh(carry(b), if product { b.mesh() } else { shown_mesh(app, b, &active_bodies) });
    let drawn = |b: &sketcraft_engine::doc::ModelBody| carried_mesh(carry(b), drawn_mesh(app, b));
    let items = app.highlighted();
    let sketches = selected_sketches(app, &items);
    let mut sel = Vec::new();
    for item in &items {
        for piece in displayed_selection(app, item).into_iter().filter(|s| !product || matches!(s, Sel::Body { .. } | Sel::Face { .. })) {
            if !sel.iter().any(|x| crate::selection::same_item(x, &piece)) {
                sel.push(piece);
            }
        }
    }
    // A picked item whose dialog row is under the pointer shows as hovered, not as picked.
    if let Some((_, item)) = app.dialog.as_ref().and_then(|d| d.list_hover.as_ref()) {
        let lit = displayed_selection(app, item);
        sel.retain(|x| !lit.iter().any(|y| crate::selection::same_item(x, y)));
    }
    // Component hover expands through the same occurrence selection as a Browser row.
    let component_hover = crate::render_workspace::assign::hover(app).map(|sel| displayed_selection(app, sel)).or_else(|| {
        app.viewport
            .hover
            .as_ref()
            .filter(|h| app.ui.pick_components && matches!(h, Hit::Face { .. }))
            .and_then(|h| hit_candidate(app, h))
            .map(|(_, s)| displayed_selection(app, &s))
    });
    let hover = app.viewport.hover.as_ref().filter(|_| component_hover.is_none());
    let size = origin_size(app.cam.half_height());
    if !product {
        // Origin planes and construction planes.
        let plane_state = |name: &str| {
            let selected = sel.iter().any(|x| matches!(x, Sel::Plane { name: n } if n == name));
            let hovered = matches!(hover, Some(Hit::Plane { name: n, .. }) if n == name) || app.viewport.hover_plane.as_deref() == Some(name);
            (selected, hovered)
        };
        let mut planes: Vec<(String, [Vec3; 4], Color32)> =
            origin_planes(app).into_iter().map(|(n, _, q)| (n.to_string(), q, t.origin_plane)).collect();
        planes.extend(construction_quads(app).into_iter().map(|(n, _, q)| (n, q, t.construction_plane)));
        let through = app.picking_plane() || app.preferences.reference_geometry_through_bodies;
        for (name, q, base) in &planes {
            let (selected, hovered) = plane_state(name);
            let fill = if hovered {
                t.origin_plane_hover
            } else if selected {
                Color32::from_rgba_unmultiplied(t.sel_face.r(), t.sel_face.g(), t.sel_face.b(), 150)
            } else {
                *base
            };
            quad(&mut sc, q, fill, if selected { t.sel_edge } else { t.origin_plane_edge }, through);
        }
        // Origin axes: solid near the origin, dashed beyond; on the SketchUp model screen red,
        // green and blue, running far, solid one way and dotted the other.
        for (name, dir) in origin_axes(app) {
            let col = match name {
                "X" => t.axis_x,
                "Y" => t.axis_y,
                _ => t.axis_z,
            };
            let on = sel.iter().any(|x| matches!(x, Sel::Axis { name: n } if n == name)) || matches!(hover, Some(Hit::Axis { name: n }) if n == name);
            let w = if on { 4.0 } else { 2.0 };
            if crate::sketchup_screen::on(app) {
                let col = c4(crate::sketchup_screen::axis_colour(name));
                for (a, b, solid) in crate::sketchup_screen::axis_segments(dir, size) {
                    reference_line(&mut sc, a.to_f32(), b.to_f32(), col, if solid { w * 0.75 } else { w * 0.6 }, through);
                }
                continue;
            }
            reference_line(&mut sc, [0.0; 3], (dir * size).to_f32(), c4(col), w, through);
            let dash = size * 0.08;
            let mut a = size + dash;
            while a < size * AXIS_LEN {
                reference_line(&mut sc, (dir * a).to_f32(), (dir * (a + dash)).to_f32(), c4(col), w * 0.75, through);
                a += dash * 2.0;
            }
        }
        // Construction axes (dashed, the length of the origin axes either side) and points (a
        // small star), in the construction colour.
        for (name, a, b) in construction_axis_segments(app) {
            let on =
                sel.iter().any(|x| matches!(x, Sel::Axis { name: n } if *n == name)) || matches!(hover, Some(Hit::Axis { name: n }) if *n == name);
            let col = c4(if on { t.sel_edge } else { t.construction_geom });
            let w = if on { 3.0 } else { 1.6 };
            let len = a.dist(b);
            let dash = size * 0.08;
            let mut s = 0.0;
            while s < len {
                let e = (s + dash).min(len);
                reference_line(&mut sc, a.lerp(b, s / len).to_f32(), a.lerp(b, e / len).to_f32(), col, w, through);
                s += dash * 1.6;
            }
        }
        for (name, p) in construction_points(app) {
            let on = sel.iter().any(|x| matches!(x, Sel::ConstructionPoint { name: n } if *n == name))
                || matches!(hover, Some(Hit::ConstructionPoint { name: n }) if *n == name);
            let col = sc.color(c4(if on { t.sel_edge } else { t.construction_geom }));
            let r = size * 0.04;
            for d in [Vec3::X, Vec3::Y, Vec3::Z] {
                sc.line((p - d * r).to_f32(), (p + d * r).to_f32(), col, 2.0, true);
            }
        }
    }
    // A round edge or face under the cursor (or picked) in an axis input shows its axis, and a
    // circle in a point input its centre, as construction geometry. The axis runs through the
    // solid it belongs to, so it is drawn over the model.
    let (round_axes, round_centres) = round_pick_feedback(app);
    let construction = sc.color(c4(t.construction_geom));
    for (a, b) in round_axes {
        let len = a.dist(b);
        let dash = size * 0.08;
        let mut s = 0.0;
        while s < len {
            let e = (s + dash).min(len);
            sc.line(a.lerp(b, s / len).to_f32(), a.lerp(b, e / len).to_f32(), construction, 2.0, true);
            s += dash * 1.6;
        }
    }
    for p in round_centres {
        let r = size * 0.04;
        for d in [Vec3::X, Vec3::Y, Vec3::Z] {
            sc.line((p - d * r).to_f32(), (p + d * r).to_f32(), construction, 2.0, true);
        }
    }
    // A face the modelling kernel refuses, while the window points at it: a ring round its edges
    // in the error colour, over the shading (and dimmed where the model hides it), like the
    // selection's outline.
    for (body, face) in &app.viewport.refused_ring {
        let Some(b) = st.body(body).filter(|_| app.session.body_visible(body)) else { continue };
        // The face itself tinted (a face whose edges are two small circles still reads as one).
        face_tris(&mut sc, &shown(b), *face, c4(t.error.gamma_multiply(0.35)), false);
        let m = drawn_mesh(app, b);
        let hidden = sc.color(c4(t.error.gamma_multiply(0.45)));
        let colours = EdgeColours { halo: sc.color(c4(t.error.gamma_multiply(0.5))), core: sc.color(c4(t.error)), hidden };
        for (ei, e) in m.edges.iter().enumerate() {
            if m.edge_faces.get(ei).is_some_and(|f| f.iter().any(|x| *x as usize == *face)) && m.edge_pickable(ei) {
                edge_lines(&mut sc, e, colours, 3.0, &edge_normals(&m, e));
            }
        }
    }
    // Reference widgets follow zoom; the often much larger world-space feedback does not.
    let projected = sel.iter().any(|s| matches!(s, Sel::Profile { .. } | Sel::SketchCurve { .. } | Sel::ExternalSketchCurve { .. }))
        || matches!(hover, Some(Hit::Profile { .. }))
        || !sketches.is_empty();
    if !projected
        && let Some(cached) = app.viewport.hover_block.lock().ok().and_then(|cache| {
            cache.as_ref().filter(|c| c.key == key && std::sync::Arc::ptr_eq(&c.state, &st) && std::sync::Arc::ptr_eq(&c.doc, &s.doc)).cloned()
        })
    {
        sc.palette = cached.palette;
        sc.blocks.push(cached.block);
        sc.hover_planned = cached.hover_planned;
        sc.selected_planned = cached.selected_planned;
        sc.hover_blocks = cached.hover_blocks;
        sc.selected_blocks = cached.selected_blocks;
        sc.recoloured = cached.recoloured;
        sc.hover_instanced = cached.hover_instanced;
        sc.selected_instanced = cached.selected_instanced;
        return sc;
    }
    let mut feedback = GpuScene { palette: std::sync::Arc::new((*sc.palette).clone()), ..Default::default() };
    let (hover_edge, selected_edge) =
        (EdgeColours::new(&mut feedback, t.hover_edge, t.hover_edge_halo), EdgeColours::new(&mut feedback, t.sel_edge, t.sel_edge_rim));
    let mut hover_instances = HighlightInstances::default();
    let mut selected_instances = HighlightInstances::default();
    // Print faces (#8429): tinted towards the colour section views give cut faces, and outlined,
    // so the face a slicer export lays its body on shows at a glance. Drawn before the selection,
    // which lights over it.
    // The share of that colour is the user's (Preferences › General › Slicer, default a fifth); the
    // outline takes a little over twice it, so the face reads as marked without outshouting the model.
    let strength = app.preferences.print_face_highlight.clamp(0.0, 1.0);
    if !product && strength > 0.0 {
        let cap = t.section_cap;
        let blend = |from: u8, to: u8, k: f32| (f32::from(from) + (f32::from(to) - f32::from(from)) * k).round().clamp(0.0, 255.0) as u8;
        let mix = |body: u8, tint: u8| blend(body, tint, strength);
        let edge = t.body_edge;
        let k = (strength * 2.25).min(1.0);
        let rim = Color32::from_rgb(blend(edge.r(), cap.r(), k), blend(edge.g(), cap.g(), k), blend(edge.b(), cap.b(), k));
        let outline = EdgeColours::new(&mut feedback, rim, rim);
        for b in st.bodies.iter().filter(|b| app.session.body_visible(&b.name) && !app.preview.effective_replaced().contains(&b.name)) {
            let Some(Ok(face)) = s.doc.print_face(&st, b) else { continue };
            let (r, g, bl) = body_rgb(app, &b.name);
            face_tris(&mut feedback, &shown(b), face.index, [mix(r, cap.r()), mix(g, cap.g()), mix(bl, cap.b()), 255], true);
            let m = drawn_mesh(app, b);
            for (ei, e) in m.edges.iter().enumerate() {
                if m.edge_pickable(ei) && m.edge_faces.get(ei).is_some_and(|fs| fs.iter().any(|f| *f as usize == face.index)) {
                    edge_lines(&mut feedback, e, outline, 2.0, &[]);
                }
            }
        }
    }
    // Selected things. Their bodies are found by name in a map made once, at the first lookup:
    // looking each one up along the world's bodies made Select All on an assembly of thousands
    // of bodies quadratic (millions of name comparisons on every hover change).
    let mut by_name = BodiesByName::new(&st);
    for x in &sel {
        match x {
            Sel::Face { body, index, .. } => {
                // A live preview stands in for the body: its old faces would fight with it.
                // Lit, not flat: an imported mesh (STL, OBJ) is one face, and a flat fill would
                // paint the whole part as a single silhouette.
                if let Some(b) = by_name.get(body).filter(|_| app.body_visible(body) && !replaced(body)) {
                    let m = shown(b);
                    if outlines && has_pickable_outline(&m) {
                        for (ei, e) in m.edges.iter().enumerate() {
                            if m.edge_pickable(ei) && m.edge_faces.get(ei).is_some_and(|faces| faces.iter().any(|f| *f as usize == *index)) {
                                edge_lines(&mut feedback, e, selected_edge, 2.0, &[]);
                            }
                        }
                    } else {
                        face_tris(&mut feedback, &m, *index, c4(t.sel_face), true);
                        hidden_face_outline(&mut feedback, &drawn(b), *index, t.sel_edge);
                    }
                }
            }
            Sel::Body { name } => {
                if let Some(b) = by_name.get(name)
                    && app.body_visible(name)
                {
                    if let Some(m) = outlines.then(|| shown(b)).filter(|mesh| has_pickable_outline(mesh)) {
                        for (_, e) in m.edges.iter().enumerate().filter(|(ei, _)| m.edge_pickable(*ei)) {
                            edge_lines(&mut feedback, e, selected_edge, 2.0, &[]);
                        }
                    } else {
                        let lit = WholeBody { selected: true, colour: c4(t.body_selected) };
                        if !(gpu_lit(b) && light_whole_body(app, b, &active_bodies, lit, &body_slots, &mut feedback, &mut selected_instances)) {
                            let m = shown(b);
                            let count = b.body.face_count();
                            mesh_tris(&mut feedback, &m, c4(t.body_selected), true, |f| (f as usize) < count);
                        }
                    }
                }
            }
            Sel::Edge { body, index, point } => {
                if let Some(b) = by_name.get(body)
                    && app.body_visible(body)
                    && !moving(body)
                {
                    let m = drawn(b);
                    let point = if carry(b).is_some() { crate::dialogs_move::shown_point(app, body, *point) } else { *point };
                    if let Some(e) = edge_of(&m, *index, point) {
                        edge_lines(&mut feedback, e, selected_edge, 3.0, &edge_normals(&m, e));
                    }
                }
            }
            // Not while a dimension's value is typed: the design's profile would show over the
            // sketch drawn at it (as for the hovered one below). Other sketches are drawn where
            // they are.
            Sel::Profile { sketch, index } if crate::dim_view::previewed_sketch(app) != Some(*sketch) => {
                profile_fill(&mut feedback, app, *sketch, *index, t.sel_profile, t.sel_profile_edge)
            }
            // A 3D sketch curve, in the sketch it names, or whichever sketch has it.
            Sel::SketchCurve { id, sketch } if st.sketches.iter().any(|ss| ss.sketch.wire_index(id).is_some()) => {
                let sel_edge = feedback.color(c4(t.sel_edge));
                for ss in st.sketches.iter().filter(|ss| sketch.is_none_or(|k| ss.feature == k)) {
                    if let Some(w) = ss.sketch.wire_index(id).and_then(|i| ss.sketch.wires.get(i)) {
                        for q in w.pts.windows(2) {
                            feedback.line(q[0].to_f32(), q[1].to_f32(), sel_edge, 3.5, true);
                        }
                    }
                }
            }
            // A curve picked into a dialog from another sketch than the active one names it.
            // Not the curves of the sketch drawn at a typed value: the design has them elsewhere.
            Sel::ExternalSketchCurve { sketch, id } | Sel::SketchCurve { id, sketch: Some(sketch) }
                if crate::dim_view::previewed_sketch(app) != Some(*sketch) =>
            {
                let sel_edge = feedback.color(c4(t.sel_edge));
                if let Some(ss) = st.sketch(*sketch)
                    && let Some(ci) = ss.sketch.curve_index(id)
                {
                    for pts in &crate::sketch_display::curve_sample(app, *sketch, ci, &ss.plane, &display_proj(app)).parts {
                        let mode = ss.sketch.curves.get(ci).map_or(0, |c| if c.centerline { 2 } else { u8::from(c.construction) });
                        for (a, b) in sketch_segments(pts, mode, grid_step(app.cam.half_height()).0) {
                            feedback.line(a.to_f32(), b.to_f32(), sel_edge, 3.5, true);
                        }
                    }
                } else if let Some(w) = st.sketch(*sketch).and_then(|ss| ss.sketch.wire_index(id).and_then(|i| ss.sketch.wires.get(i))) {
                    // A 3D sketch curve of that sketch.
                    for q in w.pts.windows(2) {
                        feedback.line(q[0].to_f32(), q[1].to_f32(), sel_edge, 3.5, true);
                    }
                }
            }
            Sel::SketchCurve { id, .. } if crate::dim_view::previewed_sketch(app).is_none() => {
                if let Some(ss) = app.session.active_sketch.and_then(|sid| st.sketch(sid))
                    && let Some(ci) = ss.sketch.curve_index(id)
                {
                    let sel_edge = feedback.color(c4(t.sel_edge));
                    for pts in &crate::sketch_display::curve_sample(app, ss.feature, ci, &ss.plane, &display_proj(app)).parts {
                        let mode = ss.sketch.curves.get(ci).map_or(0, |c| if c.centerline { 2 } else { u8::from(c.construction) });
                        for (a, b) in sketch_segments(pts, mode, grid_step(app.cam.half_height()).0) {
                            feedback.line(a.to_f32(), b.to_f32(), sel_edge, 3.5, true);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // A sketch picked in the Browser or the timeline: all its curves in their line types, over
    // the model, also while the sketch is hidden or inside a body, so it is easy to find.
    let (unit, proj) = (grid_step(app.cam.half_height()).0, display_proj(app));
    let sel_edge = feedback.color(c4(t.sel_edge));
    for ss in sketches.iter().filter_map(|id| st.sketch(*id)) {
        for (pts, col, cons) in sketch_lines_with_sampler(&ss.sketch, &ss.plane, false, &[], |_, index, plane| {
            crate::sketch_display::curve_sample(app, ss.feature, index, plane, &proj).parts.clone()
        }) {
            for (a, b) in sketch_segments(&pts, sketch_line_mode(col, cons), unit) {
                feedback.line(a.to_f32(), b.to_f32(), sel_edge, 3.5, true);
            }
        }
    }
    // Timeline modifiers light only their resulting faces; browser rows light whole bodies.
    let mut hovered = app.viewport.hover_feature.map(|id| feature_geometry(app, id)).unwrap_or_default();
    hovered.extend(app.viewport.hover_bodies.iter().map(|name| Sel::Body { name: name.clone() }));
    hovered.extend(component_hover.into_iter().flatten());
    let body_hover = hovered_body(app);
    hovered.extend(body_hover.iter().map(|name| Sel::Body { name: name.clone() }));
    if let Some((_, item @ (Sel::Body { .. } | Sel::Occurrence { .. } | Sel::Feature { .. }))) =
        app.dialog.as_ref().and_then(|d| d.list_hover.as_ref())
    {
        hovered.extend(displayed_selection(app, item));
    }
    // By name, once: a whole assembly holds thousands of bodies, and scanning the list for
    // each of them is quadratic (as is a body lookup by name in the world).
    let hovered_whole: std::collections::HashSet<&str> =
        hovered.iter().filter_map(|x| if let Sel::Body { name } = x { Some(name.as_str()) } else { None }).collect();
    let selected_whole: std::collections::HashSet<&str> =
        sel.iter().filter_map(|x| if let Sel::Body { name } = x { Some(name.as_str()) } else { None }).collect();
    let mut hovered_faces: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    let mut selected_faces: std::collections::HashMap<&str, std::collections::HashSet<usize>> = std::collections::HashMap::new();
    for x in &hovered {
        if let Sel::Face { body, index, .. } = x {
            hovered_faces.entry(body.as_str()).or_default().push(*index);
        }
    }
    for x in &sel {
        if let Sel::Face { body, index, .. } = x {
            selected_faces.entry(body.as_str()).or_default().insert(*index);
        }
    }
    for b in st.bodies.iter().filter(|b| app.body_visible(&b.name) && !app.preview.effective_replaced().contains(&b.name)) {
        let whole = hovered_whole.contains(b.name.as_str());
        let faces: Vec<usize> =
            if whole { (0..b.body.face_count()).collect() } else { hovered_faces.get(b.name.as_str()).cloned().unwrap_or_default() };
        if faces.is_empty() {
            continue;
        }
        let m = if outlines { shown(b) } else { drawn_mesh(app, b) };
        let use_outlines = outlines && has_pickable_outline(&m);
        let base = body_color(&s.doc, b).unwrap_or(t.body);
        let k = t.hover_face_lift;
        let hover_col = [base.r().saturating_add(k), base.g().saturating_add(k), base.b().saturating_add(k), 255];
        let selected_body = selected_whole.contains(b.name.as_str());
        if !selected_body && !use_outlines {
            // A whole body the model draws from a plan is lit by redrawing the plan's own
            // evaluated triangles, not by building every triangle again (see `highlight_plan`).
            let lit = WholeBody { selected: false, colour: hover_col };
            if whole && gpu_lit(b) && light_whole_body(app, b, &active_bodies, lit, &body_slots, &mut feedback, &mut hover_instances) {
                // Drawn again from what the model already has on the GPU.
            } else {
                let lit = shown(b);
                let selected_faces = selected_faces.get(b.name.as_str());
                let hovered_faces: std::collections::HashSet<usize> = faces.iter().copied().collect();
                let keeps = |f: u32| {
                    let f = f as usize;
                    hovered_faces.contains(&f) && selected_faces.is_none_or(|s| !s.contains(&f))
                };
                mesh_tris(&mut feedback, &lit, hover_col, true, keeps);
            }
        }
        for (ei, e) in m.edges.iter().enumerate() {
            if m.edge_pickable(ei) && (whole || m.edge_faces.get(ei).is_some_and(|fs| fs.iter().any(|f| faces.contains(&(*f as usize))))) {
                edge_lines(&mut feedback, e, hover_edge, 1.5, &[]);
            }
        }
    }
    // Hover (pre-highlight).
    match hover {
        Some(Hit::Face { body, index, .. })
            if body_hover.as_ref() != Some(body)
                && !replaced(body)
                && !sel.iter().any(|x| matches!(x, Sel::Face { body: b, index: i, .. } if b == body && i == index)) =>
        {
            if let Some(b) = st.body(body) {
                let k = t.hover_face_lift;
                let c = body_rgb(app, body);
                let m = if outlines { shown(b) } else { drawn(b) };
                let patch = m.surface_patch(*index);
                if !(outlines && has_pickable_outline(&m)) {
                    let lit = shown(b);
                    for &f in &patch {
                        face_tris(&mut feedback, &lit, f, [c.0.saturating_add(k), c.1.saturating_add(k), c.2.saturating_add(k), 255], true);
                    }
                }
                for (e, _) in m.edge_faces.iter().enumerate().filter(|(_, fs)| fs.iter().any(|f| patch.contains(&(*f as usize)))) {
                    if let Some(p) = m.edges.get(e)
                        && m.edge_pickable(e)
                    {
                        edge_lines(&mut feedback, p, hover_edge, 1.5, &[]);
                    }
                }
            }
        }
        Some(Hit::Edge { body, index, mid, .. }) if !moving(body) => {
            if let Some(b) = st.body(body) {
                // Lit over the drawn edges (finer away from Normal display quality).
                let m = drawn(b);
                for i in m.circular_rim(*index) {
                    let selected = sel.iter().any(|x| matches!(x, Sel::Edge { body: b, index: j, .. } if b == body && *j == i));
                    if !selected && let Some(e) = if i == *index { edge_of(&m, i, *mid) } else { m.edges.get(i) } {
                        edge_lines(&mut feedback, e, hover_edge, 2.0, &edge_normals(&m, e));
                    }
                }
            }
        }
        // While a dimension's value is typed the sketch is drawn at it: the design's profile
        // would show over the resized one.
        Some(Hit::Profile { sketch, index })
            if !sel.iter().any(|x| matches!(x, Sel::Profile { sketch: s2, index: i2 } if s2 == sketch && i2 == index))
                && crate::dim_view::previewed_sketch(app) != Some(*sketch) =>
        {
            profile_fill(&mut feedback, app, *sketch, *index, t.hover_profile, t.hover_profile_edge);
        }
        _ => {}
    }
    feedback.hover_instanced = hover_instances.finish();
    feedback.selected_instanced = selected_instances.finish();
    // The feedback palette extends the scene's own: set it before sharing the geometry, and keep
    // the indices valid on the cache hit above.
    sc.palette = feedback.palette.clone();
    if projected {
        sc.tris.append(&mut feedback.tris);
        sc.trans.append(&mut feedback.trans);
        sc.lines.append(&mut feedback.lines);
        sc.hidden.append(&mut feedback.hidden);
        sc.overlay.append(&mut feedback.overlay);
        sc.hover_planned.append(&mut feedback.hover_planned);
        sc.selected_planned.append(&mut feedback.selected_planned);
        sc.hover_blocks.append(&mut feedback.hover_blocks);
        sc.selected_blocks.append(&mut feedback.selected_blocks);
        sc.recoloured.append(&mut feedback.recoloured);
        sc.hover_instanced.append(&mut feedback.hover_instanced);
        sc.selected_instanced.append(&mut feedback.selected_instanced);
    } else {
        // The feedback triangles become a block's canonical vertices, in one opaque list.
        let vertices = std::mem::take(&mut feedback.tris);
        let count = u32::try_from(vertices.len() / crate::gpu::TRI_SIZE).unwrap_or(0);
        let block = crate::gpu::SceneBlock {
            key,
            bytes: std::sync::Arc::new(crate::gpu::BlockBytes {
                vertices: std::sync::Arc::new(vertices),
                opaque: std::sync::Arc::new(std::iter::once(0..count).filter(|r| !r.is_empty()).collect()),
                lines: std::sync::Arc::new(std::mem::take(&mut feedback.lines)),
                hidden: std::sync::Arc::new(std::mem::take(&mut feedback.hidden)),
                ..Default::default()
            }),
        };
        let palette = feedback.palette.clone();
        let (hover_planned, selected_planned) = (feedback.hover_planned, feedback.selected_planned);
        let (hover_blocks, selected_blocks) = (feedback.hover_blocks, feedback.selected_blocks);
        let (hover_instanced, selected_instanced) = (feedback.hover_instanced, feedback.selected_instanced);
        let recoloured = feedback.recoloured;
        if let Ok(mut cache) = app.viewport.hover_block.lock() {
            *cache = Some(HoverBlock {
                key,
                state: st.clone(),
                doc: s.doc.clone(),
                palette,
                block: block.clone(),
                hover_planned: hover_planned.clone(),
                selected_planned: selected_planned.clone(),
                hover_blocks: hover_blocks.clone(),
                selected_blocks: selected_blocks.clone(),
                hover_instanced: hover_instanced.clone(),
                selected_instanced: selected_instanced.clone(),
                recoloured: recoloured.clone(),
            });
        }
        sc.blocks.push(block);
        sc.hover_planned = hover_planned;
        sc.selected_planned = selected_planned;
        sc.hover_blocks = hover_blocks;
        sc.selected_blocks = selected_blocks;
        sc.hover_instanced = hover_instanced;
        sc.selected_instanced = selected_instanced;
        sc.recoloured = recoloured;
    }

    if sketcraft_engine::geom::env::var_os("PROFILE_FRAMES").is_some() {
        let in_blocks = |f: fn(&crate::gpu::SceneBlock) -> usize| sc.blocks.iter().map(f).sum::<usize>();
        let tris = sc.tris.len() + in_blocks(|b| b.bytes.vertices.len());
        let lines = sc.lines.len() + in_blocks(|b| b.bytes.lines.len());
        let hidden = sc.hidden.len() + in_blocks(|b| b.bytes.hidden.len());
        eprintln!(
            "SketCraft highlight: {:.0} ms, hovered {} bodies, planned {} hover + {} selected, blocks {} hover + {} selected, instanced {} hover + {} selected, CPU triangles {tris} B, lines {lines} B, hidden {hidden} B",
            crate::now_ms() - began,
            hovered.len(),
            sc.hover_planned.len(),
            sc.selected_planned.len(),
            sc.hover_blocks.len(),
            sc.selected_blocks.len(),
            sc.hover_instanced.len(),
            sc.selected_instanced.len(),
        );
    }
    sc
}

/// A body's display colour (appearance, material or imported colour), if it has one.
fn body_color(doc: &sketcraft_engine::doc::Document, b: &sketcraft_engine::doc::ModelBody) -> Option<Color32> {
    let to8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    doc.body_color(b).map(|[r, g, b]| Color32::from_rgb(to8(r), to8(g), to8(b)))
}

/// A body's display colour (else the theme's) as RGB.
fn body_rgb(app: &SolveApp, name: &str) -> (u8, u8, u8) {
    let st = crate::open_meshing::shown_world(app);
    let c = st.body(name).and_then(|b| body_color(&app.session.doc, b)).unwrap_or(Tokens::get().body);
    (c.r(), c.g(), c.b())
}

/// The ray `o` + t `d` in the frame of a mesh that `place` (a rigid move) puts in the world, with
/// the same t along it; `None` for a move with no inverse.
pub(crate) fn local_ray(place: &sketcraft_engine::doc::Mat, o: Vec3, d: Vec3) -> Option<(Vec3, Vec3)> {
    if sketcraft_engine::doc::is_identity(place) {
        return Some((o, d));
    }
    let back = sketcraft_engine::doc::mat_inverse(place)?;
    Some((sketcraft_engine::doc::apply_point(&back, o), sketcraft_engine::doc::apply_vector(&back, d)))
}

pub(crate) fn polyline_mid(pts: &[Vec3]) -> Vec3 {
    let len: f64 = pts.windows(2).map(|w| w[0].dist(w[1])).sum();
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = w[0].dist(w[1]);
        if acc + l >= len * 0.5 && l > 0.0 {
            return w[0].lerp(w[1], (len * 0.5 - acc) / l);
        }
        acc += l;
    }
    pts.first().copied().unwrap_or_default()
}

pub(crate) fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

/// Point on the active sketch plane under the cursor (sketch coordinates), snapped to sketch
/// points within a few pixels and otherwise to a fine grid. Returns the snapped point and the
/// id of the sketch point it snapped to.
pub fn sketch_point_at(app: &SolveApp, proj: &Proj, pos: Pos2) -> Option<(Vec2, Option<String>)> {
    let sid = app.session.active_sketch?;
    let st = crate::open_meshing::shown_world(app);
    let ss = st.sketch(sid)?;
    for h in pick_for_tool(app, proj, pos) {
        if let Hit::SketchPoint { sketch, id, at } = h
            && sketch == sid
        {
            crate::sketch_tools::set_snap_hint("Pt", at);
            return Some((at, Some(id)));
        }
    }
    let (o, d) = proj.ray(pos);
    let w = ss.plane.intersect_ray(o, d)?;
    let lp = ss.plane.to_local(w);
    if crate::drag_snap::suppressed() {
        return Some((lp, None));
    }
    let (minor, _) = grid_step(app.cam.half_height());
    let step = minor / 10.0;
    if let Some(p) = crate::sketch_tools::refine_snap(app, proj, &ss.sketch, &ss.plane, lp) {
        return Some(p);
    }
    // With Snap off (Sketch Palette) the point is where the cursor is.
    if ss.sketch.view.no_snap {
        return Some((lp, None));
    }
    Some((Vec2::new((lp.x / step).round() * step, (lp.y / step).round() * step), None))
}

pub fn delete_selection(app: &mut SolveApp) {
    crate::delete::delete_selection(app);
}

/// Whether Primary should hide the live command preview so obscured geometry can be reselected.
///
/// Shared by [`show`] so tests can lock the same path: a manipulator drag never rolls the
/// preview back, even when Primary (preview-reselect) is held.
pub(crate) fn preview_should_roll_back(app: &SolveApp, held: crate::tool_modifiers::Held, manipulator_dragging: bool) -> bool {
    app.dialog.as_ref().is_some_and(|d| d.previews()) && app.preview.active && crate::tool_modifiers::preview_reselect(held, manipulator_dragging)
}

pub fn show(app: &mut SolveApp, ui: &mut egui::Ui) {
    crate::joint_handles::sync(app);
    let t = Tokens::get();
    let rect = ui.max_rect();
    app.viewport.rect = Some(rect);
    // A component dragged as a layer is drawn from the meshes where its drag began.
    let layered = drag_layer::follow(app);
    refresh_layout(app, ui.ctx());
    let resp = ui.interact(rect, ui.scope_id().with("viewport"), Sense::click_and_drag());
    let flat = crate::dialogs_sheet::flat_active(app);
    if !flat {
        app.flat_view.fitted = false;
    }
    let input =
        if flat { crate::dialogs_sheet::navigate(app, ui, rect, &resp) } else { crate::navigation::handle(app, ui, rect, &resp, NavCaps::MODEL) };
    let CameraInput { hover, scroll, mods, middle, secondary_down, primary_down, primary_pressed, inside, axes, pinch, pan, primary_nav, mode } =
        input;
    app.viewport.physical_scale = ui.ctx().pixels_per_point();
    // Suppress-snap chord and manipulator-free prefs; Shift midpoint targeting while sketching.
    // Preview reselect uses Primary but is skipped while a manipulator drag is under way.
    crate::drag_snap::set_suppressed(crate::prefs::suppress_sketch_snap_held(app, mods));
    crate::tool_modifiers::set_manipulator_free(crate::prefs::manipulator_free_held(app, mods));
    let manipulator_dragging = crate::tool_modifiers::manipulator_dragging();
    crate::tool_modifiers::set_manipulator_dragging(false);
    {
        let held = crate::tool_modifiers::Held::from_modifiers(mods);
        let mid = if let Some(cmd) = app.tool.as_ref().map(|t| t.cmd) {
            let live = crate::tool_modifiers::resolve(
                &[crate::tool_modifiers::Scope::SketchDraw, crate::tool_modifiers::Scope::SketchDrag],
                crate::tool_modifiers::descriptor(cmd),
                held,
                crate::tool_modifiers::Trigger::Live,
            );
            live.contains(crate::tool_modifiers::ModAction::TargetMidpoint)
        } else {
            false
        };
        crate::tool_modifiers::set_target_midpoint(mid);
        app.preview.rolled_back = preview_should_roll_back(app, held, manipulator_dragging);
    }
    let proj = projection(app, rect);
    if let Ok(mut cache) = app.viewport.display.lock() {
        cache.prepare(crate::open_meshing::shown_world(app), &proj, app.viewport.pixels_per_point());
    }
    // Suppress hover for the whole navigation gesture, including frames between pointer
    // events. Testing delta made the highlight reappear whenever a held drag paused.
    // Resolve it before rendering so the GPU scene and screen overlays use the same hit.
    // A 3D mouse moving the view is not such a gesture: the pointer hand is free, and hover
    // and clicks keep picking while the other hand turns the view. Nor is a left press held
    // still under a mode lit in the navigation bar or a held Space: they own drags, and the
    // press may be opening Select Other (#8864); once it drags, `mode` says so. A preset's
    // left-button binding or Ctrl+left orbit navigates from the press on.
    let navigating = scroll != 0.0
        || pinch != 1.0
        || pan != egui::Vec2::ZERO
        || middle
        || secondary_down
        || (app.viewport.primary_nav.is_some() && primary_down)
        || mode.is_some()
        || app.viewport.axis_drag.is_some()
        || app.viewport.spin
        || app.cam_anim.is_some();
    let joint_handle_pointer = hover.is_some_and(|p| inside && crate::joint_handles::owns_pointer(app, &proj, p));
    // Nor while a component's triad drags it as a layer, unless the preference asks for the
    // highlight (`drag_layer::holds_pointer`). A part dragged along its joint owns the pointer
    // too: nothing under it lights up (#5137).
    let dragging_layer = drag_layer::holds_pointer(app, layered, manipulator_dragging);
    let hits = hover
        .filter(|_| {
            inside && app.viewport.boxsel.is_none() && !crate::joint_drag::active(app) && !joint_handle_pointer && !navigating && !dragging_layer
        })
        .map(|p| pick_cached(app, &proj, p))
        .unwrap_or_default();
    // The Fillet and Chamfer window resolves its own corners: the nearest sketch point to a
    // corner drawn from the origin is the fixed origin, which ends no two curves.
    let corner = hover.filter(|_| inside && !navigating).and_then(|p| crate::sketch_modify_ui::corner_hit(app, &proj, p));
    let cand = if app.tool.is_some() {
        tool_hover(app, &hits).map(|h| (h, None))
    } else if let Some((hit, sel)) = corner {
        Some((hit, Some(sel)))
    } else {
        candidate(app, &hits).map(|(h, s)| (h, Some(s)))
    };
    app.viewport.hover = cand.as_ref().map(|c| c.0.clone());
    wake_centre(app, &hits);
    // Select Other: a press held still opens the list of everything under it; while the list is
    // open its row under the pointer is what lights up. A mode lit in the navigation bar or a
    // held Space owns drags, not a still press, so only a navigation that has started cancels
    // it. A preset's modifier binding (Maya Alt, Alias Shift+Alt) or Ctrl+left orbit is a
    // gesture to navigate from the press on, so it still cancels the hold at once.
    let navigating_from_press = mode.is_some() || app.viewport.primary_nav.is_some() || joint_handle_pointer;
    let held = crate::select_other::track_hold(app, ui.ctx(), &proj, inside, primary_pressed, primary_down, hover, navigating_from_press);
    crate::select_other::hover_override(app);
    list_hover(app);
    let painter = ui.painter_at(rect);
    crate::gpu::paint_background(&painter, rect, &t);
    crate::sketchup_screen::paint_sky(app, &painter, rect, &proj);
    crate::gpu::paint_background_picture(app, &painter, rect);

    // ---- model ----
    crate::frame_watch::step(app, "viewport before the model");
    // A drag drawn as a layer keeps the refinements it began with.
    if !layered {
        update_refinement(app, ui.ctx(), &proj);
    }
    // Levels of detail being made: keep frames coming to draw them when they are.
    if app.viewport.mesh_lods.busy() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    crate::frame_watch::step(app, "display refinement");
    let key = scene_key(app);
    let t0 = crate::now_ms();
    // Drawn on the GPU; without one (the headless harness, tests) the model is not drawn.
    if app.viewport.gpu.is_some() {
        let moving = if layered { drag_layer::scenes(app) } else { None };
        if !layered && key != app.viewport.key {
            let sc = build_scene(app);
            if let Ok(mut slot) = app.viewport.slot.lock() {
                *slot = Some(sc);
            }
            drag_layer::whole_scene_built(app);
            app.viewport.key = key;
            app.viewport.build_ms = crate::now_ms() - t0;
            crate::frame_watch::step(app, "model scene");
        }
        refresh_sketch_scene(app, &proj);
        let hk = highlight_key(app);
        if hk != app.viewport.hl_key {
            let hl = build_highlight(app);
            if let Ok(mut slot) = app.viewport.hl_slot.lock() {
                *slot = Some(hl);
            }
            app.viewport.hl_key = hk;
        }
        crate::frame_watch::step(app, "highlight scene");
        let ppp = ui.ctx().pixels_per_point();
        let size_px = [rect.width() * ppp, rect.height() * ppp];
        let ssaa = supersampling(app, ui.ctx(), proj.vp.to_f32(), size_px);
        let ao_samples = ao_samples(app, ui.ctx(), proj.vp.to_f32(), size_px);
        let cb = ViewportCallback(Frame {
            product_lights: None,
            ssaa,
            ao_samples,
            product: false,
            upload_error: None,
            key: app.viewport.key,
            slot: app.viewport.slot.clone(),
            sketch_key: app.viewport.sketch_key,
            sketch_slot: app.viewport.sketch_slot.clone(),
            hl_key: app.viewport.hl_key,
            hl_slot: app.viewport.hl_slot.clone(),
            pv_key: app.preview.key,
            pv_slot: app.preview.slot.clone(),
            pv_hide: app.preview.rolled_back,
            preview_bounds: (app.preview.active && !app.preview.rolled_back).then_some(app.preview.bounds),
            clip: section_clip(app),
            cap: {
                let [r, g, b, _] = t.section_cap.to_normalized_gamma_f32();
                [r, g, b, if section_hatch(app) { 1.0 } else { 0.0 }]
            },
            view_proj: proj.vp.to_f32(),
            back: proj.cam.back().to_f32(),
            size_px,
            images: crate::ref_images::gpu_images(app),
            clouds: crate::point_clouds::gpu_clouds(app, ui.ctx()),
            analysis: analysis_uniform(app),
            analysis_colors: crate::gpu::analysis_colors(&t.analysis),
            access: crate::access::map(app),
            outline: matches!(app.ui.visual_style, 0 | 3),
            ao_radius: ao_radius(app),
            eye: {
                let e = proj.cam.eye().to_f32();
                [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }]
            },
            // Whole-body highlights redraw the model's own plan triangles: the hover lifts each
            // body's colour the same amount `mesh_tris` used to, and a selection replaces it.
            hover_tint: [f32::from(t.hover_face_lift) / 255.0; 3],
            selection_tint: {
                let c = t.body_selected;
                [f32::from(c.r()) / 255.0, f32::from(c.g()) / 255.0, f32::from(c.b()) / 255.0]
            },
            // The section hatch colours by part: the cap's, then the theme's others.
            hatch: std::array::from_fn(|k| {
                let c = if k == 0 { t.section_cap } else { t.section_hatch.get(k - 1).copied().unwrap_or(t.section_cap) };
                let [r, g, b, _] = c.to_normalized_gamma_f32();
                [r, g, b]
            }),
            back_face: crate::sketchup_screen::back_face_uniform(app),
            sky: sky_uniform(app.preferences.dark_parts_shading, proj.cam.up),
            moving,
        });
        crate::blur::viewport(&painter, rect, cb, rect, [t.viewport_top, t.viewport_bottom]);
        // The graphics card had no memory for the view's frame (#7296): say once why it looks
        // coarser.
        if !app.viewport.lowered_noted && app.viewport.draw_stats.as_ref().is_some_and(|s| s.lock().is_ok_and(|s| s.lowered)) {
            app.viewport.lowered_noted = true;
            app.set_status(app.tr("ui.gpu_memory_lowered"), false);
        }
    }
    let overlay_rect = crate::shell::bottom_overlay_rect(app, rect);
    crate::viewport_details::legacy_legends(app, ui, overlay_rect);
    placement_note(app, &painter, overlay_rect);
    crate::frame_rate::paint(app, ui, &painter, overlay_rect);

    // ---- interaction ----
    // A tool's click that moved on the way (#2448) clicks where the button is let go.
    if app.viewport.tool_click && resp.drag_stopped() {
        app.viewport.tool_click = false;
        if let Some(p) = ui.input(|i| i.pointer.latest_pos()).filter(|p| rect.contains(*p))
            && app.tool.is_some()
        {
            crate::tools::on_hover(app, &proj, p, mods.shift);
            crate::tools::on_click(app, &proj, p, mods.shift);
        }
    }
    // A two-point shape being dragged out (#1630) follows the pointer and ends where it is let go,
    // even over the shape's own dimension box, which rides along beside the pointer.
    app.viewport.draw_drag_handled = false;
    if let Some(from) = app.viewport.draw_drag {
        let at = ui.input(|i| i.pointer.latest_pos()).filter(|p| rect.contains(*p) && !crate::shell::covered(app, *p));
        if resp.drag_stopped() || !primary_down {
            app.viewport.draw_drag = None;
            if let Some(p) = at
                && app.tool.as_ref().is_some_and(|t| t.pts.len() == 1)
            {
                crate::tools::on_hover(app, &proj, p, mods.shift);
                let release = sketch_point_at(app, &proj, p);
                let first = app.tool.as_ref().and_then(|t| t.pts.first().map(|pt| pt.0));
                // Same sketch point as the press (a second circle on an existing centre, #8899),
                // or let go close to the press (#2448): keep a single first point and its value
                // boxes. Clearing and re-clicking used to wipe the Diameter box on release.
                let keep_first = p.distance(from) < DRAW_DRAG_MIN || matches!((first, release.as_ref()), (Some(a), Some((b, _))) if a == *b);
                if keep_first {
                    if let Some(pt) = release
                        && let Some(t) = app.tool.as_mut()
                    {
                        t.pts = vec![pt];
                        crate::sketch_dims::keep_stage_one(t);
                    }
                } else {
                    crate::tools::on_click(app, &proj, p, mods.shift);
                }
                // A line dragged out is that one line: no chain goes on from its end (#7247).
                if p.distance(from) >= DRAW_DRAG_MIN
                    && let Some(t) = app.tool.as_mut().filter(|t| t.cmd == "sketch.line")
                {
                    t.pts.clear();
                    crate::sketch_dims::reset(t);
                }
                app.viewport.draw_drag_handled = true;
            }
        } else if let Some(p) = at {
            crate::tools::on_hover(app, &proj, p, mods.shift);
        }
    }
    // A Trim drag's path follows the pointer; letting go trims what it crossed. A path that
    // crossed nothing was a click that moved on the way (#2448): the release clicks.
    if app.viewport.trim_drag {
        let at = ui.input(|i| i.pointer.latest_pos()).filter(|p| rect.contains(*p));
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            app.viewport.trim_drag = false;
            if !crate::sketch_tools::trim_drag_end(app)
                && let Some(p) = at
                && app.tool.is_some()
            {
                crate::tools::on_hover(app, &proj, p, mods.shift);
                crate::tools::on_click(app, &proj, p, mods.shift);
            }
        } else if let Some(p) = at {
            crate::sketch_tools::trim_drag_to(app, &proj, p);
        }
    }
    // Selection chords via resolve (add-only Shift is a preference; default both toggle).
    let held_mods = crate::tool_modifiers::Held::from_modifiers(mods);
    let (add, allow_toggle) = crate::tool_modifiers::selection_click(held_mods, app.preferences.selection_shift_add_only);
    crate::sketch_text::set_one_letter(mods.shift);
    // Resting on the Line tool's last point switches lines and tangent arcs (#9489): only a free
    // pointer, nothing held or under way. Not `inside`: on that point the pointer is between the
    // new segment's value boxes, which take it from the view (as a shape being dragged out does).
    let free_pointer = ui.is_enabled()
        && !primary_down
        && !secondary_down
        && !middle
        && !held
        && primary_nav.is_none()
        && mode.is_none()
        && mods.is_none()
        && !app.viewport.tool_click
        && app.viewport.draw_drag.is_none();
    let resting = hover.filter(|p| free_pointer && rect.contains(*p) && !axes.contains(*p) && !crate::shell::covered(app, *p));
    crate::sketch_line_arc::follow_pointer(app, ui.ctx(), &proj, resting);
    // #10600: a running tool's own floating value box (or its construction/normal switch) must
    // not swallow a click that lands on a sketch point: the click places the point there, and
    // the box keeps the keyboard for typed values. A click on the box away from any sketch point
    // still edits the box. The box is anchored to the tool's hover, which freezes while the
    // pointer rests on the box, so a point the pointer reaches from the box's side can sit under
    // it; the snap identifies the intended point even there.
    if !inside
        && app.tool.is_some()
        && !held
        && primary_nav.is_none()
        && mode.is_none()
        && ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
        && let Some(p) = hover.filter(|p| rect.contains(*p))
        && crate::sketch_dims::over_tool_boxes(app, ui.ctx(), p)
        && sketch_point_at(app, &proj, p).is_some_and(|(_, id)| id.is_some())
    {
        crate::tools::on_hover(app, &proj, p, mods.shift);
        crate::tools::on_click(app, &proj, p, mods.shift);
    }
    if let Some(p) = hover.filter(|_| inside) {
        if app.tool.is_some() {
            crate::tools::on_hover(app, &proj, p, mods.shift);
        }
        // Trim also trims by press and drag across the pieces to remove (its own release handling,
        // so not a click that moved on the way below).
        if app.tool.as_ref().is_some_and(|t| t.cmd == "sketch.trim")
            && !held
            && primary_nav.is_none()
            && mode.is_none()
            && resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(a) = ui.input(|i| i.pointer.press_origin())
        {
            app.viewport.trim_drag = crate::sketch_tools::trim_drag_start(app, &proj, a);
            if app.viewport.trim_drag {
                crate::sketch_tools::trim_drag_to(app, &proj, p);
            }
        }
        // Rectangles, circles and polygons also draw by press, drag and release (#1630): the press
        // places the first point as a click there would, the release the second. So does a line
        // with Preferences › Sketch › "Press and drag draws one line" (#7247).
        let line_drag = app.preferences.line_drag_draws_one;
        let two_point_start = app
            .tool
            .as_ref()
            .is_some_and(|t| t.pts.is_empty() && (matches!(t.kind, crate::tools::Kind::Draw(2)) || (line_drag && t.cmd == "sketch.line")));
        // A SketchUp-tab tool still waiting for its first click: the press picks the plane and
        // places the first point; a shape that draws by dragging then follows the pointer.
        if crate::sketchup_draw::pending(app).is_some()
            && !held
            && primary_nav.is_none()
            && mode.is_none()
            && resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(a) = ui.input(|i| i.pointer.press_origin())
        {
            crate::sketchup_draw::click(app, &proj, a, &hits, mods.shift);
            let drags = app.tool.as_ref().is_some_and(|t| {
                t.pts.len() == 1 && (matches!(t.kind, crate::tools::Kind::Draw(2)) || (line_drag && t.cmd == "sketch.line"))
            });
            app.viewport.draw_drag = drags.then_some(a);
        } else if two_point_start
            && !held
            && primary_nav.is_none()
            && mode.is_none()
            && resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(a) = ui.input(|i| i.pointer.press_origin())
        {
            crate::tools::on_click(app, &proj, a, mods.shift);
            app.viewport.draw_drag = app.tool.as_ref().is_some_and(|t| t.pts.len() == 1).then_some(a);
        } else if app.tool.is_some()
            && !app.viewport.trim_drag
            && !held
            && primary_nav.is_none()
            && mode.is_none()
            && resp.drag_started_by(egui::PointerButton::Primary)
        {
            // Any other press of a tool that turns into a drag is a click that moved on the way
            // (#2448): its release clicks.
            app.viewport.tool_click = true;
        }
        // While acquiring a midpoint dimension's second entity, geometry takes precedence
        // over existing dimension lines ending on it. Dimension text can still supply a value.
        let midpoint_geometry = crate::dim_view::live() == Some(false)
            && app.tool.as_ref().is_some_and(|t| t.kind == crate::tools::Kind::Dimension && t.picks.len() == 1 && !t.point_picks.is_empty())
            && matches!(cand.as_ref().map(|c| &c.0), Some(Hit::SketchPoint { .. } | Hit::SketchCurve { .. }))
            && crate::dim_view::hit_text(p).is_none();
        let reference_pick = (resp.clicked() || resp.double_clicked())
            && !midpoint_geometry
            && !held
            && app.dialog.is_none()
            && crate::dim_view::pick_reference(app, ui.ctx(), p, resp.double_clicked());
        // Capture the press too: text editors lose focus before the click is released. A
        // running tool keeps the keyboard for its own typed values, and so does a pick field
        // clicked into until a click picks something (#10428).
        if !held
            && app.tool.is_none()
            && (resp.clicked() || (resp.contains_pointer() && ui.input(|i| i.pointer.primary_pressed())))
            && let Some(dialog) = app.dialog.as_mut().filter(|d| !d.awaiting_pick)
        {
            dialog.restore_value_focus = true;
        }
        if resp.clicked() && !held && !reference_pick && !joint_handle_pointer && !app.viewport.draw_drag_handled {
            if crate::browser_canvas::click(app, &proj, p) {
                // Calibrate Canvas: the click sets Point A or B on the image (#9108).
            } else if crate::sketchup_draw::pending(app).is_some() {
                // A SketchUp-tab drawing tool: this click picks its plane and is its first point.
                crate::sketchup_draw::click(app, &proj, p, &hits, mods.shift);
            } else if app.tool.is_some() {
                crate::tools::on_click(app, &proj, p, mods.shift);
            } else if app.dialog.as_ref().is_some_and(|d| d.wants_picks()) {
                if let Some((hit, Some(sel))) = cand {
                    dialog_pick(app, &hit, sel);
                } else if let Some(mut dialog) = app.dialog.take() {
                    // A pick field clicked into says what it is for when the click is not one (#10428).
                    if dialog.awaiting_pick {
                        dialog.pick_refused = dialog.active_input().and_then(|i| i.tip);
                    }
                    // A sketch clicked for a pattern's or Mirror's objects: say why it is not taken (#10057).
                    let sketch = hits.iter().find_map(|h| match h {
                        Hit::SketchCurve { sketch, .. } | Hit::SketchPoint { sketch, .. } | Hit::Profile { sketch, .. } => Some(*sketch),
                        _ => None,
                    });
                    if let Some(sketch) = sketch {
                        dialog.refuse_sketch(&app.session, sketch);
                    }
                    let state = crate::open_meshing::shown_world(app);
                    if let Some(sketch) = app.session.active_sketch.and_then(|id| state.sketch(id)) {
                        let (origin, direction) = proj.ray(p);
                        if let Some(point) = sketch.plane.intersect_ray(origin, direction) {
                            crate::sketch_modify_ui::pick_position(&mut dialog, &app.session, point);
                        }
                    }
                    app.dialog = Some(dialog);
                }
            } else {
                let annotation = sketch_constraint_at(app, p, cand.as_ref().map(|c| &c.0));
                if crate::sketch_text_handles::over(app, &proj, p) {
                    // A text's resize or rotation handle: the text stays selected.
                } else if !crate::sketch3d::click(app, &proj, p) {
                    match annotation {
                        Some(sel) => select(app, Some(sel), add, allow_toggle),
                        // Clicking again on the same spot selects the next thing under it (not
                        // the second click of a double-click, which has its own actions).
                        None if !add && !resp.double_clicked() && crate::select_other::cycle_click(app, &proj, p) => {}
                        None => {
                            let sel = cand.and_then(|c| c.1);
                            select(app, sel.clone(), add, allow_toggle);
                            crate::select_other::note_click(app, p, if add { None } else { sel });
                        }
                    }
                }
            }
        }
        // Double-click a dimension: edit its value in place (a finished sketch's shown dimension
        // too, without editing the sketch).
        let dim_edit = resp.double_clicked()
            && !held
            && !crate::dim_view::editing()
            && app.tool.is_none()
            && app.dialog.is_none()
            && (crate::sketch_modify_ui::double_click_pattern(app, p)
                || crate::dim_view::double_click(app, p)
                || crate::dim_view::model_double_click(app, p));
        // Double-click a text (an outline or inside a letter; the first click selected it): edit it.
        if resp.double_clicked()
            && !held
            && !dim_edit
            && app.tool.is_none()
            && app.dialog.is_none()
            && let Some(entity) = crate::sketch_text::selected_entity(app)
        {
            crate::sketch_text::open(app, &entity);
        }
        // Double-click a sketch curve: select the chain joined to it end to end.
        if resp.double_clicked()
            && !held
            && !dim_edit
            && app.tool.is_none()
            && app.dialog.is_none()
            && let Some(Hit::SketchCurve { sketch, id, .. }) = app.viewport.hover.clone()
            && app.session.active_sketch == Some(sketch)
            && let Ok(v) = app.session.execute("sketch.chain", &json!({ "curve": id }))
        {
            let items: Vec<Sel> = v["curves"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str())
                .map(|id| Sel::SketchCurve { id: id.into(), sketch: None })
                .collect();
            let _ = app.run("select.set", json!({ "items": items, "add": add }));
        }
        // Double-click a curve while a Mirror/Move/Copy/Scale/pattern Entities input is open:
        // add the whole connected chain to that input (#8390).
        if resp.double_clicked() && !held && !dim_edit && app.tool.is_none() && app.dialog.is_some() {
            let _ = crate::sketch_modify_ui::double_click_entities(app);
        }
        // Double-click an edge: select the edges that continue it smoothly (the loop).
        if resp.double_clicked()
            && !held
            && !dim_edit
            && app.tool.is_none()
            && app.dialog.is_none()
            && let Some(Hit::Edge { body, index, .. }) = app.viewport.hover.clone()
            && let Some(b) = crate::open_meshing::shown_world(app).body(&body).cloned()
        {
            let m = b.mesh();
            let items: Vec<Sel> = m
                .tangent_chain(index, 2f64.to_radians())
                .into_iter()
                .filter_map(|i| m.edges.get(i).map(|e| Sel::Edge { body: body.clone(), index: i, point: polyline_mid(e) }))
                .collect();
            let _ = app.run("select.set", json!({ "items": items, "add": add }));
        }
        // Double-click a component's face: edit a linked one in place, activate any other.
        // Double-clicking empty space exits to the root component.
        if resp.double_clicked()
            && !held
            && !dim_edit
            && mods.is_none()
            && app.tool.is_none()
            && app.dialog.is_none()
            && app.session.active_sketch.is_none()
        {
            if let Some(Hit::Face { body, .. }) = app.viewport.hover.clone() {
                crate::edit_in_place::double_click_face(app, &body);
            } else if app.session.active_component != 0 {
                let _ = app.run("component.activate", json!({ "component": 0 }));
            }
        }
        // A right-click, unless the same press opened the mouse gestures' ring. egui's click
        // already allows the pointer a few points of travel, as a mouse moves while it is clicked.
        if resp.secondary_clicked() && !crate::mouse_gesture::took_press(app) {
            if app.tool.is_some() {
                crate::tools::finish(app);
            } else if app.dialog.is_none() {
                crate::context_menu::open(app, p, held_mods);
            }
        }
    }
    // Dragging a sketch point of the active sketch moves it; the solver keeps the constraints.
    if app.tool.is_none() && app.dialog.is_none() && primary_nav.is_none() && mode.is_none() && app.session.active_sketch.is_some() && !held {
        let on_plane = |app: &SolveApp, p: Pos2| {
            let ss = app.session.active_sketch.and_then(|sid| crate::open_meshing::shown_world(app).sketch(sid).cloned())?;
            let (o, d) = proj.ray(p);
            ss.plane.intersect_ray(o, d).map(|w| ss.plane.to_local(w))
        };
        // What a drag starts on: the 3D triad, then a dimension's text, then sketch geometry
        // (before the dimension's lines, which end on the geometry they measure), as for clicks.
        let grabbed = resp
            .drag_started_by(egui::PointerButton::Primary)
            .then(|| ui.input(|i| i.pointer.press_origin()))
            .flatten()
            .filter(|a| !crate::sketch_text_handles::drag_start(app, &proj, *a) && !crate::sketch3d::drag_start(app, &proj, *a))
            .map(|a| (a, pick(app, &proj, a).into_iter().next()));
        if let Some((a, hit)) = &grabbed
            && !crate::sketch_text_handles::dragging(app)
            && let Some(q) = on_plane(app, *a)
        {
            let on_geometry = matches!(hit, Some(Hit::SketchPoint { .. } | Hit::SketchMidpoint { .. } | Hit::SketchCurve { .. }));
            crate::dim_view::drag_start(app, *a, q, on_geometry);
        }
        if crate::sketch3d::dragging()
            && (resp.dragged_by(egui::PointerButton::Primary) || resp.drag_stopped())
            && let Some(p) = hover
        {
            crate::sketch3d::drag_to(app, &proj, p);
        }
        if crate::sketch_text_handles::dragging(app)
            && (resp.dragged_by(egui::PointerButton::Primary) || resp.drag_stopped())
            && let Some(p) = hover
        {
            crate::sketch_text_handles::drag_to(app, &proj, p, mods.shift);
        }
        if crate::dim_view::dragging()
            && (resp.dragged_by(egui::PointerButton::Primary) || resp.drag_stopped())
            && let Some(q) = hover.and_then(|p| on_plane(app, p))
        {
            crate::dim_view::drag_to(app, q);
        }
        // A point or a curve of the sketch being edited follows a drag that starts on it.
        if let Some((a, hit)) = grabbed
            && !crate::dim_view::dragging()
            && !crate::sketch3d::dragging()
            && !crate::sketch_text_handles::dragging(app)
        {
            let active = app.session.active_sketch;
            // Off the sketch's own geometry, a drag from inside the selected text's frame moves
            // the text, also from the space between its letters (#8835).
            let own = matches!(&hit, Some(Hit::SketchPoint { sketch, .. } | Hit::SketchMidpoint { sketch, .. } | Hit::SketchCurve { sketch, .. }) if Some(*sketch) == active);
            let hit = if own { hit } else { crate::sketch_text_handles::grab_inside(app, &proj, a).or(hit) };
            let hit = hit.map(|hit| point_at_midpoint(app, hit));
            match hit {
                Some(Hit::SketchPoint { sketch, id, .. }) if Some(sketch) == active => {
                    // A point dragged with the rest of a selection slides with it, from where it
                    // was grabbed, rather than going to the cursor.
                    let grab = if dragged_with(app, &id).is_empty() { None } else { on_plane(app, a) };
                    app.viewport.point_drag = Some((id, app.session.undo.len(), grab))
                }
                Some(Hit::SketchCurve { sketch, id, .. } | Hit::SketchMidpoint { sketch, id, .. }) if Some(sketch) == active => {
                    app.viewport.point_drag = on_plane(app, a).map(|q| (id, app.session.undo.len(), Some(q)));
                }
                _ => {}
            }
            if let Some((id, _, _)) = &app.viewport.point_drag {
                let link = app.session.active_sketch.and_then(|sid| app.session.doc.sketch(sid).ok()).and_then(|sk| {
                    let link = sk
                        .curve_index(id)
                        .and_then(|i| sk.curves.get(i))
                        .and_then(|c| c.link.as_deref())
                        .or_else(|| sk.resolve_point(id).and_then(|i| sk.points.get(i)).and_then(|p| p.link.as_deref()))?;
                    sk.link(link).filter(|l| l.kind == sketcraft_engine::sketch::LinkKind::Text).map(|l| l.id.clone())
                });
                if let Some(link) = link
                    && let Some(q) = on_plane(app, a)
                {
                    if let Some((_, _, grab)) = &mut app.viewport.point_drag {
                        *grab = Some(q);
                    }
                    if let Ok(result) = app.session.execute("edit.checkpoint", &json!({}))
                        && let Some(checkpoint) = result["checkpoint"].as_u64()
                    {
                        app.viewport.text_drag = Some((checkpoint, link));
                    }
                }
            }
        }
        if let Some((id, depth, grab)) = app.viewport.point_drag.clone()
            && (resp.dragged_by(egui::PointerButton::Primary) || resp.drag_stopped())
            && let Some(mut q) = hover.and_then(|p| on_plane(app, p))
        {
            let from = grab.unwrap_or(q);
            let with = dragged_with(app, &id);
            // Snap the dragged point (or the line's ends) to other geometry, like drawing.
            let snap = {
                let st = crate::open_meshing::shown_world(app);
                app.session.active_sketch.and_then(|s| st.sketch(s)).and_then(|ss| {
                    let sk = &ss.sketch;
                    let (own, exclude) = crate::drag_snap::moving_points(sk, std::iter::once(id.as_str()).chain(with.iter().map(String::as_str)));
                    if own.iter().any(|&p| sk.point_locked(p)) {
                        return crate::drag_snap::find(sk, &[], &[], 0.0);
                    }
                    let a = proj.to_screen(ss.plane.to_world(q))?;
                    let b = proj.to_screen(ss.plane.to_world(q + Vec2::X))?;
                    let mm_px = 1.0 / (a.distance(b) as f64).max(1e-9);
                    let d = q - from;
                    let moving: Vec<(usize, Vec2)> =
                        own.iter().filter_map(|&p| if grab.is_some() { sk.point(p).map(|x| (p, x + d)) } else { Some((p, q)) }).collect();
                    crate::drag_snap::find(sk, &moving, &exclude, mm_px)
                })
            };
            if let Some(s) = snap {
                q += s.offset;
            }
            // Each frame re-solves the sketch only (`live`); the model follows when the drag ends.
            let params = json!({"entity": id, "with": with, "from": [from.x, from.y], "to": [q.x, q.y], "live": true});
            let at = match app.session.execute("sketch.drag", &params) {
                Ok(r) => {
                    app.session.undo.truncate(depth + 1);
                    // Budget-limited progress continues even when the mouse stops. A stalled
                    // search asks for no more frames; release gets one bounded final attempt.
                    if r.get("retry").and_then(serde_json::Value::as_bool) == Some(true) && !resp.drag_stopped() {
                        ui.ctx().request_repaint();
                    }
                    r.get("at").and_then(sketcraft_engine::params::vec2).unwrap_or(q)
                }
                Err(_) => from,
            };
            // The grab stays on the geometry: what it could not follow this frame it catches up
            // with on the next one instead of lagging the cursor for the rest of the drag.
            if grab.is_some() {
                app.viewport.point_drag = Some((id, depth, Some(at)));
            }
        }
        if resp.drag_stopped() {
            crate::sketch_text_handles::drag_end(app);
            if let Some((_, depth, _)) = app.viewport.point_drag.clone() {
                // Letting go on a point, midpoint or curve adds the matching constraint, then the
                // whole model follows the drag, all in the drag's undo step.
                let _ = app.session.execute("sketch.drag_end", &json!({ "constraint": crate::drag_snap::release() }));
                app.session.undo.truncate(depth + 1);
            }
            crate::drag_snap::clear();
            app.viewport.point_drag = None;
            crate::sketch_text_move::end_drag(app);
            crate::dim_view::drag_end();
            crate::sketch3d::drag_end();
        }
    }
    // A sketch drag whose release never came (focus lost, a tool or dialog took over) ends here,
    // so the model does not stay behind its sketch until the next command. Not while CAD input
    // is held back (an issue report's capture): a release then belongs to the report, and the
    // drag continues afterwards.
    if app.viewport.point_drag.is_some() && !crate::issue_report::cad_input_blocked() && !ui.input(|i| i.pointer.primary_down()) {
        app.viewport.point_drag = None;
        crate::drag_snap::clear();
        let _ = app.session.execute("sketch.drag_end", &json!({}));
        crate::sketch_text_move::end_drag(app);
    }
    if crate::sketch_text_handles::dragging(app) && !crate::issue_report::cad_input_blocked() && !ui.input(|i| i.pointer.primary_down()) {
        crate::sketch_text_handles::drag_end(app);
    }
    // Dragging a part moves its free chain (outside sketches), or reaches within its joints
    // when the chain is anchored. Shift, Ctrl/Cmd or Alt held as the drag starts
    // draws a selection box instead, as a drag from empty space does.
    if app.tool.is_none() && app.dialog.is_none() && primary_nav.is_none() && mode.is_none() && app.session.active_sketch.is_none() && !held {
        if resp.drag_started_by(egui::PointerButton::Primary)
            // Physical Ctrl on macOS is not the primary click-selection modifier, but must
            // still keep a modified drag from moving a part.
            && !(mods.shift || mods.command || mods.ctrl || mods.alt)
            && let Some(a) = ui.input(|i| i.pointer.press_origin())
            && rect.contains(a)
            && !crate::shell::covered(app, a)
            && !axes.contains(a)
            && !crate::joint_handles::start_drag(app, &proj, a)
            && !crate::occurrence_drag::start(app, &proj, a)
            && !crate::joint_drag::start(app, &proj, a)
        {
            crate::drag_extrude::press(app, &proj, a);
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(p) = hover
        {
            if crate::joint_handles::active(app) {
                // Captured handle motion is handled below, including the release frame.
            } else if crate::joint_drag::active(app) {
                crate::joint_drag::to(app, &proj, p);
            } else if crate::occurrence_drag::active(app) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                crate::occurrence_drag::to(app, &proj, p);
            }
        }
    }
    if crate::joint_handles::active(app) && !crate::issue_report::cad_input_blocked() {
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()).filter(|p| p.is_finite()) {
            crate::joint_handles::to(app, &proj, p);
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            crate::joint_handles::end_drag(app);
        }
    }
    if crate::joint_drag::active(app) && (resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down())) {
        crate::joint_drag::end(app);
    }
    // A drag whose release never came (focus lost, a tool or dialog took over) ends where it is.
    if crate::occurrence_drag::active(app) && (resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down())) {
        crate::occurrence_drag::end(app);
    }
    // Pulling a face or profile extrudes it; the release applies.
    if resp.dragged_by(egui::PointerButton::Primary)
        && let Some(p) = hover
    {
        crate::drag_extrude::to(app, &proj, p);
    }
    if resp.drag_stopped() || !primary_down {
        crate::drag_extrude::end(app);
    }
    // Box selection: a primary drag on the model when no navigation mode is on.
    if app.tool.is_none()
        && !held
        && primary_nav.is_none()
        && mode.is_none()
        && app.viewport.point_drag.is_none()
        && !crate::joint_drag::active(app)
        && !crate::joint_handles::active(app)
        && !joint_handle_pointer
        && !crate::occurrence_drag::active(app)
        && !crate::drag_extrude::started(app)
        && !crate::dim_view::dragging()
        && !crate::sketch3d::dragging()
        && !crate::sketch_text_handles::dragging(app)
    {
        let origin = ui.input(|i| i.pointer.press_origin());
        if resp.dragged_by(egui::PointerButton::Primary)
            && let (Some(a), Some(b)) = (origin, hover)
            && rect.contains(a)
            && !crate::shell::covered(app, a)
            && !axes.contains(a)
        {
            // The lasso (#4376) follows the pointer from the press; the rectangle spans it.
            match &mut app.viewport.boxsel {
                Some(lasso) if app.ui.lasso_select && lasso.is_lasso() => lasso.extend(b),
                _ if app.ui.lasso_select => app.viewport.boxsel = Some(BoxSel::lasso(&[a, b])),
                _ => app.viewport.boxsel = Some(BoxSel::from_drag(a, b)),
            }
        }
        // A shell card disables its underlying widgets. The release still ends a
        // rubber band even when egui no longer sends drag_stopped to the viewport.
        if !primary_down && !inside {
            app.viewport.boxsel = None;
        }
        if resp.drag_stopped()
            && let Some(bx) = app.viewport.boxsel.take()
        {
            box_select(app, &proj, bx, add);
        }
    } else {
        app.viewport.boxsel = None;
    }
    crate::preview::paint_sketches(app, &painter, &proj);
    crate::ref_images::show(app, ui, &painter, &proj);
    crate::browser_canvas::paint_marks(app, &painter, &proj);
    crate::point_clouds::show(app, &painter, &proj);
    crate::dialogs_assembly::origins(app, &painter, &proj);
    if !crate::joint_handles::visible(app) {
        crate::dialogs_assembly::joint_glyphs(app, &painter, &proj);
    }
    crate::joint_handles::show(app, ui, &painter, &proj);
    overlays(app, &painter, &proj);
    paint_device_pivot(app, &painter, &proj);
    crate::dim_view::show(app, ui, &painter, &proj);
    crate::sketch_text_handles::show(app, ui, &painter, &proj, hover);
    crate::sketch3d::show(app, &painter, &proj);
    let tracking_cursor = hover.filter(|_| inside && !navigating && !app.esc_handled && !ui.input(|i| i.key_pressed(egui::Key::Escape)));
    crate::inference::track_and_show(app, &painter, &proj, tracking_cursor);
    crate::inference::show(app, &painter, &proj);
    crate::drag_snap::show(app, &painter, &proj);
    crate::insert_handles::show(app, ui, &proj);
    crate::sketch_transform_ui::refresh(app);
    crate::sketch_transform_ui::show(app, ui, &proj);
    hover_highlight(app, &painter, &proj);
    crate::drag_extrude::tick(app);
    crate::drag_extrude::show(app, &painter);
    points_2d(app, &painter, &proj);
    if let Some((plane, sketch)) = crate::sketch_attachment_ui::lost_guide(app)
        && let Some((low, high)) = sketch.bounds()
        && let Some(at) = proj.to_screen(plane.to_world(solidcore_record::Vec2::new((low.x + high.x) * 0.5, high.y)))
    {
        painter.text(
            at - vec2(0.0, 12.0),
            Align2::CENTER_BOTTOM,
            "Saved sketch plane — attachment lost",
            FontId::proportional(t.font_body),
            Tokens::view().warning,
        );
    }
    if let Some(bx) = &app.viewport.boxsel {
        paint_box_selection(&painter, bx, app.session.active_sketch.is_some());
    }
    if let Some(tl) = app.tool.as_ref() {
        crate::tools::preview(app, tl, &painter, &proj);
    }
    crate::canvas::show(app, ui, &painter, &proj);
    crate::sketch_tools::show(app, ui, &painter, &proj);
    crate::sketch_line_arc::paint_mode(app, ui, &painter, &proj);
    crate::design_properties::overlay(app, &painter, &proj);
    crate::sketch_dims::show(app, ui, &proj);
    crate::select_other::show(app, ui.ctx());
    if !app.universal_panel_host_enabled() {
        axis_view(app, ui, rect);
        crate::view_angles::show(app, ui, rect);
        crate::navigation_bar_view::show(app, ui, rect);
    }
    crate::viewport_status::show(app, ui.ctx(), rect);
    crate::viewport_status_fixture::show(app, ui.ctx(), rect);
    // The notices and the agent chip first, so the 3D mouse picture can keep clear of them.
    // The selection's measurement in the corner, before the notices so they keep clear of it.
    let readout = quick_measure_readout(app, ui, rect);
    let toasts = transient_messages(app, ui, rect, readout);
    if app.preferences.space_mouse_in_view
        && app.space_mouse.calibrating.is_none()
        && let Some(device) = &app.space_mouse.device
    {
        use crate::prefs::SpaceMouseGhost;
        let calibration = app.preferences.space_mouse_calibrations.get(device);
        let position = app.preferences.space_mouse_ghost_position;
        let navigation = app.viewport.layout.as_ref().map(|layout| layout.navigation);
        let obstacles = navigation.into_iter().chain(toasts).collect::<Vec<_>>();
        // The Viewport positions sit in the content area (right of the browser), Left Window in the
        // corner of the whole window; both stay above the floating History card, like the legend.
        let area = if position == SpaceMouseGhost::LeftWindow { rect } else { crate::shell::content_rect(app, rect) };
        let area = crate::shell::bottom_overlay_rect(app, area);
        let picture = crate::space_mouse_model::corner_rect(area, position, &obstacles);
        crate::space_mouse_model::corner(ui, picture, calibration, app.space_mouse.raw.into());
    }
    // Last, so it sits over the overlays; only where a click would reach the model.
    let badge_at = hover.filter(|p| {
        inside
            && !navigating
            && resp.hovered()
            && ui.ctx().layer_id_at(*p).is_none_or(|layer| layer == ui.layer_id())
            && !crate::shell::covered(app, *p)
    });
    crate::tool_cursor::show(app, &painter, badge_at);
}

fn paint_box_selection(painter: &egui::Painter, bx: &BoxSel, sketch: bool) {
    let t = Tokens::get();
    let col = if sketch {
        t.sketch_accent
    } else if bx.crossing {
        t.box_crossing
    } else {
        t.box_window
    };
    // The lasso is drawn as its closed outline: a fill of a concave loop would need tessellating.
    if bx.is_lasso() {
        painter.add(Shape::closed_line(bx.lasso.clone(), Stroke::new(1.2, col)));
        return;
    }
    painter.rect_filled(bx.rect, 0.0, col.gamma_multiply(0.12));
    if bx.crossing {
        let c = [bx.rect.left_top(), bx.rect.right_top(), bx.rect.right_bottom(), bx.rect.left_bottom()];
        for i in 0..4 {
            painter.add(Shape::dashed_line(&[c[i], c[(i + 1) % 4]], Stroke::new(1.2, col), 6.0, 4.0));
        }
    } else {
        painter.rect_stroke(bx.rect, 0.0, Stroke::new(1.2, col), egui::StrokeKind::Inside);
    }
}

/// An agent's command computing off the window thread (#1989): which one, in the agent cursor's
/// colour, and a button that stops waiting for it. Centred on the navigation bar, just above it,
/// and moved up past anything it would cover (`taken`: the toasts and other controls).
fn agent_chip(app: &mut SolveApp, ui: &egui::Ui, view: Rect, taken: &[Rect]) -> Option<Rect> {
    let t = Tokens::get();
    let painter = ui.painter().clone();
    if let Some((since, label)) = app.agent_computing() {
        let seconds = ((crate::now_ms() - since) / 1000.0).floor().max(0.0);
        let computing = crate::i18n::format("dialog.computing", &[("seconds", &format!("{seconds:.0}"))]);
        let galley = painter.layout_no_wrap(format!("{label} · {computing}"), FontId::proportional(t.font_label), t.text);
        let stop = painter.layout_no_wrap(crate::i18n::tr("ui.agent_chat_stop").to_string(), FontId::proportional(t.font_label), t.text);
        let size = vec2(galley.size().x + stop.size().x + 54.0, galley.size().y.max(stop.size().y) + 8.0);
        // Above the navigation bar while it sits on the bottom edge; a bar on top leaves the bottom free.
        let nav = app
            .viewport
            .layout
            .as_ref()
            .map(|l| l.navigation)
            .filter(|n| n.is_positive() && n.is_finite() && app.preferences.navigation_bar.edge == crate::navigation_bar::Edge::Bottom);
        let (x, bottom) = nav.map_or((view.center().x, view.bottom() - t.space_8), |n| (n.center().x, n.top() - t.space_8));
        let x = x.clamp(view.left() + size.x * 0.5, (view.right() - size.x * 0.5).max(view.left() + size.x * 0.5));
        let mut r = Rect::from_min_size(pos2(x - size.x * 0.5, bottom - size.y), size);
        // Each step clears one rectangle, so this ends after at most `taken.len()` moves.
        for _ in 0..=taken.len() {
            match taken.iter().filter(|o| o.expand(t.space_4).intersects(r)).map(|o| o.top()).reduce(f32::min) {
                Some(top) => r = r.translate(vec2(0.0, top - t.space_8 - r.bottom())),
                None => break,
            }
        }
        let pos = r.min;
        crate::scenario::publish_rect("agent:chip", r);
        painter.rect_filled(r, t.radius_sm, t.overlay);
        painter.rect_stroke(r, t.radius_sm, Stroke::new(1.0, crate::icons::pointer::agent()), egui::StrokeKind::Inside);
        egui::Spinner::new()
            .color(crate::icons::pointer::agent())
            .size(t.space_12)
            .paint_at(ui, Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), vec2(12.0, 12.0)));
        painter.galley(pos + vec2(26.0, 4.0), galley.clone(), t.text);
        let button = Rect::from_min_size(pos2(pos.x + galley.size().x + 34.0, r.top() + 2.0), vec2(stop.size().x + 12.0, r.height() - 4.0));
        let clicked = ui.interact(button, egui::Id::unique("sc_agent_stop"), Sense::click());
        painter.rect_filled(button, t.radius_sm.saturating_sub(1), if clicked.hovered() { t.hover } else { t.overlay });
        painter.galley(button.min + vec2(6.0, 2.0), stop, t.text);
        crate::scenario::publish_handle("agent:stop", button.center());
        if clicked.clicked() {
            app.stop_agent_command();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        return Some(r);
    }
    None
}

/// How long a status message stays in the view (milliseconds).
const STATUS_TOAST_MS: f64 = 6000.0;

/// The selection's measurement in the view's bottom right corner (`quick_measure`), clear of the
/// panels and cards over the view and its navigation bar.
fn quick_measure_readout(app: &mut SolveApp, ui: &egui::Ui, viewport: Rect) -> Option<Rect> {
    crate::quick_measure::poll(app, ui.ctx());
    let mut taken = crate::shell::covered_rects(app);
    if let Some(layout) = &app.viewport.layout {
        taken.extend([layout.navigation, layout.axis_view]);
    }
    let view = crate::shell::content_rect(app, viewport);
    crate::quick_measure::show(app, ui, view, &taken)
}

/// A single visual pattern for ongoing work, the existing six-second status, the 3D mouse
/// calibration offer and the stand-up note, stacked from the bottom of the free view. Returns the toasts' rects.
pub(crate) fn transient_messages(app: &mut SolveApp, ui: &egui::Ui, viewport: Rect, readout: Option<Rect>) -> Vec<Rect> {
    let placement_hint =
        app.tool.as_ref().filter(|tool| matches!(tool.cmd, "canvas.insert" | "canvas.decal")).map(|tool| crate::tools::hint(tool.cmd));
    // Nothing to show or time: an idle window skips the layout. The offer is checked too, as its clock
    // runs from the frames `due` sees.
    if placement_hint.is_none()
        && app.status.is_none()
        && app.computing_since().is_none()
        && app.agent_computing().is_none()
        && crate::space_mouse_offer::offerable(app).is_none()
        && !crate::stand_up::note_pending(app)
    {
        return Vec::new();
    }
    let t = Tokens::get();
    let mut shown = Vec::new();
    let mut obstacles = crate::shell::covered_rects(app);
    if let Some(layout) = &app.viewport.layout {
        obstacles.extend([layout.navigation, layout.axis_view]);
    }
    obstacles.extend(readout);
    let view = crate::shell::content_rect(app, viewport);
    let mut free = crate::viewport_layout::free_rect(view, &obstacles).shrink(t.space_8);
    if ToastPlace::current(ui.ctx()) == ToastPlace::Corner {
        free = corner_free(view, &obstacles);
    }
    // The note offering to stand up a file just opened from a program not known to save Y up
    // (#7717): the shared toast with its buttons, which go by themselves and never block input.
    if let Some(text) = crate::stand_up::note_due(app, ui.ctx()) {
        let labels = crate::stand_up::note_labels();
        let actions = [(labels[0].as_str(), ToastKind::Primary), (labels[1].as_str(), ToastKind::Secondary)];
        let [close, dismiss] = crate::stand_up::NOTE_CLOSE_HANDLES;
        let extras = ToastExtras {
            close: Some(close),
            countdown: Some((crate::stand_up::note_left(app, ui.ctx()), crate::stand_up::NOTE_SECONDS)),
            ..Default::default()
        };
        if let Some(toast) = paint_toast_ext(ui, free, text, ToastLead::Dot(false), &actions, extras) {
            let r = toast.rect;
            crate::scenario::publish_rect("toast:stand_up", r);
            for (handle, button) in crate::stand_up::NOTE_HANDLES.iter().zip(&toast.buttons) {
                crate::scenario::publish_handle(handle, button.center());
            }
            if let Some(at) = toast.close {
                crate::scenario::publish_handle(dismiss, at.center());
            }
            obstacles.push(r);
            free.max.y = r.top() - t.space_8;
            shown.push(r);
            crate::stand_up::note_keep(app, ui.ctx(), r);
            if let Some(index) = toast.clicked {
                crate::stand_up::note_act(app, index);
            } else if toast.closed {
                crate::stand_up::leave(app);
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
    // The 3D mouse calibration offer: text and actions from its module, drawn by the shared toast.
    if let Some(offer) = crate::space_mouse_offer::due(app, ui.ctx()) {
        let labels: Vec<String> = offer.actions.iter().map(|action| crate::i18n::tr(action.label()).to_string()).collect();
        let labels: Vec<(&str, ToastKind)> = labels.iter().map(String::as_str).zip(offer.actions.iter().map(|action| action.kind())).collect();
        let tips: Vec<&str> = offer.actions.iter().map(|action| action.tip().map_or("", crate::i18n::tr)).collect();
        let extras = ToastExtras {
            close: Some("toast:space_mouse_offer:close"),
            countdown: Some((crate::space_mouse_offer::left(app, ui.ctx()), crate::space_mouse_offer::SHOW_SECONDS)),
            tips: &tips,
            ..Default::default()
        };
        if let Some(toast) = paint_toast_ext(ui, free, crate::i18n::tr("ui.3d_mouse_offer").to_string(), ToastLead::Dot(false), &labels, extras) {
            let r = toast.rect;
            crate::scenario::publish_rect("toast:space_mouse_offer", r);
            for (index, button) in toast.buttons.iter().enumerate() {
                crate::scenario::publish_rect(&format!("toast:space_mouse_offer:button:{index}"), *button);
            }
            obstacles.push(r);
            free.max.y = r.top() - t.space_8;
            shown.push(r);
            crate::space_mouse_offer::keep(app, ui.ctx(), r);
            // The close (×) does what the Close button did (#7971).
            let action =
                if toast.closed { Some(crate::space_mouse_offer::Action::Close) } else { toast.clicked.and_then(|i| offer.actions.get(i).copied()) };
            if let Some(action) = action {
                crate::space_mouse_offer::act(app, ui.ctx(), &offer, action);
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
    if let Some(hint) = placement_hint
        && let Some(r) = paint_toast(ui, free, hint, false, false)
    {
        crate::scenario::publish_rect("toast:placement", r);
        obstacles.push(r);
        free.max.y = r.top() - t.space_8;
    }
    if let Some((msg, at, error)) = app.status.clone() {
        let shown_ms = crate::now_ms() - at;
        if shown_ms < STATUS_TOAST_MS {
            let undo = app.status_undo;
            let actions = if undo { vec![(crate::i18n::tr_label("Undo"), ToastKind::Secondary)] } else { Vec::new() };
            let extras = ToastExtras {
                close: Some("toast:status:close"),
                countdown: Some((countdown_left(0.0, shown_ms / 1000.0, STATUS_TOAST_MS / 1000.0), STATUS_TOAST_MS / 1000.0)),
                // Without Undo, a click anywhere else on it opens the run's messages.
                card_hint: (!undo).then(|| crate::i18n::tr("messages.chip_hint")),
                ..Default::default()
            };
            if let Some(toast) = paint_toast_ext(ui, free, msg, ToastLead::Dot(error), &actions, extras) {
                let r = toast.rect;
                if undo && toast.clicked == Some(0) {
                    app.status = None;
                    app.status_undo = false;
                    let _ = app.run("edit.undo", json!({}));
                }
                obstacles.push(r);
                crate::scenario::publish_rect("toast", r);
                free.max.y = r.top() - t.space_8;
                shown.push(r);
                // Open the run's messages from the tokenised status toast.
                crate::scenario::publish_handle("status:chip", r.center());
                if toast.card_clicked {
                    app.messages.open = true;
                }
                if toast.closed {
                    app.status = None;
                    app.status_undo = false;
                }
            }
            // One frame when it is due to go, not a frame every half second while it shows.
            let left_ms = STATUS_TOAST_MS - shown_ms;
            let left_ms = if (0.0..=STATUS_TOAST_MS).contains(&left_ms) { left_ms } else { STATUS_TOAST_MS };
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left_ms / 1000.0 + 0.01));
        } else {
            app.status = None;
        }
    }
    if let Some(since) = app.computing_since() {
        let seconds = ((crate::now_ms() - since) / 1000.0).floor().max(0.0);
        let text = crate::i18n::format("dialog.computing", &[("seconds", &format!("{seconds:.0}"))]);
        // A history command or a delete's check can be given up on (#8266); a feature edit
        // applying has the dialog's Cancel instead.
        let cancel = app.tr_label("Cancel").to_string();
        let actions: Vec<(&str, ToastKind)> = if app.can_cancel_computing() { vec![(cancel.as_str(), ToastKind::Secondary)] } else { Vec::new() };
        if let Some((r, clicked, buttons)) = paint_toast_with(ui, free, text, ToastLead::Spinner, &actions) {
            if let Some(button) = buttons.first() {
                crate::scenario::publish_handle("toast:computing:cancel", button.center());
            }
            if clicked == Some(0) {
                app.cancel_computing();
            }
            obstacles.push(r);
            crate::scenario::publish_rect("toast:computing", r);
            if app.status.is_none() {
                crate::scenario::publish_rect("toast", r);
            }
            shown.push(r);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    // The navigation bar is the chip's anchor, so it doesn't count as something to move past.
    let nav = app.viewport.layout.as_ref().map(|l| l.navigation);
    obstacles.retain(|o| Some(*o) != nav);
    shown.extend(agent_chip(app, ui, view, &obstacles));
    shown
}

/// Where toasts stack (Preferences › Interface › Notices, #7893).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastPlace {
    /// One column in the view's bottom-right corner, every toast `TOAST_WIDTH` wide, buttons as in
    /// the dialogs: right-aligned, dismissals first and the main action right-most.
    #[default]
    Corner,
    /// At the bottom centre, above the view toolbar, each as wide as its text (as before #7893).
    /// Also what a value this version doesn't know reads as (`prefs::lenient`).
    Centre,
}

impl ToastPlace {
    pub(crate) fn publish(self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique("sc_toast_place"), self));
    }

    pub(crate) fn current(ctx: &egui::Context) -> Self {
        ctx.data(|data| data.get_temp(egui::Id::unique("sc_toast_place"))).unwrap_or_default()
    }
}

/// Every toast's width in the corner stack: room for three buttons on one row (the 3D mouse offer's
/// Calibrate…, Use Direct Driver and Never show again, #7971); longer text wraps.
pub(crate) const TOAST_WIDTH: f32 = 360.0;

/// Where the corner stack may go: the largest free rect in a `TOAST_WIDTH` column at the right of
/// `view`, clear of `obstacles` (the axis view and cube at the top, the view toolbar when a narrow
/// window brings it under the column, docked panels), less the usual margin. A pane at the right
/// edge (an edge-docked dialog, the Assistant) moves the column, at full width, to its left edge
/// rather than leaving a strip beside it (#8804, as the frame-rate readout does, #6755). The first
/// candidate column that is `TOAST_WIDTH` wide and high enough wins, otherwise the best of the rest:
/// a tall enough place beats a flat one, then the wider.
pub(crate) fn corner_free(view: Rect, obstacles: &[Rect]) -> Rect {
    let t = Tokens::get();
    let width = TOAST_WIDTH + 2.0 * t.space_8;
    // The column's right edge: the view's, then each obstacle's left edge, nearest the corner first (at most 16).
    let mut rights = vec![view.right()];
    let mut lefts: Vec<f32> = obstacles.iter().filter(|o| o.is_finite() && o.left() < view.right()).map(|o| o.left()).collect();
    lefts.sort_by(|a, b| b.total_cmp(a));
    rights.extend(lefts.into_iter().take(16));
    let min_height = 8.0 * t.space_8; // room for a one-line toast
    let mut best = Rect::NOTHING;
    let (mut best_fits, mut best_tall) = (false, false);
    for right in rights {
        let left = (right - width).max(view.left());
        let column = Rect::from_min_max(pos2(left, view.top()), pos2(right, view.bottom()));
        let free = crate::viewport_layout::free_rect(column, obstacles).shrink(t.space_8);
        // A strip of full width but a line high would hide the toast, so it does not fit.
        let tall = free.height() >= min_height;
        let fits = tall && free.width() >= TOAST_WIDTH - 0.5;
        let better =
            (fits, tall) > (best_fits, best_tall) || ((fits, tall) == (best_fits, best_tall) && (!best.is_positive() || free.width() > best.width()));
        if better {
            best = free;
            best_fits = fits;
            best_tall = tall;
        }
        if best_fits || left <= view.left() {
            break;
        }
    }
    best
}

/// What a toast's button is for, which sets its look and where it goes: the main action first
/// (the accent button), then an alternative, then the quiet dismissals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ToastKind {
    Primary,
    Secondary,
    Dismiss,
}

/// Where each button of a toast goes, relative to the row's top left: `buttons` are (width, kind) in
/// the order given; the primary and the alternative form one group and the dismissals another.
/// Groups go side by side while they fit in `row_width` and wrap as a whole, so a button is not
/// left alone on a row while its group-mate fits beside it; a group wider than the row wraps
/// button by button.
fn toast_action_spots(buttons: &[(f32, ToastKind)], row_width: f32, gap: f32, height: f32) -> Vec<Rect> {
    let mut spots = vec![Rect::NOTHING; buttons.len()];
    let groups: [Vec<usize>; 2] = [
        (0..buttons.len()).filter(|&i| buttons[i].1 != ToastKind::Dismiss).collect(),
        (0..buttons.len()).filter(|&i| buttons[i].1 == ToastKind::Dismiss).collect(),
    ];
    let (mut x, mut y) = (0.0_f32, 0.0_f32);
    for group in groups.iter().filter(|g| !g.is_empty()) {
        let whole: f32 = group.iter().map(|&i| buttons[i].0).sum::<f32>() + gap * (group.len() - 1) as f32;
        if x > 0.0 && x + whole > row_width {
            (x, y) = (0.0, y + height + gap);
        }
        for &i in group {
            let width = buttons[i].0;
            // Only a group wider than a whole row wraps inside itself.
            if x > 0.0 && x + width > row_width.max(width) && whole > row_width {
                (x, y) = (0.0, y + height + gap);
            }
            spots[i] = Rect::from_min_size(pos2(x, y), vec2(width, height));
            x += width + gap;
        }
        x += gap;
    }
    spots
}

/// The corner stack's buttons, as the dialogs place theirs: right-aligned, dismissals first, then
/// the alternative, then the main action right-most. Rows fill from the main action leftwards; a
/// button that doesn't fit starts a row above, so the row with the main action is the bottom one.
/// Positions are relative to the rows' top left, within `row_width`.
fn toast_action_spots_trailing(buttons: &[(f32, ToastKind)], row_width: f32, gap: f32, height: f32) -> Vec<Rect> {
    let rank = |kind: ToastKind| match kind {
        ToastKind::Dismiss => 0,
        ToastKind::Secondary => 1,
        ToastKind::Primary => 2,
    };
    let mut order: Vec<usize> = (0..buttons.len()).collect();
    order.sort_by_key(|&i| rank(buttons[i].1));
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut used = 0.0_f32;
    for &i in order.iter().rev() {
        let width = buttons[i].0;
        match rows.last_mut() {
            Some(row) if used + gap + width <= row_width => {
                row.push(i);
                used += gap + width;
            }
            _ => {
                rows.push(vec![i]);
                used = width;
            }
        }
    }
    let mut spots = vec![Rect::NOTHING; buttons.len()];
    let count = rows.len();
    for (k, row) in rows.iter().enumerate() {
        let y = (count - 1 - k) as f32 * (height + gap);
        let mut x = row_width;
        for &i in row {
            x -= buttons[i].0;
            if let Some(spot) = spots.get_mut(i) {
                *spot = Rect::from_min_size(pos2(x, y), vec2(buttons[i].0, height));
            }
            x -= gap;
        }
    }
    spots
}

/// Where a toast button's drawn width is remembered: by its text and kind, which set its look.
fn toast_button_key(label: &str, kind: ToastKind) -> egui::Id {
    egui::Id::unique(("sc_toast_button_width", label, kind))
}

/// The width a toast button with this text and kind was last drawn at, if it has been drawn.
fn toast_button_width(ctx: &egui::Context, label: &str, kind: ToastKind) -> Option<f32> {
    ctx.data(|data| data.get_temp::<f32>(toast_button_key(label, kind))).filter(|width| width.is_finite() && *width > 0.0)
}

/// What leads a toast's text: a dot (red for an error) or a spinner.
#[derive(Clone, Copy)]
enum ToastLead {
    Dot(bool),
    Spinner,
}

fn paint_toast(ui: &egui::Ui, free: Rect, text: String, error: bool, computing: bool) -> Option<Rect> {
    let lead = if computing { ToastLead::Spinner } else { ToastLead::Dot(error) };
    paint_toast_with(ui, free, text, lead, &[]).map(|(rect, _, _)| rect)
}

/// [`paint_toast_ext`] with no close (×), countdown, hover texts or card click: the card, which
/// button was clicked, if any, and where each button is.
fn paint_toast_with(
    ui: &egui::Ui,
    free: Rect,
    text: String,
    lead: ToastLead,
    actions: &[(&str, ToastKind)],
) -> Option<(Rect, Option<usize>, Vec<Rect>)> {
    paint_toast_ext(ui, free, text, lead, actions, ToastExtras::default()).map(|toast| (toast.rect, toast.clicked, toast.buttons))
}

/// What a toast has besides its text and buttons (#7971).
#[derive(Clone, Copy, Default)]
pub(crate) struct ToastExtras<'a> {
    /// A close (×) in its top-right corner, published as this control-channel handle
    /// (`toast:<name>:close`). Every toast the user may close has one, in place of a Close button.
    pub close: Option<&'a str>,
    /// For a toast that goes by itself: the share of its time left (1 to 0) and its whole time in
    /// seconds, drawn as a thin ring around the × that shrinks over its time.
    pub countdown: Option<(f32, f64)>,
    /// Hover texts of the buttons, by index; an empty one has none.
    pub tips: &'a [&'a str],
    /// The whole card answers a click, under its buttons and ×, with this hover text.
    pub card_hint: Option<&'a str>,
}

/// A toast as drawn: its card, which button was clicked, if any, where each button and the × are
/// (for handles), and whether the × or the card itself was clicked.
pub(crate) struct ToastShown {
    pub rect: Rect,
    pub clicked: Option<usize>,
    pub buttons: Vec<Rect>,
    pub close: Option<Rect>,
    pub closed: bool,
    pub card_clicked: bool,
}

/// The share of a countdown of `lifetime` seconds left at `now` when it started at `since`: 1 when
/// it starts, 0 when it is over, and 1 for times no clock gives (non-finite, or a start in the future).
pub(crate) fn countdown_left(since: f64, now: f64, lifetime: f64) -> f32 {
    let left = 1.0 - (now - since) / lifetime;
    if left.is_finite() { left.clamp(0.0, 1.0) as f32 } else { 1.0 }
}

/// The countdown ring's radius around a toast's ×; the × is drawn inside it and its hit target is
/// the ring's square.
fn toast_close_radius() -> f32 {
    Tokens::get().space_10
}

/// The points of the countdown ring: an arc of `left` of a turn round `centre`, from twelve o'clock
/// clockwise, so it shrinks back towards twelve o'clock as the time goes. Empty when nothing is left.
pub(crate) fn countdown_arc(centre: Pos2, radius: f32, left: f32) -> Vec<Pos2> {
    let left = if left.is_finite() { left.clamp(0.0, 1.0) } else { 0.0 };
    if left <= 0.0 || !radius.is_finite() || radius <= 0.0 {
        return Vec::new();
    }
    // One segment per 7.5 degrees of the arc, at least one.
    let segments = ((48.0 * left).ceil() as usize).max(1);
    (0..=segments)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * left * i as f32 / segments as f32;
            centre + radius * vec2(angle.cos(), angle.sin())
        })
        .collect()
}

/// A toast's close (×) at `rect`, with its countdown ring when `left` is given: a kit-drawn button
/// with an accessible name and hover text (`ui.close`), reachable by Tab and taken with Enter or Space.
fn toast_close(ui: &egui::Ui, rect: Rect, id: egui::Id, left: Option<f32>) -> egui::Response {
    let t = Tokens::get();
    let response = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    let centre = rect.center();
    let radius = rect.width().min(rect.height()) * 0.5;
    let lit = response.hovered() || response.has_focus();
    if lit && ui.is_enabled() {
        painter.circle_filled(centre, radius, t.hover);
    }
    if response.has_focus() {
        painter.circle_stroke(centre, radius + t.space_2, Stroke::new(1.0, t.form_primary));
    }
    if let Some(left) = left {
        let arc = countdown_arc(centre, radius - 0.75, left);
        if arc.len() >= 2 {
            painter.add(egui::Shape::line(arc, Stroke::new(1.5, t.text_dim)));
        }
    }
    let ink = if lit && ui.is_enabled() { t.text } else { t.text_dim };
    crate::icons::paint_ink(painter, Rect::from_center_size(centre, vec2(t.space_10, t.space_10)), "close", ink);
    let label = crate::i18n::tr("ui.close");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    if response.hovered() && ui.is_enabled() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.backing_on_hover_text(label)
}

/// The one toast painter: its text in a card at the bottom centre of `free`, led by a dot or a
/// spinner, and, below the text, a wrapping row of buttons (`actions`, kit buttons), with what
/// `extras` adds (a close (×) at the top right with its countdown ring, hover texts, a click on the
/// card). Returns the card, which button was clicked, if any, and where each button is (for
/// handles). With no actions and no extras the card is exactly the status and computing toast it
/// always was. The buttons and the × are not live while an issue report holds CAD input; none is
/// laid outside the card, whose width is bounded by `free`.
fn paint_toast_ext(
    ui: &egui::Ui,
    free: Rect,
    text: String,
    lead: ToastLead,
    actions: &[(&str, ToastKind)],
    extras: ToastExtras<'_>,
) -> Option<ToastShown> {
    // Rects no screen has are rejected too: the corner layout anchors to `free`'s right edge.
    if !free.is_finite() || !free.is_positive() || free.width() <= 40.0 || free.max.x.abs().max(free.max.y.abs()) > 1.0e7 {
        return None;
    }
    let t = Tokens::get();
    let corner = ToastPlace::current(ui.ctx()) == ToastPlace::Corner;
    let painter = ui.painter().with_clip_rect(free);
    // In the corner every toast is `TOAST_WIDTH` wide (or the room there is); at the centre each
    // is as wide as its text, up to 440 points.
    let wrap = if corner { (TOAST_WIDTH.min(free.width()) - 40.0).max(0.0) } else { (free.width() - 40.0).min(440.0) };
    // The × keeps the top-right corner: the text stops short of its ring.
    let radius = toast_close_radius();
    let close_room = if extras.close.is_some() { 2.0 * radius + t.space_4 + t.space_8 - 12.0 } else { 0.0 };
    let galley = painter.layout(text, crate::fonts::medium(t.font_label), t.text, (wrap - close_room).max(0.0));
    // The buttons: each as wide as its text and a margin, in rows no wider than the text may be.
    let gap = t.space_8;
    let button_height = crate::ui_kit::row_height();
    let row_width = if corner { wrap } else { (free.width() - 40.0).clamp(0.0, 440.0) };
    // Each button's width as the kit drew it (#7971): the same text, font and padding, measured
    // from the button itself. Before a button was ever drawn, an estimate stands in for one frame.
    let sizes: Vec<(f32, ToastKind)> = actions
        .iter()
        .map(|(label, kind)| {
            let width = toast_button_width(ui.ctx(), label, *kind).unwrap_or_else(|| {
                let padding = if *kind == ToastKind::Primary { t.space_16 } else { t.space_12 };
                painter.layout_no_wrap((*label).to_string(), crate::fonts::medium(t.font_label), t.text).size().x + 2.0 * padding
            });
            (width.min(row_width), *kind)
        })
        .collect();
    let spots = if corner {
        toast_action_spots_trailing(&sizes, row_width, gap, button_height)
    } else {
        toast_action_spots(&sizes, row_width, gap, button_height)
    };
    let widest = spots.iter().map(|spot| spot.right()).fold(0.0, f32::max);
    let y = spots.iter().map(|spot| spot.top()).fold(0.0, f32::max);
    let actions_height = if spots.is_empty() { 0.0 } else { y + button_height + gap };
    let content = vec2((galley.size().x + close_room).max(widest), galley.size().y + actions_height);
    let mut size = content + vec2(40.0, 16.0);
    if corner {
        size.x = row_width + 40.0;
    }
    if extras.close.is_some() {
        size.y = size.y.max(2.0 * radius + 12.0);
    }
    if !size.is_finite() || size.x > free.width() || size.y > free.height() {
        return None;
    }
    let x = if corner { free.right() - size.x * 0.5 } else { free.center().x };
    let r = Rect::from_center_size(pos2(x, free.bottom() - size.y * 0.5), size);
    if !r.is_finite() {
        return None;
    }
    let frame = Container::new(ui.ctx(), Surface::Toast).frame(ui.ctx(), t.panel, crate::frame::Side::Left);
    Container::paint_frame(ui.painter(), frame, r.shrink(frame.stroke.width));
    let dot = pos2(r.left() + 14.0, r.center().y.min(r.top() + 8.0 + galley.size().y / 2.0));
    match lead {
        ToastLead::Spinner => egui::Spinner::new().size(12.0).color(t.text).paint_at(ui, Rect::from_center_size(dot, vec2(12.0, 12.0))),
        ToastLead::Dot(error) => drop(painter.circle_filled(dot, 3.0, if error { t.error } else { t.text })),
    }
    let text_top = r.min + vec2(28.0, 8.0);
    painter.galley(text_top, galley.clone(), t.text);
    // The card's own click goes first, so its buttons and × (added after it) take theirs. Behind an
    // issue report it is dead too, so a click on the dead × doesn't fall through to it.
    let blocked = crate::issue_report::cad_input_blocked();
    let card_clicked = extras.card_hint.is_some_and(|hint| {
        let sense = if blocked { Sense::hover() } else { Sense::click() };
        ui.interact(r, egui::Id::unique(("sc_toast_card", extras.close.unwrap_or_default())), sense).backing_on_hover_text(hint).clicked()
    });
    let mut clicked = None;
    let mut buttons = Vec::with_capacity(actions.len());
    let origin = text_top + vec2(0.0, galley.size().y + gap);
    for (index, ((label, kind), spot)) in actions.iter().zip(&spots).enumerate() {
        let at = spot.translate(origin.to_vec2());
        // A UI of its own on the viewport's layer, so the toast needs no `&mut Ui`. It has the
        // row's width, so a button is never squeezed into a spot laid out for a smaller one and
        // its measured width is its own.
        let room = Rect::from_min_size(at.min, vec2(row_width.max(at.width()), at.height()));
        let mut button_ui = egui::Ui::new(
            ui.ctx().clone(),
            egui::Id::unique(("sc_toast_action", index)),
            egui::UiBuilder::new().layer_id(ui.layer_id()).max_rect(room).layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        button_ui.set_clip_rect(button_ui.clip_rect().intersect(r));
        let response = button_ui
            .add_enabled_ui(!blocked, |ui| match kind {
                ToastKind::Primary => crate::ui_kit::primary_button(ui, label),
                ToastKind::Secondary => crate::ui_kit::button(ui, *label, crate::ui_kit::Button::Normal),
                ToastKind::Dismiss => crate::ui_kit::button(ui, *label, crate::ui_kit::Button::Small),
            })
            .inner;
        let response = match extras.tips.get(index).filter(|tip| !tip.is_empty()) {
            Some(tip) => response.backing_on_hover_text(*tip),
            None => response,
        };
        if response.clicked() {
            clicked = Some(index);
        }
        // Remember the width it was drawn at; laid out at another, the toast is laid out again next frame.
        let drawn = response.rect.width();
        if drawn.is_finite() && drawn > 0.0 {
            if sizes.get(index).is_some_and(|(used, _)| (used - drawn).abs() > 0.5 && drawn < row_width) {
                ui.ctx().request_repaint();
            }
            ui.ctx().data_mut(|data| data.insert_temp(toast_button_key(label, *kind), drawn));
        }
        buttons.push(response.rect);
    }
    // The close (×), top right, level with the text's first line.
    let mut close = None;
    let mut closed = false;
    if let Some(handle) = extras.close {
        let line = painter.layout_no_wrap("A".into(), crate::fonts::medium(t.font_label), t.text).size().y;
        let centre = pos2(r.right() - t.space_8 - radius, (text_top.y + line * 0.5).max(r.top() + 6.0 + radius));
        let at = Rect::from_center_size(centre, vec2(2.0 * radius, 2.0 * radius));
        let mut close_ui = egui::Ui::new(
            ui.ctx().clone(),
            egui::Id::unique(("sc_toast_close_ui", handle)),
            egui::UiBuilder::new().layer_id(ui.layer_id()).max_rect(at),
        );
        close_ui.set_clip_rect(close_ui.clip_rect().intersect(r));
        if blocked {
            close_ui.disable();
        }
        let left = extras.countdown.map(|(left, _)| left);
        closed = toast_close(&close_ui, at, egui::Id::unique(("sc_toast_close", handle)), left).clicked();
        crate::scenario::publish_handle(handle, centre);
        close = Some(at);
        // A frame each time the ring's end moves about a point, not one per display refresh.
        if let Some((_, lifetime)) = extras.countdown {
            let every = lifetime / (std::f64::consts::TAU * f64::from(radius));
            let every = if every.is_finite() { every.clamp(0.05, 1.0) } else { 1.0 };
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(every));
        }
    }
    Some(ToastShown { rect: r, clicked, buttons, close, closed, card_clicked })
}

/// A click without a command: select the candidate (replacing the selection, or adding /
/// toggling with modifiers); a click on nothing clears.
///
/// `toggle`: whether already-selected items may be removed by this click (Ctrl/Cmd held).
/// When `add` is true but `toggle` is false (Shift add-only preference), picked items
/// are only added.
pub(crate) fn select(app: &mut SolveApp, sel: Option<Sel>, add: bool, toggle: bool) {
    if !add {
        // A joint picked in the browser (highlighted even when hidden) lets go.
        app.tree.picked_joints.clear();
    }
    match sel {
        Some(x) => {
            let items = picked_selection(&app.session, &x);
            // Toggle-removal: all picked items are already selected and toggle is allowed.
            if add && toggle && items.iter().all(|x| app.session.selection.iter().any(|y| crate::selection::same_item(x, y))) {
                let rest: Vec<Sel> =
                    app.session.selection.iter().filter(|y| !items.iter().any(|x| crate::selection::same_item(x, y))).cloned().collect();
                let _ = app.run("select.set", json!({ "items": rest }));
            } else {
                let _ = app.run("select.set", json!({"items": items, "add": add}));
                crate::browser::reveal_pick(app);
            }
        }
        None if !add => {
            let _ = app.run("select.clear", json!({}));
        }
        None => {}
    }
}

/// The rest of the selected sketch geometry when the point or curve `id` is part of it (#2621):
/// a drag that starts on a selected entity moves the whole selection with it.
/// A drag that starts on a line's middle where a point is held (a centre rectangle's centre on
/// its diagonal) drags that point, as a drag that starts on the point does. Dragging the line by
/// its middle instead moved its ends: with the point constrained (on the origin, on a line) the
/// line turned and stretched about it, and the rectangle round it changed size (#10510).
fn point_at_midpoint(app: &SolveApp, hit: Hit) -> Hit {
    let Hit::SketchMidpoint { sketch, id, .. } = &hit else { return hit };
    let st = crate::open_meshing::session_world(&app.session);
    let held = st.sketch(*sketch).and_then(|ss| {
        let sk = &ss.sketch;
        let line = sk.curve_index(id)?;
        sk.constraints.iter().filter(|c| !c.driven).find_map(|c| match c.kind {
            sketcraft_engine::sketch::ConstraintKind::Midpoint { p, l } if l == line => sk.points.get(p).map(|q| (q.id.clone(), q.pos)),
            _ => None,
        })
    });
    match held {
        Some((id, at)) => Hit::SketchPoint { sketch: *sketch, id, at },
        None => hit,
    }
}

fn dragged_with(app: &SolveApp, id: &str) -> Vec<String> {
    fn geometry(s: &Sel) -> Option<&str> {
        match s {
            Sel::SketchPoint { id } | Sel::SketchCurve { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }
    let selected = || app.session.selection.iter().filter_map(geometry);
    if !selected().any(|s| s == id) {
        return Vec::new();
    }
    let rest: Vec<String> = selected().filter(|s| *s != id).map(str::to_owned).collect();
    // A curve's own points, selected with it by a window, are not a group: a circle grabbed on
    // its edge still takes its radius from the drag.
    let st = crate::open_meshing::session_world(&app.session);
    let own: Vec<&str> = app
        .session
        .active_sketch
        .and_then(|s| st.sketch(s))
        .and_then(|ss| {
            let sk = &ss.sketch;
            let c = sk.curves.get(sk.curve_index(id)?)?;
            Some(c.kind.point_ids().into_iter().filter_map(|p| sk.points.get(p).map(|p| p.id.as_str())).collect())
        })
        .unwrap_or_default();
    if rest.iter().all(|r| own.contains(&r.as_str())) {
        return Vec::new();
    }
    rest
}

/// What a click on `sel` picks: its pieces ([`grouped_selection`]), and for a letter of a sketch
/// text the whole text, unless Shift is held (Shift picks the one letter).
pub(crate) fn picked_selection(s: &sketcraft_engine::Session, sel: &Sel) -> Vec<Sel> {
    if let Sel::Profile { sketch, index } = sel
        && !crate::sketch_text::one_letter()
        && let Some(letters) = crate::sketch_text::text_profiles(s, *sketch, *index)
    {
        return letters.into_iter().map(|index| Sel::Profile { sketch: *sketch, index }).collect();
    }
    grouped_selection(s, sel)
}

/// Expand a visible rim or surface into the kernel pieces a command needs. The original
/// face pick keeps its point (used by tools such as Hole and construction geometry).
pub(crate) fn grouped_selection(s: &sketcraft_engine::Session, sel: &Sel) -> Vec<Sel> {
    let st = crate::open_meshing::session_world(s);
    match sel {
        Sel::Edge { body, index, .. } => {
            if let Some(b) = st.body(body) {
                let m = b.mesh();
                return m
                    .circular_rim(*index)
                    .into_iter()
                    .filter_map(|i| m.edges.get(i).map(|e| Sel::Edge { body: body.clone(), index: i, point: polyline_mid(e) }))
                    .collect();
            }
        }
        Sel::Face { body, index, .. } => {
            if let Some(b) = st.body(body) {
                let m = b.mesh();
                let mut patch = m.surface_patch(*index);
                patch.sort_by_key(|i| usize::from(i != index));
                return patch
                    .into_iter()
                    .filter_map(|i| {
                        if i == *index {
                            return Some(sel.clone());
                        }
                        m.triangles.iter().zip(&m.tri_face).find_map(|(t, f)| {
                            if *f as usize != i {
                                return None;
                            }
                            let [a, b, c] = m.tri(t)?;
                            Some(Sel::Face { body: body.clone(), index: i, point: (a + b + c) / 3.0 })
                        })
                    })
                    .collect();
            }
        }
        _ => {}
    }
    vec![sel.clone()]
}

/// Measure keeps one entry per visible item, rather than using its two slots for kernel halves.
pub(crate) fn measurement_selection(s: &sketcraft_engine::Session, sel: &Sel) -> Sel {
    let pieces = grouped_selection(s, sel);
    let mut representative = sel.clone();
    match &mut representative {
        Sel::Edge { index, .. } => {
            if let Some(Sel::Edge { index: i, .. }) = pieces.first() {
                *index = *i;
            }
        }
        Sel::Face { index, .. } => {
            if let Some(i) = pieces
                .iter()
                .filter_map(|p| match p {
                    Sel::Face { index, .. } => Some(*index),
                    _ => None,
                })
                .min()
            {
                *index = i;
            }
        }
        _ => {}
    }
    representative
}

/// What a box selects: sketch entities while sketching, otherwise what the open
/// dialog's active input takes (edges, faces, bodies or regions), otherwise the
/// bodies of the design. A design with no body (an imported SVG leaves a sketch
/// and no body) has only its regions (profiles) to select, so the marquee takes
/// those too. An input that asks for edges or faces never takes a region: the
/// regions are neither. A click takes a region of a shown sketch as well (see
/// hit_sel), but the marquee of a design that has a body takes the bodies alone.
fn box_select(app: &mut SolveApp, proj: &Proj, bx: BoxSel, add: bool) {
    use crate::selection::{BODIES, EDGES, FACES, PROFILES};
    // While OK computes, the dialog's inputs hold.
    if app.applying.is_some() {
        return;
    }
    let st = crate::open_meshing::shown_world(app);
    let mut out: Vec<Sel> = Vec::new();
    let to2 = |pts: &[Vec3]| -> Vec<Pos2> { pts.iter().filter_map(|p| proj.to_screen(*p)).collect() };
    let sketch_dialog = app.dialog.as_ref().is_some_and(|d| matches!(d.kind, crate::dialogs::Kind::SketchModify(_)));
    let accepts = app.dialog.as_ref().and_then(|d| d.active_input()).map(|i| i.accept);
    if (app.dialog.is_none() || sketch_dialog)
        && let Some(ss) = app.session.active_sketch.and_then(|sid| st.sketch(sid))
    {
        // What the selection filter lets through (window: wholly inside; crossing: touching).
        let off = |k: &str| app.ui.pick_off.iter().any(|x| x == k);
        for (i, c) in ss.sketch.curves.iter().enumerate().filter(|_| !off("sketch") && !off("sketch_curves")) {
            if accepts
                .is_some_and(|a| a & crate::selection::CURVES == 0 && (a & crate::selection::AXES == 0 || !matches!(c.kind, CurveKind::Line { .. })))
            {
                continue;
            }
            if !sketcraft_engine::view::sketch_curve_visible(&ss.sketch, i) {
                continue;
            }
            let sampled = if bx.crossing {
                crate::sketch_display::curve_sample(app, ss.feature, i, &ss.plane, proj)
            } else {
                std::sync::Arc::new(crate::sketch_display::curve_unclipped(&ss.sketch, i, &ss.plane, proj, app.viewport.pixels_per_point()))
            };
            let parts: Vec<Vec<Pos2>> = sampled.parts.iter().map(|part| to2(part)).collect();
            let takes = if bx.crossing {
                parts.iter().any(|part| bx.takes(part))
            } else {
                sampled.quality == sketcraft_engine::render::SamplingQuality::Complete
                    && !parts.is_empty()
                    && parts.iter().all(|part| bx.takes(part))
            };
            if takes {
                out.push(Sel::SketchCurve { id: c.id.clone(), sketch: None });
            }
        }
        let visible = sketcraft_engine::view::SketchPointVisibility::new(&ss.sketch);
        for (i, p) in ss.sketch.points.iter().enumerate().filter(|_| !off("sketch") && !off("sketch_points")) {
            if accepts.is_none_or(|a| a & crate::selection::SKETCH_POINTS != 0)
                && i != 0
                && visible.takes(p)
                && bx.takes(&to2(&[ss.plane.to_world(p.pos)]))
            {
                out.push(Sel::SketchPoint { id: p.id.clone() });
            }
        }
        if !sketch_dialog && !off("dimensions") {
            for (id, r) in crate::dim_view::text_rects() {
                if bx.takes(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]) {
                    out.push(Sel::SketchConstraint { id });
                }
            }
        }
        if !sketch_dialog && !off("constraints") {
            for (id, c) in crate::sketch_tools::glyph_positions() {
                if bx.takes(&[c]) {
                    out.push(Sel::SketchConstraint { id });
                }
            }
        }
    } else {
        // With no dialog the marquee takes what a click takes: the bodies. A design whose only
        // feature is an imported SVG has no body to pick at all, so its regions are what there is
        // to select (#7058). A design that has a body keeps the old behaviour: the bodies alone.
        let dialog_accept = app.dialog.as_ref().and_then(|d| d.active_input()).map(|i| i.accept);
        let has_body = st.bodies.iter().any(|b| app.session.body_visible(&b.name));
        let accept = dialog_accept.unwrap_or(if has_body { BODIES } else { BODIES | PROFILES });
        // Bulk body selection follows the active component, just like its unfaded bodies.
        // Keep other dialogs and Move's reference inputs free to pick across components.
        let scoped_bodies =
            dialog_accept.unwrap_or(BODIES) == BODIES && app.dialog.as_ref().is_none_or(|d| matches!(d.kind, crate::dialogs::Kind::Move { .. }));
        let active_bodies = if scoped_bodies { active_body_names(app) } else { None };
        for b in &st.bodies {
            if !app.body_visible(&b.name) || (scoped_bodies && body_faded(app, b, &active_bodies)) {
                continue;
            }
            let m = b.mesh();
            if accept & EDGES != 0 {
                for (ei, e) in m.edges.iter().enumerate() {
                    let rim = m.circular_rim(ei);
                    let takes = if bx.crossing { bx.takes(&to2(e)) } else { rim.iter().all(|&i| m.edges.get(i).is_some_and(|p| bx.takes(&to2(p)))) };
                    if edge_pickable(app, &m, ei) && takes {
                        out.push(Sel::Edge { body: b.name.clone(), index: ei, point: polyline_mid(e) });
                    }
                }
            } else if accept & FACES != 0 {
                for f in 0..b.body.face_count() {
                    let mut pts: Vec<Vec3> = Vec::new();
                    for e in m.face_edges(u32::try_from(f).unwrap_or(u32::MAX)) {
                        pts.extend(m.edges.get(e).into_iter().flatten().copied());
                    }
                    let point = m
                        .triangles
                        .iter()
                        .zip(&m.tri_face)
                        .find(|(_, tf)| **tf as usize == f)
                        .and_then(|(t, _)| m.tri(t))
                        .map(|[a, bb, c]| (a + bb + c) / 3.0);
                    let takes = bx.takes(&to2(&pts))
                        && (bx.crossing
                            || m.surface_patch(f).iter().all(|&face| {
                                m.face_edges(u32::try_from(face).unwrap_or(u32::MAX))
                                    .iter()
                                    .all(|&e| m.edges.get(e).is_some_and(|p| bx.takes(&to2(p))))
                            }));
                    if let Some(point) = point
                        && takes
                    {
                        out.push(Sel::Face { body: b.name.clone(), index: f, point });
                    }
                }
            } else if accept & BODIES != 0 {
                let crossing_hit = m.edges.iter().any(|e| bx.takes(&to2(e)));
                let all_in = m.edges.iter().all(|e| bx.takes(&to2(e)));
                if (bx.crossing && crossing_hit) || (!bx.crossing && all_in && !m.edges.is_empty()) {
                    out.push(Sel::Body { name: b.name.clone() });
                }
            }
        }
        // The regions of the shown sketches. They are not bodies, and a design whose only
        // feature is an imported SVG has none: the regions are what there is to select (#7058).
        // They are not edges or faces either: a mask that asks for those never takes them.
        if accept & PROFILES != 0 && accept & (EDGES | FACES) == 0 {
            for sid in visible_sketches(app) {
                let Some(ss) = st.sketch(sid) else { continue };
                for (pi, p) in ss.profiles.iter().enumerate() {
                    let parts = crate::sketch_display::boundary_parts(&p.region.outer, &ss.plane, proj, app.viewport.pixels_per_point());
                    let takes = if bx.crossing {
                        parts.iter().any(|part| bx.takes(&to2(part)))
                    } else {
                        crate::sketch_display::profile_region(app, sid, pi, &ss.plane, proj).as_ref().as_ref().is_some_and(|display| {
                            let points: Vec<_> = display.outer.segs.iter().map(|segment| ss.plane.to_world(segment.start())).collect();
                            bx.takes(&to2(&points))
                        })
                    };
                    if takes {
                        out.push(Sel::Profile { sketch: sid, index: pi });
                    }
                }
            }
        }
    }
    let mut grouped = Vec::new();
    for item in out {
        for piece in grouped_selection(&app.session, &item) {
            if !grouped.iter().any(|x| crate::selection::same_item(x, &piece)) {
                grouped.push(piece);
            }
        }
    }
    let out = grouped;
    if let Some(mut d) = app.dialog.take() {
        d.take_box(&app.session, out, add);
        app.dialog = Some(d);
        return;
    }
    let items = serde_json::to_value(&out).unwrap_or_default();
    let _ = app.run("select.set", json!({"items": items, "add": add}));
}

/// Whether the origin point is drawn (the Origin shown and its point not hidden).
fn origin_point_shown(app: &SolveApp) -> bool {
    app.origin_visible() && !app.session.visibility.hidden_origin.iter().any(|h| h == "O")
}

/// Whether a body covers the origin along the view ray. The origin point is a flat dot painted
/// after the 3D scene, so the GPU cannot depth-test it the way it tests the axes and planes;
/// this asks the meshes the same question that test asks: is a nearer kept face in front of the
/// origin. The point then hides behind bodies under the same rule the axes and planes follow:
/// over them only while a dialog waits for a plane or the through-bodies preference is on (#10050).
fn origin_hidden_by_body(app: &SolveApp, proj: &Proj) -> bool {
    if app.picking_plane() || app.preferences.reference_geometry_through_bodies {
        return false;
    }
    // Wireframe draws no faces, so nothing hides the point, as for vertices and edges.
    if app.ui.visual_style == 2 {
        return false;
    }
    let Some(at) = proj.to_screen(Vec3::ZERO) else {
        return false;
    };
    let (o, d) = proj.ray(at);
    let t_origin = -o.dot(d);
    let st = crate::open_meshing::shown_world(app);
    let bounds = body_bounds(app, &st);
    // A section hides the material its normal points at: removed material does not hide the
    // point, as when picking a vertex behind a section (#6328).
    let section = section_plane(app);
    let kept = |p: Vec3| section.is_none_or(|(so, sn)| sn.dot(p) <= sn.dot(so) + crate::view_pick::SECTION_EPS);
    let walk_eps = 1e-6 * (1.0 + scene_radius(app));
    let slack = scene_pick_slack(app);
    for (i, b) in st.bodies.iter().enumerate() {
        if !app.body_visible(&b.name) {
            continue;
        }
        // Bodies whose box the ray misses cannot cover the origin.
        if let Some((_, bb)) = bounds.get(i)
            && ray_box(o, d, bb, bb.diagonal() * 1e-6 + 1e-9).is_none()
        {
            continue;
        }
        let (m, place) = b.mesh_in_place();
        let index = app.viewport.pick_index.of(&m);
        // The nearest kept face along the ray: one at or behind the origin does not hide it.
        let mut from = 0.0f64;
        for _ in 0..64 {
            let Some((lo, ld)) = local_ray(&place, o + d * from, d) else { break };
            let Some((dt, _)) = crate::pick_index::raycast(index.as_deref(), &m, lo, ld) else { break };
            let t = from + dt;
            if t >= t_origin - slack {
                break;
            }
            if kept(o + d * t) {
                return true;
            }
            from = t + walk_eps;
        }
    }
    false
}

/// Screen-space dots: the origin point, and hovered or selected body vertices.
fn points_2d(app: &SolveApp, painter: &egui::Painter, proj: &Proj) {
    let t = Tokens::get();
    if origin_point_shown(app)
        && !origin_hidden_by_body(app, proj)
        && let Some(o) = proj.to_screen(Vec3::ZERO)
    {
        painter.circle(o, 4.5, t.origin_point, Stroke::new(1.0, Color32::from_gray(110)));
    }
    for x in app.highlighted() {
        if let Sel::Vertex { point, .. } = x
            && let Some(p) = proj.to_screen(point)
        {
            painter.circle(p, 4.0, t.sel_vertex, Stroke::new(1.5, t.sel_edge));
        }
    }
    if let Some(Hit::Vertex { point, .. }) = &app.viewport.hover
        && let Some(p) = proj.to_screen(*point)
    {
        painter.circle(p, 4.5, t.hover_edge_halo, Stroke::new(1.5, t.hover_edge));
    }
}

/// A grid line's colour, as faint as the fade makes it.
/// The grid's minor colour blended towards its major one by `major` (0..1): a line two grid
/// levels share while they cross-fade, minor in one and major in the other (#9570).
fn grid_blend(minor: [u8; 4], major: [u8; 4], share: f32) -> [u8; 4] {
    let t = if share.is_finite() { f64::from(share.clamp(0.0, 1.0)) } else { 0.0 };
    std::array::from_fn(|i| (f64::from(minor[i]) + (f64::from(major[i]) - f64::from(minor[i])) * t).round().clamp(0.0, 255.0) as u8)
}

fn grid_colour(c: [u8; 4], alpha: f32) -> [u8; 4] {
    [c[0], c[1], c[2], (f64::from(c[3]) * f64::from(alpha)).round().clamp(0.0, 255.0) as u8]
}

/// Where the ground grid is centred: where the view's axis (`back`, through the camera's target
/// `tf`) meets the plane through the origin with normal `n`, so the middle of the view has grid
/// (#3087). At a grazing slant, return smoothly toward the point under the target: the actual
/// intersection runs beyond the visible grid and jumps to the other side at the horizon.
fn grid_centre(tf: Vec3, back: Vec3, n: Vec3, ext: f64) -> Vec3 {
    let height = tf.dot(n);
    let under = tf - n * height;
    let slant = back.dot(n);
    // Use the same minimum slant as grid_ext and GridFade. Above it, this is the view-axis
    // intersection; below it, the offset tends continuously to zero instead of infinity.
    let reach_slant = slant.abs().max(0.25);
    let along = (back - n * slant) * (-height * slant / (reach_slant * reach_slant));
    let len = along.len();
    if len > ext && len > 0.0 { under + along * (ext / len) } else { under + along }
}

/// Hover feedback for the active sketch's points and curves (drawn on top of everything).
fn hover_highlight(app: &SolveApp, painter: &egui::Painter, proj: &Proj) {
    // While a dimension's value is typed the open sketch is drawn at that value, but hover follows
    // the design: a highlight there would trace a curve where none is drawn. Other sketches are
    // drawn where they are.
    let previewed = crate::dim_view::previewed_sketch(app);
    let col = Tokens::get().hover_profile_edge;
    let st = crate::open_meshing::shown_world(app);
    let hover = app.viewport.hover.as_ref().filter(|h| {
        !matches!(h, Hit::SketchCurve { sketch, .. } | Hit::SketchMidpoint { sketch, .. } | Hit::SketchPoint { sketch, .. } if previewed == Some(*sketch))
    });
    match hover {
        // Trim shows the piece it would take away instead (`sketch_tools::preview`, #3052).
        Some(Hit::SketchCurve { sketch, id, .. }) if crate::sketch_tools::trim_hover_piece(app, *sketch, id).is_some() => {}
        Some(Hit::SketchCurve { sketch, id, .. }) => {
            if let Some(ss) = st.sketch(*sketch)
                && let Some(ci) = ss.sketch.curve_index(id)
            {
                for part in &crate::sketch_display::curve_sample(app, *sketch, ci, &ss.plane, proj).parts {
                    let mode = ss.sketch.curves.get(ci).map_or(0, |c| if c.centerline { 2 } else { u8::from(c.construction) });
                    crate::tools::paint_sketch_path(part, mode, painter, proj, Stroke::new(3.0, col));
                }
            }
        }
        Some(Hit::SketchMidpoint { sketch, at, .. }) => {
            if let Some(ss) = st.sketch(*sketch)
                && let Some(p) = proj.to_screen(ss.plane.to_world(*at))
            {
                midpoint_marker(painter, p, col);
            }
        }
        Some(Hit::SketchPoint { sketch, at, .. }) => {
            if let Some(ss) = st.sketch(*sketch)
                && let Some(p) = proj.to_screen(ss.plane.to_world(*at))
            {
                painter.circle_stroke(p, 6.0, Stroke::new(2.0, col));
            }
        }
        _ => {}
    }
    // The centre of the circle or arc hovered last: a cross in a ring, where a click takes it.
    if let Some((sketch, id)) = &app.viewport.woken_centre
        && previewed != Some(*sketch)
        && !matches!(&app.viewport.hover, Some(Hit::SketchPoint { id: h, .. }) if h == id)
        && let Some(ss) = st.sketch(*sketch)
        && let Some(at) = ss.sketch.resolve_point(id).and_then(|i| ss.sketch.point(i))
        && let Some(p) = proj.to_screen(ss.plane.to_world(at))
    {
        let stroke = Stroke::new(1.5, col);
        painter.circle_stroke(p, 5.0, stroke);
        painter.line_segment([p - vec2(4.0, 0.0), p + vec2(4.0, 0.0)], stroke);
        painter.line_segment([p - vec2(0.0, 4.0), p + vec2(0.0, 4.0)], stroke);
        crate::scenario::publish_handle("woken_centre", p);
    }
}

/// How near the pointer must come to the woken centre of a circle or arc (screen points),
/// against 8 for other sketch points.
const WOKEN_CENTRE_PX: f32 = 16.0;

/// Hovering a circle's or an arc's edge in the sketch being edited wakes its centre: drawn, and
/// picked from further away, so a line or a dimension is easily taken to it, and it stays awake
/// while the pointer crosses the circle's inside to it. Hovering another curve or point (but
/// that centre) puts it back to sleep, as does leaving the sketch.
fn wake_centre(app: &mut SolveApp, hits: &[Hit]) {
    let Some(active) = app.session.active_sketch else {
        app.viewport.woken_centre = None;
        return;
    };
    let first = hits.iter().find(|h| matches!(h, Hit::SketchPoint { sketch, .. } | Hit::SketchCurve { sketch, .. } if *sketch == active));
    let st = crate::open_meshing::shown_world(app);
    match first {
        Some(Hit::SketchCurve { id, .. }) => {
            let centre = st.sketch(active).and_then(|ss| {
                let sk = &ss.sketch;
                let c = match sk.curves.get(sk.curve_index(id)?)?.kind {
                    CurveKind::Circle { c, .. } | CurveKind::Arc { c, .. } => c,
                    _ => return None,
                };
                Some(sk.points.get(c)?.id.clone())
            });
            app.viewport.woken_centre = centre.map(|id| (active, id));
        }
        Some(Hit::SketchPoint { id, .. }) if app.viewport.woken_centre.as_ref().is_some_and(|(_, w)| w == id) => {}
        Some(_) => app.viewport.woken_centre = None,
        None => {
            if app.viewport.woken_centre.as_ref().is_some_and(|(k, _)| *k != active) {
                app.viewport.woken_centre = None;
            }
        }
    }
}

/// A midpoint stays attached to its edge; hover and selection share the same marker.
fn midpoint_marker(painter: &egui::Painter, at: Pos2, color: Color32) {
    let triangle = vec![at + vec2(0.0, -6.0), at + vec2(6.0, 5.0), at + vec2(-6.0, 5.0)];
    painter.add(Shape::closed_line(triangle, Stroke::new(2.0, color)));
    painter.text(at + vec2(10.0, -8.0), Align2::LEFT_BOTTOM, "MID", FontId::proportional(Tokens::get().font_button), color);
}

/// The standalone points of the finished sketches shown (placed with Point, not a curve's
/// ends or centres), in world space: they can still be picked (a hole's centre), so they are
/// drawn too (#3267). Not the sketch's origin, projected points, or a sketch's points while it
/// hides them.
pub(crate) fn standalone_points(app: &SolveApp) -> Vec<Vec3> {
    let st = crate::open_meshing::session_world(&app.session);
    let mut out = Vec::new();
    for sid in visible_sketches(app) {
        if app.session.active_sketch == Some(sid) {
            continue;
        }
        let Some(ss) = st.sketch(sid) else { continue };
        let sk = &ss.sketch;
        if sk.view.hide_points {
            continue;
        }
        let used: std::collections::HashSet<usize> = sk.curves.iter().flat_map(|c| c.kind.point_ids()).collect();
        let visible = sketcraft_engine::view::SketchPointVisibility::new(sk);
        for (i, p) in sk.points.iter().enumerate().skip(1) {
            if !used.contains(&i) && p.link.is_none() && visible.takes(p) {
                out.push(ss.plane.to_world(p.pos));
            }
        }
    }
    out
}

/// Sketch points and the status of the active sketch (constraint glyphs: `sketch_tools`).
/// The point the 3D mouse is turning the view around, as a faint ball, so the user sees what the
/// turn holds still (#7435). Lighter, smaller discs stacked toward the top left give it its shading.
fn paint_device_pivot(app: &SolveApp, painter: &egui::Painter, proj: &Proj) {
    let Some(p) = app.viewport.device_pivot.and_then(|p| proj.to_screen(p)) else { return };
    let t = Tokens::get();
    painter.circle(p, 8.0, t.accent.gamma_multiply(0.18), Stroke::new(1.0, t.accent.gamma_multiply(0.5)));
    painter.circle_filled(p + vec2(-1.5, -1.5), 5.0, t.accent.gamma_multiply(0.2));
    painter.circle_filled(p + vec2(-2.5, -2.5), 2.5, t.accent.gamma_multiply(0.3));
}

fn overlays(app: &SolveApp, painter: &egui::Painter, proj: &Proj) {
    let t = Tokens::get();
    let st = crate::open_meshing::shown_world(app);
    for item in &app.session.selection {
        if let Sel::ExternalSketchPoint { sketch, id } = item
            && let Some(ss) = st.sketch(*sketch)
            && let Some(point) = ss.sketch.resolve_point(id).and_then(|i| ss.sketch.point(i))
            && let Some(pos) = proj.to_screen(ss.plane.to_world(point))
        {
            painter.circle_stroke(pos, 6.0, Stroke::new(2.0, t.sel_edge));
        }
    }
    for p in standalone_points(app) {
        if let Some(sp) = proj.to_screen(p) {
            painter.rect_filled(Rect::from_center_size(sp, vec2(4.0, 4.0)), 0.0, t.sketch);
        }
    }
    let Some(sid) = app.session.active_sketch else { return };
    // While a dimension's value is typed, its points where that value puts them.
    let st = crate::dim_view::preview_world(app).unwrap_or(st);
    let Some(ss) = st.sketch(sid) else { return };
    let sk = &ss.sketch;
    let locked = sk.locked_points();
    let conflict = Conflict::of(app, sk, &ss.report);
    let virtual_corners: Vec<usize> = sk.virtual_corners().into_iter().map(|(i, _)| i).collect();
    let visible = sketcraft_engine::view::SketchPointVisibility::new(sk);
    let selected: std::collections::HashSet<_> = app
        .highlighted()
        .iter()
        .filter_map(|item| crate::sketch_modify_ui::point_id(&app.session, item).and_then(|id| sk.resolve_point(id)))
        .collect();
    for (i, p) in sk.points.iter().enumerate() {
        if !visible.takes(p) {
            continue;
        }
        let hidden = (sk.view.hide_points && i != 0) || (sk.view.hide_projected && p.link.is_some());
        let Some(sp) = proj.to_screen(ss.plane.to_world(p.pos)).filter(|_| !hidden) else { continue };
        let det = ss.report.point_determined.get(i).copied().unwrap_or(false);
        let c = if conflict.point(i) {
            t.sketch_conflict
        } else if p.link.is_none() && locked.get(i).copied().unwrap_or(false) {
            t.sketch_locked
        } else if det {
            t.sketch_point
        } else {
            t.sketch_point_free
        };
        if i == 0 {
            painter.circle(sp, 3.5, Color32::from_rgb(240, 200, 60), Stroke::new(1.0, t.body_edge));
        } else if virtual_corners.binary_search(&i).is_ok() {
            // A virtual sharp corner: an open square, it is not on any drawn curve.
            painter.rect_stroke(Rect::from_center_size(sp, vec2(6.0, 6.0)), 0.0, Stroke::new(1.2, c), egui::StrokeKind::Middle);
        } else {
            painter.rect_filled(Rect::from_center_size(sp, vec2(5.0, 5.0)), 0.0, c);
        }
        if selected.contains(&i) {
            painter.circle_stroke(sp, 6.0, Stroke::new(2.0, t.sel_edge));
        }
    }
    // The markers and the status line are drawn straight onto the view: its side's colours (#4146).
    let v = Tokens::view();
    for selected in &app.session.selection {
        if let Sel::SketchMidpoint { id } = selected
            && let Some(line) = sk.curve_index(id)
            && let Some(at) = crate::selection::line_midpoint(sk, line)
            && let Some(screen) = proj.to_screen(ss.plane.to_world(at))
        {
            midpoint_marker(painter, screen, v.sel_edge);
        }
    }
    loose_end_markers(sk, &ss.ends, &ss.plane, painter, proj, &v);
    // Sketch status, unless the status bar shows it (#7243).
    if let Some(r) = app.viewport.layout.as_ref().map(|l| l.content).filter(|_| !crate::status_bar::shown(app)) {
        let status = if !ss.report.ok() {
            crate::i18n::tr("sketch.status_conflict").to_string()
        } else if !crate::sketch_palette::has_geometry(sk) {
            crate::i18n::tr("ui.no_geometry_yet").to_string()
        } else if ss.report.dof == 0 {
            crate::i18n::tr("sketch.status_fully_constrained").to_string()
        } else if ss.report.dof == 1 {
            crate::i18n::tr("ui.1_degree_of_freedom").to_string()
        } else {
            crate::i18n::format("sketch.degrees_of_freedom", &[("count", &ss.report.dof.to_string())])
        };
        let msg = format!("{} — {status}", crate::i18n::tr_name(&ss.name));
        let c = if ss.report.ok() { v.sketch_accent } else { v.error };
        painter.text(pos2(r.left() + 12.0, r.top() + 12.0), Align2::LEFT_TOP, msg, FontId::proportional(t.font_button), c);
    }
}

/// The curves and points of a sketch that its failing constraints hold (the solve's `failing`:
/// the constraints still violated after a failed solve). They are drawn in `sketch_conflict`
/// until the conflict is gone (#7244); redundant constraints that hold are not failing. None
/// while the Sketch preference `sketch_conflict_colour` is off.
#[derive(Default)]
pub(crate) struct Conflict {
    curves: Vec<bool>,
    points: Vec<bool>,
}

impl Conflict {
    pub(crate) fn of(app: &SolveApp, sk: &sketcraft_engine::sketch::Sketch, report: &sketcraft_engine::sketch::SolveReport) -> Conflict {
        let mut out = Conflict::default();
        if !app.preferences.sketch_conflict_colour || report.failing.is_empty() {
            return out;
        }
        out.curves = vec![false; sk.curves.len()];
        out.points = vec![false; sk.points.len()];
        for c in sk.constraints.iter().filter(|c| report.failing.contains(&c.id)) {
            let (points, curves) = c.kind.indices();
            for (held, of) in [(points, &mut out.points), (curves, &mut out.curves)] {
                for i in held {
                    if let Some(slot) = of.get_mut(i) {
                        *slot = true;
                    }
                }
            }
        }
        out
    }

    pub(crate) fn curve(&self, i: usize) -> bool {
        self.curves.get(i).copied().unwrap_or(false)
    }

    pub(crate) fn point(&self, i: usize) -> bool {
        self.points.get(i).copied().unwrap_or(false)
    }

    /// A failing constraint's glyph or dimension: `sketch_conflict`, or with the preference off
    /// the error red it always had.
    pub(crate) fn glyph_colour(app: &SolveApp, t: &Tokens) -> Color32 {
        if app.preferences.sketch_conflict_colour { t.sketch_conflict } else { t.error }
    }
}

/// Where a profile can fail: a ring round each open end (it touches no other curve) and a
/// diamond round each untied one (it touches another curve only by position), told apart by
/// shape. Both are warnings, not errors: an open sketch (a sweep path, an axis) is valid.
fn loose_end_markers(
    sk: &sketcraft_engine::sketch::Sketch,
    ends: &sketcraft_engine::sketch::EndReport,
    plane: &solidcore_record::Plane,
    painter: &egui::Painter,
    proj: &Proj,
    t: &Tokens,
) {
    let screen = |i: usize| sk.point(i).and_then(|p| proj.to_screen(plane.to_world(p)));
    for sp in ends.open.iter().filter_map(|&i| screen(i)) {
        painter.circle_stroke(sp, 7.0, Stroke::new(1.8, t.warning));
    }
    for sp in ends.untied.iter().filter_map(|u| screen(u.point)) {
        let r = 7.5;
        let diamond = vec![pos2(sp.x, sp.y - r), pos2(sp.x + r, sp.y), pos2(sp.x, sp.y + r), pos2(sp.x - r, sp.y)];
        painter.add(egui::Shape::closed_line(diamond, Stroke::new(1.8, t.warning)));
    }
}

/// The colours of a headless picture of the model (`ui.render`, the agent's view image) in the
/// dark or the light theme and a colour vision mode, from the tokens the window draws with, so the
/// picture matches the window. The light theme's tokens (Off) are the engine's own colours, so a light
/// render is unchanged.
pub(crate) fn scene_palette(dark: bool, mode: crate::theme::ColorVision) -> sketcraft_engine::view::Palette {
    let t = if dark == crate::theme::viewport_dark() && mode == crate::theme::vision() { Tokens::view() } else { Tokens::themed(dark, mode) };
    let rgb = |c: Color32| Rgb(c.r(), c.g(), c.b());
    sketcraft_engine::view::Palette {
        background: (rgb(t.viewport_top), rgb(t.viewport_bottom)),
        grid: rgb(t.grid),
        grid_major: rgb(t.grid_major),
        edge: rgb(t.body_edge),
        body: rgb(t.body),
        body_selected: rgb(t.body_selected),
        sketch: rgb(t.sketch),
        sketch_fixed: rgb(t.sketch_fixed),
        sketch_done: rgb(t.sketch_fixed),
        sketch_locked: rgb(t.sketch_locked),
        sketch_projected: rgb(t.sketch_projected),
        sketch_lost: rgb(t.sketch_lost),
        sketch_construction: rgb(t.sketch_construction),
        sketch_centerline: rgb(t.sketch_centerline),
        axes: t.axes().map(rgb),
        cap: rgb(t.section_cap),
        hatch: t.section_hatch.map(rgb),
        isocurve: rgb(t.accent),
        analysis: t.analysis,
    }
}

pub(crate) fn refresh_layout(app: &mut SolveApp, ctx: &egui::Context) {
    if let Some(rect) = app.viewport.rect {
        if app.universal_panel_frame().is_some() {
            let Ok(usable) = crate::shell::host_usable(app) else {
                // Refuse new control placement without inventing an occupied
                // viewport. An early pair already painted remains authoritative.
                app.viewport.layout = Some(crate::viewport_layout::Layout {
                    viewport: rect,
                    content: rect,
                    drawable: Rect::NOTHING,
                    middle: Rect::NOTHING,
                    navigation: Rect::NOTHING,
                    axis_view: Rect::NOTHING,
                    confirm: app.viewport.host_confirm.unwrap_or(Rect::NOTHING),
                    usable: None,
                });
                return;
            };
            let navigation = if crate::navigation_bar::shown(app) { crate::navigation_bar_view::measured(app, ctx, rect).rect } else { Rect::NOTHING };
            // No room for a view navigator the SketchUp model screen does not draw.
            let orbit = crate::navigation_bar::canvas_caps(app).orbit && crate::view_gizmo::visible(app);
            app.viewport.layout = Some(if let Some(confirm) = app.viewport.host_confirm {
                crate::viewport_layout::Layout::host_with_retained_confirm(rect, &usable, app.preferences.navigator_style, confirm, navigation, orbit)
            } else if orbit {
                crate::viewport_layout::Layout::host(rect, &usable, app.preferences.navigator_style, crate::view_confirm::size(app), navigation)
            } else {
                crate::viewport_layout::Layout::host_with_axis(
                    rect,
                    &usable,
                    app.preferences.navigator_style,
                    crate::view_confirm::size(app),
                    navigation,
                    false,
                )
            });
            return;
        }
        let docks = crate::shell::fit_rects(app);
        let mut content = crate::shell::content_rect(app, rect);
        if crate::render_ui::focused(app) {
            // Focus releases dock reservations while keeping controls below the stage bar.
            content = rect;
            for card in &app.shell.cards {
                content.min.y = crate::shell::within(card.bottom() + Tokens::get().space_8, content.top(), content.bottom());
            }
        }
        // The full-height Assistant reserves control space, while rendering and picking
        // keep the original viewport projection. Ordinary feature dialogs do not move controls.
        if app.ui.layout == crate::shell::LayoutMode::Floating
            && crate::agent_chat::shown(app)
            && app.agent_chat.open
            && !app.preferences.agent_chat_popped
            && !app.preferences.agent_chat_minimized
            && let Some(chat) = app.agent_chat.panel_rect.filter(|r| r.is_finite() && r.is_positive())
        {
            if app.preferences.agent_chat_side == "left" {
                content.min.x = crate::shell::within(chat.right() + crate::shell::SHELL_GAP, content.left(), content.right());
            } else {
                content.max.x = crate::shell::within(chat.left() - crate::shell::SHELL_GAP, content.left(), content.right());
            }
        }
        // OK and Cancel in the view's corner (#6709) take the top right; the axis view goes below.
        let corner = crate::view_confirm::size(app);
        let style = app.preferences.navigator_style;
        let layout = if crate::render_workspace::active(app) {
            let navigation = if crate::navigation_bar::shown(app) { crate::navigation_bar_view::measured(app, ctx, content).rect } else { Rect::NOTHING };
            if app.preferences.render_inspector_side == "left" {
                crate::viewport_layout::Layout::product_left(rect, content, &docks, ctx.pixels_per_point(), style, corner, navigation)
            } else {
                crate::viewport_layout::Layout::product(rect, content, &docks, ctx.pixels_per_point(), style, corner, navigation)
            }
        } else {
            let content = if app.ui.layout == crate::shell::LayoutMode::Docked { rect } else { content };
            let navigation = if crate::navigation_bar::shown(app) { crate::navigation_bar_view::measured(app, ctx, content).rect } else { Rect::NOTHING };
            crate::viewport_layout::Layout::with_content(rect, content, &docks, ctx.pixels_per_point(), style, corner, navigation)
        };
        // Cache the exact product camera projection as well as its Fit room.
        // Controls retain their outer content/axis/navigation rectangles.
        let mut layout = if crate::render_workspace::active(app) && !app.preferences.render_legacy_image_defaults {
            crate::render_workspace::picture_layout(app, &layout)
        } else {
            layout
        };
        // The SketchUp model screen draws no view navigator: nothing keeps its corner.
        if !crate::view_gizmo::visible(app) {
            layout.axis_view = Rect::NOTHING;
        }
        app.viewport.layout = Some(layout);
    }
}

/// The opt-in host paints its existing controls after resolving actual Area
/// visibility. This does not render the scene or call navigation::handle again.
pub(crate) fn show_host_controls(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    refresh_layout(app, ui.ctx());
    if crate::navigation_bar::canvas_caps(app).orbit {
        axis_view(app, ui, rect);
        if !crate::render_workspace::active(app) {
            crate::view_angles::show(app, ui, rect);
        }
    }
    crate::navigation_bar_view::show(app, ui, rect);
}

/// Orientation controls share the layout reservation used by model fitting.
fn axis_center(app: &SolveApp, rect: Rect) -> Pos2 {
    let c = axis_rect(app, rect).center();
    // The compass sits a little higher in its reserved box (less margin above, the same on the
    // right); the box itself is unchanged.
    if app.preferences.navigator_style == crate::prefs::NavigatorStyle::Axes { c - vec2(0.0, AXIS_CENTER_LIFT) } else { c }
}

/// How far the signed-axis compass is lifted inside its reserved box.
pub(crate) const AXIS_CENTER_LIFT: f32 = 4.0;

pub(crate) fn axis_rect(app: &SolveApp, rect: Rect) -> Rect {
    if !crate::view_gizmo::visible(app) {
        return Rect::NOTHING;
    }
    app.viewport.layout.as_ref().filter(|l| l.viewport == crate::render_workspace::camera_rect(app, rect)).map(|l| l.axis_view).unwrap_or_else(|| {
        let size = axis_view_size(app.preferences.navigator_style);
        Rect::from_min_size(pos2(rect.right() - size, rect.top()), vec2(size, size))
    })
}

const AXIS_LENGTH: f64 = 38.0;
/// The artwork sits inside the pointer reach so the compass keeps breathing room in its circle.
const AXIS_ART_SCALE: f64 = 0.84;
use crate::icons::navigation::AXIS_RADIUS;

/// Six signed world axes, sorted back to front so overlapping tips have one owner.
fn axis_tips(cam: &Camera, c: Pos2) -> Vec<(&'static str, Vec3, Pos2, f64)> {
    let (r, u, b) = cam.basis();
    let mut tips: Vec<_> = [("x", Vec3::X), ("-x", -Vec3::X), ("y", Vec3::Y), ("-y", -Vec3::Y), ("z", Vec3::Z), ("-z", -Vec3::Z)]
        .into_iter()
        .map(|(name, d)| {
            (name, d, c + vec2((d.dot(r) * AXIS_LENGTH * AXIS_ART_SCALE) as f32, -(d.dot(u) * AXIS_LENGTH * AXIS_ART_SCALE) as f32), d.dot(b))
        })
        .collect();
    tips.sort_by(|a, b| a.3.total_cmp(&b.3));
    tips
}

fn axis_hit(tips: &[(&str, Vec3, Pos2, f64)], p: Pos2) -> Option<usize> {
    tips.iter().enumerate().rev().find(|(_, (_, _, at, _))| at.distance(p) <= AXIS_RADIUS).map(|(i, _)| i)
}

/// Dragging anywhere on the axis gizmo orbits about the view centre. A stationary press
/// on a tip chooses its view. Floating windows own their input over the gizmo.
fn axis_view_orbit(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) -> bool {
    let (pressed, down, decided, origin, pos, shift) = ui.input(|i| {
        let p = &i.pointer;
        (p.primary_pressed(), p.primary_down(), p.is_decidedly_dragging(), p.press_origin(), p.interact_pos(), i.modifiers.shift)
    });
    if pressed && ui.is_enabled() && ui.rect_contains_pointer(rect) && !pos.is_some_and(|p| crate::shell::covered(app, p)) {
        let shown = crate::view_gizmo::shown(app);
        app.viewport.axis_drag = origin.filter(|p| match shown.style {
            crate::prefs::NavigatorStyle::Axes => axis_rect(app, rect).contains(*p),
            crate::prefs::NavigatorStyle::Custom if shown.shape.is_some() => body_gizmo::drag_origin(app, rect, *p),
            crate::prefs::NavigatorStyle::Cube | crate::prefs::NavigatorStyle::Custom => {
                navigation_cube::drag_origin(app, axis_center(app, rect), *p, axis_rect(app, rect).width() / navigation_cube::DESIGN_SIZE)
            }
        });
        app.viewport.axis_warp = None;
        if app.viewport.axis_drag.is_some() {
            app.viewport.orbit_free = None;
        }
    }
    if !down && app.viewport.axis_drag.take().is_some() {
        app.viewport.axis_warp = None;
        app.viewport.orbit_free = None;
    }
    let Some(last) = app.viewport.axis_drag.filter(|_| decided) else { return false };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if let Some(p) = pos {
        app.cancel_view_animation();
        // While a warp is pending the pointer may still sit at the edge it hit (the OS has not
        // echoed the warp, or cannot), have landed at the centre (the echo), or have moved past
        // it (the echo plus a fast flick): measure each step from whichever reference the
        // pointer is nearer to, so the view never jumps by the warp distance and a platform
        // that ignores warps keeps orbiting from the real pointer instead of freezing.
        let from = match app.viewport.axis_warp {
            Some(w) if p.distance(w) <= p.distance(last) => {
                app.viewport.axis_warp = None;
                w
            }
            _ => last,
        };
        orbit_drag(app, rect, f64::from(p.x - from.x), f64::from(p.y - from.y), None, shift, false);
        app.viewport.axis_drag = Some(p);
        // The OS stops feeding movement once the cursor is pinned at the screen edge, so an
        // orbit toward that edge would stall: warp the cursor back to the window centre and
        // measure the next steps from there, keeping the orbit tracking while the button is held (#9943).
        let screen = ui.ctx().viewport_rect();
        let edge = 6.0;
        if app.viewport.axis_warp.is_none()
            && (p.x <= screen.min.x + edge || p.x >= screen.max.x - edge || p.y <= screen.min.y + edge || p.y >= screen.max.y - edge)
        {
            let home = screen.center();
            app.viewport.axis_warp = Some(home);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CursorPosition(home));
        }
        ui.ctx().request_repaint();
    }
    true
}

/// The whole navigator's box takes the right-click for the navigator's menu, before the view
/// under it would (its controls are drawn over it, and open the same menu themselves).
fn gizmo_menu(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    let outer = axis_rect(app, rect);
    if !outer.is_positive() || !outer.is_finite() {
        return;
    }
    // Read the pointer rather than add a widget over the navigator, which would take the
    // primary clicks the viewport owns there when orbiting is off.
    if ui.is_enabled()
        && ui.input(|i| i.pointer.secondary_clicked())
        && app.dialog.is_none()
        && let Some(at) = ui.input(|i| i.pointer.interact_pos())
        && ui.rect_contains_pointer(outer)
        && view_gizmo_at(app, at)
    {
        crate::context_menu::open_for(app, at, crate::context_menu::Target::ViewGizmo);
    }
}

/// The view gizmo (whichever type is shown) is under `p`: a right-click there opens its menu.
pub(crate) fn view_gizmo_at(app: &SolveApp, p: Pos2) -> bool {
    app.viewport.rect.is_some_and(|rect| {
        let outer = axis_rect(app, rect);
        outer.is_positive() && outer.contains(p) && !crate::shell::covered(app, p)
    })
}

pub(crate) fn axis_view(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    if !crate::view_gizmo::visible(app) {
        app.viewport.axis_drag = None;
        crate::scenario::publish_count("axis_view_highlights", 0.0);
        return;
    }
    gizmo_menu(app, ui, rect);
    let shown = crate::view_gizmo::shown(app);
    match shown.style {
        crate::prefs::NavigatorStyle::Axes => signed_axis_view(app, ui, rect),
        crate::prefs::NavigatorStyle::Custom if shown.shape.is_some() => body_gizmo::show(app, ui, rect, shown),
        crate::prefs::NavigatorStyle::Cube | crate::prefs::NavigatorStyle::Custom => navigation_cube::show(app, ui, rect),
    }
}

fn signed_axis_view(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    if !axis_rect(app, rect).is_positive() || !axis_rect(app, rect).is_finite() {
        app.viewport.axis_drag = None;
        crate::scenario::publish_count("axis_view_highlights", 0.0);
        return;
    }
    crate::scenario::publish_rect("axis_view:outer", axis_rect(app, rect));
    let t = Tokens::get();
    let c = axis_center(app, rect);
    let orbiting = axis_view_orbit(app, ui, rect);
    let tips = axis_tips(&app.cam, c);
    let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| ui.is_enabled() && ui.rect_contains_pointer(rect) && !crate::shell::covered(app, *p));
    let hovered = if orbiting { None } else { hover.and_then(|p| axis_hit(&tips, p)) };
    let painter = ui.painter_at(rect);
    // Anywhere in the compass circle drags the view; the circle turns fully opaque under the pointer.
    let in_circle = hover.is_some_and(|p| p.distance(c) <= 49.5);
    let fill = if in_circle || orbiting { 1.0 } else { 0.35 };
    painter.circle(c, 49.5, t.overlay.gamma_multiply(fill), Stroke::new(1.0, t.border.gamma_multiply(0.75)));
    for (i, (name, d, at, depth)) in tips.iter().enumerate() {
        if i == 3 {
            // The origin dot sits behind the three front axes and over the three back ones.
            painter.circle(c, 3.6, t.text.gamma_multiply(0.9), Stroke::new(1.4, t.panel));
        }
        crate::icons::navigation::axis_tip(&painter, c, name, d, *at, *depth, hovered == Some(i), t);
        crate::scenario::publish_handle(&format!("axis_view:{name}"), *at);
    }
    crate::scenario::publish_count("axis_view_highlights", f64::from(u8::from(hovered.is_some())));
    if let Some(i) = hovered
        && let Some((name, d, _, _)) = tips.get(i)
    {
        crate::scenario::publish_handle(&format!("axis_view_hover:{name}"), hover.unwrap_or(c));
        ui.interact(Rect::from_center_size(hover.unwrap_or(c), vec2(2.0, 2.0)), ui.scope_id().with("axis_view_tip"), Sense::hover())
            .backing_on_hover_text(format!(
                "View from {}. Click again to reverse. Drag to orbit{}.",
                name.to_uppercase(),
                if app.preferences.free_orbit { "" } else { "; hold Shift to snap" }
            ));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if ui.input(|i| i.pointer.primary_clicked()) {
            let frame = app.cam.to_frame(*d);
            let dir = if app.cam.back().dot(*d) > 0.9999 { -*d } else { *d };
            let mut to = app.cam.looking_from(dir);
            if frame.z.abs() > 0.5 {
                to.yaw = nearest_quarter_turn(app.cam.yaw);
            }
            app.animate_turn(to);
        }
    }
    let home = Rect::from_center_size(pos2(c.x - 37.0, c.y - 37.0), vec2(18.0, 18.0));
    crate::scenario::publish_target("axis_view:home", home);
    let hr = ui.interact(home, ui.scope_id().with("axis_view_home"), Sense::click());
    // The home button sits in its own small round chip.
    painter.circle(home.center(), 12.0, t.overlay.gamma_multiply(if hr.hovered() { 0.9 } else { 0.5 }), Stroke::new(1.0, t.border));
    icons::paint_icon(&painter, home, "home", f32::from(t.icon.a()) / 255.0);
    // Cursors: a grabbing hand while orbiting, an open hand over the circle, a pointer on the home
    // chip (the axis ends keep their pointing hand).
    if hr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if orbiting {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if in_circle && hovered.is_none() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if hr.backing_on_hover_text(crate::i18n::tr_label("Home view")).clicked() {
        app.animate_view("home");
    }
}

/// At a halfway tie, prefer the square heading nearer the standard plan view.
pub(crate) fn nearest_quarter_turn(yaw: f64) -> f64 {
    if !yaw.is_finite() {
        return 0.0;
    }
    let q = std::f64::consts::FRAC_PI_2;
    let lo = (yaw / q).floor() * q;
    let hi = lo + q;
    let (dlo, dhi) = (yaw - lo, hi - yaw);
    if (dlo - dhi).abs() < 1e-6 {
        let off_front = |a: f64| ((a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).abs();
        if off_front(lo) <= off_front(hi) { lo } else { hi }
    } else if dlo < dhi {
        lo
    } else {
        hi
    }
}

/// Turn the view to face the selected planar face or plane, else the active sketch; asked again
/// while it already faces it, from the other side (#1886).
pub fn look_at_selection(app: &mut SolveApp) {
    if sketcraft_engine::find_command("view.normal_to").is_some_and(|command| command.info(&app.session).disabled_reason.is_none()) {
        app.start("view.normal_to");
    } else {
        crate::dialogs::look_at_sketch_again(app);
    }
}

#[cfg(test)]
#[path = "viewport_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "view_home_tests.rs"]
mod view_home_tests;

#[cfg(test)]
#[path = "viewport_grid_plane_tests.rs"]
mod grid_plane_tests;

#[cfg(test)]
mod standalone_point_tests {
    use super::*;

    /// #3267: a finished sketch's standalone points stay drawn (they can be picked for a hole);
    /// its curves' ends are not added, the open sketch draws its own points, and a hidden
    /// sketch draws none.
    #[test]
    fn a_finished_sketch_shows_its_standalone_points() {
        let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [20, 10]})).unwrap();
        app.run("sketch.point", json!({"point": [5, 4]})).unwrap();
        assert!(standalone_points(&app).is_empty(), "the open sketch draws its own points");
        app.run("sketch.finish", json!({})).unwrap();
        let points = standalone_points(&app);
        assert_eq!(points.len(), 1, "only the placed point, not the rectangle's corners: {points:?}");
        assert!(points[0].dist(Vec3::new(5.0, 4.0, 0.0)) < 1e-9, "{points:?}");
        app.session.visibility.hidden_folders.push("c0/sketches".into());
        assert!(standalone_points(&app).is_empty(), "sketches hidden");
    }
}

#[cfg(test)]
mod feature_highlight_tests {
    use super::*;

    /// A feature's faces take a new name when a later Offset Face moves them again; selecting
    /// the feature still lights the faces it changed.
    #[test]
    fn a_feature_whose_face_was_renamed_later_still_lights_it() {
        let mut s = sketcraft_engine::Session::default();
        s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20, "body_name": "Base"})).unwrap();
        let first =
            s.execute("solid.offset_face", &json!({"faces": [[20, 15, 20]], "distance": 5, "name": "Raise"})).unwrap()["feature"].as_u64().unwrap();
        s.execute("solid.offset_face", &json!({"faces": [[20, 15, 25]], "distance": 2, "name": "RaiseAgain"})).unwrap();
        let app = SolveApp::new(s, crate::Services::default());
        let parts = feature_parts(&app, first, true);
        assert!(parts.iter().any(|p| matches!(p, Sel::Face { .. })), "{parts:?}");
    }

    /// A joined tab that made the body bigger lights its own faces, not round walls of the plate
    /// 12 mm away: the join reports no history, and the bigger body meshes those walls differently.
    #[test]
    fn a_joined_tab_does_not_light_round_walls_it_never_touched() {
        let mut s = sketcraft_engine::Session::default();
        let reach = 57.0_f64.sqrt(); // Where a circle of radius 11 meets y = ±8.
        s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
        s.execute("sketch.rectangle.two_point", &json!({"p0": [-20, -8], "p1": [20, 8]})).unwrap();
        s.execute("sketch.arc.three_point", &json!({"start": [-reach, 8], "end": [reach, 8], "through": [0, 11]})).unwrap();
        s.execute("sketch.arc.three_point", &json!({"start": [-reach, -8], "end": [reach, -8], "through": [0, -11]})).unwrap();
        s.execute("sketch.finish", &json!({})).unwrap();
        s.execute("solid.extrude", &json!({"distance": 10})).unwrap();
        s.execute("sketch.create", &json!({"plane": {"face": [20, 0, 5]}, "project_edges": false})).unwrap();
        s.execute("sketch.line", &json!({"points": [[-8, -5], [8, -5], [8, 23], [-8, 23], [-8, -5]]})).unwrap();
        s.execute("sketch.circle.center", &json!({"center": [0, 23], "radius": 8.0667})).unwrap();
        s.execute("sketch.finish", &json!({})).unwrap();
        let tab = s.execute("solid.extrude", &json!({"distance": 10, "operation": "join", "profiles": [{"point": [0, -3]}]})).unwrap()["feature"]
            .as_u64()
            .unwrap();
        let app = SolveApp::new(s, crate::Services::default());
        let parts = feature_parts(&app, tab, true);
        let faces: Vec<_> = parts.iter().filter_map(|p| if let Sel::Face { point, .. } = p { Some(*point) } else { None }).collect();
        assert!(faces.iter().any(|p| p.x > 20.0), "the tab lights its own faces: {parts:?}");
        // The plate's walls bulge past y = ±8; nothing of the tab does.
        assert!(faces.iter().all(|p| p.y.abs() <= 8.0 + 1e-6), "{parts:?}");
    }
}
