//! Full-scene content of the production scene builder over bounded cases.
use super::*;

const FNV: u64 = 0x0000_0100_0000_01b3;

fn mix(hash: &mut u64, value: u64) {
    *hash = (*hash ^ value).wrapping_mul(FNV);
}

fn mix_bytes(hash: &mut u64, bytes: &[u8]) {
    bytes.iter().for_each(|byte| mix(hash, u64::from(*byte)));
}

fn mix_frame(hash: &mut u64, frame: &GameFrame) {
    mix(hash, u64::from(frame.atlas.page));
    frame
        .atlas
        .uv
        .iter()
        .for_each(|v| mix(hash, v.to_bits().into()));
    frame
        .size
        .iter()
        .for_each(|v| mix(hash, v.to_bits().into()));
    frame
        .anchor
        .iter()
        .for_each(|v| mix(hash, v.to_bits().into()));
}

/// Order-sensitive digest of every layer's kind, depth, id, sprite bytes and frame.
fn digest(layers: &[WorldLayer]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for layer in layers {
        match layer {
            WorldLayer::Surface(t) => {
                mix(&mut hash, 1);
                mix(&mut hash, triangle_depth(t).to_bits());
                mix(&mut hash, u64::from(t.order));
                t.color
                    .iter()
                    .for_each(|v| mix(&mut hash, v.to_bits().into()));
            }
            WorldLayer::Selection(sprite, depth) => {
                mix(&mut hash, 2);
                mix(&mut hash, depth.to_bits());
                mix_bytes(&mut hash, bytemuck::bytes_of(sprite));
            }
            WorldLayer::Sprite(sprite, frame, depth, id) => {
                mix(&mut hash, 3);
                mix(&mut hash, depth.to_bits());
                mix(&mut hash, *id);
                mix_bytes(&mut hash, bytemuck::bytes_of(sprite));
                mix_frame(&mut hash, frame);
            }
        }
    }
    hash
}

/// The stable-sort contract: (depth by total order, surface < ring < sprite, id).
fn assert_sorted(layers: &[WorldLayer]) {
    for pair in layers.windows(2) {
        let (a, b) = (layer_order(&pair[0]), layer_order(&pair[1]));
        let ordering = a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2));
        assert!(ordering.is_le(), "layers out of order: {a:?} then {b:?}");
    }
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn direct_scene_has_expected_content_order_and_ties_in_72_bounded_cases() {
    use aoe_core::{EntityId, ScreenPoint};
    let mut base = species_fixture::art();
    let frame = |i| {
        let mut f = species_fixture::frame(i);
        f.atlas.page = (i % 3) as u32;
        f.size = [24.0 + (i % 7) as f32, 48.0 + (i % 13) as f32];
        f.anchor = [-8.0 + (i % 31) as f32, 60.0];
        f
    };
    base.walking = (0..80).map(frame).collect();
    base.standing = (80..160).map(frame).collect();
    base.grass = vec![frame(160)];
    base.terrain = std::array::from_fn(|i| vec![frame(161 + i)]);
    base.resources[1] = (170..184).map(frame).collect();
    base.tree_shadows = (184..198).map(frame).collect();
    base.tree_families = [
        (198..207).map(frame).collect(),
        (207..220).map(frame).collect(),
    ];
    let mut digests = Vec::new();
    for mode in 0..6 {
        let mut art = GameArt {
            walking: base.walking.clone(),
            standing: base.standing.clone(),
            grass: base.grass.clone(),
            terrain: base.terrain.clone(),
            terrain_topology: base.terrain_topology,
            resources: base.resources.clone(),
            tree_shadows: base.tree_shadows.clone(),
            tree_families: base.tree_families.clone(),
        };
        match mode {
            1 => {
                art.tree_families = Default::default();
                art.tree_shadows.clear();
            }
            2 => {
                art.tree_families[0].truncate(5);
                art.tree_families[1].truncate(1);
            }
            3 => {
                art.walking.clear();
                art.standing.clear();
            }
            4 => {
                art.grass.clear();
                art.resources[2].clear();
            }
            5 => {
                art.resources[1].clear();
                art.tree_shadows.truncate(3);
            }
            _ => {}
        }
        for camera_case in 0..3 {
            let mut camera = species_fixture::camera();
            let world_max = f64::from(aoe_core::MAX_WORLD_DIMENSION_TILES);
            camera.center = [[0.5; 2], [-0.5, world_max + 0.5], [world_max - 0.5; 2]][camera_case];
            camera.zoom = [1.0, 0.25, 2.0][camera_case];
            camera.focus_elevation_meters = [-0.0, 12.0, -2.0][camera_case];
            let projection = Camera {
                center: camera.center,
                zoom: camera.zoom,
                viewport: camera.viewport,
                focus_elevation_meters: camera.focus_elevation_meters,
            };
            let positions = [
                camera.center,
                camera.center,
                projection.screen_to_world_at_height(ScreenPoint { x: -8.0, y: 64.0 }, 0.0),
                projection.screen_to_world_at_height(ScreenPoint { x: 140.0, y: 64.0 }, 0.0),
                [100000.0; 2],
                [f64::NAN, camera.center[1]],
            ];
            let resources = (0..24)
                .map(|i| SceneResource {
                    id: (i % 3) as u64,
                    position: positions[i % 6],
                    kind: [1, 1, 1, 1, 0, 2, 3, 255][i % 8],
                    visual_family: [0, 1, 2, 4][i % 4],
                    visual_variant: [0, 3, 8, 255][i % 4],
                    elevation_meters: if i == 1 {
                        f64::from_bits(0xfff8_0000_0000_0002)
                    } else {
                        0.0
                    },
                })
                .collect::<Vec<_>>();
            let units = (0..16)
                .map(|i| SceneUnit {
                    id: EntityId((i % 3) as u32),
                    position: positions[(i / 2) % 6],
                    moving: i % 2 == 0,
                    facing: (i % 8) as u8,
                    selected: i % 3 == 0,
                    elevation_meters: if i == 1 {
                        f64::from_bits(0x7ff8_0000_0000_0001)
                    } else {
                        0.0
                    },
                })
                .collect::<Vec<_>>();
            let terrain = [SceneTerrain {
                appearance: None,
                position: camera.center,
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
            }];
            let mut surfaces = projected_surface_triangles(&terrain, camera);
            apply_terrain_textures(&mut surfaces, &art);
            if let Some(mut t) = surfaces.first().copied() {
                t.order = t.order.wrapping_add(31);
                surfaces.push(t); // Exact depth/type ties with distinct source order.
                t.order = 77;
                t.points
                    .iter_mut()
                    .for_each(|p| p.world = [camera.center[0], camera.center[1], 0.0]);
                surfaces.push(t); // Cross-type tie against visible objects.
                t.points[0].world[2] = f64::from_bits(0x7ff8_0000_0000_0001);
                surfaces.push(t);
            }
            for terrain in [&[][..], &terrain[..]] {
                for animation in [0, 19] {
                    let actual = direct_world_layers(
                        surfaces.iter().copied(),
                        &art,
                        terrain,
                        &resources,
                        &units,
                        camera,
                        animation,
                    );
                    assert_sorted(&actual);
                    let rings = units.iter().filter(|unit| unit.selected).count()
                        * game_grid::SELECTION_RING_SPRITES;
                    let count = |f: fn(&WorldLayer) -> bool| actual.iter().filter(|l| f(l)).count();
                    assert_eq!(
                        count(|l| matches!(l, WorldLayer::Surface(_))),
                        surfaces.len()
                    );
                    assert_eq!(count(|l| matches!(l, WorldLayer::Selection(..))), rings);
                    digests.push((actual.len(), digest(&actual)));
                }
            }
        }
    }
    assert_eq!(digests.len(), 72);
    assert!(
        digests.as_slice() == GOLDEN,
        "scene content changed; digests: {digests:#x?}"
    );
}

/// (layer count, digest) per mode x camera x terrain x animation, in loop order.
const GOLDEN: &[(usize, u64)] = &[
    (0x108, 0x4ff9549368174cbd),
    (0x109, 0xe5d11dfdfb418a03),
    (0xfb, 0x10250694ee240018),
    (0xfc, 0x78ffe8f66d7cb5da),
    (0x12d, 0xa5d19447e5c6c5a),
    (0x12d, 0x720892cdb69fe2d8),
    (0xce, 0x772e6f8b1afefa98),
    (0xce, 0xf83a28ee62ca3896),
    (0xfa, 0x36ca41843896320a),
    (0xfa, 0x8ec3940fff66c902),
    (0xef, 0x847bfba088addf45),
    (0xef, 0x2cb397fb55cdb22d),
    (0x107, 0x221ef9db8e214dff),
    (0x108, 0xcfe0465fc47f6cf1),
    (0xfa, 0x24ec3a04e8f7f862),
    (0xfb, 0x933b9c246394cae8),
    (0x12d, 0x3977d0a04018d612),
    (0x12d, 0x473e74f6dd87c908),
    (0xce, 0x45fe4f1575ecdce0),
    (0xce, 0xbff6d9f9e31c5f56),
    (0xfb, 0x13ee7ada4b6c209d),
    (0xfb, 0xce30a1c927bf54c9),
    (0xf0, 0x714c0e90345f858a),
    (0xf0, 0xdfef5c441e65d97e),
    (0x106, 0xcb2981da9feb76cd),
    (0x107, 0x4cdf48c02412e02b),
    (0xf9, 0xa0c3f7d48ebb6cdc),
    (0xfa, 0x789229149f14dab6),
    (0x12d, 0x16dd46adc430f07f),
    (0x12d, 0xb459f659a334f251),
    (0xce, 0x10586f63adbbfd11),
    (0xce, 0x625f8c10d31ec5ab),
    (0xf9, 0x7244bd829d58159e),
    (0xf9, 0x359a1add937c3dc2),
    (0xee, 0x297e79f5771f2461),
    (0xee, 0xc747349ac150de35),
    (0xed, 0xffb1bf7be78521c2),
    (0xed, 0xffb1bf7be78521c2),
    (0xe0, 0x23806bf3324d1b1f),
    (0xe0, 0x23806bf3324d1b1f),
    (0x127, 0x55927f38a2c2ff62),
    (0x127, 0x55927f38a2c2ff62),
    (0xc8, 0xb1c8f326681c515c),
    (0xc8, 0xb1c8f326681c515c),
    (0xe5, 0x5fa1c7164956a9e7),
    (0xe5, 0x5fa1c7164956a9e7),
    (0xda, 0xa498339f56b4617c),
    (0xda, 0xa498339f56b4617c),
    (0xf7, 0x13c26bbb8972f88e),
    (0xf8, 0x12a5b323a99f6290),
    (0xf7, 0x13c26bbb8972f88e),
    (0xf8, 0x12a5b323a99f6290),
    (0xcc, 0xfcb7114f1c59899d),
    (0xcc, 0x9c205ef2155397),
    (0xcc, 0xfcb7114f1c59899d),
    (0xcc, 0x9c205ef2155397),
    (0xeb, 0x7affeb6312dbd9e9),
    (0xeb, 0x636f002ae701699),
    (0xeb, 0x7affeb6312dbd9e9),
    (0xeb, 0x636f002ae701699),
    (0x101, 0xe55206e72a3969dd),
    (0x102, 0x2c03aa3fffb5d91f),
    (0xf4, 0x111b34355bd006e0),
    (0xf5, 0xa5a54e3115c17396),
    (0x129, 0x64082c8bdacc2b2b),
    (0x129, 0x23f23eb64b990cb5),
    (0xca, 0xb9850246da1f8bed),
    (0xca, 0xdb312264fed894c7),
    (0xf3, 0x761a6dc1edb6dd10),
    (0xf3, 0x91bd9f3cf534955c),
    (0xe8, 0x7ef355a8aaf10fd3),
    (0xe8, 0x7ad1f181f0033397),
];
