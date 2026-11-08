//! Drawing edges as SketchUp does: welding, splitting, and faces appearing, dividing and taking
//! holes. Expected counts and areas are worked out by hand.

use super::*;
use sketcraft_geom::prelude::*;

fn p(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A closed loop of edges through `points`, on the ground.
fn polygon(m: &mut Mesh, points: &[(f64, f64)]) {
    for (i, (x, y)) in points.iter().enumerate() {
        let (u, v) = points[(i + 1) % points.len()];
        m.add_edge(p(*x, *y, 0.0), p(u, v, 0.0)).unwrap();
    }
}

fn rect(m: &mut Mesh, x0: f64, y0: f64, x1: f64, y1: f64) {
    polygon(m, &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
}

/// A face's area, its holes taken out (shoelace in the face's plane, which is the ground here).
fn area(m: &Mesh, f: &Face) -> f64 {
    let ring_area = |ring: &Vec<VertexId>| {
        let pts: Vec<Vec3> = ring.iter().map(|v| m.position(*v).unwrap()).collect();
        let mut a = 0.0;
        for (i, q) in pts.iter().enumerate() {
            let r = pts[(i + 1) % pts.len()];
            a += q.x * r.y - r.x * q.y;
        }
        (a / 2.0).abs()
    };
    ring_area(&f.outer) - f.holes.iter().map(ring_area).sum::<f64>()
}

fn areas(m: &Mesh) -> Vec<f64> {
    let mut v: Vec<f64> = m.faces().map(|(_, f)| (area(m, f) * 1e6).round() / 1e6).collect();
    v.sort_by(f64::total_cmp);
    v
}

#[test]
fn four_edges_closing_a_rectangle_make_a_face() {
    let mut m = Mesh::new();
    m.add_edge(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0)).unwrap();
    m.add_edge(p(20.0, 0.0, 0.0), p(20.0, 10.0, 0.0)).unwrap();
    m.add_edge(p(20.0, 10.0, 0.0), p(0.0, 10.0, 0.0)).unwrap();
    assert_eq!(m.faces().count(), 0, "three edges close nothing");
    m.add_edge(p(0.0, 10.0, 0.0), p(0.0, 0.0, 0.0)).unwrap();
    assert_eq!(m.vertices().count(), 4, "the ends weld");
    assert_eq!(areas(&m), [200.0]);
    let (_, f) = m.faces().next().unwrap();
    assert!(f.normal.z.abs() > 1.0 - 1e-12, "{:?}", f.normal);
    // Every edge knows the face.
    assert!(m.edges().all(|(e, _)| m.faces_of(e).count() == 1));
}

#[test]
fn ends_within_the_tolerance_weld_to_the_vertex_there() {
    let mut m = Mesh::new();
    m.add_edge(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0)).unwrap();
    m.add_edge(p(10.0 + TOLERANCE / 2.0, 0.0, 0.0), p(10.0, 10.0, 0.0)).unwrap();
    assert_eq!(m.vertices().count(), 3);
}

#[test]
fn an_edge_across_a_face_divides_it() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 20.0, 10.0);
    m.add_edge(p(5.0, 0.0, 0.0), p(5.0, 10.0, 0.0)).unwrap();
    assert_eq!(areas(&m), [50.0, 150.0], "the 200 mm² face is replaced by its two sides");
    // The divider split the two long edges where it starts and ends.
    assert_eq!(m.edges().count(), 7);
}

#[test]
fn rectangles_sharing_a_side_make_two_faces_not_their_outline() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 10.0, 10.0);
    rect(&mut m, 10.0, 0.0, 20.0, 10.0);
    assert_eq!(areas(&m), [100.0, 100.0]);
    let shared = m.edges().find(|(_, e)| {
        let (a, b) = (m.position(e.a).unwrap(), m.position(e.b).unwrap());
        a.x == 10.0 && b.x == 10.0
    });
    assert_eq!(m.faces_of(shared.unwrap().0).count(), 2, "the shared side bounds both");
}

#[test]
fn a_loop_drawn_inside_a_face_becomes_a_hole_and_a_face() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 30.0, 30.0);
    rect(&mut m, 10.0, 10.0, 20.0, 20.0);
    assert_eq!(areas(&m), [100.0, 800.0], "the inner square, and the outer one with it taken out");
    let outer = m.faces().map(|(_, f)| f).find(|f| f.holes.len() == 1);
    assert!(outer.is_some(), "the outer face has the square as a hole");
    // Erasing the inner face leaves the hole, as in SketchUp.
    let inner = m.faces().find(|(_, f)| f.holes.is_empty()).map(|(id, _)| id).unwrap();
    m.erase_face(inner).unwrap();
    assert_eq!(areas(&m), [800.0]);
}

#[test]
fn dividing_a_face_with_a_hole_keeps_the_hole_on_its_side() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 40.0, 20.0);
    rect(&mut m, 25.0, 5.0, 35.0, 15.0);
    m.add_edge(p(20.0, 0.0, 0.0), p(20.0, 20.0, 0.0)).unwrap();
    // Left half 400; right half 400 less the 100 hole; the hole's own face 100.
    assert_eq!(areas(&m), [100.0, 300.0, 400.0]);
}

#[test]
fn an_edge_onto_another_splits_it_and_crossing_edges_split_both() {
    let mut m = Mesh::new();
    m.add_edge(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0)).unwrap();
    // A T: it ends on the first edge's middle.
    m.add_edge(p(5.0, 5.0, 0.0), p(5.0, 0.0, 0.0)).unwrap();
    assert_eq!((m.vertices().count(), m.edges().count()), (4, 3));
    // An X through the T's stem.
    let made = m.add_edge(p(0.0, 2.0, 0.0), p(10.0, 2.0, 0.0)).unwrap();
    assert_eq!(made.len(), 2, "the new edge is split where it crosses");
    assert_eq!((m.vertices().count(), m.edges().count()), (7, 6));
}

#[test]
fn a_loop_that_is_not_flat_makes_no_face() {
    let mut m = Mesh::new();
    let pts = [p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), p(10.0, 10.0, 3.0), p(0.0, 10.0, 0.0)];
    for i in 0..4 {
        m.add_edge(pts[i], pts[(i + 1) % 4]).unwrap();
    }
    assert_eq!(m.faces().count(), 0);
}

#[test]
fn faces_close_in_any_plane() {
    let mut m = Mesh::new();
    // A wall on XZ, and a tilted triangle.
    for (a, b) in [((0.0, 0.0), (10.0, 0.0)), ((10.0, 0.0), (10.0, 5.0)), ((10.0, 5.0), (0.0, 5.0)), ((0.0, 5.0), (0.0, 0.0))] {
        m.add_edge(p(a.0, 0.0, a.1), p(b.0, 0.0, b.1)).unwrap();
    }
    let t = [p(0.0, 20.0, 0.0), p(4.0, 20.0, 0.0), p(0.0, 23.0, 4.0)];
    for i in 0..3 {
        m.add_edge(t[i], t[(i + 1) % 3]).unwrap();
    }
    assert_eq!(m.faces().count(), 2);
    let wall = m.faces().map(|(_, f)| f).find(|f| f.normal.y.abs() > 0.99).unwrap();
    assert_eq!(wall.outer.len(), 4);
}

#[test]
fn erasing_an_edge_takes_its_faces_and_lone_vertices() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 10.0, 10.0);
    rect(&mut m, 10.0, 0.0, 20.0, 10.0);
    let right = m.edges().find(|(_, e)| m.position(e.a).unwrap().x == 20.0 && m.position(e.b).unwrap().x == 20.0).unwrap().0;
    m.erase_edge(right).unwrap();
    assert_eq!(areas(&m), [100.0], "the right square's face goes; the left one stays");
    assert_eq!(m.vertices().count(), 6, "no vertex is left alone");
    // A dangling edge alone: erasing it leaves nothing.
    let mut n = Mesh::new();
    let e = n.add_edge(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)).unwrap()[0];
    n.erase_edge(e).unwrap();
    assert_eq!((n.vertices().count(), n.edges().count()), (0, 0));
}

#[test]
fn hostile_points_are_refused() {
    let mut m = Mesh::new();
    assert_eq!(m.add_edge(p(f64::NAN, 0.0, 0.0), p(1.0, 0.0, 0.0)), Err(MeshError::BadPoint));
    assert_eq!(m.add_edge(p(1e10, 0.0, 0.0), p(1.0, 0.0, 0.0)), Err(MeshError::BadPoint));
    assert_eq!(m.add_edge(p(1.0, 0.0, 0.0), p(1.0, 0.0, TOLERANCE / 10.0)), Err(MeshError::ZeroLength));
    assert_eq!(m.vertices().count(), 0, "a refused edge leaves nothing behind");
}

#[test]
fn a_saved_model_opens_the_same_and_a_broken_one_is_refused() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 30.0, 30.0);
    rect(&mut m, 10.0, 10.0, 20.0, 20.0);
    let json = serde_json::to_string(&m).unwrap();
    let back: Mesh = serde_json::from_str(&json).unwrap();
    assert_eq!(areas(&back), areas(&m));
    assert!(back.edges().all(|(e, _)| back.faces_of(e).count() == m.faces_of(e).count()), "the lookups are made again");
    // New ids don't reuse saved ones.
    let mut back = back;
    let made = back.add_edge(p(40.0, 0.0, 0.0), p(50.0, 0.0, 0.0)).unwrap();
    assert!(m.edge(made[0]).is_none());
    // An edge to a vertex that isn't there.
    let fine = r#"{"vertices":{"1":{"position":[0,0,0]},"9":{"position":[1,0,0]}},"edges":{"2":{"a":1,"b":9}},"faces":{},"next":9}"#;
    assert!(serde_json::from_str::<Mesh>(fine).is_ok(), "the form the next check breaks");
    let broken = r#"{"vertices":{"1":{"position":[0,0,0]}},"edges":{"2":{"a":1,"b":9}},"faces":{},"next":2}"#;
    let why = serde_json::from_str::<Mesh>(broken).unwrap_err().to_string();
    assert!(why.contains("missing end"), "{why}");
}

/// The volume the faces enclose (divergence theorem: a third of each face's area times its
/// distance from the origin along its normal). Positive when every face points outwards.
fn volume(m: &Mesh) -> f64 {
    m.faces()
        .map(|(_, f)| {
            let p0 = m.position(f.outer[0]).unwrap();
            f.area(m) * p0.dot(f.normal) / 3.0
        })
        .sum()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn ground_rect() -> (Mesh, FaceId) {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 20.0, 10.0);
    let f = m.faces().next().unwrap().0;
    (m, f)
}

/// Pull a ground face up by `d` (its normal may point either way).
fn pull_up(m: &mut Mesh, f: FaceId, d: f64) -> FaceId {
    let up = m.face(f).unwrap().normal.z;
    m.push_pull(f, d * up.signum()).unwrap()
}

#[test]
fn pulling_a_rectangle_makes_a_box_facing_out() {
    let (mut m, f) = ground_rect();
    let top = pull_up(&mut m, f, 5.0);
    assert_eq!((m.faces().count(), m.edges().count(), m.vertices().count()), (6, 12, 8));
    assert!(near(volume(&m), 1000.0), "{} (positive: every face points out)", volume(&m));
    assert!(m.face(top).unwrap().normal.z > 0.99, "the top faces up");
    assert!(m.face(f).unwrap().normal.z < -0.99, "the rectangle stays as the bottom, facing down");
}

#[test]
fn pulling_a_box_top_makes_it_taller_with_one_face_per_side() {
    let (mut m, f) = ground_rect();
    let top = pull_up(&mut m, f, 5.0);
    let top = m.push_pull(top, 3.0).unwrap();
    assert_eq!(m.faces().count(), 6, "each wall joins the side below it");
    assert!(near(volume(&m), 1600.0), "{}", volume(&m));
    assert!(near(m.face(top).unwrap().area(&m), 200.0));
}

#[test]
fn pushing_a_box_top_in_makes_it_shorter() {
    let (mut m, f) = ground_rect();
    let top = pull_up(&mut m, f, 5.0);
    m.push_pull(top, -2.0).unwrap();
    assert_eq!(m.faces().count(), 6, "no strip is left above the new top");
    assert!(near(volume(&m), 600.0), "{}", volume(&m));
}

#[test]
fn pulling_a_side_of_a_box_out_makes_it_wider() {
    let (mut m, f) = ground_rect();
    pull_up(&mut m, f, 5.0);
    let side = m.faces().find(|(_, g)| g.normal.x > 0.99).unwrap().0;
    m.push_pull(side, 4.0).unwrap();
    assert_eq!(m.faces().count(), 6);
    assert!(near(volume(&m), 24.0 * 10.0 * 5.0), "{}", volume(&m));
}

#[test]
fn a_face_with_a_hole_pulls_into_a_tube() {
    let mut m = Mesh::new();
    rect(&mut m, 0.0, 0.0, 30.0, 30.0);
    rect(&mut m, 10.0, 10.0, 20.0, 20.0);
    let ring = m.faces().find(|(_, f)| f.holes.len() == 1).unwrap().0;
    let inner = m.faces().find(|(_, f)| f.holes.is_empty()).unwrap().0;
    m.erase_face(inner).unwrap();
    pull_up(&mut m, ring, 10.0);
    // Bottom ring, top ring, four outer walls and four inner ones.
    assert_eq!(m.faces().count(), 10);
    assert!(near(volume(&m), 800.0 * 10.0), "{}", volume(&m));
}

#[test]
fn hostile_push_pulls_are_refused() {
    let (mut m, f) = ground_rect();
    assert_eq!(m.push_pull(f, f64::NAN), Err(MeshError::BadPoint));
    assert_eq!(m.push_pull(f, 0.0), Err(MeshError::ZeroLength));
    assert_eq!(m.push_pull(FaceId(999), 1.0), Err(MeshError::Missing("face")));
    assert_eq!(m.faces().count(), 1);
}

fn a_box() -> (Mesh, FaceId) {
    let (mut m, f) = ground_rect();
    let top = pull_up(&mut m, f, 5.0);
    (m, top)
}

fn all(m: &Mesh) -> std::collections::BTreeSet<VertexId> {
    m.vertices().map(|(v, _)| v).collect()
}

fn bounds(m: &Mesh) -> (Vec3, Vec3) {
    let mut lo = p(f64::MAX, f64::MAX, f64::MAX);
    let mut hi = p(f64::MIN, f64::MIN, f64::MIN);
    for (_, v) in m.vertices() {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    (lo, hi)
}

/// Every face's vertices in its plane.
fn all_flat(m: &Mesh) -> bool {
    m.faces().all(|(_, f)| {
        let p0 = m.position(f.outer[0]).unwrap();
        f.loops().flatten().all(|v| (m.position(*v).unwrap() - p0).dot(f.normal).abs() <= TOLERANCE)
    })
}

#[test]
fn moving_rotating_and_scaling_a_whole_box_keeps_it_a_box() {
    let (mut m, _) = a_box();
    let every = all(&m);
    m.move_by(&every, p(100.0, 0.0, 0.0)).unwrap();
    assert_eq!(bounds(&m), (p(100.0, 0.0, 0.0), p(120.0, 10.0, 5.0)));
    m.rotate(&every, p(100.0, 0.0, 0.0), p(0.0, 0.0, 1.0), 90.0).unwrap();
    // 20 along X turned a quarter about Z through (100, 0) lies along Y, on the -X side.
    let (lo, hi) = bounds(&m);
    assert!(lo.dist(p(90.0, 0.0, 0.0)) < 1e-9 && hi.dist(p(100.0, 20.0, 5.0)) < 1e-9, "{lo:?} {hi:?}");
    assert!(near(volume(&m), 1000.0), "turned, the faces still point out");
    m.scale(&every, p(90.0, 0.0, 0.0), p(2.0, 2.0, 2.0)).unwrap();
    assert!(near(volume(&m), 8000.0), "{}", volume(&m));
    assert_eq!(m.faces().count(), 6);
    // A mirror (a negative factor) turns the faces inside out by its sign: the volume says so.
    m.scale(&every, p(90.0, 0.0, 0.0), p(-1.0, 1.0, 1.0)).unwrap();
    assert!(near(volume(&m).abs(), 8000.0));
}

#[test]
fn moving_a_face_stretches_the_faces_joined_to_it() {
    let (mut m, top) = a_box();
    let verts = m.vertices_of(&[], &[top]);
    m.move_by(&verts, p(0.0, 0.0, 3.0)).unwrap();
    assert!(near(volume(&m), 1600.0), "{}", volume(&m));
    assert_eq!(m.faces().count(), 6);
    assert!(all_flat(&m));
}

#[test]
fn moving_an_edge_slants_the_box_and_its_faces_stay_flat() {
    let (mut m, _) = a_box();
    // The top front edge (y = 0, z = 5) moves back 2.
    let edge = m
        .edges()
        .find(|(_, e)| {
            [e.a, e.b].iter().all(|v| {
                let q = m.position(*v).unwrap();
                q.y == 0.0 && q.z == 5.0
            })
        })
        .unwrap()
        .0;
    let verts = m.vertices_of(&[edge], &[]);
    m.move_by(&verts, p(0.0, 2.0, 0.0)).unwrap();
    // A prism on a trapezoid: (10 + 8) / 2 x 5, 20 long.
    assert!(near(volume(&m), 900.0), "{}", volume(&m));
    assert_eq!(m.faces().count(), 6, "every face stays flat");
    assert!(all_flat(&m));
}

#[test]
fn moving_a_corner_up_folds_the_faces_that_would_bend() {
    let (mut m, _) = a_box();
    let corner = m.vertex_at(p(20.0, 10.0, 5.0)).unwrap();
    m.move_by(&[corner].into(), p(0.0, 0.0, 2.0)).unwrap();
    // Straight up, the two sides at the corner stay in their planes; only the top bends.
    assert_eq!(m.faces().count(), 7, "the top folds into two triangles");
    assert!(all_flat(&m));
    assert!(volume(&m) > 1000.0, "the faces still point out: {}", volume(&m));
}

#[test]
fn a_refused_move_changes_nothing() {
    let (mut m, _) = a_box();
    let before = serde_json::to_string(&m).unwrap();
    let every = all(&m);
    assert_eq!(m.move_by(&every, p(f64::NAN, 0.0, 0.0)), Err(MeshError::BadPoint));
    assert_eq!(m.rotate(&every, p(0.0, 0.0, 0.0), p(0.0, 0.0, 0.0), 10.0), Err(MeshError::BadPoint));
    assert_eq!(m.scale(&every, p(0.0, 0.0, 0.0), p(0.0, 1.0, 1.0)), Err(MeshError::BadPoint));
    // A face with a hole that would bend.
    let mut t = Mesh::new();
    rect(&mut t, 0.0, 0.0, 30.0, 30.0);
    rect(&mut t, 10.0, 10.0, 20.0, 20.0);
    let corner = t.vertex_at(p(30.0, 30.0, 0.0)).unwrap();
    let saved = serde_json::to_string(&t).unwrap();
    assert_eq!(t.move_by(&[corner].into(), p(0.0, 0.0, 5.0)), Err(MeshError::Bends));
    assert_eq!(serde_json::to_string(&t).unwrap(), saved);
    assert_eq!(serde_json::to_string(&m).unwrap(), before);
}

#[test]
fn a_circle_is_one_curve_and_closes_a_face() {
    let mut m = Mesh::new();
    let c = m.add_circle(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), 10.0, 24).unwrap();
    let curve = m.curve(c).unwrap();
    assert_eq!(curve.edges.len(), 24);
    assert!(matches!(curve.kind, CurveKind::Circle { radius, .. } if radius == 10.0));
    assert!(curve.edges.iter().all(|e| m.edge(*e).unwrap().curve == Some(c)));
    // The 24-gon's area: n/2 r² sin(2π/n).
    let want = 12.0 * 100.0 * (std::f64::consts::TAU / 24.0).sin();
    assert_eq!(m.faces().count(), 1);
    assert!(near(m.faces().next().unwrap().1.area(&m), want));
}

#[test]
fn pulling_a_circle_makes_a_cylinder_with_a_smooth_side() {
    let mut m = Mesh::new();
    m.add_circle(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), 10.0, 24).unwrap();
    let disc = m.faces().next().unwrap().0;
    let area = m.face(disc).unwrap().area(&m);
    pull_up(&mut m, disc, 30.0);
    assert_eq!(m.faces().count(), 26, "two caps and 24 walls");
    assert!(near(volume(&m), area * 30.0), "{}", volume(&m));
    let vertical: Vec<&Edge> = m.edges().map(|(_, e)| e).filter(|e| (m.position(e.a).unwrap().z - m.position(e.b).unwrap().z).abs() > 1.0).collect();
    assert_eq!(vertical.len(), 24);
    assert!(vertical.iter().all(|e| e.soft && e.smooth), "the seams between walls are soft and smooth");
}

#[test]
fn an_arc_ends_where_it_turns_to_and_keeps_its_order_when_split() {
    let mut m = Mesh::new();
    let c = m.add_arc(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), p(10.0, 0.0, 0.0), 90.0, 6).unwrap();
    assert!(m.vertex_at(p(0.0, 10.0, 0.0)).is_some(), "a quarter turn ends on +Y");
    // A line onto the middle of the first segment splits it; the halves stay on the curve.
    let (a, b) = m.ends(m.curve(c).unwrap().edges[0]).unwrap();
    let mid = (a + b) * 0.5;
    m.add_edge(mid, mid + p(0.0, -5.0, 0.0)).unwrap();
    let edges = &m.curve(c).unwrap().edges;
    assert_eq!(edges.len(), 7);
    // Still in order: each edge meets the next.
    for pair in edges.windows(2) {
        let (x, y) = (m.edge(pair[0]).unwrap(), m.edge(pair[1]).unwrap());
        assert!([x.a, x.b].iter().any(|v| *v == y.a || *v == y.b), "{pair:?}");
    }
    // Erasing a segment takes it off the curve; erasing them all takes the curve.
    for e in m.curve(c).unwrap().edges.clone() {
        m.erase_edge(e).unwrap();
    }
    assert!(m.curve(c).is_none());
}

#[test]
fn a_polygon_saves_and_opens_as_a_curve_and_bad_curves_are_refused() {
    let mut m = Mesh::new();
    let c = m.add_polygon(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), p(10.0, 0.0, 0.0), 6).unwrap();
    let back: Mesh = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert_eq!(back.curve(c), m.curve(c));
    assert_eq!(m.add_circle(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), 10.0, 2), Err(MeshError::BadPoint), "too few segments");
    assert_eq!(m.add_circle(p(0.0, 0.0, 0.0), p(0.0, 0.0, 0.0), 10.0, 24), Err(MeshError::BadPoint), "no normal");
    assert_eq!(m.add_arc(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), p(0.0, 0.0, 5.0), 90.0, 6), Err(MeshError::ZeroLength), "start on the axis");
    assert_eq!(m.add_arc(p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), p(5.0, 0.0, 0.0), 400.0, 6), Err(MeshError::BadPoint));
}

/// An `n` x `n` grid of 10 mm squares, each drawn as its own four edges (shared sides weld).
fn grid(n: usize) -> Mesh {
    let mut m = Mesh::new();
    for i in 0..n {
        for j in 0..n {
            let (x, y) = (i as f64 * 10.0, j as f64 * 10.0);
            rect(&mut m, x, y, x + 10.0, y + 10.0);
        }
    }
    m
}

#[test]
fn a_grid_of_squares_makes_one_face_each() {
    let n = 40;
    let mut m = grid(n);
    assert_eq!(m.faces().count(), n * n);
    assert_eq!(m.edges().count(), 2 * n * (n + 1));
    assert_eq!(m.vertices().count(), (n + 1) * (n + 1));
    // A line across the whole grid splits every edge it crosses and every face it runs through.
    m.add_edge(p(0.0, 5.0, 0.0), p(n as f64 * 10.0, 5.0, 0.0)).unwrap();
    assert_eq!(m.faces().count(), n * n + n);
    // Push/Pull one square in the middle.
    let f = m.faces().find(|(_, f)| f.outer.iter().any(|v| m.position(*v).unwrap().dist(p(n as f64 * 5.0, n as f64 * 5.0, 0.0)) < 1e-9)).unwrap().0;
    pull_up(&mut m, f, 10.0);
    // Joined to the squares round it, the square itself goes (it would be inside the box); the
    // box adds four walls and a top.
    assert_eq!(m.faces().count(), n * n + n - 1 + 5);
}

/// The saved form of an `n` x `n` grid of 10 mm squares, written out directly (what an import
/// that builds the model in one go hands to it).
fn grid_json(n: usize) -> String {
    let v = |i: usize, j: usize| 1 + i * (n + 1) + j;
    let mut vertices = Vec::new();
    for i in 0..=n {
        for j in 0..=n {
            vertices.push(format!("\"{}\":{{\"position\":[{},{},0]}}", v(i, j), i * 10, j * 10));
        }
    }
    let mut next = (n + 1) * (n + 1);
    let mut edges = Vec::new();
    for i in 0..=n {
        for j in 0..=n {
            if i < n {
                next += 1;
                edges.push(format!("\"{next}\":{{\"a\":{},\"b\":{}}}", v(i, j), v(i + 1, j)));
            }
            if j < n {
                next += 1;
                edges.push(format!("\"{next}\":{{\"a\":{},\"b\":{}}}", v(i, j), v(i, j + 1)));
            }
        }
    }
    let mut faces = Vec::new();
    for i in 0..n {
        for j in 0..n {
            next += 1;
            let ring = [v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)];
            faces.push(format!("\"{next}\":{{\"outer\":{ring:?},\"normal\":[0,0,1]}}"));
        }
    }
    format!("{{\"vertices\":{{{}}},\"edges\":{{{}}},\"faces\":{{{}}},\"next\":{next}}}", vertices.join(","), edges.join(","), faces.join(","))
}

#[test]
fn a_grid_built_in_one_go_opens_like_a_drawn_one() {
    let m: Mesh = serde_json::from_str(&grid_json(6)).unwrap();
    let drawn = grid(6);
    assert_eq!((m.faces().count(), m.edges().count(), m.vertices().count()), (drawn.faces().count(), drawn.edges().count(), drawn.vertices().count()));
    assert!(m.edges().all(|(e, _)| m.faces_of(e).count() == drawn.faces_of(drawn.edge_between(drawn.vertex_at(m.position(m.edge(e).unwrap().a).unwrap()).unwrap(), drawn.vertex_at(m.position(m.edge(e).unwrap().b).unwrap()).unwrap()).unwrap()).count()));
}

/// `cargo test -p sketcraft-mesh --release -- --ignored --nocapture big_model`: timings for
/// drawing edges one at a time, and for opening and editing a model of about a million edges.
#[test]
#[ignore]
fn big_model_timings() {
    let t = std::time::Instant::now();
    let m = grid(200);
    eprintln!("drew {} edges one at a time in {:?}", m.edges().count(), t.elapsed());
    let json = grid_json(707);
    let t = std::time::Instant::now();
    let mut big: Mesh = serde_json::from_str(&json).unwrap();
    eprintln!("opened {} edges, {} faces ({} MB) in {:?}", big.edges().count(), big.faces().count(), json.len() / 1_000_000, t.elapsed());
    let f = big.faces().nth(big.faces().count() / 2).unwrap().0;
    let t = std::time::Instant::now();
    pull_up(&mut big, f, 10.0);
    eprintln!("push/pull in {:?}", t.elapsed());
    let t = std::time::Instant::now();
    big.add_edge(p(3.0, 3.0, 0.0), p(7.0, 7.0, 0.0)).unwrap();
    eprintln!("an edge across a face in {:?}", t.elapsed());
}
