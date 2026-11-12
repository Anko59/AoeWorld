use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn march(w: &mut GameWorld, g: EntityId, ids: &[EntityId], goal: WorldPosition) {
    let members = ids
        .iter()
        .map(|&id| w.unit(id).unwrap())
        .collect::<Vec<_>>();
    w.tick.0 += 10;
    w.refill_route_budgets();
    w.with_order_issuer(crate::OrderIssuer::General, |w| {
        w.march_squads(
            g,
            &members,
            goal,
            false,
            &mut Vec::new(),
            &mut GeneralWork::default(),
        );
    });
}

#[test]
fn a_reinforcement_keeps_existing_soldiers_on_their_admitted_march() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let a = w.spawn_militia(PlayerId(0), at(20, 20)).unwrap();
    let b = w.spawn_militia(PlayerId(0), at(22, 20)).unwrap();
    let g = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(21, 24))
        .unwrap();
    let goal = at(70, 40);
    w.general_march(g, goal);
    march(&mut w, g, &[a, b], goal);
    let slots = [w.formation_slot(a), w.formation_slot(b)];
    assert!(slots.iter().all(Option::is_some));
    let old_goal = w.movement_goal(a).unwrap();
    let before = w.order_demand()[crate::OrderIssuer::General as usize];
    let recruit = w.spawn_militia(PlayerId(0), at(24, 20)).unwrap();
    w.claim_general_unit(g, w.unit(recruit).unwrap());
    assert_eq!(w.general_controller(recruit), Some(g));
    march(&mut w, g, &[a, b, recruit], goal);
    let after = w.order_demand()[crate::OrderIssuer::General as usize];
    assert_eq!(
        after.orders,
        before.orders + 1,
        "only the new soldier needs an order"
    );
    assert_eq!(after.near_reissues, before.near_reissues);
    assert_eq!(after.superseded_10s, before.superseded_10s);
    assert_eq!([w.formation_slot(a), w.formation_slot(b)], slots);
    let unit = w.unit(recruit).unwrap();
    assert!(
        unit.moving || unit.planning,
        "the reinforcement joins promptly"
    );
    let changed = at(120, 80);
    w.general_march(g, changed);
    march(&mut w, g, &[a, b, recruit], changed);
    assert_ne!(
        w.movement_goal(a),
        Some(old_goal),
        "a new goal still redirects the army"
    );
}
