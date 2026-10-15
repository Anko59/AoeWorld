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
    cache.record_success(&scene, &art(), &units, &[], false, 0);
    assert_eq!(cache.position(EntityId(7)), Some([1.0, 2.0]));
    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([1.5, 2.0], false, true)],
        &[],
        false,
        0
    ));
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

    assert!(cache.needs_render(&scene, &art(), &units, &resources, false, 10));
    cache.record_success(&scene, &art(), &units, &resources, false, 10);
    assert!(!cache.needs_render(&scene, &art(), &units, &resources, false, 11));
}

#[wasm_bindgen_test]
fn scene_replacement_retries_and_releases_the_previous_scene() {
    let first = Rc::new(());
    let second = Rc::new(());
    let mut cache = RenderFrameCache::default();
    let units = [];
    let resources = [];

    assert!(cache.needs_render(&first, &art(), &units, &resources, false, 0));
    // A failed render does not update the successful-frame cache.
    assert!(cache.needs_render(&first, &art(), &units, &resources, false, 0));
    cache.record_success(&first, &art(), &units, &resources, false, 0);
    assert!(cache.needs_render(&second, &art(), &units, &resources, false, 0));
    cache.record_success(&second, &art(), &units, &resources, false, 0);
    assert_eq!(Rc::strong_count(&first), 1);
}

#[wasm_bindgen_test]
fn unit_selection_movement_resources_grid_and_moving_animation_invalidate() {
    let scene = Rc::new(());
    let idle = [unit([1.0, 2.0], false, false)];
    let resources = [resource(9), resource(10)];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art(), &idle, &resources, false, 0);

    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([2.0, 2.0], false, false)],
        &resources,
        false,
        0
    ));
    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([1.0, 2.0], true, false)],
        &resources,
        false,
        0
    ));
    assert!(cache.needs_render(&scene, &art(), &idle, &[resource(10)], false, 0));
    assert!(cache.needs_render(&scene, &art(), &idle, &resources, true, 0));

    let moving = [unit([1.0, 2.0], false, true)];
    cache.record_success(&scene, &art(), &moving, &resources, false, 5);
    assert!(cache.needs_render(&scene, &art(), &moving, &resources, false, 6));
}

#[wasm_bindgen_test]
fn a_working_villager_marker_does_not_invalidate_on_animation_ticks() {
    let scene = Rc::new(());
    let mut worker = unit([1.0, 2.0], false, false);
    worker.kind = aoe_rendering::SceneUnitKind::VillagerWorking;
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art(), &[worker], &[], false, 0);
    assert!(!cache.needs_render(&scene, &art(), &[worker], &[], false, 1));
}

fn art() -> aoe_rendering::GameArt {
    let frame = aoe_rendering::GameFrame {
        atlas: aoe_rendering::AtlasAddress {
            page: 0,
            uv: [0.0; 4],
        },
        size: [24.0, 45.0],
        anchor: [12.0, 40.0],
    };
    aoe_rendering::GameArt {
        village: Default::default(),
        town_center: Default::default(),
        walking: vec![frame; 50],
        standing: vec![frame; 50],
        grass: vec![],
        terrain: Default::default(),
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: vec![],
        tree_families: Default::default(),
        scenery: Default::default(),
    }
}

#[wasm_bindgen_test]
fn native_chopping_animates_but_a_stationary_idle_fallback_does_not() {
    let scene = Rc::new(());
    let mut worker = unit([1.0, 2.0], false, false);
    worker.kind = aoe_rendering::SceneUnitKind::VillagerWorking;
    let mut art = art();
    let frame = art.walking[0];
    art.village[0] = vec![frame; 75];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art, &[worker], &[], false, 0);
    assert!(!cache.needs_render(&scene, &art, &[worker], &[], false, 1));
    art.village[2] = vec![frame; 75];
    assert!(cache.needs_render(&scene, &art, &[worker], &[], false, 1));
    cache.record_success(&scene, &art, &[worker], &[], false, 1);
    assert!(cache.needs_render(&scene, &art, &[worker], &[], false, 2));
}
