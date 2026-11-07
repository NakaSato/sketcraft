#[path = "renderer_mesh_buffer_tests.rs"]
mod mesh_buffers_tests;
#[path = "renderer_parallel_tests.rs"]
mod parallel_tests;
#[path = "renderer_joint_tests.rs"]
mod joints_tests;
#[path = "renderer_shadow_tests.rs"]
mod shadows_tests;
#[path = "renderer_occlusion_tests.rs"]
mod occlusion_tests;

use super::*;

fn occluding_rectangle(occlusion: &mut Occlusion, lo: DVec2, hi: DVec2, depth: f64) {
    let p = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)].map(|p| p.extend(depth));
    occlusion.insert([p[0], p[1], p[2]]);
    occlusion.insert([p[0], p[2], p[3]]);
}

fn polygon_area(polygons: &[Vec<DVec2>]) -> f64 {
    polygons
        .iter()
        .map(|p| {
            (0..p.len())
                .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
                .sum::<f64>()
                .abs()
                * 0.5
        })
        .sum()
}
