use super::*;

/// Runs only where the ignored original trial data exists (skips otherwise).
#[test]
fn real_trial_fishing_values_include_five_tile_ship_los() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        eprintln!("no local trial data; real-file check skipped");
        return;
    };
    let file = decode(&bytes).unwrap();
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
        row.unit == 133 && row.family.iter().any(|art| art.name == "DOCK2NNW" && art.slp == 388)
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
