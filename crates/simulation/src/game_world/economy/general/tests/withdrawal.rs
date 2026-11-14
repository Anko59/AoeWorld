use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

#[test]
fn retained_withdrawal_keeps_the_shared_order_without_renewing_admission() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.economy.blocked_tiles.insert(TileCoord::new(200, 200));
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 20)).unwrap();
    let id = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(20))
        .unwrap();
    w.issue_move(id, at(100)).unwrap();
    let mut reached = false;
    for _ in 0..120 {
        let unit = w.unit(id).unwrap();
        let order = w.movement_order(id).unwrap();
        if unit.position.tile_floor() == order.waypoint.tile_floor()
            && unit.position != order.waypoint
        {
            reached = true;
            break;
        }
        w.advance();
    }
    assert!(
        reached,
        "the unit enters the next route tile before its centre"
    );
    let old = w.movement_order(id).unwrap();
    let since = w.units[w.lookup[&id]].planning_since;
    let requests = w.order_demand()[crate::OrderIssuer::Automation as usize].route_requests;
    w.issue_move_goal(
        id,
        old.destination,
        crate::game_world::orders::GoalPrecision::Contact,
    )
    .unwrap();
    let next = w.movement_order(id).unwrap();
    assert_eq!(
        next, old,
        "the ordinary move API retains the admitted route"
    );
    assert_eq!(next.destination, old.destination);
    assert_eq!(
        w.units[w.lookup[&id]].planning_since, since,
        "do not renew admission"
    );
    assert_eq!(
        w.order_demand()[crate::OrderIssuer::Automation as usize].route_requests,
        requests
    );
}
