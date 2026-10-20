use super::tests::{art, camera, unit};
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn foundations_extend_one_tile_and_retain_the_landscape_mixer() {
    let art = art();
    for kind in [SceneUnitKind::TownCenter, SceneUnitKind::foundation(109)] {
        let mut terrain = (-4..=4)
            .flat_map(|y| {
                (-4..=4).map(move |x| SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 0,
                    elevation_meters: 0.0,
                    surface: SceneTerrainSurface::flat(0.0),
                    appearance: Some(crate::SceneTerrainAppearance {
                        canopy_strength: 700,
                        floor_strength: 800,
                        palette: 2,
                        exposure: 30,
                        height_band: 1,
                    }),
                })
            })
            .collect::<Vec<_>>();
        crate::apply_building_foundations(&mut terrain, &art, &[unit(kind)]);
        assert_eq!(terrain.iter().filter(|t| t.material == 2).count(), 36);
        for tile in terrain.iter().filter(|t| t.material == 2) {
            let appearance = tile
                .appearance
                .expect("foundation participates in ground mixing");
            assert_eq!(appearance.floor_strength, 0, "no forest litter over dirt");
            assert_eq!(appearance.canopy_strength, 0);
        }
        let faces = crate::projected_surface_triangles(&terrain, camera());
        assert!(
            faces.iter().any(|f| f.mix != [0; 6]),
            "map foundations must use noisy ground shares, not metadata-free vertex splats"
        );
    }
}

#[wasm_bindgen_test]
fn dat_foundation_replaces_ground_for_complete_and_unfinished_buildings() {
    let mut art = art();
    art.compositions[0].foundation_material = Some(2);
    art.compositions[0].footprint = [4.0, 4.0];
    let terrain = (-4..5)
        .flat_map(|y| {
            (-4..5).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
        })
        .collect::<Vec<_>>();
    for kind in [SceneUnitKind::TownCenter, SceneUnitKind::foundation(109)] {
        let mut after = terrain.clone();
        crate::apply_building_foundations(&mut after, &art, &[unit(kind)]);
        assert_eq!(after.iter().filter(|t| t.material == 2).count(), 36);
        assert!(
            after
                .iter()
                .zip(&terrain)
                .all(|(a, b)| a.position == b.position && a.surface == b.surface)
        );
        let faces = crate::projected_surface_triangles(&after, camera());
        assert!(
            faces.iter().any(|f| f
                .texture_materials
                .is_some_and(|m| m.contains(&0) && m.contains(&2))),
            "ordinary terrain blending owns the dirt border"
        );
    }
    art.compositions[0].foundation_material = None;
    let mut after = terrain.clone();
    crate::apply_building_foundations(&mut after, &art, &[unit(SceneUnitKind::TownCenter)]);
    assert_eq!(after.iter().filter(|t| t.material == 2).count(), 36);
}

#[wasm_bindgen_test]
fn farm_crops_stay_in_the_footprint_and_neighbouring_margins_do_not_erase_them() {
    let mut art = art();
    art.compositions.push(crate::BuildingArt {
        unit_id: 50,
        footprint: [3.0; 2],
        foundation_material: Some(17),
        parts: Vec::new(),
        construction: Vec::new(),
    });
    let farm = unit(SceneUnitKind::Farm);
    let mut house = unit(SceneUnitKind::House);
    house.position = [3.0, 0.5];
    for units in [vec![farm, house], vec![house, farm]] {
        let mut tiles = [-1.5, 1.5, 2.5].map(|x| SceneTerrain {
            position: [x, 0.5],
            material: 0,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
            appearance: None,
        });
        crate::apply_building_foundations(&mut tiles, &art, &units);
        assert_eq!(tiles.map(|t| t.material), [2, 17, 2]);
    }
}

#[wasm_bindgen_test]
fn every_building_and_construction_has_dirt_even_when_trial_art_is_absent() {
    let mut art = art();
    art.compositions.clear();
    for slot in 0..9 {
        let kind = SceneUnitKind::building(slot);
        let side = kind.building_side().unwrap();
        for kind in [
            kind,
            SceneUnitKind::foundation(kind.building_id().unwrap() as i16),
        ] {
            let mut terrain = vec![
                SceneTerrain {
                    position: [0.5; 2],
                    material: 0,
                    elevation_meters: 0.0,
                    surface: SceneTerrainSurface::flat(0.0),
                    appearance: None,
                },
                SceneTerrain {
                    position: [side + 1.0; 2],
                    material: 0,
                    elevation_meters: 0.0,
                    surface: SceneTerrainSurface::flat(0.0),
                    appearance: None,
                },
            ];
            crate::apply_building_foundations(&mut terrain, &art, &[unit(kind)]);
            assert_eq!(terrain[0].material, 2, "{kind:?}");
            assert_eq!(terrain[1].material, 0, "outside {kind:?}");
        }
    }
}
