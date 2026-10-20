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
            actors: Vec::new(),
            buildings: Default::default(),
            village: Default::default(),
            town_center: Default::default(),
            walking: base.walking.clone(),
            standing: base.standing.iter().step_by(10).copied().collect(),
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
    (0x52, 0x35303be98de03071),
    (0x52, 0x1f64f6d17135b109),
    (0x45, 0xade30b3eb841c413),
    (0x45, 0x51983b2f70f1207),
    (0x76, 0x27892752e529923f),
    (0x75, 0x288c066a556120ff),
    (0x17, 0x730fc9b85e811482),
    (0x16, 0xc5a44ceae1f5d116),
    (0x40, 0x7c37440287a9a4cb),
    (0x40, 0xca41f39000d03a56),
    (0x35, 0x1b598134f058d33f),
    (0x35, 0x21d796797416839a),
    (0x53, 0x63a571f8b1143721),
    (0x53, 0x333099a0517a0be9),
    (0x46, 0x90b60c55c7a0f1f),
    (0x46, 0xeb093412a95c723),
    (0x76, 0xe4ea5d639ec61f01),
    (0x75, 0x4c9ad488c6a76941),
    (0x17, 0xd1c21b90123c37c),
    (0x16, 0x59bf69443d832520),
    (0x41, 0xc017fc06a83e6c48),
    (0x41, 0x372bb013056ce635),
    (0x36, 0x6bb5a218a40be000),
    (0x36, 0x56523778ab24600d),
    (0x51, 0x6519c709c9600fa5),
    (0x51, 0x7edcd0e7fb06f26d),
    (0x44, 0xf2f904eac4f3033f),
    (0x44, 0x723a6eada637e1e3),
    (0x77, 0xec85b384a8386cb8),
    (0x76, 0xfdf99f6f8a7091e0),
    (0x18, 0x4e2710396ec2a53d),
    (0x17, 0x927dd80241e4eab9),
    (0x3f, 0x1c1da941e6139ebc),
    (0x3f, 0x1fa390b6e12b60cd),
    (0x34, 0x312c81d6a745ce90),
    (0x34, 0x6b7883ea9d401ca1),
    (0x40, 0xe8ff5be0342730cd),
    (0x40, 0xe8ff5be0342730cd),
    (0x33, 0xf18e0676754522b7),
    (0x33, 0xf18e0676754522b7),
    (0x70, 0xf3ee96fb4121489c),
    (0x70, 0xf3ee96fb4121489c),
    (0x11, 0x5d901859ddfeab21),
    (0x11, 0x5d901859ddfeab21),
    (0x32, 0xb5ddc482b89a2997),
    (0x32, 0xb5ddc482b89a2997),
    (0x27, 0x19b4c3e9dfd26483),
    (0x27, 0x19b4c3e9dfd26483),
    (0x41, 0xd5cead374986f6b9),
    (0x41, 0xa3d77a4a99110071),
    (0x41, 0xd5cead374986f6b9),
    (0x41, 0xa3d77a4a99110071),
    (0x15, 0xf977f404f7b704f7),
    (0x14, 0xf52516d701404c3b),
    (0x15, 0xf977f404f7b704f7),
    (0x14, 0xf52516d701404c3b),
    (0x31, 0x9a738fa8abf995c8),
    (0x31, 0xed426add91312c59),
    (0x31, 0x9a738fa8abf995c8),
    (0x31, 0xed426add91312c59),
    (0x4a, 0x88b8aa6d2a736c46),
    (0x4a, 0xfc29ee44e5d37ef6),
    (0x3d, 0x20b3fac0e922d0ec),
    (0x3d, 0xc8ab639f63647630),
    (0x72, 0x6dc2eb192532b004),
    (0x71, 0xa4d05c2eb28587c),
    (0x13, 0x4c004de706050571),
    (0x12, 0xe4c43f18e475ee95),
    (0x39, 0xe211c0e73bb05dcd),
    (0x39, 0x74dc76b5b9c5ec30),
    (0x2e, 0x44aeae1719020269),
    (0x2e, 0xfbd48063a58df1fc),
];
