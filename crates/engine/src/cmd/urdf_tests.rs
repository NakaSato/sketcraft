//! URDF export (#719, docs/urdf.md): files, names, limits, mass and forward kinematics against
//! the joint solver.

use sketcraft_geom::prelude::*;
use std::path::PathBuf;

use serde_json::{Value, json};
use sketcraft_doc::{Mat, joints, mat_mul};
use sketcraft_io::urdf::{UrdfModel, UrdfOptions, UrdfPackage};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn occ(s: &Session, component: &str) -> u64 {
    let c = s.doc.find_component(component).unwrap_or(0);
    s.doc.occurrence_of(c).map(|o| o.id).unwrap_or(0)
}

fn ground(s: &mut Session, component: &str, grounded: bool) {
    let o = occ(s, component);
    run(s, "occurrence.ground", json!({"occurrence": o, "grounded": grounded}));
}

/// 10 mm cubes in components A (grounded, at the origin) and B (at x = 20).
fn two_cubes() -> Session {
    let mut s = Session::default();
    for (name, x) in [("A", 0), ("B", 20)] {
        run(&mut s, "component.activate", json!({"component": "root"}));
        run(&mut s, "component.create", json!({"name": name}));
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [x, 0, 0]}));
    }
    ground(&mut s, "A", true);
    s
}

fn package(s: &Session) -> UrdfPackage {
    match sketcraft_io::urdf(&s.doc, &s.doc.painted(&s.model.state()), "t", &UrdfOptions::default()) {
        Ok(p) => p,
        Err(e) => panic!("urdf: {e}"),
    }
}

fn close(a: &Mat, b: &Mat) -> bool {
    a.iter().flatten().zip(b.iter().flatten()).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// Every placement's link, composed at the design's current joint values, sits where SketCraft
/// places it.
fn assert_fk(m: &UrdfModel, s: &Session, what: &str) {
    let vals = joints::joint_values(&s.doc.assembly);
    let frames = m.link_frames(|j| j.source.and_then(|(id, k)| vals.get(&id)?.get(k).copied()).unwrap_or(0.0));
    let placements = s.doc.placements();
    let mut seen = 0;
    for (l, f) in m.links.iter().zip(&frames) {
        if l.path.is_empty() {
            continue;
        }
        let Some((_, _, want)) = placements.iter().find(|p| p.1 == l.path) else { panic!("{what}: {} has no placement", l.name) };
        let got = mat_mul(f, &l.mesh_origin);
        assert!(close(&got, want), "{what}: {} at {got:?}, SketCraft has {want:?}", l.name);
        seen += 1;
    }
    assert_eq!(seen, placements.len() - 1, "{what}: a link per placement");
}

fn temp(tag: &str) -> PathBuf {
    let d = sketcraft_io::vfs::temp_dir().join(format!("sketcraft-urdf-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Volume (m³) of an STL whose triangles share their corners (`None`: not closed).
fn stl_volume(bytes: &[u8]) -> Option<f64> {
    let mut index: std::collections::HashMap<[u32; 3], u32> = std::collections::HashMap::new();
    let mut mesh = sketcraft_geom::Mesh::default();
    for t in sketcraft_io::read_stl(bytes).ok()? {
        let mut tri = [0u32; 3];
        for (k, p) in t.iter().enumerate() {
            let n = mesh.positions.len() as u32;
            tri[k] = *index.entry(p.map(f32::to_bits)).or_insert_with(|| {
                mesh.positions.push(sketcore_record::Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                n
            });
        }
        mesh.triangles.push(tri);
    }
    mesh.mass_properties().map(|m| m.volume)
}

fn names(m: &UrdfModel) -> (Vec<&str>, Vec<&str>) {
    (m.links.iter().map(|l| l.name.as_str()).collect(), m.joints.iter().map(|j| j.name.as_str()).collect())
}

fn has_warning(p: &UrdfPackage, needle: &str) -> bool {
    p.warnings.iter().any(|w| w.contains(needle))
}

/// A moved occurrence whose position is not captured yet exports where it is shown.
#[test]
fn urdf_exports_uncaptured_moves() {
    let mut s = two_cubes();
    let b = occ(&s, "B");
    run(&mut s, "occurrence.move", json!({"occurrence": b, "translate": [0, 0, 30], "capture": false}));
    assert!(!s.pending_moves.is_empty());
    let d = temp("pending");
    let path = d.join("m.urdf");
    run(&mut s, "file.export", json!({"path": path.to_string_lossy()}));
    let xml = std::fs::read_to_string(&path).unwrap();
    // B hangs on base_link where it is shown: 30 mm up.
    assert!(xml.contains(r#"<origin xyz="0 0 0.03" rpy="0 0 0"/>"#), "{xml}");
    let _ = std::fs::remove_dir_all(&d);
}

/// The visual's colour keeps the body's opacity, and a too long prefix says so.
#[test]
fn urdf_colour_keeps_opacity() {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Glass"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10}));
    let body = s.model.state().bodies.first().map(|b| b.name.clone()).unwrap_or_default();
    run(&mut s, "appearance.assign", json!({"bodies": [body], "color": [255, 0, 0], "opacity": 0.5}));
    let p = package(&s);
    assert!(p.xml.contains(r#"<color rgba="1 0 0 0.5"/>"#), "{}", p.xml);
    let d = temp("long-prefix");
    let e = s.execute("file.export", &json!({"path": d.join("l.urdf").to_string_lossy(), "mesh_prefix": "a".repeat(5000)}));
    assert!(e.is_err_and(|e| e.to_string().contains("longer than")));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn urdf_inertial_is_mass_weighted() {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Cube"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [100, 0, 0]}));
    let body = s.model.state().bodies.first().map(|b| b.name.clone()).unwrap_or_default();
    run(&mut s, "material.assign", json!({"bodies": [body], "material": "Aluminum"}));
    let p = package(&s);
    let Some(l) = p.model.links.iter().find(|l| l.name == "Cube") else { panic!("{:?}", names(&p.model)) };
    let Some(i) = l.inertial else { panic!("no inertial") };
    let m = 0.0027;
    assert!((i.mass - m).abs() < 1e-12, "{}", i.mass);
    let ixx = m * (0.01f64.powi(2) * 2.0) / 12.0;
    for k in 0..3 {
        assert!((i.inertia[k][k] - ixx).abs() < 1e-12 * ixx.max(1.0) + 1e-15, "{:?}", i.inertia);
    }
    assert!(i.inertia[0][1].abs() < 1e-15);
    // The centre of mass is the cube's centre, in the link frame.
    assert!(i.com.dist(sketcraft_doc::apply_point(&l.mesh_origin, sketcore_record::Vec3::new(105.0, 5.0, 5.0))) < 1e-9, "{:?}", i.com);
}

#[test]
fn urdf_collision_skips_open_bodies() {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Part"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [0, 0, 0]}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
    run(&mut s, "sketch.line", json!({"points": [[30, 0], [40, 0]], "ids": ["l1"]}));
    run(&mut s, "sketch.finish", json!({}));
    crate::cmd::sheet_fixtures::extrude(&mut s, json!({"sketch": "S", "curves": ["l1"], "distance": 10, "body_name": "Wall"}));
    let p = package(&s);
    let Some(l) = p.model.links.iter().find(|l| l.name == "Part") else { panic!("{:?}", names(&p.model)) };
    let (Some(v), Some(c)) = (&l.visual, &l.collision) else { panic!("{l:?}") };
    assert_ne!(v, c, "a separate collision mesh");
    let bytes = &p.meshes.iter().find(|(f, _)| f == c).unwrap().1;
    assert!(stl_volume(bytes).is_some(), "the collision mesh is closed");
    assert!(has_warning(&p, "not a closed solid"), "{:?}", p.warnings);
    // Mass from the cube alone (default density).
    let rho = s.doc.density(&s.model.state().bodies.first().map(|b| b.name.clone()).unwrap_or_default());
    assert!(l.inertial.is_some_and(|i| (i.mass - 1000.0 * rho * 1e-6).abs() < 1e-12));
}

#[test]
fn urdf_open_only_component_is_drawn() {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Sheet"}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "S"}));
    run(&mut s, "sketch.line", json!({"points": [[30, 0], [40, 0]], "ids": ["l1"]}));
    run(&mut s, "sketch.finish", json!({}));
    crate::cmd::sheet_fixtures::extrude(&mut s, json!({"sketch": "S", "curves": ["l1"], "distance": 10, "body_name": "Wall"}));
    let p = package(&s);
    let Some(l) = p.model.links.iter().find(|l| l.name == "Sheet") else { panic!("{:?}", names(&p.model)) };
    assert!(l.visual.is_some() && l.collision.is_none(), "{l:?}");
    assert!(l.inertial.is_some_and(|i| i.mass == 1e-6), "{l:?}");
}

/// Every link but `base_link` has exactly one parent joint.
fn assert_tree(m: &UrdfModel) {
    for (i, l) in m.links.iter().enumerate().skip(1) {
        assert_eq!(m.joints.iter().filter(|j| j.child == i).count(), 1, "{}", l.name);
    }
    assert_eq!(m.joints.iter().filter(|j| j.child == 0).count(), 0);
}

#[test]
fn urdf_fixes_an_unjointed_occurrence_to_base() {
    let s = two_cubes();
    let p = package(&s);
    assert!(has_warning(&p, "`B` has no joint"), "{:?}", p.warnings);
    assert_tree(&p.model);
    assert_fk(&p.model, &s, "loose");
}

#[test]
fn urdf_copies_nested_occurrences_of_each_placement() {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Leg"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 30, "corner": [0, 0, 0]}));
    let leg = s.doc.find_component("Leg").unwrap_or(0);
    run(&mut s, "component.create", json!({"name": "Foot", "parent": leg}));
    run(&mut s, "solid.box", json!({"length": 12, "width": 12, "height": 2, "corner": [-1, -1, -2]}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "occurrence.copy", json!({"component": "Leg", "translate": [50, 0, 0]}));
    assert_eq!(s.doc.placements().len(), 5, "root, two legs, two feet");
    let p = package(&s);
    assert_fk(&p.model, &s, "copies");
    assert_tree(&p.model);
}

/// Deeper than `placements()` walks (a file can hold it; commands stop at 64 levels): the
/// export says copies past the limit are left out.
#[test]
fn urdf_warns_when_placements_are_cut_off() {
    let s = Session::default();
    let mut doc = (*s.doc).clone();
    for k in 1..=66u64 {
        doc.components.push(sketcraft_doc::Component {
            id: 1000 + k,
            name: format!("C{k}"),
            parent: if k == 1 { 0 } else { 999 + k },
            ..Default::default()
        });
        doc.occurrences.push(sketcraft_doc::Occurrence {
            id: 2000 + k,
            component: 1000 + k,
            parent: if k == 1 { 0 } else { 999 + k },
            name: format!("C{k}:1"),
            transform: sketcraft_doc::IDENTITY,
            grounded: false,
        });
    }
    // A component placed twice with a part nested in it, next to the deep chain: its copy is
    // still there, since only the branch past the limit stops.
    doc.components.push(sketcraft_doc::Component { id: 3001, name: "Leg".into(), parent: 0, ..Default::default() });
    doc.components.push(sketcraft_doc::Component { id: 3002, name: "Foot".into(), parent: 3001, ..Default::default() });
    // Listed first, so the walk (a stack) goes down the chain before reaching them.
    for (id, component, parent, name) in [(4003, 3002, 3001, "Foot:1"), (4002, 3001, 0, "Leg:2"), (4001, 3001, 0, "Leg:1")] {
        let o = sketcraft_doc::Occurrence { id, component, parent, name: name.into(), transform: sketcraft_doc::IDENTITY, grounded: false };
        doc.occurrences.insert(0, o);
    }
    let p = sketcraft_io::urdf(&doc, &s.model.state(), "t", &UrdfOptions::default()).unwrap();
    assert!(has_warning(&p, "left out"), "{:?}", p.warnings);
    assert!(p.model.links.iter().any(|l| l.path == [4002, 4003]), "the copy of Foot under Leg:2");
    assert_tree(&p.model);
}
