use super::*;
use crate::{
    game_grid,
    surface_mesh::{
        MAX_SURFACE_TILES, SurfacePoint, apply_terrain_textures, barycentric, pick_surface_point,
        projected_surface_triangles, surface_depth_at,
    },
};
use aoe_core::ScreenPoint;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn synthetic_art() -> GameArt {
    let frame = GameFrame {
        uv: [0.0, 0.0, 0.1, 0.1],
        size: [32.0, 48.0],
        anchor: [16.0, 48.0],
    };
    GameArt {
        walking: vec![frame; 80],
        standing: vec![frame; 80],
        grass: vec![frame],
        terrain: std::array::from_fn(|_| vec![frame]),
        resources: std::array::from_fn(|_| vec![frame]),
    }
}

#[wasm_bindgen_test]
fn frontmost_surface_depth_includes_elevation_and_cliff_faces() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let ground = SceneTerrain {
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let high = SceneTerrain {
        position: [1.5, 1.5],
        material: 4,
        elevation_meters: 2.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [2, 2, 2, 2],
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles = projected_surface_triangles(&[ground, high], camera);
    let screen = ScreenPoint { x: 128.0, y: 32.0 };
    assert_eq!(surface_depth_at(&triangles, screen), Some(7.0));
    assert_eq!(pick_surface_point(&triangles, screen), None);
}

#[wasm_bindgen_test]
fn pickable_elevated_plateau_wins_over_overlapping_lower_ground() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let tile = |position, heights: [i16; 4]| SceneTerrain {
        position,
        material: 0,
        elevation_meters: f64::from(heights[0]),
        surface: SceneTerrainSurface {
            corner_game_height_levels: heights,
            kind: SceneTerrainSurface::PLATEAU,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles = projected_surface_triangles(
        &[tile([0.5, 0.5], [0; 4]), tile([1.5, 1.5], [1; 4])],
        camera,
    );
    let screen_center = |tile| {
        let mut count = 0_usize;
        let (mut x, mut y) = (0.0, 0.0);
        for triangle in triangles.iter().filter(|triangle| triangle.tile == tile) {
            for point in triangle.points {
                x += point.screen.x;
                y += point.screen.y;
                count += 1;
            }
        }
        ScreenPoint {
            x: x / count as f64,
            y: y / count as f64,
        }
    };
    let lower_center = screen_center([0, 0]);
    let upper_center = screen_center([1, 1]);
    let screen = ScreenPoint {
        x: (lower_center.x + upper_center.x) * 0.5,
        y: (lower_center.y + upper_center.y) * 0.5,
    };
    assert!(
        triangles
            .iter()
            .filter(|triangle| triangle.tile == [0, 0])
            .any(|triangle| barycentric(triangle.points, screen).is_some())
    );
    assert!(
        triangles
            .iter()
            .filter(|triangle| triangle.tile == [1, 1])
            .any(|triangle| barycentric(triangle.points, screen).is_some())
    );
    assert_eq!(pick_surface_point(&triangles, screen), Some([1.25, 1.25]));
}

#[wasm_bindgen_test]
fn pickable_ground_wins_an_exact_tie_against_an_unpickable_skirt() {
    let screen = ScreenPoint { x: 96.0, y: 48.0 };
    let points = [
        SurfacePoint {
            world: [4.0, 8.0, 0.0],
            screen: ScreenPoint { x: 32.0, y: 16.0 },
        },
        SurfacePoint {
            world: [4.0, 8.0, 0.0],
            screen: ScreenPoint { x: 160.0, y: 16.0 },
        },
        SurfacePoint {
            world: [4.0, 8.0, 0.0],
            screen: ScreenPoint { x: 96.0, y: 80.0 },
        },
    ];
    let triangle = |skirt, pickable| ProjectedSurfaceTriangle {
        points,
        color: [0.2, 0.3, 0.4],
        tile: [7, 11],
        skirt,
        material: 0,
        texture_mode: 4,
        tint: 0,
        texture_uv: None,
        pickable,
        order: 0,
    };
    let triangles = [triangle(true, false), triangle(false, true)];

    assert_eq!(pick_surface_point(&triangles, screen), Some([4.0, 8.0]));
    assert_eq!(surface_depth_at(&triangles, screen), Some(12.0));
}

#[wasm_bindgen_test]
fn an_overlapping_cliff_is_drawn_over_a_lower_depth_selection_marker() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let unit = SceneUnit {
        id: EntityId(7),
        position: [0.5, 0.5],
        moving: false,
        facing: 0,
        selected: true,
        elevation_meters: 0.0,
    };
    let (marker, depth) =
        game_grid::selection_ring(camera, unit.position, unit.elevation_meters)[0];
    let screen = ScreenPoint {
        x: (f64::from(marker.position[0]) + 1.0) * camera.viewport[0] * 0.5,
        y: (1.0 - f64::from(marker.position[1])) * camera.viewport[1] * 0.5,
    };
    let cliff = ProjectedSurfaceTriangle {
        points: [
            SurfacePoint {
                world: [0.0, 0.0, 8.0],
                screen: ScreenPoint {
                    x: screen.x - 12.0,
                    y: screen.y - 10.0,
                },
            },
            SurfacePoint {
                world: [0.0, 0.0, 8.0],
                screen: ScreenPoint {
                    x: screen.x + 12.0,
                    y: screen.y - 10.0,
                },
            },
            SurfacePoint {
                world: [0.0, 0.0, 8.0],
                screen: ScreenPoint {
                    x: screen.x,
                    y: screen.y + 14.0,
                },
            },
        ],
        color: [0.2, 0.2, 0.2],
        tile: [0, 0],
        skirt: true,
        material: 4,
        texture_mode: 4,
        tint: 3,
        texture_uv: None,
        pickable: false,
        order: 2,
    };
    assert!(depth < triangle_depth(&cliff));
    assert!(barycentric(cliff.points, screen).is_some());

    let layers = ordered_world_layers(vec![cliff], Vec::new(), &[unit], camera);
    let marker_index = layers
        .iter()
        .position(|layer| {
            matches!(layer, WorldLayer::Selection(sprite, _) if sprite.position == marker.position)
        })
        .unwrap();
    let cliff_index = layers
        .iter()
        .position(|layer| matches!(layer, WorldLayer::Surface(triangle) if triangle.skirt))
        .unwrap();
    assert!(marker_index < cliff_index);
}

#[wasm_bindgen_test]
fn units_order_behind_and_in_front_of_a_raised_surface_at_contact_height() {
    let camera = SceneCamera {
        center: [1.5, 1.5],
        zoom: 1.0,
        viewport: [256.0, 256.0],
        focus_elevation_meters: 2.0,
    };
    let raised = SceneTerrain {
        position: [1.5, 1.5],
        material: 4,
        elevation_meters: 4.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [4; 4],
            kind: SceneTerrainSurface::PLATEAU,
            triangulation: 0,
            water: 0,
        },
    };
    let unit = |id, elevation_meters| SceneUnit {
        id: EntityId(id),
        position: [1.8, 1.8],
        moving: false,
        facing: 0,
        selected: false,
        elevation_meters,
    };
    let mut surfaces = projected_surface_triangles(&[raised], camera);
    apply_terrain_textures(&mut surfaces, &synthetic_art());
    let objects = world_sprite_frames(
        &synthetic_art(),
        &[raised],
        &[],
        &[unit(1, 0.0), unit(2, 4.0)],
        camera,
        0,
    );
    assert_eq!(objects.len(), 2);
    let layers = ordered_world_layers(surfaces, objects, &[], camera);
    let surface_index = layers
        .iter()
        .position(|layer| matches!(layer, WorldLayer::Surface(_)))
        .unwrap();
    let sprite_indices = layers
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| matches!(layer, WorldLayer::Sprite(_, _, _)).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(sprite_indices.len(), 2);
    assert!(sprite_indices[0] < surface_index);
    assert!(sprite_indices[1] > surface_index);
}

#[wasm_bindgen_test]
fn depleted_resource_disappears_from_both_backend_draw_lists() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let resource = SceneResource {
        id: 9,
        position: [0.5, 0.5],
        kind: 2,
        visual_variant: 0,
        elevation_meters: 0.0,
    };
    let terrain = SceneTerrain {
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let art = synthetic_art();
    let populated = world_sprite_frames(&art, &[terrain], &[resource], &[], camera, 0);
    assert_eq!(populated.len(), 1);
    let depleted = world_sprite_frames(&art, &[terrain], &[], &[], camera, 0);
    assert!(depleted.is_empty());
    let mut surfaces = projected_surface_triangles(&[terrain], camera);
    apply_terrain_textures(&mut surfaces, &art);
    let layers = ordered_world_layers(surfaces, depleted, &[], camera);
    let instances = layers
        .iter()
        .map(|layer| match layer {
            WorldLayer::Surface(triangle) => {
                crate::web::surface_instance(triangle, camera.viewport, 0.0)
            }
            WorldLayer::Selection(sprite, _) | WorldLayer::Sprite(sprite, _, _) => *sprite,
        })
        .collect::<Vec<_>>();
    assert_eq!(instances.len(), layers.len());
    assert_eq!(
        instances
            .iter()
            .filter(|sprite| sprite.color[3] >= 0.0)
            .count(),
        0
    );
}

#[wasm_bindgen_test]
fn zoomed_out_covering_lod_reaches_every_viewport_border() {
    let camera = SceneCamera {
        center: [128.0, 128.0],
        zoom: 0.25,
        viewport: [1_280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let terrain = (0..256)
        .flat_map(|y| {
            (0..256).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
            })
        })
        .collect::<Vec<_>>();
    let triangles = projected_surface_triangles(&terrain, camera);
    let tiles = triangles
        .iter()
        .map(|triangle| triangle.tile)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!tiles.is_empty() && tiles.len() <= MAX_SURFACE_TILES);
    for x in [0.0, 1.0, camera.viewport[0] * 0.5, 1_279.0] {
        for y in [0.0, 1.0, camera.viewport[1] * 0.5, 719.0] {
            let sample = ScreenPoint { x, y };
            assert!(
                triangles
                    .iter()
                    .any(|triangle| barycentric(triangle.points, sample).is_some()),
                "covering LOD missed viewport sample {sample:?}"
            );
        }
    }
}

#[wasm_bindgen_test]
fn high_elevation_moves_an_offscreen_tile_into_the_projected_viewport() {
    let camera = SceneCamera {
        center: [0.0, 0.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let high = SceneTerrain {
        position: [3.5, 3.5],
        material: 4,
        elevation_meters: 7.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [7; 4],
            kind: SceneTerrainSurface::CLIFF,
            triangulation: 0,
            water: 0,
        },
    };
    let triangles = projected_surface_triangles(&[high], camera);
    assert!(!triangles.is_empty());
    assert!(triangles.iter().all(|triangle| triangle.tile == [3, 3]));
    let centroid = ScreenPoint {
        x: triangles[0]
            .points
            .iter()
            .map(|point| point.screen.x)
            .sum::<f64>()
            / 3.0,
        y: triangles[0]
            .points
            .iter()
            .map(|point| point.screen.y)
            .sum::<f64>()
            / 3.0,
    };
    assert!((0.0..=camera.viewport[0]).contains(&centroid.x));
    assert!((0.0..=camera.viewport[1]).contains(&centroid.y));
    assert!(barycentric(triangles[0].points, centroid).is_some());
}
