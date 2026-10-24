mod spacing;
use super::*;

#[test]
fn each_resource_uses_its_exact_rate_and_the_ten_unit_carry_cap() {
    for (kind, hundredths) in [
        (ResourceKind::Wood, 39),
        (ResourceKind::Food, 31),
        (ResourceKind::Gold, 38),
        (ResourceKind::Stone, 36),
    ] {
        let (mut world, node, id) = resource_world(kind);
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        for tick in 1..=200_u32 {
            world.advance();
            let expected = (tick * u32::from(hundredths) / (world.config().tick_hz * 100)) as u16;
            assert_eq!(
                world
                    .unit(id)
                    .unwrap()
                    .carried
                    .map_or(0, |load| load.amount),
                expected,
                "{kind:?}, tick {tick}"
            );
        }
        let unit = world.unit(id).unwrap();
        assert_eq!(
            unit.carried,
            Some(CarriedResource {
                kind,
                amount: hundredths / 10
            })
        );
        assert_eq!(
            world.resource_remaining(node.id).unwrap(),
            node.initial_amount - hundredths / 10
        );
        advance(&mut world, 20 * 40);
        assert_eq!(world.unit(id).unwrap().carried.unwrap().amount, 10);
        assert_eq!(
            world.resource_remaining(node.id).unwrap(),
            node.initial_amount - 10
        );
    }
}

#[test]
fn a_full_load_returns_deposits_and_resumes_the_same_node() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    let before = world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    let mut returning = false;
    let mut deposited = false;
    for _ in 0..6000 {
        world.advance();
        let unit = world.unit(id).unwrap();
        assert!(unit.carried.is_none_or(|load| load.amount <= 10));
        returning |= unit.task == UnitTask::Returning;
        if world.player_economy(PlayerId(0)).unwrap().stockpile.wood == before + 10 {
            deposited = true;
            assert_eq!(world.gather_target(id), Some(node.id));
            break;
        }
    }
    assert!(returning && deposited);
    advance(&mut world, 2000);
    assert!(world.resource_remaining(node.id).unwrap() < node.initial_amount - 10);
}

#[test]
fn workers_stand_against_the_bush_or_tree_they_gather() {
    for kind in [ResourceKind::Food, ResourceKind::Wood] {
        let (mut world, node, id) = resource_world(kind);
        world.initialize_village(PlayerId(0), node.tile).unwrap();
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        let centre = WorldPosition::from_tile_center(node.tile).unwrap();
        let mut worked = false;
        for _ in 0..2000 {
            world.advance();
            let unit = world.unit(id).unwrap();
            if unit.task != UnitTask::Gathering {
                continue;
            }
            let (dx, dy) = (
                i64::from(unit.position.x - centre.x),
                i64::from(unit.position.y - centre.y),
            );
            // A tile-centre stand is 1.0 (side) or 1.41 (corner) tiles away;
            // AoE2 workers touch the node: at most 0.7 or 1.0 tiles.
            let tile = i64::from(FIXED_SUBUNITS_PER_TILE);
            let limit = if dx != 0 && dy != 0 {
                tile
            } else {
                7 * tile / 10
            };
            assert!(dx * dx + dy * dy <= limit * limit, "{kind:?} {dx},{dy}");
            assert_eq!(
                unit.position
                    .tile_floor()
                    .x
                    .abs_diff(node.tile.x)
                    .max(unit.position.tile_floor().y.abs_diff(node.tile.y)),
                1
            );
            worked = true;
            break;
        }
        assert!(worked, "{kind:?} gatherer never worked");
    }
}

#[test]
fn depleted_nodes_deposit_partial_load_then_find_only_nearby_same_type() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    world
        .deplete_resource(node.id, node.initial_amount - 2)
        .unwrap();
    let replacement = world
        .nearest_resource(node.tile, node.kind, 10, Some(node.id))
        .unwrap();
    assert_ne!(replacement.id, node.id);
    assert_eq!(replacement.kind, node.kind);
    assert!(
        replacement.tile.x.abs_diff(node.tile.x).pow(2)
            + replacement.tile.y.abs_diff(node.tile.y).pow(2)
            <= 100
    );
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    let before = world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    for _ in 0..4000 {
        world.advance();
        if world.player_economy(PlayerId(0)).unwrap().stockpile.wood > before {
            break;
        }
    }
    assert_eq!(world.resource_remaining(node.id).unwrap(), 0);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        before + 2
    );
    assert_eq!(world.gather_target(id), Some(replacement.id));
    assert!(
        world
            .resource_snapshot([1; 32])
            .unwrap()
            .changes
            .iter()
            .any(|change| change.id == node.id && change.remaining == 0)
    );
}

#[test]
fn no_nearby_resource_leaves_the_villager_idle_after_deposit() {
    let (mut world, node, id) = resource_world(ResourceKind::Gold);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    while let Some(other) = world.nearest_resource(node.tile, node.kind, 10, Some(node.id)) {
        world
            .deplete_resource(other.id, other.initial_amount)
            .unwrap();
    }
    world
        .deplete_resource(node.id, node.initial_amount - 1)
        .unwrap();
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    advance(&mut world, 6000);
    let unit = world.unit(id).unwrap();
    assert_eq!(unit.task, UnitTask::Idle);
    assert_eq!(unit.carried, None);
    assert_eq!(world.gather_target(id), None);
}

#[test]
fn changing_type_drops_load_and_move_cancels_gathering() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    advance(&mut world, 200);
    assert!(world.unit(id).unwrap().carried.is_some());
    let (_, gold, _) = resource_world(ResourceKind::Gold);
    world.issue_gather(PlayerId(0), id, gold.id).unwrap();
    assert!(world.unit(id).unwrap().carried.is_none());
    let current = world.unit(id).unwrap().position;
    world.issue_move(id, current).unwrap();
    assert_eq!(world.gather_target(id), None);
    let remaining = world.resource_remaining(gold.id).unwrap();
    advance(&mut world, 200);
    assert_eq!(world.resource_remaining(gold.id).unwrap(), remaining);
}

#[test]
fn gather_rejections_preserve_orders_and_carried_load() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    assert_eq!(
        world.issue_gather(PlayerId(1), id, node.id),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        world.issue_gather(PlayerId(0), EntityId(u32::MAX), node.id),
        Err(EconomyError::UnknownEntity)
    );
    let scout = world
        .spawn_unit(PlayerId(0), world.unit(id).unwrap().position)
        .unwrap();
    assert_eq!(
        world.issue_gather(PlayerId(0), scout, node.id),
        Err(EconomyError::WrongUnitKind)
    );
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    advance(&mut world, 200);
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_gather(PlayerId(0), id, u64::MAX),
        Err(EconomyError::InvalidResource)
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn a_failed_resource_sink_never_depletes_or_creates_carried_resources() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    for _ in 0..200 {
        world.advance_with_resource_sink(&mut |_| false);
    }
    assert_eq!(
        world.resource_remaining(node.id).unwrap(),
        node.initial_amount
    );
    assert_eq!(world.unit(id).unwrap().carried, None);
    advance(&mut world, 200);
    assert_eq!(world.unit(id).unwrap().carried.unwrap().amount, 3);
}

#[test]
fn all_four_loads_deposit_into_only_the_matching_player_stockpile_field() {
    fn amount(stockpile: crate::Stockpile, kind: ResourceKind) -> u32 {
        match kind {
            ResourceKind::Food => stockpile.food,
            ResourceKind::Wood => stockpile.wood,
            ResourceKind::Gold => stockpile.gold,
            ResourceKind::Stone => stockpile.stone,
        }
    }
    for kind in [
        ResourceKind::Food,
        ResourceKind::Wood,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        let (mut world, node, id) = resource_world(kind);
        world.initialize_village(PlayerId(0), node.tile).unwrap();
        let before = world.player_economy(PlayerId(0)).unwrap().stockpile;
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
        for _ in 0..4000 {
            world.advance();
            if amount(world.player_economy(PlayerId(0)).unwrap().stockpile, kind)
                != amount(before, kind)
            {
                break;
            }
        }
        let after = world.player_economy(PlayerId(0)).unwrap().stockpile;
        assert_eq!(amount(after, kind), amount(before, kind) + 10);
        for other in [
            ResourceKind::Food,
            ResourceKind::Wood,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ] {
            if other != kind {
                assert_eq!(amount(after, other), amount(before, other));
            }
        }
        assert_eq!(world.unit(id).unwrap().carried, None);
    }
}

#[test]
fn authoritative_job_kind_survives_collection_and_return_but_clears_on_stop() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    assert_eq!(world.gathering_kind(id), Some(ResourceKind::Wood));
    assert!(world.unit(id).unwrap().carried.is_none());
    for _ in 0..6000 {
        world.advance();
        assert_eq!(world.gathering_kind(id), Some(ResourceKind::Wood));
        if world.unit(id).unwrap().task == UnitTask::Returning {
            break;
        }
    }
    let unit = world.unit(id).unwrap();
    assert_eq!(unit.task, UnitTask::Returning);
    assert!(unit.carried.is_some());
    world.issue_move(id, unit.position).unwrap();
    assert_eq!(world.gathering_kind(id), None);
    // A worker stepped up against its node walks the last step to the centre.
    for _ in 0..40 {
        world.advance();
    }
    assert_eq!(world.unit(id).unwrap().task, UnitTask::Carrying);
    assert!(world.unit(id).unwrap().carried.is_some());
}

/// Finds a fixture node of `kind` that has a passable neighbour tile.
fn accessible_node(
    generator: &aoe_map::MapChunkGenerator,
    world: &GameWorld,
    kind: ResourceKind,
) -> ResourceNode {
    for y in 0..8 {
        for x in 0..8 {
            for node in generator.chunk(x, y).unwrap().resources {
                if node.kind != kind {
                    continue;
                }
                let reachable = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| {
                        (dx != 0 || dy != 0)
                            && world.terrain().passable(
                                TileCoord::new(node.tile.x + dx, node.tile.y + dy),
                                world.config(),
                            )
                    })
                });
                if reachable {
                    return node;
                }
            }
        }
    }
    panic!("fixture needs an accessible {kind:?} node");
}

/// A villager spawned on a passable tile next to `node`.
fn gatherer_at(world: &mut GameWorld, node: ResourceNode) -> EntityId {
    let tile = (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
        .find(|(dx, dy)| {
            (*dx != 0 || *dy != 0)
                && world.terrain().passable(
                    TileCoord::new(node.tile.x + dx, node.tile.y + dy),
                    world.config(),
                )
        })
        .map(|(dx, dy)| TileCoord::new(node.tile.x + dx, node.tile.y + dy))
        .expect("node has a passable neighbour");
    world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(tile).unwrap())
        .expect("villager spawn")
}

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
/// preserving every assertion and the independent replay tests.
const GATHER_REPLAY_PINS: [&str; 6] = [
    "54e9230fa998922d4541e3f00dd45163498ed8010fc7de9971e523827527fc58",
    "94b67f2ba4e2aee309524cf8c39b46d583f5aa7386c0c345d6dfa96615723f0a",
    "0806263785863d896aa886a6f9456df9688f88171f721784df8745650ff90375",
    "13a24d870131d00b9fbf3a6fd404ab670a6fb305a0bfe8e5ef6719e92fad305b",
    "954a7dbcfc7e102af0c853b1ca3f1a114ec467e5d9b0edfdad12220efe903916",
    "cd1ff52cea4185f6a258ffd7e0951367e9bb791554531997015320daaa0fb6ba",
];
