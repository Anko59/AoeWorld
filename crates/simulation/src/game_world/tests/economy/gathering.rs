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
    // Exhaustion now ranks from the drop-off, rather than the old tree.
    // Keep the partial-load/type/locality contract and assert the new preference.
    let chosen = world.gather_target(id).unwrap();
    let Terrain::Map { generator, overlay } = world.terrain() else {
        unreachable!()
    };
    let chosen = overlay
        .resource_by_id_with_cancel(generator, chosen, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(chosen.kind, node.kind);
    assert_ne!(chosen.id, node.id);
    assert!(
        chosen.tile.x.abs_diff(node.tile.x).pow(2) + chosen.tile.y.abs_diff(node.tile.y).pow(2)
            <= 100
    );
    let camp = world
        .nearest_dropoff(
            PlayerId(0),
            world.unit(id).unwrap().position.tile_floor(),
            node.kind,
        )
        .unwrap();
    let rect = world.building(camp).unwrap().footprint;
    let distance = |tile: TileCoord| {
        let dx = tile.x.abs_diff(tile.x.clamp(rect.min.x - 1, rect.max.x));
        let dy = tile.y.abs_diff(tile.y.clamp(rect.min.y - 1, rect.max.y));
        dx * dx + dy * dy
    };
    assert!(
        distance(chosen.tile) < distance(replacement.tile),
        "prefer the camp side of this forest"
    );
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

mod replay;
