use super::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn test_art(frame: GameFrame) -> GameArt {
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame],
        terrain: std::array::from_fn(|_| vec![frame]),
        resources: std::array::from_fn(|_| Vec::new()),
    }
}

#[wasm_bindgen_test]
fn textured_terrain_uses_shared_corner_uvs_and_material_atlas_groups() {
    let frame = |uv| GameFrame {
        uv,
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    };
    let mut art = test_art(frame([0.0, 0.0, 0.05, 0.05]));
    art.terrain[3] = vec![frame([0.2, 0.3, 0.04, 0.06])];
    art.terrain[4] = vec![frame([0.4, 0.5, 0.03, 0.02])];
    let sample = SceneTerrain {
        position: [0.5, 0.5],
        material: 3,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [0, 1, 2, 3],
            kind: SceneTerrainSurface::RAMP,
            triangulation: 0,
            water: 1,
        },
    };
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let mut triangles = projected_surface_triangles(&[sample], camera);
    apply_terrain_textures(&mut triangles, &art);
    assert_eq!(triangles.len(), 2);
    assert!(triangles.iter().all(|triangle| {
        triangle.texture_uv == Some([0.2, 0.3, 0.04, 0.06]) && triangle.tint == 4
    }));
    assert_eq!(
        triangle_texture_coordinates(0),
        [[0.5, 0.0], [1.0, 0.5], [0.5, 1.0]]
    );
    assert_eq!(
        triangle_texture_coordinates(1),
        [[0.5, 0.0], [0.5, 1.0], [0.0, 0.5]]
    );
    assert_eq!(
        triangle_texture_coordinates(2),
        [[0.5, 0.0], [1.0, 0.5], [0.0, 0.5]]
    );
    assert_eq!(
        triangle_texture_coordinates(3),
        [[1.0, 0.5], [0.5, 1.0], [0.0, 0.5]]
    );
    assert_eq!(
        triangle_texture_coordinates(4),
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
    );
    assert_eq!(
        triangle_texture_coordinates(5),
        [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    );
}

#[wasm_bindgen_test]
fn flat_zero_height_ground_is_textured_contiguous_and_pickable() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let sample = SceneTerrain {
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let mut triangles = projected_surface_triangles(&[sample], camera);
    apply_terrain_textures(
        &mut triangles,
        &test_art(GameFrame {
            uv: [0.0, 0.0, 0.1, 0.1],
            size: [96.0, 48.0],
            anchor: [48.0, 24.0],
        }),
    );
    assert_eq!(triangles.len(), 2);
    assert!(triangles.iter().all(|triangle| {
        !triangle.skirt
            && triangle.pickable
            && triangle.tint == 0
            && triangle.texture_uv.is_some()
            && triangle.points.iter().all(|point| point.world[2] == 0.0)
    }));
    assert!(shares_world_vertices(&triangles[0], &triangles[1], 2));
    assert_eq!(
        pick_surface_point(&triangles, ScreenPoint { x: 128.0, y: 64.0 }),
        Some([0.5, 0.5])
    );
}

#[wasm_bindgen_test]
fn both_ramp_orientations_keep_matching_uvs_on_the_shared_diagonal() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    for triangulation in 0..=1_u8 {
        let sample = SceneTerrain {
            position: [0.5, 0.5],
            material: 1,
            elevation_meters: 1.5,
            surface: SceneTerrainSurface {
                corner_game_height_levels: [0, 1, 3, 2],
                kind: SceneTerrainSurface::RAMP,
                triangulation,
                water: 0,
            },
        };
        let mut triangles = projected_surface_triangles(&[sample], camera);
        apply_terrain_textures(
            &mut triangles,
            &test_art(GameFrame {
                uv: [0.2, 0.3, 0.05, 0.06],
                size: [96.0, 48.0],
                anchor: [48.0, 24.0],
            }),
        );
        triangles.sort_unstable_by_key(|triangle| triangle.order);
        assert_eq!(triangles[0].texture_mode, triangulation * 2);
        assert_eq!(triangles[1].texture_mode, triangulation * 2 + 1);
        let shared: &[[f64; 2]] = if triangulation == 0 {
            &[[0.0, 0.0], [1.0, 1.0]]
        } else {
            &[[1.0, 0.0], [0.0, 1.0]]
        };
        for world in shared {
            let first = uv_at_world(&triangles[0], *world).unwrap();
            let second = uv_at_world(&triangles[1], *world).unwrap();
            assert_eq!(first, second);
        }
        assert!(triangles.iter().all(|triangle| triangle.tint == 1));
    }
}

fn shares_world_vertices(
    left: &ProjectedSurfaceTriangle,
    right: &ProjectedSurfaceTriangle,
    count: usize,
) -> bool {
    left.points
        .iter()
        .filter(|left_point| {
            right.points.iter().any(|right_point| {
                left_point.world[..2]
                    .iter()
                    .zip(right_point.world)
                    .all(|(axis, value)| (axis - value).abs() < 1e-9)
            })
        })
        .count()
        >= count
}

fn uv_at_world(triangle: &ProjectedSurfaceTriangle, world: [f64; 2]) -> Option<[f64; 2]> {
    let index = triangle.points.iter().position(|point| {
        (point.world[0] - world[0]).abs() < 1e-9 && (point.world[1] - world[1]).abs() < 1e-9
    })?;
    Some(triangle_texture_coordinates(triangle.texture_mode)[index])
}

#[wasm_bindgen_test]
fn webgpu_terrain_shader_contains_the_native_diamond_uv_vertices() {
    let shader = include_str!("../sprites.wgsl");
    for mode in 0..=3 {
        for [u, v] in triangle_texture_coordinates(mode) {
            let coordinate = format!("vec2<f32>({u:.1}, {v:.1})");
            assert!(
                shader.contains(&coordinate),
                "missing native terrain UV {coordinate} in WebGPU shader"
            );
        }
    }
}

#[wasm_bindgen_test]
fn canvas_texture_affine_mapping_hits_all_projected_triangle_corners() {
    let points = [
        SurfacePoint {
            world: [0.0; 3],
            screen: ScreenPoint { x: 14.0, y: 26.0 },
        },
        SurfacePoint {
            world: [0.0; 3],
            screen: ScreenPoint { x: 82.0, y: 18.0 },
        },
        SurfacePoint {
            world: [0.0; 3],
            screen: ScreenPoint { x: 67.0, y: 73.0 },
        },
    ];
    for mode in 0..=5 {
        let uv = triangle_texture_coordinates(mode);
        let transform = texture_transform(points, uv);
        for (coordinate, point) in uv.into_iter().zip(points) {
            let projected = [
                transform[0] * coordinate[0] + transform[2] * coordinate[1] + transform[4],
                transform[1] * coordinate[0] + transform[3] * coordinate[1] + transform[5],
            ];
            assert!((projected[0] - point.screen.x).abs() < 1e-9);
            assert!((projected[1] - point.screen.y).abs() < 1e-9);
        }
    }
}

#[wasm_bindgen_test]
fn canvas_keeps_an_alpha_bearing_native_terrain_diamond_aligned_to_the_tile() {
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

    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_width(256);
    canvas.set_height(128);
    let context: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let art = test_art(GameFrame {
        uv: [
            0.0,
            0.0,
            96.0 / GAME_ATLAS_SIDE as f32,
            48.0 / GAME_ATLAS_SIDE as f32,
        ],
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    });
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let mut triangles = projected_surface_triangles(
        &[SceneTerrain {
            position: [0.5, 0.5],
            material: 0,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
        }],
        camera,
    );
    apply_terrain_textures(&mut triangles, &art);
    let atlases: [web_sys::HtmlCanvasElement; 5] = std::array::from_fn(|_| atlas.clone());
    for triangle in &triangles {
        draw_surface_triangle(&context, &atlases, triangle).unwrap();
    }

    for (x, y) in [(128, 34), (180, 60), (128, 94), (76, 64), (128, 64)] {
        let pixel = context
            .get_image_data(f64::from(x), f64::from(y), 1.0, 1.0)
            .unwrap();
        assert!(pixel.data().0[3] > 0, "transparent tile edge at {x},{y}");
    }
    let outside = context.get_image_data(128.0, 30.0, 1.0, 1.0).unwrap();
    assert_eq!(outside.data().0[3], 0);
}

#[wasm_bindgen_test]
fn cliff_adjacency_creates_a_textured_nonpickable_height_transition() {
    let camera = SceneCamera {
        center: [1.0, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 2.0,
    };
    let cliff = |position: [f64; 2], heights| SceneTerrain {
        position,
        material: 4,
        elevation_meters: 2.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: heights,
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let mut triangles = projected_surface_triangles(
        &[
            cliff([0.5, 0.5], [4, 4, 4, 4]),
            cliff([1.5, 0.5], [0, 0, 0, 0]),
        ],
        camera,
    );
    apply_terrain_textures(
        &mut triangles,
        &test_art(GameFrame {
            uv: [0.4, 0.5, 0.03, 0.02],
            size: [96.0, 48.0],
            anchor: [48.0, 24.0],
        }),
    );
    let skirts = triangles
        .iter()
        .filter(|triangle| triangle.skirt)
        .collect::<Vec<_>>();
    assert_eq!(triangles.len(), 6);
    assert_eq!(skirts.len(), 2);
    assert!(skirts.iter().all(|triangle| {
        !triangle.pickable
            && triangle.tint == 3
            && triangle.material == 4
            && triangle.texture_uv.is_some()
            && triangle
                .points
                .iter()
                .all(|point| point.world[0] == 1.0 && (0.0..=4.0).contains(&point.world[2]))
    }));
    assert!(
        skirts
            .iter()
            .flat_map(|triangle| triangle.points)
            .any(|point| point.world[2] == 4.0)
    );
    assert!(
        skirts
            .iter()
            .flat_map(|triangle| triangle.points)
            .any(|point| point.world[2] == 0.0)
    );
}

#[wasm_bindgen_test]
fn adjacent_chunks_share_boundary_geometry_and_deterministic_texture_placement() {
    let camera = SceneCamera {
        center: [1.0, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let tile = |x| SceneTerrain {
        position: [x, 0.5],
        material: 3,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let mut left = projected_surface_triangles(&[tile(0.5)], camera);
    let mut right = projected_surface_triangles(&[tile(1.5)], camera);
    let art = test_art(GameFrame {
        uv: [0.1, 0.2, 0.07, 0.08],
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    });
    apply_terrain_textures(&mut left, &art);
    apply_terrain_textures(&mut right, &art);
    assert_eq!(left[0].texture_uv, right[0].texture_uv);
    for boundary in [[1.0, 0.0], [1.0, 1.0]] {
        assert!(
            left.iter()
                .any(|triangle| uv_at_world(triangle, boundary).is_some())
        );
        assert!(
            right
                .iter()
                .any(|triangle| uv_at_world(triangle, boundary).is_some())
        );
    }
    assert_eq!(left.iter().filter(|triangle| triangle.skirt).count(), 0);
    assert_eq!(right.iter().filter(|triangle| triangle.skirt).count(), 0);
}

#[wasm_bindgen_test]
fn water_beside_raised_land_shares_a_shore_edge_without_a_cliff_skirt() {
    let camera = SceneCamera {
        center: [1.0, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 2.0,
    };
    let tile = |x, water| SceneTerrain {
        position: [x, 0.5],
        material: if water == 0 { 3 } else { 5 },
        elevation_meters: 2.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [2, 2, 2, 2],
            kind: SceneTerrainSurface::PLATEAU,
            triangulation: 0,
            water,
        },
    };
    let mut triangles = projected_surface_triangles(&[tile(0.5, 0), tile(1.5, 1)], camera);
    apply_terrain_textures(
        &mut triangles,
        &test_art(GameFrame {
            uv: [0.7, 0.1, 0.04, 0.04],
            size: [96.0, 48.0],
            anchor: [48.0, 24.0],
        }),
    );
    assert_eq!(triangles.len(), 4);
    assert!(triangles.iter().all(|triangle| !triangle.skirt));
    assert_eq!(
        triangles
            .iter()
            .filter(|triangle| triangle.tint == 4)
            .count(),
        2
    );
    for shore in [[1.0, 0.0, 2.0], [1.0, 1.0, 2.0]] {
        for tile_x in 0..=1 {
            assert!(
                triangles
                    .iter()
                    .filter(|triangle| triangle.tile == [tile_x, 0])
                    .any(|triangle| triangle.points.iter().any(|point| point.world == shore))
            );
        }
    }
}

#[wasm_bindgen_test]
fn pretinted_canvas_atlases_match_webgpu_rgb_formulas_and_preserve_alpha() {
    let source = [100_u8, 150, 200, 128, 17, 83, 241, 0];
    for tint in 0..=4_u8 {
        let tinted = tint_atlas_pixels(&source, tint).unwrap();
        assert_eq!(tinted.len(), source.len());
        for (index, texel) in tinted.chunks_exact(4).enumerate() {
            let input = &source[index * 4..index * 4 + 4];
            for channel in 0..3 {
                let expected = match tint {
                    1 => f32::from(input[channel]) * 0.92,
                    2 => f32::from(input[channel]) * 0.78,
                    3 => f32::from(input[channel]) * 0.72,
                    4 => {
                        f32::from(input[channel]) * 0.86
                            + f32::from([38_u8, 113, 190][channel]) * 0.14
                    }
                    _ => f32::from(input[channel]),
                };
                assert!((f32::from(texel[channel]) - expected).abs() <= 0.5);
            }
            assert_eq!(texel[3], input[3]);
        }
    }
    assert_eq!(tint_atlas_pixels(&[0_u8; 3], 0), None);
}
