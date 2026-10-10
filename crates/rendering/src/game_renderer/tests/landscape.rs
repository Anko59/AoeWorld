use super::*;

#[wasm_bindgen_test]
fn indexed_world_layers_match_original_stable_sort_packets_and_frame_order() {
    let camera = SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let terrain = [SceneTerrain {
        position: [0.5; 2],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: None,
    }];
    let mut surfaces = projected_surface_triangles(&terrain, camera);
    let mut duplicate = surfaces[0];
    duplicate.color = [0.2, 0.8, 0.3];
    surfaces.push(duplicate);
    let frame = synthetic_art().resources[1][0];
    let objects = (0..32)
        .map(|index| {
            (
                Sprite {
                    position: [0.0; 2],
                    radius: [0.1; 2],
                    color: [index as f32 / 32.0, 0.0, 0.0, 1.0],
                    uv: frame.atlas.uv,
                    depths: [0.0; 4],
                    terrain_blend: [[0.0; 4]; 2],
                    pages: [2, 0, 0, 0],
                },
                frame,
                [
                    0.0,
                    -0.0,
                    8.0,
                    f64::INFINITY,
                    f64::from_bits(0x7ff8_0000_0000_0001),
                ][index % 5],
                (index % 3) as u64,
            )
        })
        .collect::<Vec<_>>();
    let mut original = surfaces
        .iter()
        .copied()
        .map(WorldLayer::Surface)
        .collect::<Vec<_>>();
    original.extend(
        objects
            .iter()
            .copied()
            .map(|(s, f, d, id)| WorldLayer::Sprite(s, f, d, id)),
    );
    original.sort_by(|left, right| {
        let left = layer_order(left);
        let right = layer_order(right);
        left.0
            .total_cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    let actual = ordered_world_layers(surfaces, objects, &[], camera);
    assert_eq!(actual.len(), original.len());
    for (new, old) in actual.iter().zip(&original) {
        match (new, old) {
            (WorldLayer::Surface(a), WorldLayer::Surface(b)) => {
                assert_eq!(a.points.map(|p| p.world), b.points.map(|p| p.world));
                assert_eq!(a.color, b.color);
            }
            (WorldLayer::Sprite(a, fa, da, ida), WorldLayer::Sprite(b, fb, db, idb)) => {
                assert_eq!(bytemuck::bytes_of(a), bytemuck::bytes_of(b));
                assert_eq!(fa.atlas, fb.atlas);
                assert_eq!(da.to_bits(), db.to_bits());
                assert_eq!(ida, idb);
            }
            _ => panic!("indexed sort changed layer kind/order"),
        }
    }
}

#[wasm_bindgen_test]
fn semantic_tree_families_share_healthy_broadleaf_draw_cull_and_shadow_fallback() {
    let mut art = synthetic_art();
    let base = art.resources[1][0];
    art.resources[1] = (0..14)
        .map(|index| GameFrame {
            atlas: crate::AtlasAddress {
                page: 2,
                uv: [index as f32 / 100.0, 0.0, 0.001, 0.001],
            },
            ..base
        })
        .collect();
    art.tree_shadows = art.resources[1].clone();
    let camera = SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    for family in 0..=4 {
        for variant in 0..=255 {
            let resource = SceneResource {
                id: 1,
                position: [0.5; 2],
                kind: 1,
                visual_variant: variant,
                visual_family: family,
                elevation_meters: 0.0,
            };
            let selected = scene_resource_frame(&art, resource).expect("approved generic art");
            let index = if family == 0 {
                crate::resource_frame_index(1, variant, 14).unwrap()
            } else {
                [0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13][usize::from(variant) % 11]
            };
            assert_eq!(selected.atlas, art.resources[1][index].atlas);
            assert!(resource_sprite_bounds(resource, selected, camera).is_some());
            let terrain = [SceneTerrain {
                position: [0.5; 2],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            }];
            let drawn = world_sprite_frames(&art, &terrain, &[resource], &[], camera, 0);
            assert_eq!(drawn.len(), 2, "one retained tree plus its matching shadow");
            assert!(
                drawn
                    .iter()
                    .all(|(_, frame, _, _)| frame.atlas == selected.atlas)
            );
        }
    }
}
