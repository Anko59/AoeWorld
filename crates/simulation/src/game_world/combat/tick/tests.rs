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
/// archer of player 1 shooting a villager of player 2.
fn duel() -> (GameWorld, [EntityId; 3]) {
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
    (w, [a, b, runner])
}

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn the_first_blow_in_each_reach_is_counted_with_its_wait() {
    let (mut w, ids) = duel();
    let (mut first, mut waited, mut hits) = (0, 0, 0);
    for tick in 0..200 {
        if tick == 60 {
            // Out of reach and back: its pursuer's next blow is a first one.
            w.issue_move(ids[2], at(44, 90)).unwrap();
        }
        w.advance();
        first += w.combat_stats().first_hits;
        waited += w.combat_stats().first_hit_wait_ticks;
        hits += w.combat_stats().hits;
    }
    // Three opening blows in reach, one more after the chase.
    assert!(first >= 4, "{first} first blows");
    assert!(hits > first, "{hits} hits for {first} first blows");
    assert!(
        waited < u64::from(first) * 20,
        "{waited} ticks over {first}"
    );
}
