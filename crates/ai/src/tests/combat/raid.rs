//! `ai/tests/test_combat_orders.py`, second half: raids, recalls, defence and
//! contact. Every raid is reported by an `under_attack` event.
use std::collections::BTreeSet;

use aoe_sdk::{BuildingKind, EntityId, Event, Order, UnitKind, UnitTask, Visibility};

use super::{house, ids, raider, spearmen, step};
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::play::combat::CombatManager;
use crate::play::combat::march::{Purpose, march};
use crate::tests::fake::{
    FakeWorld, base, building, enemy, marching, soldier, under_attack, with_context, with_task,
};
use crate::view::Unit;

/// `test_combat_orders.raid`: our Town Center hit at (55, 54) by a militia of
/// owner 2 (visible or not), with `soldiers` and three villagers.
fn raid(soldiers: &[Unit], tick: u64, raider_visible: bool) -> (FakeWorld, Event) {
    let (mut units, buildings) = base(3);
    units.extend(soldiers.to_vec());
    let victim = buildings[0].id;
    let mut world = FakeWorld::new(units, buildings);
    world.tick = tick;
    if raider_visible {
        world.enemies = vec![raider(901, (55, 54))];
    }
    (world, under_attack(victim, Some(901), true, (55, 54)))
}

/// A worker of base(6) standing at (68, 52), gathering beside the raiders' path.
fn endangered_worker(units: &mut [Unit]) -> u32 {
    let worker = &mut units[0];
    worker.x = 68;
    worker.y = 52;
    worker.task = UnitTask::Gathering;
    worker.idle = false;
    worker.id
}

fn raiders_at_home(count: u32) -> Vec<Unit> {
    (0..count)
        .map(|i| enemy(800 + i, UnitKind::Spearman, (53 + i as i32, 52)))
        .collect()
}

#[test]
fn test_two_defenders_still_pull_workers_back_from_ten_raiders() {
    let (mut units, buildings) = base(6);
    let worker = endangered_worker(&mut units);
    let army = spearmen(300, 2, (55, 52));
    units.extend(army.clone());
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = raiders_at_home(10);
    let config = Config::default();
    let mut board = Blackboard::default();
    let (queries, seat) = with_context(&world, &[], &mut board, &config, |ctx| {
        CombatManager::default().step(ctx).expect("within budget");
        ctx.land_queries()
    });
    assert!(seat.moves().contains(&(worker, (56, 56))));
    assert!(
        !seat.attacks().is_empty(),
        "route budget must still leave room to defend"
    );
    assert!(queries <= config.land_queries_per_decision / 4 + army.len() as u32);
}

#[test]
fn test_reported_raid_preserves_worker_retreat_when_the_army_is_outnumbered() {
    let (mut units, buildings) = base(6);
    let worker = endangered_worker(&mut units);
    units.push(soldier(301, (55, 52)));
    let hit = under_attack(buildings[0].id, Some(800), true, (53, 52));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = raiders_at_home(10);
    let seat = step(
        &world,
        &[hit],
        &mut Blackboard::default(),
        &Config::default(),
    );

    assert!(seat.moves().contains(&(worker, (56, 56))));
    assert!(!seat.attacks().is_empty());
}

#[test]
fn test_unconnected_raiders_do_not_trigger_defence() {
    let (units, buildings) = base(6);
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = (0..10)
        .map(|i| enemy(800 + i, UnitKind::Spearman, (60, 52)))
        .collect();
    world.disconnected = true;
    let mut board = Blackboard::default();
    let (defended, seat) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        CombatManager::default().defend(ctx, &[])
    });
    assert_eq!(defended, Ok(false));
    assert!(seat.orders.is_empty());
}

#[test]
fn test_damage_to_a_town_center_recalls_only_unengaged_soldiers() {
    let mut soldiers: Vec<Unit> = (0..12).map(|i| marching(300 + i, (245, 220))).collect();
    soldiers[0].attack_target = Some(888);
    let (mut world, hit) = raid(&soldiers, 100, true);
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: soldiers.len() as u32,
        ..Blackboard::default()
    };
    let config = Config::default();
    let seat = step(&world, &[hit], &mut board, &config);

    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, ids(&soldiers[1..]));
    let ((x, y), first) = (groups[0].1, &soldiers[0]);
    assert!(x < first.x && y < first.y, "recalled toward home");
    assert!(!groups[0].3, "a recall is not an attack-move");
    world.tick += 20 * config.march_pause_seconds + 1;
    let seat = step(&world, &[], &mut board, &config);
    assert!(seat.formation_moves().is_empty());
}

#[test]
fn test_a_small_idle_group_engages_a_visible_enemy_away_from_home() {
    let soldiers: Vec<Unit> = (0..3)
        .map(|i| soldier(300 + i, (220 + i as i32, 220)))
        .collect();
    let (mut units, buildings) = base(3);
    units.extend(soldiers.clone());
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![raider(800, (221, 222))];
    let seat = step(&world, &[], &mut Blackboard::default(), &Config::default());

    let attackers: BTreeSet<u32> = seat.attacks().iter().map(|a| a.0).collect();
    assert_eq!(
        attackers,
        ids(&soldiers).into_iter().collect::<BTreeSet<u32>>()
    );
}

#[test]
fn test_repelled_raid_targets_the_attackers_base_over_a_closer_bystander() {
    let soldiers = spearmen(300, 15, (65, 60));
    let mut board = Blackboard::default();
    let config = Config::default();
    let (world, hit) = raid(&soldiers, 100, true);
    step(&world, &[hit], &mut board, &config);

    let (mut world, _) = raid(&soldiers, world.tick + 20 * 90, false);
    let bystander = house(880, (90, 65));
    let mut base_building = building(890, BuildingKind::TownCenter, (175, 165));
    base_building.mine = false;
    base_building.owner = 2;
    world.enemy_buildings = vec![bystander, base_building.clone()];
    let seat = step(&world, &[], &mut board, &config);

    assert_eq!(board.attack_target, Some(base_building.id));
    assert!(!seat.formation_moves().is_empty());
}

#[test]
fn test_each_spread_out_march_group_uses_one_route_query_for_its_hop() {
    let army: Vec<Unit> = (0..8)
        .map(|i| soldier(300 + i, (35 + 20 * i as i32, 60)))
        .collect();
    let mut world = FakeWorld::new(army.clone(), Vec::new());
    world.size = (500, 500);
    let config = Config {
        land_queries_per_decision: 8,
        ..Config::default()
    };
    let mut board = Blackboard::default();
    let (queries, seat) = with_context(&world, &[], &mut board, &config, |ctx| {
        march(ctx, &army, army[0].tile(), (360, 60), Purpose::Attack).expect("within budget");
        ctx.land_queries()
    });
    let groups = seat.formation_moves();
    let firsts: BTreeSet<u32> = groups.iter().map(|g| g.0[0]).collect();
    assert_eq!(firsts, ids(&army).into_iter().collect::<BTreeSet<u32>>());
    assert!(groups.iter().all(|g| g.3));
    assert!(queries <= 8);
}

#[test]
fn test_a_hit_on_a_worker_recalls_the_army_even_when_the_attacker_is_out_of_view() {
    let soldiers = spearmen(300, 12, (245, 220));
    let (world, _) = raid(&soldiers, 100, false);
    let hit = under_attack(1, Some(901), false, (55, 54));
    let mut board = Blackboard::default();
    let seat = step(&world, &[hit], &mut board, &Config::default());

    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, ids(&soldiers));
    assert_eq!(board.counter_owner, None);
}

#[test]
fn test_a_field_force_can_march_to_a_goal_outside_its_terrain_view() {
    let army = spearmen(300, 4, (145, 150));
    let (mut units, buildings) = base(3);
    units.extend(army.clone());
    let mut world = FakeWorld::new(units, buildings);
    world.known_fn = Some(Box::new(|_| false));
    world.visibility_fn = Some(Box::new(|_| Visibility::Unexplored));
    world.route_fn = Some(Box::new(|_, _| None));
    let mut board = Blackboard::default();
    let (done, seat) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        march(ctx, &army, (145, 150), (215, 215), Purpose::Attack)
    });
    done.expect("within budget");
    let sent: Vec<(Vec<u32>, (i32, i32))> = seat
        .formation_moves()
        .into_iter()
        .map(|g| (g.0, g.1))
        .collect();
    assert_eq!(sent, vec![(ids(&army), (215, 215))]);
}

#[test]
fn test_unreachable_raider_does_not_keep_defence_mode_alive() {
    let (mut units, buildings) = base(6);
    let army = spearmen(300, 4, (55, 52));
    units.extend(army.clone());
    let hit = under_attack(buildings[0].id, Some(800), true, (53, 52));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![enemy(800, UnitKind::Spearman, (60, 52))];
    world.disconnected = true;
    let (mut board, config) = (Blackboard::default(), Config::default());
    step(&world, &[hit], &mut board, &config);
    let first_until = board.raid_until;
    assert!(first_until > 0);
    // Later decisions see the same raider: the raid must expire, not renew.
    let late = u64::from(world.tick_hz) * config.raid_hold_seconds + 1;
    world.tick = first_until as u64 + late;
    let (defended, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        CombatManager::default().defend(ctx, &army)
    });
    assert_eq!(defended, Ok(false));
    assert_eq!(board.raid_until, first_until);
}

#[test]
fn test_a_stalled_march_engages_instead_of_stopping_for_route_recovery() {
    let (mut units, buildings) = base(3);
    let walker = marching(300, (120, 120));
    units.push(walker.clone());
    let mut world = FakeWorld::new(units, buildings);
    world.tick = 2_000;
    world.enemies = vec![enemy(801, UnitKind::Archer, (126, 120))];
    // The server can accept an attack even when the client has no exact terrain.
    world.route_fn = Some(Box::new(|_, _| None));
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: 1,
        ..Blackboard::default()
    };
    board.march_progress.insert(walker.id, (walker.tile(), 100));
    let seat = step(&world, &[], &mut board, &Config::default());

    assert_eq!(seat.attacks(), vec![(300, 801)]);
    assert!(seat.stops().is_empty());
}

fn answers_damage_from_outside_contact_radius(visible: bool) {
    let (mut units, buildings) = base(3);
    units.push(marching(300, (120, 120)));
    let attacker = enemy(801, UnitKind::Archer, (130, 120));
    let hit = under_attack(300, Some(801), false, (120, 120));
    let mut world = FakeWorld::new(units, buildings);
    if visible {
        world.enemies = vec![attacker];
    }
    let seat = step(
        &world,
        &[hit],
        &mut Blackboard::default(),
        &Config::default(),
    );

    let expected = if visible {
        Order::Attack {
            unit: EntityId(300),
            target: EntityId(801),
        }
    } else {
        Order::Stop {
            unit: EntityId(300),
        }
    };
    assert_eq!(seat.orders, vec![expected]);
}

#[test]
fn test_a_marching_soldier_responds_to_damage_from_outside_contact_radius_visible() {
    answers_damage_from_outside_contact_radius(true);
}

#[test]
fn test_a_marching_soldier_responds_to_damage_from_outside_contact_radius_unseen() {
    answers_damage_from_outside_contact_radius(false);
}

#[test]
fn test_army_defends_a_lumber_camp_against_visible_raiders_before_damage() {
    let (mut units, mut buildings) = base(3);
    buildings.push(building(600, BuildingKind::LumberCamp, (145, 150)));
    let walker = with_task(soldier(300, (115, 150)), UnitTask::Moving);
    units.push(walker.clone());
    let mut world = FakeWorld::new(units, buildings);
    let raider = raider(800, (148, 150));
    world.enemies = vec![raider.clone()];
    let mut board = Blackboard::default();
    let seat = step(&world, &[], &mut board, &Config::default());

    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, vec![walker.id]);
    assert!(groups[0].1.0 > walker.x, "sent toward the camp");
    assert!(!groups[0].3);
    assert_eq!(board.counter_owner, Some(raider.owner));
}

#[test]
fn test_dense_contact_can_redirect_the_whole_group_in_one_order() {
    let (mut units, buildings) = base(3);
    let mut soldiers: Vec<Unit> = (0..64)
        .map(|i| with_task(soldier(300 + i, (120, 120)), UnitTask::Moving))
        .collect();
    soldiers[0].attack_target = Some(800);
    units.extend(soldiers.clone());
    let archer = enemy(800, UnitKind::Archer, (126, 120));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![archer.clone()];
    let seat = step(&world, &[], &mut Blackboard::default(), &Config::default());

    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, ids(&soldiers[1..]));
    assert_eq!(groups[0].1, archer.tile());
    assert!(groups[0].3, "contact is an attack-move");
}

#[test]
fn test_an_unarmed_visitor_at_a_camp_does_not_pull_the_army() {
    let (mut units, mut buildings) = base(3);
    buildings.push(building(600, BuildingKind::LumberCamp, (145, 150)));
    units.push(soldier(300, (115, 150)));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![enemy(800, UnitKind::Villager, (148, 150))];
    let seat = step(&world, &[], &mut Blackboard::default(), &Config::default());

    assert!(seat.formation_moves().is_empty() && seat.attacks().is_empty());
}
