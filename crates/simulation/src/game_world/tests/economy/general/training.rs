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
    let recruits: Vec<_> = (0..100)
        .map(|n| soldier(&mut w, UnitKind::Militia, 100 + n % 10, 100 + n / 10))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 105, 98);
    assert_eq!(w.general_status(g).unwrap().army_size, 100);
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
    // The 99 Militia leave archers furthest below their share.
    assert!(!queued(&w, range).is_empty());
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
