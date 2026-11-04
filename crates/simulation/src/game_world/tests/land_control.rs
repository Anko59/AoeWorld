use super::*;

#[test]
fn held_land_is_a_clipped_union_with_stable_contention_and_no_fog_leak() {
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(30, 30))
        .unwrap();
    world
        .place_town_center(PlayerId(1), TileCoord::new(40, 30))
        .unwrap();
    let first = world.land_control(PlayerId(0));
    assert!(first.tiles > 16);
    assert!(first.share_ppm > 0);
    assert!(first.shapes.iter().all(|s| (s.x0..s.x1).all(|x| {
        world.visibility(PlayerId(0), TileCoord::new(x, s.y)) != TileVisibility::Unexplored
    })));
    assert!(first.shapes.iter().any(|s| s.player == PlayerId(0)));
    assert_eq!(world.land_control_rebuilds(), 1);
    assert_eq!(world.land_control(PlayerId(0)), first);
    world.set_fog_disabled(true);
    let all = world.land_control(PlayerId(0));
    assert!(all.shapes.iter().any(|s| s.player == PlayerId(1)));
    let area: u64 = all
        .shapes
        .iter()
        .filter(|s| s.player == PlayerId(0))
        .map(|s| (s.x1 - s.x0) as u64)
        .sum();
    assert_eq!(area, all.tiles);
    let reports = [1, 4].map(|workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut w = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
                w.place_town_center(PlayerId(0), TileCoord::new(30, 30))
                    .unwrap();
                w.place_town_center(PlayerId(1), TileCoord::new(40, 30))
                    .unwrap();
                assert_eq!(w.movement_worker_count(), workers);
                for i in 0..512 {
                    let at = TileCoord::new(10 + i % 16, 10 + i / 16);
                    let id = w
                        .spawn_unit(
                            PlayerId((i % 2) as u16),
                            WorldPosition::from_tile_center(at).unwrap(),
                        )
                        .unwrap();
                    w.issue_move(
                        id,
                        WorldPosition::from_tile_center(TileCoord::new(at.x + 3, at.y + 2))
                            .unwrap(),
                    )
                    .unwrap();
                }
                for _ in 0..20 {
                    w.advance();
                }
                (
                    w.canonical_hash(),
                    w.land_control(PlayerId(0)),
                    w.land_control(PlayerId(1)),
                )
            })
    });
    assert_eq!(reports[0], reports[1]);
}

#[test]
fn destroyed_claims_expire_at_the_next_second_without_scanning_units() {
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap();
    let tc = world
        .place_town_center(PlayerId(0), TileCoord::new(30, 30))
        .unwrap();
    let old = world.land_control(PlayerId(0));
    world.economy.buildings.remove(&tc);
    world.tick = aoe_core::Tick(19);
    assert_eq!(world.land_control(PlayerId(0)), old);
    world.tick = aoe_core::Tick(20);
    assert_eq!(world.land_control(PlayerId(0)).tiles, 0);
    assert_eq!(world.land_control_rebuilds(), 2);
}
