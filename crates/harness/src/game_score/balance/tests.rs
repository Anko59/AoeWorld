use super::{Recorder, Stop, stop};
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};
use aoe_simulation::{GameWorld, TECH_RULES, UnitKind};
use std::time::Duration;

#[test]
fn balance_records_first_sighting_and_census_at_the_sighting_tick() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    let left = PlayerId(0);
    let right = PlayerId(1);
    world
        .initialize_village(left, TileCoord::new(40, 40))
        .unwrap();
    world
        .initialize_village(right, TileCoord::new(200, 200))
        .unwrap();
    let mut recorder = Recorder::new(&world, 2).unwrap();
    let initial = recorder.observe(&world, true).unwrap().unwrap();
    assert_eq!(initial.players[0].town_centers, 1);
    assert_eq!(initial.players[0].villagers, 3);
    assert_eq!(initial.players[0].army, 1);
    assert_eq!(initial.players[0].delivered, [0; 4]);
    assert!(initial.players[0].first_contact.is_none());

    world
        .spawn_kind(
            right,
            UnitKind::Militia,
            WorldPosition::from_tile_center(TileCoord::new(46, 40)).unwrap(),
        )
        .unwrap();
    world.advance();
    let sighting = recorder.observe(&world, false).unwrap().unwrap();
    let contact = sighting.players[0].first_contact.as_ref().unwrap();
    assert_eq!((contact.tick, contact.opponent), (1, right));
    assert_eq!(contact.town_centers, 1);
    assert_eq!(contact.technologies, 0);
    assert_eq!(sighting.players[0].research.len(), TECH_RULES.len());
    assert!(sighting.players[0].research.iter().all(Option::is_none));

    world.advance();
    let later = recorder.observe(&world, true).unwrap().unwrap();
    assert_eq!(later.players[0].first_contact.as_ref().unwrap().tick, 1);
}

#[test]
fn balance_stop_distinguishes_tick_target_from_wall_censoring() {
    let bound = Duration::from_secs(30);
    assert_eq!(stop(71, 72, Duration::from_secs(29), bound), None);
    assert_eq!(stop(71, 72, bound, bound), Some(Stop::WallLimit));
    assert_eq!(stop(72, 72, bound, bound), Some(Stop::TickTarget));
}

#[test]
fn balance_research_completion_keeps_exact_tick_between_samples() {
    use aoe_simulation::{BuildingKind, Tech};

    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    let player = PlayerId(0);
    world
        .initialize_village(player, TileCoord::new(40, 40))
        .unwrap();
    let center = world
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let mut recorder = Recorder::new(&world, 1).unwrap();
    let initial = recorder.observe(&world, true).unwrap().unwrap();
    assert_eq!(initial.players[0].age_ticks, [Some(0), None, None, None]);

    let loom = Tech::Loom;
    world.issue_research(player, center, loom).unwrap();
    let clock = u64::from(world.config().tick_hz);
    let completion = u64::from(loom.rules().seconds) * clock;
    for _ in 0..=completion {
        world.advance();
        if let Some(sample) = recorder
            .observe(&world, world.tick().0 > completion)
            .unwrap()
        {
            let index = TECH_RULES
                .iter()
                .position(|rules| rules.tech == loom)
                .unwrap();
            assert_eq!(sample.players[0].research[index], Some(completion));
            assert_eq!(sample.players[0].research.iter().flatten().count(), 1);
        }
    }
}
