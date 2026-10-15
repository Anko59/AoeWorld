use super::*;
use aoe_core::EntityId;
use wasm_bindgen_test::wasm_bindgen_test;

fn unit(position: [f64; 2], selected: bool, moving: bool) -> SceneUnit {
    SceneUnit {
        kind: Default::default(),
        id: EntityId(7),
        position,
        moving,
        facing: 2,
        selected,
        elevation_meters: 1.0,
    }
}

fn resource(id: u64) -> SceneResource {
    SceneResource {
        id,
        position: [3.5, 4.5],
        kind: 1,
        visual_variant: 0,
        visual_family: 0,
        elevation_meters: 1.0,
    }
}

#[wasm_bindgen_test]
fn picking_uses_only_successfully_presented_positions_until_reset() {
    let scene = Rc::new(());
    let units = [unit([1.0, 2.0], false, true)];
    let mut cache = RenderFrameCache::default();
    assert_eq!(cache.position(EntityId(7)), None);
    cache.record_success(&scene, &units, &[], false, 0);
    assert_eq!(cache.position(EntityId(7)), Some([1.0, 2.0]));
    assert!(cache.needs_render(&scene, &[unit([1.5, 2.0], false, true)], &[], false, 0));
    // Failed or throttled frames cannot move hitboxes ahead of their pixels.
    assert_eq!(cache.position(EntityId(7)), Some([1.0, 2.0]));
    cache.clear();
    assert_eq!(cache.position(EntityId(7)), None);
}

#[wasm_bindgen_test]
fn unchanged_static_frame_ignores_animation_clock() {
    let scene = Rc::new(());
    let units = [unit([1.0, 2.0], false, false)];
    let resources = [resource(9)];
    let mut cache = RenderFrameCache::default();

    assert!(cache.needs_render(&scene, &units, &resources, false, 10));
    cache.record_success(&scene, &units, &resources, false, 10);
    assert!(!cache.needs_render(&scene, &units, &resources, false, 11));
}

#[wasm_bindgen_test]
fn scene_replacement_retries_and_releases_the_previous_scene() {
    let first = Rc::new(());
    let second = Rc::new(());
    let mut cache = RenderFrameCache::default();
    let units = [];
    let resources = [];

    assert!(cache.needs_render(&first, &units, &resources, false, 0));
    // A failed render does not update the successful-frame cache.
    assert!(cache.needs_render(&first, &units, &resources, false, 0));
    cache.record_success(&first, &units, &resources, false, 0);
    assert!(cache.needs_render(&second, &units, &resources, false, 0));
    cache.record_success(&second, &units, &resources, false, 0);
    assert_eq!(Rc::strong_count(&first), 1);
}

#[wasm_bindgen_test]
fn unit_selection_movement_resources_grid_and_moving_animation_invalidate() {
    let scene = Rc::new(());
    let idle = [unit([1.0, 2.0], false, false)];
    let resources = [resource(9), resource(10)];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &idle, &resources, false, 0);

    assert!(cache.needs_render(
        &scene,
        &[unit([2.0, 2.0], false, false)],
        &resources,
        false,
        0
    ));
    assert!(cache.needs_render(
        &scene,
        &[unit([1.0, 2.0], true, false)],
        &resources,
        false,
        0
    ));
    assert!(cache.needs_render(&scene, &idle, &[resource(10)], false, 0));
    assert!(cache.needs_render(&scene, &idle, &resources, true, 0));

    let moving = [unit([1.0, 2.0], false, true)];
    cache.record_success(&scene, &moving, &resources, false, 5);
    assert!(cache.needs_render(&scene, &moving, &resources, false, 6));
}

#[wasm_bindgen_test]
fn a_working_villager_marker_does_not_invalidate_on_animation_ticks() {
    let scene = Rc::new(());
    let mut worker = unit([1.0, 2.0], false, false);
    worker.kind = aoe_rendering::SceneUnitKind::VillagerWorking;
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &[worker], &[], false, 0);
    assert!(!cache.needs_render(&scene, &[worker], &[], false, 1));
}
