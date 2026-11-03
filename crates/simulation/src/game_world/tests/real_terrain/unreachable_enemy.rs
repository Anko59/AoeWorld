//! Seeing an enemy behind an impassable enclosure must not cancel a march.
use super::*;
use crate::game_world::terrain_fixtures::build;

#[test]
fn attack_move_drops_a_visible_enclosed_enemy_and_continues() {
    let mut f = build("forest");
    for rect in [
        TileRect::from_xywh(68, 46, 5, 1),
        TileRect::from_xywh(68, 50, 5, 1),
        TileRect::from_xywh(68, 47, 1, 3),
        TileRect::from_xywh(72, 47, 1, 3),
    ] {
        forest(&mut f.world, rect);
    }
    let id = f
        .world
        .spawn_kind(PlayerId(0), UnitKind::Militia, center(62, 48))
        .unwrap();
    let enemy = f
        .world
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(70, 48))
        .unwrap();
    f.world
        .issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let goal = center(110, 48);
    f.world
        .issue_formation_move(
            PlayerId(0),
            &[id],
            aoe_core::Formation::Line,
            goal,
            false,
            true,
        )
        .unwrap();
    let mut targeted_ticks = 0;
    let mut still = 0;
    let mut longest_still = 0;
    let mut previous = f.world.unit(id).unwrap().position;
    let mut arrived = None;
    for tick in 1..=6000 {
        f.world.advance();
        let u = f.world.unit(id).unwrap();
        targeted_ticks += u32::from(u.attack_target == Some(enemy));
        still = if previous == u.position { still + 1 } else { 0 };
        longest_still = longest_still.max(still);
        previous = u.position;
        if u.position == goal && !u.moving && !u.planning {
            arrived = Some(tick);
            break;
        }
    }
    eprintln!(
        "enclosed enemy: arrived={arrived:?} targeted_ticks={targeted_ticks} longest_still={longest_still}"
    );
    assert!(arrived.is_some(), "the retained march must continue");
    assert!(
        targeted_ticks <= 100,
        "unreachable enemies must be dropped, not reacquired forever"
    );
    assert!(
        longest_still <= 100,
        "a rejected fight must not freeze the march"
    );
    assert_eq!(
        f.world.unit(enemy).unwrap().hp,
        f.world.unit(enemy).unwrap().max_hp
    );
}

#[test]
fn general_army_continues_past_a_visible_enclosed_enemy() {
    let mut f = build("general-forest");
    for rect in [
        TileRect::from_xywh(54, 35, 13, 1),
        TileRect::from_xywh(54, 47, 13, 1),
        TileRect::from_xywh(54, 36, 1, 11),
        TileRect::from_xywh(66, 36, 1, 11),
    ] {
        forest(&mut f.world, rect);
    }
    let enemy = f
        .world
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(60, 41))
        .unwrap();
    f.world
        .issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let mut previous: Vec<_> = f
        .ids
        .iter()
        .map(|&id| f.world.unit(id).unwrap().position)
        .collect();
    let mut still = vec![0; f.ids.len()];
    let mut longest_still = 0;
    let mut passed = 0;
    f.issue();
    for _ in 0..12_000 {
        f.world.advance();
        passed = 0;
        for (n, &id) in f.ids.iter().enumerate() {
            let u = f.world.unit(id).unwrap();
            if u.position.x >= 100 * 1024 {
                passed += 1;
                continue;
            }
            still[n] = if u.position == previous[n] {
                still[n] + 1
            } else {
                0
            };
            longest_still = longest_still.max(still[n]);
            previous[n] = u.position;
        }
        if passed == f.ids.len() {
            break;
        }
    }
    eprintln!(
        "general enclosed enemy: passed={passed}/{} longest_still={longest_still}",
        f.ids.len()
    );
    assert_eq!(
        passed,
        f.ids.len(),
        "an unreachable sighting must not consume the army's march"
    );
    assert!(
        longest_still <= 100,
        "drop an unreachable fight within five seconds"
    );
    assert_eq!(
        f.world.unit(enemy).unwrap().hp,
        f.world.unit(enemy).unwrap().max_hp
    );
}
