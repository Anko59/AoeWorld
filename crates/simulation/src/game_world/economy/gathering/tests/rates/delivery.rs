use super::*;

fn farm_delivery(techs: &[Tech]) -> (u64, u16) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    let farm = world
        .insert_building(
            PlayerId(0),
            BuildingKind::Farm,
            TileRect::from_xywh(80, 64, 3, 3),
            true,
        )
        .unwrap();
    world
        .economy
        .buildings
        .get_mut(&farm)
        .unwrap()
        .food_remaining = Some(10_000);
    for tech in techs {
        world.complete_research(PlayerId(0), *tech, &mut Vec::new());
    }
    let worker = world
        .spawn_villager(PlayerId(0), position(TileCoord::new(81, 65)).unwrap())
        .unwrap();
    world.issue_farm(PlayerId(0), worker, farm).unwrap();
    let initial = world.player_economy(PlayerId(0)).unwrap().stockpile.food;
    let mut previous = initial;
    let mut trips = 0;
    for ticks in 1..=world.config.tick_hz * 120 * 100 {
        world.advance();
        let food = world.player_economy(PlayerId(0)).unwrap().stockpile.food;
        if food == previous {
            continue;
        }
        previous = food;
        trips += 1;
        // End at the same delivery phase for every carry capacity. Sampling
        // at a fixed wall-clock tick can put a larger last load in transit.
        if trips == 100 {
            let per_minute =
                u64::from(food - initial) * u64::from(world.config.tick_hz) * 60 * 1_000_000
                    / u64::from(ticks);
            return (
                per_minute,
                world.gather_capacity(&world.unit(worker).unwrap(), true),
            );
        }
    }
    panic!("one hundred full loads must reach the stockpile");
}

#[test]
fn wheelbarrow_and_hand_cart_raise_delivered_resources_per_minute_without_a_work_bonus() {
    let plain = farm_delivery(&[]);
    let wheel = farm_delivery(&[Tech::Wheelbarrow]);
    let hand = farm_delivery(&[Tech::Wheelbarrow, Tech::HandCart]);
    assert_eq!((plain.1, wheel.1, hand.1), (10, 13, 20));
    assert!(
        wheel.0 > plain.0,
        "per-minute deliveries: {plain:?} -> {wheel:?}"
    );
    assert!(
        hand.0 > wheel.0,
        "per-minute deliveries: {wheel:?} -> {hand:?}"
    );
}

#[test]
fn heavy_plow_adds_one_carrying_slot_and_improves_delivered_resources_per_minute() {
    let horse = farm_delivery(&[Tech::HorseCollar]);
    let heavy = farm_delivery(&[Tech::HorseCollar, Tech::HeavyPlow]);
    assert_eq!((horse.1, heavy.1), (10, 11));
    assert!(
        heavy.0 > horse.0,
        "per-minute deliveries: {horse:?} -> {heavy:?}"
    );
}

#[test]
fn hand_cart_stacks_the_exact_dat_walking_speed_with_wheelbarrow() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(20, 20))
        .unwrap();
    for tech in [Tech::Wheelbarrow, Tech::HandCart] {
        world.complete_research(PlayerId(0), tech, &mut Vec::new());
    }
    let start = position(TileCoord::new(100, 100)).unwrap();
    let worker = world.spawn_villager(PlayerId(0), start).unwrap();
    world
        .issue_move(worker, position(TileCoord::new(140, 100)).unwrap())
        .unwrap();
    for _ in 0..100 {
        world.advance();
    }
    // 0.8 * 1.1 * 1.1 tiles/s over five seconds, in 1/1024 tile units.
    assert_eq!(world.unit(worker).unwrap().position.x - start.x, 4956);
    assert_eq!(world.villager_capacity(PlayerId(0)), 20);
}
