use super::*;
use crate::blackboard::Blackboard;
use crate::tests::fake::{FakeWorld, base, building, with_context};

#[test]
fn food_reserve_limits_military_recruitment_without_stopping_villagers() {
    let (units, mut buildings) = base(20);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    let ready = with_age(eco(180, 900, 500, 21, 40), Age::Feudal, &[Tech::Loom]);
    let mut world = FakeWorld::new(units, buildings);
    for (upkeep, recruits) in [(0, true), (180, false)] {
        world.economy = Some(Economy {
            food_upkeep_per_minute: upkeep,
            ..ready.clone()
        });
        let (_, orders) = with_context(
            &world,
            &[],
            &mut Blackboard::default(),
            &Config::default(),
            |ctx| {
                crate::play::economy::produce(ctx).unwrap();
                crate::play::military::train(ctx).unwrap();
            },
        );
        let trained = trains(&orders);
        assert!(trained.contains(&(500, UnitKind::Villager)));
        assert_eq!(trained.iter().any(|(id, _)| *id == 600), recruits);
    }
}

#[test]
fn supplying_a_large_army_sends_an_extra_worker_to_a_farm() {
    use crate::tests::fake::node;
    use aoe_sdk::{ResourceKind, UnitTask};

    let (mut units, mut buildings) = base(10);
    for worker in units.iter_mut().take(6) {
        worker.task = UnitTask::Farming;
        worker.idle = false;
    }
    for worker in units.iter_mut().skip(6).take(3) {
        worker.task = UnitTask::Gathering;
        worker.idle = false;
        worker.gathering = Some(ResourceKind::Wood);
    }
    buildings.push(building(600, BuildingKind::Mill, (54, 50)));
    buildings.push(building(700, BuildingKind::Farm, (56, 50)));
    let mut world = FakeWorld::new(units, buildings);
    let ready = eco(200, 300, 100, 11, 30);
    world.economy = Some(ready.clone());
    world.nodes = vec![node(1, ResourceKind::Wood, (55, 53))];
    world.tick = 400;
    let (_, baseline) = play_fresh(&world);
    assert!(farms(&baseline).is_empty());

    world.economy = Some(Economy {
        food_upkeep_per_minute: 140,
        ..ready
    });
    let (_, supplied) = play_fresh(&world);
    assert_eq!(farms(&supplied), vec![(10, 700)]);
}

#[test]
fn a_large_idle_reserve_delays_recruitment_during_peacetime() {
    use crate::tests::fake::unit_of;
    use aoe_sdk::UnitTask;

    let (mut units, mut buildings) = base(20);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    units.extend((0..100).map(|i| unit_of(200 + i, UnitKind::Militia, (55, 55))));
    let ready = with_age(eco(180, 900, 500, 121, 140), Age::Feudal, &[Tech::Loom]);
    let ready = Economy {
        food_upkeep_per_minute: 101,
        ..ready
    };
    let mut marching = units.clone();
    for soldier in &mut marching[21..] {
        soldier.task = UnitTask::Moving;
        soldier.idle = false;
        soldier.moving = true;
    }
    let recruits = |units| {
        let mut world = FakeWorld::new(units, buildings.clone());
        world.economy = Some(ready.clone());
        let (_, orders) = with_context(
            &world,
            &[],
            &mut Blackboard::default(),
            &Config::default(),
            |ctx| crate::play::military::train(ctx).unwrap(),
        );
        trains(&orders)
    };
    assert!(recruits(marching).iter().any(|(id, _)| *id == 600));
    assert!(!recruits(units).iter().any(|(id, _)| *id == 600));
}
