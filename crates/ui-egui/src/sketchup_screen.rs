//! The model screen as SketchUp's (Preferences › Layout › SketchUp model screen): a sky that
//! fades to the horizon over a ground, long red, green and blue axes through the origin, solid
//! one way and dotted the other, near-black edges, off-white faces and blue-grey back faces.
//! Origin planes are left out (`viewport::origin_planes`), and so are the view navigator
//! (`view_gizmo::visible`) and the navigation bar (`navigation_bar::shown`).

use egui::{Color32, Pos2, Rect};
use sketcraft_engine::geom::prelude::*;
use solidcore_record::Vec3;

use crate::SolveApp;
use crate::viewport::Proj;

/// Sky at the top of the view, and where it meets the horizon.
const SKY_TOP: Color32 = Color32::from_rgb(158, 193, 224);
const SKY_HORIZON: Color32 = Color32::from_rgb(226, 236, 245);
/// The ground below the horizon: darker near the horizon's haze, then flat.
const GROUND_HORIZON: Color32 = Color32::from_rgb(214, 211, 200);
const GROUND: Color32 = Color32::from_rgb(196, 192, 180);

/// The axes' colours: red X, green Y, blue Z.
pub(crate) const AXIS_X: Color32 = Color32::from_rgb(214, 38, 38);
pub(crate) const AXIS_Y: Color32 = Color32::from_rgb(28, 150, 46);
pub(crate) const AXIS_Z: Color32 = Color32::from_rgb(36, 68, 220);

/// Edges drawn near-black, and faces without a colour of their own off-white, as SketchUp's
/// default style draws them.
pub(crate) const EDGE: Color32 = Color32::from_rgb(24, 24, 24);
pub(crate) const FACE: Color32 = Color32::from_rgb(238, 236, 230);

/// The edge colour on the SketchUp model screen; `None` for the theme's.
pub(crate) fn edge(app: &SolveApp) -> Option<Color32> {
    on(app).then_some(EDGE)
}

/// The colour of a face with no colour of its own on the SketchUp model screen; `None` for the
/// theme's.
pub(crate) fn face(app: &SolveApp) -> Option<Color32> {
    on(app).then_some(FACE)
}

/// The back side of a face, blue-grey as SketchUp shows it.
pub(crate) const BACK_FACE: Color32 = Color32::from_rgb(164, 178, 187);

/// The viewport shader's back-face colour (`U.back_face`: straight sRGB 0..1, alpha 1 on): on
/// the SketchUp model screen, else off (alpha 0), back sides in the body's colour.
pub(crate) fn back_face_uniform(app: &SolveApp) -> [f32; 4] {
    if !on(app) {
        return [0.0; 4];
    }
    let [r, g, b, _] = BACK_FACE.to_normalized_gamma_f32();
    [r, g, b, 1.0]
}

/// How far the axes run, in origin sizes (the origin's size follows the zoom).
pub(crate) const AXIS_REACH: f64 = 60.0;

/// The SketchUp model screen is on.
pub(crate) fn on(app: &SolveApp) -> bool {
    app.preferences.sketchup_screen && !crate::render_workspace::active(app)
}

/// Where the horizon crosses the view (a screen height), in perspective; `None` in a parallel
/// view or when the camera looks straight up or down.
fn horizon_y(proj: &Proj) -> Option<f32> {
    let cam = &proj.cam;
    if cam.fov <= 0.0 {
        return None;
    }
    let up = cam.up.dir();
    let eye = cam.eye();
    let forward = cam.target - eye;
    let flat = forward - up * forward.dot(up);
    if flat.len() < 1e-9 * forward.len().max(1.0) {
        return None;
    }
    // A point far ahead at eye height sits on the horizon.
    let far = eye + flat * (1.0e6 / flat.len()) * cam.distance.max(1.0);
    proj.to_screen(far).map(|p| p.y)
}

fn gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    if rect.height() <= 0.0 || rect.width() <= 0.0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Paint the sky and the ground behind the model.
pub(crate) fn paint_sky(app: &SolveApp, painter: &egui::Painter, rect: Rect, proj: &Proj) {
    if !on(app) {
        return;
    }
    let Some(y) = horizon_y(proj) else {
        gradient(painter, rect, SKY_TOP, SKY_HORIZON);
        return;
    };
    let y = y.clamp(rect.top(), rect.bottom());
    gradient(painter, Rect::from_min_max(rect.min, Pos2::new(rect.right(), y)), SKY_TOP, SKY_HORIZON);
    let haze = (y + rect.height() * 0.15).min(rect.bottom());
    gradient(painter, Rect::from_min_max(Pos2::new(rect.left(), y), Pos2::new(rect.right(), haze)), GROUND_HORIZON, GROUND);
    painter.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), haze), rect.max), 0.0, GROUND);
}

/// The colour of an origin axis on the SketchUp screen.
pub(crate) fn axis_colour(name: &str) -> Color32 {
    match name {
        "X" => AXIS_X,
        "Y" => AXIS_Y,
        _ => AXIS_Z,
    }
}

/// The axis segments to draw for `dir`: one solid run out along it, then dots back along the
/// other way, each `(from, to, solid)`.
pub(crate) fn axis_segments(dir: Vec3, size: f64) -> Vec<(Vec3, Vec3, bool)> {
    let reach = size * AXIS_REACH;
    let mut out = vec![(Vec3::ZERO, dir * reach, true)];
    let dot = size * 0.12;
    let mut a = dot;
    while a < reach {
        out.push((dir * -a, dir * -(a + dot), false));
        a += dot * 2.5;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Harness, rect_of};
    use serde_json::json;

    /// A Harness (the classic screen) showing one extruded block.
    fn block() -> Harness {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [10, 10]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        h.app.run("solid.extrude", json!({"distance": 5})).unwrap();
        h.settle();
        h.frames(2);
        h
    }

    #[test]
    fn the_sketchup_screen_has_no_view_navigator_or_navigation_bar() {
        let mut h = block();
        assert!(rect_of("axis_view:outer").is_some(), "the classic screen shows the view navigator");
        assert!(rect_of("nav:outer").is_some(), "the classic screen shows the navigation bar");
        h.app.preferences.sketchup_screen = true;
        h.frames(3);
        assert!(rect_of("axis_view:outer").is_none(), "no view navigator on the SketchUp screen");
        assert!(rect_of("nav:outer").is_none(), "no navigation bar on the SketchUp screen");
        let layout = h.app.viewport.layout.as_ref().unwrap();
        assert!(!layout.axis_view.is_positive() && !layout.navigation.is_positive(), "nothing keeps their room");
        let middle = h.app.viewport.rect.unwrap().center();
        assert!(!crate::viewport::view_gizmo_at(&h.app, middle));
        assert!(h.app.ui.show_navigation_bar, "the saved switch is left as it was");
        // And back.
        h.app.preferences.sketchup_screen = false;
        h.frames(3);
        assert!(rect_of("axis_view:outer").is_some() && rect_of("nav:outer").is_some());
    }

    #[test]
    fn back_faces_are_blue_grey_on_the_sketchup_screen_only() {
        let mut h = block();
        assert_eq!(back_face_uniform(&h.app)[3], 0.0, "off on the classic screen");
        h.app.preferences.sketchup_screen = true;
        let [r, g, b, a] = back_face_uniform(&h.app);
        assert_eq!(a, 1.0);
        assert_eq!([r, g, b].map(|v| (v * 255.0).round() as u8), [164, 178, 187]);
    }

    #[test]
    fn the_sketchup_screen_draws_near_black_edges_and_off_white_faces() {
        let mut h = block();
        let classic = crate::viewport::scene_key(&h.app);
        let entries = |h: &Harness| crate::viewport::build_scene(&h.app).palette.entries().to_vec();
        let face = [FACE.r(), FACE.g(), FACE.b(), 255];
        let edge = [EDGE.r(), EDGE.g(), EDGE.b(), 255];
        let before = entries(&h);
        assert!(!before.contains(&face) && !before.contains(&edge), "the theme's colours on the classic screen");
        h.app.preferences.sketchup_screen = true;
        assert_ne!(crate::viewport::scene_key(&h.app), classic, "turning the screen on rebuilds the model scene");
        let after = entries(&h);
        assert!(after.contains(&face), "off-white faces");
        assert!(after.contains(&edge), "near-black edges");
    }

    #[test]
    fn axes_run_solid_one_way_and_dotted_the_other() {
        let segments = axis_segments(Vec3::X, 1.0);
        let (from, to, solid) = segments[0];
        assert!(solid && from == Vec3::ZERO && (to.x - AXIS_REACH).abs() < 1e-9, "one solid run along +X");
        assert!(segments[1..].iter().all(|(a, b, solid)| !solid && a.x < 0.0 && b.x < a.x), "dots along -X");
        assert!(segments.len() > 20 && segments.len() < 400, "{} segments", segments.len());
    }
}
