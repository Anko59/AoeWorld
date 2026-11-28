//! Fixed survey of natural and start-kit gold in the starting Town Center's
//! 14-tile radius, the outer bound of the starting gold band.
use super::BoxError;
use aoe_core::TileCoord;
use aoe_map::{ResourceKind, ResourceNode};
use aoe_simulation::{BuildingKind, GameWorld, Terrain};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Serialize)]
pub(super) struct Gold {
    pub piles: usize,
    pub initial: u64,
    pub remaining: u64,
}

pub(super) fn survey(world: &GameWorld, seats: usize) -> Result<Vec<Vec<ResourceNode>>, BoxError> {
    let mut sites = vec![Vec::new(); seats];
    let Terrain::Map { generator, overlay } = world.terrain() else {
        return Ok(sites);
    };
    const RADIUS: i32 = 14;
    let mut chunks = BTreeMap::new();
    for center in world
        .buildings()
        .filter(|b| b.kind == BuildingKind::TownCenter)
    {
        let x = center.footprint.min.x + 2;
        let y = center.footprint.min.y + 2;
        let min = TileCoord::new(x - RADIUS, y - RADIUS);
        let max = TileCoord::new(x + RADIUS, y + RADIUS);
        let inside = |node: &ResourceNode| {
            node.kind == ResourceKind::Gold
                && (node.tile.x - x).pow(2) + (node.tile.y - y).pow(2) <= RADIUS.pow(2)
        };
        let near = &mut sites[usize::from(center.player.0)];
        near.extend(overlay.starting_nodes_in(min, max).filter(inside));
        let side = aoe_map::CHUNK_TILES;
        for cy in min.y.max(0) / side..=max.y / side {
            for cx in min.x.max(0) / side..=max.x / side {
                let nodes = match chunks.entry((cx, cy)) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(generator.chunk_resource_nodes((cx, cy))?)
                    }
                };
                near.extend(nodes.iter().filter(|node| inside(node)).copied());
            }
        }
        near.sort_unstable_by_key(|node| node.id);
        near.dedup_by_key(|node| node.id);
    }
    Ok(sites)
}

pub(super) fn counts(world: &GameWorld, sites: &[ResourceNode]) -> Gold {
    let mut gold = Gold {
        piles: sites.len(),
        ..Gold::default()
    };
    if let Terrain::Map { overlay, .. } = world.terrain() {
        for node in sites {
            gold.initial += u64::from(node.initial_amount);
            gold.remaining += u64::from(
                overlay
                    .changed_amount(node.id)
                    .unwrap_or(node.initial_amount),
            );
        }
    }
    gold
}
