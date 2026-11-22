use super::*;
use crate::{OrderIssuer, UnitKind};
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig};

#[test]
fn pursuit_keeps_a_valid_firing_contact_when_the_target_changes_tile() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.set_fog_disabled(true);
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 50)).unwrap();
    let actor = w.spawn_kind(PlayerId(0), UnitKind::Archer, at(20)).unwrap();
    let range = crate::game_world::navy::attack_range(&w.unit_stats(actor).unwrap());
    let tiles = i32::try_from(range / 1024).unwrap();
    let start = 23 + tiles;
    let target = w.spawn_militia(PlayerId(1), at(start)).unwrap();
    w.issue_attack(PlayerId(0), actor, target).unwrap();
    let old = w.movement_order(actor).unwrap();
    let index = w.lookup[&actor];
    assert!(!w.waiting_for_route(index));
    let before = w.order_demand()[OrderIssuer::Automation as usize];
    w.units[w.lookup[&target]].state.position = at(start - 1);
    let goal = w.attack_goal(target, PlayerId(0)).unwrap();
    assert!(goal.distance_squared(old.destination) <= range * range);
    assert!(goal.distance_squared(w.unit(actor).unwrap().position) > range * range);
    w.combat.attackers.get_mut(&actor).unwrap().repath_tick = 0;
    let attacker = w.combat.attackers[&actor];
    w.approach(actor, index, attacker, goal, w.tick.0, &mut Vec::new());
    assert_eq!(
        w.order_demand()[OrderIssuer::Automation as usize].orders,
        before.orders
    );
    assert_eq!(
        w.movement_order(actor).unwrap().destination,
        old.destination
    );
    w.units[w.lookup[&target]].state.position = at(start + tiles + 2);
    let goal = w.attack_goal(target, PlayerId(0)).unwrap();
    assert!(goal.distance_squared(old.destination) > range * range);
    w.combat.attackers.get_mut(&actor).unwrap().repath_tick = 0;
    let attacker = w.combat.attackers[&actor];
    w.approach(actor, index, attacker, goal, w.tick.0, &mut Vec::new());
    assert!(w.order_demand()[OrderIssuer::Automation as usize].orders > before.orders);
    assert_ne!(
        w.movement_order(actor).unwrap().destination,
        old.destination
    );
}

/// Two archers of players 1 and 2 shooting each other in place, and an
/// archer of player 1 shooting a villager of player 2 that walks off, with
/// player 0 as a far human seat when `watched`.
fn duel(watched: bool) -> (GameWorld, [EntityId; 3]) {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let a = w
        .spawn_kind(PlayerId(1), UnitKind::Archer, at(40, 50))
        .unwrap();
    let b = w
        .spawn_kind(PlayerId(2), UnitKind::Archer, at(44, 50))
        .unwrap();
    let c = w
        .spawn_kind(PlayerId(1), UnitKind::Archer, at(40, 60))
        .unwrap();
    let runner = w
        .spawn_kind(PlayerId(2), UnitKind::Villager, at(44, 60))
        .unwrap();
    w.issue_attack(PlayerId(1), a, b).unwrap();
    w.issue_attack(PlayerId(2), b, a).unwrap();
    w.issue_attack(PlayerId(1), c, runner).unwrap();
    if watched {
        w.set_human_seats(&[PlayerId(0)]);
        w.spawn_kind(PlayerId(0), UnitKind::Villager, at(200, 200))
            .unwrap();
    }
    (w, [a, b, runner])
}

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn unseen_attackers_rest_between_shots_and_hit_as_often() {
    let (mut full, ids) = duel(false);
    let (mut resting, _) = duel(true);
    let (mut rested, mut runner_hp) = (0, 0);
    for tick in 0..200 {
        if tick == 60 {
            resting.issue_move(ids[2], at(44, 90)).unwrap();
            runner_hp = resting.unit(ids[2]).unwrap().hp;
        }
        full.advance();
        resting.advance();
        rested += resting.combat_stats().attackers_rested;
        assert_eq!(full.combat_stats().attackers_rested, 0);
        // The archers' duel is the same blow for blow.
        for &id in &ids[..2] {
            let hp = |w: &GameWorld| w.unit(id).map(|u| u.hp);
            assert_eq!(hp(&full), hp(&resting), "hit points of {id:?} at {tick}");
        }
    }
    assert!(rested > 100, "attackers rested {rested} times");
    // The runner's moves wake its resting pursuer, which keeps hitting it.
    let hp = resting.unit(ids[2]).map_or(0, |u| u.hp);
    assert!(
        hp < runner_hp,
        "the runner kept {hp} of {runner_hp} hit points"
    );
}
