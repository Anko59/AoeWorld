use super::*;
use wasm_bindgen::JsCast;

#[derive(Clone, Copy)]
enum ShoreEdge {
    East,
    South,
}

fn terrain(tile: [i32; 2], water: u8, height: i16) -> SceneTerrain {
    SceneTerrain {
        appearance: None,
        position: [f64::from(tile[0]) + 0.5, f64::from(tile[1]) + 0.5],
        material: if water == 0 { 3 } else { 5 },
        elevation_meters: f64::from(height),
        surface: SceneTerrainSurface {
            corner_game_height_levels: [height; 4],
            kind: SceneTerrainSurface::PLATEAU,
            triangulation: 0,
            water,
        },
    }
}

fn pair(
    edge: ShoreEdge,
    water_on_first: bool,
    height_first: i16,
    height_second: i16,
) -> [SceneTerrain; 2] {
    let (first, second) = match edge {
        ShoreEdge::East => ([0, 0], [1, 0]),
        ShoreEdge::South => ([0, 0], [0, 1]),
    };
    [
        terrain(first, u8::from(water_on_first), height_first),
        terrain(second, u8::from(!water_on_first), height_second),
    ]
}

fn camera() -> SceneCamera {
    SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 2.0,
    }
}

#[wasm_bindgen_test]
fn shoreline_skirts_close_unequal_east_and_south_edges_with_water_on_either_side() {
    let art = test_art(GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.4, 0.5, 0.03, 0.02],
        },
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    });
    for edge in [ShoreEdge::East, ShoreEdge::South] {
        for water_on_first in [false, true] {
            let samples = pair(edge, water_on_first, 4, 0);
            let land = samples
                .iter()
                .copied()
                .find(|sample| sample.surface.water == 0)
                .unwrap();
            let mut triangles = projected_surface_triangles(&samples, camera());
            apply_terrain_textures(&mut triangles, &art);
            let skirts = triangles
                .iter()
                .filter(|triangle| triangle.skirt)
                .collect::<Vec<_>>();
            assert_eq!(skirts.len(), 2, "edge/water side: {water_on_first}");
            assert!(skirts.iter().all(|triangle| {
                !triangle.pickable
                    && triangle.material == 4
                    && triangle.tint == 7
                    && triangle.texture_uv.is_some()
                    && triangle.color == darken(surface_color(land), 0.62)
            }));
            let points = skirts.iter().flat_map(|triangle| triangle.points);
            let mut heights = Vec::new();
            for point in points {
                match edge {
                    ShoreEdge::East => assert_eq!(point.world[0], 1.0),
                    ShoreEdge::South => assert_eq!(point.world[1], 1.0),
                }
                heights.push(point.world[2]);
            }
            assert!(heights.contains(&0.0));
            assert!(heights.contains(&4.0));
        }
    }
}

#[wasm_bindgen_test]
fn coplanar_shore_edges_remain_skirt_free_for_both_directions_and_sides() {
    for edge in [ShoreEdge::East, ShoreEdge::South] {
        for water_on_first in [false, true] {
            let samples = pair(edge, water_on_first, 2, 2);
            let triangles = projected_surface_triangles(&samples, camera());
            assert_eq!(triangles.len(), 4);
            assert!(triangles.iter().all(|triangle| !triangle.skirt));
        }
    }
}

#[wasm_bindgen_test]
fn one_quantized_height_step_at_a_beach_does_not_create_vertical_walls() {
    for edge in [ShoreEdge::East, ShoreEdge::South] {
        for water_on_first in [false, true] {
            for heights in [[2, 3], [3, 2]] {
                let samples = pair(edge, water_on_first, heights[0], heights[1]);
                let faces = projected_surface_triangles(&samples, camera());
                assert!(
                    faces.iter().all(|face| !face.skirt),
                    "a one-level shore mismatch is a gentle beach, heights {heights:?}"
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn a_real_beach_drop_keeps_skirts_textured_with_the_upper_sand_bank() {
    let art = mix::fixture::art();
    let sand = terrain_texture_frame(&art, 3, [0, 0]).unwrap().atlas;
    for edge in [ShoreEdge::East, ShoreEdge::South] {
        for water_on_first in [false, true] {
            let heights = if water_on_first { [0, 4] } else { [4, 0] };
            let samples = pair(edge, water_on_first, heights[0], heights[1]);
            let mut faces = projected_surface_triangles(&samples, camera());
            apply_terrain_textures(&mut faces, &art);
            let skirts = faces.iter().filter(|face| face.skirt).collect::<Vec<_>>();
            assert_eq!(skirts.len(), 2, "a real height drop must stay closed");
            for face in skirts {
                assert!(!face.pickable);
                assert_eq!(face.material, 3, "upper sand bank owns the side face");
                assert_eq!(face.texture_uv, Some(sand));
                assert_eq!(face.tint, 3, "sand bank shading, without a rock kernel");
                assert!(face.points.iter().any(|point| point.world[2] == 4.0));
                assert!(face.points.iter().any(|point| point.world[2] == 0.0));
            }
        }
    }
}

#[wasm_bindgen_test]
fn shoreline_appearance_keeps_source_water_ownership_picking_and_inland_vertex_count() {
    let mut samples = mix::fixture::shore_terrain();
    let source = samples
        .iter()
        .map(|sample| sample.surface)
        .collect::<Vec<_>>();
    let view = mix::fixture::camera(0);
    let projection = Camera {
        center: view.center,
        zoom: view.zoom,
        viewport: view.viewport,
        focus_elevation_meters: view.focus_elevation_meters,
    };
    let mut faces = projected_surface_triangles(&samples, view);
    apply_terrain_textures(&mut faces, &mix::fixture::art());
    assert_eq!(
        source,
        samples
            .iter()
            .map(|sample| sample.surface)
            .collect::<Vec<_>>()
    );
    for world in [[-0.25, 0.25], [0.25, 0.25]] {
        let picked = pick_surface_point(&faces, projection.world_to_screen_at_height(world, 0.0))
            .expect("shore appearance retains the existing pickable surface");
        assert!((picked[0] - world[0]).abs() < 1e-7);
        assert!((picked[1] - world[1]).abs() < 1e-7);
    }
    for sample in &mut samples {
        sample.material = 3;
        sample.surface.water = 0;
    }
    // A flat, fully visible 4x4 inland patch still has two triangles per tile.
    samples.retain(|sample| {
        (-2.0..2.0).contains(&sample.position[0]) && (-2.0..2.0).contains(&sample.position[1])
    });
    let inland = projected_surface_triangles(
        &samples,
        SceneCamera {
            viewport: [512.0; 2],
            ..view
        },
    );
    assert_eq!(inland.len() * 3, 16 * 6);
    assert!(inland.iter().all(|face| !face.skirt));
}

#[wasm_bindgen_test]
fn modeled_water_steps_use_the_shared_water_material_and_are_not_pickable() {
    let samples = [terrain([0, 0], 1, 4), terrain([1, 0], 2, 0)];
    let mut triangles = projected_surface_triangles(&samples, camera());
    apply_terrain_textures(
        &mut triangles,
        &test_art(GameFrame {
            atlas: crate::AtlasAddress {
                page: 0,
                uv: [0.4, 0.5, 0.03, 0.02],
            },
            size: [96.0, 48.0],
            anchor: [48.0, 24.0],
        }),
    );
    let skirts = triangles
        .iter()
        .filter(|triangle| triangle.skirt)
        .collect::<Vec<_>>();
    assert_eq!(skirts.len(), 2);
    assert!(skirts.iter().all(|triangle| {
        !triangle.pickable
            && triangle.material == 5
            && triangle.tint == 0
            && triangle.texture_uv.is_some()
    }));
}

#[wasm_bindgen_test]
fn alpha_bearing_diamond_fills_shore_skirts_without_transparent_uv_corners() {
    let document = web_sys::window().unwrap().document().unwrap();
    let atlas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    atlas.set_width(GAME_ATLAS_SIDE);
    atlas.set_height(GAME_ATLAS_SIDE);
    let atlas_context: web_sys::CanvasRenderingContext2d = atlas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    atlas_context.set_fill_style_str("#ffffff");
    atlas_context.begin_path();
    atlas_context.move_to(48.0, 0.0);
    atlas_context.line_to(96.0, 24.0);
    atlas_context.line_to(48.0, 48.0);
    atlas_context.line_to(0.0, 24.0);
    atlas_context.close_path();
    atlas_context.fill();

    let art = test_art(GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [
                0.0,
                0.0,
                96.0 / GAME_ATLAS_SIDE as f32,
                48.0 / GAME_ATLAS_SIDE as f32,
            ],
        },
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    });
    let mut test_camera = camera();
    test_camera.viewport = [256.0, 256.0];
    let mut triangles =
        projected_surface_triangles(&pair(ShoreEdge::East, false, 4, 0), test_camera);
    apply_terrain_textures(&mut triangles, &art);
    let skirts = triangles
        .iter()
        .filter(|triangle| triangle.skirt)
        .collect::<Vec<_>>();
    assert_eq!(skirts.len(), 2);

    let atlases: [web_sys::HtmlCanvasElement; 5] = std::array::from_fn(|_| atlas.clone());
    for triangle in skirts {
        assert!(matches!(triangle.texture_mode, 4 | 5));
        let canvas: web_sys::HtmlCanvasElement = document
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        canvas.set_width(256);
        canvas.set_height(256);
        let context: web_sys::CanvasRenderingContext2d = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into()
            .unwrap();
        draw_surface_triangle(&context, &atlases, triangle).unwrap();

        // Near the first corner, old full-rectangle UVs sampled the transparent
        // atlas corner. The inset UV rectangle stays inside the opaque diamond.
        let [first, second, third] = triangle.points.map(|point| point.screen);
        let x = (first.x * 0.9 + second.x * 0.05 + third.x * 0.05).floor();
        let y = (first.y * 0.9 + second.y * 0.05 + third.y * 0.05).floor();
        let pixel = context.get_image_data(x, y, 1.0, 1.0).unwrap();
        assert!(
            pixel.data().0[3] > 0,
            "transparent shore skirt pixel for mode {} at {x},{y}",
            triangle.texture_mode
        );
    }
}

#[wasm_bindgen_test]
fn a_one_tile_shallow_band_still_blends_into_sand_with_deeper_water_behind_it() {
    for deeper in [5, 11] {
        let mut samples = mix::fixture::shore_terrain();
        for sample in &mut samples {
            if sample.position[0] >= 1.0 {
                sample.material = deeper;
            }
        }
        let mut faces = projected_surface_triangles(&samples, mix::fixture::camera(0));
        apply_terrain_textures(&mut faces, &mix::fixture::art());
        let shallow = faces
            .iter()
            .filter(|face| !face.skirt && face.tile == [0, 0])
            .collect::<Vec<_>>();
        assert_eq!(shallow.len(), 2);
        for face in shallow {
            assert!(
                face.texture_materials.unwrap().contains(&3),
                "the loaded deeper-water halo must not remove sand from the shallow shoreline: deeper {deeper}, layers {:?}",
                face.texture_materials
            );
        }
    }
}

#[wasm_bindgen_test]
fn a_dry_triangle_beside_a_shore_corner_keeps_only_its_ground_sheet() {
    // A stepped coast like Geneva: the opposite quad corner sees water, but
    // this triangle's three incident supports are all sand. Its zero shares
    // must not fall back to a barycentric water layer at one arbitrary vertex.
    for overlay in [0, 10] {
        for diagonal in [0, 1] {
            let mut samples = mix::fixture::shore_terrain();
            for sample in &mut samples {
                let wet = if diagonal == 0 {
                    sample.position[0] - sample.position[1] >= 2.0
                } else {
                    sample.position[0] + sample.position[1] >= 2.0
                };
                sample.material = if wet { overlay } else { 3 };
                sample.surface.water = u8::from(wet && overlay == 10);
                sample.surface.triangulation = diagonal;
            }
            let mut view = mix::fixture::camera(0);
            view.viewport = [512.0; 2];
            let mut faces = projected_surface_triangles(&samples, view);
            let dry = faces
                .iter()
                .enumerate()
                .filter(|(_, face)| {
                    face.material == 3
                        && face.points.iter().all(|point| {
                            samples
                                .iter()
                                .filter(|sample| {
                                    (sample.position[0] - point.world[0]).abs() < 1.0
                                        && (sample.position[1] - point.world[1]).abs() < 1.0
                                })
                                .all(|sample| sample.material == 3)
                        })
                        && face.tile[0].abs() < 3
                        && face.tile[1].abs() < 3
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert!(
                !dry.is_empty(),
                "fixture must include a dry corner triangle"
            );
            apply_terrain_textures(&mut faces, &mix::fixture::art());
            for index in dry {
                assert!(
                    faces[index].texture_blend.is_none(),
                    "zero coverage must stay pure sand beside material {overlay}, diagonal {diagonal}: {:?}",
                    faces[index].texture_materials
                );
            }
        }
    }
}

#[wasm_bindgen_test]
fn stepped_shallow_shores_share_their_water_sheet_with_adjoining_depth_faces() {
    for direction in [-1, 1] {
        for deeper in [5, 11] {
            let mut samples = mix::fixture::shore_terrain();
            for sample in &mut samples {
                let x = (sample.position[0] - 0.5) as i32;
                let y = (sample.position[1] - 0.5) as i32;
                let band = x + direction * y;
                sample.material = if band < 0 {
                    3
                } else if band < 2 {
                    10
                } else {
                    deeper
                };
                sample.surface.water = u8::from(band >= 0);
            }
            let mut view = mix::fixture::camera(0);
            view.viewport = [512.0; 2];
            let mut art = mix::fixture::art();
            art.water_depth[0].0 = art.terrain[0].clone();
            art.water_depth[1].0 = art.terrain[1].clone();
            let mut faces = projected_surface_triangles(&samples, view);
            apply_terrain_textures(&mut faces, &art);
            let mut pure_shore_vertices = std::collections::BTreeSet::new();
            for face in &faces {
                if (face.appearance >> 1) & 7 == 6 {
                    for (point, coverage) in face.points.iter().zip(face.floor_strengths.unwrap()) {
                        if coverage == 255 {
                            pure_shore_vertices
                                .insert((point.world[0] as i32, point.world[1] as i32));
                        }
                    }
                }
            }
            let mut shared = 0;
            for face in &faces {
                if face.skirt
                    || face.floor_strengths.is_some()
                    || !matches!(face.material, 5 | 10 | 11)
                {
                    continue;
                }
                for (corner, point) in face.points.iter().enumerate() {
                    if pure_shore_vertices.contains(&(point.world[0] as i32, point.world[1] as i32))
                    {
                        shared += 1;
                        assert_eq!(
                            face.texture_materials.unwrap()[corner],
                            10,
                            "shore/depth seam at {:?}, direction {direction}, depth {deeper}",
                            point.world
                        );
                    }
                }
            }
            assert!(shared > 0, "fixture must cover the shore/depth join");
            assert!(
                faces.iter().any(|face| !face.skirt
                    && face.material == deeper
                    && face.texture_materials == Some([deeper; 3])),
                "interior depth art stays intact"
            );
        }
    }
}
