//! Garrison healing visits only due occupants and keeps the ordinary clock.
use super::*;

fn housed(sleep: bool, kind: BuildingKind, wounded: bool) -> (GameWorld, EntityId, Vec<EntityId>) {
    let (mut w, building) = shelter(kind);
    w.set_work_sleep(sleep);
    let mut ids = Vec::new();
    for _ in 0..8 {
        let id = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
        if wounded {
            w.units[w.lookup[&id]].state.hp -= 8;
        }
        w.issue_garrison(PlayerId(0), id, building).unwrap();
        w.advance();
        ids.push(id);
    }
    w.economy.tick_work.garrison_units = 0;
    (w, building, ids)
}

fn tick(on: &mut GameWorld, off: &mut GameWorld, n: u32) {
    assert_eq!(on.advance(), off.advance(), "garrison changes at {n}");
    assert_eq!(
        on.canonical_hash_hex(),
        off.canonical_hash_hex(),
        "garrison hash at {n}"
    );
}

#[test]
fn healthy_garrison_visits_match_actual_healing_deadline_events() {
    let (mut on, building, _) = housed(true, BuildingKind::TownCenter, false);
    let (mut off, _, _) = housed(false, BuildingKind::TownCenter, false);
    let mut events = 0;
    for n in 0..300 {
        let before: Vec<_> = off
            .building(building)
            .unwrap()
            .garrison
            .iter()
            .map(|g| g.heal_at)
            .collect();
        tick(&mut on, &mut off, n);
        events += off
            .building(building)
            .unwrap()
            .garrison
            .iter()
            .zip(before)
            .filter(|(g, old)| g.heal_at != *old)
            .count();
    }
    assert!(events > 0, "the window must include real deadline changes");
    assert_eq!(off.economy.tick_work.garrison_units, 2_400);
    assert_eq!(
        on.economy.tick_work.garrison_units, events,
        "only due occupants may be visited"
    );
}

#[test]
fn staggered_garrison_healing_survives_ejection_and_new_arrivals_exactly() {
    let (mut on, building, ids) = housed(true, BuildingKind::Castle, true);
    let (mut off, _, _) = housed(false, BuildingKind::Castle, true);
    for n in 0..1_200 {
        for w in [&mut on, &mut off] {
            if n == 100 {
                w.issue_ungarrison(PlayerId(0), building, Some(ids[3]))
                    .unwrap();
            }
            if n == 150 {
                w.issue_garrison(PlayerId(0), ids[3], building).unwrap();
            }
            if n == 180 {
                let id = w.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
                w.units[w.lookup[&id]].state.hp -= 8;
                w.issue_garrison(PlayerId(0), id, building).unwrap();
            }
        }
        tick(&mut on, &mut off, n);
    }
    let b = on.building(building).unwrap();
    assert_eq!(b.garrison.len(), 9);
    assert!(b.garrison.iter().all(|g| g.unit.hp == g.unit.max_hp));
}

#[test]
fn garrison_sleep_can_toggle_between_healing_deadlines() {
    let (mut on, building, ids) = housed(true, BuildingKind::TownCenter, true);
    let (mut off, _, _) = housed(false, BuildingKind::TownCenter, true);
    for n in 0..400 {
        if n == 50 {
            on.set_work_sleep(false);
        }
        if n == 100 {
            on.set_work_sleep(true);
        }
        for w in [&mut on, &mut off] {
            if n == 200 {
                w.issue_ungarrison(PlayerId(0), building, None).unwrap();
            }
            if n == 250 {
                w.issue_garrison(PlayerId(0), ids[0], building).unwrap();
            }
        }
        tick(&mut on, &mut off, n);
    }
}

#[test]
#[should_panic(expected = "missed wake: garrison")]
fn garrison_detector_rejects_an_unannounced_deadline_change() {
    let (mut w, building, _) = housed(true, BuildingKind::TownCenter, false);
    w.economy.buildings.get_mut(&building).unwrap().garrison[0].heal_at -= 1;
    w.advance();
}
