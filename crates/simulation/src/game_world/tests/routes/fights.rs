//! Armies ordered to attack waited for routes they never got. Replaying the
//! 99-AI Mekong window (1,500 ticks from tick 72,451) the `attack_approach`
//! class of the queue report ended with 70 units walking and 75 waiting, 67
//! of them with no planner at all, median wait 178 ticks and up to 688, and
//! **all 75 with their one slot-free fast attempt already spent**: only one
//! of their player's two search slots could still answer them. Those slots
//! were held 79 by gather trips and 32 by single moves against 8 by attack
//! approaches, because the scheduler had no notion of a fight: `serve_waiting`
//! ordered a player's waiting units by `(holds a planner, wait, id)` alone,
//! so the slot a search freed went to whichever order had waited longest, and
//! economy orders both outnumber fight orders and arrive first.
use super::fairness::tile_position;
use super::*;
use crate::UnitKind;

/// A wall long enough that going around it costs more than a fast attempt,
/// so an order across it needs one of its player's near search slots.
const WALL_Y: i32 = 200;

fn walled_world() -> GameWorld {
    let config = WorldConfig::new(1_024, 1_024, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    for x in 130..270 {
        world
            .economy
            .blocked_tiles
            .insert(TileCoord::new(x, WALL_Y));
    }
    world.set_fog_disabled(true);
    world
}

/// An order across the wall, which no fast attempt can settle. Spaced so no
/// two share a route start.
fn across_the_wall(world: &mut GameWorld, player: PlayerId, n: i32) -> EntityId {
    let x = 180 + (n % 20) * 2;
    let y = WALL_Y - 10 - (n / 20) * 2;
    let id = world
        .spawn_unit(player, tile_position(x, y))
        .expect("villager");
    assert_eq!(
        world.issue_move(id, tile_position(x, WALL_Y + 10)),
        Ok(true)
    );
    id
}

/// A waiting unit that holds no planner: only a slot can still answer it.
fn waits_without_planner(world: &GameWorld, id: EntityId) -> bool {
    let unit = &world.units[world.lookup[&id]];
    unit.order.is_some() && unit.state.planning && unit.route.is_empty() && unit.planner.is_none()
}

fn holds_search(world: &GameWorld, id: EntityId) -> bool {
    world.units[world.lookup[&id]]
        .planner
        .as_deref()
        .is_some_and(aoe_map::RoutePlanner::requires_search_slot)
}

/// Fights are preferred inside a player's own queue only: a player with a
/// fight must not take search work away from another player's economy.
#[test]
fn a_fight_order_never_takes_another_players_share() {
    let mut world = walled_world();
    let fighter = PlayerId(3);
    let enemy = PlayerId(4);
    let other = PlayerId(5);

    world.fast_route_budget = 0;
    let soldier = world
        .spawn_kind(fighter, UnitKind::Militia, tile_position(220, WALL_Y - 12))
        .expect("soldier");
    let target = world
        .spawn_kind(enemy, UnitKind::Militia, tile_position(220, WALL_Y + 12))
        .expect("target");
    world
        .issue_attack(fighter, soldier, target)
        .expect("attack accepted");
    let neighbour = across_the_wall(&mut world, other, 0);
    for _ in 0..40 {
        world.fast_route_budget = 0;
        world.advance();
    }
    assert!(
        world.units[world.lookup[&soldier]]
            .state
            .attack_target
            .is_some(),
        "the fight is still a fight"
    );
    assert!(
        holds_search(&world, neighbour) || !waits_without_planner(&world, neighbour),
        "the other player's order still gets its own slot and share: {:?}",
        world.route_slots.get(&other)
    );
    assert_eq!(world.route_accounting_error(), None);
}
