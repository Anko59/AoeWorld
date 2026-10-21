use super::*;
use crate::surface_mesh::terrain_texture_frame;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn directory_resolves_every_material_tile_to_the_cpu_selected_frame() {
    let art = fixture::art();
    let atlas = fixture::atlas();
    for material in 0..=16 {
        for y in -13..13 {
            for x in -13..13 {
                assert_eq!(
                    fixture::frame(&atlas, material, [x, y]),
                    terrain_texture_frame(&art, material, [x, y]).map(|frame| frame.atlas),
                    "material {material} tile {x},{y}"
                );
            }
        }
    }
    // A wrapped accent hash and the extreme coordinates stay in range.
    for tile in [[i32::MIN, i32::MIN], [i32::MAX, i32::MIN], [i32::MAX; 2]] {
        assert_eq!(
            fixture::frame(&atlas, 2, tile),
            terrain_texture_frame(&art, 2, tile).map(|frame| frame.atlas)
        );
    }
}

#[wasm_bindgen_test]
fn dat_foundation_sheet_uses_the_same_directory_phase_and_junction_code() {
    let mut art = fixture::art();
    art.foundation = (art.terrain[0].clone(), art.terrain_topology[0]);
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    crate::write_terrain_directory(&art, &mut atlas).unwrap();
    for tile in [[-13, 7], [0, 0], [5, -4], [20, 31]] {
        assert_eq!(
            fixture::frame(&atlas, 17, tile),
            terrain_texture_frame(&art, 17, tile).map(|f| f.atlas)
        );
    }
    assert_eq!(crate::surface_mesh::mix::support::material(11), 17);
}

#[wasm_bindgen_test]
fn directory_refuses_occupied_reserved_rows_and_skips_art_without_terrain() {
    let mut art = fixture::art();
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    atlas[..4].copy_from_slice(&[255; 4]); // The reserved white origin is not directory space.
    crate::write_terrain_directory(&art, &mut atlas).expect("free reserved rows");
    assert!(fixture::frame(&atlas, 0, [0, 0]).is_some());
    assert_eq!(atlas[..4], [255; 4]);
    assert!(
        atlas[2 * 2048 * 4..].iter().all(|byte| *byte == 0),
        "the directory stays inside the two reserved rows of page 0"
    );
    let mut occupied = vec![0; crate::GAME_ATLAS_BYTES];
    occupied[40] = 1;
    assert!(crate::write_terrain_directory(&art, &mut occupied).is_err());
    assert!(crate::write_terrain_directory(&art, &mut [0; 16]).is_err());
    art.grass.clear();
    art.terrain = Default::default();
    occupied[40] = 0;
    occupied[44] = 9;
    crate::write_terrain_directory(&art, &mut occupied).expect("nothing to write");
    assert_eq!(occupied[44], 9);
    assert_eq!(fixture::frame(&occupied, 0, [0, 0]), None);
}

#[wasm_bindgen_test]
fn coarse_patches_sample_by_world_tile_and_fine_tiles_keep_their_frame() {
    let art = fixture::art();
    for case in [1, 2] {
        let faces = fixture::scene(case);
        for face in &faces {
            let span = face.points.iter().fold(0.0_f64, |span, point| {
                span.max(point.world[0] - f64::from(face.texture_tile[0]))
            });
            assert_eq!(face.texture_mode == TILED, span > 1.0, "case {case}");
            assert_eq!(
                face.texture_uv,
                terrain_texture_frame(&art, face.material, face.texture_tile).map(|f| f.atlas)
            );
        }
    }
}
