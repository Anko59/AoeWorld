use super::*;
use aoe_core::{EntityId, ScreenPoint};
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
    pub(super) static SAMPLE_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[wasm_bindgen_test]
fn dock_foundations_and_aprons_paint_land_without_changing_water() {
    let mut art = crate::surface_mesh::mix::fixture::art();
    art.compositions.push(crate::BuildingArt {
        unit_id: 45,
        footprint: [3.0; 2],
        foundation_material: None,
        parts: Vec::new(),
        construction: Vec::new(),
    });
    // The trial Dock has no dedicated sheet; also cover DAT-selected terrain.
    for foundation in [None, Some(13)] {
        art.compositions.last_mut().unwrap().foundation_material = foundation;
        for kind in [SceneUnitKind::Dock, SceneUnitKind::foundation(45)] {
            for x in [0.5, 2.5] {
                // footprint, then apron
                for (water, material) in [
                    (0, 0),
                    (0, 5),
                    (0, 10),
                    (1, 5),
                    (2, 5),
                    (3, 10),
                    (4, 5),
                    (5, 5),
                ] {
                    let surface = SceneTerrainSurface {
                        water,
                        ..SceneTerrainSurface::flat(0.0)
                    };
                    let appearance = Some(SceneTerrainAppearance {
                        canopy_strength: 700,
                        floor_strength: 800,
                        palette: 2,
                        exposure: 0,
                        height_band: 0,
                    });
                    let mut tiles = [SceneTerrain {
                        position: [x, 0.5],
                        material,
                        elevation_meters: 0.0,
                        surface,
                        appearance,
                    }];
                    apply_building_foundations(&mut tiles, &art, &[test_unit(kind)]);
                    if water == 0 && material == 0 {
                        assert_eq!(
                            tiles[0].material,
                            foundation.unwrap_or(2),
                            "land {kind:?} at {x}"
                        );
                        assert_eq!(tiles[0].appearance.unwrap().floor_strength, 0);
                    } else {
                        assert_eq!(
                            tiles[0].material, material,
                            "water {water}, {kind:?} at {x}"
                        );
                        assert_eq!(tiles[0].appearance, appearance);
                    }
                    assert_eq!(tiles[0].surface, surface);
                }
            }
        }
    }
}

#[wasm_bindgen_test]
fn native_farm_without_de_art_keeps_its_imported_crop_sheet() {
    let mut art = crate::surface_mesh::mix::fixture::art();
    art.compositions.push(crate::BuildingArt {
        unit_id: 50,
        footprint: [3.0; 2],
        foundation_material: Some(17),
        parts: Vec::new(),
        construction: Vec::new(),
    });
    assert!(art.buildings.iter().all(Vec::is_empty));
    let camera = test_camera();
    let farm = test_unit(SceneUnitKind::Farm);
    let mut tiles = [SceneTerrain {
        position: [0.5; 2],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: None,
    }];
    apply_building_foundations(&mut tiles, &art, &[farm]);
    assert_eq!(tiles[0].material, 17);
    let mut faces = projected_surface_triangles(&tiles, camera);
    apply_terrain_textures(&mut faces, &art);
    let before = faces.clone();
    let ground = BuildingGround::new(&[farm], camera);
    for (face, native) in faces.iter_mut().zip(before) {
        assert_eq!(native.texture_uv, Some(art.foundation.0[0].atlas));
        ground.farm(face, &art);
        assert_eq!(
            face.texture_uv, native.texture_uv,
            "native SLP 15004 must reach rendering"
        );
        assert_eq!(face.texture_materials, native.texture_materials);
        assert_eq!(face.texture_blend, native.texture_blend);
        assert_eq!(face.mode(), native.mode());
        assert_eq!(face.color, native.color);
    }
}

#[wasm_bindgen_test]
fn foundation_painting_rejects_distant_buildings_and_visits_visible_samples_once() {
    let art = crate::surface_mesh::mix::fixture::art();
    let mut tiles: Vec<_> = (0..100)
        .flat_map(|y| {
            (0..100).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
        })
        .collect();
    let mut units: Vec<_> = (0..200)
        .map(|i| {
            let mut unit = test_unit(SceneUnitKind::House);
            unit.position = [1000.0 + f64::from(i) * 4.0; 2];
            unit
        })
        .collect();
    SAMPLE_VISITS.set(0);
    let started = js_sys::Date::now();
    apply_building_foundations(&mut tiles, &art, &units);
    wasm_bindgen_test::console_log!(
        "10,000 samples, 200 distant foundations: {} ms, {} sample predicates",
        js_sys::Date::now() - started,
        SAMPLE_VISITS.get()
    );
    assert_eq!(
        SAMPLE_VISITS.get(),
        0,
        "offscreen foundations need only bounds rejection"
    );
    units.push(test_unit(SceneUnitKind::House));
    SAMPLE_VISITS.set(0);
    apply_building_foundations(&mut tiles, &art, &units);
    assert_eq!(
        SAMPLE_VISITS.get(),
        tiles.len(),
        "one sample scan for the visible foundation"
    );
    assert!(tiles.iter().any(|tile| tile.material == 2));
}

#[wasm_bindgen_test]
fn farm_crop_uvs_keep_both_triangle_halves_when_the_apron_blends() {
    let mut art = crate::surface_mesh::mix::fixture::art();
    art.buildings[5] = vec![art.terrain[2][0]];
    art.compositions.push(crate::BuildingArt {
        unit_id: 50,
        footprint: [3.0; 2],
        foundation_material: Some(17),
        parts: Vec::new(),
        construction: Vec::new(),
    });
    let camera = SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    };
    let farm_unit = SceneUnit {
        id: EntityId(1),
        kind: SceneUnitKind::Farm,
        position: [0.5; 2],
        elevation_meters: 0.0,
        selected: false,
        moving: false,
        facing: 0,
    };
    let mut tiles = (-3..=3)
        .flat_map(|y| {
            (-3..=3).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: Some(SceneTerrainAppearance {
                    canopy_strength: 0,
                    floor_strength: 0,
                    palette: 0,
                    exposure: 0,
                    height_band: 0,
                }),
            })
        })
        .collect::<Vec<_>>();
    apply_building_foundations(&mut tiles, &art, &[farm_unit]);
    let mut faces = projected_surface_triangles(&tiles, camera);
    let modes = faces.iter().map(|f| f.mode()).collect::<Vec<_>>();
    apply_terrain_textures(&mut faces, &art);
    let ground = BuildingGround::new(&[farm_unit], camera);
    let mut checked = 0;
    for (face, mode) in faces.iter_mut().zip(modes) {
        let center =
            [0, 1].map(|axis| face.points.iter().map(|p| p.world[axis]).sum::<f64>() / 3.0);
        if !face.skirt && center.iter().all(|v| (*v - 0.5).abs() < 1.5) {
            ground.farm(face, &art);
            assert_eq!(face.mode(), mode, "crop UV orientation at {:?}", face.tile);
            checked += 1;
        }
    }
    assert_eq!(checked, 18);
}

#[wasm_bindgen_test]
fn distant_farms_do_not_enter_the_per_triangle_candidates() {
    let camera = SceneCamera {
        center: [256.0; 2],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let farm = |id, position, elevation_meters| SceneUnit {
        id: EntityId(id),
        kind: SceneUnitKind::Farm,
        position,
        elevation_meters,
        selected: false,
        moving: false,
        facing: 0,
    };
    let mut units: Vec<_> = (0..100)
        .map(|id| farm(id, [1000.0 + f64::from(id) * 4.0; 2], 0.0))
        .collect();
    assert!(
        BuildingGround::new(&units, camera).farms.is_empty(),
        "100 distant farms must cause zero farm predicates per terrain triangle"
    );
    units.push(farm(100, camera.center, 0.0));
    // A footprint whose center is offscreen can still cover visible ground.
    units.push(farm(
        101,
        projection.screen_to_world(ScreenPoint {
            x: -190.0,
            y: 300.0,
        }),
        0.0,
    ));
    units.push(farm(
        102,
        projection.screen_to_world(ScreenPoint {
            x: -194.0,
            y: 300.0,
        }),
        0.0,
    ));
    // Height-aware projection keeps a raised footprint that reaches the view.
    units.push(farm(
        103,
        projection.screen_to_world_at_height(ScreenPoint { x: 400.0, y: 300.0 }, 30.0),
        30.0,
    ));
    let ground = BuildingGround::new(&units, camera);
    assert_eq!(ground.farms.len(), 3);
    assert!(ground.farms.contains(&units[101].position));
    assert!(!ground.farms.contains(&units[102].position));
    assert!(ground.farms.contains(&units[103].position));
}

fn test_camera() -> SceneCamera {
    SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    }
}
fn test_unit(kind: SceneUnitKind) -> SceneUnit {
    SceneUnit {
        id: EntityId(1),
        kind,
        position: [0.5; 2],
        elevation_meters: 0.0,
        selected: false,
        moving: false,
        facing: 0,
    }
}

#[wasm_bindgen_test]
fn feudal_buildings_paint_native_foundations_and_aprons_only_on_land() {
    for (slot, id) in [
        (26, 87),
        (27, 101),
        (28, 103),
        (29, 84),
        (30, 79),
        (31, 792),
        (44, 133),
    ] {
        let mut art = crate::surface_mesh::mix::fixture::art();
        art.compositions.push(crate::BuildingArt {
            unit_id: id,
            footprint: if id == 792 { [4.0, 1.0] } else { [3.0; 2] },
            foundation_material: None,
            parts: Vec::new(),
            construction: Vec::new(),
        });
        // The trial Dock has no dedicated sheet; also cover DAT-selected terrain.
        for foundation in [None, Some(13)] {
            art.compositions.last_mut().unwrap().foundation_material = foundation;
            for kind in [
                SceneUnitKind::building(slot),
                SceneUnitKind::foundation(id as i16),
            ] {
                for x in [0.5, 2.5] {
                    // footprint, then apron
                    for (water, material) in [
                        (0, 0),
                        (0, 5),
                        (0, 10),
                        (1, 5),
                        (2, 5),
                        (3, 10),
                        (4, 5),
                        (5, 5),
                    ] {
                        let surface = SceneTerrainSurface {
                            water,
                            ..SceneTerrainSurface::flat(0.0)
                        };
                        let appearance = Some(SceneTerrainAppearance {
                            canopy_strength: 700,
                            floor_strength: 800,
                            palette: 2,
                            exposure: 0,
                            height_band: 0,
                        });
                        let mut tiles = [SceneTerrain {
                            position: [x, 0.5],
                            material,
                            elevation_meters: 0.0,
                            surface,
                            appearance,
                        }];
                        apply_building_foundations(&mut tiles, &art, &[test_unit(kind)]);
                        if water == 0 && material == 0 {
                            assert_eq!(
                                tiles[0].material,
                                foundation.unwrap_or(2),
                                "land {kind:?} at {x}"
                            );
                            assert_eq!(tiles[0].appearance.unwrap().floor_strength, 0);
                        } else {
                            assert_eq!(
                                tiles[0].material, material,
                                "water {water}, {kind:?} at {x}"
                            );
                            assert_eq!(tiles[0].appearance, appearance);
                        }
                        assert_eq!(tiles[0].surface, surface);
                    }
                }
            }
        }
    }
}
