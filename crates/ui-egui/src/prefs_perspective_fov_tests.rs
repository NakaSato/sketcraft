use super::*;
use egui::{Event, Key, Modifiers, PointerButton, pos2};

fn press(h: &mut crate::scenario::Harness, at: egui::Pos2, pressed: bool) {
    h.app.synthetic.push(Event::PointerMoved(at));
    h.app.synthetic.push(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.frames(3);
}

fn graphics_slider(h: &mut crate::scenario::Harness) -> egui::Rect {
    h.call("ui.prefs", json!({"open": "Graphics"}));
    let label = crate::i18n::tr("prefs.perspective_fov");
    reveal_preference(h, &format!("dialog:slider:{label}"));
    crate::scenario::rect_of(&format!("dialog:slider_track:{label}")).expect("the Field of view slider")
}

/// Graphics › Field of view (#5364): the kit slider in whole degrees, dragged or typed, written
/// back in radians. While the view can't draw in perspective (Perspective view off, and no
/// perspective-while-orbiting mode, #2682) it changes nothing, and Cancel puts it back.
#[test]
fn the_field_of_view_slider_drags_and_types_whole_degrees() {
    let mut h = crate::scenario::Harness::new();
    for (perspective, auto_ortho) in [(false, false), (true, false), (false, true)] {
        h.call("ui.set", json!({"perspective": perspective}));
        h.app.ui.auto_ortho_faces = auto_ortho;
        let track = graphics_slider(&mut h);
        press(&mut h, pos2(track.left() + 4.0, track.center().y), true);
        h.app.synthetic.push(Event::PointerMoved(pos2(track.left() + track.width() * 0.75, track.center().y)));
        h.frames(3);
        press(&mut h, pos2(track.left() + track.width() * 0.75, track.center().y), false);
        let degrees = h.app.preferences.perspective_fov.to_degrees();
        if perspective || auto_ortho {
            assert!((degrees - degrees.round()).abs() < 1e-9 && (35.0..=90.0).contains(&degrees), "{degrees}");
        } else {
            assert_eq!(h.app.preferences.perspective_fov, PERSPECTIVE_FOV, "disabled while Perspective view is off");
        }
        h.call("ui.prefs", json!({"button": "cancel"}));
        assert_eq!(h.app.preferences.perspective_fov, PERSPECTIVE_FOV, "Cancel restores the angle");
    }
    let track = graphics_slider(&mut h);
    press(&mut h, pos2(track.right() + 30.0, track.center().y), true);
    press(&mut h, pos2(track.right() + 30.0, track.center().y), false);
    h.app.synthetic.push(Event::Text("40.4".into()));
    h.app.synthetic.push(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.frames(3);
    assert!(
        (h.app.preferences.perspective_fov - 40f64.to_radians()).abs() < 1e-9,
        "typed 40.4 keeps to 40°: {}",
        h.app.preferences.perspective_fov
    );
    h.call("ui.prefs", json!({"button": "ok"}));
}

/// `ui.prefs {perspective_fov}` takes 15° to 90° in radians and refuses anything else, changing
/// nothing; a saved value the view can't use (a hand edit) draws at the default, never a broken view.
#[test]
fn hostile_angles_are_refused_or_drawn_at_the_default() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.set", json!({"perspective": true}));
    for fine in [15f64.to_radians(), 90f64.to_radians(), 1.0] {
        assert_eq!(h.call("ui.prefs", json!({"perspective_fov": fine}))["ok"], true, "{fine}");
        assert_eq!(h.app.view_fov(), fine);
    }
    let before = (h.app.preferences.clone(), h.app.cam);
    for bad in [
        json!(-1.0),
        json!(0.0),
        json!(14.9f64.to_radians()),
        json!(90.1f64.to_radians()),
        json!(std::f64::consts::PI),
        json!(1e308),
        json!(-1e308),
        json!("x"),
        json!(null),
        json!([1]),
    ] {
        let r = h.call("ui.prefs", json!({"perspective_fov": bad}));
        assert_eq!(r["ok"], false, "{bad}: {r}");
        assert_eq!((h.app.preferences.clone(), h.app.cam), before, "{bad} changes nothing");
    }
    for saved in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.1, 2.0, std::f64::consts::PI, 1e308] {
        h.app.preferences.perspective_fov = saved;
        h.frames(2);
        assert_eq!(h.app.view_fov(), PERSPECTIVE_FOV, "{saved} draws at the default");
        assert!(h.app.cam.is_valid(), "{saved}: {:?}", h.app.cam);
    }
    // A fuzz pass: any bit pattern, and values spread over -1…3 rad, through `ui.prefs` and saved.
    let range = 15f64.to_radians()..=90f64.to_radians();
    let mut x: u64 = 0x5364;
    for i in 0..2000 {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let v = if i % 2 == 0 { f64::from_bits(x) } else { (x >> 11) as f64 / (1u64 << 53) as f64 * 4.0 - 1.0 };
        let accepted = set(&mut h.app, &json!({"perspective_fov": v})).is_ok();
        assert_eq!(accepted, range.contains(&v), "{v}");
        h.app.preferences.perspective_fov = v;
        follow_view_fov(&mut h.app);
        assert!(range.contains(&h.app.view_fov()) && h.app.cam.is_valid(), "{v}: {:?}", h.app.cam);
    }
    let read = lenient(&json!({"perspective_fov": "wide", "ui_scale": 1.5}));
    assert_eq!((read.perspective_fov, read.ui_scale), (PERSPECTIVE_FOV, 1.5), "an unreadable angle costs that setting only");
}

/// The target keeps its size when the angle changes, by every path (`ui.prefs`, Cancel, Restore
/// Defaults), and a parallel view or a sketch keeps the last perspective angle until it's back.
#[test]
fn the_view_keeps_its_size_when_the_angle_changes() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.call("ui.set", json!({"perspective": true}));
    h.frames(3);
    let half = |h: &crate::scenario::Harness| {
        let mut cam = h.app.cam;
        cam.fov = h.app.view_fov();
        cam.half_height()
    };
    let (size, distance) = (half(&h), h.app.cam.distance);
    h.call("ui.prefs", json!({"open": "Graphics"}));
    for fov in [15f64.to_radians(), 1.0, 90f64.to_radians()] {
        h.call("ui.prefs", json!({"perspective_fov": fov}));
        assert!((half(&h) / size - 1.0).abs() < 1e-9, "{fov}: {} vs {size}", half(&h));
    }
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert!((h.app.cam.distance / distance - 1.0).abs() < 1e-9, "Cancel restores the angle and the size");
    h.call("ui.prefs", json!({"perspective_fov": 1.2}));
    h.call("ui.prefs", json!({"open": "Graphics", "button": "defaults"}));
    h.call("ui.prefs", json!({"button": "ok"}));
    assert_eq!(h.app.preferences.perspective_fov, PERSPECTIVE_FOV);
    assert!(h.app.ui.perspective, "the page's Restore Defaults turns Perspective view on too (SketchUp's default)");
    // A parallel view keeps the last perspective angle: back in perspective, the default angle
    // draws at the first size, and an angle changed while it's off is followed once it's on.
    h.call("ui.set", json!({"perspective": true}));
    h.frames(2);
    assert!((h.app.cam.distance / distance - 1.0).abs() < 1e-9, "Restore Defaults restores the size");
    h.call("ui.set", json!({"perspective": false}));
    h.frames(2);
    let parallel = h.app.cam.distance;
    h.call("ui.prefs", json!({"perspective_fov": 1.2}));
    assert_eq!(h.app.cam.distance, parallel, "parallel: nothing to rescale yet");
    h.call("ui.set", json!({"perspective": true}));
    h.frames(2);
    assert!((half(&h) / size - 1.0).abs() < 1e-9);
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.call("ui.prefs", json!({"perspective_fov": 0.6}));
    h.app.finish_sketch();
    h.frames(2);
    assert!((half(&h) / size - 1.0).abs() < 1e-9);
}

/// A view change that is animating when the angle changes ends at the same size too: both its
/// ends are rescaled with the camera.
#[test]
fn an_animating_view_change_is_rescaled_too() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.set", json!({"perspective": true}));
    h.frames(2);
    let to = sketcraft_engine::render::Camera { distance: 300.0, ..h.app.cam };
    h.app.cam_anim = Some(sketcraft_engine::render::CameraAnim::new(h.app.cam, to, h.app.now));
    let from = h.app.cam;
    set(&mut h.app, &json!({"perspective_fov": 1.0})).unwrap();
    let k = (0.6f64 * 0.5).tan() / 0.5f64.tan();
    let anim = h.app.cam_anim.as_ref().expect("still animating");
    assert!((anim.to.distance - 300.0 * k).abs() < 1e-9 && (anim.from.distance - from.distance * k).abs() < 1e-9, "{:?}", (anim.from, anim.to));
}

/// A script's Cancel puts the view's size back at once, before any frame runs, so a `ui.camera`
/// read straight after it gives the restored view.
#[test]
fn cancel_restores_the_size_before_the_next_frame() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.set", json!({"perspective": true}));
    h.frames(2);
    let distance = h.app.cam.distance;
    let mut raw = |method: &str, params: serde_json::Value| {
        let (req, _rx) = crate::control::ControlRequest::new(method, params);
        match crate::control::handle(&mut h.app, &h.ctx, &req) {
            crate::control::Outcome::Done(v) => v,
            _ => serde_json::Value::Null,
        }
    };
    raw("ui.prefs", json!({"open": "Graphics", "perspective_fov": 1.2}));
    raw("ui.prefs", json!({"button": "cancel"}));
    let cam = raw("ui.camera", serde_json::Value::Null);
    assert!((cam["result"]["distance"].as_f64().unwrap() / distance - 1.0).abs() < 1e-9, "{cam}");
}

/// The angle is kept between runs.
#[test]
fn the_angle_is_kept_across_a_restart() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"perspective_fov": 1.1}));
    h.step(&json!({"restart": "close"})).unwrap();
    assert_eq!(h.app.preferences.perspective_fov, 1.1);
}
