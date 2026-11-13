//! A General trains only after the Train button, in the slider mix, and stops
//! at capacity until pressed again.
use super::*;

fn production(w: &mut GameWorld) -> (EntityId, EntityId) {
    let barracks = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(100, 80, 3, 3),
            true,
        )
        .unwrap();
    let range = w
        .insert_building(
            PlayerId(0),
            BuildingKind::ArcheryRange,
            aoe_core::TileRect::from_xywh(106, 80, 3, 3),
            true,
        )
        .unwrap();
    (barracks, range)
}

fn queued(w: &GameWorld, building: EntityId) -> Vec<UnitKind> {
    w.building(building)
        .unwrap()
        .training_queue
        .iter()
        .map(|e| e.kind)
        .collect()
}

#[test]
fn general_training_completion_keeps_a_sheltered_commanders_recruit() {
    let (mut w, castle) = setup();
    let (barracks, _) = production(&mut w);
    let general = soldier(&mut w, UnitKind::General, 95, 95);
    w.issue_general_mix(PlayerId(0), general, [100, 0, 0, 0, 0])
        .unwrap();
    w.issue_general_training(PlayerId(0), general, true)
        .unwrap();
    for _ in 0..3 {
        decide(&mut w);
    }
    assert!(w.economy.generals.recruits.contains_key(&barracks));
    w.issue_general_training(PlayerId(0), general, false)
        .unwrap();
    w.issue_garrison(PlayerId(0), general, castle).unwrap();
    advance(&mut w, 1000);
    assert!(
        w.building(castle)
            .unwrap()
            .garrison
            .iter()
            .any(|g| g.unit.id == general)
    );
    let recruit = w
        .units()
        .find(|u| u.kind == UnitKind::Militia)
        .expect("paid recruit finishes");
    assert_eq!(w.general_controller(recruit.id), Some(general));
}

#[test]
fn distant_barracks_cannot_permanently_hide_nearby_training() {
    let (mut w, _) = setup();
    for n in 0..25 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(10 + n % 8 * 4, 10 + n / 8 * 4, 3, 3),
            true,
        )
        .unwrap();
    }
    let (near, _) = production(&mut w);
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    w.issue_general_mix(PlayerId(0), g, [100, 0, 0, 0, 0])
        .unwrap();
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    for _ in 0..3 {
        decide(&mut w);
        assert!(w.general_work().building_visits <= 625 + 128);
    }
    assert!(
        !queued(&w, near).is_empty(),
        "nearby Barracks is eventually visited"
    );
}

#[test]
fn production_scan_progress_is_hashed_and_replayed() {
    let prepare = || {
        let (mut w, _) = setup();
        production(&mut w);
        let g = soldier(&mut w, UnitKind::General, 105, 100);
        w.issue_general_training(PlayerId(0), g, true).unwrap();
        (w, g)
    };
    let (mut a, g) = prepare();
    let (mut b, _) = prepare();
    let initial = a.canonical_hash();
    a.economy
        .generals
        .armies
        .get_mut(&g)
        .unwrap()
        .production_scan[0] = 1;
    assert_ne!(a.canonical_hash(), initial);
    a.economy
        .generals
        .armies
        .get_mut(&g)
        .unwrap()
        .production_scan[0] = 0;
    for _ in 0..6 {
        decide(&mut a);
        decide(&mut b);
        assert_eq!(a.canonical_hash(), b.canonical_hash());
        assert_eq!(a.general_work(), b.general_work());
    }
}

#[test]
fn a_general_buys_nothing_until_train_is_pressed() {
    let (mut w, _) = setup();
    let (barracks, range) = production(&mut w);
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    assert!(!w.general_status(g).unwrap().training);
    for _ in 0..12 {
        decide(&mut w);
    }
    assert!(queued(&w, barracks).is_empty() && queued(&w, range).is_empty());
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    assert!(w.general_status(g).unwrap().training);
    for _ in 0..12 {
        decide(&mut w);
    }
    assert!(!queued(&w, barracks).is_empty() && !queued(&w, range).is_empty());
}

#[test]
fn the_sliders_decide_the_unit_mix_and_zero_means_never() {
    let (mut w, _) = setup();
    let (barracks, range) = production(&mut w);
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    w.issue_general_mix(PlayerId(0), g, [0, 100, 0, 0, 0])
        .unwrap();
    assert_eq!(w.general_status(g).unwrap().mix, [0, 100, 0, 0, 0]);
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    for _ in 0..12 {
        decide(&mut w);
    }
    assert!(queued(&w, barracks).is_empty(), "infantry is set to zero");
    assert!(!queued(&w, range).is_empty());
    // Heavier infantry than archers: infantry is bought first and more often.
    let (mut w, _) = setup();
    let (barracks, range) = production(&mut w);
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    w.issue_general_mix(PlayerId(0), g, [90, 10, 0, 0, 0])
        .unwrap();
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    decide(&mut w);
    assert!(
        !queued(&w, barracks).is_empty(),
        "the larger share goes first"
    );
    assert!(queued(&w, range).is_empty());
}

#[test]
fn slider_orders_are_owner_and_range_checked() {
    let (mut w, _) = setup();
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    let m = soldier(&mut w, UnitKind::Militia, 101, 101);
    assert!(w.issue_general_mix(PlayerId(1), g, [1; 5]).is_err());
    assert!(
        w.issue_general_mix(PlayerId(0), g, [101, 0, 0, 0, 0])
            .is_err()
    );
    assert!(w.issue_general_mix(PlayerId(0), m, [1; 5]).is_err());
    assert!(w.issue_general_training(PlayerId(1), g, true).is_err());
    assert!(w.issue_general_training(PlayerId(0), m, true).is_err());
}

#[test]
fn at_capacity_training_stops_until_the_button_is_pressed_again() {
    let (mut w, _) = setup();
    let (barracks, range) = production(&mut w);
    let recruits: Vec<_> = (0..200)
        .map(|n| soldier(&mut w, UnitKind::Militia, 100 + n % 10, 100 + n / 10))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 105, 98);
    assert_eq!(w.general_status(g).unwrap().army_size, 200);
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    decide(&mut w);
    assert!(
        !w.general_status(g).unwrap().training,
        "full: button released"
    );
    // A loss leaves room, but nothing is bought until Train is pressed again.
    let lost = recruits[0];
    w.kill_unit(w.lookup[&lost]);
    for _ in 0..12 {
        decide(&mut w);
    }
    assert!(queued(&w, barracks).is_empty() && queued(&w, range).is_empty());
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    decide(&mut w);
    // Ten infantry offices remain: one casualty leaves an infantry vacancy,
    // rather than creating an eleventh office for a new archer sergeant.
    assert!(!queued(&w, barracks).is_empty());
    assert!(queued(&w, range).is_empty());
}

#[test]
fn status_reports_the_mix_and_the_army_by_role() {
    let (mut w, _) = setup();
    soldier(&mut w, UnitKind::Militia, 100, 100);
    soldier(&mut w, UnitKind::Militia, 101, 100);
    soldier(&mut w, UnitKind::Archer, 102, 100);
    soldier(&mut w, UnitKind::Knight, 103, 100);
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    let s = w.general_status(g).unwrap();
    assert_eq!(s.counts, [2, 1, 1, 0, 0]);
    assert_eq!(s.mix, [35, 30, 20, 10, 5]);
    assert!(!s.training);
}
