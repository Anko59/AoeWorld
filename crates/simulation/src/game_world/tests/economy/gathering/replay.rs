use super::*;

/// Replay pin: villagers gather every resource through several drop-off
/// building kinds while the scout walks, camps rise beside their resources
/// and a wood node is depleted into a replacement, so any index change that
/// alters authoritative state breaks the current pinned hashes.
#[test]
fn gathering_and_dropoffs_replay_to_pinned_hashes() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    // A search before the village exists warms any per-chunk resource cache;
    // the starting nodes inserted by the village must still be found after.
    let _ = world.nearest_resource(TileCoord::new(64, 64), ResourceKind::Wood, 10, None);
    (0..40i32)
        .find_map(|step| {
            let spot = TileCoord::new(64 + (step % 7) * 6 - 18, 64 + (step / 7) * 6);
            world
                .initialize_village(PlayerId(0), spot)
                .ok()
                .map(|_| spot)
        })
        .expect("a village start site");
    buildings::set_stock(&mut world, 0, 600, 0, 0);

    // One gatherer per resource, each spawned next to its node.
    let mut jobs = Vec::new();
    for kind in [
        ResourceKind::Wood,
        ResourceKind::Food,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let node = accessible_node(&generator, &world, kind);
        let id = gatherer_at(&mut world, node);
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        jobs.push((kind, node, id));
    }

    // A completed drop-off of each camp kind rises beside its resource, so
    // returns are attempted toward several building kinds. Candidate spots
    // skip node tiles, and the builder spawns on a checked neighbour tile.
    for (kind, base) in [
        (BuildingKind::LumberCamp, jobs[0].1.tile),
        (BuildingKind::MiningCamp, jobs[2].1.tile),
        (BuildingKind::Mill, jobs[1].1.tile),
    ] {
        let side = kind.rules().side;
        let ring = (-1..=side)
            .flat_map(|dy| (-1..=side).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| *dx == -1 || *dx == side || *dy == -1 || *dy == side);
        let placed = (0..25i32)
            .find_map(|attempt| {
                let spot = TileCoord::new(
                    base.x + 3 + (attempt % 5) * 2,
                    base.y + 3 + (attempt / 5) * 2,
                );
                let clear = (0..side).all(|dy| {
                    (0..side).all(|dx| {
                        world
                            .terrain()
                            .passable(TileCoord::new(spot.x + dx, spot.y + dy), world.config())
                    })
                });
                if !clear {
                    return None;
                }
                let (sx, sy) = ring.clone().find(|(dx, dy)| {
                    world
                        .terrain()
                        .passable(TileCoord::new(spot.x + dx, spot.y + dy), world.config())
                })?;
                let worker = world
                    .spawn_villager(
                        PlayerId(0),
                        WorldPosition::from_tile_center(TileCoord::new(spot.x + sx, spot.y + sy))
                            .unwrap(),
                    )
                    .ok()?;
                world.issue_build(PlayerId(0), &[worker], kind, spot).ok()
            })
            .expect("room for the camp");
        buildings::finish(&mut world, placed, 1_600);
    }

    // The scout walks for the whole run, so movement state changes too.
    let scout = world
        .units()
        .find(|unit| unit.kind == crate::UnitKind::Scout)
        .map(|unit| unit.id)
        .expect("village scout");
    (0..40i32)
        .find_map(|step| {
            Some(world.issue_move(
                scout,
                WorldPosition::from_tile_center(TileCoord::new(60 + step, 90)).ok()?,
            ))
            .and_then(|result| result.ok())
        })
        .expect("a scout destination");

    let mut pins = Vec::new();
    for block in 0..6 {
        if block == 1 {
            // Exhausting the wood node forces a replacement search mid-run.
            let node = jobs[0].1;
            world
                .deplete_resource(node.id, node.initial_amount)
                .unwrap();
        }
        for _ in 0..100 {
            world.advance();
        }
        pins.push(world.canonical_hash_hex());
    }
    assert_eq!(
        pins.as_slice(),
        GATHER_REPLAY_PINS,
        "tick {}",
        world.tick().0
    );
}

/// Current gathering/drop-off replay includes route guidance, certified
/// endpoints, resumable reverse-flood state and exact explored fog. Bounded
/// idle rotation and active follower sampling intentionally change hashed
/// state and timing. Regenerate the combined tree's six checkpoints while
/// preserving every assertion and the independent replay tests. Static
/// navigation adds a hashed shared preparation allowance and budgeted proof
/// state; these pins cover that intentional schema/pacing change as well.
/// Drop-off-ranked replacements and the retained last work tile intentionally
/// change gather jobs and their hashes; regenerate these current checkpoints.
/// The Mill's prepaid reseed count extends live and remembered building hashes.
/// Cavalry auto-scout flags and bounded frontier scheduling extend the current
/// authoritative schema, so these checkpoints are regenerated with that state.
/// The merged open TC arches and retained detour planner change routes and
/// planner hashes too; regenerate the same six checkpoints for their combination.
const GATHER_REPLAY_PINS: [&str; 6] = [
    "77d09f7643707edab058b064e87b157ec7d9d03081bd3f776b4212d0e70b703d",
    "54d462ec74971a3fb28e8839d9a6de88b4a105ce9e25337c09836c52e90aea0d",
    "18b0b4c51ff797f44feb86edf13652520bb3a3d8ad617ce14019f9ff0e29d96a",
    "cf61ac4c77d12d6981a8800b285b913f88df7a605aff2db016b9424abe4d20c8",
    "e8c2c75a9cdcc2d0d47db029d75e4782ea6f884af4e47f111d411b0480554afe",
    "4c5700a91fda2a313f3777ec19be0e2846bebbeaececcfa2092d5535cf85d7c9",
];
