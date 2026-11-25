//! `ai/tests/test_worker_recovery.py`: the first Barracks budget while Feudal
//! Age researches, and a village that lost its Town Center.
use aoe_sdk::{BuildingKind, ResourceKind, Tech};

use super::{built_kinds, eco, gathers, play_fresh, with_age};
use crate::blackboard::Plan;
use crate::brain::Brain;
use crate::config::Config;
use crate::tests::fake::{FakeWorld, base, building, node};
use crate::view::Building;

fn advancing_village(wood: u32, barracks: Option<Building>) -> FakeWorld {
    let (units, mut buildings) = base(21);
    buildings[0] = Building {
        researching: Some(Tech::FeudalAge),
        idle: false,
        ..buildings[0].clone()
    };
    buildings.push(building(901, BuildingKind::Mill, (50, 50)));
    buildings.push(building(902, BuildingKind::LumberCamp, (50, 50)));
    buildings.extend(barracks);
    let mut world = FakeWorld::new(units, buildings);
    world.nodes = vec![node(800, ResourceKind::Wood, (55, 48))];
    world.economy = Some(with_age(
        eco(100, wood, 100, 22, 50),
        aoe_sdk::Age::Dark,
        &[Tech::Loom],
    ));
    world
}

#[test]
fn test_farms_cannot_spend_the_first_barracks_budget_while_feudal_researches() {
    for wood in [100, 200, 225] {
        let (_, seat) = play_fresh(&advancing_village(wood, None));
        let kinds = built_kinds(&seat);
        assert!(!kinds.contains(&BuildingKind::Farm), "{:?}", seat.orders);
        assert_eq!(
            kinds.contains(&BuildingKind::Barracks),
            wood == 225,
            "{:?}",
            seat.orders
        );
        assert!(
            !gathers(&seat).is_empty(),
            "workers without affordable food should cut wood"
        );
    }
}

#[test]
fn test_first_barracks_budget_accumulates_before_the_age_is_clicked() {
    let mut world = advancing_village(100, None);
    world.buildings[0] = Building {
        researching: None,
        idle: true,
        ..world.buildings[0].clone()
    };
    let (_, seat) = play_fresh(&world);
    assert!(!built_kinds(&seat).contains(&BuildingKind::Farm));
    assert!(!gathers(&seat).is_empty());
}

#[test]
fn test_paid_barracks_foundation_or_plan_does_not_keep_reserving_its_wood() {
    for planned in [false, true] {
        let mut brain = Brain::new(Config::default());
        let foundation = Building {
            complete: false,
            ..building(903, BuildingKind::Barracks, (60, 50))
        };
        let world = advancing_village(110, (!planned).then_some(foundation));
        if planned {
            brain.board.plans.push(Plan {
                kind: BuildingKind::Barracks,
                tile: (60, 60),
                tick: world.tick,
            });
        }
        let seat = super::play(&world, &mut brain, &[]);
        let kinds = built_kinds(&seat);
        assert!(kinds.contains(&BuildingKind::Farm), "planned {planned}");
        assert!(
            !kinds.contains(&BuildingKind::Barracks),
            "planned {planned}"
        );
    }
}

#[test]
fn test_surplus_wood_can_fund_a_farm_after_the_first_barracks_is_ordered() {
    let (_, seat) = play_fresh(&advancing_village(285, None));
    let kinds = built_kinds(&seat);
    assert_eq!(
        kinds[..2],
        [BuildingKind::Barracks, BuildingKind::Farm],
        "{:?}",
        seat.orders
    );
}
