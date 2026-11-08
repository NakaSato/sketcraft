//! SketchUp's Zoom Window, Previous and Next view in the window (`view.zoom_window`,
//! `view.previous`, `view.next`; `navigation::frame_screen_rect`, `view_history`).

use crate::scenario::Harness;
use crate::workspace::SKETCHUP_TAB;
use egui::{Pos2, Rect, pos2, vec2};
use serde_json::json;
use sketcraft_engine::geom::prelude::*;
use sketcraft_engine::render::Camera;
use solidcore_record::Vec3;

/// The SketchUp tab, view changes immediate (no animation) so each step lands at once.
fn harness() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.ui.tab = SKETCHUP_TAB.into();
    h.app.preferences.look_at_anim = false;
    h.frames(2);
    h.app.cam_anim = None;
    // Long enough for the view to count as one of the recent views.
    h.frames(2 * crate::view_history::SETTLE_FRAMES as usize);
    h
}

fn view_rect(h: &Harness) -> Rect {
    h.app.viewport.rect.expect("the view is drawn")
}

/// A screen rectangle right of the view's centre and clear of the browser card, the view cube and
/// the navigation bar.
fn window_in(view: Rect) -> Rect {
    let c = view.center();
    Rect::from_min_max(c + vec2(40.0, -60.0), c + vec2(200.0, 60.0))
}

/// Where the screen point `p` meets the plane through the camera's target, facing the camera.
fn on_target_plane(h: &Harness, view: Rect, p: Pos2) -> Vec3 {
    let proj = crate::viewport::projection(&h.app, view);
    let (o, d) = proj.ray(p);
    let back = proj.cam.back();
    let t = (proj.cam.target - o).dot(back) / d.dot(back);
    o + d * t
}

fn close(a: &Camera, b: &Camera) -> bool {
    a.target.dist(b.target) < 1e-6 * a.distance.max(1.0)
        && (a.distance - b.distance).abs() < 1e-6 * a.distance.max(1.0)
        && a.yaw == b.yaw
        && a.pitch == b.pitch
}

fn drag(h: &mut Harness, window: Rect) {
    let (a, b) = (window.left_top(), window.right_bottom());
    h.step(&json!({"drag": [[a.x, a.y], [b.x, b.y]]})).unwrap();
    h.frames(2);
}

/// Zoom Window: the drag draws no selection box, and on release the view zooms so the rectangle
/// fills it: what was at the rectangle's centre is at the view's centre, and the camera comes
/// closer by the rectangle's larger side ratio. One use ends the tool.
#[test]
fn zoom_window_frames_the_dragged_rectangle() {
    for perspective in [false, true] {
        let mut h = harness();
        h.app.ui.perspective = perspective;
        h.frames(2);
        let view = view_rect(&h);
        let window = window_in(view);
        let before = h.app.cam;
        let aimed = on_target_plane(&h, view, window.center());
        // A toolbar start, as the SketchUp tab's button does.
        h.step(&json!({"start":"view.zoom_window"})).unwrap();
        assert!(h.app.viewport.zoom_window.is_some(), "the toolbar start latches Zoom Window");
        drag(&mut h, window);
        assert!(h.app.viewport.boxsel.is_none() && h.app.session.selection.is_empty(), "the drag selects nothing");
        assert!(h.app.viewport.zoom_window.is_none(), "one use ends the tool");
        let after = h.app.cam;
        let ratio = f64::from((window.width() / view.width()).max(window.height() / view.height()));
        assert!(
            (after.distance / before.distance - ratio).abs() < 1e-3,
            "perspective {perspective}: {} / {} vs {ratio}",
            after.distance,
            before.distance
        );
        assert!(after.target.dist(aimed) < before.target.dist(aimed), "the target moves toward the rectangle");
        let proj = crate::viewport::projection(&h.app, view);
        let centre = proj.to_screen(aimed).expect("on screen");
        assert!(centre.distance(view.center()) < 1.0, "perspective {perspective}: the rectangle's centre is now the view's: {centre:?}");
        assert_eq!((after.yaw, after.pitch, after.roll), (before.yaw, before.pitch, before.roll), "Zoom Window never turns the view");
    }
}

/// While Zoom Window waits, Escape or the command again releases it, and a click (no drag) leaves
/// the camera alone and keeps waiting.
#[test]
fn zoom_window_unlatches_on_escape_or_the_command_again() {
    let mut h = harness();
    h.app.run("view.zoom_window", json!({})).unwrap();
    assert!(h.app.viewport.zoom_window.is_some());
    let before = h.app.cam;
    let c = view_rect(&h).center() + vec2(80.0, 0.0);
    h.step(&json!({"drag": [[c.x, c.y], [c.x + 2.0, c.y + 2.0]]})).unwrap();
    h.frames(2);
    assert!(close(&h.app.cam, &before), "a click frames nothing");
    assert!(h.app.viewport.zoom_window.is_some(), "and the tool still waits for a drag");
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(h.app.viewport.zoom_window.is_none(), "Escape releases it");
    h.app.run("view.zoom_window", json!({})).unwrap();
    h.app.run("view.zoom_window", json!({})).unwrap();
    assert!(h.app.viewport.zoom_window.is_none(), "the command again releases it");
    h.app.run("view.zoom_window", json!({"on": true})).unwrap();
    h.app.run("view.zoom_window", json!({"on": true})).unwrap();
    assert!(h.app.viewport.zoom_window.is_some(), "on: true keeps it on");
    // One camera tool at a time.
    h.app.run("view.tool.orbit", json!({"on": true})).unwrap();
    assert!(h.app.viewport.zoom_window.is_none());
    h.app.run("view.zoom_window", json!({})).unwrap();
    assert!(h.app.viewport.nav.is_none() && h.app.viewport.zoom_window.is_some());
}

/// Previous goes back to the view before Zoom Window, Next returns to the zoomed one, and a new
/// view after Previous leaves nothing for Next.
#[test]
fn previous_and_next_step_through_the_views_shown() {
    let mut h = harness();
    let view = view_rect(&h);
    let first = h.app.cam;
    h.app.run("view.zoom_window", json!({})).unwrap();
    drag(&mut h, window_in(view));
    let zoomed = h.app.cam;
    assert!(!close(&zoomed, &first), "Zoom Window moved the view");
    h.frames(2 * crate::view_history::SETTLE_FRAMES as usize);
    h.step(&json!({"start":"view.previous"})).unwrap();
    h.frames(2);
    assert!(close(&h.app.cam, &first), "Previous restores the view before: {:?} vs {:?}", h.app.cam, first);
    h.frames(2 * crate::view_history::SETTLE_FRAMES as usize);
    h.step(&json!({"start":"view.next"})).unwrap();
    h.frames(2);
    assert!(close(&h.app.cam, &zoomed), "Next returns to the zoomed view");
    // Previous again, then a view of its own: Next has nowhere to go.
    h.app.run("view.previous", json!({})).unwrap();
    h.frames(2 * crate::view_history::SETTLE_FRAMES as usize);
    // An orbit of its own (the scenario `camera` step puts the camera there directly).
    h.step(&json!({"camera": {"yaw": first.yaw + 0.4, "pitch": first.pitch * 0.5}})).unwrap();
    h.frames(2 * crate::view_history::SETTLE_FRAMES as usize);
    let turned = h.app.cam;
    assert!(!close(&turned, &first));
    h.app.run("view.next", json!({})).unwrap();
    h.frames(2);
    assert!(close(&h.app.cam, &turned), "a new view after Previous clears Next");
    h.app.run("view.previous", json!({})).unwrap();
    h.frames(2);
    assert!(close(&h.app.cam, &first), "and Previous from it goes back to the view it replaced");
    // The steps are display only.
    assert!(h.app.session.undo.is_empty());
}

/// The rectangle being dragged is drawn while the button is held.
#[test]
fn zoom_window_draws_its_rectangle_while_dragging() {
    let mut h = harness();
    h.app.run("view.zoom_window", json!({})).unwrap();
    let window = window_in(view_rect(&h));
    let (a, b) = (window.left_top(), window.right_bottom());
    let button = egui::PointerButton::Primary;
    let modifiers = egui::Modifiers::NONE;
    // The pointer events play one a frame, so the rectangle can be looked at before the release.
    h.app.synthetic.push(egui::Event::PointerMoved(a));
    h.app.synthetic.push(egui::Event::PointerButton { pos: a, button, pressed: true, modifiers });
    for k in 1..=8 {
        h.app.synthetic.push(egui::Event::PointerMoved(a + (b - a) * (k as f32 / 8.0)));
    }
    while !h.app.synthetic.is_empty() {
        h.frame();
    }
    let held = h.app.viewport.zoom_window.and_then(|w| w.rect()).expect("a rectangle while the button is down");
    assert!((held.min - pos2(a.x, a.y)).length() < 1.0 && (held.max - pos2(b.x, b.y)).length() < 1.0, "from the press to the pointer: {held:?}");
    let before = h.app.cam;
    h.app.synthetic.push(egui::Event::PointerButton { pos: b, button, pressed: false, modifiers });
    h.frames(3);
    assert!(h.app.viewport.zoom_window.is_none() && h.app.cam.distance < before.distance, "the release frames it");
}
