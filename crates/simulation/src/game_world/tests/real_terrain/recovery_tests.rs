//! Recovery at the two boundaries where a General first admits a fight.
use super::*;
use crate::game_world::terrain_fixtures::build;

fn enclosure(world: &mut GameWorld) {
    for rect in [
        TileRect::from_xywh(54, 35, 13, 1),
        TileRect::from_xywh(54, 47, 13, 1),
        TileRect::from_xywh(54, 36, 1, 11),
        TileRect::from_xywh(66, 36, 1, 11),
    ] {
        forest(world, rect);
    }
}

fn recovery(enclose_after_acquisition: bool) {
    let mut f = build("general-forest");
    let g = f.primary;
    // The commander approaches into his own sight before the target is sealed.
    let index = f.world.lookup[&g];
    let old = f.world.units[index].state.position;
    f.world.idle_spacing.remove(g, old.tile_floor());
    f.world
        .collisions
        .relocate(g, old.tile_floor(), center(50, 41), false);
    f.world.units[index].state.position = center(50, 41);
    f.world.units[index].state.previous_position = center(50, 41);
    f.world.idle_spacing.insert(g, center(50, 41).tile_floor());
    f.world
        .move_bucket(index, aoe_core::ChunkCoord::from_position(center(50, 41)));
    let enemy = f
        .world
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(60, 41))
        .unwrap();
    f.world
        .issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    if !enclose_after_acquisition {
        enclosure(&mut f.world);
    }
    f.issue();
    if enclose_after_acquisition {
        let mut acquired = false;
        for _ in 0..100 {
            f.world.advance();
            acquired = f.ids.iter().any(|&id| {
                f.world
                    .unit(id)
                    .is_some_and(|u| u.attack_target == Some(enemy))
            });
            if acquired {
                break;
            }
        }
        assert!(acquired, "exercise an already retained reachable target");
        enclosure(&mut f.world);
    }
    let mut previous: Vec<_> = f
        .ids
        .iter()
        .map(|&id| f.world.unit(id).unwrap().position)
        .collect();
    let mut still = vec![0; f.ids.len()];
    let mut longest_still = 0;
    let mut passed = 0;
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
        "recovery after_acquisition={enclose_after_acquisition}: passed={passed}/{} longest_still={longest_still}",
        f.ids.len()
    );
    if passed != f.ids.len() {
        for &id in f.ids.iter().take(12).chain(std::iter::once(&g)) {
            let u = f.world.unit(id).unwrap();
            eprintln!(
                "stalled {id:?} {:?} moving={} planning={} target={:?} error={:?} formation={:?}",
                u.position,
                u.moving,
                u.planning,
                u.attack_target,
                f.world.movement_failure(id),
                f.world.formation(id)
            );
        }
    }
    assert_eq!(
        passed,
        f.ids.len(),
        "retain the march after rejecting the enemy"
    );
    assert!(
        longest_still <= 100,
        "reject an unreachable fight within five seconds"
    );
    assert_eq!(
        f.world.unit(enemy).unwrap().hp,
        f.world.unit(enemy).unwrap().max_hp
    );
}

#[test]
fn general_first_sight_unreachable_enemy_keeps_march() {
    recovery(false);
}

#[test]
fn general_retained_enemy_becoming_unreachable_keeps_march() {
    recovery(true);
}

#[test]
fn recovery_recording_has_a_full_army_and_a_sealed_enemy() {
    for acquired in [false, true] {
        let (mut world, general) =
            GameWorld::movement_recovery_demo(aoe_core::Seed(1), acquired).unwrap();
        assert_eq!(world.general_status(general).unwrap().army_size, 100);
        assert_eq!(
            world.units().filter(|u| u.player == PlayerId(0)).count(),
            101
        );
        assert_eq!(world.buildings().count(), 48);
        let enemy = world.units().find(|u| u.player == PlayerId(1)).unwrap();
        assert_eq!(enemy.hp, enemy.max_hp);
        assert!(
            world
                .economy
                .blocked_tiles
                .contains(&TileCoord::new(132, 128))
        );
        if acquired {
            assert!(
                world
                    .units()
                    .any(|u| u.player == PlayerId(0) && u.attack_target == Some(enemy.id))
            );
        } else {
            world.issue_move(general, center(213, 135)).unwrap();
        }
        for _ in 0..6000 {
            world.advance();
        }
        assert_eq!(
            world
                .units()
                .filter(|u| u.player == PlayerId(0) && u.position.x >= 178 * 1024)
                .count(),
            101
        );
        assert_eq!(world.unit(enemy.id).unwrap().hp, enemy.max_hp);
    }
}

#[test]
fn an_ordinary_offensive_selection_skips_a_sealed_melee_target() {
    let mut f = build("general-forest");
    for id in std::mem::take(&mut f.ids) {
        f.world.kill_unit(f.world.lookup[&id]);
        f.world.remove_unit(id);
    }
    f.world
        .initialize_village(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    f.world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .researched |= crate::Tech::CastleAge.bit();
    let ids: Vec<_> = (0..10)
        .map(|n| {
            f.world
                .spawn_kind(PlayerId(0), UnitKind::Scout, center(52, 37 + n))
                .unwrap()
        })
        .collect();
    let enemy = f.world.spawn_militia(PlayerId(1), center(59, 41)).unwrap();
    f.world
        .issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    enclosure(&mut f.world);
    f.world
        .issue_formation_move(
            PlayerId(0),
            &ids,
            aoe_core::Formation::Line,
            center(100, 41),
            false,
            true,
        )
        .unwrap();
    let starts: Vec<_> = ids
        .iter()
        .map(|&id| f.world.unit(id).unwrap().position)
        .collect();
    for _ in 0..100 {
        f.world.advance();
        for &id in &ids {
            assert_eq!(
                f.world.unit(id).unwrap().attack_target,
                None,
                "ordinary melee acquisition must reject the sealed target"
            );
        }
    }
    assert!(
        ids.iter()
            .zip(starts)
            .any(|(&id, start)| f.world.unit(id).unwrap().position != start)
    );
}
