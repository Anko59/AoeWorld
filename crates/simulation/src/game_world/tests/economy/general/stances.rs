use super::*;
use aoe_core::CombatStance;

fn army() -> (GameWorld, EntityId, Vec<(EntityId, CombatStance)>) {
    let (mut w, _) = setup();
    let members = (0..24)
        .map(|n| {
            let id = soldier(&mut w, UnitKind::Knight, 100 + n % 6, 100 + n / 6);
            let stance = [
                CombatStance::Aggressive,
                CombatStance::StandGround,
                CombatStance::Defensive,
                CombatStance::NoAttack,
            ][n as usize % 4];
            w.issue_stance(PlayerId(0), &[id], stance).unwrap();
            (id, stance)
        })
        .collect();
    let g = soldier(&mut w, UnitKind::General, 104, 104);
    (w, g, members)
}

fn retreat(w: &mut GameWorld, g: EntityId) {
    w.units[w.lookup[&g]].state.hp = 1;
    w.spawn_kind(PlayerId(1), UnitKind::Militia, at(110, 104))
        .unwrap();
    decide(w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Retreating);
}

fn restored(w: &GameWorld, members: &[(EntityId, CombatStance)]) {
    for &(id, stance) in members {
        assert_eq!(w.unit(id).unwrap().stance, stance, "soldier {id:?}");
    }
}

#[test]
fn death_restores_every_soldiers_original_stance_after_march_or_retreat() {
    for fleeing in [false, true] {
        let (mut w, g, members) = army();
        w.issue_move(g, at(145, 104)).unwrap();
        decide(&mut w);
        if fleeing {
            retreat(&mut w, g);
        }
        assert_eq!(
            w.unit(members[0].0).unwrap().stance,
            if fleeing {
                CombatStance::NoAttack
            } else {
                CombatStance::Defensive
            }
        );
        w.kill_unit(w.lookup[&g]);
        restored(&w, &members);
        assert!(
            members
                .iter()
                .all(|&(id, _)| w.general_controller(id).is_none())
        );
        decide(&mut w);
        restored(&w, &members);
    }
}

#[test]
fn stop_restores_stances_and_cancels_retreat_without_a_destination() {
    let (mut w, g, members) = army();
    retreat(&mut w, g);
    let enemies: Vec<_> = w
        .units()
        .filter(|u| u.player == PlayerId(1))
        .map(|u| u.id)
        .collect();
    for id in enemies {
        w.kill_unit(w.lookup[&id]);
    }
    w.units[w.lookup[&g]].state.hp = w.unit(g).unwrap().max_hp;
    w.issue_stop(PlayerId(0), g).unwrap();
    restored(&w, &members);
    decide(&mut w);
    restored(&w, &members);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
    assert!(
        members
            .iter()
            .all(|&(id, _)| w.movement_order(id).is_none())
    );
}

#[test]
fn stop_restores_stances_after_marching() {
    let (mut w, g, members) = army();
    w.issue_move(g, at(145, 104)).unwrap();
    decide(&mut w);
    w.issue_stop(PlayerId(0), g).unwrap();
    restored(&w, &members);
    decide(&mut w);
    restored(&w, &members);
}

#[test]
fn garrisoning_the_general_restores_every_soldier() {
    let (mut w, castle) = setup();
    let id = soldier(&mut w, UnitKind::Knight, 100, 100);
    w.issue_stance(PlayerId(0), &[id], CombatStance::StandGround)
        .unwrap();
    let g = soldier(&mut w, UnitKind::General, 94, 92);
    w.issue_move(g, at(145, 104)).unwrap();
    decide(&mut w);
    assert_eq!(w.unit(id).unwrap().stance, CombatStance::Defensive);
    w.issue_garrison(PlayerId(0), g, castle).unwrap();
    w.advance_garrisons(&mut Vec::new());
    assert!(w.unit(g).is_none());
    assert_eq!(w.unit(id).unwrap().stance, CombatStance::StandGround);
    decide(&mut w);
    assert_eq!(w.unit(id).unwrap().stance, CombatStance::StandGround);
}

#[test]
fn march_arrival_restores_stance() {
    let (mut w, _) = setup();
    let id = soldier(&mut w, UnitKind::Militia, 100, 100);
    let g = soldier(&mut w, UnitKind::General, 102, 100);
    w.issue_move(g, at(112, 100)).unwrap();
    advance(&mut w, 1000);
    assert!(!w.unit(id).unwrap().moving);
    assert_eq!(w.unit(id).unwrap().stance, CombatStance::Aggressive);
}

#[test]
fn a_favourable_fight_after_retreat_restores_members_outside_the_micro_window() {
    let (mut w, g, members) = army();
    retreat(&mut w, g);
    w.units[w.lookup[&g]].state.hp = w.unit(g).unwrap().max_hp;
    w.tick.0 += 100;
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
    restored(&w, &members);
}

#[test]
fn retreat_without_a_destination_restores_the_whole_army_when_safe() {
    let (mut w, g, members) = army();
    retreat(&mut w, g);
    let enemy = w.units().find(|u| u.player == PlayerId(1)).unwrap().id;
    w.kill_unit(w.lookup[&enemy]);
    w.units[w.lookup[&g]].state.hp = w.unit(g).unwrap().max_hp;
    w.tick.0 += 100;
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
    restored(&w, &members);
}

#[test]
fn wounded_withdrawal_restores_on_healing_and_on_fight_end_without_a_destination() {
    for healed in [false, true] {
        let (mut w, g, members) = army();
        let (id, original) = members[0];
        w.units[w.lookup[&id]].state.hp = 1;
        let enemy = w
            .spawn_kind(PlayerId(1), UnitKind::Militia, at(110, 104))
            .unwrap();
        decide(&mut w);
        assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
        assert_eq!(w.unit(id).unwrap().stance, CombatStance::NoAttack);
        if healed {
            w.units[w.lookup[&id]].state.hp = w.unit(id).unwrap().max_hp;
        } else {
            w.kill_unit(w.lookup[&enemy]);
        }
        decide(&mut w);
        assert_eq!(w.unit(id).unwrap().stance, original);
        // The second admission's micro window excludes the first soldier.
        restored(&w, &members);
    }
}

#[test]
fn release_and_general_removal_restore_stances_without_another_adopter() {
    for remove_leader in [false, true] {
        let (mut w, g, members) = army();
        w.issue_move(g, at(145, 104)).unwrap();
        decide(&mut w);
        if remove_leader {
            w.remove_unit(g);
            restored(&w, &members);
        } else {
            let (id, original) = members[1];
            w.forget_general(id);
            assert_eq!(w.general_controller(id), None);
            assert_eq!(w.unit(id).unwrap().stance, original);
        }
    }
}

#[test]
fn canonical_hash_includes_the_stance_to_restore() {
    fn run(original: CombatStance) -> (GameWorld, EntityId) {
        let (mut w, _) = setup();
        let id = soldier(&mut w, UnitKind::Militia, 100, 100);
        w.issue_stance(PlayerId(0), &[id], original).unwrap();
        let g = soldier(&mut w, UnitKind::General, 102, 102);
        w.issue_move(g, at(145, 103)).unwrap();
        decide(&mut w);
        (w, id)
    }
    let (a, id) = run(CombatStance::Aggressive);
    let (b, _) = run(CombatStance::StandGround);
    assert_eq!(a.unit(id), b.unit(id));
    assert_ne!(a.canonical_hash(), b.canonical_hash());
}
