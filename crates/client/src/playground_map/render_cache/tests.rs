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
    cache.record_success(&scene, &art(), &units, &[], false, 0, camera());
    assert_eq!(cache.position(EntityId(7)), Some([1.0, 2.0]));
    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([1.5, 2.0], false, true)],
        &[],
        false,
        0,
        camera()
    ));
    // Failed or throttled frames cannot move hitboxes ahead of their pixels.
    assert_eq!(cache.position(EntityId(7)), Some([1.0, 2.0]));
    cache.clear();
    assert_eq!(cache.position(EntityId(7)), None);
}

#[wasm_bindgen_test]
fn stationary_fish_keep_the_animation_clock_live() {
    let scene = Rc::new(());
    let fish = SceneResource {
        kind: 0,
        visual_family: 5,
        ..resource(9)
    };
    let mut art = art();
    for (family, period, count) in [(5, 3, 17), (6, 9, 7)] {
        let fish = SceneResource {
            visual_family: family,
            ..fish
        };
        art.fishing[usize::from(family == 6)] = vec![art.walking[0]; count];
        let mut cache = RenderFrameCache::default();
        cache.record_success(&scene, &art, &[], &[fish], false, 0, camera());
        for tick in 1..period {
            assert!(!cache.needs_render(&scene, &art, &[], &[fish], false, tick, camera()));
        }
        assert!(cache.needs_render(&scene, &art, &[], &[fish], false, period, camera()));
        assert!(!cache.needs_render(&scene, &art, &[], &[fish], false, period * count, camera()));
    }
}

#[wasm_bindgen_test]
fn missing_static_and_offscreen_fish_art_ignore_animation_ticks() {
    let scene = Rc::new(());
    for family in [5, 6] {
        for (count, position) in [(0, [3.5, 4.5]), (1, [3.5, 4.5]), (7, [10000.0; 2])] {
            let mut art = art();
            art.fishing[usize::from(family == 6)] = vec![art.walking[0]; count];
            let fish = SceneResource {
                kind: 0,
                visual_family: family,
                position,
                ..resource(9)
            };
            let mut cache = RenderFrameCache::default();
            cache.record_success(&scene, &art, &[], &[fish], false, 0, camera());
            assert!(!cache.needs_render(&scene, &art, &[], &[fish], false, 9, camera()));
        }
    }
}

#[wasm_bindgen_test]
fn unchanged_static_frame_ignores_animation_clock() {
    let scene = Rc::new(());
    let units = [unit([1.0, 2.0], false, false)];
    let resources = [resource(9)];
    let mut cache = RenderFrameCache::default();

    assert!(cache.needs_render(&scene, &art(), &units, &resources, false, 10, camera()));
    cache.record_success(&scene, &art(), &units, &resources, false, 10, camera());
    assert!(!cache.needs_render(&scene, &art(), &units, &resources, false, 11, camera()));
}

#[wasm_bindgen_test]
fn scene_replacement_retries_and_releases_the_previous_scene() {
    let first = Rc::new(());
    let second = Rc::new(());
    let mut cache = RenderFrameCache::default();
    let units = [];
    let resources = [];

    assert!(cache.needs_render(&first, &art(), &units, &resources, false, 0, camera()));
    // A failed render does not update the successful-frame cache.
    assert!(cache.needs_render(&first, &art(), &units, &resources, false, 0, camera()));
    cache.record_success(&first, &art(), &units, &resources, false, 0, camera());
    assert!(cache.needs_render(&second, &art(), &units, &resources, false, 0, camera()));
    cache.record_success(&second, &art(), &units, &resources, false, 0, camera());
    assert_eq!(Rc::strong_count(&first), 1);
}

#[wasm_bindgen_test]
fn unit_selection_movement_resources_grid_and_moving_animation_invalidate() {
    let scene = Rc::new(());
    let idle = [unit([1.0, 2.0], false, false)];
    let resources = [resource(9), resource(10)];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art(), &idle, &resources, false, 0, camera());

    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([2.0, 2.0], false, false)],
        &resources,
        false,
        0,
        camera()
    ));
    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([1.0, 2.0], true, false)],
        &resources,
        false,
        0,
        camera()
    ));
    assert!(cache.needs_render(&scene, &art(), &idle, &[resource(10)], false, 0, camera()));
    assert!(cache.needs_render(&scene, &art(), &idle, &resources, true, 0, camera()));

    let moving = [unit([1.0, 2.0], false, true)];
    cache.record_success(&scene, &art(), &moving, &resources, false, 5, camera());
    assert!(cache.needs_render(&scene, &art(), &moving, &resources, false, 6, camera()));
}

#[wasm_bindgen_test]
fn a_working_villager_marker_does_not_invalidate_on_animation_ticks() {
    let scene = Rc::new(());
    let mut worker = unit([1.0, 2.0], false, false);
    worker.kind = aoe_rendering::SceneUnitKind::VillagerChopping;
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art(), &[worker], &[], false, 0, camera());
    assert!(!cache.needs_render(&scene, &art(), &[worker], &[], false, 1, camera()));
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
        actors: Vec::new(),
        buildings: Default::default(),
        village: Default::default(),
        fishing: Default::default(),
        compositions: Vec::new(),
        foundation: Default::default(),
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
    worker.kind = aoe_rendering::SceneUnitKind::VillagerChopping;
    let mut art = art();
    let frame = art.walking[0];
    art.village[0] = vec![frame; 75];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art, &[worker], &[], false, 0, camera());
    assert!(!cache.needs_render(&scene, &art, &[worker], &[], false, 1, camera()));
    art.village[2] = vec![frame; 75];
    assert!(cache.needs_render(&scene, &art, &[worker], &[], false, 1, camera()));
    cache.record_success(&scene, &art, &[worker], &[], false, 1, camera());
    assert!(cache.needs_render(&scene, &art, &[worker], &[], false, 2, camera()));
}

#[wasm_bindgen_test]
fn offscreen_native_chopping_does_not_request_animation_frames() {
    let mut worker = unit([10000.0, 10000.0], false, false);
    worker.kind = aoe_rendering::SceneUnitKind::VillagerChopping;
    let mut art = art();
    art.village[2] = vec![art.walking[0]; 75];
    let camera = SceneCamera {
        center: [1.0, 2.0],
        zoom: 1.0,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    assert_eq!(moving_animation(&art, &[worker], 1, camera), None);
    worker.position = [1.0, 2.0];
    assert_eq!(moving_animation(&art, &[worker], 1, camera), Some(1));
    worker.position = [10000.0, 10000.0];
    assert_eq!(moving_animation(&art, &[worker], 2, camera), None);
}

fn camera() -> SceneCamera {
    SceneCamera {
        center: [1.0, 2.0],
        zoom: 1.0,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    }
}

#[wasm_bindgen_test]
fn actor_phase_does_not_invalidate_without_visible_animated_art() {
    let scene = Rc::new(());
    let mut actor = unit([1.0, 2.0], false, false);
    actor.kind = aoe_rendering::SceneUnitKind::actor(0);
    let mut art = art();
    for frames in [vec![], vec![art.walking[0]; 5], vec![art.walking[0]; 10]] {
        art.actors = vec![frames];
        for position in [[1.0, 2.0], [10000.0, 10000.0]] {
            actor.position = position;
            actor.facing = 2;
            let mut cache = RenderFrameCache::default();
            cache.record_success(&scene, &art, &[actor], &[], false, 0, camera());
            actor.facing = 2 | (1 << 3);
            let visible_animation = art.actors[0].len() == 10 && position == [1.0, 2.0];
            assert_eq!(
                cache.needs_render(&scene, &art, &[actor], &[], false, 1, camera()),
                visible_animation,
                "frames={}, position={position:?}",
                art.actors[0].len()
            );
        }
    }
}

#[wasm_bindgen_test]
fn visible_actor_phase_and_offscreen_actor_state_still_invalidate() {
    let scene = Rc::new(());
    let mut actor = unit([10000.0, 10000.0], false, false);
    actor.kind = aoe_rendering::SceneUnitKind::actor(0);
    let mut art = art();
    art.actors = vec![vec![art.walking[0]; 10]];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art, &[actor], &[], false, 0, camera());
    actor.facing = 3;
    assert!(cache.needs_render(&scene, &art, &[actor], &[], false, 0, camera()));
    actor.position = [1.0, 2.0];
    cache.record_success(&scene, &art, &[actor], &[], false, 0, camera());
    assert!(!cache.needs_render(&scene, &art, &[actor], &[], false, 1, camera()));
    actor.facing |= 1 << 3;
    assert!(cache.needs_render(&scene, &art, &[actor], &[], false, 1, camera()));
}

#[wasm_bindgen_test]
fn offscreen_actor_phase_does_not_invalidate_even_with_real_frames() {
    let scene = Rc::new(());
    let mut actor = unit([10000.0, 10000.0], false, false);
    actor.kind = aoe_rendering::SceneUnitKind::actor(0);
    let mut art = art();
    art.actors = vec![vec![art.walking[0]; 10]];
    let mut cache = RenderFrameCache::default();
    cache.record_success(&scene, &art, &[actor], &[], false, 0, camera());
    actor.facing |= 1 << 3;
    assert!(!cache.needs_render(&scene, &art, &[actor], &[], false, 1, camera()));
}
