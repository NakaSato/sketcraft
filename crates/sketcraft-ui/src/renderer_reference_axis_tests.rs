use super::*;

#[test]
fn axes_fade_without_changing_projected_depth_or_direction_colors() {
    for projection in [
        ketchup_core::Projection::Perspective,
        ketchup_core::Projection::Parallel,
    ] {
        for distance in [0.1, 15., 1000.] {
            for size in [DVec2::new(1200., 800.), DVec2::new(400., 900.)] {
                let camera = Camera {
                    projection,
                    distance,
                    ..Default::default()
                };
                let fade = ReferenceFade::new(&camera);
                let direction = (camera.target - camera.eye()).normalize();
                // A diagonal receding reference line remains visible over a
                // useful depth range in both projection modes.
                let side = direction.cross(DVec3::Z).normalize();
                let a = DVec3::ZERO;
                let b = a + (direction + side * 0.1) * distance * 8.;
                for color in [Color32::RED, Color32::GREEN, Color32::BLUE] {
                    let mut faded = Mesh::new(&camera, size);
                    faded.axis_line(&camera, a, b, 1.1, color);
                    assert!(!faded.vertices.is_empty());
                    let alpha: Vec<_> = faded.vertices.iter().map(|v| v.color[3]).collect();
                    assert!(alpha.windows(2).all(|v| v[0] >= v[1]));
                    assert!(alpha.last().unwrap() < alpha.first().unwrap());
                    for v in &faded.vertices {
                        assert!(v.position.iter().all(|x| x.is_finite()));
                        for channel in 0..3 {
                            if color[channel] == 0 {
                                assert_eq!(v.color[channel], 0);
                            }
                        }
                    }
                }
                assert!(
                    ReferenceFade::opacity(fade.depth(DVec3::ZERO), fade.origin_depth) >= 0.999
                );
                assert!(fade.axis_length() > camera.eye().length() + distance * 200.);
            }
        }
    }
}
