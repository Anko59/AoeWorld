use super::steward::{boat_near, domain, fish_near};
use super::*;
use crate::GameWorldError;

/// A fishing order can be accepted while its route is still planned and then
/// fail there. The failure must not let the ship re-order, and so search
/// every candidate position, on every tick: dev answered each such failure
/// with a fresh round of route requests until the planner failed again.
#[test]
fn a_ship_whose_accepted_route_keeps_failing_does_not_reorder_every_tick() {
    let mut w = coast();
    let shore = site(&w);
    let (town_center, _king) = domain(&mut w, shore);
    let near = fish_near(&w, town_center);
    let crate::Terrain::Map { generator, .. } = w.terrain() else {
        panic!()
    };
    let far = (0..5)
        .flat_map(|y| (0..8).flat_map(move |x| generator.chunk(x, y).unwrap().resources))
        .filter(|n| n.object == ObjectKind::DeepFish && n.id != near.id)
        .max_by_key(|n| (n.tile.x - near.tile.x).pow(2) + (n.tile.y - near.tile.y).pow(2))
        .expect("a second fish");
    let boat = boat_near(&mut w, near, PlayerId(0));
    w.issue_gather(PlayerId(0), boat, far.id).unwrap();
    let requests = |w: &GameWorld| -> u64 {
        w.order_diagnostics
            .demand
            .iter()
            .map(|d| d.route_requests)
            .sum()
    };
    let before = requests(&w);
    for _ in 0..20 * 10 {
        w.advance();
        // The planner behind the accepted order reports the fish unreachable.
        let index = w.lookup[&boat];
        if w.units[index].order.is_some() || w.units[index].state.planning {
            w.clear_planner(index);
            w.units[index].order = None;
            w.units[index].state.moving = false;
            w.units[index].state.planning = false;
            w.units[index].route.clear();
        }
        w.set_movement_error(index, Some(GameWorldError::Unreachable));
    }
    let spent = requests(&w) - before;
    assert!(
        spent <= 12,
        "{spent} route requests in ten seconds from one ship (dev: 200, one per tick)"
    );
}
