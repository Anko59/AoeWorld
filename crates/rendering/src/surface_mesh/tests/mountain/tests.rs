//! Synthetic mountain art and displayed-vertex coverage, without private pixels.
use super::*;

fn ground(x: i32, y: i32, material: u8) -> SceneTerrain {
    SceneTerrain {
        position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
        material,
        elevation_meters: 4.0,
        surface: SceneTerrainSurface::flat(4.0),
        appearance: Some(crate::SceneTerrainAppearance {
            floor_strength: 0,
            canopy_strength: 0,
            palette: 5,
            exposure: 0,
            height_band: 4,
        }),
    }
}

fn art() -> GameArt {
    let frame = |group: usize| GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [group as f32 / 16.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [0.0; 2],
    };
    let mut art = test_art(frame(0));
    for (group, frames) in art.terrain.iter_mut().enumerate() {
        *frames = vec![frame(group)];
    }
    art
}

#[wasm_bindgen_test]
fn displayed_mountain_vertices_keep_their_ground_cover_on_pure_and_mixed_faces() {
    let art = art();
    // An odd border falls inside a coarse cell, rather than always lining up
    // with the LOD lattice. Both orientations exercise base and overlay faces.
    for (west, east) in [(0, 4), (4, 0), (4, 7), (7, 4)] {
        let terrain = (0..256)
            .flat_map(|y| (0..256).map(move |x| ground(x, y, if x < 65 { west } else { east })))
            .collect::<Vec<_>>();
        for zoom in [1.0, 0.25, 0.12] {
            let camera = SceneCamera {
                center: [65.0, 64.0],
                zoom,
                viewport: if zoom == 1.0 {
                    [1024.0, 768.0]
                } else {
                    [8192.0, 4096.0]
                },
                focus_elevation_meters: 4.0,
            };
            let mut faces = projected_surface_triangles(&terrain, camera);
            assert!(!faces.is_empty());
            if zoom < 1.0 {
                assert!(
                    faces.iter().any(|face| {
                        let xs = face.points.map(|point| point.world[0]);
                        xs.into_iter().fold(f64::MIN, f64::max)
                            - xs.into_iter().fold(f64::MAX, f64::min)
                            > 1.0
                    }),
                    "exercise a coarse displayed patch at zoom {zoom}"
                );
            }
            apply_terrain_textures(&mut faces, &art);
            let mut checked = 0;
            for face in faces.iter().filter(|face| !face.skirt) {
                for (corner, point) in face.points.iter().enumerate() {
                    let [x, y, _] = point.world;
                    if !(60.0..=70.0).contains(&x) || !(60.0..=68.0).contains(&y) {
                        continue;
                    }
                    checked += 1;
                    let over = if mix::rank(west) > mix::rank(east) {
                        west
                    } else {
                        east
                    };
                    // Four incident source cells: half support on the border,
                    // complete support elsewhere, including unmixed faces.
                    let east_share: u8 = if x < 65.0 {
                        0
                    } else if x == 65.0 {
                        128
                    } else {
                        255
                    };
                    let share = if over == east || east_share == 128 {
                        east_share
                    } else {
                        255 - east_share
                    };
                    let noise = mix::noise([x as i32, y as i32], [0.0; 2]);
                    let expected =
                        mix::weights([f32::from(share) / 255.0, 0.0, 0.0], false, noise)[2];
                    let layers = face.texture_materials.unwrap_or([face.material; 3]);
                    let weights = if face.mix != [0; 6] {
                        mix::weights(
                            [
                                f32::from(face.mix[corner]) / 255.0,
                                f32::from(face.mix[3 + corner]) / 255.0,
                                0.0,
                            ],
                            false,
                            noise,
                        )
                    } else if face.floor_strengths.is_some() {
                        // This fixture has no forest bed. Its ecological path
                        // samples the primary sheet at every displayed vertex.
                        [1.0, 0.0, 0.0]
                    } else if face.texture_blend.is_some() {
                        // The ordinary splat assigns one layer to each vertex.
                        std::array::from_fn(|index| f32::from(u8::from(index == corner)))
                    } else {
                        [1.0, 0.0, 0.0]
                    };
                    let actual: f32 = layers
                        .into_iter()
                        .zip(weights)
                        .filter(|(material, _)| *material == over)
                        .map(|(_, weight)| weight)
                        .sum();
                    assert!(
                        (actual - expected).abs() < 1e-5,
                        "{west}/{east}, zoom {zoom}, vertex {x},{y}, face {:?}/{:?}: cover {actual}, expected {expected}",
                        face.texture_tile,
                        face.order
                    );
                }
            }
            assert!(checked > 0);
            // Correct endpoints alone are insufficient: a coarse face can
            // move the soft band several tiles away from the source border.
            // Probe the actual border, where four incident cells give half
            // coverage, using world coordinates as an affine screen plane.
            for y in 60..68 {
                let world = [65.0, f64::from(y) + 0.25];
                let over = if mix::rank(west) > mix::rank(east) {
                    west
                } else {
                    east
                };
                let noise = mix::noise([65, y], [0.0, 0.25]);
                let expected = mix::weights([128.0 / 255.0, 0.0, 0.0], false, noise)[2];
                let mut hits = 0;
                for face in &faces {
                    let points = face.points.map(|mut point| {
                        point.screen = ScreenPoint {
                            x: point.world[0],
                            y: point.world[1],
                        };
                        point
                    });
                    let Some(bary) = barycentric(
                        points,
                        ScreenPoint {
                            x: world[0],
                            y: world[1],
                        },
                    ) else {
                        continue;
                    };
                    hits += 1;
                    let layers = face.texture_materials.unwrap_or([face.material; 3]);
                    let weights = if face.mix != [0; 6] {
                        let share = (0..3)
                            .map(|corner| f64::from(face.mix[corner]) * bary[corner] / 255.0)
                            .sum::<f64>();
                        mix::weights([share as f32, 0.0, 0.0], false, noise)
                    } else {
                        [1.0, 0.0, 0.0]
                    };
                    let actual: f32 = layers
                        .into_iter()
                        .zip(weights)
                        .filter(|(material, _)| *material == over)
                        .map(|(_, weight)| weight)
                        .sum();
                    assert!(
                        (actual - expected).abs() < 0.03,
                        "{west}/{east}, zoom {zoom}, source border {world:?}: cover {actual}, expected {expected}"
                    );
                }
                assert!(hits > 0);
            }
        }
    }
}

#[wasm_bindgen_test]
fn mountain_rock_draws_its_native_sheet_and_retains_authored_colour() {
    let art = art();
    let selected = terrain_texture_frame(&art, 4, [3, -2]).unwrap();
    assert_eq!(
        selected.atlas, art.terrain[4][0].atlas,
        "native rock must take priority over gravel and dirt detail"
    );
    for source in [[155, 120, 95, 255], [95, 115, 145, 128]] {
        let layer = mix::art_layer(source, 4, 1 | 5 << 1, 0);
        assert_eq!(
            layer,
            source.map(f32::from),
            "warm/cool rock colours and broad authored relief must survive mixing"
        );
    }
    let mut faces = projected_surface_triangles(
        &[ground(0, 0, 4)],
        super::appearance_tests::camera([0.5; 2], 1.0, [256.0, 128.0]),
    );
    apply_terrain_textures(&mut faces, &art);
    assert!(
        faces
            .iter()
            .all(|face| face.texture_uv == Some(selected.atlas) && face.tint <= 3),
        "pure rock must preserve the same native colour as border layers"
    );
}

#[wasm_bindgen_test]
fn native_snow_keeps_soft_ground_art_instead_of_a_white_ice_kernel() {
    let art = art();
    assert_eq!(
        terrain_texture_frame(&art, 7, [3, -2]).unwrap().atlas,
        art.terrain[11][0].atlas
    );
    let source = [230, 235, 240, 128];
    assert_eq!(
        mix::art_layer(source, 7, 1 | 5 << 1, 0),
        source.map(f32::from),
        "native snow's gentle relief must be drawn as authored on mixed faces"
    );
    let mut faces = projected_surface_triangles(
        &[ground(0, 0, 7)],
        super::appearance_tests::camera([0.5; 2], 1.0, [256.0, 128.0]),
    );
    apply_terrain_textures(&mut faces, &art);
    assert!(
        faces.iter().all(|face| face.tint <= 3),
        "pure snow must use the same native ground colour as mixed snow"
    );
}
