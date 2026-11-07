use super::*;
use sketcraft_engine::render::{StandardView, UpAxis};

fn plane(view: StandardView, up: UpAxis) -> (Vec3, Vec3) {
    let (yaw, pitch) = view.angles();
    let cam = Camera { yaw, pitch, up, ..Camera::default() };
    let (u, v) = grid_plane(cam.to_frame(cam.back()), GridPlane::default()).axes();
    (cam.from_frame(u), cam.from_frame(v))
}

/// The camera frame's direction toward the eye for a view at `yaw` and `pitch` degrees.
fn back_at(yaw_deg: f64, pitch_deg: f64) -> Vec3 {
    let cam = Camera { yaw: yaw_deg.to_radians(), pitch: pitch_deg.to_radians(), ..Camera::default() };
    cam.to_frame(cam.back())
}

/// #1470: the grid lies on the ground unless the view sees the ground edge-on; then it lies on
/// the upright origin plane facing the view, Z up or Y up alike.
#[test]
fn the_grid_faces_side_views_and_stays_on_the_ground_otherwise() {
    let normal = |(u, v): (Vec3, Vec3)| u.cross(v);
    for up in [UpAxis::Z, UpAxis::Y] {
        for view in [StandardView::Top, StandardView::Bottom, StandardView::Iso] {
            assert_eq!(normal(plane(view, up)).dot(up.dir()).abs(), 1.0, "{view:?} {up:?}: the ground");
        }
        for view in [StandardView::Front, StandardView::Back, StandardView::Right, StandardView::Left] {
            let (yaw, pitch) = view.angles();
            let back = Camera { yaw, pitch, up, ..Camera::default() }.back();
            let n = normal(plane(view, up));
            assert!(n.dot(up.dir()).abs() < 1e-9, "{view:?} {up:?}: an upright plane");
            assert!((n.dot(back).abs() - 1.0).abs() < 1e-9, "{view:?} {up:?}: facing the view");
        }
    }
}

/// #1993: the view has to turn clearly off the ground before the grid leaves it, and clearly
/// away from a wall before it goes back, so a view wobbling at the threshold does not flicker.
#[test]
fn the_grid_plane_thresholds_have_hysteresis() {
    // 9°: edge-on enough to enter the wall.
    assert_eq!(grid_plane(back_at(0.0, 9.0), GridPlane::Ground), GridPlane::Xz);
    // 12°: past the entering limit, but not enough to leave a wall the grid is already on.
    assert_eq!(grid_plane(back_at(0.0, 12.0), GridPlane::Ground), GridPlane::Ground);
    assert_eq!(grid_plane(back_at(0.0, 12.0), GridPlane::Xz), GridPlane::Xz);
    // 16°: clearly off the ground, so even a wall goes back.
    assert_eq!(grid_plane(back_at(0.0, 16.0), GridPlane::Xz), GridPlane::Ground);
    // Below the ground the same, and a view straight down stays on the ground.
    assert_eq!(grid_plane(back_at(30.0, -12.0), GridPlane::Yz), GridPlane::Yz);
    assert_eq!(grid_plane(Vec3::new(0.0, 0.0, 1.0), GridPlane::Xz), GridPlane::Ground);
    assert_eq!(grid_plane(Vec3::new(0.0, 0.0, -1.0), GridPlane::Xz), GridPlane::Ground);
}

/// #1993: orbiting at a low elevation keeps the wall until a side view is near; the other
/// wall takes over only there, never at the 45° crossing.
#[test]
fn a_low_orbit_switches_walls_only_near_a_side_view() {
    let low = |yaw_deg: f64| back_at(yaw_deg, 3.0);
    // Entering from the ground still picks the wall facing the view.
    assert_eq!(grid_plane(low(44.0), GridPlane::Ground), GridPlane::Xz);
    assert_eq!(grid_plane(low(50.0), GridPlane::Ground), GridPlane::Yz);
    // A wall is kept across the crossing and only changes near the side view it faces.
    assert_eq!(grid_plane(low(44.0), GridPlane::Xz), GridPlane::Xz);
    assert_eq!(grid_plane(low(50.0), GridPlane::Xz), GridPlane::Xz, "no swap at the 45° crossing");
    assert_eq!(grid_plane(low(70.0), GridPlane::Xz), GridPlane::Xz);
    assert_eq!(grid_plane(low(80.0), GridPlane::Xz), GridPlane::Yz, "the Left view's wall takes over near it");
    assert_eq!(grid_plane(low(10.0), GridPlane::Yz), GridPlane::Xz, "the Front view's wall takes over near it");
}

/// #1993: a hostile view direction (a design file, a control request) never panics and never
/// leaves the grid off the ground's axes.
#[test]
fn a_hostile_view_direction_stays_on_the_ground() {
    for back in [Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(0.0, f64::NEG_INFINITY, 0.0), Vec3::new(0.0, 0.0, f64::INFINITY), Vec3::ZERO] {
        assert_eq!(grid_plane(back, GridPlane::Xz), GridPlane::Ground, "{back:?}");
    }
}

/// #1993: the viewport keeps the plane it drew, so the next frame's choice is hysteretic.
#[test]
fn the_viewport_keeps_the_grid_plane_between_frames() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::default());
    app.preferences.grid_follows_view = true;
    app.cam.yaw = 0.0;
    app.cam.pitch = 0.0;
    let _ = build_sketch_scene(&app, &projection(&app, rect));
    assert_eq!(*app.viewport.grid_plane.lock().unwrap(), GridPlane::Xz, "the Front view takes the wall");
    // Wobble past the entering limit: the wall stays (before #1993 it flipped to the ground).
    app.cam.pitch = 0.23;
    let _ = build_sketch_scene(&app, &projection(&app, rect));
    assert_eq!(*app.viewport.grid_plane.lock().unwrap(), GridPlane::Xz, "a wobble does not flicker");
    app.cam.pitch = 0.3;
    let _ = build_sketch_scene(&app, &projection(&app, rect));
    assert_eq!(*app.viewport.grid_plane.lock().unwrap(), GridPlane::Ground, "a clear turn off the ground returns");
}

/// #6975: while Create Sketch waits for its plane, the grid lies on the origin plane under
/// the pointer, or under the pointer in the browser, so the sketch's plane shows before the
/// click. Otherwise it stays on the ground.
#[test]
fn create_sketch_previews_the_plane_under_the_pointer_on_the_grid() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.show_grid = true;
    app.preferences.grid_follows_view = false;
    let grid = |app: &SolveApp| -> Vec<[f32; 3]> {
        build_sketch_scene(app, &projection(app, rect))
            .under
            .as_chunks::<32>()
            .0
            .iter()
            .flat_map(|l| [&l[0..12], &l[12..24]])
            .map(|b| [0, 4, 8].map(|i| f32::from_le_bytes(b[i..i + 4].try_into().unwrap())))
            .collect()
    };
    // Every grid point has coordinate `axis` 0: the grid lies on that origin plane.
    let on = |app: &SolveApp, axis: usize| {
        let points = grid(app);
        !points.is_empty() && points.iter().all(|p| p[axis].abs() < 1e-3)
    };
    assert!(on(&app, 2), "the ground grid");
    app.viewport.hover = Some(Hit::Plane { name: "XZ".into(), point: Vec3::ZERO });
    assert!(on(&app, 2), "a plane under the pointer outside Create Sketch moves nothing");
    app.start("sketch.create");
    assert!(on(&app, 1), "on XZ, under the pointer");
    let key = sketch_scene_key(&app, &projection(&app, rect));
    app.viewport.hover = None;
    app.viewport.hover_plane = Some("YZ".into());
    assert_ne!(sketch_scene_key(&app, &projection(&app, rect)), key, "the cached grid is drawn again");
    assert!(on(&app, 0), "on YZ, whose browser row is under the pointer");
    app.viewport.hover_plane = None;
    assert!(on(&app, 2), "back on the ground");
    // With the grid turned off, the preview still shows the plane, as an open sketch's grid does.
    app.ui.show_grid = false;
    assert!(grid(&app).is_empty());
    app.viewport.hover = Some(Hit::Plane { name: "XZ".into(), point: Vec3::ZERO });
    assert!(on(&app, 1), "the preview grid is drawn even with the grid off");
}

/// #10804: a plane selected in the view, before Create Sketch, is the plane it would use, so
/// the grid lies on it until it is deselected.
#[test]
fn a_selected_plane_previews_on_the_grid_before_create_sketch() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::default());
    app.ui.show_grid = true;
    app.preferences.grid_follows_view = false;
    let on = |app: &SolveApp, axis: usize| {
        let points: Vec<[f32; 3]> = build_sketch_scene(app, &projection(app, rect))
            .under
            .as_chunks::<32>()
            .0
            .iter()
            .flat_map(|l| [&l[0..12], &l[12..24]])
            .map(|b| [0, 4, 8].map(|i| f32::from_le_bytes(b[i..i + 4].try_into().unwrap())))
            .collect();
        !points.is_empty() && points.iter().all(|p| p[axis].abs() < 1e-3)
    };
    assert!(on(&app, 2), "the ground grid");
    // A plane under the pointer, with nothing selected, moves nothing.
    app.viewport.hover = Some(Hit::Plane { name: "XZ".into(), point: Vec3::ZERO });
    assert!(on(&app, 2), "a hover alone moves nothing");
    // Select XZ: the grid lies on it before Create Sketch is clicked.
    app.session.selection = vec![Sel::Plane { name: "XZ".into() }];
    assert!(on(&app, 1), "the selected plane shows on the grid");
    // Deselecting it returns the grid to the ground.
    app.session.selection.clear();
    assert!(on(&app, 2), "deselecting returns the grid");
}

/// The grid's centre is where the view's axis meets the ground, but at a grazing slant (a
/// camera 10 mm above the ground, looking almost along it) no further than the grid's
/// half-size from the point under the target, so the ground in front of the camera keeps
/// its grid.
#[test]
fn the_grid_centre_stays_within_reach_of_the_point_under_the_target() {
    let n = Vec3::Z;
    // Looking down at 45 degrees at a target 10 above the ground: the axis meets it 10 ahead.
    let tf = Vec3::new(0.0, 0.0, 10.0);
    let back = Vec3::new(0.0, -1.0, 1.0).normalized().unwrap();
    let c = grid_centre(tf, back, n, 100.0);
    assert!((c - Vec3::new(0.0, 10.0, 0.0)).len() < 1e-9, "{c:?}");
    // Grazing: the axis meets the ground 1 m ahead, the grid is 120 wide.
    let back = Vec3::new(0.0, -1.0, 0.01).normalized().unwrap();
    let c = grid_centre(tf, back, n, 120.0);
    assert!(c.z.abs() < 1e-9 && (c - Vec3::new(0.0, 0.0, 0.0)).len() <= 120.0 + 1e-9, "{c:?}");
    assert!(c.y > 0.0, "still toward where the view looks: {c:?}");
    // Edge-on: under the target.
    let c = grid_centre(tf, Vec3::Y, n, 120.0);
    assert!(c.len() < 1e-9, "{c:?}");
}

#[test]
fn ground_grid_centre_is_continuous_across_the_horizon() {
    for normal in [Vec3::Y, Vec3::Z] {
        for height in [-13.0, 0.0, 13.0] {
            let target = Vec3::X * 2.0 + normal * height;
            for extent in [120.0, 600.0] {
                let centre = |step: i32| {
                    let pitch = f64::from(step) * 0.0001;
                    grid_centre(target, Vec3::X * pitch.cos() + normal * pitch.sin(), normal, extent)
                };
                for step in -20..20 {
                    let (a, b) = (centre(step), centre(step + 1));
                    assert!(a.dot(normal).abs() < 1e-9 && b.dot(normal).abs() < 1e-9);
                    assert!(
                        a.dist(b) < 0.1,
                        "a tiny orbit must not move the grid away: height {height}, extent {extent}, step {step}: {a:?} -> {b:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn grazing_ground_grid_keeps_the_point_under_the_target_visible() {
    let half_height = 124.55908448439891 * 0.4;
    let major = grid_step(half_height).1;
    for normal in [Vec3::Y, Vec3::Z] {
        for height in [-12.966269622652957, 12.966269622652957] {
            let target = Vec3::X * 2.1333385556362745 + normal * height;
            let under = target - normal * height;
            for pitch in [-0.125_f64, -0.08, -0.02, 0.0, 0.02, 0.08, 0.125] {
                let back = Vec3::X * pitch.cos() + normal * pitch.sin();
                let slant = back.dot(normal);
                let ext = grid_ext(half_height, major, slant);
                let centre = grid_centre(target, back, normal, ext);
                let fade = GridFade::new(half_height, slant, ext, f64::INFINITY);
                assert_eq!(fade.alpha(centre.dist(under)), 1.0, "the grid must not recede from the model: height {height}, pitch {pitch}");
            }
        }
    }
}

/// #3087: zoomed in on a target below the ground (an orbit left it there), seen at a slant,
/// the grid still covers the ground in the middle of the view: where the view's axis meets
/// it, some fifty millimetres from the point under the target, round which it was drawn.
#[test]
fn the_grid_covers_the_middle_of_the_view_when_the_target_is_off_the_ground() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.show_grid = true;
    app.ui.perspective = false;
    // The camera of the report.
    app.cam.target = Vec3::new(-38.888, -34.247, -28.617);
    app.cam.yaw = -2.288;
    app.cam.pitch = 0.504;
    app.cam.distance = 25.563;
    let back = app.cam.back();
    let mid = app.cam.target - back * (app.cam.target.z / back.z);
    assert!(mid.z.abs() < 1e-9 && (mid - app.cam.target).len() > 40.0, "{mid:?}");
    let under = build_sketch_scene(&app, &projection(&app, rect)).under;
    let points: Vec<[f32; 3]> = under
        .as_chunks::<32>()
        .0
        .iter()
        .flat_map(|l| [&l[0..12], &l[12..24]])
        .map(|b| [0, 4, 8].map(|i| f32::from_le_bytes(b[i..i + 4].try_into().unwrap())))
        .collect();
    let (lo, hi) = points
        .iter()
        .fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
    let (x, y) = (mid.x as f32, mid.y as f32);
    assert!(x > lo[0] && x < hi[0] && y > lo[1] && y < hi[1], "the grid spans {lo:?}..{hi:?}, the middle of the view is at {mid:?}");
}

/// #4214: the grid used to stop at a fixed distance from the view, so its edge showed as a
/// cut across the picture. It now fades out with the distance from the middle of the view:
/// the lines near the middle are at full strength, the ones further out are dimmer, and the
/// grid is gone before the distance where it used to stop.
#[test]
fn the_grid_fades_out_with_the_distance_from_the_middle_of_the_view() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.show_grid = true;
    app.ui.perspective = false;
    // A top view: the ground is the XY plane, and the grid is centred on the target.
    app.cam.pitch = std::f64::consts::FRAC_PI_2;
    app.cam.yaw = 0.0;
    app.cam.target = Vec3::ZERO;
    app.cam.distance = 100.0;
    let scene = build_sketch_scene(&app, &projection(&app, rect));
    let under = &scene.under;
    // Each line of the grid: two points, a colour and a width.
    let segments: Vec<([f32; 3], [f32; 3], u8)> = under
        .as_chunks::<32>()
        .0
        .iter()
        .map(|l| {
            let point = |at: usize| [0, 4, 8].map(|i| f32::from_le_bytes(l[at + i..at + i + 4].try_into().unwrap()));
            let index = u32::from_le_bytes(l[24..28].try_into().unwrap());
            let alpha = scene.palette.get(crate::gpu::PaletteIndex(index)).map_or(0, |c| c[3]);
            (point(0), point(12), alpha)
        })
        .collect();
    assert!(!segments.is_empty(), "the grid is drawn");
    // How far each piece of the grid is from the middle of the view, and the alpha it is
    // drawn with.
    let mut pieces: Vec<(f64, u8)> = segments
        .iter()
        .map(|(a, b, alpha)| {
            let mid = [0, 1, 2].map(|i| f64::from((a[i] + b[i]) * 0.5));
            ((mid[0] * mid[0] + mid[1] * mid[1]).sqrt(), *alpha)
        })
        .collect();
    pieces.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // The grid in the middle of the view is at full strength.
    assert_eq!(pieces.iter().filter(|(d, _)| *d < 1.0).map(|(_, a)| *a).max(), Some(255), "the middle of the grid is at full strength");
    // The further from the middle, the dimmer: the outer half of the grid is fainter than the
    // inner half.
    let (near, far) = pieces.split_at(pieces.len() / 2);
    let mean = |v: &[(f64, u8)]| v.iter().map(|(_, a)| f64::from(*a)).sum::<f64>() / v.len().max(1) as f64;
    assert!(mean(near) > mean(far) + 10.0, "the grid fades: {} near the middle, {} far from it", mean(near), mean(far));
    // The grid is gone before the distance where it used to stop, so its edge never shows.
    let (_, major) = grid_step(app.cam.half_height());
    let cut = (app.cam.half_height() * 3.0 / major).ceil() * major;
    let farthest = pieces.last().map(|(d, _)| *d).unwrap_or(0.0);
    assert!(farthest < cut, "the grid fades out at {farthest}, short of the cut at {cut}");
}

/// #9570: zooming through a switch of the grid's spacing swapped the whole grid at once. At
/// a nested switch the window now draws both levels at half strength, each on its own
/// places; at the switch from 2 to 5, which do not nest, neither level's own lines; and a
/// line two levels share with different kinds takes a colour between the two.
#[test]
fn the_grid_draws_both_levels_at_a_switch_of_its_spacing() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    // The strongest alpha a line across X at `x` is drawn with, at a half height of `hh`.
    let alpha_at = |hh: f64, x: f32| {
        let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        app.ui.show_grid = true;
        app.ui.perspective = false;
        app.cam.pitch = std::f64::consts::FRAC_PI_2;
        app.cam.yaw = 0.0;
        app.cam.target = Vec3::ZERO;
        app.cam.distance = 1.0;
        app.cam.distance = hh / app.cam.half_height();
        let scene = build_sketch_scene(&app, &projection(&app, rect));
        scene
            .under
            .as_chunks::<32>()
            .0
            .iter()
            .filter(|l| {
                let f = |at: usize| f32::from_le_bytes(l[at..at + 4].try_into().unwrap());
                (f(0) - x).abs() < 1e-4 && (f(12) - x).abs() < 1e-4
            })
            .filter_map(|l| scene.palette.get(crate::gpu::PaletteIndex(u32::from_le_bytes(l[24..28].try_into().unwrap()))).map(|c| c[3]))
            .max()
            .unwrap_or(0)
    };
    let full = u16::from(c4(Tokens::get().grid)[3]);
    // A half height where the spacing switches from 5 to 10 mm (nested): the 5 mm lines
    // cross-fade at about half strength.
    let a = u16::from(alpha_at(100.0, 5.0));
    assert!(a > full / 4 && a < full * 3 / 4, "the line at 5 mm is drawn at {a} of {full}: both levels cross-fade");
    // From 2 to 5 mm (not nested) the finer level's own lines are gone before the coarser
    // level's own lines appear: right at the switch neither is drawn, the shared 10 mm are.
    for x in [2.0, 5.0] {
        assert_eq!(alpha_at(50.0, x), 0, "the line at {x} mm is drawn while 2 and 5 swap");
    }
    assert!(alpha_at(50.0, 10.0) > 0);
    assert_eq!(grid_blend([0, 0, 0, 0], [200, 100, 50, 255], 0.5), [100, 50, 25, 128]);
    assert_eq!(grid_blend([1, 2, 3, 4], [9, 9, 9, 9], 0.0), [1, 2, 3, 4]);
    assert_eq!(grid_blend([1, 2, 3, 4], [9, 9, 9, 9], f32::NAN), [1, 2, 3, 4]);
}

/// #4214: the picture and the window draw the same grid. On a sketch's plane the headless
/// render's grid used to reach further than the viewport's (the picture stretched the grid
/// with the slant, the window did not), so a picture saved from the command line and the
/// window the user sees showed different grids.
#[test]
fn the_picture_and_the_window_draw_the_same_grid_on_a_sketch_plane() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    app.ui.perspective = false;
    app.cam.fov = 0.0;
    app.cam.yaw = 0.0;
    app.cam.pitch = 35.0_f64.to_radians();
    app.cam.target = Vec3::ZERO;
    app.cam.distance = 100.0;
    // The grid's centre, as both draw it: the sketch plane's, round the view's target.
    let plane = sketcraft_engine::view::open_sketch_plane(&app.session).unwrap().unwrap();
    let (_, major) = grid_step(app.cam.half_height());
    let (centre, _, _) = sketcraft_engine::view::sketch_grid_frame(&plane, app.cam.target, major);
    // How far the picture's grid reaches from that centre.
    let scene = sketcraft_engine::view::scene(&app.session, &app.cam).unwrap();
    let picture = scene.grid.iter().flat_map(|l| l.points.iter()).map(|p| (*p - centre).len()).fold(0.0_f64, f64::max);
    // How far the window's grid reaches: the same points, as bytes.
    let under = build_sketch_scene(&app, &projection(&app, rect)).under;
    let window = under
        .as_chunks::<32>()
        .0
        .iter()
        .flat_map(|l| [&l[0..12], &l[12..24]])
        .map(|b| {
            let p = [0, 4, 8].map(|i| f64::from(f32::from_le_bytes(b[i..i + 4].try_into().unwrap())));
            (Vec3::new(p[0], p[1], p[2]) - centre).len()
        })
        .fold(0.0_f64, f64::max);
    assert!(picture > 0.0, "the picture draws a grid");
    assert!(
        (picture - window).abs() <= 1e-3 * picture.max(window),
        "the picture's grid reaches {picture} mm from the middle of the view, the window's {window} mm"
    );
}

/// #4214: the grid fades in the picture as well. The same grid drawn at full strength is
/// much fainter there where the fade has thinned it, and the same in the middle of the view,
/// where the fade leaves the grid alone. Where the picture's far plane cuts the ground, the
/// faded grid is already gone: it fades out instead of ending in a cut.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_grid_fades_out_in_the_picture_instead_of_ending_in_a_cut() {
    const SIZE: u32 = 512;
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(SIZE as f32, SIZE as f32));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.show_grid = true;
    app.ui.perspective = true;
    // A low view of the ground: the far ground is in the picture, where the grid used to end
    // in a cut.
    app.cam.pitch = 0.25;
    app.cam.yaw = 0.0;
    app.cam.target = Vec3::ZERO;
    app.cam.distance = 100.0;
    let proj = projection(&app, rect);
    let t = Tokens::get();
    let e = proj.cam.eye().to_f32();
    let eye = [e[0], e[1], e[2], if proj.cam.fov > 0.0 { 1.0 } else { 0.0 }];
    let render = |scene: GpuScene| {
        crate::gpu::render_offscreen(
            scene,
            None,
            proj.vp.to_f32(),
            proj.cam.back().to_f32(),
            eye,
            SIZE,
            None,
            [0.0; 8],
            crate::gpu::analysis_colors(&t.analysis),
        )
    };
    let faded = build_sketch_scene(&app, &proj).merged();
    // The same grid at full strength: every piece of it opaque.
    let mut full = faded.clone();
    let palette = std::sync::Arc::make_mut(&mut full.palette);
    for piece in full.under.as_chunks::<32>().0 {
        let index = u32::from_le_bytes(piece[24..28].try_into().unwrap());
        let slot = crate::gpu::PaletteIndex(index);
        if let Some(mut colour) = palette.get(slot) {
            colour[3] = 255;
            palette.set(slot, colour);
        }
    }
    let (Some(faded), Some(full)) = (render(faded), render(full)) else {
        eprintln!("skipping: no GPU adapter");
        return;
    };
    // How far the pixels of each row are from the white the picture was cleared to: the
    // grid's lines show. The ink of a row, from the top of the picture.
    let row_ink = |img: &[u8]| -> Vec<u64> {
        (0..SIZE)
            .map(|y| {
                (0..SIZE)
                    .map(|x| {
                        let i = ((y * SIZE + x) * 4) as usize;
                        img[i..i + 3].iter().map(|c| u64::from(255 - c)).sum::<u64>()
                    })
                    .sum::<u64>()
            })
            .collect()
    };
    let (faded_rows, full_rows) = (row_ink(&faded), row_ink(&full));
    // The middle of the view: the grid is drawn there, and the fade leaves it alone.
    let middle = SIZE as usize / 2;
    assert!(faded_rows[middle] > 0, "the grid is drawn in the middle of the view");
    assert_eq!(faded_rows[middle], full_rows[middle], "the fade leaves the middle of the grid alone");
    // The first row where the grid at full strength shows: the picture's far plane cuts the
    // ground there, and the rows above it are the far ground the grid has to fade out in.
    let first = full_rows.iter().position(|ink| *ink > 0).expect("the grid is drawn");
    // Where the ground is cut, the faded grid is gone: it fades out instead of ending in a
    // cut (#4214).
    let band = first.saturating_sub(10)..=first;
    let faded_ink: u64 = band.clone().map(|y| faded_rows[y]).sum();
    let full_ink: u64 = band.map(|y| full_rows[y]).sum();
    assert!(full_ink > 0, "the grid shows up to the far plane");
    assert!(
        faded_ink * 4 < full_ink,
        "the grid still shows at full strength where the far plane cuts the ground: {faded_ink} against {full_ink}"
    );
}

/// An open sketch has its grid, on its own plane, whatever the Grid switch and the
/// follow-the-view preference say; outside a sketch the switch decides.
#[test]
fn an_open_sketch_always_shows_a_grid_on_its_plane() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.show_grid = false;
    let grid = |app: &SolveApp| build_sketch_scene(app, &projection(app, rect)).under;
    assert!(grid(&app).is_empty(), "no grid outside a sketch with the switch off");
    app.run("sketch.create", json!({"plane": "XZ"})).unwrap();
    for follows in [false, true] {
        app.preferences.grid_follows_view = follows;
        let under = grid(&app);
        assert!(!under.is_empty(), "the open sketch shows a grid (follows the view: {follows})");
        // Each line is 32 bytes: two f32 points, a colour and a width; the XZ plane is Y = 0.
        let ys: Vec<f32> =
            under.as_chunks::<32>().0.iter().flat_map(|l| [&l[4..8], &l[16..20]]).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        assert!(ys.iter().all(|y| y.abs() < 1e-3), "the grid lies on the sketch plane");
    }
    app.ui.show_grid = true;
    assert!(!grid(&app).is_empty());
}

/// The Sketch panel's Sketch Grid box turns the open sketch's grid off, whatever the Grid
/// switch says, in the window and in what renders and the control channel are told; on
/// again, it is back, and outside the sketch the switch decides as before (#9147).
#[test]
fn the_sketch_grid_option_turns_the_open_sketchs_grid_off() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let grid = |app: &SolveApp| !build_sketch_scene(app, &projection(app, rect)).under.is_empty();
    for switch in [false, true] {
        let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        app.ui.show_grid = switch;
        app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        assert!(grid(&app) && crate::control::window_guides(&app).grid, "switch {switch}: the sketch's grid shows");
        app.run("sketch.options", json!({"grid": false})).unwrap();
        assert!(!grid(&app), "switch {switch}: the option turned the grid off");
        assert!(!crate::control::window_guides(&app).grid, "switch {switch}");
        app.run("sketch.options", json!({"grid": true})).unwrap();
        assert!(grid(&app) && crate::control::window_guides(&app).grid, "switch {switch}: on again");
        app.run("sketch.options", json!({"grid": false})).unwrap();
        app.run("sketch.finish", json!({})).unwrap();
        assert_eq!(grid(&app), switch, "outside the sketch the switch decides");
    }
}

#[test]
fn imported_scene_projection_toggle_keeps_front_components_in_clip_range() {
    use sketcraft_engine::doc::{Document, FeatureKind, MeshData};
    let mut doc = Document::default();
    let meshes = [-10_000.0, 10_000.0]
        .into_iter()
        .enumerate()
        .map(|(i, y)| MeshData {
            name: format!("Panel {i}"),
            positions: vec![-0.25, y, -0.25, 0.25, y, -0.25, 0.0, y, 0.25].into(),
            triangles: vec![0, 1, 2].into(),
            color: None,
        })
        .collect();
    doc.add_feature(FeatureKind::MeshImport { file: "synthetic.stl".into(), meshes }, Some("Panels")).unwrap();
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::new(doc));
    app.cam.yaw = 0.0;
    app.cam.pitch = 0.0;
    app.fit_view();
    app.cancel_view_animation();
    app.cam.distance = 10_002.0;
    assert_eq!(crate::open_meshing::session_world(&app.session).bodies.len(), 2);
    let camera = app.cam;
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0));
    let original = projection(&app, rect).vp;
    for shadow in [false, true] {
        app.ui.ground_shadow = shadow;
        for perspective in [false, true, false] {
            app.ui.perspective = perspective;
            let proj = projection(&app, rect);
            for body in &crate::open_meshing::session_world(&app.session).bodies {
                let (mesh, _) = body.mesh_in_place();
                assert_eq!(mesh.positions.len(), 3);
                for p in &mesh.positions {
                    let c = proj.vp.apply(*p);
                    assert!(c.iter().all(|v| v.is_finite()));
                    assert!(c[3] > 0.0 && (0.0..=c[3]).contains(&c[2]), "shadow={shadow}, perspective={perspective}, {p:?}: {c:?}");
                    assert!(c[0].abs() <= c[3] && c[1].abs() <= c[3]);
                }
            }
            assert_eq!(app.cam, camera, "changing projection preserves the orbit camera");
            if !perspective {
                assert_eq!(proj.vp, original);
            }
        }
    }
}

/// A sketch is drawn in a parallel view even when the preference asks for perspective; the
/// preference comes back when the sketch is finished.
#[test]
fn an_open_sketch_is_drawn_orthographic() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.ui.perspective = true;
    assert!(projection(&app, rect).cam.fov > 0.0);
    app.run("sketch.create", json!({"plane": "XZ"})).unwrap();
    assert!(!app.perspective_view());
    assert_eq!(projection(&app, rect).cam.fov, 0.0);
    app.finish_sketch();
    assert!(app.ui.perspective, "the preference is left alone");
    assert!(projection(&app, rect).cam.fov > 0.0);
}

/// #2682: with the auto mode on, a camera facing a standard axis view draws orthographic and
/// an off-axis one draws perspective; with the mode off the manual preference rules again.
#[test]
fn auto_ortho_faces_follows_camera_alignment() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::default());
    assert!(!app.ui.auto_ortho_faces, "the option is off by default");
    app.ui.auto_ortho_faces = true;
    for view in [StandardView::Front, StandardView::Top, StandardView::Left] {
        app.cam.set_view(view);
        assert_eq!(projection(&app, rect).cam.fov, 0.0, "{view:?} is orthographic");
    }
    // Orbiting off the face returns to perspective immediately.
    app.cam.yaw += 0.3;
    app.cam.pitch += 0.2;
    assert!(projection(&app, rect).cam.fov > 0.0, "off-axis is perspective");
    // Snapping back to a face (a Shift-snapped orbit or Home-style view) restores it.
    app.cam.set_view(StandardView::Left);
    assert_eq!(projection(&app, rect).cam.fov, 0.0);
    // The preference round-trips through the saved prefs.
    let saved = app.prefs();
    let mut fresh = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    fresh.load_prefs(&saved);
    assert!(fresh.ui.auto_ortho_faces, "the option is kept between runs");
    // With the mode off, the manual preference rules again (orthographic in the classic view).
    app.ui.auto_ortho_faces = false;
    assert_eq!(projection(&app, rect).cam.fov, 0.0);
    app.ui.perspective = true;
    assert!(projection(&app, rect).cam.fov > 0.0, "manual perspective is back");
    // An open sketch still wins: always orthographic.
    app.run("sketch.create", json!({"plane": "XZ"})).unwrap();
    assert_eq!(projection(&app, rect).cam.fov, 0.0);
}

#[test]
fn perspective_button_is_inert_while_editing_a_sketch() {
    for mode in [crate::shell::LayoutMode::Floating, crate::shell::LayoutMode::Docked] {
        for perspective in [false, true] {
            let mut h = crate::scenario::Harness::new();
            h.call("ui.set", json!({"layout":mode,"perspective":perspective}));
            h.app.preferences.navigation_bar.slots = vec!["projection".into()];
            h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
            h.settle();
            crate::test_support::click(&mut h, "nav:slot:projection");
            assert_eq!(h.app.ui.perspective, perspective, "a sketch's disabled control must preserve the normal view preference");
            let rect = h.app.viewport.rect.unwrap();
            assert_eq!(projection(&h.app, rect).cam.fov, 0.0);

            h.app.finish_sketch();
            h.settle();
            assert_eq!(h.app.perspective_view(), perspective, "finishing restores the original projection");
            crate::test_support::click(&mut h, "nav:slot:projection");
            assert_eq!(h.app.perspective_view(), !perspective, "the toggle works again outside sketch editing");
        }
    }
}

/// The Field of view preference (#5364) is the angle the view draws with: the default draws as
/// before (0.6), and at both ends of its range a framed box is wholly in front of the near plane
/// and inside the view.
#[test]
fn the_field_of_view_preference_drives_the_projection() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::default());
    app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    assert_eq!(projection(&app, rect).cam.fov, 0.0, "Perspective view off: parallel");
    app.ui.perspective = true;
    assert_eq!(projection(&app, rect).cam.fov, 0.6, "the default draws as before");
    for fov in [15f64.to_radians(), 90f64.to_radians()] {
        app.preferences.perspective_fov = fov;
        app.cam = app.fitted_bounds(app.cam, &model_bounds(&app));
        let p = projection(&app, rect);
        assert_eq!(p.cam.fov, fov);
        let b = model_bounds(&app);
        for corner in [b.min, b.max, Vec3::new(b.min.x, b.max.y, b.min.z), Vec3::new(b.max.x, b.min.y, b.max.z)] {
            let (x, y, depth) = p.cam.to_screen(corner, 800.0, 600.0, scene_radius(&app)).expect("in front of the eye");
            assert!(
                (0.0..=1.0).contains(&depth) && (0.0..=800.0).contains(&x) && (0.0..=600.0).contains(&y),
                "{fov}: {corner:?} at {x}, {y}, {depth}"
            );
        }
    }
}

/// Create Sketch shows the Origin through bodies; the preference opts into that outside the
/// tool while remaining off by default.
#[test]
fn create_sketch_shows_the_origin_planes_through_the_model() {
    let mut app = crate::test_support::classic_app(sketcraft_engine::Session::default());
    app.run("solid.box", json!({"length":40,"width":40,"height":40})).unwrap();
    app.set_origin_shown(false);
    assert!(origin_planes(&app).is_empty());
    assert!(build_highlight(&app).xray.is_empty());
    app.start("sketch.create");
    assert!(app.picking_sketch_plane());
    assert_eq!(origin_planes(&app).len(), 3, "every origin plane is offered");
    let sc = build_highlight(&app);
    assert_eq!(sc.xray.len(), 3 * 6 * (6 * 4 + 4), "each plane's two triangles are drawn through the model");
    assert!(sc.guide_trans.is_empty());
    app.dialog = None;
    assert!(origin_planes(&app).is_empty());
    app.set_origin_shown(true);
    let sc = build_highlight(&app);
    assert!(sc.xray.is_empty(), "through-body reference geometry is off by default");
    assert_eq!(sc.guide_trans.len(), 3 * 6 * crate::gpu::TRI_SIZE, "planes use the ordinary depth-tested path by default");
    app.preferences.reference_geometry_through_bodies = true;
    let sc = build_highlight(&app);
    assert_eq!(sc.xray.len(), 3 * 6 * crate::gpu::TRI_SIZE, "the preference draws reference planes through bodies");
    assert!(sc.guide_trans.is_empty());
}

/// #6975: while Create Sketch waits for its plane, the plane under the pointer is lit, and a
/// plane's browser row under the pointer lights it the same way.
#[test]
fn create_sketch_lights_the_plane_under_the_pointer_or_its_browser_row() {
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.start("sketch.create");
    let plain = build_highlight(&app).xray;
    app.viewport.hover = Some(Hit::Plane { name: "XZ".into(), point: Vec3::ZERO });
    let lit = build_highlight(&app).xray;
    assert_ne!(lit, plain, "the plane under the pointer is lit");
    app.viewport.hover = None;
    let key = highlight_key(&app);
    app.viewport.hover_plane = Some("XZ".into());
    assert_ne!(highlight_key(&app), key, "the highlight is built again");
    assert_eq!(build_highlight(&app).xray, lit, "its browser row lights it the same way");
}

/// #6975: New Sketch shows the origin planes; the plane under the pointer, in the view or as
/// its row in the browser, is the one previewed, and a click sketches on it.
#[test]
fn create_sketch_previews_the_hovered_plane_and_a_click_sketches_on_it() {
    let mut h = crate::scenario::Harness::new();
    h.app.tree.expanded.insert("origin".into());
    h.step(&json!({"start":"sketch.create"})).unwrap();
    assert_eq!(origin_planes(&h.app).len(), 3, "the origin planes are in the scene while choosing");
    assert!(previewed_sketch_plane(&h.app).is_none(), "nothing under the pointer yet");
    // Whether `plane` lies on origin plane `name`.
    let on = |plane: Option<Plane>, name: &str| {
        plane.zip(Plane::named(name)).is_some_and(|(a, b)| a.normal().dot(b.normal()).abs() > 0.999 && a.origin.dot(b.normal()).abs() < 1e-9)
    };
    h.step(&json!({"move":{"plane":"YZ"}})).unwrap();
    assert!(on(previewed_sketch_plane(&h.app), "YZ"), "YZ under the pointer in the view");
    let row = crate::scenario::handle_at("row:XY").expect("the XY row");
    h.call("ui.move", json!({"x": row.x, "y": row.y}));
    assert_eq!(h.app.viewport.hover_plane.as_deref(), Some("XY"));
    assert!(on(previewed_sketch_plane(&h.app), "XY"), "XY under the pointer in the browser");
    h.step(&json!({"click":{"plane":"XZ"}})).unwrap();
    let state = crate::open_meshing::session_world(&h.app.session);
    assert!(on(state.sketches.first().map(|s| s.plane), "XZ"), "the click sketches on XZ");
    assert!(previewed_sketch_plane(&h.app).is_none(), "the open sketch has its own grid");
}

/// A finished sketch is depth-tested unless the preference draws reference geometry through
/// bodies; switching it redraws the sketch scene while nothing else changes.
#[test]
fn the_preference_draws_finished_sketches_through_bodies() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    app.run("sketch.rectangle.two_point", json!({"p0": [-10, -10], "p1": [10, 10]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    let proj = projection(&app, rect);
    let off = build_sketch_scene(&app, &proj);
    assert!(!off.lines.is_empty() && off.overlay.is_empty(), "off by default: the sketch is depth-tested");
    let key = sketch_scene_key(&app, &proj);
    app.preferences.reference_geometry_through_bodies = true;
    assert_ne!(sketch_scene_key(&app, &proj), key, "the cached sketch scene is rebuilt");
    let on = build_sketch_scene(&app, &proj);
    assert!(on.lines.is_empty() && on.overlay.len() == off.lines.len(), "the sketch is drawn over the bodies");
}

/// #2581: a dialog waiting for a plane (here Mirror's plane) shows the origin planes in the
/// view as Create Sketch does, so the plane can be picked there; while it waits for its
/// objects it doesn't.
#[test]
fn a_dialog_picking_a_plane_shows_the_origin_planes() {
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("solid.box", json!({"length":40,"width":40,"height":40})).unwrap();
    app.set_origin_shown(false);
    app.start("solid.mirror");
    let d = app.dialog.as_ref().expect("the Mirror dialog");
    assert_eq!(d.active, 0, "it starts on its objects");
    assert!(!app.picking_plane() && origin_planes(&app).is_empty(), "no planes while picking objects");
    app.dialog.as_mut().unwrap().active = 1;
    assert!(app.picking_plane());
    assert_eq!(origin_planes(&app).len(), 3, "every origin plane is offered for the mirror plane");
    assert!(!build_highlight(&app).xray.is_empty(), "drawn through the model");
    app.dialog = None;
    assert!(origin_planes(&app).is_empty());
}

/// #10778: Project's pick takes origin planes, but it must not bring hidden origin planes
/// back: every line a plane projects, an origin axis already gives. The axes and point stay
/// as today; other dialogs that pick a plane keep the reveal (#2581).
#[test]
fn project_leaves_hidden_origin_planes_hidden() {
    let mut app = SolveApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.run("solid.box", json!({"length":40,"width":40,"height":40})).unwrap();
    app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    app.set_origin_shown(false);
    app.start("sketch.project");
    assert!(app.picking_project_geometry());
    assert!(app.picking_plane(), "Project's pick takes origin planes");
    assert!(origin_planes(&app).is_empty(), "Project leaves hidden origin planes hidden");
    assert!(!origin_axes(&app).is_empty(), "the origin axes stay as today");
    // Other dialogs that pick a plane keep today's reveal.
    app.dialog = None;
    app.start("solid.mirror");
    app.dialog.as_mut().unwrap().active = 1;
    assert_eq!(origin_planes(&app).len(), 3, "Mirror still offers every origin plane");
    app.dialog = None;
    app.start("sketch.create");
    assert_eq!(origin_planes(&app).len(), 3, "Create Sketch still offers every origin plane");
}

/// Primary rolls a live preview back for reselect, but not while a manipulator is dragged
/// (same gate [`show`] uses via [`preview_should_roll_back`]).
#[test]
fn preview_rollback_stays_off_while_manipulator_dragging() {
    let mut s = sketcraft_engine::Session::default();
    s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    s.execute("sketch.line", &json!({"points": [[0, 0], [20, 0]]})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    let mut app = SolveApp::new(s, crate::Services::default());
    app.dialog = crate::dialogs::Dialog::for_command(&app, "solid.extrude");
    assert!(app.dialog.as_ref().is_some_and(|d| d.previews()), "extrude opens a preview dialog");
    app.preview.active = true;
    let primary = crate::tool_modifiers::Held { primary: true, ..Default::default() };
    assert!(preview_should_roll_back(&app, primary, false), "Primary alone rolls the preview back when a preview dialog is open");
    assert!(!preview_should_roll_back(&app, primary, true), "Primary must not roll the preview back while a manipulator arrow is dragged");
}
