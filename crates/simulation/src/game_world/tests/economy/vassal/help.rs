//! A Vassal under fire from a building calls its soldiers and Generals.
use super::*;

#[test]
fn vassal_calls_for_help_when_an_enemy_tower_attacks_its_territory() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let soldier = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(106, 92))
        .unwrap();
    w.issue_stance(PlayerId(0), &[soldier], aoe_core::CombatStance::StandGround)
        .unwrap();
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(190, 92))
        .unwrap();
    let tower = w
        .insert_building(
            PlayerId(1),
            BuildingKind::WatchTower,
            TileRect::from_xywh(110, 92, 1, 1),
            true,
        )
        .unwrap();
    w.issue_attack(PlayerId(1), tower, soldier).unwrap();
    advance(&mut w, 40);
    assert!(w.unit(soldier).unwrap().last_hit.is_some());
    assert!(w.vassal_status(v).unwrap().alert);
    assert_eq!(w.vassal_status(v).unwrap().responding, 1);
    assert!(w.economy.generals.armies[&general].destination.is_some());
}

#[test]
fn a_returning_vassal_soldier_keeps_the_same_goal_without_new_orders() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    w.issue_vassal_development(PlayerId(0), v, false).unwrap();
    let id = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(106, 92))
        .unwrap();
    advance(&mut w, 20);
    assert_eq!(w.vassal_controller(id), Some(v));
    // Model a defender finishing a long pursuit outside the castle domain.
    let index = w.lookup[&id];
    let from = w.units[index].state.position.tile_floor();
    w.units[index].state.position = at(200, 92);
    let state = w.units[index].state;
    w.track_unit_step(id, from, &state);
    w.economy.stewards.automatic = true;
    w.tick.0 = 40;
    w.with_order_issuer(crate::OrderIssuer::Vassal, |w| {
        w.advance_vassals(&mut Vec::new())
    });
    let order = w.movement_order(id).expect("return to the castle");
    let before = w.order_demand()[crate::OrderIssuer::Vassal as usize];
    w.tick.0 = 60;
    w.with_order_issuer(crate::OrderIssuer::Vassal, |w| {
        w.advance_vassals(&mut Vec::new())
    });
    assert_eq!(w.movement_order(id), Some(order));
    assert_eq!(
        w.order_demand()[crate::OrderIssuer::Vassal as usize].orders,
        before.orders
    );
    assert_eq!(
        w.order_demand()[crate::OrderIssuer::Vassal as usize].decisions,
        before.decisions
    );
    // A new pursuit is a different intent even with the retained home route.
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(230, 92))
        .unwrap();
    w.begin_attack(id, enemy);
    w.tick.0 = 80;
    w.with_order_issuer(crate::OrderIssuer::Vassal, |w| {
        w.advance_vassals(&mut Vec::new())
    });
    assert_eq!(
        w.unit(id).unwrap().attack_target,
        None,
        "returning cancels pursuit"
    );
}
