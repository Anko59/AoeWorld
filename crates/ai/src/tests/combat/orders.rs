//! `ai/tests/test_combat_orders.py`, first half: rally, staging, targets,
//! refusals, marches and stalls. An engagement must survive the rest of the
//! same combat decision.
use std::collections::BTreeSet;

use aoe_sdk::{BuildingKind, CommandResult, EntityId, Formation, Order, UnitKind, UnitTask};

use super::{about, hidden, house, ids, spearmen, step};
use crate::blackboard::Blackboard;
use crate::brain::{self, Brain};
use crate::config::Config;
use crate::play::combat::CombatManager;
use crate::play::combat::march::{Purpose, march};
use crate::play::military;
use crate::tests::fake::{
    FakeWorld, base, building, enemy, foreign_building, marching, order_result, soldier, unit_of,
    with_context,
};
use crate::view::Unit;

#[test]
fn test_recruits_rally_by_plain_walk_even_while_the_field_army_is_attacking() {
    let (mut units, mut buildings) = base(3);
    let recruit = unit_of(301, UnitKind::Spearman, (68, 55));
    units.push(recruit.clone());
    buildings.push(building(600, BuildingKind::Barracks, (68, 53)));
    let world = FakeWorld::new(units, buildings);
    let mut board = Blackboard {
        attacking: true,
        attack_members: BTreeSet::from([300]),
        ..Blackboard::default()
    };
    let (done, seat) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        military::rally(ctx)
    });
    done.expect("the order budget holds");
    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, vec![recruit.id]);
    assert!(
        !groups[0].3,
        "the rally is a plain walk, not an attack-move"
    );
}

#[test]
fn test_a_scattered_roster_keeps_staging_even_above_the_attack_threshold() {
    let (mut units, buildings) = base(3);
    units.extend(spearmen(300, 7, (60, 60)));
    units.extend(spearmen(400, 7, (180, 150)));
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = vec![house(900, (90, 60))];
    let mut board = Blackboard::default();
    let seat = step(&world, &[], &mut board, &Config::default());

    assert!(!board.attacking);
    assert!(seat.attacks().is_empty());
    assert!(seat.formation_moves().is_empty());
}

#[test]
fn test_a_staged_force_attacks_the_nearest_reachable_target_without_stragglers() {
    let (mut units, buildings) = base(3);
    let staged = spearmen(300, 12, (60, 60));
    units.extend(staged.clone());
    units.extend(spearmen(400, 9, (160, 170)));
    units.extend(spearmen(500, 8, (220, 200)));
    let blocked = house(900, (71, 59));
    let reachable = house(901, (85, 59));
    let distant = house(902, (160, 185));
    let blocked_center = blocked.center();
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = vec![blocked, reachable.clone(), distant];
    world.route_fn = Some(Box::new(move |from, to| {
        (to != blocked_center).then(|| FakeWorld::straight(from, to))
    }));
    let mut board = Blackboard::default();
    let seat = step(&world, &[], &mut board, &Config::default());

    assert_eq!(board.attack_target, Some(reachable.id));
    let attacked: BTreeSet<(u32, u32)> = seat.attacks().into_iter().collect();
    let expected: BTreeSet<(u32, u32)> = staged.iter().map(|s| (s.id, reachable.id)).collect();
    assert_eq!(attacked, expected);
    assert!(seat.formation_moves().is_empty());
}

fn nearby_enemy_engagement(known_building: bool) {
    let (mut units, buildings) = base(3);
    let fighter = soldier(300, (120, 120));
    units.push(fighter.clone());
    let foe = enemy(800, UnitKind::Villager, (122, 120));
    let target = foreign_building(900, BuildingKind::House, (150, 150));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![foe.clone()];
    if known_building {
        world.enemy_buildings = vec![target];
    }
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: 1,
        ..Blackboard::default()
    };
    let seat = step(&world, &[], &mut board, &Config::default());
    assert_eq!(
        about(&seat, fighter.id),
        vec![Order::Attack {
            unit: EntityId(fighter.id),
            target: EntityId(foe.id),
        }]
    );
}

#[test]
fn test_nearby_enemy_engagement_is_not_overwritten_by_the_march_false() {
    nearby_enemy_engagement(false);
}

#[test]
fn test_nearby_enemy_engagement_is_not_overwritten_by_the_march_true() {
    nearby_enemy_engagement(true);
}

#[test]
fn test_marching_soldier_engages_visible_enemy_before_reaching_search_destination() {
    let (mut units, buildings) = base(3);
    units.push(marching(300, (120, 120)));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![enemy(800, UnitKind::Villager, (122, 120))];
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: 1,
        ..Blackboard::default()
    };
    let seat = step(&world, &[], &mut board, &Config::default());
    assert_eq!(seat.attacks(), vec![(300, 800)]);
}

#[test]
fn test_nearby_fogged_building_is_not_assumed_destroyed() {
    let (mut units, buildings) = base(3);
    units.push(soldier(300, (120, 120)));
    let target = hidden(house(900, (120, 124)));
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = vec![target.clone()];
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: 1,
        ..Blackboard::default()
    };
    let seat = step(&world, &[], &mut board, &Config::default());
    assert_eq!(board.attack_target, Some(target.id));
    assert!(!seat.formation_moves().is_empty());
    assert_eq!(board.stats.get("targets_cleared").copied().unwrap_or(0), 0);
}

fn refused_attack_retargets(result: CommandResult) {
    let (mut units, buildings) = base(3);
    let soldiers = spearmen(300, 2, (120, 120));
    units.extend(soldiers.clone());
    let near = house(900, (124, 120));
    let other = house(901, (125, 120));
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = vec![near.clone(), other.clone()];
    let mut brain = Brain::new(Config::default());
    brain.board.attacking = true;
    brain.board.attack_launch_size = 2;
    let manager = CombatManager::default();
    let (done, seat) = with_context(&world, &[], &mut brain.board, &brain.config, |ctx| {
        manager.step(ctx)
    });
    done.expect("the order budget holds");
    let sequence = seat
        .attacks()
        .iter()
        .position(|a| *a == (soldiers[0].id, near.id))
        .map(|i| i as u64 + 1)
        .expect("the first soldier attacks the nearest house");
    let events = [order_result(sequence, result)];
    let (done, seat) = with_context(&world, &events, &mut brain.board, &brain.config, |ctx| {
        brain::absorb(ctx);
        manager.step(ctx)
    });
    done.expect("the order budget holds");
    let attacks = seat.attacks();
    assert!(attacks.contains(&(soldiers[0].id, other.id)));
    assert!(attacks.contains(&(soldiers[1].id, near.id)));
}

#[test]
fn test_refused_attack_retargets_without_blacklisting_other_soldiers_unreachable() {
    refused_attack_retargets(CommandResult::RejectedUnreachable);
}

#[test]
fn test_refused_attack_retargets_without_blacklisting_other_soldiers_invalid_target() {
    refused_attack_retargets(CommandResult::RejectedInvalidTarget);
}

#[test]
fn test_refused_attack_retargets_without_blacklisting_other_soldiers_path_budget() {
    refused_attack_retargets(CommandResult::RejectedPathBudgetExceeded);
}

#[test]
fn test_marching_an_army_uses_one_formation_and_keeps_stragglers_local() {
    let mut army = spearmen(300, 14, (60, 60));
    army.push(soldier(400, (5, 60)));
    let world = FakeWorld::new(army.clone(), Vec::new());
    let mut board = Blackboard::default();
    let config = Config::default();
    let (outcome, seat) = with_context(&world, &[], &mut board, &config, |ctx| {
        march(ctx, &army, (60, 60), (200, 200), Purpose::Attack).expect("within budget");
        let (sent, busy) = (ctx.orders, ctx.busy.clone());
        // The same idle report immediately afterwards must not reset the shared route.
        march(ctx, &army, (60, 60), (200, 200), Purpose::Attack).expect("within budget");
        (sent, ctx.orders, busy)
    });
    let groups = seat.formation_moves();
    let members: Vec<Vec<u32>> = groups.iter().map(|g| g.0.clone()).collect();
    assert_eq!(members, vec![vec![400], (300..314).collect::<Vec<u32>>()]);
    assert!(groups.iter().all(|g| g.2 == Formation::Line));
    // The march is an attack-move, so soldiers answer enemies they meet.
    assert!(groups.iter().all(|g| g.3));
    assert!(seat.moves().is_empty());
    assert_eq!(outcome.2, ids(&army).into_iter().collect::<BTreeSet<u32>>());
    assert_eq!(outcome.0, outcome.1, "the second march sent nothing");
}

#[test]
fn test_distributed_army_reserves_land_queries_for_its_shared_march() {
    let (mut units, buildings) = base(10);
    let army: Vec<Unit> = (0..28)
        .map(|i| soldier(300 + i, (55 + (i % 7) as i32, 55 + (i / 7) as i32)))
        .collect();
    units.extend(army.clone());
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = vec![hidden(house(901, (250, 250)))];
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: 28,
        ..Blackboard::default()
    };
    let config = Config::default();
    let (outcome, seat) = with_context(&world, &[], &mut board, &config, |ctx| {
        CombatManager::default()
            .attack(ctx, &army)
            .expect("within budget");
        ctx.land_queries()
    });
    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, (300..328).collect::<Vec<u32>>());
    assert!(outcome <= config.land_queries_per_decision);
}

fn stationary_army_recovers(known_building: bool) {
    let (units, buildings) = base(3);
    let mut army = spearmen(300, 2, (60, 60));
    let target = hidden(house(901, (250, 250)));
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: army.len() as u32,
        ..Blackboard::default()
    };
    let config = Config::default();
    let (pause, stall) = (
        config.march_pause_seconds * 20,
        config.march_stall_seconds * 20,
    );
    let step_at = |tick: u64, army: &[Unit], board: &mut Blackboard| -> Vec<Order> {
        let mut all = units.clone();
        all.extend_from_slice(army);
        let mut world = FakeWorld::new(all, buildings.clone());
        world.tick = tick;
        if known_building {
            world.enemy_buildings = vec![target.clone()];
        }
        step(&world, &[], board, &config).orders
    };

    assert!(
        !step_at(100, &army, &mut board).is_empty(),
        "start an ordinary shared march"
    );
    for soldier in &mut army {
        soldier.task = UnitTask::Moving;
        soldier.idle = false;
        soldier.moving = true;
    }
    assert!(step_at(100 + pause + 1, &army, &mut board).is_empty());
    // Seed 3's accepted marches reported MOVING at identical tiles for thirteen
    // minutes. That task flag alone must not prevent bounded route recovery.
    let deadline = 100 + stall + 1;
    let mut recovery = step_at(deadline, &army, &mut board);
    recovery.extend(step_at(deadline + pause, &army, &mut board));
    assert!(
        !recovery.is_empty(),
        "a stationary MOVING army must receive a recovery order"
    );
    let mut recovered = BTreeSet::new();
    for order in &recovery {
        match order {
            Order::FormationMove { units, .. } => recovered.extend(units.iter().map(|u| u.0)),
            Order::Stop { unit } => {
                recovered.insert(unit.0);
            }
            _ => {}
        }
    }
    assert_eq!(recovered, ids(&army).into_iter().collect::<BTreeSet<u32>>());
    // Once the accepted stop is reflected in the snapshot, recovery must resume
    // the march rather than leave the army idle or abandon its remembered enemy.
    for soldier in &mut army {
        soldier.task = UnitTask::Idle;
        soldier.idle = true;
        soldier.moving = false;
    }
    let later = deadline + pause + 10;
    let resumed = step_at(later, &army, &mut board);
    let mut marching_ids = BTreeSet::new();
    for order in &resumed {
        if let Order::FormationMove { units, .. } = order {
            marching_ids.extend(units.iter().map(|u| u.0));
        }
    }
    assert_eq!(
        marching_ids,
        ids(&army).into_iter().collect::<BTreeSet<u32>>()
    );
    if known_building {
        assert!(!board.cooling(&format!("target:{}", target.id), later));
        assert_eq!(board.attack_target, Some(target.id));
    }
}

#[test]
fn test_stationary_moving_army_recovers_after_the_stall_deadline_false() {
    stationary_army_recovers(false);
}

#[test]
fn test_stationary_moving_army_recovers_after_the_stall_deadline_true() {
    stationary_army_recovers(true);
}

#[test]
fn test_a_moving_army_keeps_its_route_while_it_is_making_progress() {
    let (units, buildings) = base(3);
    let mut army = spearmen(300, 2, (60, 60));
    let mut board = Blackboard {
        attacking: true,
        attack_launch_size: army.len() as u32,
        ..Blackboard::default()
    };
    let config = Config::default();
    for index in 0..4u64 {
        let mut all = units.clone();
        all.extend(army.clone());
        let mut world = FakeWorld::new(all, buildings.clone());
        world.tick = 100 + index * (config.march_stall_seconds * 20 / 2);
        let seat = step(&world, &[], &mut board, &config);
        if index == 0 {
            assert!(!seat.formation_moves().is_empty());
        } else {
            assert!(
                seat.orders.is_empty(),
                "progress must preserve the accepted route"
            );
        }
        for soldier in &mut army {
            soldier.x += 10;
            soldier.y += 10;
            soldier.task = UnitTask::Moving;
            soldier.idle = false;
            soldier.moving = true;
        }
    }
}
