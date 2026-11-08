//! SketchUp's Previous and Next view (`view.previous`, `view.next`): the views the window showed
//! recently. A view counts once the camera has stayed still for a few frames and differs from the
//! last one kept, so the frames of an orbit or of a view animation are not views of their own.
//! Previous steps back through them and Next forward again; a new view after Previous drops the
//! views Next would have returned to. Each design tab keeps its own (`documents::DocSlot`); it is
//! window state, never saved with the design.

use sketcraft_engine::geom::prelude::*;
use sketcraft_engine::render::Camera;

use crate::SketApp;

/// Most views kept; the oldest goes first.
pub(crate) const CAPACITY: usize = 50;
/// Frames the camera must stay still before its view is kept (frames, not seconds, so headless
/// runs behave as the window does).
pub(crate) const SETTLE_FRAMES: u32 = 6;

#[derive(Clone, Debug, Default)]
pub(crate) struct ViewHistory {
    /// The views kept, oldest first.
    views: Vec<Camera>,
    /// The view shown now, or that Previous or Next last went to.
    at: usize,
    /// The camera last frame, and the frames it has stayed the same.
    last: Option<Camera>,
    still: u32,
}

/// Two cameras show the same view (to rounding).
fn same(a: &Camera, b: &Camera) -> bool {
    let tolerance = 1e-9 * a.distance.abs().max(b.distance.abs()).max(1.0);
    a.target.dist(b.target) <= tolerance
        && (a.distance - b.distance).abs() <= tolerance
        && (a.yaw - b.yaw).abs() <= 1e-9
        && (a.pitch - b.pitch).abs() <= 1e-9
        && (a.roll - b.roll).abs() <= 1e-9
        && (a.fov - b.fov).abs() <= 1e-9
        && a.up == b.up
}

impl ViewHistory {
    /// The camera this frame. `moving`: a view animation is under way, so this is no view yet.
    /// Returns whether the camera is still settling (it needs more frames to count).
    pub(crate) fn observe(&mut self, cam: Camera, moving: bool) -> bool {
        if moving || !cam.is_valid() || self.last.is_none_or(|last| !same(&last, &cam)) {
            self.last = Some(cam);
            self.still = 0;
            return !moving && cam.is_valid();
        }
        if self.still >= SETTLE_FRAMES {
            return false;
        }
        self.still += 1;
        if self.still == SETTLE_FRAMES {
            self.record(cam);
        }
        self.still < SETTLE_FRAMES
    }

    /// Keep `cam` as the newest view, unless it is the view shown now.
    pub(crate) fn record(&mut self, cam: Camera) {
        if !cam.is_valid() || self.views.get(self.at).is_some_and(|v| same(v, &cam)) {
            return;
        }
        // A new view after Previous: the views ahead of it are gone.
        self.views.truncate(self.at.saturating_add(1).min(self.views.len()));
        self.views.push(cam);
        if self.views.len() > CAPACITY {
            let extra = self.views.len() - CAPACITY;
            self.views.drain(..extra);
        }
        self.at = self.views.len() - 1;
    }

    /// The view before the one shown (`current`), if there is one. A view not kept yet (the
    /// camera moved less than [`SETTLE_FRAMES`] ago) is kept first, so Next comes back to it.
    pub(crate) fn previous(&mut self, current: Camera) -> Option<Camera> {
        self.record(current);
        let to = self.at.checked_sub(1)?;
        self.at = to;
        self.views.get(to).copied()
    }

    /// The view after the one shown, once Previous went back.
    pub(crate) fn next(&mut self) -> Option<Camera> {
        let to = self.at + 1;
        let view = self.views.get(to).copied()?;
        self.at = to;
        Some(view)
    }

    #[cfg(test)]
    pub(crate) fn can_go_back(&self) -> bool {
        self.at > 0
    }

    #[cfg(test)]
    pub(crate) fn can_go_forward(&self) -> bool {
        self.at + 1 < self.views.len()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.views.len()
    }
}

/// Once a frame, before the view is drawn: the camera as the last frame left it. A camera that
/// just stopped asks for the few frames it needs to count, as the window draws only on input.
pub(crate) fn observe(app: &mut SketApp, ctx: &egui::Context) {
    let moving = app.cam_anim.is_some();
    if app.view_history.observe(app.cam, moving) {
        ctx.request_repaint();
    }
}

/// `view.previous` and `view.next` in the window: animate to the view before or after.
pub(crate) fn step(app: &mut SketApp, forward: bool) {
    // A view change under way ends where it was going: that is the view stepped from.
    let current = app.cam_anim.map_or(app.cam, |anim| anim.to);
    let to = if forward { app.view_history.next() } else { app.view_history.previous(current) };
    if let Some(to) = to {
        app.animate_to(to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(distance: f64) -> Camera {
        Camera { distance, ..Camera::default() }
    }

    fn settle(h: &mut ViewHistory, c: Camera) {
        for _ in 0..=SETTLE_FRAMES {
            h.observe(c, false);
        }
    }

    #[test]
    fn a_view_counts_once_it_is_still_and_new() {
        let mut h = ViewHistory::default();
        for d in [100.0, 101.0, 102.0, 103.0] {
            h.observe(cam(d), false);
        }
        assert_eq!(h.len(), 0, "a camera on the move is no view");
        settle(&mut h, cam(103.0));
        assert_eq!(h.len(), 1);
        settle(&mut h, cam(103.0));
        assert_eq!(h.len(), 1, "the same view is kept once");
        // An animation's frames, even still ones, are not views.
        for _ in 0..SETTLE_FRAMES * 2 {
            h.observe(cam(50.0), true);
        }
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn previous_and_next_walk_the_views_and_a_new_view_drops_the_ones_ahead() {
        let mut h = ViewHistory::default();
        for d in [10.0, 20.0, 30.0] {
            settle(&mut h, cam(d));
        }
        assert!(!h.can_go_forward());
        assert_eq!(h.previous(cam(30.0)), Some(cam(20.0)));
        assert_eq!(h.previous(cam(20.0)), Some(cam(10.0)));
        assert_eq!(h.previous(cam(10.0)), None, "nothing before the first");
        assert_eq!(h.next(), Some(cam(20.0)));
        assert!(h.can_go_back() && h.can_go_forward());
        // Arriving at the view Previous went to adds nothing.
        settle(&mut h, cam(20.0));
        assert_eq!(h.next(), Some(cam(30.0)));
        assert_eq!(h.next(), None, "nothing after the last");
        assert_eq!(h.previous(cam(30.0)), Some(cam(20.0)));
        settle(&mut h, cam(25.0));
        assert!(!h.can_go_forward(), "a new view after Previous clears Next");
        assert_eq!(h.len(), 3);
        assert_eq!(h.previous(cam(25.0)), Some(cam(20.0)));
    }

    #[test]
    fn previous_keeps_a_view_not_yet_settled_so_next_returns_to_it() {
        let mut h = ViewHistory::default();
        settle(&mut h, cam(10.0));
        h.observe(cam(40.0), false);
        assert_eq!(h.previous(cam(40.0)), Some(cam(10.0)));
        assert_eq!(h.next(), Some(cam(40.0)));
    }

    #[test]
    fn the_history_keeps_the_newest_views_only() {
        let mut h = ViewHistory::default();
        for i in 0..CAPACITY + 10 {
            h.record(cam(1.0 + i as f64));
        }
        assert_eq!(h.len(), CAPACITY);
        let mut shown = cam(CAPACITY as f64 + 10.0);
        while let Some(c) = h.previous(shown) {
            shown = c;
        }
        assert_eq!(shown, cam(11.0), "the first ten went");
    }
}
