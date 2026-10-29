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
