use super::*;

#[test]
fn navy_table_preserves_original_unit_identity_stats_and_graphics() {
    for (label, id) in [
        ("Galley", 539),
        ("War Galley", 21),
        ("Fire Ship", 529),
        ("Demolition Ship", 527),
        ("Transport Ship", 545),
    ] {
        let file = parse(&fixture_with(0, id)).unwrap();
        let table = table::build(&file, None);
        let row = table
            .rows
            .iter()
            .find(|row| row.unit == id as i32)
            .unwrap_or_else(|| panic!("missing {label} DAT unit {id} from gameplay survey"));
        assert_eq!(row.label, label);
        let source = file.unit(id as i32).unwrap();
        assert_eq!(row.gameplay.as_ref(), Some(source));
        assert_eq!(row.hit_points, Some(25));
        assert_eq!(row.line_of_sight, Some(4.0));
        for role in ["stand", "walk", "attack"] {
            assert!(
                row.graphics.iter().any(|graphic| graphic.role == role),
                "{label} omitted its {role} graphic"
            );
        }
    }
}

/// Private original DAT evidence; portable CI uses synthetic records above.
#[test]
fn real_trial_navy_has_original_stats_and_sixteen_direction_art() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        eprintln!("no local trial data; real-file check skipped");
        return;
    };
    let file = decode(&bytes).unwrap();
    assert_eq!(file.version, "VER 5.7");
    // Measured current original DAT, SHA-256
    // a213ed092071e1abd234181158c198f4a83ded970e308bf312eef66d752c3f2e.
    // Update these pins only when the source/rules change intentionally.
    for (id, name, hp, los, speed, range, blast, reload, wood, gold, seconds) in [
        (539, "SGALY", 120, 7.0, 1.43, 5.0, 0.0, 3.0, 90, 30, 36),
        (21, "GALLY", 135, 8.0, 1.43, 6.0, 0.0, 3.0, 90, 30, 36),
        (529, "FRGAL", 100, 5.0, 1.35, 2.49, 0.0, 0.25, 75, 45, 36),
        (527, "RMSHP", 50, 6.0, 1.60, 0.0, 2.5, 5.0, 70, 50, 31),
        (545, "XPORT", 100, 5.0, 1.45, 0.0, 0.0, 0.0, 125, 0, 46),
    ] {
        let ship = file.unit(id).unwrap();
        assert_eq!(ship.kind, 70);
        assert_eq!(
            (
                ship.id,
                ship.name.as_str(),
                ship.hit_points,
                ship.line_of_sight
            ),
            (id, name, hp, los)
        );
        assert_eq!(ship.train_location, Some(45));
        assert_eq!(ship.train_time, Some(seconds));
        assert_eq!(
            (
                ship.speed,
                ship.attack_range,
                ship.blast_radius,
                ship.reload_time
            ),
            (Some(speed), Some(range), Some(blast), Some(reload))
        );
        assert_eq!(ship.garrison_capacity, if id == 545 { 5 } else { 0 });
        let costs = ship
            .costs
            .iter()
            .filter(|cost| cost.enabled != 0)
            .map(|cost| (cost.resource, cost.amount))
            .collect::<Vec<_>>();
        assert_eq!(
            costs,
            if gold == 0 {
                vec![(1, wood)]
            } else {
                vec![(1, wood), (3, gold)]
            }
        );
        type CombatClasses = &'static [(i16, i16)];
        let (attacks, armours, graphics): (CombatClasses, CombatClasses, [i16; 4]) = match id {
            539 => (
                &[(11, 6), (16, 8), (3, 6), (17, 3)],
                &[(16, 0), (4, 0), (3, 6)],
                [4052, 4056, 4048, 2762],
            ),
            21 => (
                &[(11, 7), (16, 9), (3, 7), (17, 4)],
                &[(16, 0), (4, 0), (3, 6)],
                [4013, 4017, 4009, 813],
            ),
            529 => (
                &[(11, 2), (16, 3), (4, 1), (3, 2)],
                &[(16, 4), (4, 0), (3, 6)],
                [4182, 4004, 4178, 2720],
            ),
            527 => (
                &[(11, 220), (4, 110)],
                &[(16, 3), (4, 0), (3, 3)],
                [4039, 4043, 4173, 4174],
            ),
            545 => (&[], &[(16, 0), (4, 4), (3, 8)], [4093, 4097, 4089, 2789]),
            _ => unreachable!(),
        };
        assert_eq!(ship.attacks, attacks);
        assert_eq!(ship.armours, armours);
        assert_eq!(
            [
                ship.standing[0],
                ship.walking.unwrap()[0],
                ship.attack_graphic.unwrap(),
                ship.dying
            ],
            graphics
        );
        for (role, graphic) in [
            ("stand", Some(ship.standing[0])),
            ("walk", ship.walking.map(|graphics| graphics[0])),
            ("attack", ship.attack_graphic),
            ("die", Some(ship.dying)),
        ] {
            if let Some(graphic) = graphic.filter(|id| *id >= 0) {
                let art = file.graphic(i32::from(graphic)).unwrap();
                assert!(art.frame_count > 0);
                // Demolition attack/death may be effect-only graphics without a body SLP.
                if role == "stand" || role == "walk" {
                    assert!(art.slp_id > 0);
                }
                if role == "stand" || role == "walk" {
                    assert_eq!(art.angle_count, 16);
                }
            }
        }
    }
}

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
