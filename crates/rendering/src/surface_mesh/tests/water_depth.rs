//! Water depth classes in the shared surface mesh: own sheets, a textured
//! fallback, and borders blended between classes but never across the shore.
use super::*;
use crate::surface_mesh::water_fixture::{self as fixture, DEEP, FORD, MEDIUM, SHALLOW};

fn water(material: u8) -> bool {
    matches!(material, SHALLOW | MEDIUM | DEEP | FORD)
}

fn tile(x: i32, y: i32, material: u8, kind: u8) -> SceneTerrain {
    SceneTerrain {
        appearance: None,
        position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
        material,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface {
            corner_game_height_levels: [0; 4],
            kind: SceneTerrainSurface::PLATEAU,
            triangulation: 0,
            water: kind,
        },
    }
}

#[wasm_bindgen_test]
fn every_water_class_selects_its_own_sheet_and_falls_back_to_textured_shallow_art() {
    let art = fixture::art(true);
    let sheet = |art: &GameArt, material| terrain_texture_frame(art, material, [3, 4]).unwrap();
    assert_eq!(sheet(&art, SHALLOW).atlas, art.terrain[5][0].atlas);
    assert_eq!(sheet(&art, MEDIUM).atlas, art.water_depth[0].0[0].atlas);
    assert_eq!(sheet(&art, DEEP).atlas, art.water_depth[1].0[0].atlas);
    assert_eq!(sheet(&art, FORD).atlas, art.water_depth[2].0[0].atlas);
    let classes = [SHALLOW, MEDIUM, DEEP, FORD].map(|material| sheet(&art, material).atlas);
    for (index, class) in classes.iter().enumerate() {
        assert!(classes[..index].iter().all(|other| other != class));
    }
    let plain = fixture::art(false);
    for material in [SHALLOW, MEDIUM, DEEP, FORD] {
        assert_eq!(sheet(&plain, material).atlas, plain.terrain[5][0].atlas);
    }
}

#[wasm_bindgen_test]
fn water_classes_blend_at_their_borders_but_never_with_land() {
    // Two shallow columns, two deep columns and a ford column over a land row.
    let mut terrain = Vec::new();
    for y in 0..3 {
        for x in 0..5 {
            terrain.push(match (x, y) {
                (_, 2) => tile(x, y, 0, 0),
                (0 | 1, _) => tile(x, y, SHALLOW, 1),
                (2 | 3, _) => tile(x, y, DEEP, 2),
                _ => tile(x, y, FORD, 1),
            });
        }
    }
    let camera = SceneCamera {
        center: [2.5, 1.5],
        zoom: 1.0,
        viewport: [640.0, 480.0],
        focus_elevation_meters: 0.0,
    };
    let mut triangles = projected_surface_triangles(&terrain, camera);
    apply_terrain_textures(&mut triangles, &fixture::art(true));
    let (mut border, mut wet, mut dry) = (0, 0, 0);
    for triangle in triangles.iter().filter(|triangle| !triangle.skirt) {
        let materials = triangle
            .texture_materials
            .expect("flat faces carry materials");
        if water(triangle.material) {
            wet += 1;
            assert_eq!(triangle.tint, 0, "water face is procedurally tinted");
            assert!(triangle.texture_uv.is_some());
            assert!(
                materials.into_iter().all(water),
                "land material {materials:?} on a water face"
            );
            if materials.contains(&SHALLOW) && materials.contains(&DEEP) {
                border += 1;
                assert!(
                    triangle.texture_blend.is_some(),
                    "class border is not blended"
                );
            }
        } else {
            dry += 1;
            assert!(
                !materials.into_iter().any(water),
                "water material {materials:?} on a land face"
            );
        }
    }
    assert!(wet > 0 && dry > 0);
    assert!(border > 0, "no face spans the shallow/deep border");
}

#[wasm_bindgen_test]
fn far_zoom_directory_resolves_every_water_class_to_the_same_sheet_as_near_zoom() {
    use crate::surface_mesh::directory;
    let art = fixture::art(true);
    let mut atlas = fixture::atlas();
    let written = directory::write_terrain_directory(&art, &mut atlas);
    assert_eq!(written, Ok(()));
    for material in [SHALLOW, MEDIUM, DEEP, FORD] {
        let tiled = directory::frame(&atlas, material, [7, -3]);
        let near = terrain_texture_frame(&art, material, [7, -3]).map(|frame| frame.atlas);
        assert!(
            tiled.is_some() && tiled == near,
            "tiled water material {material}"
        );
    }
}
