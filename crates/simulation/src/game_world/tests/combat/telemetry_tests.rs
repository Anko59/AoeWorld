//! Telemetry follows resolved damage and survives entity cleanup.
use super::*;

#[test]
fn melee_telemetry_retains_kills_after_cleanup_and_expires_an_idle_minute() {
    let (mut world, attacker, victim) = duel();
    world.units[world.lookup[&victim]].state.hp = 1;
    world.issue_attack(PlayerId(0), attacker, victim).unwrap();
    world.advance();
    let counts = world.combat_telemetry();
    assert_eq!(
        (counts.melee_hits, counts.ranged_hits, counts.deaths),
        (1, 0, 1)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 1));
    assert_eq!(world.player_combat_totals(PlayerId(99)), (0, 0));
    advance(&mut world, 650);
    assert!(
        world.unit(victim).is_none(),
        "corpse cleanup must precede the minute boundary"
    );
    assert_eq!(world.combat_telemetry().deaths, 1);
    advance(&mut world, 600);
    let counts = world.combat_telemetry();
    assert_eq!(
        (counts.melee_hits, counts.ranged_hits, counts.deaths),
        (0, 0, 0)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 1));
}

#[test]
fn ranged_telemetry_counts_impact_once_and_never_counts_a_missed_arrow() {
    let mut world = world();
    let archer = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Archer, at(100, 100))
        .unwrap();
    let victim = world.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    world.units[world.lookup[&victim]].state.hp = 1;
    world.issue_attack(PlayerId(0), archer, victim).unwrap();
    world.advance();
    assert_eq!(world.projectiles().count(), 1);
    assert_eq!(
        world.combat_telemetry().ranged_hits,
        0,
        "launch is not a hit"
    );
    advance(&mut world, 12);
    let counts = world.combat_telemetry();
    assert_eq!(
        (counts.melee_hits, counts.ranged_hits, counts.deaths),
        (0, 1, 1)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    let other = world.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    world.issue_attack(PlayerId(0), archer, other).unwrap();
    ticks_until(&mut world, 100, |w| w.projectiles().count() == 1);
    world.remove_unit(other);
    advance(&mut world, 20);
    assert_eq!(
        world.combat_telemetry().ranged_hits,
        1,
        "an absent target takes no damage"
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
}

#[test]
fn telemetry_counts_destroyed_buildings_but_excludes_wildlife() {
    let mut world = world();
    let attacker = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let building = world
        .place_town_center(PlayerId(1), TileCoord::new(101, 100))
        .unwrap();
    world.economy.buildings.get_mut(&building).unwrap().hp = 1;
    world.issue_attack(PlayerId(0), attacker, building).unwrap();
    world.advance();
    assert!(world.building(building).is_none());
    assert_eq!(world.combat_telemetry().deaths, 1);
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
    assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 1));
    let before = world.combat_telemetry();
    let sheep = world
        .spawn_animal(crate::AnimalKind::Sheep, at(99, 100))
        .unwrap();
    world.issue_attack(PlayerId(0), attacker, sheep).unwrap();
    ticks_until(&mut world, 100, |w| w.animal(sheep).unwrap().hp == 0);
    let after = world.combat_telemetry();
    assert_eq!(
        (after.melee_hits, after.ranged_hits, after.deaths),
        (before.melee_hits, before.ranged_hits, before.deaths)
    );
    assert_eq!(world.player_combat_totals(PlayerId(0)), (1, 0));
}

#[test]
fn wildlife_killing_a_player_unit_counts_hit_death_and_loss_without_player_kill_credit() {
    for kind in [crate::AnimalKind::Wolf, crate::AnimalKind::Boar] {
        let mut world = world();
        let victim = world.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
        world.units[world.lookup[&victim]].state.hp = 1;
        let animal = world.spawn_animal(kind, at(101, 100)).unwrap();
        if kind == crate::AnimalKind::Boar {
            world.issue_hunt(PlayerId(0), &[victim], animal).unwrap();
        }
        ticks_until(&mut world, 500, |w| w.unit(victim).unwrap().hp == 0);
        let counts = world.combat_telemetry();
        assert_eq!(
            (counts.melee_hits, counts.ranged_hits, counts.deaths),
            (1, 0, 1),
            "{kind:?} dealt one lethal hit to a player unit"
        );
        assert_eq!(world.player_combat_totals(PlayerId(0)), (0, 1));
        assert_eq!(world.player_combat_totals(PlayerId(1)), (0, 0));
        advance(&mut world, 650);
        assert!(world.unit(victim).is_none());
        assert_eq!(world.player_combat_totals(PlayerId(0)), (0, 1));
    }
}
