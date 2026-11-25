//! `on_rejected`: what the brain learns from each refusal (`ai/aoe_ai/brain.py`).
use aoe_sdk::{
    BuildingKind, CommandResult as R, EntityId, Formation, Order, Tech, TileCoord, UnitKind,
};

use super::brain_absorb::{decide, tracked};
use super::fake::{FakeWorld, base, order_result, soldier, with_context};
use crate::blackboard::{Blackboard, Plan};
use crate::brain::{self, Brain};
use crate::config::Config;

const TICK: u64 = 100;
const HZ: u64 = 20;

/// A world with villagers 1..=10, soldiers 300 and 301 and Town Center 500.
fn world() -> FakeWorld {
    let (mut units, buildings) = base(10);
    units.extend([soldier(300, (55, 55)), soldier(301, (56, 55))]);
    FakeWorld::new(units, buildings)
}

/// Send `order` under sequence 1 and refuse it with `result` in the next decision.
fn refused(brain: &mut Brain, name: &str, order: Order, result: R) {
    brain.board.tracked.insert(1, tracked(name, order, 90));
    decide(brain, &world(), &[order_result(1, result)]);
}

fn gather(unit: u32, resource: u64) -> Order {
    Order::Gather {
        unit: EntityId(unit),
        resource,
    }
}

fn build(builders: &[u32], kind: BuildingKind, at: (i32, i32)) -> Order {
    Order::Build {
        builders: builders.iter().map(|&i| EntityId(i)).collect(),
        kind,
        origin: TileCoord::new(at.0, at.1),
    }
}

fn bridge(from: (i32, i32), to: (i32, i32)) -> Order {
    Order::BuildBridge {
        builders: vec![EntityId(1)],
        from: TileCoord::new(from.0, from.1),
        to: TileCoord::new(to.0, to.1),
    }
}

fn plan(kind: BuildingKind, tile: (i32, i32)) -> Plan {
    Plan {
        kind,
        tile,
        tick: 90,
    }
}

fn planned(brain: &mut Brain, kind: BuildingKind, tile: (i32, i32)) {
    brain.board.plans.push(plan(kind, tile));
    brain.board.builders.insert(1, Default::default());
}

#[test]
fn a_hopeless_gather_marks_the_node_bad_and_other_refusals_do_not() {
    for result in [
        R::RejectedInvalidResource,
        R::RejectedWrongUnitKind,
        R::RejectedUnreachable,
        R::RejectedResourceUnavailable,
        R::RejectedUnknownEntity,
    ] {
        let mut brain = Brain::new(Config::default());
        refused(&mut brain, "gather", gather(1, 7), result);
        assert!(brain.board.bad_resources.contains(&7), "{result:?}");
    }
    let mut brain = Brain::new(Config::default());
    refused(&mut brain, "gather", gather(1, 7), R::RejectedNotOwner);
    assert!(brain.board.bad_resources.is_empty());
}

#[test]
fn a_hopeless_hunt_marks_the_animal_bad() {
    let mut brain = Brain::new(Config::default());
    let hunt = Order::Hunt {
        units: vec![EntityId(1), EntityId(2)],
        animal: EntityId(900),
    };
    refused(&mut brain, "hunt", hunt, R::RejectedUnreachable);
    assert!(brain.board.bad_resources.contains(&900));
}

#[test]
fn a_refused_site_is_remembered_and_its_plan_and_builders_released() {
    let mut brain = Brain::new(Config::default());
    planned(&mut brain, BuildingKind::House, (60, 60));
    brain.board.plans.push(plan(BuildingKind::Farm, (70, 70)));
    refused(
        &mut brain,
        "build",
        build(&[1], BuildingKind::House, (60, 60)),
        R::RejectedInvalidPlacement,
    );
    assert_eq!(brain.board.bad_sites.len(), 1);
    assert_eq!(brain.board.plans, vec![plan(BuildingKind::Farm, (70, 70))]);
    assert!(brain.board.builders.is_empty());
}

#[test]
fn an_unexplored_build_refusal_releases_the_plan_without_blacklisting_the_site() {
    let world = world();
    let mut board = Blackboard::default();
    board.plans.push(plan(BuildingKind::House, (60, 60)));
    board.builders.insert(1, Default::default());
    let order = build(&[1], BuildingKind::House, (60, 60));
    let refused = tracked("build", order, 90);
    with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        brain::on_rejected(ctx, &refused, R::RejectedUnexploredPlacement);
    });
    assert!(board.plans.is_empty());
    assert!(board.builders.is_empty());
    assert!(board.bad_sites.is_empty());
}

#[test]
fn a_refused_bridge_releases_its_plan_at_the_lower_corner() {
    let mut brain = Brain::new(Config::default());
    planned(&mut brain, BuildingKind::Bridge, (40, 30));
    brain.board.plans.push(plan(BuildingKind::House, (40, 30)));
    refused(
        &mut brain,
        "build_bridge",
        bridge((44, 35), (40, 30)),
        R::RejectedInvalidPlacement,
    );
    assert_eq!(brain.board.plans, vec![plan(BuildingKind::House, (40, 30))]);
    assert!(brain.board.builders.is_empty());
}

#[test]
fn a_world_limit_refusal_keeps_the_site_and_pauses_building() {
    let mut brain = Brain::new(Config::default());
    planned(&mut brain, BuildingKind::House, (60, 60));
    refused(
        &mut brain,
        "build",
        build(&[1], BuildingKind::House, (60, 60)),
        R::RejectedWorldLimit,
    );
    assert!(brain.board.bad_sites.is_empty());
    assert!(brain.board.plans.is_empty() && brain.board.builders.is_empty());
    let until = TICK + 30 * HZ;
    assert!(brain.board.cooling("world-limit:build", until - 1));
    assert!(!brain.board.cooling("world-limit:build", until));
}

#[test]
fn a_world_limit_refusal_of_a_bridge_releases_the_plan_and_waits() {
    let mut brain = Brain::new(Config::default());
    planned(&mut brain, BuildingKind::Bridge, (40, 30));
    refused(
        &mut brain,
        "build_bridge",
        bridge((40, 30), (44, 35)),
        R::RejectedWorldLimit,
    );
    assert!(brain.board.plans.is_empty() && brain.board.builders.is_empty());
    assert!(brain.board.cooling("world-limit:build_bridge", TICK));
    assert!(brain.board.bad_sites.is_empty());
}

#[test]
fn a_world_limit_refusal_of_any_order_waits_for_that_kind_only() {
    let mut brain = Brain::new(Config::default());
    let train = Order::Train {
        building: EntityId(500),
        kind: UnitKind::Villager,
    };
    refused(&mut brain, "train", train, R::RejectedWorldLimit);
    assert!(brain.board.cooling("world-limit:train", TICK));
    assert!(!brain.board.cooling("world-limit:build", TICK));
    assert!(brain.board.blocked_units.is_empty());
}

#[test]
fn a_refused_destination_is_remembered_and_the_soldier_may_be_sent_again_at_once() {
    for result in [R::RejectedInvalidDestination, R::RejectedUnreachable] {
        let mut brain = Brain::new(Config::default());
        brain.board.marched.insert(300, 95);
        brain.board.marched.insert(301, 95);
        let to = TileCoord::new(102, 100);
        refused(
            &mut brain,
            "move",
            Order::Move {
                unit: EntityId(300),
                to,
            },
            result,
        );
        assert!(brain.board.bad_moves.contains(&(102, 100)), "{result:?}");
        assert!(!brain.board.marched.contains_key(&300));
        assert!(brain.board.marched.contains_key(&301));
    }
    let mut brain = Brain::new(Config::default());
    let to = TileCoord::new(102, 100);
    refused(
        &mut brain,
        "move",
        Order::Move {
            unit: EntityId(300),
            to,
        },
        R::RejectedNotOwner,
    );
    assert!(brain.board.bad_moves.is_empty());
}

#[test]
fn a_refused_group_march_frees_every_member() {
    let mut brain = Brain::new(Config::default());
    brain.board.marched.extend([(300, 95), (301, 95)]);
    let march = Order::FormationMove {
        units: vec![EntityId(300), EntityId(301)],
        formation: Formation::Line,
        to: TileCoord::new(150, 120),
        queued: false,
        attack_move: false,
    };
    refused(
        &mut brain,
        "formation_move",
        march,
        R::RejectedInvalidDestination,
    );
    assert!(brain.board.bad_moves.contains(&(150, 120)));
    assert!(brain.board.marched.is_empty());
}

#[test]
fn refused_destinations_are_forgotten_in_bulk_so_the_table_stays_bounded() {
    let mut brain = Brain::new(Config::default());
    for i in 0..4096 {
        brain.board.bad_moves.insert((i, 0));
    }
    let to = TileCoord::new(5, 5);
    refused(
        &mut brain,
        "move",
        Order::Move {
            unit: EntityId(300),
            to,
        },
        R::RejectedUnreachable,
    );
    assert_eq!(
        brain.board.bad_moves.iter().copied().collect::<Vec<_>>(),
        vec![(5, 5)]
    );
}

#[test]
fn a_refused_attack_target_is_retried_only_after_the_refusal_wait() {
    for result in [
        R::RejectedInvalidTarget,
        R::RejectedPathBudgetExceeded,
        R::RejectedUnreachable,
    ] {
        let mut brain = Brain::new(Config::default());
        let attack = Order::Attack {
            unit: EntityId(300),
            target: EntityId(901),
        };
        refused(&mut brain, "attack", attack, result);
        let until = TICK + 30 * HZ;
        assert!(
            brain.board.cooling("attack:300:901", until - 1),
            "{result:?}"
        );
        assert!(!brain.board.cooling("attack:300:901", until), "{result:?}");
        assert!(!brain.board.cooling("attack:300:902", TICK));
    }
    let mut brain = Brain::new(Config::default());
    let attack = Order::Attack {
        unit: EntityId(300),
        target: EntityId(901),
    };
    refused(&mut brain, "attack", attack, R::RejectedNotOwner);
    assert!(!brain.board.cooling("attack:300:901", TICK));
}

#[test]
fn a_refused_farm_frees_its_farmer_and_waits() {
    let mut brain = Brain::new(Config::default());
    brain.board.farmers.insert(700, 1);
    brain.board.farmers.insert(701, 2);
    let farm = Order::Farm {
        unit: EntityId(1),
        farm: EntityId(700),
    };
    refused(&mut brain, "farm", farm, R::RejectedFarmUnavailable);
    assert_eq!(
        brain.board.farmers.keys().copied().collect::<Vec<_>>(),
        vec![701]
    );
    assert!(brain.board.cooling("farm:700", TICK + 30 * HZ - 1));
    assert!(!brain.board.cooling("farm:700", TICK + 30 * HZ));
}

#[test]
fn a_refused_farm_keeps_a_farmer_another_villager_works() {
    let mut brain = Brain::new(Config::default());
    brain.board.farmers.insert(700, 2);
    let farm = Order::Farm {
        unit: EntityId(1),
        farm: EntityId(700),
    };
    refused(&mut brain, "farm", farm, R::RejectedFarmUnavailable);
    assert_eq!(brain.board.farmers.get(&700).copied(), Some(2));
    assert!(brain.board.cooling("farm:700", TICK));
}

#[test]
fn a_spearman_refused_before_feudal_age_is_blocked_other_refusals_do_not_block() {
    let train = Order::Train {
        building: EntityId(600),
        kind: UnitKind::Spearman,
    };
    let mut brain = Brain::new(Config::default());
    refused(
        &mut brain,
        "train",
        train.clone(),
        R::RejectedRequiresFeudalAge,
    );
    assert_eq!(brain.board.blocked_units.len(), 1);
    let mut brain = Brain::new(Config::default());
    refused(&mut brain, "train", train, R::RejectedInsufficientFood);
    assert!(brain.board.blocked_units.is_empty());
}

#[test]
fn a_refused_research_is_not_asked_again_for_eight_seconds() {
    let mut brain = Brain::new(Config::default());
    let research = Order::Research {
        building: EntityId(500),
        tech: Tech::Loom,
    };
    refused(&mut brain, "research", research, R::RejectedResearchBusy);
    assert!(brain.board.cooling("research:Loom", TICK + 8 * HZ - 1));
    assert!(!brain.board.cooling("research:Loom", TICK + 8 * HZ));
    assert!(!brain.board.cooling("research:FeudalAge", TICK));
}

#[test]
fn a_refused_general_training_switch_is_forgotten_so_it_is_pressed_again() {
    let mut brain = Brain::new(Config::default());
    brain.board.general_training.insert(7, true);
    brain.board.general_training.insert(8, true);
    let order = Order::GeneralTrain {
        unit: EntityId(7),
        enabled: true,
    };
    // Units 7 and 8 exist in `world()`, so the decision's pruning keeps their entries.
    refused(&mut brain, "general_train", order, R::RejectedWrongUnitKind);
    assert_eq!(
        brain
            .board
            .general_training
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![8]
    );
}
