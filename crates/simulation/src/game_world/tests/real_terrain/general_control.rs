//! Owner orders through a seeded map with a wooded, single-deck river crossing.
use super::*;
use crate::game_world::terrain_fixtures::{Command, build};
use aoe_core::Formation;

fn order(w: &mut GameWorld, ids: &[EntityId], goal: WorldPosition, queued: bool) {
    w.issue_player_formation_move(PlayerId(0), ids, Formation::Line, goal, queued, queued)
        .unwrap();
}

fn objective(f: &crate::game_world::terrain_fixtures::Fixture) -> WorldPosition {
    let Command::General(_, goal) = f.commands[0] else {
        panic!("expected a General's march");
    };
    goal
}

#[test]
fn general_control_keeps_the_whole_army_across_a_wooded_bridge_and_queued_turn() {
    let mut f = build("forest-bridge");
    let g = f.primary;
    let first = objective(&f);
    let second = center(218, 158);
    assert_eq!(f.world.general_status(g).unwrap().army_size, 100);
    assert!((f.world.unit(g).unwrap().position.x - first.x).abs() > 50 * 1024);
    let selection = [g, f.ids[0]];
    order(&mut f.world, &selection, first, false);
    order(&mut f.world, &selection, second, true);
    assert_eq!(f.world.move_destinations(g), [first, second]);
    assert!(f.world.move_destinations(f.ids[0]).is_empty());
    let mut crossed = std::collections::BTreeSet::new();
    let mut lost = std::collections::BTreeSet::new();
    let mut on_turn = None;
    let mut arrived = 0;
    for tick in 1..=12_000 {
        f.world.advance();
        for &id in f.ids.iter().filter(|&&id| id != g) {
            if f.world.general_controller(id) != Some(g) {
                lost.insert(id);
            }
            if f.world
                .unit(id)
                .is_some_and(|u| u.position.tile_floor().x + u.position.tile_floor().y > 252)
            {
                crossed.insert(id);
            }
        }
        if f.world.economy.generals.armies[&g].destination == Some(second) {
            on_turn.get_or_insert_with(|| {
                let settled = f
                    .ids
                    .iter()
                    .filter(|&&id| id != g)
                    .filter(|&&id| {
                        f.world.unit(id).is_some_and(|u| {
                            !u.moving
                                && !u.planning
                                && f.world.formation_destination_slot(id) == Some(u.position)
                        })
                    })
                    .count();
                (tick, crossed.len(), settled)
            });
        }
        if on_turn.is_some() {
            arrived = f
                .ids
                .iter()
                .filter(|&&id| {
                    let u = f.world.unit(id).unwrap();
                    !u.moving
                        && !u.planning
                        && (u.position.x - second.x).abs() <= 40 * 1024
                        && (u.position.y - second.y).abs() <= 40 * 1024
                        && f.world.formation_destination_slot(id) == Some(u.position)
                })
                .count();
            if arrived == f.ids.len() {
                break;
            }
        }
    }
    eprintln!(
        "wooded bridge: crossed={}, lost={}, queue turn={on_turn:?}, final arrivals={arrived}/101",
        crossed.len(),
        lost.len()
    );
    assert_eq!(lost.len(), 0);
    assert_eq!(crossed.len(), 100);
    assert_eq!(
        on_turn.map(|(_, crossed, settled)| (crossed, settled)),
        Some((100, 100))
    );
    assert_eq!(arrived, 101);
    for _ in 0..20 {
        f.world.advance();
    }
    assert!(f.world.move_waypoints(g).is_empty());
}

#[test]
fn general_control_keeps_an_independently_ordered_soldier_out_of_the_march() {
    let mut f = build("forest-bridge");
    let g = f.primary;
    let soldier = f.ids[0];
    let goal = objective(&f);
    let own = center(45, 59);
    order(&mut f.world, &[g], goal, false);
    for _ in 0..40 {
        f.world.advance();
    }
    order(&mut f.world, &[soldier], own, false);
    let mut overrides = 0;
    for _ in 0..200 {
        f.world.advance();
        overrides += usize::from(
            f.world.general_controller(soldier).is_some()
                || f.world.movement_goal(soldier).is_some_and(|p| p != own),
        );
    }
    eprintln!("detached soldier: {overrides} overridden ticks in 10 seconds");
    assert_eq!(overrides, 0);
    assert_eq!(f.world.economy.generals.armies[&g].destination, Some(goal));
    assert_eq!(f.world.general_status(g).unwrap().army_size, 99);
}

#[test]
fn general_control_owner_march_beats_a_real_vassal_defence_call() {
    let mut f = build("forest-bridge");
    let g = f.primary;
    let goal = objective(&f);
    let w = &mut f.world;
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(53, 52, 4, 4),
            true,
        )
        .unwrap();
    let v = w
        .spawn_kind(PlayerId(0), UnitKind::Vassal, center(52, 54))
        .unwrap();
    w.issue_garrison(PlayerId(0), v, castle).unwrap();
    for _ in 0..10 {
        w.advance();
    }
    w.issue_vassal_development(PlayerId(0), v, false).unwrap();
    order(w, &[g], goal, false);
    let rear = center(19, 70);
    let enemy = w.spawn_kind(PlayerId(1), UnitKind::Militia, rear).unwrap();
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let mut overrides = 0;
    let mut alert = false;
    for _ in 0..200 {
        w.advance();
        alert |= w.vassal_status(v).unwrap().alert;
        overrides += usize::from(w.economy.generals.armies[&g].destination != Some(goal));
    }
    eprintln!("Vassal defence: alert={alert}, {overrides} overridden ticks in 10 seconds");
    assert!(alert);
    assert_eq!(overrides, 0);
    assert_eq!(w.vassal_status(v).unwrap().responding, 0);
    w.issue_stop(PlayerId(0), g).unwrap();
    for _ in 0..20 {
        w.advance();
    }
    assert_eq!(w.vassal_status(v).unwrap().responding, 1);
    order(w, &[g], goal, false);
    for _ in 0..40 {
        w.advance();
    }
    assert_eq!(w.vassal_status(v).unwrap().responding, 0);
    assert_eq!(w.economy.generals.armies[&g].destination, Some(goal));
}

#[test]
fn general_control_offensive_squad_reacts_to_the_generals_sighting() {
    let mut f = build("forest-bridge");
    let g = f.primary;
    let soldier = f.ids[0];
    let w = &mut f.world;
    for id in f.ids.into_iter().filter(|&id| id != g && id != soldier) {
        w.remove_unit(id);
    }
    w.set_fog_disabled(false);
    let at = center(24, 48);
    let enemy = w.spawn_kind(PlayerId(1), UnitKind::Militia, at).unwrap();
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    assert!(w.unit_visible_to(PlayerId(0), enemy));
    let (_, soldier_tile, sight) = w.unit_observer(&w.unit(soldier).unwrap()).unwrap();
    let delta = at.tile_floor();
    assert!(
        (delta.x - soldier_tile.x).pow(2) + (delta.y - soldier_tile.y).pow(2)
            > i32::from(sight).pow(2)
    );
    assert_eq!(w.general_controller(soldier), Some(g));
    w.issue_formation_move(
        PlayerId(0),
        &[soldier],
        Formation::Line,
        center(42, 38),
        false,
        true,
    )
    .unwrap();
    for _ in 0..8 {
        w.advance();
    }
    let reacted = w.unit(soldier).unwrap().attack_target == Some(enemy);
    eprintln!(
        "shared sight: {} of 1 soldier reacted within eight ticks",
        usize::from(reacted)
    );
    assert!(reacted);
}

#[test]
fn general_control_orders_a_rostered_straggler_beyond_fifty_tiles() {
    let mut f = build("forest-bridge");
    let g = f.primary;
    let soldier = f.ids[0];
    let goal = objective(&f);
    let w = &mut f.world;
    let index = w.lookup[&soldier];
    let from = w.units[index].state.position.tile_floor();
    let straggler = center(26, 105);
    w.units[index].state.position = straggler;
    let state = w.units[index].state;
    w.track_unit_step(soldier, from, &state);
    assert_eq!(w.general_controller(soldier), Some(g));
    order(w, &[g], goal, false);
    for _ in 0..40 {
        w.advance();
    }
    let ordered = w.formation_destination_slot(soldier).is_some();
    eprintln!(
        "62-tile straggler: {} of 1 soldier received the march",
        usize::from(ordered)
    );
    assert!(ordered);
}

#[test]
fn general_control_owner_move_is_not_replaced_by_a_general_retreat() {
    let run = |manual| {
        let mut f = build("forest-bridge");
        let g = f.primary;
        let soldier = f.ids[0];
        let goal = objective(&f);
        let w = &mut f.world;
        for id in f.ids.into_iter().filter(|&id| id != g && id != soldier) {
            w.remove_unit(id);
        }
        w.units[w.lookup[&g]].state.hp = 1;
        for y in 42..46 {
            let enemy = w
                .spawn_kind(PlayerId(1), UnitKind::Paladin, center(36, y))
                .unwrap();
            w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
                .unwrap();
        }
        if manual {
            order(w, &[g], goal, false);
        } else {
            w.issue_move(g, goal).unwrap();
        }
        let mut overrides = 0;
        for _ in 0..200 {
            w.advance();
            overrides +=
                usize::from(w.general_status(g).unwrap().state == crate::GeneralState::Retreating);
        }
        assert_eq!(w.economy.generals.armies[&g].destination, Some(goal));
        overrides
    };
    let automated = run(false);
    let owner = run(true);
    eprintln!(
        "General retreat: {automated} automated and {owner} owner ticks diverted in 10 seconds"
    );
    assert!(automated > 0);
    assert_eq!(owner, 0);
}

#[test]
fn general_control_keeps_an_owner_move_on_a_steward_worker() {
    let mut f = build("forest-bridge");
    let goal = objective(&f);
    let w = &mut f.world;
    let player = PlayerId(0);
    let town = w
        .insert_building(
            player,
            BuildingKind::TownCenter,
            TileRect::from_xywh(62, 52, 4, 4),
            true,
        )
        .unwrap();
    let steward = w
        .spawn_kind(player, UnitKind::Steward, center(61, 54))
        .unwrap();
    w.issue_garrison(player, steward, town).unwrap();
    let worker = w.spawn_villager(player, center(68, 54)).unwrap();
    let tile = TileCoord::new(69, 57);
    let wood = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Wood).unwrap();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, wood).unwrap();
    }
    w.collision_closed(TileRect::from_xywh(tile.x, tile.y, 1, 1));
    w.issue_gather(player, worker, wood.id).unwrap();
    for _ in 0..50 {
        w.advance();
    }
    assert_eq!(w.steward_controller(worker), Some(steward));
    w.issue_move(worker, goal).unwrap();
    let mut overrides = 0;
    for _ in 0..200 {
        w.advance();
        overrides += usize::from(
            w.steward_controller(worker).is_some() || w.movement_goal(worker) != Some(goal),
        );
    }
    eprintln!("Steward worker: {overrides} overridden ticks in 10 seconds");
    assert_eq!(overrides, 0);
}
