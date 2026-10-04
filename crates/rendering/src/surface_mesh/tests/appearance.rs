use super::*;
use std::collections::BTreeMap;

#[wasm_bindgen_test]
fn fixed_texture_frame_loop_preserves_array_map_option_and_uv_bits() {
    let template = projected_surface_triangles(&map(2), camera([0.5, 0.5], 1.0, [256.0, 128.0]))[0];
    for uv in [
        [0.0, -0.0, 0.01, 0.02],
        [f32::from_bits(0x7fc0_0001), f32::INFINITY, -0.0, 0.01],
    ] {
        for missing in 0..3 {
            let mut art = test_art(GameFrame {
                uv,
                size: [97.0, 49.0],
                anchor: [48.0, 24.0],
            });
            if missing != 0 {
                art.grass.clear();
                art.terrain[1].clear();
            }
            if missing == 2 {
                for frames in &mut art.terrain {
                    frames.clear();
                }
            }
            for materials in [None, Some([0, 1, 255]), Some([2; 3]), Some([6, 0, 2])] {
                let mut triangle = template;
                triangle.material = 1;
                triangle.texture_materials = materials;
                let frames = materials.unwrap_or([triangle.material; 3]).map(|material| {
                    terrain_texture_frame(&art, material, triangle.texture_tile)
                        .map(|frame| frame.uv)
                });
                let expected_blend = match frames {
                    [Some(a), Some(b), Some(c)] if a != b || a != c => Some([b, c]),
                    _ => None,
                };
                apply_terrain_textures(std::slice::from_mut(&mut triangle), &art);
                assert_eq!(
                    triangle.texture_uv.map(|uv| uv.map(f32::to_bits)),
                    frames[0].map(|uv| uv.map(f32::to_bits)),
                );
                assert_eq!(
                    triangle
                        .texture_blend
                        .map(|rects| rects.map(|uv| uv.map(f32::to_bits))),
                    expected_blend.map(|rects| rects.map(|uv| uv.map(f32::to_bits))),
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn fixed_height_conversion_preserves_old_array_map_bits() {
    let mut seed = 0x917a_2345_u32;
    for _ in 0..1_024 {
        let mut corners = [0_i16; 4];
        for corner in &mut corners {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *corner = seed as i16;
        }
        for triangulation in 0..3 {
            for (x, y) in [(0.0, -0.0), (0.25, 0.75), (0.75, 0.25), (f64::NAN, 0.5)] {
                let expected =
                    sample_float_surface_height(corners.map(f64::from), triangulation, x, y);
                assert_eq!(
                    sample_surface_height(corners, triangulation, x, y).to_bits(),
                    expected.to_bits(),
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn bounded_forest_accents_mix_with_dirt_and_missing_art_uses_dirt() {
    let frame = GameFrame {
        uv: [0.0, 0.0, 0.01, 0.01],
        size: [97.0, 49.0],
        anchor: [0.0, 0.0],
    };
    let mut art = test_art(frame);
    art.terrain[2] = vec![
        GameFrame {
            uv: [0.2, 0.0, 0.01, 0.01],
            ..frame
        };
        100
    ];
    art.terrain[6] = (0..10)
        .map(|index| GameFrame {
            uv: [0.5 + index as f32 / 100.0, 0.0, 0.01, 0.01],
            ..frame
        })
        .collect();
    let mut accents = 0;
    let mut dirt = 0;
    for y in -16..16 {
        for x in -16..16 {
            let selected = terrain_texture_frame(&art, 6, [x, y]).unwrap();
            accents += usize::from(selected.uv[0] >= 0.5);
            dirt += usize::from(selected.uv[0] == 0.2);
            assert_eq!(
                selected.uv,
                terrain_texture_frame(&art, 6, [x, y]).unwrap().uv
            );
        }
    }
    assert_eq!((accents, dirt), (512, 512));
    art.terrain[6].clear();
    assert_eq!(
        terrain_texture_frame(&art, 6, [8, 0]).unwrap().uv,
        terrain_texture_frame(&art, 2, [8, 0]).unwrap().uv
    );
}

fn map(side: i32) -> Vec<SceneTerrain> {
    (-2..side)
        .flat_map(|y| {
            (-2..side).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: if x < side / 2 { 0 } else { 2 },
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
            })
        })
        .collect()
}

fn camera(center: [f64; 2], zoom: f64, viewport: [f64; 2]) -> SceneCamera {
    SceneCamera {
        center,
        zoom,
        viewport,
        focus_elevation_meters: 0.0,
    }
}

#[wasm_bindgen_test]
fn material_vertices_are_shared_world_keyed_and_input_order_independent() {
    let terrain = map(8);
    let camera = camera([4.0, 4.0], 1.0, [640.0, 480.0]);
    let mut triangles = projected_surface_triangles(&terrain, camera);
    let mut vertices = BTreeMap::new();
    for triangle in &triangles {
        for (point, material) in triangle
            .points
            .into_iter()
            .zip(triangle.texture_materials.unwrap())
        {
            let key = [point.world[0] as i32, point.world[1] as i32];
            if let Some(previous) = vertices.insert(key, material) {
                assert_eq!(previous, material, "material seam at {key:?}");
            }
        }
    }
    let mut reversed = terrain;
    reversed.reverse();
    let other = projected_surface_triangles(&reversed, camera);
    for (left, right) in triangles.iter().zip(&other) {
        assert_eq!(left.texture_materials, right.texture_materials);
        assert_eq!(left.texture_tile, right.texture_tile);
    }
    let frame = GameFrame {
        uv: [0.0, 0.0, 0.01, 0.01],
        size: [97.0, 49.0],
        anchor: [48.0, 24.0],
    };
    let mut art = test_art(frame);
    art.terrain[2][0].uv[0] = 0.5;
    apply_terrain_textures(&mut triangles, &art);
    assert!(
        triangles
            .iter()
            .any(|triangle| triangle.texture_blend.is_some())
    );
    assert!(
        triangles
            .iter()
            .any(|triangle| triangle.texture_blend.is_none())
    );
    for triangle in triangles {
        let materials = triangle.texture_materials.unwrap();
        assert_eq!(
            triangle.texture_uv,
            terrain_texture_frame(&art, materials[0], triangle.texture_tile).map(|frame| frame.uv)
        );
    }
}

#[wasm_bindgen_test]
fn water_and_cliffs_keep_their_own_material_instead_of_land_splatting() {
    let mut terrain = map(4);
    for sample in &mut terrain {
        if sample.position == [1.5, 1.5] {
            sample.surface.water = 2;
            sample.material = 1;
        }
        if sample.position == [2.5, 1.5] {
            sample.surface.kind = SceneTerrainSurface::CLIFF;
            sample.material = 4;
        }
    }
    let mut triangles =
        projected_surface_triangles(&terrain, camera([2.0, 2.0], 1.0, [640.0, 480.0]));
    apply_terrain_textures(
        &mut triangles,
        &test_art(GameFrame {
            uv: [0.0, 0.0, 0.01, 0.01],
            size: [97.0, 49.0],
            anchor: [48.0, 24.0],
        }),
    );
    for triangle in triangles
        .iter()
        .filter(|triangle| triangle.tile == [1, 1] || triangle.tile == [2, 1])
    {
        assert!(triangle.texture_materials.is_none());
        assert!(triangle.texture_blend.is_none());
    }
    for triangle in triangles
        .iter()
        .filter(|triangle| triangle.texture_materials.is_some())
    {
        assert!(
            !triangle
                .texture_materials
                .unwrap()
                .iter()
                .any(|material| *material == 1 || *material == 4)
        );
    }
}

#[wasm_bindgen_test]
fn coarse_art_patches_reduce_stretch_without_changing_the_contact_planes_or_budget() {
    let terrain = map(256)
        .into_iter()
        .map(|mut sample| {
            let x = sample.position[0].floor() as i32;
            let y = sample.position[1].floor() as i32;
            sample.surface.corner_game_height_levels = [
                x.rem_euclid(9) as i16,
                (x + 1).rem_euclid(9) as i16,
                (x + y + 2).rem_euclid(9) as i16,
                (y + 1).rem_euclid(9) as i16,
            ];
            sample
        })
        .collect::<Vec<_>>();
    let triangles =
        projected_surface_triangles(&terrain, camera([128.0, 128.0], 0.25, [1280.0, 720.0]));
    assert!(triangles.len() <= MAX_SURFACE_TRIANGLES);
    let mut cells = BTreeMap::<_, Vec<_>>::new();
    let mut shared_heights = BTreeMap::new();
    for triangle in &triangles {
        cells.entry(triangle.tile).or_default().push(triangle);
        for point in triangle.points {
            let key = [point.world[0] as i32, point.world[1] as i32];
            if let Some(previous) = shared_heights.insert(key, point.world[2]) {
                assert!((previous - point.world[2]).abs() < 1e-10);
            }
        }
    }
    assert!(cells.len() <= MAX_SURFACE_TILES);
    assert!(
        triangles.len() > cells.len() * 2,
        "coarse cells still stretch just one tile"
    );
    for (tile, patches) in cells {
        let size = patches
            .iter()
            .flat_map(|triangle| triangle.points)
            .map(|point| point.world[0] - f64::from(tile[0]))
            .fold(0.0, f64::max);
        let heights = [
            shared_heights[&tile],
            shared_heights[&[tile[0] + size as i32, tile[1]]],
            shared_heights[&[tile[0] + size as i32, tile[1] + size as i32]],
            shared_heights[&[tile[0], tile[1] + size as i32]],
        ];
        for triangle in patches {
            let width = triangle
                .points
                .iter()
                .map(|point| point.world[0])
                .fold(f64::NEG_INFINITY, f64::max)
                - triangle
                    .points
                    .iter()
                    .map(|point| point.world[0])
                    .fold(f64::INFINITY, f64::min);
            assert!(width <= size * 0.5);
            let centroid = [0, 1, 2].map(|axis| {
                triangle
                    .points
                    .iter()
                    .map(|point| point.world[axis])
                    .sum::<f64>()
                    / 3.0
            });
            let expected = sample_float_surface_height(
                heights,
                0,
                (centroid[0] - f64::from(tile[0])) / size,
                (centroid[1] - f64::from(tile[1])) / size,
            );
            assert!(
                (expected - centroid[2]).abs() < 1e-10,
                "art subdivision changed contact height"
            );
        }
    }
}

#[wasm_bindgen_test]
fn art_subdivision_budget_holds_for_sparse_and_large_lod_inputs() {
    for cells in [1, 17, 1024, 3072, 4096] {
        for size in [1, 2, 4, 8, 16, 1024] {
            let divisions = appearance::texture_subdivisions(size, cells);
            assert!(divisions <= size && size % divisions == 0);
            assert!(cells * 2 * divisions as usize * divisions as usize <= MAX_SURFACE_TRIANGLES);
        }
    }
}
