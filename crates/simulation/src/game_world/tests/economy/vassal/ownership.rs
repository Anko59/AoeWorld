use super::*;
use aoe_core::{CombatStance, Formation};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn soldier() -> (GameWorld, EntityId, EntityId, EntityId) {
    let (mut w, v, castle) = setup();
    w.set_fog_disabled(true);
    let id = w
        .spawn_kind(PlayerId(0), UnitKind::Knight, at(98, 100))
        .unwrap();
    w.issue_stance(PlayerId(0), &[id], CombatStance::NoAttack)
        .unwrap();
    advance(&mut w, 20);
    assert_eq!(w.vassal_controller(id), Some(v));
    (w, v, castle, id)
}

#[test]
fn vassal_owner_repeated_attack_releases_control() {
    let (mut w, _, _, id) = soldier();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(140, 100))
        .unwrap();
    w.economy.stewards.automatic = true;
    w.issue_attack(PlayerId(0), id, enemy).unwrap();
    w.economy.stewards.automatic = false;
    assert!(w.vassal_controller(id).is_some());
    w.issue_attack(PlayerId(0), id, enemy).unwrap();
    assert_eq!(w.vassal_controller(id), None);
    advance(&mut w, 40);
    assert_eq!(w.unit(id).unwrap().attack_target, Some(enemy));
}

#[test]
fn vassal_owner_deferred_attack_releases_control() {
    let (mut w, _, _, id) = soldier();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(140, 100))
        .unwrap();
    while w.reserve_attack_search() {}
    w.issue_attack(PlayerId(0), id, enemy).unwrap();
    assert_eq!(w.vassal_controller(id), None);
    advance(&mut w, 40);
    assert_eq!(w.unit(id).unwrap().attack_target, Some(enemy));
}

#[test]
fn vassal_preserves_idle_general_army_inside_its_territory() {
    let (mut w, v, _) = setup();
    let ids: Vec<_> = (0..12)
        .map(|i| {
            w.spawn_kind(PlayerId(0), UnitKind::Knight, at(98 + i % 4, 94 + i / 4))
                .unwrap()
        })
        .collect();
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(98, 92))
        .unwrap();
    for id in &ids {
        assert_eq!(w.general_controller(*id), Some(general));
        assert_eq!(w.unit(*id).unwrap().task, UnitTask::Idle);
    }
    advance(&mut w, 100);
    for id in ids {
        assert_eq!(
            w.general_controller(id),
            Some(general),
            "idle army member {id:?} was stolen"
        );
        assert_eq!(w.vassal_controller(id), None);
    }
    assert_eq!(w.general_status(general).unwrap().army_size, 12);
    // The 100-tile domain reaches the village: only its idle scout may serve.
    assert!(w.vassal_status(v).unwrap().military <= 1);
}

#[test]
fn vassal_recall_and_return_preserve_every_surviving_general_member() {
    let (mut w, general) = GameWorld::vassal_defense_demo(Seed(7)).unwrap();
    let army: Vec<_> = w
        .units()
        .filter(|u| w.general_controller(u.id) == Some(general))
        .map(|u| u.id)
        .collect();
    assert_eq!(army.len(), 12);
    let home = w.unit(general).unwrap().position.tile_floor();
    let mut called = false;
    let mut returned = false;
    for _ in 0..6_000 {
        w.advance();
        called |= w
            .buildings()
            .flat_map(|b| &b.garrison)
            .any(|g| g.vassal.is_some_and(|s| s.responding > 0));
        if !called {
            continue;
        }
        for &id in &army {
            if w.unit(id).is_some_and(|u| u.hp > 0) {
                assert_eq!(
                    w.general_controller(id),
                    Some(general),
                    "recall lost army member {id:?} at {:?}",
                    w.tick()
                );
                assert_eq!(w.vassal_controller(id), None);
            }
        }
        let g = w.unit(general).unwrap();
        if g.position.tile_floor() == home
            && !g.moving
            && w.general_status(general).unwrap().state == crate::GeneralState::Idle
        {
            returned = true;
            break;
        }
    }
    assert!(
        called && returned,
        "the recall and physical return must both execute"
    );
    advance(&mut w, 100);
    for id in army {
        if w.unit(id).is_some_and(|u| u.hp > 0) {
            assert_eq!(w.general_controller(id), Some(general));
            assert_eq!(w.vassal_controller(id), None);
        }
    }
}

#[test]
fn vassal_owner_attack_outside_territory_keeps_the_owner_target() {
    let (mut w, _, _, id) = soldier();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(140, 100))
        .unwrap();
    w.issue_attack(PlayerId(0), id, enemy).unwrap();
    advance(&mut w, 40);
    assert_eq!(w.vassal_controller(id), None);
    assert_eq!(w.unit(id).unwrap().attack_target, Some(enemy));
}

#[test]
fn vassal_owner_stop_is_not_recruited_again_even_during_an_alert() {
    let (mut w, v, _, id) = soldier();
    w.issue_stop(PlayerId(0), id).unwrap();
    w.spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 92))
        .unwrap();
    advance(&mut w, 100);
    assert!(w.vassal_status(v).unwrap().alert);
    assert_eq!(
        w.vassal_controller(id),
        None,
        "Stop must survive future recruitment passes"
    );
    assert_eq!(w.unit(id).unwrap().task, UnitTask::Idle);
    assert_eq!(w.unit(id).unwrap().attack_target, None);
}

#[test]
fn vassal_all_accepted_soldier_orders_release_and_keep_manual_control() {
    for (command, name) in [
        "move",
        "queued move",
        "attack",
        "repeated attack",
        "deferred attack",
        "stop",
        "stance",
        "formation move",
        "queued formation",
        "formation posture",
        "garrison",
        "unchanged scouting",
        "enable scouting",
    ]
    .into_iter()
    .enumerate()
    {
        let (mut w, _, castle, id) = soldier();
        let enemy = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(140, 100))
            .unwrap();
        // Exercise accepted shortcuts as well as the normal order paths.
        if matches!(command, 1 | 8 | 9) {
            w.economy.stewards.automatic = true;
            if command == 1 {
                w.issue_move(id, at(105, 100)).unwrap();
            } else {
                w.issue_formation_move(
                    PlayerId(0),
                    &[id],
                    Formation::Line,
                    at(105, 100),
                    false,
                    false,
                )
                .unwrap();
            }
            w.economy.stewards.automatic = false;
        }
        if command == 3 {
            w.economy.stewards.automatic = true;
            w.issue_attack(PlayerId(0), id, enemy).unwrap();
            w.economy.stewards.automatic = false;
        }
        if command == 4 {
            while w.reserve_attack_search() {}
        }
        match command {
            0 => {
                w.issue_move(id, at(105, 100)).unwrap();
            }
            1 => {
                w.issue_queued_move(id, at(107, 100)).unwrap();
            }
            2..=4 => {
                w.issue_attack(PlayerId(0), id, enemy).unwrap();
            }
            5 => w.issue_stop(PlayerId(0), id).unwrap(),
            6 => w
                .issue_stance(PlayerId(0), &[id], CombatStance::StandGround)
                .unwrap(),
            7 | 8 => w
                .issue_formation_move(
                    PlayerId(0),
                    &[id],
                    Formation::Box,
                    at(107, 100),
                    command == 8,
                    true,
                )
                .unwrap(),
            9 => w
                .issue_formation(PlayerId(0), &[id], Formation::Box)
                .unwrap(),
            10 => w.issue_garrison(PlayerId(0), id, castle).unwrap(),
            11 => w.issue_auto_scout(PlayerId(0), id, false).unwrap(),
            12 => w.issue_auto_scout(PlayerId(0), id, true).unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(
            w.vassal_controller(id),
            None,
            "accepted owner command {name} must release control"
        );
        w.issue_stop(PlayerId(0), id).unwrap();
        advance(&mut w, 60);
        assert_eq!(
            w.vassal_controller(id),
            None,
            "owner command {name} must stay released after becoming idle"
        );
    }
}

#[test]
fn vassal_owner_garrison_and_ungarrison_keep_the_soldier_manual() {
    let (mut w, _, castle, id) = soldier();
    w.issue_garrison(PlayerId(0), id, castle).unwrap();
    advance(&mut w, 300);
    assert!(w.unit(id).is_none());
    assert!(
        w.building(castle)
            .unwrap()
            .garrison
            .iter()
            .any(|g| g.unit.id == id)
    );
    w.issue_ungarrison(PlayerId(0), castle, Some(id)).unwrap();
    advance(&mut w, 100);
    assert!(w.unit(id).is_some());
    assert_eq!(
        w.vassal_controller(id),
        None,
        "garrison storage preserves the soldier's identity and owner orders"
    );
}

#[test]
fn vassal_rejected_owner_orders_do_not_release_control() {
    let (mut w, v, _, id) = soldier();
    assert!(w.issue_stop(PlayerId(1), id).is_err());
    assert!(w.issue_attack(PlayerId(0), id, EntityId(u32::MAX)).is_err());
    assert!(
        w.issue_stance(PlayerId(1), &[id], CombatStance::StandGround)
            .is_err()
    );
    assert!(
        w.issue_formation_move(
            PlayerId(1),
            &[id],
            Formation::Box,
            at(105, 100),
            false,
            false
        )
        .is_err()
    );
    assert_eq!(w.vassal_controller(id), Some(v));
}

#[test]
fn vassal_automatic_defence_keeps_control_of_its_soldiers() {
    let (mut w, _, _, id) = soldier();
    w.spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 92))
        .unwrap();
    advance(&mut w, 40);
    assert!(w.vassal_controller(id).is_some());
    assert!(w.unit(id).unwrap().attack_target.is_some());
}

#[test]
fn vassal_manual_recruitment_exclusion_is_hashed_and_removed_on_death() {
    let (mut manual, _, _, id) = soldier();
    let (mut automatic, _, _, other) = soldier();
    assert_eq!(manual.canonical_hash(), automatic.canonical_hash());
    manual.issue_stop(PlayerId(0), id).unwrap();
    automatic.release_vassal_unit(other);
    automatic.issue_stop(PlayerId(0), other).unwrap();
    assert_ne!(
        manual.canonical_hash(),
        automatic.canonical_hash(),
        "manual recruitment exclusion affects future state"
    );
    manual.issue_delete(PlayerId(0), &[id]).unwrap();
    automatic.issue_delete(PlayerId(0), &[other]).unwrap();
    assert_eq!(
        manual.canonical_hash(),
        automatic.canonical_hash(),
        "dead unit exclusions must not accumulate"
    );
}

#[test]
fn vassal_ownership_replay_agrees_with_one_and_four_workers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (mut w, _) = GameWorld::vassal_defense_demo(Seed(7)).unwrap();
                let soldier = w.units().find(|u| u.kind == UnitKind::Militia).unwrap().id;
                let mut hashes = Vec::new();
                for tick in 0..6_000 {
                    w.advance();
                    if tick == 40 {
                        assert!(w.vassal_controller(soldier).is_some());
                        w.issue_stop(PlayerId(0), soldier).unwrap();
                        hashes.push(w.canonical_hash());
                    }
                    if tick == 4_000 {
                        hashes.push(w.canonical_hash());
                    }
                }
                hashes.push(w.canonical_hash());
                hashes
            })
    };
    assert_eq!(
        run(1),
        run(4),
        "Vassal manual control, recall and return must agree with one and four workers"
    );
}

#[test]
fn active_vassal_expands_the_castle_claim_to_its_domain_radius() {
    let (mut w, _, _) = setup();
    w.set_fog_disabled(true);
    let held = w.land_control(PlayerId(0));
    assert!(
        held.shapes
            .iter()
            .any(|s| s.y == 92 && s.x0 <= 69 && s.x1 > 69)
    );
}
