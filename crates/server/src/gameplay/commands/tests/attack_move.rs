use super::*;
use aoe_core::{PlayerId, Seed, TileCoord};
use aoe_simulation::{GameWorld, UnitKind};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// One soldier ordered along a row past an enemy villager three tiles off it.
fn march(attack_move: bool) -> (GameWorld, EntityId, EntityId) {
    let mut world = GameWorld::new(aoe_core::WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let soldier = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(100, 100))
        .unwrap();
    let enemy = world.spawn_villager(PlayerId(1), at(110, 103)).unwrap();
    let order = PendingAction::Move {
        entity_id: soldier,
        destination: at(130, 100),
        queued: false,
        attack_move,
    };
    assert_eq!(
        apply(&mut world, PlayerId(0), order),
        CommandResult::Accepted
    );
    (world, soldier, enemy)
}

#[test]
fn a_single_units_attack_move_engages_an_enemy_it_passes_and_then_resumes() {
    let (mut world, soldier, enemy) = march(true);
    let mut engaged = false;
    for _ in 0..2_000 {
        world.advance();
        engaged |= world.unit(soldier).and_then(|u| u.attack_target) == Some(enemy);
    }
    assert!(engaged, "the soldier walked past the enemy");
    assert!(
        world.unit(enemy).is_none_or(|e| e.hp == 0),
        "enemy survived"
    );
    assert_eq!(
        world.unit(soldier).unwrap().position,
        at(130, 100),
        "the soldier did not resume toward its destination"
    );
}

#[test]
fn a_single_units_plain_move_walks_past_the_same_enemy() {
    let (mut world, soldier, enemy) = march(false);
    for _ in 0..2_000 {
        world.advance();
        assert_ne!(world.unit(soldier).unwrap().attack_target, Some(enemy));
    }
    assert!(world.unit(enemy).is_some_and(|e| e.hp > 0));
    assert_eq!(world.unit(soldier).unwrap().position, at(130, 100));
}

#[test]
fn repeating_a_single_units_attack_move_discards_its_queued_waypoints() {
    let (mut world, soldier, _) = march(true);
    world
        .issue_queued_leg(soldier, at(100, 140), false)
        .unwrap();
    assert_eq!(world.move_destinations(soldier).len(), 2);
    world.issue_attack_move(soldier, at(130, 100)).unwrap();
    assert_eq!(world.move_destinations(soldier), vec![at(130, 100)]);
    for _ in 0..3_000 {
        world.advance();
    }
    assert_eq!(world.unit(soldier).unwrap().position, at(130, 100));
}
