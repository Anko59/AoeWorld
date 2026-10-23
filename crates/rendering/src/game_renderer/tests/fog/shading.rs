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
        compositions: Default::default(),
        foundation: Default::default(),
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
        highlight: 0,
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
        visual: crate::SceneUnitVisual::new(false),
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
    let WorldLayer::Sprite(unit_sprite, _, _, _) = layers[1] else {
        panic!("unit");
    };
    assert_eq!(resource_sprite.color[..3], [0.45; 3]);
    assert_eq!(unit_sprite.color[..3], [1.0; 3]);

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

    // Full 64-bit resource identities, paired shadows and duplicate updates
    // must retain the original frame-specific, last-write shading semantics.
    let mut other = resource_frame;
    other.atlas.uv[1] = 0.25;
    let mut art = art;
    art.resources[1] = vec![other];
    art.tree_shadows = vec![resource_frame];
    let resources = [
        SceneResource {
            id: 7,
            kind: 1,
            position: [5.5, 0.5],
            ..resource
        },
        SceneResource {
            id: 7 + (1_u64 << 32),
            position: [5.5, 0.5],
            ..resource
        },
        resource,
    ];
    let mut layers = [
        WorldLayer::Sprite(sprite, other, 0.0, 7),
        WorldLayer::Sprite(sprite, resource_frame, 0.0, 7),
        WorldLayer::Sprite(sprite, resource_frame, 0.0, 7 + (1_u64 << 32)),
        WorldLayer::Sprite(sprite, unit_frame, 0.0, 7),
        WorldLayer::Sprite(sprite, unit_frame, 0.0, 999),
    ];
    shade_objects(&mut layers, &art, &resources, &[unit], 0, &|p| {
        if p[0] < 1.0 { 0.45 } else { 1.0 }
    });
    for (layer, expected) in layers.iter().zip([1.0, 0.45, 1.0, 1.0, 1.0]) {
        let WorldLayer::Sprite(sprite, _, _, _) = layer else {
            panic!("sprite");
        };
        assert_eq!(sprite.color[..3], [expected; 3]);
    }

    let resources = (0..8192_u64)
        .map(|i| SceneResource {
            id: (i << 32) | 7,
            kind: 1,
            position: [if i % 2 == 0 { 0.5 } else { 5.5 }, 0.5],
            ..resource
        })
        .collect::<Vec<_>>();
    let units = (0..1024)
        .map(|id| SceneUnit {
            id: aoe_core::EntityId(id),
            ..unit
        })
        .collect::<Vec<_>>();
    let mut layers = resources
        .iter()
        .flat_map(|r| {
            [other, resource_frame].map(|frame| WorldLayer::Sprite(sprite, frame, 0.0, r.id))
        })
        .chain(
            units
                .iter()
                .map(|u| WorldLayer::Sprite(sprite, unit_frame, 0.0, u64::from(u.id.0))),
        )
        .collect::<Vec<_>>();
    let started = js_sys::Date::now();
    shade_objects(&mut layers, &art, &resources, &units, 0, &|p| {
        if p[0] < 1.0 { 0.45 } else { 1.0 }
    });
    wasm_bindgen_test::console_log!(
        "fog: 8192 paired resources + 1024 units, {} layers, {} ms; {}-byte factor, {}-byte resource record (temporary vectors/index have additional capacity)",
        layers.len(),
        js_sys::Date::now() - started,
        std::mem::size_of::<Factor>(),
        std::mem::size_of::<ResourceFactor>()
    );
    for (i, layer) in layers.iter().enumerate() {
        let WorldLayer::Sprite(sprite, _, _, _) = layer else {
            panic!("sprite");
        };
        let expected = if i < 16384 && i / 2 % 2 == 0 {
            0.45
        } else {
            1.0
        };
        assert_eq!(sprite.color[..3], [expected; 3]);
    }
}
