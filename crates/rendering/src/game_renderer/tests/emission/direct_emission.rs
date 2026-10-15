//! Full-scene content of the production scene builder over bounded cases.
use super::*;
#[path = "alloc_budget.rs"]
mod alloc_budget;
#[path = "object_scale.rs"]
mod object_scale;
#[path = "scenery.rs"]
mod scenery;

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
            village: Default::default(),
            town_center: Default::default(),
            walking: base.walking.clone(),
            standing: base.standing.clone(),
            grass: base.grass.clone(),
            terrain: base.terrain.clone(),
            terrain_topology: base.terrain_topology,
            water_depth: base.water_depth.clone(),
            resources: base.resources.clone(),
            tree_shadows: base.tree_shadows.clone(),
            tree_families: base.tree_families.clone(),
            scenery: Default::default(),
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
                    kind: Default::default(),
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

/// Current packet bytes, including the nearest terrain-quad marker.
/// (layer count, digest) per mode x camera x terrain x animation, in loop order.
const GOLDEN: &[(usize, u64)] = &[
    (0x10c, 0x25283ed41dc770a9),
    (0x10c, 0x442ed0c8674ac349),
    (0xff, 0x367d12c5012909ff),
    (0xff, 0xbd24f587b0a86fa3),
    (0x130, 0xe077508727b4861d),
    (0x12f, 0xd5acfb2757cf6f61),
    (0xd1, 0x72b0e79220cc606c),
    (0xd0, 0x144681a2e1392684),
    (0xfa, 0x8a2f3109c71043dc),
    (0xfa, 0xf8c0cc11a97a447d),
    (0xef, 0x01eaa8f8c2832ec8),
    (0xef, 0xaf17616026e00ae9),
    (0x10d, 0x72fb059f17a73ec1),
    (0x10d, 0x610f3c5a3baa57d1),
    (0x100, 0x16ac256d39ba9b83),
    (0x100, 0xd0d2d61eb5100967),
    (0x130, 0x90cc0b418f3c6697),
    (0x12f, 0xc6853dea2626014b),
    (0xd1, 0xcf247fe55f59c04e),
    (0xd0, 0x2c82e9e898b8e8e6),
    (0xfb, 0x8371e72063fe6d73),
    (0xfb, 0xe0f2d10c23d2c7ba),
    (0xf0, 0x34ca5eaa521f436b),
    (0xf0, 0x70923fdb6062c1f2),
    (0x10b, 0xc2d139e35cf6ddcd),
    (0x10b, 0x999aa18e27c01bdd),
    (0xfe, 0xec2e87948ed4ed6b),
    (0xfe, 0x0baef4850a45a8af),
    (0x131, 0x74853fe5100b06d6),
    (0x130, 0xc16ebcbadbe4deaa),
    (0xd2, 0xdba911e565645107),
    (0xd1, 0x50dac8b7a82cc6ff),
    (0xf9, 0x0c6d0bfcda6a3a03),
    (0xf9, 0xf338be9c61210e7e),
    (0xee, 0xf70fb475a81e6bff),
    (0xee, 0x14671c713db5006a),
    (0xf0, 0xec7bef1a2231b465),
    (0xf0, 0xec7bef1a2231b465),
    (0xe3, 0x455349b67ecb4577),
    (0xe3, 0x455349b67ecb4577),
    (0x127, 0xcb10176e4341c5be),
    (0x127, 0xcb10176e4341c5be),
    (0xc8, 0xf6ea57c036922def),
    (0xc8, 0xf6ea57c036922def),
    (0xe5, 0xe49aa6d9edf97660),
    (0xe5, 0xe49aa6d9edf97660),
    (0xda, 0xaf81a0bbb612a4d4),
    (0xda, 0xaf81a0bbb612a4d4),
    (0xfb, 0xfcd94a46974b1649),
    (0xfb, 0x3ee2c613ede57589),
    (0xfb, 0xfcd94a46974b1649),
    (0xfb, 0x3ee2c613ede57589),
    (0xcf, 0xe0b9f3d29528622d),
    (0xce, 0x3749fea6f0943205),
    (0xcf, 0xe0b9f3d29528622d),
    (0xce, 0x3749fea6f0943205),
    (0xeb, 0x7d863c9478760e47),
    (0xeb, 0x4bb6ec151c9bfc12),
    (0xeb, 0x7d863c9478760e47),
    (0xeb, 0x4bb6ec151c9bfc12),
    (0x104, 0xc7c81373e765af82),
    (0x104, 0xcd568bc72b5bfbaa),
    (0xf7, 0x56a7d1c6f414efd4),
    (0xf7, 0x26a19f5d7fcdc6c8),
    (0x12c, 0xb59c6dc66d0cad56),
    (0x12b, 0x8568b59ec58c83ba),
    (0xcd, 0xd0a1412fe6467b6f),
    (0xcc, 0xbbd89e03137ce0ef),
    (0xf3, 0x774175bb2a55a836),
    (0xf3, 0xd39656f6c34d846f),
    (0xe8, 0x962346bafacb144a),
    (0xe8, 0xe51663c2c983adf3),
];
