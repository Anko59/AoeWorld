use super::*;

/// Runs only where the ignored original trial data exists (skips otherwise).
#[test]
fn real_trial_fishing_values_include_five_tile_ship_los() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        eprintln!("no local trial data; real-file check skipped");
        return;
    };
    let file = decode(&bytes).unwrap();
    let dock = file.unit(45).unwrap();
    assert_eq!(dock.collision_size.map(|r| r * 2.0), dock.footprint);
    let composition = file.building_composition(45).unwrap();
    assert_eq!(composition.footprint, [3.0; 2]);
    assert_eq!(composition.foundation_terrain, -1);
    assert_eq!(composition.foundation_slp, None);
    assert_eq!(
        composition
            .parts
            .iter()
            .map(|p| (p.slp_id, p.frames))
            .collect::<Vec<_>>(),
        [(374, 1), (376, 1), (4518, 60)]
    );
    assert_eq!(
        composition.foundation_terrain,
        dock.building.as_ref().unwrap().foundation_terrain
    );
    assert!(!composition.parts.is_empty());
    assert!(!composition.construction.is_empty());
    eprintln!("trial Dock composition: {composition:?}");
    let ship = file.unit(13).unwrap();
    assert_eq!(ship.line_of_sight, 5.0);
    assert_eq!(
        (
            ship.work_rate,
            ship.resource_capacity,
            ship.train_time,
            ship.train_location
        ),
        (Some(0.28), 15, Some(40), Some(45))
    );
    assert_eq!(ship.tasks[0].work_values[0], 1.75);
    assert_eq!(file.unit(56).unwrap().work_rate, Some(0.43));
    let dock = file.unit(133).unwrap();
    assert_eq!(dock.name, "DOCK2");
    assert!(table::build(&file, None).rows.iter().any(|row| {
        row.unit == 133
            && row
                .family
                .iter()
                .any(|art| art.name == "DOCK2NNW" && art.slp == 388)
    }));
    for (id, amount) in [
        (53, 200.0),
        (69, 200.0),
        (455, 225.0),
        (456, 225.0),
        (457, 225.0),
        (458, 225.0),
    ] {
        assert_eq!(file.unit(id).unwrap().resource_storage[0], (17, amount));
    }
}

#[test]
fn gameplay_catalog_includes_all_available_feudal_composition_families() {
    for (_, id) in table::FEUDAL {
        assert!(
            crate::catalog::GAMEPLAY_BUILDING_IDS.contains(id),
            "missing Feudal DAT unit {id}"
        );
    }
}

#[test]
fn real_trial_feudal_town_center_composes_feudal_annex_families() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        return;
    };
    let file = decode(&bytes).unwrap();
    let composition = file.building_composition(71).unwrap();
    let ids = composition
        .parts
        .iter()
        .map(|p| p.slp_id)
        .collect::<Vec<_>>();
    for slp in [903, 3608, 4624, 3604, 4620, 3600, 4616] {
        assert!(
            ids.contains(&slp),
            "missing Feudal annex SLP {slp}: {ids:?}"
        );
    }
    for slp in [891, 3595, 4611, 3594, 4610] {
        assert!(
            !ids.contains(&slp),
            "Dark annex leaked into Feudal composition: {slp}"
        );
    }
}
