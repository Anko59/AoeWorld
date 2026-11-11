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
