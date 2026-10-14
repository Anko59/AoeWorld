use super::*;

#[wasm_bindgen_test]
fn continuous_steep_ground_does_not_turn_into_a_checkerboard_of_coarse_walls() {
    let camera = SceneCamera {
        center: [48.0; 2],
        zoom: 0.25,
        viewport: [1440.0, 1000.0],
        focus_elevation_meters: 48.0,
    };
    let terrain = (0..96)
        .flat_map(|y| {
            (0..96).map(move |x| {
                let heights = [
                    (x + y) / 2,
                    (x + y + 1) / 2,
                    (x + y + 2) / 2,
                    (x + y + 1) / 2,
                ]
                .map(|z| z as i16);
                SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 7,
                    elevation_meters: f64::from(heights[0]),
                    appearance: None,
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: heights,
                        kind: SceneTerrainSurface::CLIFF,
                        triangulation: 0,
                        water: 0,
                    },
                }
            })
        })
        .collect::<Vec<_>>();
    let faces = projected_surface_triangles(&terrain, camera);
    assert!(faces.len() <= MAX_SURFACE_TRIANGLES);
    assert!(
        faces.iter().all(|face| !face.skirt),
        "continuous steep source corners have no vertical discontinuity"
    );
}

#[wasm_bindgen_test]
fn coarse_terrain_keeps_the_cliff_wall_instead_of_averaging_its_ledge_away() {
    let camera = SceneCamera {
        center: [48.0; 2],
        zoom: 0.25,
        viewport: [1440.0, 1000.0],
        focus_elevation_meters: 2.0,
    };
    let terrain = (0..96)
        .flat_map(|y| {
            (0..96).map(move |x| {
                let cliff = (45..48).contains(&x);
                let height = if x < 48 { 4 } else { 0 };
                SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 7,
                    elevation_meters: f64::from(height),
                    appearance: None,
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: [height; 4],
                        kind: if cliff {
                            SceneTerrainSurface::CLIFF_LEDGE
                        } else {
                            SceneTerrainSurface::PLATEAU
                        },
                        triangulation: 0,
                        water: 0,
                    },
                }
            })
        })
        .collect::<Vec<_>>();
    let faces = projected_surface_triangles(&terrain, camera);
    assert!(faces.len() <= MAX_SURFACE_TRIANGLES);
    let walls = faces.iter().filter(|face| face.skirt).collect::<Vec<_>>();
    assert!(walls.len() > 10, "a ridge is visible at minimum zoom");
    assert!(
        walls
            .iter()
            .flat_map(|face| face.points)
            .any(|point| point.world[2] == 4.0)
    );
}

#[wasm_bindgen_test]
fn a_foremost_nonpickable_wall_does_not_cancel_the_ground_hit() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let ground = SceneTerrain {
        position: [0.5; 2],
        material: 7,
        elevation_meters: 0.0,
        appearance: None,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let faces = projected_surface_triangles(&[ground], camera);
    let mut occluder = faces[0];
    occluder.pickable = false;
    for point in &mut occluder.points {
        point.world[2] = 10.0;
    }
    let pointer = ScreenPoint { x: 140.0, y: 32.0 };
    let expected = pick_surface_point(&faces, pointer).unwrap();
    for skirt in [true, false] {
        occluder.skirt = skirt;
        let mut triangles = faces.clone();
        triangles.push(occluder);
        assert_eq!(pick_surface_point(&triangles, pointer), Some(expected));
        triangles.reverse();
        assert_eq!(pick_surface_point(&triangles, pointer), Some(expected));
        assert_eq!(pick_surface_point(&[occluder], pointer), None);
    }
}

#[wasm_bindgen_test]
fn joined_cliff_walls_use_original_rock_and_share_the_ledge_vertices() {
    let camera = SceneCamera {
        center: [1.0, 1.0],
        zoom: 1.0,
        viewport: [512.0; 2],
        focus_elevation_meters: 2.0,
    };
    let mut terrain = Vec::new();
    for y in 0..2 {
        for x in 0..2 {
            let height = if x == 0 { 4 } else { 0 };
            terrain.push(SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                elevation_meters: f64::from(height),
                material: 7,
                appearance: None,
                surface: SceneTerrainSurface {
                    corner_game_height_levels: [height; 4],
                    kind: if x == 0 {
                        SceneTerrainSurface::CLIFF_LEDGE
                    } else {
                        SceneTerrainSurface::PLATEAU
                    },
                    triangulation: 0,
                    water: 0,
                },
            });
        }
    }
    let mut art = test_art(GameFrame {
        atlas: AtlasAddress {
            page: 0,
            uv: [0.0, 0.0, 0.05, 0.025],
        },
        size: [97.0, 49.0],
        anchor: [0.0; 2],
    });
    art.scenery.cliff_faces = (0..25)
        .map(|slot| GameFrame {
            atlas: AtlasAddress {
                page: 2,
                uv: [slot as f32 / 32.0, 0.0, 0.03, 0.07],
            },
            size: [294.0, 156.0],
            anchor: [153.0, 80.0],
        })
        .collect();
    let mut faces = projected_surface_triangles(&terrain, camera);
    apply_terrain_textures(&mut faces, &art);
    let walls = faces.iter().filter(|face| face.skirt).collect::<Vec<_>>();
    assert_eq!(walls.len(), 4);
    for wall in &walls {
        let address = wall.texture_uv.unwrap();
        assert_eq!(address.page, 2, "native rock, never snow or gravel");
        assert_native_region(address, &art, &[4, 5, 16, 17, 24]);
        assert_eq!(wall.tint, 0, "retain the authored face lighting");
        assert!(!wall.pickable);
        for point in wall.points.iter().filter(|point| point.world[2] == 4.0) {
            assert!(
                faces
                    .iter()
                    .filter(|face| !face.skirt)
                    .flat_map(|face| face.points)
                    .any(|top| top.world == point.world && top.screen == point.screen)
            );
        }
    }
    assert!(
        walls
            .iter()
            .filter(|wall| wall.tile[1] == 0)
            .flat_map(|wall| wall.points)
            .any(|point| point.world == [1.0, 1.0, 0.0])
    );
    assert!(
        walls
            .iter()
            .filter(|wall| wall.tile[1] == 1)
            .flat_map(|wall| wall.points)
            .any(|point| point.world == [1.0, 1.0, 4.0])
    );
    for sample in &mut terrain {
        sample.position.swap(0, 1);
    }
    let mut rotated = projected_surface_triangles(&terrain, camera);
    apply_terrain_textures(&mut rotated, &art);
    for wall in rotated.iter().filter(|face| face.skirt) {
        assert_native_region(wall.texture_uv.unwrap(), &art, &[1, 2, 18, 15, 19]);
    }
}

fn rocky_ridge() -> (Vec<SceneTerrain>, SceneCamera) {
    let terrain = (0..96)
        .flat_map(|y| {
            (0..96).map(move |x| {
                let raised = (45..48).contains(&x) && (20..76).contains(&y);
                let height = if raised { 4 } else { 0 };
                SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 7,
                    elevation_meters: f64::from(height),
                    appearance: None,
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: [height; 4],
                        triangulation: 0,
                        water: 0,
                        kind: if raised {
                            SceneTerrainSurface::CLIFF_LEDGE
                        } else {
                            SceneTerrainSurface::PLATEAU
                        },
                    },
                }
            })
        })
        .collect();
    (
        terrain,
        SceneCamera {
            center: [48.0; 2],
            zoom: 1.0,
            viewport: [1440.0, 1000.0],
            focus_elevation_meters: 2.0,
        },
    )
}

#[wasm_bindgen_test]
fn cliff_rim_is_irregular_and_shared_with_both_ground_sheets_at_both_zooms() {
    let (terrain, mut camera) = rocky_ridge();
    for zoom in [1.0, 0.25] {
        camera.zoom = zoom;
        let faces = projected_surface_triangles(&terrain, camera);
        let walls = faces.iter().filter(|f| f.skirt).collect::<Vec<_>>();
        assert!(!walls.is_empty());
        assert!(
            walls
                .iter()
                .flat_map(|f| f.points)
                .any(|p| { p.world[0].fract() != 0.0 || p.world[1].fract() != 0.0 }),
            "cliff rim must not be a ruler-straight tile outline at {zoom}"
        );
        for wall in &walls {
            assert!(!wall.pickable);
            for point in wall.points {
                assert!(
                    faces
                        .iter()
                        .filter(|f| !f.skirt)
                        .flat_map(|f| f.points)
                        .any(|ground| ground.world == point.world && ground.screen == point.screen),
                    "wall endpoint meets its ground sheet: {:?}",
                    point.world
                );
            }
        }
        let mut reversed = terrain.clone();
        reversed.reverse();
        let other = projected_surface_triangles(&reversed, camera);
        let positions = |faces: &[ProjectedSurfaceTriangle]| {
            faces
                .iter()
                .flat_map(|f| f.points)
                .map(|p| p.world)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            positions(&faces),
            positions(&other),
            "stable across input order"
        );
        assert!(faces.len() <= MAX_SURFACE_TRIANGLES);
    }
}

#[wasm_bindgen_test]
fn adjacent_wall_segments_use_varied_original_rock_regions_deterministically() {
    let (terrain, camera) = rocky_ridge();
    let base = GameFrame {
        atlas: AtlasAddress {
            page: 2,
            uv: [0.01, 0.02, 294.0 / 2048.0, 156.0 / 2048.0],
        },
        size: [294.0, 156.0],
        anchor: [153.0, 80.0],
    };
    let mut art = test_art(base);
    art.scenery.cliff_faces = (0..25)
        .map(|n| GameFrame {
            atlas: AtlasAddress {
                uv: [n as f32 / 32.0, 0.02, base.atlas.uv[2], base.atlas.uv[3]],
                ..base.atlas
            },
            ..base
        })
        .collect();
    let mut faces = projected_surface_triangles(&terrain, camera);
    apply_terrain_textures(&mut faces, &art);
    let walls = faces
        .iter()
        .filter(|f| f.skirt && f.order == 2)
        .collect::<Vec<_>>();
    let first = walls[0].texture_uv.unwrap();
    assert!(
        walls.iter().any(|f| f.texture_uv.unwrap() != first),
        "adjacent segments must not repeat one stripe"
    );
    for wall in walls {
        assert_eq!(wall.texture_uv.unwrap().page, 2, "original cliff atlas");
        let twin = faces
            .iter()
            .find(|f| f.tile == wall.tile && f.order == 3)
            .unwrap();
        assert_eq!(
            wall.texture_uv, twin.texture_uv,
            "the two triangles share one rock region"
        );
    }
    let mut again = projected_surface_triangles(&terrain, camera);
    apply_terrain_textures(&mut again, &art);
    assert_eq!(
        faces.iter().map(|f| f.texture_uv).collect::<Vec<_>>(),
        again.iter().map(|f| f.texture_uv).collect::<Vec<_>>()
    );
}

fn assert_native_region(address: AtlasAddress, art: &GameArt, frames: &[usize]) {
    assert!(
        frames.iter().any(|slot| {
            let rect = art.scenery.cliff_faces[*slot].atlas;
            address.page == rect.page
                && (0..2).all(|axis| {
                    let lo = address.uv[axis] + address.uv[axis + 2] * 0.3;
                    let hi = address.uv[axis] + address.uv[axis + 2] * 0.7;
                    lo >= rect.uv[axis] - 0.00001
                        && hi <= rect.uv[axis] + rect.uv[axis + 2] + 0.00001
                })
        }),
        "sampled region belongs to reviewed native faces: {address:?}"
    );
}

#[wasm_bindgen_test]
fn rocky_presentation_keeps_ground_orders_in_the_source_cell_at_both_lods() {
    let (mut terrain, mut camera) = rocky_ridge();
    for sample in &mut terrain {
        sample.material = if sample.position[0] as i32 % 2 == 0 {
            0
        } else {
            7
        };
    }
    let mut unsculpted = terrain.clone();
    for sample in &mut unsculpted {
        sample.surface.kind = SceneTerrainSurface::PLATEAU;
    }
    let art = test_art(GameFrame {
        atlas: AtlasAddress {
            page: 0,
            uv: [0.0, 0.0, 97.0 / 2048.0, 49.0 / 2048.0],
        },
        size: [97.0, 49.0],
        anchor: [0.0; 2],
    });
    let mut checked = 0;
    for zoom in [1.0, 0.25] {
        camera.zoom = zoom;
        let mut faces = projected_surface_triangles(&terrain, camera);
        let mut control = projected_surface_triangles(&unsculpted, camera);
        apply_terrain_textures(&mut faces, &art);
        apply_terrain_textures(&mut control, &art);
        for face in faces.iter().filter(|f| f.pickable && !f.skirt) {
            let original = control
                .iter()
                .find(|f| {
                    !f.skirt
                        && f.tile == face.tile
                        && f.texture_tile == face.texture_tile
                        && f.draw_order() == face.draw_order()
                })
                .unwrap();
            for corner in 0..3 {
                let mut weights = [0.01; 3];
                weights[corner] = 0.98;
                let pointer = ScreenPoint {
                    x: face
                        .points
                        .iter()
                        .zip(weights)
                        .map(|(p, w)| p.screen.x * w)
                        .sum(),
                    y: face
                        .points
                        .iter()
                        .zip(weights)
                        .map(|(p, w)| p.screen.y * w)
                        .sum(),
                };
                let picked =
                    pick_surface_point(&[*face], pointer).expect("presented ground is pickable");
                for axis in 0..2 {
                    let expected = original
                        .points
                        .iter()
                        .zip(weights)
                        .map(|(p, w)| p.world[axis] * w)
                        .sum::<f64>();
                    assert!(
                        (picked[axis] - expected).abs() < 0.000001,
                        "cosmetic bevel preserves source coordinates at zoom {zoom}: {picked:?}, {expected}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100);
    assert_eq!(std::mem::size_of::<ProjectedSurfaceTriangle>(), 240);
}

mod crossings;
