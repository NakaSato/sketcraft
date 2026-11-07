use super::*;
use std::collections::BTreeSet;
#[test]
fn hidden_geometry_objects_and_tag_visibility_remain_independent_in_cache() {
    let mut model = Model::default();
    let edge = model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let object = model
        .make_object(&BTreeSet::from([edge]), false, "Group")
        .unwrap();
    let edge = *model.edges.keys().next().unwrap();
    model.hidden.insert(object);
    let mut cache = EntityCache::default();
    let mut d = ketchup_core::DisplaySettings::default();
    cache.update(&model);
    assert!(!cache.get(edge).displayed(&d));
    d.hidden_geometry = true;
    assert!(!cache.get(edge).displayed(&d));
    d.hidden_objects = true;
    assert!(cache.get(edge).displayed(&d));
    let tag = model.add_tag("Structure").unwrap();
    model
        .assign_tag(&BTreeSet::from([object]), Some(tag))
        .unwrap();
    model.tags.get_mut(&tag).unwrap().visible = false;
    cache.update(&model);
    assert!(!cache.get(edge).displayed(&d));
    model.tags.get_mut(&tag).unwrap().visible = true;
    cache.update(&model);
    assert!(cache.get(edge).displayed(&d));
    assert_eq!(cache.get(edge).tag_color, Some(model.tags[&tag].color));
    model.hidden.clear();
    model.hidden.insert(edge);
    d.hidden_geometry = false;
    cache.update(&model);
    assert!(!cache.get(edge).displayed(&d));
    d.hidden_geometry = true;
    assert!(cache.get(edge).displayed(&d));
}

#[test]
fn section_clipping_rejects_geometry_above_plane_and_fog_blends_at_distance() {
    let camera = Camera::default();
    let mut mesh = Mesh::new(&camera, DVec2::new(1000., 800.));
    let mut d = ketchup_core::DisplaySettings {
        section_cuts: true,
        section_height: 1.,
        ..Default::default()
    };
    mesh.set_display(&d);
    mesh.world_triangle(
        &camera,
        [
            DVec3::Z * 2.,
            DVec3::X + DVec3::Z * 2.,
            DVec3::Y + DVec3::Z * 2.,
        ],
        Color32::WHITE,
    );
    mesh.world_line(
        &camera,
        DVec3::Z * 2.,
        DVec3::X + DVec3::Z * 2.,
        2.,
        Color32::WHITE,
    );
    assert!(mesh.vertices.is_empty());
    mesh.world_triangle(
        &camera,
        [DVec3::ZERO, DVec3::X, DVec3::Y + DVec3::Z * 2.],
        Color32::WHITE,
    );
    assert!(!mesh.vertices.is_empty());
    d.fog = true;
    d.fog_start = 1.;
    d.fog_end = 2.;
    d.background = [100, 120, 140];
    mesh.set_display(&d);
    assert_eq!(mesh.fog_color(Color32::WHITE, camera.eye()), Color32::WHITE);
    assert_eq!(
        mesh.fog_color(Color32::WHITE, DVec3::ZERO),
        Color32::from_rgb(100, 120, 140)
    );
}
