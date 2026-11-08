use serde_json::{Value, json};
use sketcore_record::Vec3;

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

/// Where B's corner (20, 0, 0) is now.
fn b_x(s: &Session) -> f64 {
    super::world_point(s, occ(s, "B"), Vec3::new(20.0, 0.0, 0.0)).x
}

/// Cubes A (grounded, x 0…10) and B (x 20…30) on a slider along x: B's −x face on A's +x face
/// at value 0, B 10 mm away to start.
fn two_parts() -> Session {
    let mut s = Session::default();
    for (name, x) in [("A", 0), ("B", 20)] {
        run(&mut s, "component.activate", json!({"component": "root"}));
        run(&mut s, "component.create", json!({"name": name}));
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [x, 0, 0]}));
    }
    run(&mut s, "component.activate", json!({"component": "root"}));
    let a = occ(&s, "A");
    run(&mut s, "occurrence.ground", json!({"occurrence": a, "grounded": true}));
    assert!((b_x(&s) - 20.0).abs() < 1e-6, "{}", b_x(&s));
    s
}

#[test]
fn automatic_explode_centres_follow_reassigned_bodies() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Transferred"}));
    let component = run(&mut s, "component.create", json!({"name": "Carrier", "grounded": false}));
    run(&mut s, "occurrence.move", json!({"occurrence": component["occurrence"], "translate": [100, 0, 0]}));
    run(&mut s, "component.move_bodies", json!({"bodies": ["Transferred"], "component": component["component"]}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "Neighbour"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [20, 0, 0]}));
    let exploded = run(&mut s, "explode.create", json!({"scale": 1}));
    let moves = exploded["moves"].as_array().unwrap();
    let carrier = moves.iter().find(|m| m["occurrence"] == component["occurrence"]).unwrap();
    assert!((carrier["translate"][0].as_f64().unwrap() + 10.0).abs() < 1e-6, "{exploded}");
}

#[test]
fn exploded_views_show_without_changing_the_design() {
    let mut s = two_parts();
    let before = s.doc.occurrences.clone();
    let r = run(&mut s, "explode.create", json!({"name": "Apart", "scale": 1}));
    assert_eq!(r["moves"].as_array().map(Vec::len), Some(2), "{r}");
    run(&mut s, "explode.show", json!({"view": "Apart"}));
    // Centres at x 5 and 25 → middle 15: each moves 10 mm further out.
    let bb = |s: &mut Session, b: &str| run(s, "inspect.measure", json!({"bodies": [b]}))["bodies"][0]["bbox"]["min"][0].as_f64().unwrap_or(f64::NAN);
    let names: Vec<String> = s.world_state().unwrap().bodies.iter().map(|b| b.name.clone()).collect();
    assert!((bb(&mut s, &names[0]) + 10.0).abs() < 1e-6, "{names:?}");
    assert!((bb(&mut s, &names[1]) - 30.0).abs() < 1e-6);
    assert_eq!(s.doc.occurrences, before, "shown only");
    // The design's placements are not the view's: the properties report where the design
    // places B, and Capture Position has nothing of the view's to keep (it stays shown).
    let b_id = occ(&s, "B");
    let centre =
        |s: &mut Session| run(s, "occurrence.properties", json!({"occurrence": b_id}))["total"]["center_of_mass"][0].as_f64().unwrap_or(f64::NAN);
    run(&mut s, "explode.show", json!({"view": null}));
    let placed = centre(&mut s);
    run(&mut s, "explode.show", json!({"view": "Apart"}));
    assert!((centre(&mut s) - placed).abs() < 1e-9, "{} shown, {placed} placed", centre(&mut s));
    assert_eq!(run(&mut s, "component.capture_position", json!({}))["captured"], 0);
    assert_eq!(s.doc.occurrences, before, "the exploded view is never captured");
    assert!((bb(&mut s, &names[1]) - 30.0).abs() < 1e-6, "still shown");
    run(&mut s, "explode.show", json!({"view": null}));
    assert!(bb(&mut s, &names[0]).abs() < 1e-6);
    run(&mut s, "explode.create", json!({"name": "Hand", "moves": [{"occurrence": "B", "translate": [0, 0, 50]}]}));
    run(&mut s, "explode.show", json!({"view": "Hand"}));
    assert!(
        (run(&mut s, "inspect.measure", json!({"bodies": [names[1]]}))["bodies"][0]["bbox"]["min"][2].as_f64().unwrap_or(0.0) - 50.0).abs() < 1e-6
    );
    assert_eq!(run(&mut s, "explode.list", json!({}))["views"].as_array().map(Vec::len), Some(2));
}

/// #9142: on a part of plain bodies (the sample plate) Exploded View opened its dialog and failed
/// on OK ("an exploded view needs two or more occurrences"). It is greyed out instead, saying why,
/// until the design has two components to pull apart.
#[test]
fn exploded_view_is_greyed_out_until_there_are_two_components() {
    let spec = crate::cmd::find_command("explode.create").unwrap();
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Plate"}));
    let info = spec.info(&s);
    assert!(!info.enabled, "a part of plain bodies has nothing to explode");
    assert_eq!(info.disabled_reason.as_deref(), Some(super::EXPLODE_NEEDS_COMPONENTS));
    let e = s.execute("explode.create", &json!({"scale": 1.62})).unwrap_err();
    assert!(e.to_string().contains("two or more of them"), "{e}");
    assert!(s.doc.assembly.exploded.is_empty(), "nothing was added: {e}");
    // One component is still nothing to pull apart.
    run(&mut s, "component.from_bodies", json!({"bodies": ["Plate"]}));
    assert_eq!(s.doc.occurrences.len(), 1);
    assert!(!spec.info(&s).enabled);
    // A second one brings it back, and the automatic view separates the two.
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "Lid"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [30, 0, 0]}));
    assert!(spec.info(&s).enabled, "{:?}", spec.info(&s).disabled_reason);
    let r = run(&mut s, "explode.create", json!({"scale": 1.62}));
    assert_eq!(r["moves"].as_array().map(Vec::len), Some(2), "{r}");
}
