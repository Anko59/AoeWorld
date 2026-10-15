use super::*;
use aoe_map::{GroundMaterial, ResourceKind};
use std::collections::{BTreeSet, VecDeque};

fn barren() -> GameWorld {
    let mut world = GameWorld::new(WorldConfig::new(4096, 4096, Seed(0)).unwrap()).unwrap();
    world.terrain = super::far_orders::prepared_terrain(vec![0; 4], vec![28; 4]);
    world
}

fn connected(world: &GameWorld, origin: TileCoord, center: TileCoord) -> BTreeSet<TileCoord> {
    let mut seen = BTreeSet::from([origin]);
    let mut queue = VecDeque::from([origin]);
    while let Some(tile) = queue.pop_front() {
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if next.x.abs_diff(center.x) <= 18
                && next.y.abs_diff(center.y) <= 18
                && world.tile_passable(next)
                && world.terrain().crossable(tile, next, world.config())
                && seen.insert(next)
            {
                queue.push_back(next);
            }
        }
    }
    seen
}

#[test]
fn barren_start_gets_complete_deterministic_reachable_clusters() {
    let mut a = barren();
    let mut b = barren();
    let start = TileCoord::new(2048, 2048);
    a.initialize_village(PlayerId(0), start).unwrap();
    b.initialize_village(PlayerId(0), start).unwrap();
    assert_eq!(a.canonical_hash(), b.canonical_hash());
    let snapshot = a.resource_snapshot([0; 32]).unwrap();
    assert_eq!(snapshot, b.resource_snapshot([0; 32]).unwrap());
    let footprint = a.buildings().next().unwrap().footprint;
    let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
    let villager = a
        .units()
        .find(|u| u.kind == crate::UnitKind::Villager)
        .unwrap();
    let reached = connected(&a, villager.position.tile_floor(), center);
    for (kind, count) in [
        (ResourceKind::Food, 7),
        (ResourceKind::Gold, 7),
        (ResourceKind::Stone, 5),
        (ResourceKind::Wood, 12),
    ] {
        assert_eq!(
            a.starting_resources()
                .iter()
                .filter(|n| n.kind == kind)
                .count(),
            count
        );
    }
    for node in &a.starting_resources() {
        assert!(!footprint.contains(node.tile));
        assert!(!a.tile_passable(node.tile));
        assert!(
            (u64::from(center.x.abs_diff(node.tile.x)).pow(2)
                + u64::from(center.y.abs_diff(node.tile.y)).pow(2))
                <= 225
        );
        assert!(
            [(0, -1), (-1, 0), (1, 0), (0, 1)]
                .iter()
                .any(|(dx, dy)| reached
                    .contains(&TileCoord::new(node.tile.x + dx, node.tile.y + dy)))
        );
        assert!(a.units().all(|u| u.position.tile_floor() != node.tile));
    }
    let villager = villager.id;
    for node in a.starting_resources() {
        assert!(a.issue_gather(PlayerId(0), villager, node.id).is_ok());
        assert_eq!(a.resource_remaining(node.id), Some(node.initial_amount));
    }
}

#[test]
fn shoreline_start_moves_inland_and_has_every_resource_within_fifteen_tiles() {
    let terrain = super::far_orders::prepared_terrain(vec![100, 0, 100, 0], vec![28; 4]);
    let Terrain::Map { generator, .. } = &terrain else {
        unreachable!()
    };
    let beach = (0..4096)
        .map(|x| TileCoord::new(x, 2048))
        .find(|tile| {
            generator
                .tile_at(*tile)
                .is_some_and(|t| t.material == GroundMaterial::Sand && t.passable)
                && terrain.passable(*tile, WorldConfig::new(4096, 4096, Seed(0)).unwrap())
        })
        .expect("shoreline fixture has a walkable beach");
    let mut world = GameWorld::new(WorldConfig::new(4096, 4096, Seed(0)).unwrap()).unwrap();
    world.terrain = terrain;
    world.initialize_village(PlayerId(0), beach).unwrap();
    let footprint = world.buildings().next().unwrap().footprint;
    let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
    let Terrain::Map { generator, .. } = world.terrain() else {
        unreachable!()
    };
    assert_ne!(
        generator.tile_at(center).unwrap().material,
        GroundMaterial::Sand
    );
    for kind in [
        ResourceKind::Food,
        ResourceKind::Wood,
        ResourceKind::Gold,
        ResourceKind::Stone,
    ] {
        assert!(
            world
                .starting_resources()
                .iter()
                .any(|node| node.kind == kind
                    && node.tile.x.abs_diff(center.x).pow(2)
                        + node.tile.y.abs_diff(center.y).pow(2)
                        <= 225)
        );
    }
}

#[test]
fn kit_depletion_restore_and_paths_share_authoritative_state() {
    let mut world = barren();
    world
        .initialize_village(PlayerId(0), TileCoord::new(2048, 2048))
        .unwrap();
    let node = world.starting_resources()[0];
    let hash = world.canonical_hash();
    world
        .deplete_resource(node.id, node.initial_amount)
        .unwrap();
    assert!(world.tile_passable(node.tile));
    assert_ne!(world.canonical_hash(), hash);
    let saved = world.resource_snapshot([0; 32]).unwrap();
    let mut restored = barren();
    restored
        .restore_resources(&saved, [0; 32], &|| false)
        .unwrap();
    assert_eq!(restored.resource_remaining(node.id), Some(0));
    assert!(restored.tile_passable(node.tile));
    restored
        .initialize_village(PlayerId(0), TileCoord::new(2048, 2048))
        .unwrap();
    assert_eq!(restored.resource_snapshot([0; 32]).unwrap(), saved);
}
