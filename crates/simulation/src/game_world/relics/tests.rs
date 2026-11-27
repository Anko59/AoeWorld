use super::*;
use aoe_core::{PlayerId, Seed, TileRect, WorldConfig};

fn fixture(side: i32, starts: &[TileCoord]) -> (GameWorld, Vec<EntityId>) {
    let mut world = GameWorld::new(WorldConfig::new(side, side, Seed(57)).unwrap()).unwrap();
    let ids = starts
        .iter()
        .enumerate()
        .map(|(seat, &tile)| {
            world
                .spawn_unit(
                    PlayerId(seat as u16),
                    WorldPosition::from_tile_center(tile).unwrap(),
                )
                .unwrap()
        })
        .collect();
    (world, ids)
}

#[test]
fn relic_sites_are_seeded_spread_and_scale_with_map_and_seats() {
    let seats = [TileCoord::new(30, 30), TileCoord::new(90, 90)];
    let (mut first, ids) = fixture(128, &seats);
    let (mut second, _) = fixture(128, &seats);
    first.place_relics(&ids, &|| false).unwrap();
    second.place_relics(&ids, &|| false).unwrap();
    let sites = first.ground_relics().collect::<Vec<_>>();
    assert_eq!(sites.len(), 5);
    assert_eq!(sites, second.ground_relics().collect::<Vec<_>>());
    for seat in seats {
        assert!(sites.iter().any(|&(_, site)| {
            let site = site.tile_floor();
            (site.x - seat.x).abs().max((site.y - seat.y).abs()) <= 40
        }));
    }
    for (i, &(_, site)) in sites.iter().enumerate() {
        assert!(sites[..i].iter().all(|&(_, other)| {
            let (site, other) = (site.tile_floor(), other.tile_floor());
            (site.x - other.x).abs().max((site.y - other.y).abs()) >= 6
        }));
    }

    let (mut large, ids) = fixture(512, &seats);
    large.place_relics(&ids, &|| false).unwrap();
    assert!(large.ground_relics().count() > sites.len());
    let (mut solo, ids) = fixture(128, &seats[..1]);
    solo.place_relics(&ids, &|| false).unwrap();
    assert!(solo.ground_relics().count() < sites.len());
}

#[test]
fn relic_sites_exclude_ground_separated_by_a_complete_barrier() {
    let (mut world, ids) = fixture(128, &[TileCoord::new(40, 64)]);
    for y in 0..128 {
        for x in 44..46 {
            world.economy.blocked_tiles.insert(TileCoord::new(x, y));
        }
    }
    world.collision_closed(TileRect::from_xywh(44, 0, 2, 128));
    world.place_relics(&ids, &|| false).unwrap();
    let sites = world.ground_relics().collect::<Vec<_>>();
    assert_eq!(sites.len(), 3);
    assert!(sites.iter().all(|&(_, site)| site.tile_floor().x < 44));
}

#[test]
fn generated_relics_keep_their_ids_and_positions_in_saves() {
    let (mut world, starts) = fixture(128, &[TileCoord::new(64, 64)]);
    world.place_relics(&starts, &|| false).unwrap();
    let expected = world.ground_relics().collect::<Vec<_>>();
    let mut restored = GameWorld::new(world.config()).unwrap();
    restored
        .restore_snapshot(&world.snapshot([0; 32]).unwrap(), [0; 32], &|| false)
        .unwrap();
    assert_eq!(restored.ground_relics().collect::<Vec<_>>(), expected);
}

#[test]
fn cancellation_leaves_no_partially_seeded_relics() {
    let (mut world, starts) = fixture(128, &[TileCoord::new(64, 64)]);
    let before = world.canonical_hash();
    let attempts = std::cell::Cell::new(0);
    let cancelled = || {
        attempts.set(attempts.get() + 1);
        attempts.get() > 3
    };
    assert_eq!(
        world.place_relics(&starts, &cancelled),
        Err(GameWorldError::Environment(EnvironmentPageError::Cancelled))
    );
    assert_eq!(world.canonical_hash(), before);
}
