use super::*;

#[test]
fn accepted_manual_farm_stays_manual_after_initial_adoption_and_need_change() {
    let (mut w, vassal, workers, _) = wood_realm(1);
    let worker = workers[0];
    let castle = w.vassal_castle(vassal).unwrap();
    w.issue_ungarrison(PlayerId(0), castle, Some(vassal))
        .unwrap();
    let mill = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .next()
        .unwrap()
        .footprint
        .min;
    reserves(&mut w, 0, 30_000, 30_000, 30_000);
    let first = place_near(&mut w, BuildingKind::Farm, mill);
    let first_rect = w.building(first).unwrap().footprint;
    let first_tile = TileCoord::new(first_rect.min.x + 1, first_rect.min.y + 1);
    w.issue_farm(PlayerId(0), worker, first).unwrap();
    let mut initially_farmed = false;
    for _ in 0..2_000 {
        w.advance();
        initially_farmed |= w.unit(worker).unwrap().task == UnitTask::Farming;
        assert_eq!(w.vassal_controller(worker), None);
    }
    assert!(
        initially_farmed,
        "preexisting ordinary Farm work must be real"
    );
    assert_eq!(w.gather_tile(worker), Some(first_tile));
    w.issue_garrison(PlayerId(0), vassal, castle).unwrap();
    for _ in 0..200 {
        w.advance();
        if w.vassal_controller(worker) == Some(vassal) {
            break;
        }
    }
    assert_eq!(w.vassal_controller(worker), Some(vassal));
    assert_eq!(w.gather_tile(worker), Some(first_tile));

    let manual = place_near(&mut w, BuildingKind::Farm, mill);
    assert_ne!(manual, first);
    let rect = w.building(manual).unwrap().footprint;
    let manual_tile = TileCoord::new(rect.min.x + 1, rect.min.y + 1);
    w.issue_farm(PlayerId(0), worker, manual).unwrap();
    assert_eq!(w.vassal_controller(worker), None);
    reserves(&mut w, 30_000, 0, 30_000, 30_000);
    let mut manually_farmed = false;
    for _ in 0..2_000 {
        w.advance();
        assert_eq!(
            w.vassal_controller(worker),
            None,
            "an accepted manual job stays manual while it remains busy"
        );
        assert_eq!(w.gathering_kind(worker), Some(ResourceKind::Food));
        assert_eq!(w.gather_tile(worker), Some(manual_tile));
        manually_farmed |= w.unit(worker).unwrap().task == UnitTask::Farming;
    }
    assert!(
        manually_farmed,
        "the chosen manual Farm must keep producing"
    );
}
