use super::*;

#[wasm_bindgen_test]
fn dense_foundation_painting_only_tests_nearby_candidates() {
    let art = crate::surface_mesh::mix::fixture::art();
    let mut samples: Vec<_> = (-48..48)
        .flat_map(|y| {
            (-48..48).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
        })
        .collect();
    let units: Vec<_> = (-8..8)
        .flat_map(|y| {
            (-8..8).map(move |x| SceneUnit {
                position: [f64::from(x * 6), f64::from(y * 6)],
                ..test_unit(SceneUnitKind::House)
            })
        })
        .collect();
    SAMPLE_VISITS.set(0);
    apply_building_foundations(&mut samples, &art, &units);
    for tile in &samples {
        let expected = units.iter().any(|unit| {
            (0..2).all(|axis| {
                tile.position[axis] >= unit.position[axis] - 2.0
                    && tile.position[axis] < unit.position[axis] + 2.0
            })
        });
        assert_eq!(
            tile.material,
            if expected { 2 } else { 0 },
            "{:?}",
            tile.position
        );
    }
    assert!(
        SAMPLE_VISITS.get() <= samples.len() * 4,
        "{} predicate checks for {} samples",
        SAMPLE_VISITS.get(),
        samples.len()
    );
}

#[wasm_bindgen_test]
fn overlapping_foundation_precedence_survives_bucket_boundaries() {
    let mut art = crate::surface_mesh::mix::fixture::art();
    art.compositions.push(crate::BuildingArt {
        unit_id: 50,
        footprint: [3.0; 2],
        foundation_material: Some(17),
        parts: Vec::new(),
        states: Vec::new(),
        construction: Vec::new(),
        damage: Vec::new(),
    });
    let farm = SceneUnit {
        position: [8.0, 0.0],
        ..test_unit(SceneUnitKind::Farm)
    };
    let house = SceneUnit {
        position: [9.5, 0.0],
        ..test_unit(SceneUnitKind::House)
    };
    let tile = |x| SceneTerrain {
        position: [x, 0.0],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: None,
    };
    let mut tiles = [tile(7.5), tile(8.25), tile(9.0)];
    apply_building_foundations(&mut tiles, &art, &[farm, house]);
    assert_eq!(tiles.map(|tile| tile.material), [17, 17, 2]);
    apply_building_foundations(&mut tiles, &art, &[house, farm]);
    assert_eq!(tiles.map(|tile| tile.material), [17, 17, 17]);
}

#[wasm_bindgen_test]
fn foundation_painting_checks_only_the_local_bucket_for_a_single_building() {
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
        64,
        "only the building's eight-by-eight bucket is visited"
    );
    assert!(tiles.iter().any(|tile| tile.material == 2));
}
