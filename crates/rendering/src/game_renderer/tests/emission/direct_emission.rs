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
            buildings: std::array::from_fn(|_| Vec::new()),
            village: Default::default(),
            fishing: Default::default(),
            compositions: Vec::new(),
            foundation: Default::default(),
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
                    highlight: 0,
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
                    visual: crate::SceneUnitVisual::new(i % 2 == 0).with_selected(i % 3 == 0),
                    facing: (i % 8) as u8,
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
                    let rings = units.iter().filter(|unit| unit.visual.selected()).count()
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
/// opaque-object coverage bits and semantic markers when optional art is absent.
/// (layer count, digest) per mode x camera x terrain x animation, in loop order.
const GOLDEN: &[(usize, u64)] = &[
    (0x52, 0xb24cfa5ae96d55c1),
    (0x52, 0xb171e9ee79adc459),
    (0x45, 0x9a3ad4482fd90cc3),
    (0x45, 0x876d53367d0ebf4f),
    (0x76, 0x6580aaef1bcb3957),
    (0x75, 0x17c47f1a6816580f),
    (0x17, 0xaec0a5d9fcb1dff2),
    (0x16, 0xab0944e66c675f96),
    (0x40, 0xf253a2ab485a237f),
    (0x40, 0xc895affc6032a7c2),
    (0x35, 0xb96906eefb07ebcb),
    (0x35, 0x717e58507f28652e),
    (0x53, 0x4830c608dd4f7811),
    (0x53, 0x4aa7ff31a7179b39),
    (0x46, 0x800144ebc710e9ff),
    (0x46, 0xa37b3c63331127db),
    (0x76, 0x520e1e9f30994ac1),
    (0x75, 0xb736a76a24c5bab9),
    (0x17, 0xbe3f2ba22c37f69c),
    (0x16, 0xb895119c15eb4f50),
    (0x41, 0xa19dfdca60396f2c),
    (0x41, 0xa7242076df6af231),
    (0x36, 0x22e01bbd04f512ac),
    (0x36, 0xb488ba33c1058b1),
    (0x51, 0x7c18ae4691497655),
    (0x51, 0x5d802138973a9455),
    (0x44, 0x5e2ed62a62acfa7f),
    (0x44, 0xe188b64c6046ad43),
    (0x77, 0x60655ace15c6bf70),
    (0x76, 0x99723594d635a2e0),
    (0x18, 0x3198fa3eab054d95),
    (0x17, 0x171aa1101d8e8991),
    (0x3f, 0x81272999e3463d90),
    (0x3f, 0x8d2795c15a495c21),
    (0x34, 0xf38221058115f0fc),
    (0x34, 0x7fc46669cbf19acd),
    (0x40, 0x9cb4a2693a24ae55),
    (0x40, 0x9cb4a2693a24ae55),
    (0x33, 0xcb6ce9cf3aefcff7),
    (0x33, 0xcb6ce9cf3aefcff7),
    (0x70, 0xf585f5bef8d9b9fc),
    (0x70, 0xf585f5bef8d9b9fc),
    (0x11, 0x23081165e408a7f1),
    (0x11, 0x23081165e408a7f1),
    (0x32, 0xc7e422a8a1f0eefb),
    (0x32, 0xc7e422a8a1f0eefb),
    (0x27, 0x844d31054b824f9f),
    (0x27, 0x844d31054b824f9f),
    (0x41, 0xe0fc4dcb5e4c0641),
    (0x41, 0x6a17b23838f11da9),
    (0x41, 0xe0fc4dcb5e4c0641),
    (0x41, 0x6a17b23838f11da9),
    (0x15, 0x393782464d61b54b),
    (0x14, 0x4dae7ab1bfdf6b17),
    (0x15, 0x393782464d61b54b),
    (0x14, 0x4dae7ab1bfdf6b17),
    (0x31, 0x7daf1b76c02ea98c),
    (0x31, 0xa8e489bbd3bd63dd),
    (0x31, 0x7daf1b76c02ea98c),
    (0x31, 0xa8e489bbd3bd63dd),
    (0x4a, 0xa8858d02553fcbaa),
    (0x4a, 0xb473a46b61cb8ba),
    (0x3d, 0x8e6ed038986ce620),
    (0x3d, 0x99fcf5e53abdc77c),
    (0x72, 0xd608657422b37ec),
    (0x71, 0xd5af8de7f2f07584),
    (0x13, 0x2e8d61716c673bf9),
    (0x12, 0x4110ff9173dbb12d),
    (0x39, 0xf60249300b8405f9),
    (0x39, 0x5d75a9307dc9e13c),
    (0x2e, 0x2d85879fb4f51a1d),
    (0x2e, 0x33369c227611f090),
];
