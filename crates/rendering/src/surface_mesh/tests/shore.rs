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
