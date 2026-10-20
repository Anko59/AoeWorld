use super::*;
// Exercise identities independently from raster pixels and painter order.
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn resource_and_unit_identity_collision_cannot_change_their_fog_shading() {
    let (sprite, resource_frame) = super::super::filter_fixture::sprite();
    let mut unit_frame = resource_frame;
    unit_frame.atlas.uv[0] = 0.5;
    let mut art = GameArt {
        actors: Default::default(),
        buildings: std::array::from_fn(|_| Vec::new()),
        village: Default::default(),
        fishing: Default::default(),
        town_center: Default::default(),
        walking: vec![unit_frame],
        standing: vec![unit_frame],
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: Default::default(),
        water_depth: Default::default(),
        resources: std::array::from_fn(|_| vec![resource_frame]),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
    };
    let resource = SceneResource {
        id: 7,
        position: [0.5, 0.5],
        kind: 0,
        visual_variant: 0,
        visual_family: 0,
        elevation_meters: 0.0,
    };
    let unit = SceneUnit {
        kind: SceneUnitKind::Scout,
        id: aoe_core::EntityId(7),
        position: [5.5, 0.5],
        moving: false,
        facing: 0,
        selected: false,
        elevation_meters: 0.0,
    };
    let mut layers = [
        WorldLayer::Sprite(sprite, resource_frame, 0.0, 7),
        WorldLayer::Sprite(sprite, unit_frame, 0.0, 7),
    ];
    shade_objects(&mut layers, &art, &[resource], &[unit], 0, &|p| {
        if p[0] < 1.0 { 0.45 } else { 1.0 }
    });
    let WorldLayer::Sprite(resource_sprite, _, _, _) = layers[0] else {
        panic!("resource");
    };
    let WorldLayer::Sprite(unit, _, _, _) = layers[1] else {
        panic!("unit");
    };
    assert_eq!(resource_sprite.color[..3], [0.45; 3]);
    assert_eq!(unit.color[..3], [1.0; 3]);

    // Fish remain remembered resources on every displayed animation frame.
    art.fishing[0] = vec![resource_frame, unit_frame];
    let fish = SceneResource {
        id: 8,
        visual_family: 5,
        ..resource
    };
    let mut layers = [WorldLayer::Sprite(sprite, unit_frame, 0.0, fish.id)];
    shade_objects(&mut layers, &art, &[fish], &[], 3, &|_| 0.45);
    let WorldLayer::Sprite(fish, _, _, _) = layers[0] else {
        panic!("fish")
    };
    assert_eq!(fish.color[..3], [0.45; 3]);
}
