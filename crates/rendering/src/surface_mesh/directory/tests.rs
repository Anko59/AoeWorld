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

#[wasm_bindgen_test]
fn native_planar_cells_cross_inside_tiles_and_directory_tracks_eviction_and_lod() {
    let mut art = fixture::art();
    art.grass.clear();
    art.terrain = Default::default();
    art.terrain_topology = [None; 12];
    art.terrain[0] = (0..7)
        .map(|i| crate::GameFrame {
            stream: i + 1,
            atlas: AtlasAddress::WHITE,
            size: [256.0; 2],
            anchor: [0.0; 2],
        })
        .collect();
    art.terrain_topology[0] = Some(TerrainTopology::Dds {
        size: [512, 256],
        period: [10, 10],
        level: 0,
    });
    let a = AtlasAddress::new(8, [2.0, 2.0, 256.0, 256.0].map(|n| n / 2048.0));
    let b = AtlasAddress::new(10, [2.0, 2.0, 256.0, 256.0].map(|n| n / 2048.0));
    art.resident = vec![Some(a), Some(b), Some(a), None, None, None, None];
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    let write = |atlas: &mut [u8], art: &crate::GameArt| {
        let rows = terrain_directory_rows(art).unwrap();
        atlas[..rows.len()].copy_from_slice(&rows);
    };
    write(&mut atlas, &art);
    let s = sheet(&atlas, 0).unwrap();
    for (world, expected, local) in [
        ([4.9, 0.5], a, [0.98, 0.05]),
        ([5.1, 0.5], b, [0.02, 0.05]),
        ([-4.9, 0.5], b, [0.02, 0.05]),
    ] {
        let tile = world.map(|n: f64| n.floor() as i32);
        let inside = [0, 1].map(|i| world[i] - f64::from(tile[i]));
        let (frame, uv, gradient) =
            s.sample(&atlas, tile, diamond(inside), [[0.1, 0.0], [0.0, 0.1]]);
        assert_eq!(frame, expected);
        for i in 0..2 {
            assert!((uv[i] - local[i]).abs() < 1e-9);
        }
        for (a, b) in gradient
            .into_iter()
            .flatten()
            .zip([0.02, -0.01, 0.02, 0.01])
        {
            assert!((a - b).abs() < 1e-9);
        }
    }
    art.resident[1] = None;
    write(&mut atlas, &art);
    assert_eq!(
        sheet(&atlas, 0).unwrap().frame(&atlas, [6, 0]),
        AtlasAddress::WHITE
    );
    art.terrain_topology[0] = Some(TerrainTopology::Dds {
        size: [512, 256],
        period: [10, 10],
        level: 1,
    });
    write(&mut atlas, &art);
    assert_eq!(sheet(&atlas, 0).unwrap().frame(&atlas, [6, 0]), a);
}

#[wasm_bindgen_test]
fn terrain_lod_working_set_stays_bounded_and_offscreen_tiles_request_nothing() {
    let mut art = fixture::art();
    art.grass.clear();
    art.terrain = Default::default();
    art.terrain_topology = [None; 12];
    let topology = TerrainTopology::Dds {
        size: [4096; 2],
        period: [10; 2],
        level: 0,
    };
    let count = topology.dds_frames().unwrap();
    for material in [0, 2, 6] {
        art.terrain_topology[material] = Some(topology);
        art.terrain[material] = (0..count)
            .map(|i| crate::GameFrame {
                stream: (material * count + i + 1) as u32,
                atlas: AtlasAddress::WHITE,
                size: [256.0; 2],
                anchor: [0.0; 2],
            })
            .collect();
    }
    let terrain: Vec<_> = (0..100)
        .map(|i| crate::SceneTerrain {
            position: [(i % 10) as f64, (i / 10) as f64],
            material: 0,
            elevation_meters: 0.0,
            surface: crate::SceneTerrainSurface::flat(0.0),
            appearance: None,
        })
        .collect();
    let camera = crate::SceneCamera {
        center: [5.0; 2],
        zoom: 8.0,
        viewport: [4096.0; 2],
        focus_elevation_meters: 0.0,
    };
    let ids = crate::scene_terrain_frames(&mut art, &terrain, camera);
    assert!(!ids.is_empty());
    assert!(ids.len() * 320 * 320 * 4 <= 16 * 1024 * 1024);
    assert!(
        crate::scene_terrain_frames(
            &mut art,
            &terrain,
            crate::SceneCamera {
                center: [10000.0; 2],
                ..camera
            }
        )
        .is_empty()
    );
}

mod native;

#[wasm_bindgen_test]
fn an_empty_view_also_coarsens_oversized_directory_metadata() {
    let mut art = fixture::art();
    art.grass.clear();
    let topology = TerrainTopology::Dds {
        size: [4096; 2],
        period: [10; 2],
        level: 0,
    };
    let count = topology.dds_frames().unwrap();
    let f = crate::GameFrame {
        stream: 0,
        atlas: AtlasAddress::WHITE,
        size: [256.0; 2],
        anchor: [0.0; 2],
    };
    art.terrain = std::array::from_fn(|_| vec![f; count]);
    art.terrain_topology = [Some(topology); 12];
    art.water_depth = std::array::from_fn(|_| (vec![f; count], Some(topology)));
    art.foundation = (vec![f; count], Some(topology));
    assert!(
        crate::scene_terrain_frames(
            &mut art,
            &[],
            crate::SceneCamera {
                center: [0.0; 2],
                zoom: 16.0,
                viewport: [128.0; 2],
                focus_elevation_meters: 0.0
            }
        )
        .is_empty()
    );
    assert_eq!(terrain_directory_rows(&art).unwrap().len(), 16384);
}
