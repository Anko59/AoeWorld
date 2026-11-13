use super::*;
use aoe_core::Formation;

#[test]
fn formations_attack_move_notices_contact_at_the_outer_wing() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 200);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Flank,
        center(180, 50),
        false,
        true,
    )
    .unwrap();
    // The last ranks begin far from the army's centre.
    let wing = w.unit(*ids.last().unwrap()).unwrap().position.tile_floor();
    let enemy = w
        .spawn_unit(PlayerId(1), center(wing.x + 2, wing.y))
        .unwrap();
    // Keep the contact alive through the snapshot. Only members that saw
    // this enemy before leaving may be absent from the marching group.
    let index = w.lookup[&enemy];
    w.units[index].state.hp = 10_000;
    w.units[index].state.max_hp = 10_000;
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let mut saw_enemy = std::collections::BTreeSet::new();
    for _ in 0..120 {
        let marching = ids
            .iter()
            .copied()
            .filter(|&id| w.formation(id).is_some())
            .collect::<Vec<_>>();
        let target = w.unit(enemy).unwrap().position;
        for &id in &ids {
            let unit = w.unit(id).unwrap();
            let (_, _, sight) = w.unit_observer(&unit).unwrap();
            let reach = i64::from(sight) * 1024;
            if w.formation(id).is_some()
                && super::super::super::movement_step::slots::squared(unit.position, target)
                    <= reach * reach
            {
                saw_enemy.insert(id);
            }
        }
        w.advance();
        for &id in &marching {
            if w.formation(id).is_none() {
                let unit = w.unit(id).unwrap();
                let (_, _, sight) = w.unit_observer(&unit).unwrap();
                let reach = i64::from(sight) * 1024;
                assert!(
                    super::super::super::movement_step::slots::squared(unit.position, target)
                        <= reach * reach,
                    "member {id:?} left beyond its own sight"
                );
                saw_enemy.insert(id);
            }
        }
    }
    let fighters = ids
        .iter()
        .filter(|&&id| w.unit(id).unwrap().attack_target == Some(enemy))
        .count();
    let left = ids.iter().filter(|&&id| w.formation(id).is_none()).count();
    assert!(fighters > 0 && fighters <= left && left <= saw_enemy.len());
    eprintln!(
        "wing snapshot: fighters={fighters} left={left} observers={}",
        saw_enemy.len()
    );
    assert_eq!(w.formation(ids[0]), Some(Formation::Flank));
}

#[test]
fn contact_reaction_retains_an_offensive_groups_destination_slot() {
    let mut w = wide_world();
    let ids = army(&mut w, 3);
    let enemy = w.spawn_militia(PlayerId(1), center(44, 40)).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        aoe_core::Formation::Line,
        center(140, 40),
        false,
        true,
    )
    .unwrap();
    let slot = w.formation_destination_slot(ids[0]).unwrap();
    let origin = w.unit(ids[0]).unwrap().position;
    // The shared contact/retaliation path must keep the same continuation as
    // the periodic offensive formation look, for any player's group.
    w.begin_autonomous_attack(ids[0], enemy, origin);
    assert_eq!(w.combat.attackers[&ids[0]].resume, Some(slot));
    w.kill_unit(w.lookup[&enemy]);
    for _ in 0..3500 {
        w.advance();
    }
    // Resumption is an ordinary one-member order, including its ground snap.
    let resolved = w.config.snap_ground_position(slot);
    assert_eq!(w.formation_destination_slot(ids[0]), Some(resolved));
    assert_eq!(w.unit(ids[0]).unwrap().position, resolved);
}

#[test]
fn an_idle_members_contact_reaction_keeps_its_offensive_resume() {
    let mut w = wide_world();
    let ids = army(&mut w, 3);
    let enemy = w.spawn_militia(PlayerId(1), center(42, 40)).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        aoe_core::Formation::Line,
        center(140, 40),
        false,
        true,
    )
    .unwrap();
    let slot = w.formation_destination_slot(ids[0]).unwrap();
    // A stopped member reacts through ordinary idle retaliation. Moving
    // members use the formation's bounded engage look, regardless of owner.
    let index = w.lookup[&ids[0]];
    w.units[index].state.moving = false;
    w.units[index].state.planning = false;
    w.units[index].state.task = UnitTask::Idle;
    w.retaliate(index, enemy, &mut Vec::new());
    assert_eq!(w.unit(ids[0]).unwrap().attack_target, Some(enemy));
    assert_eq!(w.combat.attackers[&ids[0]].resume, Some(slot));
}

#[test]
fn ordinary_player_groups_can_share_a_pace_without_a_general() {
    let run = |limit| {
        let mut w = wide_world();
        let id = w.spawn_militia(PlayerId(0), center(40, 40)).unwrap();
        w.issue_formation_move(
            PlayerId(0),
            &[id],
            Formation::Line,
            center(180, 40),
            false,
            true,
        )
        .unwrap();
        if let Some(pace) = limit {
            let before = w.canonical_hash();
            assert!(w.issue_formation_pace(PlayerId(1), id, Some(pace)).is_err());
            assert!(
                w.issue_formation_pace(PlayerId(0), id, Some((0, 1)))
                    .is_err()
            );
            assert_eq!(before, w.canonical_hash());
            w.issue_formation_pace(PlayerId(0), id, Some(pace)).unwrap();
            assert_ne!(before, w.canonical_hash(), "pace is deterministic state");
        }
        for _ in 0..100 {
            w.advance();
        }
        (
            w.unit(id).unwrap().position.x,
            w.formation_anchor(id).unwrap().x,
        )
    };
    let (normal, _) = run(None);
    let (limited, anchor) = run(Some((15, 1)));
    assert!(
        limited < normal,
        "ordinary coordination slows this selection"
    );
    assert!(
        anchor - center(40, 40).x <= 1500,
        "the route anchor respects the cap"
    );
    assert!(
        limited > center(40, 40).x,
        "the coordinated group keeps advancing"
    );
}
