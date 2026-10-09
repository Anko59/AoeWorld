//! Full-scene bitwise parity against frozen pre-change sprite emission.
use super::*;
#[path = "species/frozen_world_sprites.rs"]
mod frozen;

// Frozen old composition: object Vec copied after surfaces/selection; stable sort.
fn old_layers(
    surfaces: &[ProjectedSurfaceTriangle],
    objects: Vec<(Sprite, GameFrame, f64, u64)>,
    units: &[SceneUnit],
    camera: SceneCamera,
) -> Vec<WorldLayer> {
    let mut entries = surfaces
        .iter()
        .copied()
        .map(WorldLayer::Surface)
        .collect::<Vec<_>>();
    for unit in units.iter().filter(|unit| unit.selected) {
        entries.extend(
            game_grid::selection_ring(camera, unit.position, unit.elevation_meters)
                .into_iter()
                .map(|(sprite, depth)| WorldLayer::Selection(sprite, depth)),
        );
    }
    entries.extend(
        objects
            .into_iter()
            .map(|(s, f, d, id)| WorldLayer::Sprite(s, f, d, id)),
    );
    let key = |layer: &WorldLayer| match layer {
        WorldLayer::Surface(t) => (
            t.points
                .iter()
                .map(|p| surface_render_depth(p.world, t.skirt))
                .sum::<f64>()
                / 3.0,
            0,
            0,
        ),
        WorldLayer::Selection(_, d) => (*d, 1, 0),
        WorldLayer::Sprite(_, _, d, id) => (*d, 2, *id),
    };
    entries.sort_by(|a, b| {
        let (ad, at, ai) = key(a);
        let (bd, bt, bi) = key(b);
        ad.total_cmp(&bd).then(at.cmp(&bt)).then(ai.cmp(&bi))
    });
    entries
}

fn address(a: crate::AtlasAddress) -> (u32, [u32; 4]) {
    (a.page, a.uv.map(f32::to_bits))
}
fn assert_scene(actual: &[WorldLayer], expected: &[WorldLayer]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        match (a, b) {
            (WorldLayer::Surface(a), WorldLayer::Surface(b)) => {
                for (a, b) in a.points.iter().zip(&b.points) {
                    assert_eq!(a.world.map(f64::to_bits), b.world.map(f64::to_bits));
                    assert_eq!(
                        [a.screen.x.to_bits(), a.screen.y.to_bits()],
                        [b.screen.x.to_bits(), b.screen.y.to_bits()]
                    );
                }
                assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
                assert_eq!(
                    (a.tile, a.skirt, a.material, a.appearance, a.floor_strengths),
                    (b.tile, b.skirt, b.material, b.appearance, b.floor_strengths)
                );
                assert_eq!(
                    (
                        a.texture_mode,
                        a.tint,
                        a.texture_tile,
                        a.texture_materials,
                        a.pickable,
                        a.order
                    ),
                    (
                        b.texture_mode,
                        b.tint,
                        b.texture_tile,
                        b.texture_materials,
                        b.pickable,
                        b.order
                    )
                );
                assert_eq!(a.texture_uv.map(address), b.texture_uv.map(address));
                assert_eq!(
                    a.texture_blend.map(|v| v.map(address)),
                    b.texture_blend.map(|v| v.map(address))
                );
            }
            (WorldLayer::Selection(a, ad), WorldLayer::Selection(b, bd)) => {
                assert_eq!(bytemuck::bytes_of(a), bytemuck::bytes_of(b));
                assert_eq!(ad.to_bits(), bd.to_bits());
            }
            (WorldLayer::Sprite(a, af, ad, ai), WorldLayer::Sprite(b, bf, bd, bi)) => {
                assert_eq!(bytemuck::bytes_of(a).len(), 112);
                assert_eq!(bytemuck::bytes_of(a), bytemuck::bytes_of(b));
                assert_eq!(address(af.atlas), address(bf.atlas));
                assert_eq!(af.size.map(f32::to_bits), bf.size.map(f32::to_bits));
                assert_eq!(af.anchor.map(f32::to_bits), bf.anchor.map(f32::to_bits));
                assert_eq!((ad.to_bits(), ai), (bd.to_bits(), bi));
            }
            _ => panic!("different layer kind/source order"),
        }
    }
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn direct_production_scene_matches_frozen_old_scene_every_bit() {
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
    for mode in 0..6 {
        let mut art = GameArt {
            walking: base.walking.clone(),
            standing: base.standing.clone(),
            grass: base.grass.clone(),
            terrain: base.terrain.clone(),
            terrain_topology: base.terrain_topology,
            terrain_world: None,
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
                    let expected = old_layers(
                        &surfaces,
                        frozen::world_sprite_frames(
                            &art, terrain, &resources, &units, camera, animation,
                        ),
                        &units,
                        camera,
                    );
                    let actual = direct_world_layers(
                        surfaces.iter().copied(),
                        &art,
                        terrain,
                        &resources,
                        &units,
                        camera,
                        animation,
                    );
                    assert_scene(&actual, &expected);
                }
            }
        }
    }
}
