//! Work-only rates exclude transport; delivery tests below include real walks.
//! Expected constants were measured from the licensed VER 8.9 DAT, not the
//! implementation's rate helpers. Millionths include the unfinished last unit.
use super::*;
use crate::{BuildingKind, Tech};

#[path = "rates/delivery.rs"]
mod delivery;

fn workstation(kind: ResourceKind, farm: bool) -> (GameWorld, EntityId) {
    let (mut world, node) = replacement::cleared_work_area(kind);
    world
        .place_town_center(
            PlayerId(0),
            TileCoord::new(node.tile.x - 10, node.tile.y - 2),
        )
        .unwrap();
    let worker = world
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new(node.tile.x - 1, node.tile.y)).unwrap(),
        )
        .unwrap();
    if farm {
        let rect = TileRect::from_xywh(node.tile.x - 6, node.tile.y - 1, 3, 3);
        let id = world
            .insert_building(PlayerId(0), BuildingKind::Farm, rect, true)
            .unwrap();
        world.issue_farm(PlayerId(0), worker, id).unwrap();
        let index = world.lookup[&worker];
        world.stop_movement(index);
        let from = world.units[index].state.position;
        let to = position(TileCoord::new(rect.min.x + 1, rect.min.y + 1)).unwrap();
        world.units[index].state.position = to;
        let state = world.units[index].state;
        world.track_unit_step(worker, from.tile_floor(), &state);
    } else {
        world.issue_gather(PlayerId(0), worker, node.id).unwrap();
        place_at_work(&mut world, worker);
    }
    (world, worker)
}

fn work_per_minute(world: &mut GameWorld, worker: EntityId) -> u64 {
    let mut task = world.economy.gatherers.remove(&worker).unwrap();
    let remaining = world.task_remaining(&task).unwrap();
    let before = task.credits;
    let index = world.lookup[&worker];
    for _ in 0..world.config.tick_hz * 60 {
        world.tick.0 += 1;
        // Remove each earned unit without walking so this measures work rate,
        // not carrying capacity or the distance of this particular fixture.
        world.units[index].state.carried = None;
        assert!(world.gather_step(worker, &mut task, &mut |_| true));
        assert_eq!(task.phase, GatherPhase::Gathering);
    }
    let earned = u64::from(remaining - world.task_remaining(&task).unwrap()) * 1_000_000;
    let fractional = if task.farm.is_some() {
        0 // A ready farmer's work credit is not food the farm has produced.
    } else {
        (i128::from(task.credits) - i128::from(before)) / i128::from(world.config.tick_hz)
    };
    world.economy.gatherers.insert(worker, task);
    (i128::from(earned) + fractional) as u64
}

fn compare(tech: Tech, prior: &[Tech], expected: [u64; 5]) {
    for (index, (kind, farm)) in [
        (ResourceKind::Food, false),
        (ResourceKind::Wood, false),
        (ResourceKind::Gold, false),
        (ResourceKind::Stone, false),
        (ResourceKind::Food, true),
    ]
    .into_iter()
    .enumerate()
    {
        let (mut world, worker) = workstation(kind, farm);
        for t in prior {
            world.complete_research(PlayerId(0), *t, &mut Vec::new());
        }
        let before = work_per_minute(&mut world, worker);
        let pending = world.economy.gatherers[&worker].clone();
        world.complete_research(PlayerId(0), tech, &mut Vec::new());
        assert_eq!(
            world.economy.gatherers[&worker], pending,
            "{tech:?} must retain its gather cycle"
        );
        let after = work_per_minute(&mut world, worker);
        assert_eq!(
            after, expected[index],
            "{tech:?}: {kind:?}, farm={farm}, before={before}"
        );
        if !matches!(
            tech,
            Tech::DoubleBitAxe
                | Tech::BowSaw
                | Tech::GoldMining
                | Tech::GoldShaftMining
                | Tech::StoneMining
        ) {
            assert_eq!(
                before, after,
                "{tech:?} changes carry/speed/food, not work rate"
            );
        }
    }
}

const BASE: [u64; 5] = [18_600_000, 23_400_000, 22_800_000, 21_600_000, 24_000_000];

#[test]
fn dat_base_work_rates_per_minute_include_the_separate_farm_cap() {
    for ((kind, farm), expected) in [
        (ResourceKind::Food, false),
        (ResourceKind::Wood, false),
        (ResourceKind::Gold, false),
        (ResourceKind::Stone, false),
        (ResourceKind::Food, true),
    ]
    .into_iter()
    .zip(BASE)
    {
        let (mut world, worker) = workstation(kind, farm);
        assert_eq!(
            work_per_minute(&mut world, worker),
            expected,
            "{kind:?}, farm={farm}"
        );
    }
}

#[test]
fn double_bit_axe_resources_per_minute() {
    compare(
        Tech::DoubleBitAxe,
        &[],
        [18_600_000, 28_080_000, 22_800_000, 21_600_000, 24_000_000],
    );
}

#[test]
fn a_preproduced_farm_uses_the_point_fifty_three_farmer_work_rate() {
    let (mut world, worker) = workstation(ResourceKind::Food, true);
    world.tick.0 += u64::from(world.config.tick_hz) * 120;
    assert_eq!(work_per_minute(&mut world, worker), 31_000_000);
    assert_eq!(world.economy.gatherers[&worker].credits, 16_000_000);
}

#[test]
fn an_untended_farm_stores_at_most_the_dat_fifteen_food_capacity() {
    let (mut world, worker) = workstation(ResourceKind::Food, true);
    let farm = world.economy.gatherers[&worker].farm.unwrap();
    world.tick.0 += u64::from(world.config.tick_hz) * 3600;
    for _ in 0..15 {
        assert!(world.deplete_farm(farm).is_some());
    }
    assert_eq!(world.deplete_farm(farm), None);
    assert_eq!(world.farm_remaining(farm), Some(160));
}

#[test]
fn bow_saw_resources_per_minute() {
    compare(
        Tech::BowSaw,
        &[Tech::DoubleBitAxe],
        [18_600_000, 33_696_000, 22_800_000, 21_600_000, 24_000_000],
    );
}
#[test]
fn gold_mining_resources_per_minute() {
    compare(
        Tech::GoldMining,
        &[],
        [18_600_000, 23_400_000, 26_220_000, 21_600_000, 24_000_000],
    );
}
#[test]
fn gold_shaft_mining_resources_per_minute_do_not_overflow() {
    compare(
        Tech::GoldShaftMining,
        &[Tech::GoldMining],
        [18_600_000, 23_400_000, 30_153_000, 21_600_000, 24_000_000],
    );
}
#[test]
fn stone_mining_resources_per_minute() {
    compare(
        Tech::StoneMining,
        &[],
        [18_600_000, 23_400_000, 22_800_000, 24_840_000, 24_000_000],
    );
}
#[test]
fn horse_collar_resources_per_minute() {
    compare(Tech::HorseCollar, &[], BASE);
}
#[test]
fn heavy_plow_resources_per_minute() {
    compare(Tech::HeavyPlow, &[Tech::HorseCollar], BASE);
}
#[test]
fn wheelbarrow_resources_per_minute() {
    compare(Tech::Wheelbarrow, &[], BASE);
}
#[test]
fn hand_cart_resources_per_minute() {
    compare(Tech::HandCart, &[Tech::Wheelbarrow], BASE);
}

fn carcass_workstation(kind: crate::AnimalKind) -> (GameWorld, EntityId) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(90, 95))
        .unwrap();
    let worker = world
        .spawn_villager(PlayerId(0), position(TileCoord::new(99, 100)).unwrap())
        .unwrap();
    let animal = world
        .spawn_animal(kind, position(TileCoord::new(100, 100)).unwrap())
        .unwrap();
    world.strike_animal(animal, worker, 255);
    for _ in 0..world.config.tick_hz {
        world.advance();
    }
    world.begin_carcass_gather(worker, animal).unwrap();
    place_at_work(&mut world, worker);
    (world, worker)
}

#[test]
fn dat_shepherd_and_hunter_work_rates_per_minute_are_separate_from_berries() {
    for (kind, expected) in [
        (crate::AnimalKind::Sheep, 19_800_000),
        (crate::AnimalKind::Deer, 24_600_000),
    ] {
        let (mut world, worker) = carcass_workstation(kind);
        assert_eq!(work_per_minute(&mut world, worker), expected);
    }
}

#[test]
fn dat_hunter_capacity_is_thirty_five_and_carry_techs_multiply_it() {
    for (techs, expected) in [
        (vec![], 35),
        (vec![Tech::Wheelbarrow], 45),
        (vec![Tech::Wheelbarrow, Tech::HandCart], 67),
    ] {
        let (mut world, worker) = carcass_workstation(crate::AnimalKind::Deer);
        for tech in techs {
            world.complete_research(PlayerId(0), tech, &mut Vec::new());
        }
        let mut maximum = 0;
        for _ in 0..4000 {
            world.advance();
            maximum = maximum.max(
                world
                    .unit(worker)
                    .unwrap()
                    .carried
                    .map_or(0, |load| load.amount),
            );
        }
        assert_eq!(maximum, expected);
    }
}

// Work-rate fixtures exclude travel without bypassing physical contact.
fn place_at_work(world: &mut GameWorld, worker: EntityId) {
    let to = world.economy.gatherers[&worker].work_position.unwrap();
    let index = world.lookup[&worker];
    let from = world.units[index].state.position;
    world.units[index].state.position = to;
    let state = world.units[index].state;
    world.track_unit_step(worker, from.tile_floor(), &state);
    world.stop_movement(index);
}
