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
    march(&mut w, g, &[a, b, g], goal);
    let slots = [w.formation_slot(a), w.formation_slot(b)];
    assert!(slots.iter().all(Option::is_some));
    let old_goal = w.movement_goal(a).unwrap();
    let before = w.order_demand()[crate::OrderIssuer::General as usize];
    let recruit = w.spawn_militia(PlayerId(0), at(24, 20)).unwrap();
    w.claim_general_unit(g, w.unit(recruit).unwrap());
    assert_eq!(w.general_controller(recruit), Some(g));
    assert_eq!(
        w.economy.generals.armies[&g].routed,
        Some(goal),
        "recruitment preserves the commander's final leg"
    );
    march(&mut w, g, &[a, b, recruit, g], goal);
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

#[test]
fn squads_and_commander_keep_one_offensive_order_when_enemies_appear() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let ids: Vec<_> = (0..8)
        .map(|n| w.spawn_militia(PlayerId(0), at(20 + n, 20)).unwrap())
        .collect();
    let g = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(24, 24))
        .unwrap();
    let goal = at(140, 24);
    w.issue_move(g, goal).unwrap();
    w.tick.0 = 10;
    w.with_order_issuer(crate::OrderIssuer::General, |w| {
        w.advance_generals(&mut Vec::new())
    });
    let slots: Vec<_> = ids
        .iter()
        .chain([&g])
        .map(|&id| {
            let records = w.formation_records();
            let group = records
                .iter()
                .find(|group| group.members.contains(&id))
                .unwrap();
            assert!(
                group.attack_move,
                "{id:?} uses the player's offensive movement"
            );
            assert_eq!(
                w.unit(id).unwrap().stance,
                aoe_core::CombatStance::Aggressive
            );
            w.formation_destination_slot(id)
        })
        .collect();
    let before = w.order_demand()[crate::OrderIssuer::General as usize];
    assert!(before.orders > 0, "observe actual General-issued orders");
    w.spawn_militia(PlayerId(1), at(29, 24)).unwrap();
    w.tick.0 = 20;
    w.refill_route_budgets();
    w.with_order_issuer(crate::OrderIssuer::General, |w| {
        w.advance_generals(&mut Vec::new())
    });
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
    assert_eq!(
        before.orders,
        w.order_demand()[crate::OrderIssuer::General as usize].orders
    );
    assert_eq!(
        slots,
        ids.iter()
            .chain([&g])
            .map(|&id| w.formation_destination_slot(id))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_idle_army_keeps_its_offensive_click_when_the_enemy_moves() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.set_fog_disabled(true);
    for n in 0..8 {
        w.spawn_militia(PlayerId(0), at(20 + n, 20)).unwrap();
    }
    let g = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(24, 24))
        .unwrap();
    let enemy = w.spawn_militia(PlayerId(1), at(29, 24)).unwrap();
    let decide = |w: &mut GameWorld| {
        w.tick.0 += 10;
        w.refill_route_budgets();
        w.with_order_issuer(crate::OrderIssuer::General, |w| {
            w.advance_generals(&mut Vec::new())
        });
    };
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
    let before = w.order_demand()[crate::OrderIssuer::General as usize].orders;
    assert!(before > 0);
    let old = w.unit(enemy).unwrap().position;
    let moved = at(31, 24);
    w.collisions.relocate(enemy, old.tile_floor(), moved, false);
    w.units[w.lookup[&enemy]].state.position = moved;
    decide(&mut w);
    assert_eq!(
        before,
        w.order_demand()[crate::OrderIssuer::General as usize].orders
    );
}
