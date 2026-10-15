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

/// Current packet bytes, including the nearest terrain-quad marker and
/// semantic unit markers when optional sprite groups are absent.
/// (layer count, digest) per mode x camera x terrain x animation, in loop order.
const GOLDEN: &[(usize, u64)] = &[
    (0x52, 0x5ce09f39b60e9031),
    (0x52, 0xd15a2c9da4a17289),
    (0x45, 0x22ab974e01bd2f53),
    (0x45, 0x8962d5dd350a37c7),
    (0x76, 0xc4647ea7616e1c7f),
    (0x75, 0x7d2b79f0531e6abf),
    (0x17, 0x2fec6875c03d8442),
    (0x16, 0x63331515ce37e9d6),
    (0x40, 0xab85420b9ddf95ab),
    (0x40, 0x974e25ac2bd89336),
    (0x35, 0x26bb039c273587f),
    (0x35, 0xd502cf7183ab75da),
    (0x53, 0x35dd291f439789e1),
    (0x53, 0xbb1b504eb129a8a9),
    (0x46, 0xb0ccbcfd775c1fdf),
    (0x46, 0xfc7bba8d36742f63),
    (0x76, 0x388a28badd436c41),
    (0x75, 0xf46b793a64256f01),
    (0x17, 0x801030a2b52f183c),
    (0x16, 0xb4c9b2550650c7e0),
    (0x41, 0xa4948224f90fa688),
    (0x41, 0x22425292ddfe5475),
    (0x36, 0xc928fd4b63dbe940),
    (0x36, 0xaa8cfc77d8137dcd),
    (0x51, 0xb7b9bcaa53500025),
    (0x51, 0x27976a32e6ea7d6d),
    (0x44, 0xcdb392b6aa36d3ff),
    (0x44, 0x2724b9d144a3),
    (0x77, 0x4d2704a5ec098678),
    (0x76, 0xf706e37d32bfcca0),
    (0x18, 0x1b17f98674868ffd),
    (0x17, 0x4c75fa341cb60d79),
    (0x3f, 0x128833c28ffc1b1c),
    (0x3f, 0x1d7c0a26d618c82d),
    (0x34, 0x20a1aa488003bed0),
    (0x34, 0x32fe1030a5fc85e1),
    (0x40, 0xa0e93f416bcc878d),
    (0x40, 0xa0e93f416bcc878d),
    (0x33, 0x65309d78223c3737),
    (0x33, 0x65309d78223c3737),
    (0x70, 0xa4e198ba787adbdc),
    (0x70, 0xa4e198ba787adbdc),
    (0x11, 0x136a6b27d446ebe1),
    (0x11, 0x136a6b27d446ebe1),
    (0x32, 0xfe0dd53b4d74f117),
    (0x32, 0xfe0dd53b4d74f117),
    (0x27, 0x63119e5077eca683),
    (0x27, 0x63119e5077eca683),
    (0x41, 0x77295fb62b538739),
    (0x41, 0xe5d051a4af79f271),
    (0x41, 0x77295fb62b538739),
    (0x41, 0xe5d051a4af79f271),
    (0x15, 0xf9ad7c5a3c350437),
    (0x14, 0x533ba1d5cfb257b),
    (0x15, 0xf9ad7c5a3c350437),
    (0x14, 0x533ba1d5cfb257b),
    (0x31, 0xdb041f1241484388),
    (0x31, 0x90251048aa27d319),
    (0x31, 0xdb041f1241484388),
    (0x31, 0x90251048aa27d319),
    (0x4a, 0x126bc86b2ceec306),
    (0x4a, 0x50fcec796e935b6),
    (0x3d, 0xab26d22189a175ec),
    (0x3d, 0xa82ed68838ddd9b0),
    (0x72, 0xac3d23826cccd6c4),
    (0x71, 0x56e28af83b3213c),
    (0x13, 0xc30bbda90f28ab1),
    (0x12, 0x7c6cfea10f8c1d5),
    (0x39, 0x8d002bd680f1e08d),
    (0x39, 0xc113b83d3de6a1f0),
    (0x2e, 0xe51c34f7fef8b2c9),
    (0x2e, 0xfab426fc38c59ddc),
];
