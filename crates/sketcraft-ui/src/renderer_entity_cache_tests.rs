use super::*;
use std::collections::BTreeSet;
#[test]
fn entity_cache_matches_nested_visibility_locks_and_editing_scopes() {
    let mut model = Model::default();
    let edge = model.add_edge(DVec3::ZERO, DVec3::X).unwrap();
    let inner = model
        .make_object(&BTreeSet::from([edge]), false, "Inner")
        .unwrap();
    let outer = model
        .make_object(&BTreeSet::from([inner]), false, "Outer")
        .unwrap();
    model.add_edge(DVec3::Y, DVec3::Y + DVec3::X).unwrap();
    let mut cache = EntityCache::default();
    for context in [None, Some(outer), Some(inner), None] {
        model.edit_context = context;
        for hidden in [true, false] {
            if hidden {
                model.hidden.insert(outer);
                model.locked.insert(inner);
            } else {
                model.hidden.clear();
                model.locked.clear();
            }
            cache.update(&model);
            for id in model
                .edges
                .keys()
                .chain(model.faces.keys())
                .chain(model.objects.keys())
            {
                let state = cache.get(*id);
                assert_eq!(state.hidden, model.is_hidden(*id));
                assert_eq!(state.locked, model.is_locked(*id));
                assert_eq!(state.context, model.entity_in_context(*id));
            }
        }
    }
    model.edit_context = None;
    model
        .move_selection(&BTreeSet::from([outer]), DVec3::Z)
        .unwrap();
    cache.update(&model);
    for id in model.edges.keys() {
        assert_eq!(cache.get(*id).context, model.entity_in_context(*id));
    }
    cache.update(&Model::default());
    assert!(cache.states.is_empty());
}
