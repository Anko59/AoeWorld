use super::*;
use crate::surface_mesh::terrain_texture_frame;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn terrain_atlas_selection_follows_reviewed_ten_by_ten_neighbor_order() {
    let frames = (0..100)
        .map(|index| GameFrame {
            atlas: crate::AtlasAddress {
                page: 0,
                uv: [index as f32, 0.0, 0.01, 0.01],
            },
            size: [97.0, 49.0],
            anchor: [0.0; 2],
        })
        .collect::<Vec<_>>();
    let mut art = test_art(frames[0]);
    art.terrain[0] = frames.clone();
    art.grass.clone_from(&frames);
    art.terrain_topology[0] = Some(crate::TerrainTopology::PeriodicXMajorReversedY {
        columns: 10,
        rows: 10,
    });

    for (tile, expected) in [
        ([0, 0], 0),
        ([1, 0], 10),
        ([0, 1], 9),
        ([9, 9], 91),
        ([-1, -1], 91),
        ([i32::MIN, i32::MIN], 28),
    ] {
        assert_eq!(
            terrain_texture_frame(&art, 0, tile).map(|frame| frame.atlas.uv[0]),
            Some(expected as f32),
            "tile {tile:?} selected the wrong texture frame"
        );
    }
}

#[wasm_bindgen_test]
fn undeclared_hundred_frames_use_coordinate_stable_fallback_not_sheet_inference() {
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 1,
            uv: [0.0; 4],
        },
        size: [97.0, 49.0],
        anchor: [0.0; 2],
    };
    let mut art = test_art(frame);
    art.terrain[0] = (0..100)
        .map(|index| GameFrame {
            atlas: crate::AtlasAddress {
                page: 1,
                uv: [index as f32, 0.0, 0.01, 0.01],
            },
            ..frame
        })
        .collect();
    assert_eq!(art.terrain_topology[0], None);
    for (tile, expected) in [([0, 0], 0), ([1, 0], 7), ([0, 1], 13), ([-1, -1], 20)] {
        let selected = terrain_texture_frame(&art, 0, tile).unwrap().atlas;
        assert_eq!(selected.page, 1);
        assert_eq!(selected.uv[0], expected as f32);
        assert_eq!(
            terrain_texture_frame(&art, 0, tile).unwrap().atlas,
            selected
        );
    }
}

#[wasm_bindgen_test]
fn same_uv_different_pages_remain_distinct_terrain_blend_addresses() {
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [0.0; 2],
    };
    let mut art = test_art(frame);
    art.terrain[1][0].atlas.page = 1;
    art.terrain[2][0].atlas.page = 2;
    let sample = SceneTerrain {
        position: [0.5; 2],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let camera = SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let mut triangles = projected_surface_triangles(&[sample], camera);
    for triangle in &mut triangles {
        triangle.texture_materials = Some([0, 1, 2]);
    }
    apply_terrain_textures(&mut triangles, &art);
    for triangle in triangles {
        assert_eq!(triangle.texture_uv, Some(frame.atlas));
        assert_eq!(
            triangle.texture_blend,
            Some([art.terrain[1][0].atlas, art.terrain[2][0].atlas])
        );
    }
}

#[wasm_bindgen_test]
fn partial_terrain_variant_sets_vary_on_both_axes() {
    let frames = (0..10)
        .map(|index| GameFrame {
            atlas: crate::AtlasAddress {
                page: 0,
                uv: [index as f32, 0.0, 0.01, 0.01],
            },
            size: [97.0, 49.0],
            anchor: [0.0; 2],
        })
        .collect::<Vec<_>>();
    let mut art = test_art(frames[0]);
    art.terrain[1] = frames;
    let origin = terrain_texture_frame(&art, 1, [0, 0]).unwrap().atlas.uv;
    assert_ne!(
        origin,
        terrain_texture_frame(&art, 1, [1, 0]).unwrap().atlas.uv
    );
    assert_ne!(
        origin,
        terrain_texture_frame(&art, 1, [0, 1]).unwrap().atlas.uv
    );
}
