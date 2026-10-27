use super::*;

/// Villagers gather every resource through several drop-off building kinds
/// while the scout walks, camps rise beside their resources and a wood node is
/// depleted into a replacement. Two independent runs must agree at every
/// 100-tick checkpoint.
#[test]
fn gathering_and_dropoffs_replay_deterministically() {
    assert_eq!(gather_replay(), gather_replay());
}

fn gather_replay() -> Vec<String> {
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
                world
                    .issue_build(PlayerId(0), &[worker], kind, spot, false)
                    .ok()
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

    let mut checkpoints = Vec::new();
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
        checkpoints.push(world.canonical_hash_hex());
    }
    checkpoints
}
